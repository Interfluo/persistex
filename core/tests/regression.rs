//! Regression tests. Golden values come from the Python reference implementation in
//! the repository root, which was itself cross-validated against numpy/scipy.

use persistex_core::design::*;
use persistex_core::fft::{next_power_of_two, transform, Cx};
use persistex_core::optimize::*;
use persistex_core::signal::*;
use std::f64::consts::PI;

fn fixture() -> (Vec<usize>, Vec<f64>, Vec<f64>, usize) {
    let bins = vec![2usize, 5, 9, 14, 20, 27, 33, 41, 50];
    let amps: Vec<f64> = (0..bins.len())
        .map(|i| 1.0 / ((i + 1) as f64).powf(0.3))
        .collect();
    let ph: Vec<f64> = (0..bins.len())
        .map(|i| (0.7 * i as f64 + 1.3).rem_euclid(2.0 * PI))
        .collect();
    (bins, amps, ph, 1024)
}

#[test]
fn fft_round_trips() {
    for n in [256usize, 1024, 4096] {
        let original: Vec<Cx> = (0..n)
            .map(|i| Cx::new((i as f64 * 0.7).sin(), 0.0))
            .collect();
        let mut a = original.clone();
        transform(&mut a, false);
        transform(&mut a, true);
        let err = a
            .iter()
            .zip(&original)
            .map(|(x, y)| (x.re - y.re).abs().max((x.im - y.im).abs()))
            .fold(0.0f64, f64::max);
        assert!(err < 1e-12, "n={n} round-trip error {err:e}");
    }
}

#[test]
fn synthesis_matches_direct_summation() {
    let (bins, amps, ph, n) = fixture();
    let viafft = synthesize(&bins, &amps, &ph, n);
    for i in [0usize, 1, 7, 100, 511, 1023] {
        let direct: f64 = bins
            .iter()
            .zip(&amps)
            .zip(&ph)
            .map(|((&k, &a), &p)| a * (2.0 * PI * k as f64 * i as f64 / n as f64 + p).sin())
            .sum();
        assert!((viafft[i] - direct).abs() < 1e-12, "sample {i}");
    }
}

#[test]
fn lp_gradient_matches_finite_differences() {
    let (bins, amps, ph, n) = fixture();
    for p in [4u32, 64, 256] {
        let (_, g) = lp_cost_gradient(&bins, &amps, &ph, n, p);
        let h = 1e-7;
        for i in 0..bins.len() {
            let mut up = ph.clone();
            up[i] += h;
            let mut down = ph.clone();
            down[i] -= h;
            let fd = (lp_cost_gradient(&bins, &amps, &up, n, p).0
                - lp_cost_gradient(&bins, &amps, &down, n, p).0)
                / (2.0 * h);
            assert!(
                (g[i] - fd).abs() < 1e-6,
                "p={p} tone {i}: {} vs {}",
                g[i],
                fd
            );
        }
    }
}

/// Golden values from the Python reference.
#[test]
fn primitives_match_reference() {
    let (bins, amps, ph, n) = fixture();
    let s = synthesize(&bins, &amps, &ph, n);
    assert!((s[0] - 6.152163307348212e-1).abs() < 1e-12);
    assert!((relative_peak_factor(&s) - 2.209281609362578).abs() < 1e-10);

    let sch = schroeder_phases(&amps);
    assert!((sch[1] - 4.787261903818496).abs() < 1e-12);

    let (cost, _) = lp_cost_gradient(&bins, &amps, &ph, n, 256);
    assert!((cost - 3.8830602531835776).abs() < 1e-9, "cost {cost}");
}

/// The RNG must match Python's, or designs stop reproducing across the two cores.
#[test]
fn rng_matches_reference() {
    let mut rng = Rng::new(0);
    let expected = [6.389921564551887e-1, 3.8033586068119956, 2.5209157560606856];
    for want in expected {
        let got = rng.uniform(0.0, 2.0 * PI);
        assert!((got - want).abs() < 1e-12, "{got} vs {want}");
    }
}

use std::collections::HashSet;

fn gen(f_min: f64, f_max: f64, count: usize) -> Generator {
    Generator {
        f_min,
        f_max,
        count,
        ..Default::default()
    }
}

/// n inputs over the same band, each stepping around the bins already taken.
fn specs(n: usize, f_min: f64, f_max: f64, count: usize, f0: f64) -> Vec<InputSpec> {
    let mut taken: HashSet<usize> = HashSet::new();
    let mut out = Vec::new();
    for i in 0..n {
        let tones = generate_tones(&gen(f_min, f_max, count), f0, &taken);
        for t in &tones {
            taken.insert((t.frequency / f0).round() as usize);
        }
        out.push(InputSpec::with_tones(&format!("u{i}"), 1.0, tones));
    }
    out
}

#[test]
fn generated_tones_are_evenly_spaced_and_avoid_each_other() {
    let f0 = 1.0 / 30.0;
    for (n, f_min, f_max, count) in [
        (4usize, 0.1, 3.0, 8usize),
        (8, 0.3, 6.0, 5),
        (2, 0.2, 5.0, 20),
    ] {
        let s = specs(n, f_min, f_max, count, f0);
        let d = build_design(&s, 30.0, 2, 100.0).expect("design");

        let mut all: Vec<usize> = d.channels.iter().flat_map(|c| c.bins.clone()).collect();
        let total = all.len();
        all.sort_unstable();
        all.dedup();
        assert_eq!(all.len(), total, "inputs share bins");
        assert!(d.shared_bins().is_empty());

        // an evenly spaced harmonic set optimises far better than a nearly-even one
        for ch in &d.channels {
            let steps: Vec<usize> = ch.bins.windows(2).map(|w| w[1] - w[0]).collect();
            assert!(
                steps.windows(2).all(|w| w[0] == w[1]),
                "{} is not an arithmetic run: {:?}",
                ch.name,
                ch.bins
            );
        }
    }
}

#[test]
fn odd_only_generation_stays_odd() {
    let f0 = 1.0 / 30.0;
    let g = Generator {
        odd_only: true,
        ..gen(0.1, 3.0, 8)
    };
    let tones = generate_tones(&g, f0, &HashSet::new());
    assert_eq!(tones.len(), 8);
    for t in &tones {
        assert_eq!((t.frequency / f0).round() as usize % 2, 1);
    }
}

#[test]
fn shaped_generation_scales_amplitudes() {
    let f0 = 1.0 / 30.0;
    let g = Generator {
        shape: Shape::InvF,
        amplitude: 0.5,
        ..gen(0.1, 3.0, 6)
    };
    let tones = generate_tones(&g, f0, &HashSet::new());
    let peak = tones.iter().map(|t| t.amplitude).fold(0.0f64, f64::max);
    assert!((peak - 0.5).abs() < 1e-12, "peak amplitude {peak}");
    // 1/f means amplitude falls as frequency rises
    for w in tones.windows(2) {
        assert!(w[1].amplitude < w[0].amplitude);
    }
}

#[test]
fn tones_are_taken_as_given() {
    // an arbitrary, deliberately uneven set: nothing should be rearranged or refused
    let wanted = [0.37, 0.9, 1.13, 2.7, 4.02];
    let spec = InputSpec::with_tones(
        "ail",
        1.0,
        wanted.iter().map(|&f| Tone::new(f, 1.0)).collect(),
    );
    let d = build_design(&[spec], 30.0, 2, 100.0).unwrap();
    let got = d.channels[0].frequencies();
    assert_eq!(got.len(), wanted.len());
    for (g, w) in got.iter().zip(wanted) {
        // snapped to the nearest harmonic of f0 = 1/30 Hz, so within half a bin
        assert!((g - w).abs() <= 0.5 / 30.0 + 1e-9, "{g} vs {w}");
    }
}

#[test]
fn period_round_trips_with_frequency() {
    let t = Tone::new(0.25, 1.0);
    assert!((t.period() - 4.0).abs() < 1e-12);
    assert!((Tone::new(1.0 / t.period(), 1.0).frequency - 0.25).abs() < 1e-12);
}

#[test]
fn questionable_designs_warn_rather_than_fail() {
    // above Nyquist: allowed, but said so
    let spec = InputSpec::with_tones("fast", 1.0, vec![Tone::new(80.0, 1.0)]);
    let d = build_design(&[spec], 30.0, 2, 100.0).expect("must still build");
    assert!(
        d.warnings.iter().any(|w| w.contains("Nyquist")),
        "{:?}",
        d.warnings
    );
    assert_eq!(d.aliased_tones().len(), 1);

    // two inputs sharing a bin: legal signal, not separable
    let a = InputSpec::with_tones("a", 1.0, vec![Tone::new(1.0, 1.0)]);
    let b = InputSpec::with_tones("b", 1.0, vec![Tone::new(1.0, 1.0)]);
    let d = build_design(&[a, b], 30.0, 2, 100.0).unwrap();
    assert_eq!(d.shared_bins(), vec![30]);
    assert!(d.warnings.iter().any(|w| w.contains("more than one input")));

    // a record length that does not divide the sample rate (100 * 7.305 = 730.5)
    let s = InputSpec::with_tones("a", 1.0, vec![Tone::new(1.0, 1.0)]);
    let d = build_design(&[s], 7.305, 2, 100.0).unwrap();
    assert!(
        d.warnings.iter().any(|w| w.contains("seamlessly")),
        "{:?}",
        d.warnings
    );
}

#[test]
fn only_structural_problems_are_errors() {
    assert!(build_design(&[], 30.0, 2, 100.0).is_err());
    let empty = InputSpec::with_tones("a", 1.0, vec![]);
    let err = build_design(&[empty], 30.0, 2, 100.0).unwrap_err();
    assert!(err.to_string().contains("at least one tone"), "{err}");
    let s = InputSpec::with_tones("a", 1.0, vec![Tone::new(1.0, 1.0)]);
    assert!(build_design(std::slice::from_ref(&s), 0.0, 2, 100.0).is_err());
    assert!(build_design(&[s], 30.0, 2, 0.0).is_err());
}

#[test]
fn optimisation_lowers_rpf_and_respects_peak_limits() {
    let mut d = build_design(&specs(3, 0.1, 3.0, 10, 1.0 / 30.0), 30.0, 2, 100.0).unwrap();
    let before: Vec<f64> = (0..3).map(|i| d.channels[i].rpf()).collect();
    let no_cancel = || false;
    let reports = optimize_design(&mut d, Effort::Standard, None, &no_cancel, &mut Silent);
    assert_eq!(reports.len(), 3);

    for (i, r) in reports.iter().enumerate() {
        assert!(r.after <= before[i] + 1e-9, "{} got worse", r.name);
        assert!(r.after < 1.35, "{} rpf {} is poor", r.name, r.after);
        // reported RPF must equal what the channel reports, or the UI disagrees
        assert!((r.after - d.channels[i].rpf()).abs() < 1e-12);
    }

    // the exported signal must sit inside the peak limit it was scaled to
    let (fs, periods) = (d.fs, d.n_periods);
    for i in 0..d.channels.len() {
        let limit = d.channels[i].peak_limit;
        let peak = d.channels[i]
            .samples(fs, periods)
            .iter()
            .fold(0.0f64, |m, v| m.max(v.abs()));
        assert!(peak <= limit * 1.0001, "peak {peak} exceeds limit {limit}");
    }
}

#[test]
fn optimising_one_channel_leaves_the_others_alone() {
    let mut d = build_design(&specs(3, 0.1, 3.0, 10, 1.0 / 30.0), 30.0, 2, 100.0).unwrap();
    let untouched: Vec<Vec<f64>> = d.channels.iter().map(|c| c.phases.clone()).collect();
    let no_cancel = || false;
    optimize_design(&mut d, Effort::Fast, Some(&[1]), &no_cancel, &mut Silent);
    assert_eq!(d.channels[0].phases, untouched[0]);
    assert_eq!(d.channels[2].phases, untouched[2]);
    assert_ne!(d.channels[1].phases, untouched[1]);
}

#[test]
fn heterogeneous_inputs_are_allowed() {
    let f0 = 1.0 / 30.0;
    let mut taken: HashSet<usize> = HashSet::new();
    let mut specs = Vec::new();
    for (name, peak, g) in [
        ("ail", 1.0, gen(0.1, 3.0, 10)),
        (
            "rud",
            0.5,
            Generator {
                spacing: Spacing::Logarithmic,
                shape: Shape::InvF,
                ..gen(0.5, 8.0, 6)
            },
        ),
        ("thr", 0.35, gen(0.05, 1.0, 12)),
    ] {
        let tones = generate_tones(&g, f0, &taken);
        for t in &tones {
            taken.insert((t.frequency / f0).round() as usize);
        }
        specs.push(InputSpec::with_tones(name, peak, tones));
    }

    let d = build_design(&specs, 30.0, 2, 100.0).unwrap();
    assert_eq!(d.channels.len(), 3);
    assert!(d.shared_bins().is_empty());
    assert!((d.channels[1].peak_limit - 0.5).abs() < 1e-12);
}

#[test]
fn grids_are_powers_of_two() {
    let d = build_design(&specs(2, 0.1, 8.0, 10, 1.0 / 30.0), 30.0, 2, 100.0).unwrap();
    for ch in &d.channels {
        assert!(ch.grid_size().is_power_of_two());
        assert!(ch.measure_grid().is_power_of_two());
        assert!(ch.measure_grid() > ch.grid_size());
        assert_eq!(next_power_of_two(ch.grid_size()), ch.grid_size());
    }
}

#[test]
fn artifact_hash_covers_the_document() {
    let mut d = build_design(&specs(2, 0.1, 3.0, 8, 1.0 / 30.0), 30.0, 2, 100.0).unwrap();
    let json = persistex_core::export::artifact_json(&mut d, "2026-01-01T00:00:00Z");
    assert!(json.contains("\"format\":\"persistex.excitation\""));
    let marker = ", \"sha256\": \"";
    let at = json.find(marker).expect("hash present");
    let body = format!("{}}}", &json[..at]);
    let digest = &json[at + marker.len()..json.len() - 2];
    assert_eq!(persistex_core::sha256::hex(body.as_bytes()), digest);
}

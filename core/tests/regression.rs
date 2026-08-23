//! Regression tests. Golden values come from the Python reference implementation in
//! the repository root, which was itself cross-validated against numpy/scipy.

use persistex_core::design::*;
use persistex_core::fft::{next_power_of_two, transform, Cx};
use persistex_core::optimize::*;
use persistex_core::signal::*;
use std::f64::consts::PI;

fn fixture() -> (Vec<usize>, Vec<f64>, Vec<f64>, usize) {
    let bins = vec![2usize, 5, 9, 14, 20, 27, 33, 41, 50];
    let amps: Vec<f64> = (0..bins.len()).map(|i| 1.0 / ((i + 1) as f64).powf(0.3)).collect();
    let ph: Vec<f64> = (0..bins.len())
        .map(|i| (0.7 * i as f64 + 1.3).rem_euclid(2.0 * PI))
        .collect();
    (bins, amps, ph, 1024)
}

#[test]
fn fft_round_trips() {
    for n in [256usize, 1024, 4096] {
        let original: Vec<Cx> =
            (0..n).map(|i| Cx::new((i as f64 * 0.7).sin(), 0.0)).collect();
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
            assert!((g[i] - fd).abs() < 1e-6, "p={p} tone {i}: {} vs {}", g[i], fd);
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
    let expected = [
        6.389921564551887e-1,
        3.8033586068119956,
        2.5209157560606856,
    ];
    for want in expected {
        let got = rng.uniform(0.0, 2.0 * PI);
        assert!((got - want).abs() < 1e-12, "{got} vs {want}");
    }
}

fn specs(n: usize, f_min: f64, f_max: f64, tones: usize) -> Vec<InputSpec> {
    (0..n)
        .map(|i| InputSpec {
            name: format!("u{i}"),
            f_min,
            f_max,
            n_tones: tones,
            ..Default::default()
        })
        .collect()
}

#[test]
fn bins_are_mutually_exclusive_and_span_the_band() {
    for (n, f_min, f_max, tones) in
        [(4usize, 0.1, 3.0, 8usize), (8, 0.3, 6.0, 5), (2, 0.2, 5.0, 20)]
    {
        let d = build_design(&specs(n, f_min, f_max, tones), 30.0, 2, 100.0, BinMode::All)
            .expect("design");
        let mut all: Vec<usize> = d.channels.iter().flat_map(|c| c.bins.clone()).collect();
        let count = all.len();
        all.sort_unstable();
        all.dedup();
        assert_eq!(all.len(), count, "bins collide across inputs");

        // an evenly spaced harmonic set optimises far better than a nearly-even one
        for ch in &d.channels {
            let steps: Vec<usize> =
                ch.bins.windows(2).map(|w| w[1] - w[0]).collect();
            let uniform = steps.windows(2).all(|w| w[0] == w[1]);
            assert!(uniform, "{} did not get an arithmetic run: {:?}", ch.name, ch.bins);
        }
        let (lo, hi) = d.bin_range;
        let covered = d.channels.iter().map(|c| *c.bins.last().unwrap()).max().unwrap()
            - d.channels.iter().map(|c| c.bins[0]).min().unwrap();
        assert!(
            covered as f64 >= 0.85 * (hi - lo) as f64,
            "allocation covers only {covered} of {}",
            hi - lo
        );
    }
}

#[test]
fn odd_harmonics_stay_odd() {
    let d = build_design(&specs(2, 0.1, 3.0, 8), 30.0, 2, 100.0, BinMode::Odd).unwrap();
    for ch in &d.channels {
        assert!(ch.bins.iter().all(|k| k % 2 == 1), "{:?}", ch.bins);
    }
}

#[test]
fn optimisation_lowers_rpf_and_respects_peak_limits() {
    let mut d = build_design(&specs(3, 0.1, 3.0, 10), 30.0, 2, 100.0, BinMode::All).unwrap();
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
    let mut d = build_design(&specs(3, 0.1, 3.0, 10), 30.0, 2, 100.0, BinMode::All).unwrap();
    let untouched: Vec<Vec<f64>> = d.channels.iter().map(|c| c.phases.clone()).collect();
    let no_cancel = || false;
    optimize_design(&mut d, Effort::Fast, Some(&[1]), &no_cancel, &mut Silent);
    assert_eq!(d.channels[0].phases, untouched[0]);
    assert_eq!(d.channels[2].phases, untouched[2]);
    assert_ne!(d.channels[1].phases, untouched[1]);
}

#[test]
fn heterogeneous_inputs_are_allowed() {
    let specs = vec![
        InputSpec { name: "ail".into(), f_min: 0.1, f_max: 3.0, n_tones: 10, ..Default::default() },
        InputSpec { name: "rud".into(), f_min: 0.5, f_max: 8.0, n_tones: 6, peak_limit: 0.5,
                    spacing: Spacing::Logarithmic, shape: Shape::InvF, ..Default::default() },
        InputSpec { name: "thr".into(), f_min: 0.05, f_max: 1.0, n_tones: 12, peak_limit: 0.35,
                    ..Default::default() },
    ];
    let d = build_design(&specs, 30.0, 2, 100.0, BinMode::All).unwrap();
    assert_eq!(d.channels.len(), 3);
    let mut all: Vec<usize> = d.channels.iter().flat_map(|c| c.bins.clone()).collect();
    let count = all.len();
    all.sort_unstable();
    all.dedup();
    assert_eq!(all.len(), count);
}

#[test]
fn errors_name_the_offending_input() {
    let mut s = specs(2, 0.1, 3.0, 10);
    s[1].n_tones = 900;
    s[1].name = "elevator".into();
    let err = build_design(&s, 30.0, 2, 100.0, BinMode::All).unwrap_err();
    assert!(err.to_string().contains("elevator"), "{err}");

    let mut s = specs(1, 0.1, 3.0, 10);
    s[0].f_max = 400.0;
    let err = build_design(&s, 30.0, 2, 100.0, BinMode::All).unwrap_err();
    assert!(err.to_string().contains("Nyquist"), "{err}");
}

#[test]
fn grids_are_powers_of_two() {
    let d = build_design(&specs(2, 0.1, 8.0, 10), 30.0, 2, 100.0, BinMode::All).unwrap();
    for ch in &d.channels {
        assert!(ch.grid_size().is_power_of_two());
        assert!(ch.measure_grid().is_power_of_two());
        assert!(ch.measure_grid() > ch.grid_size());
        assert_eq!(next_power_of_two(ch.grid_size()), ch.grid_size());
    }
}

#[test]
fn artifact_hash_covers_the_document() {
    let mut d = build_design(&specs(2, 0.1, 3.0, 8), 30.0, 2, 100.0, BinMode::All).unwrap();
    let json = persistex_core::export::artifact_json(&mut d, "2026-01-01T00:00:00Z");
    assert!(json.contains("\"format\":\"persistex.excitation\""));
    let marker = ", \"sha256\": \"";
    let at = json.find(marker).expect("hash present");
    let body = format!("{}}}", &json[..at]);
    let digest = &json[at + marker.len()..json.len() - 2];
    assert_eq!(persistex_core::sha256::hex(body.as_bytes()), digest);
}

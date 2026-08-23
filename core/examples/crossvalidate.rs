//! Dumps numeric results for comparison against the Python reference implementation.
use persistex_core::design::*;
use persistex_core::optimize::*;
use persistex_core::signal::*;

fn arr(v: &[f64]) -> String {
    v.iter().map(|x| format!("{:.17e}", x)).collect::<Vec<_>>().join(" ")
}

fn main() {
    let bins: Vec<usize> = vec![2, 5, 9, 14, 20, 27, 33, 41, 50];
    let n = 1024usize;
    let amps: Vec<f64> = (0..bins.len()).map(|i| 1.0 / ((i + 1) as f64).powf(0.3)).collect();
    let ph: Vec<f64> = (0..bins.len())
        .map(|i| (0.7 * i as f64 + 1.3).rem_euclid(2.0 * std::f64::consts::PI))
        .collect();

    println!("SYNTH {}", arr(&synthesize(&bins, &amps, &ph, n)[..8]));
    println!("RPF {:.17e}", relative_peak_factor(&synthesize(&bins, &amps, &ph, n)));
    println!("SCHROEDER {}", arr(&schroeder_phases(&amps)));
    for p in [4u32, 16, 64, 256] {
        let (c, g) = lp_cost_gradient(&bins, &amps, &ph, n, p);
        println!("LP {} {:.17e} {}", p, c, arr(&g));
    }
    let mut rng = Rng::new(0);
    println!("RNG {}", arr(&(0..5).map(|_| rng.uniform(0.0, 6.283185307179586)).collect::<Vec<_>>()));

    // allocation + full optimisation across representative designs
    let cases: Vec<(&str, Vec<InputSpec>)> = vec![
        ("4x8", (0..4).map(|i| InputSpec { name: format!("u{}", i), f_min: 0.1, f_max: 3.0, n_tones: 8, ..Default::default() }).collect()),
        ("2x20", (0..2).map(|i| InputSpec { name: format!("u{}", i), f_min: 0.2, f_max: 5.0, n_tones: 20, ..Default::default() }).collect()),
        ("8x5", (0..8).map(|i| InputSpec { name: format!("u{}", i), f_min: 0.3, f_max: 6.0, n_tones: 5, ..Default::default() }).collect()),
        ("1x30", vec![InputSpec { name: "u".into(), f_min: 0.1, f_max: 2.0, n_tones: 30, ..Default::default() }]),
        ("hetero", vec![
            InputSpec { name: "ail".into(), f_min: 0.1, f_max: 3.0, n_tones: 10, ..Default::default() },
            InputSpec { name: "ele".into(), f_min: 0.1, f_max: 3.0, n_tones: 10, ..Default::default() },
            InputSpec { name: "rud".into(), f_min: 0.5, f_max: 8.0, n_tones: 8, peak_limit: 0.5, ..Default::default() },
            InputSpec { name: "thr".into(), f_min: 0.05, f_max: 1.0, n_tones: 12, peak_limit: 0.35, shape: Shape::InvF, ..Default::default() },
        ]),
    ];
    for (label, specs) in cases {
        let mut d = build_design(&specs, 30.0, 2, 100.0, BinMode::All).unwrap();
        let allocation: Vec<String> = d.channels.iter()
            .map(|c| c.bins.iter().map(|k| k.to_string()).collect::<Vec<_>>().join(","))
            .collect();
        println!("BINS {} {}", label, allocation.join(" | "));
        let no_cancel = || false;
        optimize_design(&mut d, Effort::Standard, None, &no_cancel, &mut Silent);
        let rpfs: Vec<f64> = (0..d.channels.len()).map(|i| d.channels[i].rpf()).collect();
        println!("RPF {} {}", label, arr(&rpfs));
        let peaks: Vec<f64> = (0..d.channels.len()).map(|i| {
            let limit = d.channels[i].peak_limit;
            let fs = d.fs; let np = d.n_periods;
            d.channels[i].samples(fs, np).iter().fold(0.0f64, |m, v| m.max(v.abs())) / limit
        }).collect();
        println!("PEAK {} {}", label, arr(&peaks));
    }
}

use persistex_core::design::*;
use persistex_core::optimize::*;
fn main() {
    for (label, specs) in [
        (
            "4x8",
            (0..4)
                .map(|i| InputSpec {
                    name: format!("u{i}"),
                    f_min: 0.1,
                    f_max: 3.0,
                    n_tones: 8,
                    ..Default::default()
                })
                .collect::<Vec<_>>(),
        ),
        (
            "8x5",
            (0..8)
                .map(|i| InputSpec {
                    name: format!("u{i}"),
                    f_min: 0.3,
                    f_max: 6.0,
                    n_tones: 5,
                    ..Default::default()
                })
                .collect(),
        ),
        (
            "2x20",
            (0..2)
                .map(|i| InputSpec {
                    name: format!("u{i}"),
                    f_min: 0.2,
                    f_max: 5.0,
                    n_tones: 20,
                    ..Default::default()
                })
                .collect(),
        ),
    ] {
        let mut d = build_design(&specs, 30.0, 2, 100.0, BinMode::All).unwrap();
        let t = std::time::Instant::now();
        let r = optimize_design(&mut d, Effort::Standard, None, &|| false, &mut Silent);
        let worst = r.iter().map(|x| x.after).fold(f64::NEG_INFINITY, f64::max);
        println!(
            "{:6} {:.2}s  worst rpf {:.4}",
            label,
            t.elapsed().as_secs_f64(),
            worst
        );
    }
}

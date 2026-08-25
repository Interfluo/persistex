use persistex_core::design::*;
use persistex_core::optimize::*;
use std::collections::HashSet;

/// n inputs over the same band, each stepping around the bins already taken.
fn spread(n: usize, f_min: f64, f_max: f64, count: usize, f0: f64) -> Vec<InputSpec> {
    let mut taken: HashSet<usize> = HashSet::new();
    let mut out = Vec::new();
    for i in 0..n {
        let g = Generator {
            f_min,
            f_max,
            count,
            ..Default::default()
        };
        let tones = generate_tones(&g, f0, &taken);
        for t in &tones {
            taken.insert((t.frequency / f0).round() as usize);
        }
        out.push(InputSpec::with_tones(&format!("u{i}"), 1.0, tones));
    }
    out
}

fn main() {
    let f0 = 1.0 / 30.0;
    for (label, specs) in [
        ("4x8", spread(4, 0.1, 3.0, 8, f0)),
        ("8x5", spread(8, 0.3, 6.0, 5, f0)),
        ("2x20", spread(2, 0.2, 5.0, 20, f0)),
    ] {
        let mut d = build_design(&specs, 30.0, 2, 100.0).unwrap();
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

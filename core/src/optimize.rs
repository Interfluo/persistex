//! Phase optimisation: an lp-norm surrogate polished after time/frequency swapping.

use crate::design::Design;
use crate::fft::{transform, Cx};
use crate::signal::{relative_peak_factor, schroeder_phases, synthesize};
use std::f64::consts::PI;

/// Seconds between preview frames handed to a consumer.
pub const FRAME_INTERVAL: f64 = 0.06;

/// One algorithm, three budgets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effort {
    Fast,
    Standard,
    Thorough,
}

impl Effort {
    pub const ALL: [Effort; 3] = [Effort::Fast, Effort::Standard, Effort::Thorough];

    pub fn label(self) -> &'static str {
        match self {
            Effort::Fast => "fast",
            Effort::Standard => "standard",
            Effort::Thorough => "thorough",
        }
    }

    /// (starts, swap iterations, lp anneal ceiling)
    pub fn budget(self) -> (usize, usize, u32) {
        match self {
            Effort::Fast => (1, 80, 256),
            Effort::Standard => (3, 120, 256),
            Effort::Thorough => (8, 200, 512),
        }
    }
}

/// Deterministic uniform stream, so designs reproduce exactly.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407))
    }

    pub fn uniform(&mut self, lo: f64, hi: f64) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        lo + (hi - lo) * ((self.0 >> 11) as f64 / (1u64 << 53) as f64)
    }
}

/// Objective `||x||_p / ||x||_2` and its phase gradient.
///
/// A smooth surrogate for the crest factor: as p grows it approaches the true
/// max-norm, but each fixed p is differentiable so gradient methods make progress.
/// Directly minimising the RPF does not work -- max and min are non-smooth.
pub fn lp_cost_gradient(
    bins: &[usize],
    amplitudes: &[f64],
    phases: &[f64],
    n_grid: usize,
    p: u32,
) -> (f64, Vec<f64>) {
    let signal = synthesize(bins, amplitudes, phases, n_grid);
    let peak = signal.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    if peak <= 0.0 {
        return (f64::INFINITY, vec![0.0; bins.len()]);
    }

    // objective is scale invariant, so peak needs no gradient term
    let u: Vec<f64> = signal.iter().map(|v| v / peak).collect();
    let abs_u: Vec<f64> = u.iter().map(|v| v.abs()).collect();
    let pf = p as f64;

    // |u|^p below this is lost in rounding against the peak sample, so it contributes
    // nothing to the norm or the gradient. At p=256 that excludes ~all but the peaks.
    let floor = 1e-17f64.powf(1.0 / (pf - 1.0));

    let n = n_grid as f64;
    let mut acc_p = 0.0f64;
    for &v in &abs_u {
        if v > floor {
            acc_p += v.powf(pf);
        }
    }
    let mean_p = acc_p / n;
    let norm_p = mean_p.powf(1.0 / pf);
    let norm_2 = (u.iter().map(|v| v * v).sum::<f64>() / n).sqrt();
    let cost = norm_p / norm_2;

    // dJ/du_n, assembled as a weight vector over the time grid
    let lead = mean_p.powf(1.0 / pf - 1.0) * norm_2 / n;
    let trail = norm_p / (n * norm_2);
    let denom = norm_2 * norm_2;
    let mut weights = vec![Cx::ZERO; n_grid];
    for i in 0..n_grid {
        let un = u[i];
        let au = abs_u[i];
        let term = if au > floor {
            lead * au.powf(pf - 1.0) * if un >= 0.0 { 1.0 } else { -1.0 }
        } else {
            0.0
        };
        weights[i] = Cx::new((term - trail * un) / denom, 0.0);
    }

    // sum_n w_n*cos(2*pi*k*n/N + phi_k) == Re{e^(i*phi_k) * conj(W[k])}, so the whole
    // gradient costs one forward FFT instead of an (n_tones x n_grid) matrix.
    transform(&mut weights, false);
    let gradient = bins
        .iter()
        .zip(amplitudes)
        .zip(phases)
        .map(|((&k, &a), &phi)| a / peak * Cx::polar(1.0, phi).mul(weights[k].conj()).re)
        .collect();

    (cost, gradient)
}

/// Compact L-BFGS with Armijo backtracking.
fn lbfgs<F, C, N>(
    mut cost_gradient: F,
    x0: &[f64],
    max_iter: usize,
    cancel: &C,
    notify: &mut N,
) -> Vec<f64>
where
    F: FnMut(&[f64]) -> (f64, Vec<f64>),
    C: Fn() -> bool,
    N: FnMut(&[f64]),
{
    const HISTORY: usize = 8;
    const TOL: f64 = 1e-8;

    let n = x0.len();
    let mut x = x0.to_vec();
    let (mut f, mut g) = cost_gradient(&x);
    let mut s_hist: Vec<Vec<f64>> = Vec::new();
    let mut y_hist: Vec<Vec<f64>> = Vec::new();
    let mut rho_hist: Vec<f64> = Vec::new();

    for _ in 0..max_iter {
        if g.iter().fold(0.0f64, |m, v| m.max(v.abs())) < TOL || cancel() {
            break;
        }
        notify(&x);

        // two-loop recursion
        let mut q = g.clone();
        let mut alphas = Vec::with_capacity(s_hist.len());
        for idx in (0..s_hist.len()).rev() {
            let a: f64 = rho_hist[idx]
                * s_hist[idx].iter().zip(&q).map(|(s, qi)| s * qi).sum::<f64>();
            alphas.push(a);
            for i in 0..n {
                q[i] -= a * y_hist[idx][i];
            }
        }
        if let (Some(s), Some(y)) = (s_hist.last(), y_hist.last()) {
            let sy: f64 = s.iter().zip(y).map(|(a, b)| a * b).sum();
            let yy: f64 = y.iter().map(|v| v * v).sum();
            if yy > 0.0 {
                let gamma = sy / yy;
                for v in q.iter_mut() {
                    *v *= gamma;
                }
            }
        }
        alphas.reverse();
        for idx in 0..s_hist.len() {
            let b: f64 = rho_hist[idx]
                * y_hist[idx].iter().zip(&q).map(|(y, qi)| y * qi).sum::<f64>();
            for i in 0..n {
                q[i] += (alphas[idx] - b) * s_hist[idx][i];
            }
        }

        let mut direction: Vec<f64> = q.iter().map(|v| -v).collect();
        let mut slope: f64 = g.iter().zip(&direction).map(|(a, b)| a * b).sum();
        if slope >= 0.0 {
            direction = g.iter().map(|v| -v).collect();
            slope = -g.iter().map(|v| v * v).sum::<f64>();
        }

        let mut step = 1.0f64;
        let mut accepted = None;
        for _ in 0..30 {
            let trial: Vec<f64> =
                (0..n).map(|i| x[i] + step * direction[i]).collect();
            let (f_trial, g_trial) = cost_gradient(&trial);
            if f_trial <= f + 1e-4 * step * slope {
                accepted = Some((trial, f_trial, g_trial));
                break;
            }
            step *= 0.5;
        }
        let (trial, f_trial, g_trial) = match accepted {
            Some(v) => v,
            None => break, // line search failed; converged
        };

        let s: Vec<f64> = (0..n).map(|i| step * direction[i]).collect();
        let y: Vec<f64> = (0..n).map(|i| g_trial[i] - g[i]).collect();
        let sy: f64 = s.iter().zip(&y).map(|(a, b)| a * b).sum();
        if sy > 1e-12 {
            s_hist.push(s);
            y_hist.push(y);
            rho_hist.push(1.0 / sy);
            if s_hist.len() > HISTORY {
                s_hist.remove(0);
                y_hist.remove(0);
                rho_hist.remove(0);
            }
        }
        x = trial;
        f = f_trial;
        g = g_trial;
    }
    x
}

fn wrap(phases: &[f64]) -> Vec<f64> {
    let tau = 2.0 * PI;
    phases.iter().map(|p| p.rem_euclid(tau)).collect()
}

/// Minimise the lp-norm surrogate for a rising sequence of p.
pub fn optimize_lp<C, N>(
    bins: &[usize],
    amplitudes: &[f64],
    phases: &[f64],
    n_grid: usize,
    p_max: u32,
    cancel: &C,
    notify: &mut N,
) -> Vec<f64>
where
    C: Fn() -> bool,
    N: FnMut(&[f64]),
{
    let mut phases = phases.to_vec();
    let mut p: u32 = 4;
    loop {
        let final_stage = p >= p_max;
        let max_iter = if final_stage { 90 } else { 22 };
        phases = lbfgs(
            |ph| lp_cost_gradient(bins, amplitudes, ph, n_grid, p),
            &phases,
            max_iter,
            cancel,
            notify,
        );
        notify(&phases);
        if final_stage || cancel() {
            return wrap(&phases);
        }
        p = (p * 4).min(p_max);
    }
}

/// Van der Ouderaa swapping: clip in time, restore the design amplitude spectrum.
pub fn optimize_swap<C, N>(
    bins: &[usize],
    amplitudes: &[f64],
    phases: &[f64],
    n_grid: usize,
    n_iter: usize,
    clip_factor: f64,
    cancel: &C,
    notify: &mut N,
) -> Vec<f64>
where
    C: Fn() -> bool,
    N: FnMut(&[f64]),
{
    let mut phases = phases.to_vec();
    let scale = n_grid as f64 / 2.0;
    let mut best_phases = phases.clone();
    let mut best_rpf = f64::INFINITY;

    for iteration in 0..n_iter {
        if iteration % 8 == 0 && cancel() {
            break;
        }
        let mut spectrum = vec![Cx::ZERO; n_grid];
        for ((&k, &a), &phi) in bins.iter().zip(amplitudes).zip(&phases) {
            let value = Cx::polar(scale * a, phi - PI / 2.0);
            spectrum[k] = value;
            spectrum[n_grid - k] = value.conj();
        }
        transform(&mut spectrum, true);
        let signal: Vec<f64> = spectrum.iter().map(|z| z.re).collect();

        let rpf = relative_peak_factor(&signal);
        if rpf < best_rpf {
            best_rpf = rpf;
            best_phases = phases.clone();
        }
        notify(&phases);

        let limit = clip_factor * signal.iter().fold(0.0f64, |m, v| m.max(v.abs()));
        let mut clipped: Vec<Cx> = signal
            .iter()
            .map(|v| Cx::new(v.max(-limit).min(limit), 0.0))
            .collect();
        transform(&mut clipped, false);
        phases = bins.iter().map(|&k| clipped[k].arg() + PI / 2.0).collect();
    }

    wrap(&best_phases)
}

/// Per-channel before/after report.
#[derive(Debug, Clone)]
pub struct OptimizeReport {
    pub name: String,
    pub before: f64,
    pub after: f64,
}

/// Progress and preview sink. Cancellation is passed separately so a preview
/// closure can borrow this mutably while the cancel check stays live.
pub trait Progress {
    fn progress(&mut self, _fraction: f64, _message: &str) {}
    /// Called with one period of the current waveform on the optimisation grid.
    fn preview(&mut self, _channel: usize, _signal: &[f64], _rpf: f64) {}
}

/// Nothing listening.
pub struct Silent;
impl Progress for Silent {}

/// Optimise channel phases for minimum RPF.
///
/// `channels` selects indices to optimise, or `None` for all. Untouched channels
/// keep their phases.
pub fn optimize_design<P: Progress>(
    design: &mut Design,
    effort: Effort,
    channels: Option<&[usize]>,
    cancel: &dyn Fn() -> bool,
    sink: &mut P,
) -> Vec<OptimizeReport> {
    let (n_starts, n_swap, p_max) = effort.budget();
    let targets: Vec<usize> = match channels {
        Some(list) => list.to_vec(),
        None => (0..design.channels.len()).collect(),
    };

    let mut rng = Rng::new(0);
    let mut reports = Vec::new();
    let total = (targets.len() * n_starts.max(1)).max(1);
    let mut done = 0usize;
    let start_time = std::time::Instant::now();
    let mut last_frame = -1.0f64;

    for &index in &targets {
        let grid = design.channels[index].grid_size();
        let measure_grid = design.channels[index].measure_grid();
        let bins = design.channels[index].bins.clone();
        let amplitudes = design.channels[index].amplitudes.clone();
        let name = design.channels[index].name.clone();
        let before = design.channels[index].rpf(); // measured honestly, on the fine grid

        let schroeder = schroeder_phases(&amplitudes);
        let current = design.channels[index].phases.clone();
        let mut starts: Vec<Vec<f64>> = vec![current.clone()];
        if current
            .iter()
            .zip(&schroeder)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f64, f64::max)
            > 1e-9
        {
            starts.push(schroeder);
        }
        while starts.len() < n_starts {
            starts.push((0..bins.len()).map(|_| rng.uniform(0.0, 2.0 * PI)).collect());
        }

        let mut best_phases = current;
        let mut best_rpf = before;

        for phases in starts {
            if cancel() {
                return reports;
            }

            let phases = {
                // Throttled by wall clock, not iteration count: the lp path has to
                // synthesise a signal to report one, and at optimiser speed that
                // floods the consumer. A frame budget bounds the cost whatever the
                // problem size.
                let mut notify = |ph: &[f64]| {
                    let now = start_time.elapsed().as_secs_f64();
                    if now - last_frame < FRAME_INTERVAL {
                        return;
                    }
                    last_frame = now;
                    let sig = synthesize(&bins, &amplitudes, ph, grid);
                    let r = relative_peak_factor(&sig);
                    sink.preview(index, &sig, r);
                };

                let swapped = optimize_swap(
                    &bins, &amplitudes, &phases, grid, n_swap, 0.9, &cancel, &mut notify,
                );
                optimize_lp(&bins, &amplitudes, &swapped, grid, p_max, &cancel, &mut notify)
            };

            // Score on the measurement grid, not the optimiser's coarse one -- the
            // coarse peak is an underestimate, so it both misranks candidates and
            // reports a better RPF than the exported signal actually has.
            let rpf = relative_peak_factor(&synthesize(&bins, &amplitudes, &phases, measure_grid));
            if rpf < best_rpf {
                best_rpf = rpf;
                best_phases = wrap(&phases);
            }

            done += 1;
            let message = format!("{}: rpf {:.4}", name, best_rpf);
            sink.progress((done as f64 / total as f64).min(1.0), &message);
        }

        design.channels[index].set_phases(best_phases);
        reports.push(OptimizeReport { name, before, after: best_rpf });
    }

    sink.progress(1.0, "done");
    reports
}

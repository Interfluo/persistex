//! Multisine synthesis and the relative peak factor.

use crate::fft::{transform, Cx};
use std::f64::consts::PI;

/// One period of `sum_k a_k*sin(2*pi*k*n/n_grid + phi_k)`, via inverse FFT.
pub fn synthesize(bins: &[usize], amplitudes: &[f64], phases: &[f64], n_grid: usize) -> Vec<f64> {
    let mut spectrum = vec![Cx::ZERO; n_grid];
    let scale = n_grid as f64 / 2.0;
    for ((&k, &a), &phi) in bins.iter().zip(amplitudes).zip(phases) {
        let value = Cx::polar(scale * a, phi - PI / 2.0); // sin convention
        spectrum[k] = value;
        spectrum[n_grid - k] = value.conj();
    }
    transform(&mut spectrum, true);
    spectrum.iter().map(|z| z.re).collect()
}

/// Sample the multisine directly on an arbitrary time grid (t = n/fs).
pub fn sample(
    bins: &[usize],
    amplitudes: &[f64],
    phases: &[f64],
    f0: f64,
    fs: f64,
    n_samples: usize,
) -> Vec<f64> {
    let mut signal = vec![0.0f64; n_samples];
    for ((&k, &a), &phi) in bins.iter().zip(amplitudes).zip(phases) {
        let w = 2.0 * PI * k as f64 * f0 / fs;
        for (n, value) in signal.iter_mut().enumerate() {
            *value += a * (w * n as f64 + phi).sin();
        }
    }
    signal
}

/// Sampled max and min, corrected by parabolic interpolation.
///
/// A sampled peak always understates the continuous one, which would let an exported
/// signal exceed the peak limit it was scaled to. Fitting a parabola through the
/// samples either side of each extreme recovers almost all of that, far more cheaply
/// than raising the grid size would.
pub fn refined_extremes(signal: &[f64]) -> (f64, f64) {
    let n = signal.len();
    if n < 3 {
        let hi = signal.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let lo = signal.iter().cloned().fold(f64::INFINITY, f64::min);
        return (hi, lo);
    }

    let vertex = |i: usize| -> f64 {
        let y0 = signal[(i + n - 1) % n];
        let y1 = signal[i];
        let y2 = signal[(i + 1) % n];
        let curvature = y0 - 2.0 * y1 + y2;
        if curvature == 0.0 {
            return y1;
        }
        let offset = 0.5 * (y0 - y2) / curvature;
        if offset.abs() > 1.0 {
            return y1;
        }
        y1 - 0.25 * (y0 - y2) * offset
    };

    let mut hi_i = 0usize;
    let mut lo_i = 0usize;
    for i in 1..n {
        if signal[i] > signal[hi_i] {
            hi_i = i;
        }
        if signal[i] < signal[lo_i] {
            lo_i = i;
        }
    }
    (
        signal[hi_i].max(vertex(hi_i)),
        signal[lo_i].min(vertex(lo_i)),
    )
}

/// Kahan-compensated sum, matching Python's `math.fsum` closely enough that the
/// Rust and Python cores agree to machine precision.
fn compensated_sum<I: Iterator<Item = f64>>(values: I) -> f64 {
    let mut sum = 0.0f64;
    let mut correction = 0.0f64;
    for value in values {
        let adjusted = value - correction;
        let total = sum + adjusted;
        correction = (total - sum) - adjusted;
        sum = total;
    }
    sum
}

pub fn mean_square(signal: &[f64]) -> f64 {
    compensated_sum(signal.iter().map(|v| v * v)) / signal.len() as f64
}

/// Morelli's RPF: crest factor normalised by that of a pure sine.
///
/// Values below 1.0 are legitimate -- an optimised multisine is flat-topped and
/// beats a sine's crest factor.
pub fn relative_peak_factor(signal: &[f64]) -> f64 {
    let (high, low) = refined_extremes(signal);
    let ms = mean_square(signal);
    if ms <= 0.0 {
        return f64::INFINITY;
    }
    (high - low) / (2.0 * std::f64::consts::SQRT_2 * ms.sqrt())
}

/// Schroeder (1970) phases: closed form, a good starting point for smooth spectra.
pub fn schroeder_phases(amplitudes: &[f64]) -> Vec<f64> {
    let total = compensated_sum(amplitudes.iter().map(|a| a * a));
    if total <= 0.0 {
        return vec![0.0; amplitudes.len()];
    }
    let tau = 2.0 * PI;
    let mut phases = Vec::with_capacity(amplitudes.len());
    let mut running = 0.0f64; // sum_{l<n} P_l
    let mut weighted = 0.0f64; // sum_{l<n} (n-l) P_l
    for &a in amplitudes {
        phases.push((-tau * weighted).rem_euclid(tau));
        running += a * a / total;
        weighted += running;
    }
    phases
}

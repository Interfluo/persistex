//! Input specifications, bin allocation, and the design containers.

use crate::signal::{refined_extremes, relative_peak_factor, sample, schroeder_phases, synthesize};
use std::collections::HashSet;
use std::fmt;

/// Optimisation grid points per cycle of the highest tone.
pub const OVERSAMPLE: usize = 8;
pub const MIN_GRID: usize = 256;
/// Finer grid for reporting RPF and setting the peak scale.
pub const MEASURE_OVERSAMPLE: usize = 32;
pub const MIN_MEASURE_GRID: usize = 2048;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesignError(pub String);

impl fmt::Display for DesignError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for DesignError {}

macro_rules! design_err {
    ($($arg:tt)*) => { DesignError(format!($($arg)*)) };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Flat,
    InvSqrtF,
    InvF,
    InvF15,
}

impl Shape {
    pub const ALL: [Shape; 4] = [Shape::Flat, Shape::InvSqrtF, Shape::InvF, Shape::InvF15];

    pub fn exponent(self) -> f64 {
        match self {
            Shape::Flat => 0.0,
            Shape::InvSqrtF => 0.5,
            Shape::InvF => 1.0,
            Shape::InvF15 => 1.5,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Shape::Flat => "flat",
            Shape::InvSqrtF => "1/sqrt(f)",
            Shape::InvF => "1/f",
            Shape::InvF15 => "1/f^1.5",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spacing {
    Linear,
    Logarithmic,
}

impl Spacing {
    pub const ALL: [Spacing; 2] = [Spacing::Linear, Spacing::Logarithmic];

    pub fn label(self) -> &'static str {
        match self {
            Spacing::Linear => "linear",
            Spacing::Logarithmic => "logarithmic",
        }
    }
}

/// One tone: a requested frequency and its amplitude.
///
/// The frequency is what the user asked for. What actually gets synthesised is the
/// nearest harmonic of f0 = 1/record_length, reported back as the tone's bin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tone {
    pub frequency: f64,
    pub amplitude: f64,
}

impl Tone {
    pub fn new(frequency: f64, amplitude: f64) -> Self {
        Tone {
            frequency,
            amplitude,
        }
    }

    /// Period in seconds. The GUI offers this as an alternative way in.
    pub fn period(&self) -> f64 {
        if self.frequency > 0.0 {
            1.0 / self.frequency
        } else {
            f64::INFINITY
        }
    }
}

/// Per-input excitation specification: a name, an actuator limit, and its tones.
#[derive(Debug, Clone, PartialEq)]
pub struct InputSpec {
    pub name: String,
    pub peak_limit: f64,
    pub tones: Vec<Tone>,
}

impl Default for InputSpec {
    fn default() -> Self {
        InputSpec {
            name: "u1".into(),
            peak_limit: 1.0,
            tones: Vec::new(),
        }
    }
}

impl InputSpec {
    pub fn with_tones(name: &str, peak_limit: f64, tones: Vec<Tone>) -> Self {
        InputSpec {
            name: name.into(),
            peak_limit,
            tones,
        }
    }
}

/// How tones are laid out when generating a set. Amplitudes follow `shape`,
/// normalised so the largest equals `amplitude`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Generator {
    pub f_min: f64,
    pub f_max: f64,
    pub count: usize,
    pub spacing: Spacing,
    pub shape: Shape,
    pub amplitude: f64,
    pub odd_only: bool,
}

impl Default for Generator {
    fn default() -> Self {
        Generator {
            f_min: 0.1,
            f_max: 3.0,
            count: 10,
            spacing: Spacing::Linear,
            shape: Shape::Flat,
            amplitude: 1.0,
            odd_only: false,
        }
    }
}

/// Find the widest exact arithmetic bin progression that fits the free bins.
///
/// Worth searching for: an evenly spaced harmonic set optimises far better than a
/// nearly-even one. Snapping ideal positions to the nearest free bin leaves spacings
/// like 11,11,12,11 -- and measured over 8 tones that irregularity costs ~30% of RPF
/// against a true arithmetic run. When several inputs share a band, each in turn
/// finds a run offset from the others, which is Morelli's interleave exactly.
fn arithmetic_run(
    free: &HashSet<usize>,
    k_lo: usize,
    k_hi: usize,
    count: usize,
    odd_only: bool,
) -> Option<Vec<usize>> {
    if count == 1 {
        return free.iter().min().map(|&k| vec![k]);
    }
    let step = if odd_only { 2 } else { 1 };
    if k_hi <= k_lo {
        return None;
    }
    let widest = (k_hi - k_lo) / (count - 1);
    // A run that cannot reach across the band is not worth having: covering 0.1-1 Hz
    // when 0.1-2 Hz was asked for is a worse design than an unevenly spaced one.
    let needed = 0.85 * (k_hi - k_lo) as f64;

    let mut d = widest.saturating_sub(widest % step);
    while d >= step {
        if (((count - 1) * d) as f64) < needed {
            break;
        }
        let last_start = k_hi.saturating_sub((count - 1) * d);
        for start in k_lo..=last_start {
            let run: Vec<usize> = (0..count).map(|j| start + j * d).collect();
            if run.iter().all(|k| free.contains(k)) {
                return Some(run);
            }
        }
        d -= step;
    }
    None
}

/// Build a set of tones. `avoid` lists bins already taken by other inputs, which the
/// generator steps around so the inputs stay orthogonal over the record.
pub fn generate_tones(gen: &Generator, f0: f64, avoid: &HashSet<usize>) -> Vec<Tone> {
    if gen.count == 0 || f0 <= 0.0 {
        return Vec::new();
    }
    let (lo, hi) = if gen.f_min <= gen.f_max {
        (gen.f_min, gen.f_max)
    } else {
        (gen.f_max, gen.f_min)
    };
    let k_lo = ((lo / f0).round() as usize).max(1);
    let k_hi = ((hi / f0).round() as usize).max(k_lo + 1);

    let free: HashSet<usize> = (k_lo..=k_hi)
        .filter(|k| !avoid.contains(k) && (!gen.odd_only || k % 2 == 1))
        .collect();

    let bins: Vec<usize> = if gen.spacing == Spacing::Linear {
        arithmetic_run(&free, k_lo, k_hi, gen.count, gen.odd_only)
            .unwrap_or_else(|| snap_spread(gen, k_lo, k_hi, &free))
    } else {
        snap_spread(gen, k_lo, k_hi, &free)
    };

    let exponent = gen.shape.exponent();
    let mut amplitudes: Vec<f64> = bins
        .iter()
        .map(|&k| (k as f64 * f0).powf(-exponent))
        .collect();
    let norm = amplitudes.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    if norm > 0.0 {
        for a in amplitudes.iter_mut() {
            *a = *a / norm * gen.amplitude;
        }
    }

    bins.into_iter()
        .zip(amplitudes)
        .map(|(k, a)| Tone::new(k as f64 * f0, a))
        .collect()
}

/// Even spread across the band, snapped to the nearest free bin.
fn snap_spread(gen: &Generator, k_lo: usize, k_hi: usize, free: &HashSet<usize>) -> Vec<usize> {
    let count = gen.count.min(free.len().max(1));
    let mut chosen: Vec<usize> = Vec::with_capacity(count);
    let lo = k_lo as f64;
    let hi = k_hi as f64;

    for j in 0..count {
        let f = if count == 1 {
            0.0
        } else {
            j as f64 / (count - 1) as f64
        };
        let target = match gen.spacing {
            Spacing::Logarithmic => lo * (hi / lo).powf(f),
            Spacing::Linear => lo + (hi - lo) * f,
        };
        let pick = free
            .iter()
            .filter(|k| !chosen.contains(k))
            .min_by(|a, b| {
                let da = (**a as f64 - target).abs();
                let db = (**b as f64 - target).abs();
                da.partial_cmp(&db).unwrap().then(a.cmp(b))
            })
            .copied();
        match pick {
            Some(k) => chosen.push(k),
            None => break,
        }
    }
    chosen.sort_unstable();
    chosen
}

/// One excitation input: a set of harmonic bins with amplitudes and phases.
#[derive(Debug, Clone)]
pub struct Channel {
    pub name: String,
    pub bins: Vec<usize>,
    pub amplitudes: Vec<f64>,
    pub phases: Vec<f64>,
    pub f0: f64,
    pub peak_limit: f64,
    cache: Option<Measured>,
}

#[derive(Debug, Clone)]
struct Measured {
    phases: Vec<f64>,
    waveform: Vec<f64>,
    rpf: f64,
    peak: f64,
}

impl Channel {
    pub fn new(
        name: String,
        bins: Vec<usize>,
        amplitudes: Vec<f64>,
        phases: Vec<f64>,
        f0: f64,
        peak_limit: f64,
    ) -> Self {
        Channel {
            name,
            bins,
            amplitudes,
            phases,
            f0,
            peak_limit,
            cache: None,
        }
    }

    pub fn n_tones(&self) -> usize {
        self.bins.len()
    }

    pub fn frequencies(&self) -> Vec<f64> {
        self.bins.iter().map(|&k| k as f64 * self.f0).collect()
    }

    pub fn set_phases(&mut self, phases: Vec<f64>) {
        self.phases = phases;
        self.cache = None;
    }

    /// Optimisation grid: decoupled from the output rate, sized to resolve the peak.
    pub fn grid_size(&self) -> usize {
        let k_max = *self.bins.iter().max().unwrap_or(&1);
        MIN_GRID.max(crate::fft::next_power_of_two(OVERSAMPLE * k_max))
    }

    /// Reporting grid. Must out-resolve the output sampling, or the scale factor is
    /// set from a peak the exported signal then exceeds.
    pub fn measure_grid(&self) -> usize {
        let k_max = *self.bins.iter().max().unwrap_or(&1);
        MIN_MEASURE_GRID.max(crate::fft::next_power_of_two(MEASURE_OVERSAMPLE * k_max))
    }

    /// Synthesise on the measurement grid, memoised. The grid is large by design,
    /// so recomputing it per animation frame stalls the UI.
    fn measure(&mut self) -> &Measured {
        let stale = match &self.cache {
            Some(m) => m.phases != self.phases,
            None => true,
        };
        if stale {
            let waveform = synthesize(
                &self.bins,
                &self.amplitudes,
                &self.phases,
                self.measure_grid(),
            );
            let (high, low) = refined_extremes(&waveform);
            let rpf = relative_peak_factor(&waveform);
            self.cache = Some(Measured {
                phases: self.phases.clone(),
                waveform,
                rpf,
                peak: high.abs().max(low.abs()),
            });
        }
        self.cache.as_ref().unwrap()
    }

    pub fn rpf(&mut self) -> f64 {
        if self.bins.is_empty() {
            return f64::NAN;
        }
        self.measure().rpf
    }

    pub fn waveform(&mut self) -> Vec<f64> {
        self.measure().waveform.clone()
    }

    /// Factor bringing the waveform peak to `peak_limit`.
    pub fn scale(&mut self) -> f64 {
        let limit = self.peak_limit;
        let peak = self.measure().peak;
        if peak > 0.0 {
            limit / peak
        } else {
            0.0
        }
    }

    pub fn scaled_amplitudes(&mut self) -> Vec<f64> {
        let s = self.scale();
        self.amplitudes.iter().map(|a| a * s).collect()
    }

    /// The output time series: one period sampled at fs, repeated n_periods times.
    pub fn samples(&mut self, fs: f64, n_periods: usize) -> Vec<f64> {
        let n_period = (fs / self.f0).round() as usize;
        let amps = self.scaled_amplitudes();
        let one = sample(&self.bins, &amps, &self.phases, self.f0, fs, n_period);
        let mut out = Vec::with_capacity(n_period * n_periods);
        for _ in 0..n_periods {
            out.extend_from_slice(&one);
        }
        out
    }
}

/// A complete multi-input excitation design.
#[derive(Debug, Clone)]
pub struct Design {
    pub channels: Vec<Channel>,
    pub specs: Vec<InputSpec>,
    pub f0: f64,
    pub fs: f64,
    pub n_periods: usize,
    /// Things worth telling the user that are not worth refusing to build over.
    pub warnings: Vec<String>,
}

impl Design {
    pub fn record_length(&self) -> f64 {
        1.0 / self.f0
    }

    pub fn duration(&self) -> f64 {
        self.n_periods as f64 / self.f0
    }

    pub fn n_samples(&self) -> usize {
        (self.fs / self.f0).round() as usize * self.n_periods
    }

    pub fn used_bins(&self) -> usize {
        self.channels.iter().map(|c| c.n_tones()).sum()
    }

    /// Repeats join seamlessly only when this is a whole number.
    pub fn samples_per_period(&self) -> f64 {
        self.fs / self.f0
    }

    pub fn nyquist(&self) -> f64 {
        self.fs / 2.0
    }

    /// Tones at or above Nyquist, which will alias when sampled.
    pub fn aliased_tones(&self) -> Vec<(String, f64)> {
        let nyquist = self.nyquist();
        let mut out = Vec::new();
        for ch in &self.channels {
            for f in ch.frequencies() {
                if f >= nyquist {
                    out.push((ch.name.clone(), f));
                }
            }
        }
        out
    }

    /// Bins used by more than one input. Sharing a bin means those inputs are not
    /// separable from a single manoeuvre; it is still a legal signal.
    pub fn shared_bins(&self) -> Vec<usize> {
        let mut seen: HashSet<usize> = HashSet::new();
        let mut shared: Vec<usize> = Vec::new();
        for ch in &self.channels {
            for &k in &ch.bins {
                if !seen.insert(k) && !shared.contains(&k) {
                    shared.push(k);
                }
            }
        }
        shared.sort_unstable();
        shared
    }
}

/// Assemble a design from explicit per-input tone lists.
///
/// Deliberately permissive: requested frequencies are snapped to the nearest
/// harmonic of f0 and anything questionable is reported through `warnings` rather
/// than refused. The only hard errors are structural -- no inputs, no tones, or a
/// record length or sample rate that cannot describe a signal at all.
pub fn build_design(
    specs: &[InputSpec],
    record_length: f64,
    n_periods: usize,
    fs: f64,
) -> Result<Design, DesignError> {
    if specs.is_empty() {
        return Err(design_err!("add at least one input"));
    }
    if record_length <= 0.0 || !record_length.is_finite() {
        return Err(design_err!("record length must be a positive number"));
    }
    if fs <= 0.0 || !fs.is_finite() {
        return Err(design_err!("sample rate must be a positive number"));
    }
    if n_periods < 1 {
        return Err(design_err!("repeats must be at least one"));
    }

    let f0 = 1.0 / record_length;
    let mut warnings: Vec<String> = Vec::new();
    let mut channels = Vec::with_capacity(specs.len());

    for spec in specs {
        if spec.tones.is_empty() {
            return Err(design_err!("{}: add at least one tone", spec.name));
        }

        let mut bins: Vec<usize> = Vec::with_capacity(spec.tones.len());
        let mut amplitudes: Vec<f64> = Vec::with_capacity(spec.tones.len());
        let mut seen: HashSet<usize> = HashSet::new();

        for tone in &spec.tones {
            if tone.frequency <= 0.0 || !tone.frequency.is_finite() {
                warnings.push(format!(
                    "{}: skipped a tone with a non-positive frequency",
                    spec.name
                ));
                continue;
            }
            let bin = ((tone.frequency / f0).round() as usize).max(1);
            if !seen.insert(bin) {
                // two requested frequencies landed on the same harmonic
                warnings.push(format!(
                    "{}: {:.4} Hz collides with another tone at bin {} ({:.4} Hz); \
                     lengthen the record to separate them",
                    spec.name,
                    tone.frequency,
                    bin,
                    bin as f64 * f0
                ));
                continue;
            }
            bins.push(bin);
            amplitudes.push(tone.amplitude);
        }

        if bins.is_empty() {
            return Err(design_err!("{}: no usable tones", spec.name));
        }

        // keep bins ascending, carrying amplitudes with them
        let mut order: Vec<usize> = (0..bins.len()).collect();
        order.sort_by_key(|&i| bins[i]);
        let bins: Vec<usize> = order.iter().map(|&i| bins[i]).collect();
        let amplitudes: Vec<f64> = order.iter().map(|&i| amplitudes[i]).collect();

        let phases = schroeder_phases(&amplitudes);
        channels.push(Channel::new(
            spec.name.clone(),
            bins,
            amplitudes,
            phases,
            f0,
            spec.peak_limit,
        ));
    }

    let design = Design {
        channels,
        specs: specs.to_vec(),
        f0,
        fs,
        n_periods,
        warnings: Vec::new(),
    };

    for (name, f) in design.aliased_tones() {
        warnings.push(format!(
            "{}: {:.4} Hz is at or above Nyquist ({:.4} Hz) and will alias",
            name,
            f,
            design.nyquist()
        ));
    }
    let shared = design.shared_bins();
    if !shared.is_empty() {
        warnings.push(format!(
            "{} bin(s) are used by more than one input, so those inputs cannot be \
             separated from a single manoeuvre",
            shared.len()
        ));
    }
    let per_period = design.samples_per_period();
    if (per_period - per_period.round()).abs() > 1e-9 {
        warnings.push(format!(
            "sample rate / f0 = {:.3} is not a whole number, so repeats will not join \
             seamlessly",
            per_period
        ));
    }

    Ok(Design { warnings, ..design })
}

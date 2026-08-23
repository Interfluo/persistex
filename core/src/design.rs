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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinMode {
    All,
    Odd,
}

impl BinMode {
    pub const ALL: [BinMode; 2] = [BinMode::All, BinMode::Odd];

    pub fn label(self) -> &'static str {
        match self {
            BinMode::All => "all harmonics",
            BinMode::Odd => "odd harmonics",
        }
    }

    pub fn is_odd(self) -> bool {
        matches!(self, BinMode::Odd)
    }
}

/// Per-input excitation specification. Each input carries its own band, tone count,
/// shaping and spacing, so channels need not resemble one another.
#[derive(Debug, Clone, PartialEq)]
pub struct InputSpec {
    pub name: String,
    pub f_min: f64,
    pub f_max: f64,
    pub n_tones: usize,
    pub peak_limit: f64,
    pub shape: Shape,
    pub spacing: Spacing,
}

impl Default for InputSpec {
    fn default() -> Self {
        InputSpec {
            name: "u1".into(),
            f_min: 0.1,
            f_max: 3.0,
            n_tones: 10,
            peak_limit: 1.0,
            shape: Shape::Flat,
            spacing: Spacing::Linear,
        }
    }
}

impl InputSpec {
    pub fn validate(&self) -> Result<(), DesignError> {
        if self.name.trim().is_empty() {
            return Err(design_err!("every input needs a name"));
        }
        if !(self.f_min > 0.0 && self.f_min < self.f_max) {
            return Err(design_err!("{}: require 0 < f min < f max", self.name));
        }
        if self.n_tones < 1 {
            return Err(design_err!("{}: needs at least one tone", self.name));
        }
        if self.peak_limit <= 0.0 {
            return Err(design_err!("{}: peak limit must be positive", self.name));
        }
        Ok(())
    }
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
        Channel { name, bins, amplitudes, phases, f0, peak_limit, cache: None }
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
            let waveform = synthesize(&self.bins, &self.amplitudes, &self.phases, self.measure_grid());
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
    pub bin_mode: BinMode,
    pub bin_range: (usize, usize),
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

    /// Bins available inside the union of the requested bands.
    pub fn available_bins(&self) -> usize {
        let (lo, hi) = self.bin_range;
        (lo..=hi).filter(|k| !self.bin_mode.is_odd() || k % 2 == 1).count()
    }

    pub fn used_bins(&self) -> usize {
        self.channels.iter().map(|c| c.n_tones()).sum()
    }

    /// Repeats join seamlessly only when this is a whole number.
    pub fn samples_per_period(&self) -> f64 {
        self.fs / self.f0
    }
}

/// Ideal bin positions for one input, offset by index/n_inputs.
///
/// The offset is what produces Morelli's interleave: when several inputs share a
/// band, their target sets fall between one another rather than on top of one
/// another, so the greedy assignment has little left to resolve.
fn targets(
    k_lo: usize,
    k_hi: usize,
    count: usize,
    spacing: Spacing,
    index: usize,
    n_inputs: usize,
) -> Vec<f64> {
    if count == 1 {
        return vec![k_lo as f64];
    }
    let n = n_inputs as f64;
    let span = (count - 1) as f64 + (n - 1.0) / n;
    let lo = k_lo as f64;
    let hi = k_hi as f64;
    (0..count)
        .map(|j| {
            let f = (j as f64 + index as f64 / n) / span;
            match spacing {
                Spacing::Logarithmic => lo * (hi / lo).powf(f),
                Spacing::Linear => lo + (hi - lo) * f,
            }
        })
        .collect()
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
    let widest = (k_hi - k_lo) / (count - 1);
    // A run that cannot reach across the band is not worth having: covering 0.1-1 Hz
    // when 0.1-2 Hz was asked for is a worse design than an unevenly spaced one.
    let needed = 0.85 * (k_hi - k_lo) as f64;

    let mut d = widest - widest % step;
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

fn allocate_one(
    order: &[usize],
    specs: &[InputSpec],
    ranges: &[(usize, usize)],
    pools: &[Vec<usize>],
    odd_only: bool,
) -> Result<(Vec<Vec<usize>>, usize), DesignError> {
    let mut used: HashSet<usize> = HashSet::new();
    let mut allocated: Vec<Vec<usize>> = vec![Vec::new(); specs.len()];
    let mut exact = 0usize;

    for &i in order {
        let spec = &specs[i];
        let (k_lo, k_hi) = ranges[i];
        let free: HashSet<usize> =
            pools[i].iter().copied().filter(|k| !used.contains(k)).collect();

        if free.len() >= spec.n_tones && spec.spacing != Spacing::Logarithmic {
            if let Some(run) = arithmetic_run(&free, k_lo, k_hi, spec.n_tones, odd_only) {
                used.extend(run.iter().copied());
                allocated[i] = run;
                exact += 1;
                continue;
            }
        }
        if free.len() < spec.n_tones {
            return Err(design_err!(
                "{}: wants {} tones but only {} free bins remain in {:.4}-{:.4} Hz{} -- \
                 lengthen the record, widen this input's band, or use fewer tones",
                spec.name,
                spec.n_tones,
                free.len(),
                spec.f_min,
                spec.f_max,
                if odd_only { " (odd harmonics only)" } else { "" }
            ));
        }

        let mut chosen: Vec<usize> = Vec::with_capacity(spec.n_tones);
        for target in targets(k_lo, k_hi, spec.n_tones, spec.spacing, i, specs.len()) {
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
        used.extend(chosen.iter().copied());
        allocated[i] = chosen;
    }

    Ok((allocated, exact))
}

/// Assign each input a set of bins, mutually exclusive across inputs. Exclusivity is
/// what keeps the inputs orthogonal over the record, so it holds even when the
/// requested bands overlap.
fn allocate_bins(
    specs: &[InputSpec],
    f0: f64,
    odd_only: bool,
) -> Result<(Vec<Vec<usize>>, Vec<(usize, usize)>), DesignError> {
    let mut ranges = Vec::with_capacity(specs.len());
    let mut pools = Vec::with_capacity(specs.len());
    for spec in specs {
        let k_lo = ((spec.f_min / f0).round() as usize).max(1);
        let k_hi = (spec.f_max / f0).round() as usize;
        if k_hi <= k_lo {
            return Err(design_err!(
                "{}: {:.4}-{:.4} Hz collapses into one bin at {:.4} Hz resolution -- \
                 lengthen the record or widen the band",
                spec.name,
                spec.f_min,
                spec.f_max,
                f0
            ));
        }
        ranges.push((k_lo, k_hi));
        pools.push(
            (k_lo..=k_hi).filter(|k| !odd_only || k % 2 == 1).collect::<Vec<usize>>(),
        );
    }

    let n = specs.len();
    // Order changes how many inputs get an exact arithmetic run: whoever is allocated
    // first fragments the pool for everyone after. Most-constrained-first avoids
    // starving a narrow-band input, but can spend the low bins on it and leave the
    // wideband inputs with nothing evenly spaced. Try a few orders and keep the best.
    let mut orders: Vec<Vec<usize>> = Vec::new();
    let mut by_slack: Vec<usize> = (0..n).collect();
    by_slack.sort_by_key(|&i| pools[i].len() as i64 - specs[i].n_tones as i64);
    orders.push(by_slack);
    let mut by_span: Vec<usize> = (0..n).collect();
    by_span.sort_by_key(|&i| -((ranges[i].1 - ranges[i].0) as i64));
    orders.push(by_span);
    let mut by_tones: Vec<usize> = (0..n).collect();
    by_tones.sort_by_key(|&i| -(specs[i].n_tones as i64));
    orders.push(by_tones);

    let mut best: Option<(Vec<Vec<usize>>, usize)> = None;
    let mut failure: Option<DesignError> = None;
    for order in &orders {
        match allocate_one(order, specs, &ranges, &pools, odd_only) {
            Ok((allocated, exact)) => {
                let better = best.as_ref().map(|(_, e)| exact > *e).unwrap_or(true);
                if better {
                    best = Some((allocated, exact));
                }
                if exact == n {
                    break;
                }
            }
            Err(e) => {
                if failure.is_none() {
                    failure = Some(e);
                }
            }
        }
    }

    match best {
        Some((allocated, _)) => Ok((allocated, ranges)),
        None => Err(failure.unwrap_or_else(|| design_err!("could not allocate bins"))),
    }
}

/// Assemble a MIMO design from a list of `InputSpec`.
pub fn build_design(
    specs: &[InputSpec],
    record_length: f64,
    n_periods: usize,
    fs: f64,
    bin_mode: BinMode,
) -> Result<Design, DesignError> {
    if specs.is_empty() {
        return Err(design_err!("add at least one input"));
    }
    if record_length <= 0.0 || fs <= 0.0 {
        return Err(design_err!("record length and sample rate must be positive"));
    }
    if n_periods < 1 {
        return Err(design_err!("repeats must be at least one"));
    }
    for spec in specs {
        spec.validate()?;
        if spec.f_max >= fs / 2.0 {
            return Err(design_err!(
                "{}: f max must stay below Nyquist ({:.4} Hz)",
                spec.name,
                fs / 2.0
            ));
        }
    }

    let f0 = 1.0 / record_length;
    let odd_only = bin_mode.is_odd();
    let (allocated, ranges) = allocate_bins(specs, f0, odd_only)?;

    let mut channels = Vec::with_capacity(specs.len());
    for (spec, bins) in specs.iter().zip(allocated) {
        let exponent = spec.shape.exponent();
        let mut amplitudes: Vec<f64> =
            bins.iter().map(|&k| (k as f64 * f0).powf(-exponent)).collect();
        let norm = amplitudes.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        if norm > 0.0 {
            for a in amplitudes.iter_mut() {
                *a /= norm;
            }
        }
        // Schroeder start: already decent before any optimisation.
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

    let k_lo = ranges.iter().map(|r| r.0).min().unwrap();
    let k_hi = ranges.iter().map(|r| r.1).max().unwrap();
    Ok(Design {
        channels,
        specs: specs.to_vec(),
        f0,
        fs,
        n_periods,
        bin_mode,
        bin_range: (k_lo, k_hi),
    })
}

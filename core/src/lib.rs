//! Multisine excitation design for system identification.
//!
//! Orthogonal phase-optimised multisines after Morelli: each input is assigned a
//! mutually exclusive set of harmonic bins, so the inputs stay orthogonal in both
//! time and frequency over the record and a single manoeuvre separates every
//! input's contribution to every output. Phases are then optimised per input to
//! minimise the relative peak factor, which buys more injected energy for a given
//! peak actuator deflection.
//!
//! This crate has no dependencies and performs no I/O beyond the explicit export
//! helpers, so it can be lifted into an embedded or autocoded context later.

pub mod design;
pub mod export;
pub mod fft;
pub mod optimize;
pub mod sha256;
pub mod signal;

pub use design::{build_design, BinMode, Channel, Design, DesignError, InputSpec, Shape, Spacing};
pub use optimize::{optimize_design, Effort, OptimizeReport, Progress};
pub use signal::{relative_peak_factor, sample, schroeder_phases, synthesize};

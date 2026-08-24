# persistex — design notes

Excitation signal design and system identification for generic UAS
(multirotor / fixed-wing / hybrid transition vehicles).

## Production port (2026-08-23)

The tool is a Rust application: a **zero-dependency `core`** (own FFT, L-BFGS,
RNG, SHA-256) plus an egui GUI. The core carries no dependencies on purpose, so
it can be lifted into an embedded or autocoded context later without unpicking a
dependency tree -- which is what the online estimation half will need.

It was ported from a Python/tkinter implementation, which was itself validated
against numpy/scipy. **That prototype has since been removed** (2026-08-23) now
that the Rust tool is the product. Nothing was lost in terms of verification:
before the port the two were cross-checked with bin allocations byte-identical
across every test design, and synthesis, RPF, Schroeder phases, the lp
cost/gradient and the RNG stream agreeing to machine precision (1e-16 to 1e-15).
Optimised RPF agreed to ~1e-8, with one case at 1.3e-4 from float accumulation
order inside the line search. Rust was 20-27x faster for identical answers.
**Those golden values are frozen in `core/tests/regression.rs`**, so the
regression suite still pins the behaviour the Python implementation established.

Shipping: each platform builds on its own runner (cross-compiling a GUI app is
not worth it). Installers are .dmg (universal), NSIS .exe (per-user, no admin),
and .deb + AppImage. **Signing is the one thing that cannot be scripted around**
-- it needs an Apple Developer account and a Windows code-signing certificate.
Without them the installers work but recipients see Gatekeeper/SmartScreen
warnings on first run. The scripts pick up credentials from environment
variables/secrets when present.

CI notes, from getting the first release green (runs 1-4):

- **CI runs on `stable`, which is newer than the local toolchain**, so lints
  diverge. `float-literal-f32-fallback` (rust#154024) is a warning locally and
  fatal under `clippy -D warnings`. If a lint failure appears that cannot be
  reproduced, check the toolchain versions before hunting for anything subtler.
- The lint step is a hard gate (`cargo fmt --check`, `clippy -D warnings` over
  all targets). It caught the above on its first run; the earlier
  `|| echo warning` version would have passed silently.
- `choco install nsis` does not put `makensis` on PATH for the running session.
  The Windows script resolves it from the usual install locations.
- `appimagetool` needs FUSE, which GitHub runners do not have. Use
  `APPIMAGE_EXTRACT_AND_RUN=1`.
- The `test` job links the GUI, so it needs the same system libraries as the
  Linux build job. Keep those two package lists identical.
- `workflow_dispatch` builds and uploads artifacts but publishes nothing; only a
  `v*` tag runs the publish job. Use a dispatch run to shake out changes before
  spending a version number.

egui is pinned to 0.33: 0.35 restructured the API (no `SidePanel`, `App::ui`
instead of `App::update`) and 0.36 requires rustc 1.95.

## Scope decision (2026-08-22)

Deliberately narrowed to **tool 1: offline excitation signal design, with a GUI**.
The data processing (offline model extraction, online estimation) is deferred.
Notes below record the design reasoning so it isn't re-derived later.

**Hard constraint: zero dependencies.** stdlib only — tkinter for the GUI,
hand-rolled FFT and optimizer, Canvas plotting instead of matplotlib. A flight
test tool that is one file and needs no `pip install` is materially more useful
than a fast one that needs an environment.

## Established

- **Signal generation is offline, always.** Phase optimization takes seconds to
  minutes; it is never a real-time operation. Online use = playback from a
  precomputed design library indexed by flight condition, not live redesign.
- **RPF** (relative peak factor, Morelli's convention) is
  `(max u - min u) / (2*sqrt(2)*rms(u))`. Values below 1.0 are legitimate and
  desirable — an optimized multisine is flat-topped and beats a pure sine's
  crest factor. Low RPF matters because it buys more injected energy per unit
  of peak actuator deflection.
- **Phase optimization**: Schroeder phases as analytic init, then either
  lp-norm minimization (smooth surrogate, p annealed 4 -> 256, analytic
  gradient) or Van der Ouderaa time/frequency swapping. `swap+lp` is the
  default. Direct minimization of RPF does not work — max/min are non-smooth,
  so gradient methods stall.
- **MIMO via interleaved bins** (Morelli): input 1 gets bins {1,4,7,...},
  input 2 {2,5,8,...}. Mutually exclusive bin sets make inputs orthogonal in
  both time and frequency over the record, which is what lets you separate each
  input's contribution to each output from a single maneuver. Interleave rather
  than block-partition, so every input spans the whole band of interest.
- **Exact arithmetic bin progressions matter enormously.** Measured on 8 tones:
  a true arithmetic run scored RPF 1.135, while jittering the same run by +/-1
  bin scored 1.48-1.58, and the near-uniform sets produced by snapping ideal
  positions to the nearest free bin (spacings 11,11,12,11) scored 1.36. So the
  allocator searches for the widest exact run that fits the free bins, and only
  falls back to greedy snapping when none exists. This was worth far more than
  any change to the phase optimiser -- 4 inputs x 8 tones went 1.362 -> 1.140.
  A run must still span >=85% of the requested band, otherwise it degenerates to
  a contiguous block covering half the band (30 tones collapsed to bins 3..32).

## Optimiser: things tried that did not help

Measured across five designs; none beat plain swap+lp within noise, several cost
2-4x the time:

- alternating swap/lp rounds rather than swap once then lp once
- a clip level that tightens over the swap run instead of a fixed 0.9
- raising the lp anneal ceiling from p=256 to p=1024
- Newman phases as an extra start alongside Schroeder
- jittering bin spacing to break up arithmetic progressions (much *worse*, see above)

What does help: exact arithmetic allocation (above), and occasionally more random
starts -- 120 starts found 1.3555 where 2 starts found 1.4177 on one case, but on
four of five cases extra starts changed nothing. Hence effort levels rather than
always paying for it.

The optimiser also scores candidates on the measurement grid rather than the
coarse optimisation grid. The coarse peak is an underestimate, so scoring there
both misranks starts and reports a better RPF than the exported signal has.

Sampled peaks are corrected by parabolic interpolation before setting the scale
factor, which keeps exported signals inside their peak limit (worst observed
overshoot 8e-6, versus 6e-4 without it).

## Deferred, but decided

### Architecture
- The real axis is **batch vs. recursive**, not GUI vs. library. A recursive
  Fourier transform run to the end of a record equals the batch DFT exactly;
  RLS with no forgetting factor equals batch LS exactly. So the streaming core
  *contains* the batch core — write streaming first, and offline becomes a
  consumer. Building batch-first produces something that structurally can't
  stream.
- Iterative methods (frequency-domain output error, ML) are inherently
  multi-pass and layer on top as offline-only.
- Layering: core (no I/O) -> streaming -> batch -> GUI. GUI contains zero math.
- **The contract between offline and online is a serializable design artifact**,
  not shared code: bins, amplitudes, phases, fs, record length, per-surface
  scaling, plus a hash. The GUI produces it, the aircraft plays it back, the
  estimator reads it to know which bins to watch. This is what makes it one
  tool rather than two.
- scipy naturally lands entirely on the offline side (phase optimization,
  iterative refinement). The streaming path needs no scipy: recursive FT is a
  multiply-accumulate, and RLS avoids matrix inversion via the
  matrix-inversion lemma on small (4-8 square) matrices. So a portable
  restricted-style core costs nothing.

### Estimation
- **Frequency-domain equation error** is the trustworthy formulation because
  `jw*X(f)` replaces numerical differentiation of noisy measurements. Protect
  this as a core invariant — no differentiation of noisy data anywhere.
- Model structure should be a **declarative regressor spec** (per output
  equation, which channels are regressors). Covers multirotor / fixed-wing /
  rotorcraft identically. Ship short-period and Dutch roll as example configs,
  never as built-ins.
- **Uncertainties are where these tools are usually wrong.** Naive LS
  covariance is optimistic because equation-error residuals are colored by
  unmodeled dynamics. Use the Klein & Morelli corrected (sandwich) covariance
  `(X'X)^-1 X' S X (X'X)^-1`, or at minimum the ~2x standard error inflation.
  Build it in from day one — anything shipped before it is misleading.
  Verify the exact form against Klein & Morelli rather than recollection.
- Detrending must happen before the FT, and differs between paths: full-record
  mean offline vs. something causal online. Easy to get silently inconsistent.

### INDI
- **INDI needs G (control effectiveness), not the full (A, B).** Using measured
  angular acceleration subsumes the aerodynamic model. So the online problem is
  RLS on `dv = G*du` — maybe 4x8 parameters, well conditioned, fast converging,
  and far easier to argue is safe in a control loop.
- **The injected multisine is the persistent-excitation guarantee.** Online RLS
  with a forgetting factor drifts and blows up its covariance when excitation
  dies out, which in normal flight is most of the time. Continuous low-amplitude
  multisine injection is the fix — meaning tool 1 is not only for dedicated ID
  maneuvers.
- Estimating G in the frequency domain dodges INDI's classic footgun: time
  domain control effectiveness estimation needs differentiated gyro, then the
  *identical* filter must be applied to u to keep them synchronized. Fast INDI
  loop still needs time-domain acceleration for control; the estimator does not
  inherit the problem. Fast loop consumes best-known G, slow estimator updates it.

### Open questions
- **Actuator space vs. virtual control space excitation.** Actuator space gives
  the full allocation matrix (needed for fault-tolerant INDI) but a quadrotor
  means 8 inputs sharing the band, so ~1/8 of the bins each. Virtual control
  space is 4 inputs with much better resolution but can't detect a single rotor
  degrading. Probably per-vehicle configurable.
- **Transition is the genuinely hard part.** A tailsitter or tiltrotor mid
  transition is violently time-varying, which breaks the stationarity assumption
  LTI identification rests on. The first-class object should be a *family of
  models indexed by a scheduling variable* (tilt angle, airspeed), with the
  design artifact carrying that index. Hover and cruise identify normally;
  transition either gets fenced into quasi-stationary slices or needs LPV,
  which is a much larger piece of work. Decide scope early.
- Multirotor/eVTOL are usually identified closed-loop, sometimes necessarily
  (open-loop unstable). Equation error tolerates this better than output error,
  but input/noise correlation still biases. Don't assume open-loop is default.

### Ideas worth building later
- **Predicted-covariance preview in the design GUI.** Given a proposed
  excitation, assumed noise level, and candidate model structure, compute the
  Fisher information and get parameter covariance *before flying* — build the
  regressor matrix from a simulated response and invert X'X. Even a crude
  nominal model shows which parameters a design leaves poorly excited, and makes
  bin-allocation choices comparable instead of a matter of taste. This is what
  turns tool 1 from a signal prettifier into the thing that determines whether
  tool 2 can succeed.
- **Odd-harmonic designs** (Schoukens): excite only odd bins so even-order
  nonlinear distortion lands on empty even bins where it can be *measured*
  rather than silently contaminating estimates. Cheap insurance on a flight
  vehicle, composes fine with Morelli interleaving.
- **Calibration Monte Carlo** as the next real milestone: truth model, multisine
  excitation, recursive FT, complex equation-error RLS, naive vs. corrected
  covariance, and check whether the error bars actually cover truth across a few
  hundred runs. Cheaper to run in a 200-line prototype than after there's a GUI
  on top.

## References
- Morelli, "Multiple Input Design for Real-Time Parameter Estimation in the
  Frequency Domain," IFAC SYSID 2003.
- Klein & Morelli, *Aircraft System Identification: Theory and Practice*,
  AIAA 2006.
- Schroeder, "Synthesis of low-peak-factor signals and binary sequences with
  low autocorrelation," IEEE Trans. Inf. Theory, 1970.
- van der Ouderaa, Schoukens & Renneboog, "Peak factor minimization using a
  time-frequency domain swapping algorithm," IEEE Trans. Instrum. Meas., 1988.

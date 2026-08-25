<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/logo-dark.png">
  <img src="docs/logo.png" alt="persistex" width="623">
</picture>

Excitation signal design for system identification of generic UAS — multirotor,
fixed-wing, and hybrid transition vehicles.

Ships as a single self-contained executable. Nothing to install alongside it: no
runtime, no interpreter, no shared libraries beyond what the OS provides.

## Install

Grab the installer for your platform from the [latest release](../../releases/latest).

| Platform | File | Install |
|---|---|---|
| macOS | `persistex-x.y.z-macos.dmg` | Open, drag persistex to Applications. Universal (Apple silicon + Intel) |
| Windows | `persistex-x.y.z-windows-setup.exe` | Run it. Per-user, no admin needed |
| Linux | `persistex-x.y.z-linux-amd64.deb` | `sudo dpkg -i persistex-*.deb` |
| Linux | `persistex-x.y.z-linux-x86_64.AppImage` | `chmod +x` and run |

### First run on an unsigned build

Releases are only signed if signing credentials are configured (see
[Packaging](#packaging)). Without them:

- **macOS** shows *"persistex cannot be opened because the developer cannot be
  verified"*. Right-click the app → **Open** → **Open**, once. Or
  `xattr -dr com.apple.quarantine /Applications/persistex.app`.
- **Windows** shows a SmartScreen warning. **More info** → **Run anyway**.

Expected for unsigned software, and not specific to this tool.

## What it does

Orthogonal phase-optimised multisines after Morelli. Each input is assigned a
mutually exclusive set of harmonic bins, interleaved so every input still spans
its band. That keeps the inputs orthogonal in both time and frequency over the
record, so a single manoeuvre separates every input's contribution to every
output. Phases are then optimised per input to minimise the relative peak factor,
which buys more injected energy for a given peak actuator deflection.

## Using it

Each input is a row: a name, an actuator peak limit, and a set of tones. Click the
**tones** button on a row to open its editor.

### The tone editor

A table of **frequency / period / amplitude**, one row per tone. Type into either
frequency or period and the other follows, so you can specify a 4-second tone
without doing the arithmetic. Amplitude defaults to 1 and is per tone.

Above the table are generators for when you do not want to type twenty rows: give
a band, a count, linear or logarithmic spacing, and an amplitude shape, then
**Replace** or **Append**. The generated rows are ordinary rows -- edit any of
them afterwards.

Generation steps around bins already used by other inputs, so generated sets stay
mutually exclusive and the inputs remain separable. Linear spacing looks for an
exact arithmetic progression, which optimises far better than a nearly-even one:
jittering a run by a single bin can cost 30% of RPF.

Frequencies snap to the nearest multiple of `f0 = 1/record_length`, since only
harmonics of the record are periodic over it. The row shows the band you asked
for and the band actually allocated.

**Optimise all** does every input; a row's **Run** does that input alone. Phases
start from Schroeder, so a design is reasonable before you optimise at all.
**Effort** picks the search budget: fast (1 start), standard (3), thorough (8).

## Record settings

Length is the fundamental period `T`, setting resolution `f0 = 1/T`. Repeats is
how many times the record plays back to back. Sample rate is the output rate.

The tool does not refuse designs. Tones above Nyquist, tones from two inputs
landing on the same harmonic, a record length that does not divide the sample
rate -- all of these are reported in the side panel and, for Nyquist, again when
exporting a CSV. None of them stop you building the signal.

### Reading the plots

The spectrum switches to a **log frequency axis** when the tones would otherwise
crowd against the left edge. The decision is made on the closest pair, not the
decade span, so a deliberately even set keeps its linear axis where the spacing is
visible. The checkbox next to the tab overrides it either way.

Dense traces render as a min/max envelope rather than a polyline, since a polyline
below a few pixels per cycle is a moire mess. That choice follows the bandwidth
carrying 90% of the amplitude, not the highest tone -- a 1/f set's top tone can be
a few percent of amplitude and should not by itself turn a smooth trace into a
filled band.

### RPF

Morelli's relative peak factor, `(max u − min u) / (2√2 · rms(u))` — the crest
factor normalised by that of a pure sine. **Values below 1.0 are legitimate**: an
optimised multisine is flat-topped and genuinely beats a sine's crest factor.

### Output

- **CSV** — `time` plus one column per channel, over the full record.
- **JSON** — the design artifact: bins, frequencies, scaled amplitudes, phases,
  peak limits, RPF, sample rate, record length, and a SHA-256 over the document.
  Meant as the contract between design and playback: this tool produces it, the
  aircraft plays it back, and an estimator reads it to know which bins to watch.

## Layout

```
core/       design, FFT, phase optimisation, export.  Zero dependencies.
gui/        egui application
packaging/  macos.sh, windows.ps1 + .nsi, linux.sh, icon assets
docs/       logo assets and their generator
NOTES.md    design reasoning, including the deferred estimation half
```

`core` deliberately has **no dependencies at all** — its own FFT, L-BFGS, RNG and
SHA-256 — so it can be lifted into an embedded or autocoded context later without
unpicking a dependency tree. Only the GUI pulls crates.

## Building

```bash
cargo run --release -p persistex     # run it
cargo test --workspace --release     # core regression + headless GUI tests
```

Requires Rust 1.92+ (egui 0.33's floor). egui is pinned: 0.35 restructured the
API and 0.36 needs rustc 1.95.

## Packaging

Each platform builds on its own runner — cross-compiling a GUI app is more trouble
than it is worth. `.github/workflows/release.yml` does all three on a `v*` tag and
attaches the installers to a GitHub release.

Locally:

```bash
./packaging/macos.sh          # -> dist/persistex-x.y.z-macos.dmg
pwsh ./packaging/windows.ps1  # on Windows, needs NSIS
./packaging/linux.sh          # on Linux
```

To ship signed builds, set these repository secrets:

| Secret | Effect |
|---|---|
| `MACOS_IDENTITY` | Developer ID to codesign with |
| `MACOS_NOTARY_PROFILE` | `notarytool` keychain profile; enables notarisation and stapling |
| `WINDOWS_PFX_PATH`, `WINDOWS_PFX_PASSWORD` | Authenticode signing of the .exe |

Signing needs an Apple Developer account and a Windows code-signing certificate.
Without them the build still produces working installers, with the first-run
warnings described above.

## Testing

`core/tests/regression.rs` holds golden values — synthesis, RPF, Schroeder phases,
the lp cost and gradient, and the RNG stream — captured from a Python/numpy
implementation that was validated against numpy and scipy before this port. The
suite also checks that bin allocations stay mutually exclusive and arithmetic,
that optimising one channel leaves the others untouched, that exported signals sit
inside their peak limits, and that the artifact hash covers the document.

The GUI tests drive the whole egui tree headlessly, which catches layout panics
and widget id collisions without needing a display.

## Status

Excitation design is complete. The data-processing halves — offline model
extraction with uncertainties, and online control-effectiveness estimation for
INDI — are designed but deliberately deferred. `NOTES.md` records that reasoning
so it does not have to be re-derived.

## License

MIT. See [LICENSE](LICENSE).

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/logo-dark.svg">
  <img src="docs/logo-light.svg" alt="persistex" width="623">
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

Each input is a row, with its own band, tone count, peak limit, spectral shaping
and spacing — channels need not resemble one another.

| Column | Meaning |
|---|---|
| Input | channel name, used in the CSV header and the JSON artifact |
| f min / f max | this input's own band |
| Tones | tones for this input alone |
| Peak | actuator limit. The channel is scaled so its waveform peak equals this |
| Amplitude | spectral shaping, flat through 1/f^1.5 |
| Spacing | linear or logarithmic across this input's band |
| RPF / Allocated band | computed. The band actually achieved after bins snap to the harmonic grid |

**Optimise all** does every input; each row's **Run** does that input alone, which
is what you want after editing one row. Phases start from Schroeder, so a design
is already reasonable before you optimise at all. **Effort** picks the search
budget: fast (1 start), standard (3), thorough (8 starts, p to 512).

Record settings are global: length (the fundamental period `T`, which sets
resolution `f0 = 1/T`), repeats, sample rate, and all harmonics versus odd only —
odd-only puts even-order distortion on empty bins where it can be measured.
Prefer a record length where `sample rate × length` is a whole number, or repeats
will not join seamlessly; the side panel warns when it is not.

### Bin allocation

Bins are allocated as **exact arithmetic progressions** wherever one fits the free
bins and spans at least 85% of the requested band. This matters more than it
sounds: an evenly spaced harmonic set optimises far better than a nearly-even one,
and jittering a run by a single bin can cost 30% of RPF. Bins stay mutually
exclusive across inputs whatever bands you ask for — that is what keeps the inputs
orthogonal over the record.

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

# CLAUDE.md

This is Parrot Cart MIDI (`parrot-midi`), a desktop program that plays the
MIDI music some Parrot Cart plugins send out of the cart's USB-C port. The
SCUMM plugin sends Monkey Island's Roland MT-32 music this way. The program
plays it on an emulated MT-32 or CM-32L, or passes it on to a real one. It
runs on macOS, Windows and Linux. README.md is for players and covers the
options.

## Layout

| Path | What |
|---|---|
| `src/main.rs` | the command line, and the window or headless mode |
| `src/gui.rs` | the window (eframe/egui on wgpu) |
| `src/midi.rs` | finding the cart's MIDI port (`CART_NAMES`) and reading it (midir) |
| `src/engine.rs`, `src/ring.rs` | the music thread and the audio buffer (cpal) |
| `src/synth.rs`, `src/mt32.rs` | the `Synth` trait, and the MT-32 behind it |
| `src/midifile.rs` | playing a `.mid` for `--test-file` |
| `src/paths.rs` | where the ROMs and settings live |
| `vendor/munt` | libmt32emu from Munt, built by `build.rs` with `cc` |
| `examples/fake_cart.rs` | a virtual MIDI port named like the cart, to test without one |
| `scripts/` | `package.sh` (a release download), `macos-app.sh` (the universal .app), `third_party_licenses.py` |

## Commands

```sh
cargo build --release                  # target/release/parrot-midi
cargo test --release --locked          # what CI runs
cargo run --release -- --headless --list
cargo run --release --example fake_cart -- song.mid        # macOS and Linux
cargo run --release -- --test-file song.mid --quiet --wav out.wav
```

Rust 1.95 or newer (edition 2024), and a C++ compiler for Munt. Linux needs
`pkg-config` and `libasound2-dev`.

## Releases

- Push a tag `vX.Y.Z` that matches `version` in Cargo.toml.
  `.github/workflows/release.yml` builds macOS (universal), Windows and Linux
  through `build.yml` and attaches the three downloads.
- `build.yml` also runs on pushes and pull requests: tests, then a package
  for each system.
- The macOS app is ad-hoc signed and not notarized, and the Windows build
  isn't signed. README.md tells players how to open it the first time.

## Rules

- The MT-32 and CM-32L ROMs are Roland's. Never add them to the repository,
  the tests or a release. The program finds a player's ROMs by their
  contents, so file names don't matter.
- Our code is MIT. Munt's libmt32emu is LGPL-2.1-or-later and linked
  statically, so keep `vendor/munt` complete and its license with every
  download (`package.sh` writes `THIRD-PARTY-LICENSES.txt`). A new crate
  needs its license in that file, which `third_party_licenses.py` makes.
- The cart's USB name is "Parrot Cart MIDI", the only name in `CART_NAMES`.
  The owner had the cart's old name taken out (2026-11-23); SCUMM 0.2.15
  and later use the new one.
- The `Synth` trait is there so a General MIDI (SoundFont) synth can be added
  later without touching the engine. Whether SCUMM gets a General MIDI option
  is still open.
- README text is short and plain, the way a person writes. No em dashes and
  no taglines.
- Commit with the no-reply identity set in this repo's git config, and check
  `git log -1 --format='%an %ae %cn %ce'`.

## Related

- The SCUMM plugin: https://github.com/plasticbugs/parrot-plugin-scumm. Its
  README has a section on hearing the MT-32 music with this program.
- The plugin starter kit, with a Claude skill for writing plugins:
  https://github.com/plasticbugs/parrot-plugin-template

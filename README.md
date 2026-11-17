# Parrot Cart MIDI

Hear the Parrot Cart's game music on your computer.

Some Parrot Cart plugins send their music out of the cart's USB-C port as
MIDI. The SCUMM plugin does this for Monkey Island's Roland MT-32 music.
This program plays that music through your computer's speakers, on an
emulated Roland MT-32 or CM-32L. It can also pass the music on to a real
MT-32.

It runs on macOS, Windows and Linux. It's free, and the source is here.

## Download

Get the newest version from the
[Releases page](https://github.com/plasticbugs/parrot-cart-midi/releases):

| System | File |
|---|---|
| macOS 10.15 or later (Apple silicon or Intel) | `parrot-midi-X.Y.Z-macos.zip` |
| Windows 10 or 11 (64-bit) | `parrot-midi-X.Y.Z-windows.zip` |
| Linux (x86_64) | `parrot-midi-X.Y.Z-linux.tar.gz` |

Unzip it. On macOS, drag **Parrot Cart MIDI** to your Applications folder.

### The first time you open it

The program isn't signed by Apple or Microsoft, so your computer warns you
the first time you open it.

- **macOS:** Double-click **Parrot Cart MIDI**. macOS says it can't check the
  app for malware. Click **Done**. Open **System Settings**, click
  **Privacy & Security**, and scroll down. Next to "Parrot Cart MIDI was
  blocked", click **Open Anyway**, and enter your password. Click
  **Open Anyway** once more. After that, the app opens normally.
  (On macOS 14 and earlier, you can Control-click the app, choose **Open**,
  and click **Open**.)
- **Windows:** Double-click `parrot-midi.exe`. If you see "Windows protected
  your PC", click **More info**, then **Run anyway**.
- **Linux:** Run `./parrot-midi`. There's no warning.

## The ROMs

The emulated MT-32 needs the two ROM files from a Roland CM-32L or MT-32:
the control ROM and the PCM ROM. They aren't included, because Roland owns
them. Use the ROMs from a CM-32L or MT-32 that you own.

CM-32L ROMs are best. LucasArts games use sound effects that the CM-32L
has and the MT-32 doesn't. MT-32 ROMs work too, without those sounds.

The file names don't matter. The program reads each file to see what it is.
It also takes ROMs that come in two halves.

To show the program your ROMs, click **Choose ROM folder…** and pick the
folder that has them. It remembers the folder.

It also looks in these places on its own:

- a folder named `roms` next to the program (on macOS, next to the app)
- your user data folder:
  - macOS: `~/Library/Application Support/Parrot Cart MIDI`
  - Windows: `%APPDATA%\Parrot Cart MIDI`
  - Linux: `~/.local/share/parrot-cart-midi`

It uses the first folder that has a full set. In that folder, a CM-32L set
comes before an MT-32 set.

## Play

1. Open Parrot Cart MIDI. It says **Waiting for the cart**.
2. Plug the cart's USB-C port into your computer.
3. Start the game on your ColecoVision.

The window says **Connected to Parrot Cart MIDI**, and the music plays.
The green panel shows what an MT-32's display would show.

The cart is a MIDI device only while a game that sends MIDI is running. You
can leave the program open. It connects again each time the cart comes
back.

On a Mac with Apple silicon, macOS may ask whether to allow the accessory to
connect. Click **Allow**.

Older cart firmware calls the cart "MovieCart MIDI". The program finds the
cart by either name.

## If you hear nothing

- **Start the program before the game.** At the start, the game sets up the
  MT-32's instruments. If the program wasn't listening then, the music
  sounds wrong or doesn't play. Restart the game.
- **Click Test sound.** If you hear a few piano notes, the program and your
  speakers work. If not, check your computer's volume and its sound output.
  The program plays through your computer's default output.
- **If it says Waiting for the cart:**
  - Make sure the game has started. The cart becomes a MIDI device only once
    the game runs.
  - Try another USB-C cable. Some cables only charge.
  - On a Mac, check that you allowed the accessory. The setting is in
    **System Settings > Privacy & Security > Allow accessories to connect**.
  - On Linux, run `parrot-midi --list`. Parrot Cart MIDI should be in the
    list of MIDI inputs.
- **If it says No ROMs yet,** click **Choose ROM folder…** and pick the folder
  with your ROM files.

## Use a real MT-32

If you have a real MT-32 or CM-32L and a USB MIDI cable:

1. Connect the cable's MIDI OUT plug to the MT-32's MIDI IN.
2. In the program, open **Play on** and choose the cable.
3. Start the game.

The program passes the music straight through. It doesn't need ROMs for this.

## Linux

The program needs ALSA (`libasound2`), and a desktop with X11 or Wayland
and working graphics drivers (OpenGL or Vulkan). **Choose ROM folder…** uses
your desktop's file chooser (through xdg-desktop-portal). The downloads are
built on Ubuntu 22.04 and run on Linux systems from about 2022 on.

To run it with no window, for example on a computer with no screen, use
`parrot-midi --headless`.

## Command line

`parrot-midi --help` shows these:

| Option | What it does |
|---|---|
| `--headless` | No window. Status goes to the terminal. Ctrl-C ends it. |
| `--list` | List the MIDI inputs and outputs. |
| `--from NAME` | Listen to the MIDI input whose name contains NAME, instead of the cart. |
| `--to NAME` | Send the music to the MIDI output whose name contains NAME (a real MT-32). |
| `--roms FOLDER` | Look for the ROMs in FOLDER first. |
| `--mt32` | Use MT-32 ROMs even when CM-32L ROMs are there. |
| `--wav FILE` | Also write what it plays to a WAV file. |
| `--quiet` | Make the sound, but don't play it. For tests. |
| `--test-file FILE` | Play a MIDI file (.mid) instead of listening for the cart. |
| `--version` | Show the version. |

`--wav`, `--quiet` and `--test-file` imply `--headless`. With `--quiet`,
`--test-file` runs as fast as it can.

On Windows, parrot-midi.exe is a window program, so Command Prompt doesn't
wait for it. Type `start /wait parrot-midi.exe --headless` to make it wait.

## Build it yourself

You need [Rust](https://rustup.rs) 1.95 or newer and a C++ compiler:

- macOS: the Xcode command line tools (`xcode-select --install`)
- Windows: Visual Studio Build Tools, with "Desktop development with C++"
- Linux: `build-essential`, `pkg-config` and `libasound2-dev` (Debian and
  Ubuntu names)

Then:

```sh
git clone https://github.com/plasticbugs/parrot-cart-midi
cd parrot-cart-midi
cargo build --release
```

The program is `target/release/parrot-midi`. On macOS,
`scripts/macos-app.sh` builds the app for both Apple silicon and Intel,
and `scripts/package.sh` makes a download like the ones on the Releases
page.

To test without a cart, on macOS or Linux, `examples/fake_cart.rs` pretends
to be one. It plays a MIDI file into a virtual MIDI port named like the
cart:

```sh
cargo run --release --example fake_cart -- song.mid
```

The MT-32 emulation is libmt32emu from the [Munt](https://github.com/munt/munt)
project, in `vendor/munt`. The build compiles it from there.

## License

This program's own code is under the MIT License (LICENSE). It includes
libmt32emu, which is under the GNU Lesser General Public License 2.1 or
later. THIRD-PARTY.md has the details.

Roland, MT-32 and CM-32L are trademarks of Roland Corporation. This project
isn't connected with Roland.

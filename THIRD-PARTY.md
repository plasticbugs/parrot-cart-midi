# Third-party software

Parrot Cart MIDI's own code is under the MIT License (see LICENSE). The
program is built with the software below. Release downloads also include
THIRD-PARTY-LICENSES.txt, with the license text of every Rust crate in the
program, and LGPL-2.1.txt.

## libmt32emu (Munt)

The MT-32 and CM-32L emulation is libmt32emu, from the Munt project.

- Source: https://github.com/munt/munt, tag `libmt32emu_2_8_6`
  (commit `91d2205eb8f2591d177b257b92e2c75f830508de`), in
  `vendor/munt/mt32emu` of this repository, unchanged.
- Copyright (C) 2003-2026 Dean Beeler, Jerome Fisher, Sergey V. Mikayev
  and the other Munt authors (see `vendor/munt/mt32emu/AUTHORS.txt`).
- License: GNU Lesser General Public License, version 2.1 or (at your
  option) any later version. The full text is in
  `vendor/munt/mt32emu/COPYING.LESSER.txt`, and in LGPL-2.1.txt in the
  release downloads.

parrot-midi links libmt32emu statically. The LGPL lets you change the
library and use your changed version with the program. Because all of the
program's source is published here, under the MIT License, you can always
do that: change the files in `vendor/munt/mt32emu` and build the program
again, as the README explains. That is how this program meets the LGPL's
terms for a statically linked library.

libmt32emu includes a SHA-1 implementation (`vendor/munt/mt32emu/src/sha1`)
under this license:

> Copyright (c) 2011, Micael Hildenborg. All rights reserved.
>
> Redistribution and use in source and binary forms, with or without
> modification, are permitted provided that the following conditions are met:
> Redistributions of source code must retain the above copyright notice,
> this list of conditions and the following disclaimer. Redistributions in
> binary form must reproduce the above copyright notice, this list of
> conditions and the following disclaimer in the documentation and/or other
> materials provided with the distribution. Neither the name of Micael
> Hildenborg nor the names of its contributors may be used to endorse or
> promote products derived from this software without specific prior
> written permission.
>
> THIS SOFTWARE IS PROVIDED BY Micael Hildenborg ''AS IS'' AND ANY EXPRESS
> OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED
> WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
> DISCLAIMED. IN NO EVENT SHALL Micael Hildenborg BE LIABLE FOR ANY DIRECT,
> INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES
> (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
> SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
> CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
> LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
> OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
> SUCH DAMAGE.

## Rust crates

These come from crates.io at the versions pinned in Cargo.lock. The main
ones:

| Crate | Used for | License |
|---|---|---|
| midir | MIDI in and out (CoreMIDI, WinMM, ALSA) | MIT |
| cpal | Sound output | Apache-2.0 |
| eframe, egui | The window | MIT OR Apache-2.0 |
| rfd | The folder picker | MIT |
| hound | Writing WAV files (`--wav`) | Apache-2.0 |
| midly | Reading MIDI files (`--test-file`) | Unlicense |
| ctrlc | Ctrl-C in headless mode | MIT OR Apache-2.0 |
| cc | Building libmt32emu (build time only) | MIT OR Apache-2.0 |

They pull in many smaller crates, all under permissive licenses (MIT,
Apache-2.0, BSD, Zlib, ISC, BSL-1.0, Unicode-3.0, Unlicense, or a choice of
these). Where a crate offers a choice that includes the GPL, this program
uses it under the permissive option. `scripts/third_party_licenses.py`
lists every one with its license text, and makes THIRD-PARTY-LICENSES.txt.

## Fonts

The window uses the fonts that come with egui (in the `epaint_default_fonts`
crate): Ubuntu Light (Ubuntu Font Licence 1.0), Hack (MIT, with Bitstream
Vera parts), Noto Emoji (SIL Open Font License 1.1) and emoji-icon-font
(MIT). Their license texts are in THIRD-PARTY-LICENSES.txt.

## ROMs

No Roland ROMs are included. The emulator needs the ROM files from a
CM-32L or MT-32 that you own.

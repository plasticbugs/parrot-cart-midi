# Vendored: Munt's mt32emu (libmt32emu)

- Upstream: https://github.com/munt/munt
- Tag: `libmt32emu_2_8_6`
- Commit: `91d2205eb8f2591d177b257b92e2c75f830508de`
- License: GNU Lesser General Public License, version 2.1 or later
  (`mt32emu/COPYING.LESSER.txt`), except `mt32emu/src/sha1`, which is
  BSD-3-Clause (Micael Hildenborg; the notice is in its files and in
  THIRD-PARTY.md).

This is the `mt32emu` folder of that commit, unchanged, without its tests
(`mt32emu/src/test`). `build.rs` compiles it with the `cc` crate and writes
the `config.h` that CMake would make from `src/config.h.in`. The rest of
upstream's CMake files are kept for reference but are not used.

To update: replace `mt32emu/` with the same folder from a newer tag, drop
`src/test`, update this file, and check `SOURCES` in `build.rs` against
the source list in `mt32emu/CMakeLists.txt`.

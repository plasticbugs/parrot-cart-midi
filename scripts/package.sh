#!/bin/bash
# Builds parrot-midi for this computer's system and packs it, with the
# README and licenses, into dist/:
#   parrot-midi-VERSION-macos.zip     (macOS: a universal app)
#   parrot-midi-VERSION-windows.zip   (Windows x64)
#   parrot-midi-VERSION-linux.tar.gz  (Linux x86_64)
# VERSION is the first argument, or the version in Cargo.toml.
set -euo pipefail
cd "$(dirname "$0")/.."

version="${1:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)}"
name="parrot-midi-$version"
stage="target/dist/$name"
rm -rf "$stage"
mkdir -p "$stage" dist

case "$(uname -s)" in
    Darwin)
        os=macos
        scripts/macos-app.sh
        cp -R "target/macos/Parrot Cart MIDI.app" "$stage/"
        ;;
    Linux)
        os=linux
        cargo build --release --locked
        cp target/release/parrot-midi "$stage/"
        ;;
    MINGW* | MSYS* | CYGWIN*)
        os=windows
        cargo build --release --locked
        cp target/release/parrot-midi.exe "$stage/"
        ;;
    *)
        echo "unknown system: $(uname -s)" >&2
        exit 1
        ;;
esac

cp README.md LICENSE THIRD-PARTY.md "$stage/"
cp vendor/munt/mt32emu/COPYING.LESSER.txt "$stage/LGPL-2.1.txt"
host=$(rustc -vV | sed -n 's/^host: //p')
if [ $os = windows ]; then python=python; else python=python3; fi
"$python" scripts/third_party_licenses.py --target "$host" > "$stage/THIRD-PARTY-LICENSES.txt"

case $os in
    macos) (cd target/dist && ditto -c -k --norsrc --noextattr --noacl --keepParent "$name" "../../dist/$name-macos.zip") ;;
    windows) (cd target/dist && 7z a -tzip "../../dist/$name-windows.zip" "$name" > /dev/null) ;;
    linux) tar -C target/dist -czf "dist/$name-linux.tar.gz" "$name" ;;
esac
ls -l dist/"$name"-*

#!/bin/sh
# Builds "Parrot Cart MIDI.app" for both Apple silicon and Intel Macs, in
# target/macos/. Needs both Rust targets:
#   rustup target add aarch64-apple-darwin x86_64-apple-darwin
set -eu
cd "$(dirname "$0")/.."

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-10.15}"

for target in aarch64-apple-darwin x86_64-apple-darwin; do
    cargo build --release --locked --target "$target"
done

app="target/macos/Parrot Cart MIDI.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS"
lipo -create -output "$app/Contents/MacOS/parrot-midi" \
    target/aarch64-apple-darwin/release/parrot-midi \
    target/x86_64-apple-darwin/release/parrot-midi
sed "s/@VERSION@/$version/g" packaging/macos/Info.plist > "$app/Contents/Info.plist"
printf 'APPL????' > "$app/Contents/PkgInfo"
# An ad-hoc signature: no developer identity, nothing from Apple. Apple
# silicon Macs need one to run the app at all, and a whole-bundle signature
# lets macOS offer "Open Anyway" instead of calling the download damaged.
codesign --force --sign - "$app"
codesign --verify --strict "$app"
echo "$app ($version)"
lipo -info "$app/Contents/MacOS/parrot-midi"

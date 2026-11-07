#!/usr/bin/env bash
# Build the Debian .deb — runs INSIDE a debian:trixie container (or any
# Debian/Ubuntu host with the build deps) with the project mounted at /src.
# Architecture follows the container/runner: amd64 and arm64 are both built
# natively this way (CI uses an arm64 runner; no emulation involved).
#
# Toolchain note: gtk4-rs 0.22 requires rustc >= 1.92, which is newer than
# Debian trixie's rustc (1.85), so the current stable Rust is installed via
# the official rustup installer. Distro cargo/rustc stay installed for
# consistency but are shadowed by rustup's PATH entry.
set -euxo pipefail

SRC=${SRC:-/src}
STAGE=/build/frame-1.0.0

apt-get update -qq
apt-get install -y -qq \
    build-essential debhelper cargo rustc pkg-config curl ca-certificates \
    libgtk-4-dev libadwaita-1-dev libmpv-dev \
    libegl1-mesa-dev libglx-dev >/dev/null

export RUSTUP_HOME=/opt/rustup
export CARGO_HOME=/opt/cargo
curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal \
    --default-toolchain stable --no-modify-path
export PATH="$CARGO_HOME/bin:$PATH"

echo "rustc: $(rustc --version)"
echo "cargo: $(cargo --version)"

rm -rf /build && mkdir -p /build
# dpkg-buildpackage needs a clean source tree; strip VCS + build output.
cp -a "$SRC" "$STAGE"
cd "$STAGE"
rm -rf target .git packaging .github dist
cp -a "$SRC/packaging/debian" ./debian
chmod +x debian/rules

dpkg-buildpackage -us -uc -b

mkdir -p "$SRC/dist"
cp /build/frame_*.deb "$SRC/dist/"
echo "---- built packages ----"
ls -la "$SRC/dist/"
dpkg-deb --info "$SRC"/dist/frame_*.deb | head -25

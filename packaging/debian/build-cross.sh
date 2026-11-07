#!/usr/bin/env bash
# Cross-compile the arm64 .deb inside an amd64 debian:trixie container using
# Debian's standard multiarch mechanism (dpkg --add-architecture + foreign
# build deps). This is the local-verification path for the arm64 package;
# CI builds arm64 natively on an arm64 runner instead. Both produce a real
# arm64 .deb — no emulation involved in this script.
#
#   sudo docker run --rm -v "$PWD:/src" -w /src \
#       debian:trixie bash packaging/debian/build-cross.sh
set -euxo pipefail

SRC=${SRC:-/src}
STAGE=/build/frame-1.0.0
HOST_ARCH=amd64
TGT_ARCH=arm64
TGT_TRIPLET=aarch64-linux-gnu

apt-get update -qq
# Declare arm64 as a foreign architecture. Both arches must resolve from the
# SAME suite: trixie main and trixie-updates carry different pcre2 revisions
# whose Breaks field forbids mixing across architectures.
dpkg --add-architecture arm64
rm -f /etc/apt/sources.list.d/debian.sources
cat > /etc/apt/sources.list <<'EOF'
deb [arch=amd64]  http://deb.debian.org/debian trixie main
deb [arch=arm64]  http://deb.debian.org/debian trixie main
EOF
apt-get update -qq

apt-get install -y -qq \
    build-essential debhelper curl ca-certificates pkg-config \
    crossbuild-essential-arm64 \
    libgtk-4-dev:arm64 libadwaita-1-dev:arm64 libmpv-dev:arm64 \
    libegl1-mesa-dev:arm64 libglx-dev:arm64 >/dev/null

export RUSTUP_HOME=/opt/rustup
export CARGO_HOME=/opt/cargo
curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal \
    --default-toolchain stable --no-modify-path
export PATH="$CARGO_HOME/bin:$PATH"
rustup target add aarch64-unknown-linux-gnu

echo "rustc: $(rustc --version)"

rm -rf /build && mkdir -p /build
cp -a "$SRC" "$STAGE"
cd "$STAGE"
rm -rf target .git packaging .github dist
cp -a "$SRC/packaging/debian" ./debian
chmod +x debian/rules

# Cargo cross-configuration: point the linker and pkg-config at the arm64
# toolchain and libraries.
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER="$TGT_TRIPLET-gcc"
export PKG_CONFIG_ALLOW_CROSS=1
export PKG_CONFIG_LIBDIR="/usr/lib/$TGT_TRIPLET/pkgconfig:/usr/share/pkgconfig"

dpkg-buildpackage -a "$TGT_ARCH" -us -uc -b

mkdir -p "$SRC/dist"
cp /build/frame_*.deb "$SRC/dist/"
echo "---- built packages ----"
ls -la "$SRC/dist/"
dpkg-deb --info "$SRC"/dist/frame_*arm64.deb | head -25

#!/usr/bin/env bash
# Build the aarch64 pacman package on an arm64 host (CI: ubuntu-24.04-arm)
# by chrooting into Arch Linux ARM's official aarch64 rootfs. There is no
# Arch container image for arm64 — Docker Hub's archlinux org is
# x86_64-only and no archlinuxarm org exists — so the rootfs tarball is the
# official artifact to build in. The runner is aarch64: nothing emulated.
#
# Trust note: the rootfs itself is fetched over http (ALARM mirrors do not
# serve TLS and the canonical host's certificate does not cover the os.
# subdomain). Everything pacman installs from ALARM's repositories inside
# the chroot is GPG-verified against the archlinuxarm keyring, so the
# toolchain packages are authenticated regardless of the rootfs transport.
set -euxo pipefail

SRC=${SRC:-/src}
CHROOT=${CHROOT:-/tmp/alarm}
ROOTFS_URL=${ROOTFS_URL:-http://os.archlinuxarm.org/os/ArchLinuxARM-aarch64-rootfs.tar.gz}

curl -fL --retry 3 -o /tmp/alarm-rootfs.tar.gz "$ROOTFS_URL"
SIZE=$(stat -c %s /tmp/alarm-rootfs.tar.gz)
echo "rootfs: $SIZE bytes"
test "$SIZE" -gt 100000000

mkdir -p "$CHROOT"
sudo tar -xpf /tmp/alarm-rootfs.tar.gz -C "$CHROOT"
sudo cp /etc/resolv.conf "$CHROOT/etc/resolv.conf"
sudo mkdir -p "$CHROOT/src" "$CHROOT/proc"
sudo mount --bind "$SRC" "$CHROOT/src"
sudo mount -t proc proc "$CHROOT/proc"
sudo chroot "$CHROOT" bash /src/packaging/arch/build.sh

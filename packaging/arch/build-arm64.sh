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

# Default to the working directory — unlike build.sh (whose /src default
# assumes the docker -v mount), this script runs directly on the host.
SRC=${SRC:-$PWD}
CHROOT=${CHROOT:-/tmp/alarm}
TARBALL=${TARBALL:-/tmp/alarm-rootfs.tar.gz}
ROOTFS_URL=${ROOTFS_URL:-http://os.archlinuxarm.org/os/ArchLinuxARM-aarch64-latest.tar.gz}

echo "disk headroom: $(df -h /tmp | awk 'NR==2{print $4}') free on /tmp"
curl -fL --retry 3 -o "$TARBALL" "$ROOTFS_URL"
SIZE=$(stat -c %s "$TARBALL")
echo "rootfs: $SIZE bytes"
test "$SIZE" -gt 100000000

# Integrity, not authentication: the md5 travels over the same http
# channel. Authentication comes from pacman GPG-verifying every package
# it installs inside the chroot.
if curl -fsSL -o "$TARBALL.md5" "$ROOTFS_URL.md5"; then
    EXPECT=$(awk '{print $1}' "$TARBALL.md5")
    ACTUAL=$(md5sum "$TARBALL" | awk '{print $1}')
    echo "rootfs md5: $ACTUAL (expected $EXPECT)"
    test "$EXPECT" = "$ACTUAL"
fi

mkdir -p "$CHROOT"
sudo tar -xpf "$TARBALL" -C "$CHROOT"

# The kernel hides mounts outside the reader's tree from /proc/mounts, so
# a chrooted pacman sees no `/` entry and cannot map its cachedir or its
# install targets to a filesystem ("could not determine cachedir mount
# point ... not enough free disk space"). Bind the chroot root onto
# itself — exactly what a container runtime provides — so the bind shows
# up inside as a `/` entry covering every path pacman touches. The proc
# mount must come after: a plain bind does not carry submounts, and would
# shadow the procfs mounted before it.
sudo mount --bind "$CHROOT" "$CHROOT"
sudo mount -t proc proc "$CHROOT/proc"

# pacman reads mount points from /etc/mtab; point it at procfs like a
# running system has it.
sudo ln -sf /proc/self/mounts "$CHROOT/etc/mtab"

# DNS: copying the host resolv.conf failed on CI — the host runs
# systemd-resolved, and its 127.0.0.53 stub answers the host but not
# processes inside the chroot. Write the host resolver's real upstream
# servers instead, then the Azure DNS proxy (GitHub runners live on
# Azure) and public resolvers as fallbacks; glibc tries them in order.
sudo rm -f "$CHROOT/etc/resolv.conf"
{
    resolvectl dns 2>/dev/null | awk '{for(i=1;i<=NF;i++) if($i ~ /^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$/) print "nameserver " $i}'
    printf 'nameserver 168.63.129.16\nnameserver 1.1.1.1\nnameserver 8.8.8.8\n'
} | awk '!seen[$0]++' | head -n 3 | sudo tee "$CHROOT/etc/resolv.conf" >/dev/null

# Fail fast, with the config in the log, if DNS still cannot resolve.
if ! sudo chroot "$CHROOT" getent hosts mirror.archlinuxarm.org; then
    echo "chroot DNS check failed; resolv.conf content:" >&2
    sudo cat "$CHROOT/etc/resolv.conf" >&2
    exit 1
fi

sudo mkdir -p "$CHROOT/src"
sudo mount --bind "$SRC" "$CHROOT/src"
sudo chroot "$CHROOT" bash /src/packaging/arch/build.sh

#!/usr/bin/env bash
# Build the Fedora .rpm — runs INSIDE a fedora container (or any Fedora host
# with the build deps) with the project mounted at /src. Architecture follows
# the container: x86_64 and aarch64 are both built natively this way.
set -euxo pipefail

SRC=${SRC:-/src}
VERSION=1.0.0

dnf install -y -q gcc rust cargo pkgconf-pkg-config \
    gtk4-devel libadwaita-devel mpv-devel rpm-build >/dev/null

echo "rustc: $(rustc --version)"

mkdir -p ~/rpmbuild/{BUILD,RPMS,SOURCES,SPECS,SRPMS}

# Produce the release-style source tarball expected by Source0.
WORK=$(mktemp -d)
cp -a "$SRC" "$WORK/frame-$VERSION"
cd "$WORK/frame-$VERSION"
rm -rf target .git packaging .github dist
cd "$WORK"
tar --sort=name --owner=0 --group=0 --numeric-owner \
    -czf ~/rpmbuild/SOURCES/frame-$VERSION.tar.gz "frame-$VERSION"

cp "$SRC/packaging/fedora/frame.spec" ~/rpmbuild/SPECS/frame.spec
rpmbuild -bb ~/rpmbuild/SPECS/frame.spec

mkdir -p "$SRC/dist"
cp ~/rpmbuild/RPMS/*/*.rpm "$SRC/dist/"
echo "---- built packages ----"
ls -la "$SRC/dist/"
rpm -qip "$SRC"/dist/*.rpm | head -25 || true

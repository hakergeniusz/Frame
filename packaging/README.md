# Packaging

Distributable packages for Frame, built for **AMD64 and ARM64**. Every
package contains the same payload: `/usr/bin/frame`, the desktop entry, the
AppStream metainfo file, the full hicolor icon set, and the licence.

Both architectures are always built the same way: natively on matching
hardware. **AMD64 is built and verified locally** (this host + amd64
containers). **ARM64 is built by GitHub Actions on native arm64 runners**
(`ubuntu-24.04-arm` in `.github/workflows/packages.yml`) — no emulation, no
cross-compilation shortcuts.

## Debian (.deb)

Files: `debian/` — debhelper 13 packaging (`control`, `rules`, `changelog`,
DEP-5 `copyright`).

```sh
docker run --rm -v "$PWD:/src" -w /src debian:trixie bash packaging/debian/build.sh
# → dist/frame_1.0.0_<arch>.deb
```

`dpkg-shlibdeps` computes the runtime `Depends` from the actual ELF
dependencies, so `libmpv2` (or the distribution's libmpv package) is pulled
in automatically and stays under its own licence — see the note in
`debian/copyright` (Frame's code is MIT; the mpv engine is LGPL-2.1-or-later
or GPL-2.0-or-later depending on the distribution's mpv build).

## Fedora (.rpm)

Files: `fedora/frame.spec`.

```sh
docker run --rm -v "$PWD:/src" -w /src fedora:latest bash packaging/fedora/build.sh
# → dist/frame-1.0.0-1.*.rpm
```

The spec's `License: MIT` covers Frame's own content; `mpv-libs` is an RPM
dependency, not shipped content, and remains under its own licence.

## Arch (PKGBUILD)

Files: `arch/PKGBUILD`.

```sh
bash packaging/arch/build.sh          # container or native Arch host
# → dist/frame-1.0.0-1-<arch>.pkg.tar.zst
```

`build.sh` stages the tree, generates a release-style tarball, substitutes
its real sha256 into the PKGBUILD, and runs `makepkg`. The shipped PKGBUILD
points at the GitHub release tag; regenerate its checksum with `updpkgsums`
when the tag is created (see IMPLEMENTATION_DECISIONS.md).

## Flatpak

Files: `flatpak/io.github.hakergeniusz.Frame.yml`.

```sh
flatpak-builder --user --install-deps-from=flathub --force-clean \
    --repo=repo dist packaging/flatpak/io.github.hakergeniusz.Frame.yml
flatpak build-bundle repo frame-1.0.0-<arch>.flatpak io.github.hakergeniusz.Frame
```

The manifest builds FFmpeg (LGPL configuration — no `--enable-gpl`, no
GPL-only components), libass, libplacebo (OpenGL-only), and mpv with
`-Dgpl=false` (libmpv only), then Frame itself. Frame's crates are vendored
into `cargo-sources.json` (generated from `Cargo.lock` by
`flatpak-builder-tools/cargo`; regenerate it after every dependency change)
so the build is fully offline — the build sandbox has no network, matching
Flathub policy. Runtime `org.gnome.Platform//49` with the rust-stable SDK
extension (branch resolved from the SDK metadata; install
`org.freedesktop.Sdk.Extension.rust-stable//25.08` explicitly on CI).
Finish args are least-privilege: DRM render, Wayland/X11, audio socket,
network for URL streaming, read-only home for opening media files (mpv
cannot use the document portal), and create access to `~/Pictures/Frame`
for screenshots.

**Re-verify after every mpv/FFmpeg bump**: confirm `mpv -Dgpl=false` still
passes meson's GPL guard and that no GPL library entered the FFmpeg build.

## CI

`.github/workflows/ci.yml` — build + tests + metadata validation on amd64
and arm64 runners for every push/PR.
`.github/workflows/packages.yml` — on tags (`v*`) or manual dispatch: builds
all four package formats on both architectures and uploads artifacts.

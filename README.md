# Frame

**Frame** is an independent, keyboard-driven media player for the Linux
desktop, built with GTK 4, libadwaita, and the libmpv playback engine.

It plays local video files, whole folders, and network URLs, with playlists
and shuffle, watch history, multiple audio and subtitle tracks (including a
*secondary* subtitle track), subtitle styling (delay, position, scale,
colors, font, background), playback speed, A-B looping, chapter navigation,
frame stepping, zoom and picture adjustments, screenshots, session restore,
and a statistics overlay.

Frame is a clean-room reimplementation: it was written from behavioral
specifications of the user experience only, without reference to any other
player's source code. See `docs/` for those specifications.

## Installation

Packages are built for **AMD64 and ARM64**.

### Debian and derivatives (.deb)

```sh
sudo apt install ./frame_1.0.0_amd64.deb    # or *_arm64.deb
```

Runtime dependencies (libgtk-4, libadwaita, libmpv) are resolved
automatically by the package manager.

### Fedora (.rpm)

```sh
sudo dnf install ./frame-1.0.0-1.*.rpm      # x86_64 or aarch64
```

### Arch Linux (PKGBUILD)

```sh
# from the project root, or via your AUR helper once published
bash packaging/arch/build.sh
sudo pacman -U dist/frame-1.0.0-1-*.pkg.tar.zst
```

### Flatpak

```sh
flatpak install ./frame-1.0.0-<arch>.flatpak
```

The Flatpak build compiles mpv with `-Dgpl=false` against an FFmpeg
configuration that enables no GPL-only components, so the bundled engine is
LGPL-2.1-or-later; the rest of the bundle is Frame's MIT code. Permissions
are minimal: graphics/audio/network access, read-only home (for opening
media files), and write access to `~/Pictures/Frame` (screenshots).

## Building from source

Requirements: Rust ≥ 1.85 (edition 2024), GTK 4 ≥ 4.14, libadwaita ≥ 1.5,
libmpv ≥ 2.0 development files, and the EGL/GLX development headers.

```sh
cargo build --release
cargo test
./target/release/frame <video-file-or-url>
```

## Keyboard shortcuts

Press <kbd>Ctrl</kbd>+<kbd>?</kbd> inside the player for the full list
(searchable). Highlights: <kbd>Space</kbd>/<kbd>K</kbd>/<kbd>P</kbd>
play/pause, <kbd>J</kbd>/<kbd>L</kbd> seek, <kbd>[</kbd>/<kbd>]</kbd> frame
step, <kbd>Ctrl</kbd>+<kbd>L</kbd> A-B loop, <kbd>C</kbd> subtitles,
<kbd>Z</kbd>/<kbd>Alt+Z</kbd> subtitle/secondary track cycling,
<kbd>S</kbd>/<kbd>Shift+S</kbd> screenshots, <kbd>I</kbd>/<kbd>Shift+I</kbd>
statistics, <kbd>F</kbd>/<kbd>F11</kbd> fullscreen, <kbd>Ctrl+U</kbd> open
URL.

## Configuration and data

Plain JSON files under `XDG_CONFIG_HOME/Frame/` (preferences, watch history,
resume positions, saved sessions). Screenshots are saved to
`~/Pictures/Frame/`.

## Licence

Frame's original code and assets are released under the **MIT licence**
(see `LICENSE`). The playback engine is **libmpv**, dynamically linked and
*not* part of this project's source or licence: libmpv is
LGPL-2.1-or-later in the LGPL-compatible build configuration (the Flatpak
build verifies this configuration directly), or GPL-2.0-or-later when using
a distribution's default mpv build. GTK 4, libadwaita, and GLib are
LGPL-2.1-or-later system libraries. See
[THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md) for the full per-component
breakdown.

## Development

- `IMPLEMENTATION_PLAN.md` — requirement checklist and roadmap
- `IMPLEMENTATION_DECISIONS.md` — every behavioural or technical decision
  and its source in `docs/`
- `IMPLEMENTATION_QUESTIONS.md` — open questions for the project owner
- `CONTRIBUTING.md` — how to build, test, and send changes
- `packaging/README.md` — how the packages are built and verified

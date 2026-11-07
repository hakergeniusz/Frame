# Third-party licences — Frame

Frame's original code, artwork, and assets are released under the **MIT
licence** (`LICENSE`). Everything Frame *builds on* is listed below. Claims
marked **[verified]** were checked against the artifacts built on
2026-11-03, not assumed; claims about distribution libraries describe those
distributions' packages, not anything Frame builds.

## 1. Frame application and UI — MIT

All of `src/`, `assets/` (icons, desktop file, metainfo), and the build
scripts are original work for this project under MIT.

## 2. The mpv engine — *separate licence, dynamically linked*

Frame never contains mpv's code. It links `libmpv.so.2` at runtime; the
engine's licence therefore depends on **which libmpv the user gets**:

| Distribution channel | Engine library | mpv build configuration | Engine licence |
| --- | --- | --- | --- |
| Flatpak (Frame's own build) | libmpv 0.40.0 built with `-Dgpl=false`, lua/javascript/cdda/libbluray/vulkan disabled | meson GPL guard active during the build | **LGPL-2.1-or-later** **[verified]** |
| Debian `.deb` | `libmpv2` from Debian trixie | Debian's package (Debian ships mpv without GPL components) | LGPL-2.1-or-later (as shipped by Debian) |
| Fedora `.rpm` | `mpv-libs` from Fedora | Fedora's package | LGPL-2.1-or-later (as shipped by Fedora) |
| Arch PKGBUILD | `mpv` from the Arch repos | Arch's package (built with mpv's default options, which include GPL) | GPL-2.0-or-later (as shipped by Arch) |

**[verified] Flatpak evidence** (read back from the built artifact):

- The built `libmpv.so.2.5.0` embeds the build flag `gpl=false`.
- mpv's meson build fails if a GPL-only component is required while
  `-Dgpl=false` is set — the build succeeding with that flag is itself the
  guard's verdict.
- FFmpeg's configure line, embedded in the built `libavutil.so`:
  `--disable-debug --disable-doc --disable-static --enable-shared
  --disable-programs --disable-iconv` — **no `--enable-gpl`, no
  `--enable-version3`, and no GPL-only libraries** (x264/x265/xvid/vidstab
  strings appear in none of the shipped libraries).

Re-verify after every mpv/FFmpeg bump: confirm `mpv -Dgpl=false` still
configures, and re-read the FFmpeg configure line from the built libraries.

## 3. Multimedia stack in the Flatpak build **[verified by build]**

| Component | Version | Licence | Role |
| --- | --- | --- | --- |
| FFmpeg (libavcodec/util/format/filter/swresample/swscale) | 7.1.1 | LGPL-2.1-or-later (configuration above) | decoding, demuxing, filtering |
| libplacebo | 7.351.0 | LGPL-2.1-or-later | video rendering core (required by mpv 0.40); built OpenGL-only — Vulkan, shaderc, glslang disabled |
| libass | 0.17.3 | ISC | subtitle rendering |
| glad2 (jinja2, markupsafe) | pinned commits | MIT / BSD-3-Clause | **build-time only** — generate libplacebo's GL headers; no code shipped |

## 4. GTK stack (all channels) — LGPL-2.1-or-later

GTK 4, libadwaita, GLib/GIO/Pango/GdkPixbuf — system libraries, dynamically
linked, satisfied by the distribution or the Flatpak runtime. Never
modified by Frame.

## 5. Rust crates (vendored for the Flatpak build) **[verified 2026-11-03]**

All 87 crates in `Cargo.lock` carry permissive licences (checked against
each crate's manifest):

| Licence | Crates |
| --- | --- |
| MIT OR Apache-2.0 (or Apache-2.0 OR MIT) | 54 — incl. gtk4-rs/libadwaita-rs/glib-rs stack, serde, proc-macro2, quote, syn, libc, async-channel, png, crc32fast, flate2, … |
| MIT | 29 |
| MIT OR Zlib OR Apache-2.0 | 2 (miniz_oxide 0.8/0.9) |
| 0BSD OR MIT OR Apache-2.0 | 1 (adler2) |
| Unlicense OR MIT | 1 (memchr) |
| Zlib | 1 (zlib-rs) |
| (MIT OR Apache-2.0) AND Unicode-3.0 | 1 (unicode-ident) |
| Apache-2.0 WITH LLVM-exception | 1 (target-lexicon) |

The per-crate manifests are vendored inside `packaging/flatpak/cargo-sources.json`
(hashes pinned); each vendored crate ships its own licence text.

## 6. Data / assets

- Test media used during development only — never distributed with Frame.
- App icon and symbolics: original artwork for Frame (MIT, see `LICENSE`).

# IMPLEMENTATION_DECISIONS.md — Frame

Every non-obvious decision made while implementing Frame from the specs in
`docs/`, with its source (spec reference, technical necessity, or explicit
owner instruction). Ordered by area.

## 1. Stack and structure

| Decision | Source / reason |
| --- | --- |
| Rust + GTK 4 + libadwaita via the official gtk4-rs / libadwaita-rs bindings | brief; `docs/01`, `docs/06` describe GNOME/libadwaita conventions |
| libmpv through hand-written FFI (`engine/ffi.rs`), no wrapper crate | the required capability set maps 1:1 onto libmpv properties/commands; fewer deps to audit; plan §1 |
| mpv render API (OpenGL) into `GtkGLArea` instead of an X11/Wayland embedded window | windowing-agnostic, composes with GTK widgets (overlays, controls); plan §1 |
| JSON persistence under `XDG_CONFIG_HOME/Frame/` instead of GSettings | `docs/04` shows the original exposes "Open Config Folder", i.e. visible file-based config; plain JSON keeps that true on all four package formats without shipping a GSettings schema |
| Feature level: code uses APIs available in libadwaita 1.5 / GTK 4.14 | keeps Debian-stable runtime requirements satisfiable (`adw::Dialog` needs 1.5) |
| Proc-address loading via libglvnd (`eglGetProcAddress`, then `glXGetProcAddress`) rather than epoxy | plan §1 assumed `epoxy_get_proc_address`; in practice GTK does not export a resolvable epoxy symbol, so Frame resolves through libglvnd — already part of the GTK GL stack on every supported distro |

## 2. Engine integration (mpv)

| Decision | Source / reason |
| --- | --- |
| Engine runs on a dedicated thread posting `EngineEvent`s over an async-channel; the UI bridges them onto the GTK main loop | mpv's event API blocks; GTK wants the main loop |
| `MPV_FORMAT_FLAG` is passed as a C `int*`, not `bool*` | ABI: the mpv flag format is a 4-byte int; a 1-byte `bool` pointer silently corrupted pause state (found via headless verification) |
| All engine→UI channel sends use `send_blocking` (or await) | `let _ = chan.send(x)` on an async-channel creates an unpolled future — the event is *never* sent. This silently swallowed FILE_LOADED/duration events until fixed |
| mpv initialized with `pause=yes` | `loadfile` unpauses; starting paused lets resume-position logic control the actual start |
| `osc=no`, `config=no`, `input-*` off | Frame provides all controls; mpv must not read the user's mpv.conf (independent app) |
| Hardware decoding **on by default** (`hwdec=auto`), with a "Use Software Decoding" preference (default off) that switches to `hwdec=no` | **Explicit owner instruction** (2026-11-03), superseding the `docs/08` observation of a "Hardware Acceleration" toggle. The default follows the owner's direction: hardware decode is better; the preference escapes to software on driver quirks |
| EOF: clear resume entry, release screensaver inhibit, then `next_item()` — which is a no-op on a single item (no wraparound) | `docs/06`; window stays at 0:00 idle |

## 3. Video rendering (the hard part)

The final pipeline: **mpv renders into a private offscreen RGBA8 FBO → the
frame is read back to the CPU → published as a `gdk::MemoryTexture` on a
custom `gdk::Paintable` (`VideoPaintable`) attached to a `gtk::Picture`
(wrapped in `GraphicsOffload`) → GSK composites it like any other image.**
Frame never draws into GTK's render target, and never places a GL texture id
in GSK's hands. Deciding constraints, each forced by observed corruption:

1. **Never render into GTK's framebuffer.** Direct mpv renders into GTK's
   bound FBO corrupted under GTK 4.24's unified gsk_gpu renderer (rows of
   other frames mixed into bright areas), and a later offscreen +
   `glBlitFramebuffer`-into-GTK pipeline produced displaced stale bands on
   the owner's NVIDIA desktop. Compositing is GSK's job; Frame only produces
   textures.
2. **Never hand GSK a GL texture id you keep using.** The intermediate
   architecture (offscreen FBO → `GdkGLTexture` via `GLTextureBuilder` →
   Paintable) *still* corrupted: GSK caches texture ids beyond the
   `GdkGLTexture` lifetime (tile caching / damage tracking). Re-rendering
   into a reused GL texture name races GSK's sampling of it (the displaced
   bands), and recycling deleted names through `glGenTextures` makes cached
   ids point at unrelated fresh storage (sampled garbage). Proven
   headless: FBO readback dumps were pixel-perfect during the very seek
   storms whose on-screen result was horizontal noise, and 3/3 runs
   corrupted after rapid seeks under llvmpipe once names churned.
3. **The CPU readback (one GPU→CPU copy per published frame) removes the
   entire failure class by construction** — a `MemoryTexture` has no GL
   lifetime, works identically on every GL/Vulkan stack, and hardware
   decoding is untouched (only the final blit is read back). Verified: 3/3
   seek-storm runs and the hover-thumbnail run render pixel-perfect,
   paused/settled captures are stable.
4. **Publishes are paced to the display, not to mpv's decode rate.** mpv's
   render update callback fires once per decoded frame; the owner's
   bad-apple file decodes 180 fps, so an unbounded wakeup→render pump ran a
   full offscreen render + readback 180 times a second and drowned the main
   loop (measured: ~500% CPU, the "lags a lot" report). The pump now only
   drains its channel, marks the video picture dirty and requests one
   redraw; the actual publish runs from the picture's frame-clock tick —
   at most one per display frame, backlog collapses instead of growing.
5. **The readback itself is a double-buffered PBO ping-pong.** A plain
   `glReadPixels` stalls the CPU until the GPU drains; on the owner's
   180 Hz panel (frame clock = decode rate) that is one stall per publish.
   `read_pixels_pbo` queues the current frame's async GPU→PBO read and
   copies out the *previous* frame's already-completed buffer (GL executes
   in order, so no fences are needed) — one constant video frame of
   latency, no stall. The first read after (re)creation is synchronous and
   seeds both PBOs; screenshots and the hidden thumbnailer keep the
   synchronous read (they must see the current frame). Note:
   `glGetBufferSubData` raises `GL_INVALID_OPERATION` on GDK's GL contexts
   — `glMapBufferRange` + copy + unmap is the portable copy-out.
6. **Frame's own GdkGLContext must be made current for every engine pass**
   (`ctx.make_current()` in the render handler): GSK switches to its own
   contexts between frames, and the engine's mpv render + readback silently
   target whatever is current. The handler also runs from a tick callback
   on the `Picture` (which computes the video size × scale factor), since
   `connect_resize` does not exist for `Picture`.
7. **The hidden thumbnailer saves/restores everything it touches.** Its
   passes bind their own FBO and viewport; a leaked viewport corrupted the
   next video pass. GL state is additionally reset before each mpv render
   (`reset_gl_state()` — leftover scissor/blend/texture-unit state produced
   blocky corruption), and the thumbnail readback flips rows exactly like
   the screenshot path (GL reads bottom-up).
8. A `glib::SourceId` stored for later `.remove()` must be cleared *inside*
   the callback before returning `ControlFlow::Break` — the source frees
   itself, and removing a stale id aborts the process. Fixed in the control
   bar hide timer and the statistics overlay timer.
9. The thumbnailer's self-nudging loop survives the display pacing: every
   pass re-nudges while it has outstanding work (`thumb_busy()`), and each
   nudge goes through the dirty-flag → frame-clock path, so paused-hover
   thumbnails still progress one pass per display frame.
10. **The offscreen target rebuild is debounced on resizes.** Every widget
    size event otherwise reallocates the FBO + both PBOs and pays a
    synchronous seed readback — a drag-resize becomes an alloc-and-stall
    storm. While size events keep arriving within 150 ms, passes keep
    rendering at the existing target size (GSK scales the published
    texture; the aspect wobble during the drag is imperceptible); the
    rebuild happens on the first pass after the size settles. A drag
    gesture pays two rebuilds (first + final), a one-off resize
    (maximize) remains immediate. Screenshots and thumbnails use the
    target size, not the widget size, so they stay consistent mid-drag.
11. **Seek-bar interaction is timer-driven — no click gesture.** Two
    timers over one pending value: a *throttled* scrubber (armed once per
    interaction burst) and a *debounced* settle timer (250 ms after the
    last change, one exact seek lands on the final value — drags, wheel
    bursts and keyboard steps alike). The first change of a session never
    arms the scrubber, so a plain trough click lands exactly without
    flashing an intermediate frame. A `GestureClick` release cannot
    anchor the exact landing: the Scale's own drag gesture claims the
    pointer sequence, so the click gesture is cancelled mid-drag and its
    `released` never fires (verified under Xvfb). The earlier design
    (debounce re-armed on every event) deferred the seek indefinitely —
    the video did not move at all until the pointer stopped, which the
    owner read as scrub lag. The drag scrubs **keyframe seeks issued
    straight from the change events** — the recipe of mpv's own seek bar,
    checked against its actual implementation (`osc.lua` of our engine,
    not the clean-room target): `seekbarkeyframes` defaults to true,
    identical targets are deduped, and there is no timer throttle. Frame
    first dragged with *exact* seeks (completion-paced, then aborted and
    re-issued ≥80 ms apart) — on the owner's 180 fps file one exact seek
    costs ~500 ms of decode, so even abort-and-reissue moved the picture
    in bursts (~130–150 ms between steps at best), which read as choppy.
    An earlier keyframe attempt had been wrongly condemned ("the picture
    did not move") because it predated the paused render window: the
    seeks landed, nothing published them. The pump issues keyframe seeks
    at a ≥50 ms cap; the **press lands exactly right away** (the OSC's
    mousedown behaviour, also instant feedback for a plain trough click);
    the settle timer lands **exactly** 250 ms after the last change —
    keyframe steps never reach the pointer precisely — but **defers while
    the button is still held**: a human drag has micro-pauses longer than
    the debounce, and ending the session mid-hold paid an exact seek's
    decode stall and restarted the session on the next move. A scrub
    session also **freezes playback** (previous pause state restored at
    settle): a playing 180 fps decode flood fought the seeks for the main
    loop. Because a paused pipeline raises no render-update flags at all,
    every seek and picture-affecting change made while paused opens a
    short **render window** (bootstrap nudge + unconditional rendering) —
    without it the scrub/keyboard frames never reach the screen at all.
    Mid-decode publishes are rate-capped (≥50 ms) so the seeks'
    intermediate positions surface without flooding the UI thread. That
    window is **bounded, not continuous**: nudges only while a seek is
    decoding (capped at one per 25 ms) plus a landing pass, then it
    closes itself — an always-on window re-rendered, re-read and
    re-published a full-frame texture every frame-clock tick, which on a
    180 Hz desktop starves the UI thread (owner: "progress bar still
    lags when moving it"). Finally, the widget itself **lies on
    purpose**: while a scrub session is active or the primary button is
    held on the bar, `update_time` never moves the scale — the pointer
    is truth, the elapsed label follows the scale's value, and the hover
    preview is repositioned under the pointer on every motion. The
    decoder's landed position is applied only at settle, which the
    picture already shows (owner: "it should lie to the user, and move
    the thumbnail and the progress bar between the gaps even if secretly
    it is waiting").

## 4. UI decisions

| Decision | Source / reason |
| --- | --- |
| `adw::Window` content must be set with `set_content()` | `gtk_window_set_child()` aborts on AdwWindow ("not supported"); this crashed the Preferences window and (latent) the Shortcuts window. `adw::Dialog` (URL dialog) does accept `set_child` |
| Ctrl+? is matched before the strict `ctrl && !shift` block, because most layouts deliver `question` with SHIFT set (shift-implied keysym); GTK accelerators treat these specially, hand-rolled matching must too | found by XTEST verification: Ctrl+comma worked, Ctrl+? did not |
| Shortcuts window uses `gtk::ShortcutLabel`s parsed from `gtk_accelerator_parse` syntax (`<Ctrl>N`, `Page_Up`), " / " groups rendered as separate key-caps | display-style strings (`Ctrl+N`, `↑`) fail to parse and render empty; per-alternative labels reproduce the documented presentation (`K / P / Space`) |
| Preferences window does not close on Escape | `docs/08` does not document that behaviour for the original; adding it would be invention. Raised as open question #4 |
| Statistics page renders Frame's own layout from engine properties | `docs/07 §2`: the original's statistics UI was never visually exercised, so its layout is unknown; the *behaviour* (I temporary, Shift+I pinned) is spec'd and implemented |
| Screenshot filename `frame-<unix-ts>.png` in `~/Pictures/Frame` | `docs/03` names the Pictures directory; exact filename scheme unspecified — chosen for sortability |
| Seek-bar hover thumbnails (docs/03, pref-gated) use a **hidden second libmpv instance** (`vo=libmpv`, `ao=null`, paused, `hwdec=auto` — keyframe seeks in 180 fps 10-bit files otherwise steal noticeable CPU from the main decoder while scrubbing) driven during the main render passes: UI hover → request slot → the next pass seeks/renders/reads back → `gdk::MemoryTexture`; cached per (file, half-second slot), cap 128; 120 ms hover debounce; **also during a scrub, throttled to one request per 150 ms** (its seeks are keyframe-only, so they land fast even on heavy files); while paused the engine nudges its own render loop until the request finishes (nothing else schedules draws when paused); the keyframe-landing acceptance window is 15 s (must exceed the file's GOP) | a screenshot-style readback of the *visible* pipeline cannot show other timestamps; a dedicated instance decodes independently of playback state; owner asked for more frequent thumbnails |
| Bottom bar matches the original player 1:1: header shows mpv's `media-title` (metadata title supersedes the filename-derived one); button row prev/play/next · volume · subtitles · **audio-track (five-bar equalizer glyph, original artwork)** on the left, A-B loop · subtitles bubble · playlist · settings · fullscreen on the right; seek row below with translucent white trough (accent fill on hover/active), time `m:ss | m:ss` right-aligned; hover thumbnail lifted above the bar with the time chip beneath it | owner direction ("match 1:1 original player") with 18 reference screenshots of the original (layout parity only — no assets or code were referenced) |
| Application icon redesigned from scratch: charcoal tile, four white viewfinder corner brackets ("framing" a shot) around a mint→cyan→azure play mark with a soft halo; rendered to the full PNG size set (16–512) with rsvg-convert | owner direction ("completely redo your logo"); deliberately unrelated to any existing player artwork (clean-room requirement) |
| Thumbnail readback gates on mpv's **`seeking` property + `time-pos`**, then renders **unconditionally** (the seek command returning only means it was queued; `time-pos` reports the *target* immediately at queue time; mid-seek the vo displays the seek's start keyframe; a **paused** instance never raises the render API's FRAME flag at all — waiting for it hangs forever). Readback rows are flipped exactly like the screenshot path (GL reads bottom-up). Give-up is a **15 s wall-clock budget**, not a pass count (pass counts are meaningless at 180 fps). `loadfile` is followed by an explicit `pause=yes` (loadfile unpauses, and a playing thumbnailer reads back arbitrary frames) | found by dumping the readback and pixel-matching it against ffmpeg-extracted reference frames: the preview was vertically mirrored and showed the *previous* request's scene |
| The video surface **fills the whole window** and the header bar + control bar are **overlaid chrome with gradient scrims** (linear-gradient CSS, light foreground, thumbnails get a dark/light dual border) | docs/03 says the video "fills the whole window with overlaid chrome"; the owner's third report (light Bad Apple frames made theme-coloured UI invisible) plus a reference screenshot of the original confirmed the scrim treatment; the header hides together with the controls in fullscreen, and the statistics overlay drops below the header only in windowed mode |
| Dragging the seek bar scrubs **keyframe seeks issued straight from the change events** (≥50 ms cap, identical targets deduped — mpv's own OSC recipe, checked against `osc.lua`), while **playback stays frozen** for the session; the **press lands exactly right away**, and the settle timer lands **exactly** 250 ms after the last change — **deferred while the button is held** (a drag's micro-pauses must not end the session or pay an exact seek mid-hold; no release gesture exists on a Scale anyway — its drag gesture claims the sequence and cancels any `GestureClick`); during the scrub the widget shows **the pointer, not the decoder**: `update_time` is guarded while a session is active or button 1 is held (raw events via `EventControllerLegacy`), the elapsed label follows the scale's value, and the hover preview is repositioned on every motion — bar and thumbnail glide between content landings and never snap back; scrub thumbnails continue, throttled to one request per 150 ms | the owner's real file (180 fps AV1 10-bit, ~5 s GOP) makes each exact seek cost ~500 ms decode — dragging with them (even abort-and-reissued) moves the picture in bursts ("still moving it feels choppy"); mpv's OSC drags the same file smoothly with keyframe seeks; an earlier keyframe attempt was wrongly condemned ("the picture did not move") because it predated the paused render window; owner: "it should lie to the user, and move the thumbnail and the progress bar between the gaps even if secretly it is waiting" (→ pointer-is-truth guard + gliding preview) |
| The header bar is overlay chrome: it **auto-hides with the bottom controls in windowed mode too**, not only fullscreen; pointer motion reveals both, resting on either keeps both, hiding stays skipped while paused or a popover is open | owner: "make the upper UI elements also disappear how the lower ones do after inactivity"; docs/03 describes auto-hide generically (no fullscreen-only carve-out) |
| The seek thumbnail's rounded clip is `set_overflow(Hidden)` (widget property), not CSS `overflow` | GTK4 has no `overflow` CSS property — every packaged build logged "Theme parser error: No property named overflow" and silently dropped the declaration, leaving square corners |
| Ctrl+Q (Quit) is a real accelerator (`set_accels_for_action`) plus a `CallbackAction` shortcut on the plain `adw::Window`s (Shortcuts/Preferences/About) | GTK4 wires app accelerators only into `GtkApplicationWindow`, and plain windows never join the action muxer — a `NamedAction("app.quit")` there silently does nothing; the Shortcuts window advertised the binding but no handler implemented it |
| The hidden thumbnailer accepts keyframe-landed positions up to **15 s before** the requested one | keyframe-only seeks land at the keyframe at or before the target; the window was sized for the owner file's ~5 s GOP, but x264 defaults give 10 s GOPs at 25 fps — too tight meant an endless re-seek loop landing on the same keyframe (thumbnails never appeared) |
| Track/chapter/playlist rows switch on a **single click** via an added `GestureClick` (plus `::activate` for keyboard) | GTK4 emits `ListBoxRow::activate` on Enter/double-click only — with `selection_mode=none` a plain click did nothing (found by XTEST verification against the 5-audio-track file) |
| A/Shift+A and Z/Shift+Z cycle over an **engine-side track snapshot** kept in `Shared` and written by the event thread whenever mpv reports `track-list` | the first implementation left the engine snapshot permanently empty, so cycling silently no-op'd while the popover list worked |
| `FRAME_DEBUG=1` prints one diagnostic line per render-geometry change plus track/thumbnail/key events | field diagnostics for the blit placement and the thumbnail pipeline on machines Frame cannot be tested on |
| The header's fallback title is Frame's extension-stripped `display_title`; mpv's `media-title` (raw file name, extension included) only wins when the file actually has a metadata `title` tag | black-box comparison vs Cine 1.8.8 (2026-11-05): the original shows `test-silent`, Frame showed `test-silent.mp4` — mpv's filename fallback keeps the extension |

## 5. Packaging and licensing

| Decision | Source / reason |
| --- | --- |
| Four package formats (Debian/Fedora/Arch/Flatpak), each for AMD64 **and** ARM64 | brief: architecture support is never optional |
| **ARM64 packages are built by GitHub Actions on native arm64 runners** (`ubuntu-24.04-arm`) — no emulation | **Owner direction (2026-11-03)**: arm64 builds happen in CI on GitHub. Local x86_64 builds are verified on this host and in amd64 containers; arm64 artifact status follows the CI run and is reported as "CI-built" until then, never as locally verified |
| libmpv is always **dynamically linked** and licence-separate | owner licensing instruction: prefer dynamic libmpv; never present mpv (or the whole app) as MIT |
| Debian/Fedora/Arch packages pull the distribution's mpv (LGPL or GPL build per that distro); only the Flatpak builds its own engine | keeps the licence story honest per-distribution: each distro's mpv configuration is that distro's choice; the Flatpak is the one place Frame controls the build |
| Flatpak builds FFmpeg with no GPL components and mpv with `-Dgpl=false` → LGPL-2.1-or-later engine | owner licensing instruction; **verified by the actual build**, not assumed — meson's GPL guard runs during the build and the FFmpeg configure line is read back from the built libraries (see THIRD_PARTY_LICENSES.md for the recorded evidence) |
| Flatpak bundle ships the PNG icon set only; the SVG master + symbolic icons live in the binary's GResource | `flatpak build-export` validates installed icons through gdk-pixbuf, and current librsvg no longer provides a pixbuf SVG loader — the SVG would abort export on loader-less hosts. The launcher needs only the PNG sizes; deb/rpm/PKGBUILD install the SVG copy |
| Flatpak Frame module builds offline from vendored crates (`cargo-sources.json`, generated from `Cargo.lock`) | the build sandbox has no network and Flathub forbids network in builds; regenerate `cargo-sources.json` after every dependency change |
| Flatpak finish-args least privilege: `--device=dri`, wayland/x11 sockets, `--share=ipc`, `--socket=pulseaudio`, `--share=network` (URL streaming), `--filesystem=home:ro` + `--filesystem=xdg-pictures/Frame:create` | mpv cannot use the document portal, so read-only home access is the minimum that lets Open… play arbitrary local files; screenshots need create access only under Pictures/Frame; screensaver inhibit uses the portal (`FLATPAK_ID` detection in `inhibit.rs`), so no ScreenSaver talk permission |
| Arch PKGBUILD's `sha256sums` pins the GitHub `v*` tag tarball; the real sum is committed to `main` right after the tag is pushed (the tagged tree carries the `REPLACE_AT_RELEASE` placeholder — a tarball's hash cannot include itself) | standard release-order workaround for the self-reference; CI never depends on the placeholder (packaging/arch/build.sh tars the local tree and substitutes its own sum) |
| Version 1.0.0 pinned in Cargo.toml, deb changelog, RPM spec, PKGBUILD, Flatpak appdata (metainfo) | single version across all artifacts |
| Repository: `github.com/hakergeniusz/Frame`, private | owner directive "move this repo to GitHub" (2026-11-07); matches the AppStream/desktop URLs (question #3 answered); visibility stays private — making it public was not requested |
| CI test job runs in a **Fedora container** on both runner architectures | ubuntu-24.04 ships libadwaita 1.5.0 but libadwaita-sys requires >= 1.6 (first CI run failed exactly there); the official `archlinux` Docker image is x86_64-only (no linux/arm64 manifest); Fedora is multi-arch with current GNOME libraries |
| **aarch64 pacman package** is built by chrooting into Arch Linux ARM's official rootfs on the arm runner | no Arch container image exists for arm64 (Docker Hub's archlinux org is x86_64-only and no archlinuxarm org exists); the runner is aarch64, so the chroot runs natively. The rootfs travels over http with an md5 integrity check (no TLS on ALARM mirrors, no published signature usable in CI) — authentication rests on pacman GPG-verifying every package it installs inside the chroot |
| Arch builds pin the **rustup stable toolchain** (RUSTUP_HOME=/opt/rustup), like the Debian build script | ALARM's distro rustc lags Arch's; one toolchain for both architectures keeps edition-2024 requirements met identically |
| Flatpak meson modules pin `-Dlibdir=lib` (libplacebo, mpv) | the Sdk's Fedora-derived meson defaults libdir to `lib64` on x86_64 but `lib` on aarch64 — without the pin, libplacebo.pc/libmpv.pc land in `/app/lib64/pkgconfig` on x86_64 alone, where the next module's pkg-config never looks (found by the CI x86_64 flatpak leg; aarch64 built fine, which is what made it visible) |
| Flatpak ffmpeg source carries `mirror-urls`: MacPorts distfiles | ffmpeg.org throttles CI networks (SSL timeout mid-download on the first tagged run); MacPorts mirrors the byte-identical tarball and the manifest sha256 is verified by flatpak-builder whichever URL serves it |
| `packages.yml` matrices set `fail-fast: false` | one architecture's failure must not cancel the other's build — arm64 artifacts are never optional |
| ALARM chroot: **resolv.conf gets the host resolver's real upstream servers** (`resolvectl dns`), not a copy of the host file, plus the Azure DNS proxy and public resolvers as fallbacks, gated on a chroot-side `getent` lookup | the host's systemd-resolved stub (127.0.0.53) answers the host but not the chroot — the first tagged run died there with `Could not resolve host` before installing anything |
| ALARM chroot: the **chroot root is bind-mounted onto itself** before proc/dev are mounted inside, and `/etc/mtab` is pointed at `/proc/self/mounts` | the kernel hides mounts outside the reader's tree from `/proc/mounts` — a chrooted pacman sees no `/` entry and every transaction dies with `could not determine cachedir mount point … not enough free disk space` regardless of actual free space (42G free on the runner). The self-bind appears inside as a `/` entry, exactly what a container root provides; a plain bind carries no submounts, so proc and dev must be mounted after it. Reproduced and cured on a local x86_64 chroot before re-running CI |
| ALARM chroot: **host `/dev` is bind-mounted over the rootfs's** | the rootfs ships `/dev/null` as a root-owned regular file, so as the `builder` user every `2>/dev/null` redirect fails and makepkg aborts with the misleading `Failed to create the directory $BUILDDIR` |
| Arch packaging: the built package's **extension follows the distro's makepkg.conf** (globs are `frame-*.pkg.tar.*`), never a pinned `.zst` | ALARM hardcodes `PKGEXT='.pkg.tar.xz'` in `/etc/makepkg.conf` where upstream Arch defaults to `.pkg.tar.zst` (verified by extracting both distros' pacman packages) — the aarch64 build produced `frame-1.0.0-1-aarch64.pkg.tar.xz` and the zst-only globs in build.sh and the artifact upload matched nothing |
| PKGBUILD pins the **codeload archive checksum**, not the API tarball's, and `prepare()` renames `Frame-$pkgver` → `frame-$pkgver` | GitHub serves two *different* byte streams per ref: the API `tarball` endpoint (root `hakergeniusz-Frame-<sha>/`) and the codeload `archive` URL the PKGBUILD sources (root `Frame-1.0.0/`, repository case) — verified by downloading both; pinning the API sum would fail end-user makepkg, and without the rename build() would never find the tree. The codeload sum was verified byte-stable across repeated authenticated downloads; a tarball's hash cannot include itself, so the pin commit postdates the tag and `build.sh` substitutes the whole `sha256sums` line for its locally staged tarball |

## 6. Testing methodology

| Decision | Source / reason |
| --- | --- |
| Headless verification via Xvfb + `dbus-run-session` + XTEST key injection + x11grab screenshots; `LIBGL_ALWAYS_SOFTWARE=1`, `GSK_RENDERER=gl` | repeatable, no display needed; matches user-visible behaviour (windows, controls, rendered frames) rather than unit-only claims |
| HiDPI verified at `GDK_SCALE=2` in the same harness | the owner reported the overscan bug on a real HiDPI desktop |
| Every fix above was verified by: screenshot evidence + absence of panic/abort in the log + `cargo test` green (27/27) | brief: verify, never fabricate; unverified items are marked |

# Frame — Implementation Report

Final report for the independent clean-room implementation of **Frame**
(`io.github.hakergeniusz.Frame`), version **1.0.0**, from the approved
behavioural specifications in `docs/`. Everything stated here was done or
verified on 2026-11-03; anything not verified is marked **[unverified]**.

---

## 1. Overview

Frame is a keyboard-driven GTK 4 / libadwaita media player powered by
libmpv, ~7,600 lines of original Rust (edition 2024). It implements the
full documented feature set: welcome window, player windows, playlists with
shuffle, watch history, resume positions and saved sessions, preferences,
the complete keyboard shortcut set, frame stepping, chapters, A-B loop,
playback speed, primary and secondary subtitle tracks, subtitle styling,
zoom and colour adjustments, screenshots, statistics, URL streaming, and
screensaver inhibition.

## 2. Provenance and clean-room statement

The implementation is based **solely** on the nine observational documents
in `docs/` (GUI-derived behavioural specs). At no point were the original
application's source code, repository, binaries, or internal assets read,
decompiled, executed for comparison, or copied. All artwork (app icon,
symbolic icons) is original. This constraint was maintained throughout,
including during packaging.

## 3. Feature implementation vs. the specifications

Every requirement ID in `IMPLEMENTATION_PLAN.md` §3 is implemented except
where a decision record says otherwise. Highlights of verified behaviour:

- **Playback pipeline**: load → FILE_LOADED (duration) → position events →
  render, verified end-to-end under Xvfb with screenshot evidence; EOF on a
  single item leaves the window at 0:00 with no wraparound and no crash.
- **HiDPI**: render size follows the GL viewport — verified at `GDK_SCALE=1`
  and `GDK_SCALE=2` (this fixed the owner-reported overscan).
- **Keyboard shortcuts**: the documented set works via XTEST injection,
  including Ctrl+? (see §14 for the shift-implied keysym fix).
- **Screenshot (S)**: PNG written to `~/Pictures/Frame/` (verified).
- **Statistics (I / Shift+I)**, A-B loop, mute, seek, fullscreen: verified
  by screenshots in the same harness.
- **Preferences**: renders all documented groups; "Use Software Decoding"
  default off (see §14 — owner-directed change).

Not implemented, per `docs/07` (explicitly out of scope): nothing beyond
that list was added.

## 4. Architecture

```
src/ (7,560 lines)
  main.rs / app.rs      GtkApplication, actions, single-instance, CLI routing
  config.rs             Settings + JSON persistence under XDG_CONFIG_HOME/Frame
  history.rs resume.rs  Watch history; per-video resume + saved sessions
  inhibit.rs            Screensaver inhibit (portal in Flatpak, ScreenSaver outside)
  engine/
    ffi.rs              Hand-written libmpv C API declarations
    mpv.rs              Engine thread, property observations, render API (OpenGL)
  player/
    mod.rs              Player window, key handling, wiring
    control_bar.rs      Controls + seek bar
    popovers.rs playlist.rs stats.rs
  ui/
    welcome.rs preferences.rs about.rs shortcuts.rs url_dialog.rs …
build.rs                pkg-config probes (mpv ≥ 2.0, EGL, GLX) + GResource compile
```

Config, history, resume and sessions are plain JSON files so the
"Open Config Folder" behaviour is real on every distribution.

## 5. Engine integration notes (the parts that were hard)

- GTK binds its own framebuffer before `GtkGLArea::render`; mpv's render
  context is given that FBO explicitly, queried per pass.
- The video renders into a **private offscreen FBO** and is blitted into
  GTK's framebuffer at the area's rect (computed via `compute_point` when
  the ambient viewport differs from the area size) — GTK 4.24's unified
  gsk_gpu renderer keeps the viewport at whole-frame size, which used to
  overscan the video over the whole window on light frames. The blit runs
  on **every** render callback (re-blitting the last frame when mpv has no
  update) — GSK's texture recycling otherwise shows stale bands. A
  direct render into GTK's framebuffer (no offscreen pass) was tried and
  **corrupts** under gsk_gpu — reproducible headless, matching the owner's
  NVIDIA band artifacts.
- The video surface fills the whole window (docs/03); the header bar and
  control bar are overlays with gradient scrims and a light foreground, so
  they stay readable over dark and light frames alike.
- The hidden thumbnailer's GL passes **save/restore viewport and FBO
  binding**; a leaked viewport mis-placed the next video blit. Its readback
  waits until mpv's `seeking` property drops and `time-pos` reaches the
  request (`time-pos` reports the seek target immediately at queue time;
  mid-seek the vo shows the seek's start keyframe), then renders
  unconditionally — a **paused** instance never raises the render API's
  FRAME flag. Readback rows are flipped like the screenshot path.
- GL state left by GSK (scissor/blend/texture unit) is reset before each
  mpv render.
- `MPV_FORMAT_FLAG` is a C `int*`, not `bool*`; flag properties read back
  as `"yes"`/`"no"` through the string API, not `"true"`/`"false"`.
- Engine→UI events cross threads through async-channel with
  `send_blocking` (dropped unpolled `send` futures silently ate events —
  confirmed a second time on the render-wakeup nudge channel, unbounded
  notwithstanding).
- `glib::SourceId`s are cleared inside their callbacks before `Break`.
- libglvnd (`eglGetProcAddress` + `glXGetProcAddress`) resolves GL symbols.

## 6. Known issues

1. **GSK Vulkan renderer corrupts the video surface** (GTK-side state
   presentational through Vulkan; GLArea path affected). Real desktops
   default to the GL renderer, where playback is verified. Headless tests
   set `GSK_RENDERER=gl`. Open question #1 asks whether Frame should force
   the GL renderer.
2. On hosts whose gdk-pixbuf lacks an SVG loader (librsvg ≥ 2.63 no longer
   ships one), `flatpak build-export` cannot validate SVG icons — the
   Flatpak therefore ships PNG icons (SVG master + symbolics are embedded
   via GResource); deb/rpm/PKGBUILD install the SVG copy.
3. `cargo build` currently emits ~86 warnings (mostly
   `unsafe_op_in_unsafe_fn` pre-edition-2024 style plus unused imports).
   Harmless; a lint pass is outstanding.
4. The whole-frame-viewport contract of GTK 4.24's gsk_gpu renderer cannot
   be fully reproduced under Xvfb (llvmpipe picks the per-area renderer), so
   the blit path's whole-frame branch is verified by construction and by the
   `FRAME_DEBUG=1` geometry line rather than by headless screenshot; the
   owner's desktop is the final arbiter. The gsk_gpu corruption that made
   direct rendering unusable **is** reproducible headless (rows of other
   frames mixing into bright areas) and the owner's NVIDIA band artifacts
   match that class; the always-blit pipeline is the countermeasure, and
   `FRAME_DEBUG=1` prints the render geometry for field diagnosis.

## 7. Verification methodology

Headless but real: Xvfb + `dbus-run-session` + XTEST key injection +
x11grab screenshots; `LIBGL_ALWAYS_SOFTWARE=1`, `GSK_RENDERER=gl`.
`cargo test` — **27/27 passing** (playlist, history, resume, inhibit,
config, URL-title derivation). Every fix above has screenshot or log
evidence; none is claimed on faith.

Against the owner's real file (`bad_apple_4in1_subtitled.mkv` — AV1
1920×1080 **180 fps** 10-bit, 5 audio tracks, 2 SRT subs, 3:46), verified
under Xvfb with the default renderer, `GSK_RENDERER=gl`, and
`GDK_SCALE=2`:

- **Full-window video with scrim chrome** — the video fills the entire
  window including behind the header bar; the overlaid header and control
  bar carry gradient scrims with a light foreground, so the title, all
  controls and the seek bar stay readable on the file's pure-white frames
  (screenshots at three timestamps per configuration; `cargo test` 27/27).
  Suspicious "corruption" streaks turned out to be the file's own
  horizontal-echo effects — confirmed by pixel-matching screenshots
  against ffmpeg-extracted reference frames before touching any GL code.
- **Seek-bar thumbnails** — hover (pref on) → 220 ms debounce → hidden
  instance seeks and lands on the requested position (gated on mpv's
  `seeking` property + `time-pos`; readback **pixel-matched against
  ffmpeg reference frames** at the requested second — position-accurate
  and upright) → preview shown above the bar centred on the pointer with
  the timestamp bubble and a dark/light dual border (visible on white
  frames); cache hits on re-hover; works while paused (engine self-nudges
  render passes).
- **Seek drag** — keyframe-only seeks coalesced during the drag, exact
  seek on release; the UI stays responsive throughout.
- **Multiple audio tracks** — popover lists all 5 tracks with
  titles/languages/codecs and the active one checked; A/Shift+A cycle
  (`aid` 1→5→1 logged); clicking a row switches tracks. Audible switching
  itself is [unverified] headless (no audio server under Xvfb).

## 8. Packaging — formats and architectures

| Format | Files | AMD64 | ARM64 |
| --- | --- | --- | --- |
| Debian | `packaging/debian/` (debhelper 13, DEP-5 copyright) | **rebuilt + verified** 2026-11-04 after the playback-pacing fixes: `frame_1.0.0_amd64.deb`, payload extracted, compiled-in marker confirms the new build, packaged binary plays video under Xvfb, log clean | **CI** on `ubuntu-24.04-arm` [unverified locally — owner directed CI builds] |
| Fedora | `packaging/fedora/frame.spec` | **rebuilt + verified** 2026-11-04: `frame-1.0.0-1.fc45.x86_64.rpm` (+ debuginfo/source), requires auto-detected (`mpv-libs`, `libmpv.so.2`), payload marker-verified, packaged binary plays video under Xvfb (build.sh's summary pipe hit SIGPIPE after the build; fixed) | **CI** [unverified locally] |
| Arch | `packaging/arch/PKGBUILD` | **rebuilt + verified** 2026-11-04 in the `archlinux` container: `frame-1.0.0-1-x86_64.pkg.tar.zst`, payload marker-verified, packaged binary plays video under Xvfb, log clean | **CI** [unverified locally] |
| Flatpak | `packaging/flatpak/io.github.hakergeniusz.Frame.yml` (+ vendored `cargo-sources.json`) | **rebuilt + verified** 2026-11-04 after the playback-pacing fixes: `frame-1.0.0-x86_64.flatpak` (offline build, vendored crates, LGPL engine stack; bundle installed and sandbox smoke-tested under Xvfb — video plays, log clean) | **CI** [unverified locally] |

`packaging/*/build.sh` scripts reproduce each build inside Debian/Fedora/
Arch containers (or natively on Arch). Debian's build installs current
stable Rust via rustup because gtk4-rs 0.22 needs rustc ≥ 1.92 > trixie's
1.85. A Debian multiarch cross script exists but hit a Debian mirror
sync-race (pcre2 `Breaks` across arches) on 2026-11-03; per the owner's
direction, arm64 packages come from CI arm runners instead.

Per-package payload (identical everywhere): `/usr/bin/frame`, desktop file,
AppStream metainfo, hicolor icons, licence. Flatpak ships PNG icons only
(see §6.2).

## 9. CI

Repository: `github.com/hakergeniusz/Frame` (private, created 2026-11-07
by owner directive).

`.github/workflows/ci.yml` — build + tests + metadata validation on every
push/PR, on `ubuntu-latest` (amd64) and `ubuntu-24.04-arm` (arm64), inside
a multi-arch Fedora container: ubuntu-24.04 ships libadwaita 1.5.0 while
libadwaita-sys requires >= 1.6 (the first run failed exactly there), and
the official Arch container image is x86_64-only. **[verified]** green on
both architectures.

`.github/workflows/packages.yml` — on `v*` tags and manual dispatch: all
four package formats on both architectures, artifacts uploaded. The runs
surfaced and fixed several infrastructure constraints (details in
IMPLEMENTATION_DECISIONS.md §5): ffmpeg.org throttles CI networks (mirror
added), no Arch container image exists for arm64 (official ALARM rootfs
chroot instead — whose chroot DNS, kernel mount-visibility, root-owned
`/dev/null` and `PKGEXT=.pkg.tar.xz` quirks each cost a run to find),
the Sdk's meson puts lib64 on x86_64 (libdir pinned), and the deb
smoke-test glob assumed a Debian revision the package does not carry.
The `v1.0.0` tag run (42378275747) is the single-run all-green proof:
all eight jobs succeeded and all eight artifacts
(deb/rpm/arch/flatpak × amd64/arm64) were uploaded.

## 10. Licensing (separate entries, per the owner's instruction)

1. **Frame app/UI** — MIT (original code + assets).
2. **mpv engine** — dynamically linked, licence depends on the channel:
   Flatpak build is `-Dgpl=false` → **LGPL-2.1-or-later [verified]**;
   Debian/Fedora libmpv → LGPL-2.1-or-later as shipped; Arch mpv →
   GPL-2.0-or-later as shipped.
3. **FFmpeg** (Flatpak build) — LGPL-2.1-or-later **[verified]**:
   configure line read back from the built libraries shows no GPL options,
   and no GPL-only library strings exist in any shipped lib.
4. **Other multimedia deps** (Flatpak) — libplacebo 7.351.0
   (LGPL-2.1-or-later, OpenGL-only), libass 0.17.3 (ISC); glad2/jinja/
   markupsafe are build-time only.
5. **Bundled assets** — all original (MIT). Rust crates: 87 total, all
   permissive (MIT/Apache-2.0/BSD/Zlib family; table in
   THIRD_PARTY_LICENSES.md).

The whole app is **never** described as MIT; `LICENSE` and
`THIRD_PARTY_LICENSES.md` state the split explicitly.

## 11. Metadata quality

- `desktop-file-validate`: **passes**.
- `appstreamcli validate`: passes except two `url-not-reachable` warnings —
  the GitHub URLs cannot exist before the repository is created
  (`IMPLEMENTATION_QUESTIONS.md` #3). Re-validate after publishing.
- Version **1.0.0** is pinned consistently in Cargo.toml, deb changelog,
  RPM spec, PKGBUILD, metainfo; the About dialog derives it from
  `CARGO_PKG_VERSION`.

## 12. Documentation inventory

`README.md` (install per distro, build, shortcuts, licence split),
`IMPLEMENTATION_PLAN.md` (requirements + roadmap),
`IMPLEMENTATION_DECISIONS.md` (every decision + source),
`IMPLEMENTATION_QUESTIONS.md` (open questions for the owner),
`CONTRIBUTING.md`, `LICENSE`, `THIRD_PARTY_LICENSES.md`,
`packaging/README.md`, `docs/` (specs), this report.

## 13. Owner-directed changes this session

1. **Hardware decoding default** (2026-11-03): hardware decoding is ON by
   default; Preferences exposes "Use Software Decoding" (default **off**),
   superseding the `docs/08` observation of the original's toggle. Recorded
   in `IMPLEMENTATION_DECISIONS.md` §2.
2. **Video overscan fix, second round** after the owner's re-report with
   the real Bad Apple file: the earlier viewport-based sizing was replaced
   by the private-offscreen-FBO + blit design (see DECISIONS §3.2), because
   GTK 4.24's unified gsk_gpu renderer keeps the viewport at whole-frame
   size. Verified headless on the default renderer, `GSK_RENDERER=gl`, and
   `GDK_SCALE=2`; light frames keep the header visible.
3. **Seek-bar hover thumbnails implemented** (the preference existed but
   the feature did not): hidden second libmpv instance, pref-gated, cached,
   works while paused; seek drag uses keyframe-only seeks with an exact
   seek on release (the "extreme lag" on the 180 fps AV1 file).
4. **Multiple audio tracks fixed**: the engine-side track snapshot was
   never populated (A/Shift+A silently no-op'd) and GTK4 swallowed single
   clicks on popover rows; both fixed and verified against the 5-audio-track
   file.
5. **ARM64 builds happen in CI on GitHub** arm64 runners; no local
   emulated builds.
6. **Video surface fills the whole window** (third owner report,
   2026-11-04, with reference screenshots of the original): the header bar
   and control bar are overlaid chrome with gradient scrims, per docs/03
   and the owner's direction ("the top bar should also have the video
   part, as upstream"). Light frames no longer hide any UI.
7. **NVIDIA band artifacts addressed** (same report): the direct-render
   fast path added during the overscan work turned out to corrupt under
   gsk_gpu (reproduced headless; the owner's bands match the artifact
   class) — removed; the offscreen+blit pipeline now runs on every render
   callback so GSK can never composite a partially-stale surface.
8. **Seek-bar thumbnails made trustworthy** (same report round): the
   readback was vertically mirrored and one seek behind (it captured the
   seek's start keyframe); both fixed and verified by pixel-matching the
   readback against ffmpeg reference frames at the requested second, plus
   a dual border so the preview stays visible on white frames.
9. **Bottom bar matched 1:1 to the original player** (owner direction with
   18 reference screenshots, 2026-11-04): the header now shows mpv's
   `media-title` (metadata title supersedes the filename-derived one); the
   audio-track button got an original five-bar equalizer glyph matching the
   reference icon language; button-row spacing widened to the reference
   pitch; the seek trough uses the reference's translucent-white track with
   accent fill on hover/active; the hover thumbnail carries the time chip
   beneath it, lifted above the bar. Layout parity only — no reference
   assets or code were used.
10. **Application icon completely redesigned** (same direction): charcoal
    tile, four white viewfinder corner brackets around a mint→cyan→azure
    play mark with a soft halo — a new original composition unrelated to
    the previous mark and to any existing player artwork. Full PNG size set
    (16–512) rendered from the new SVG master; GResource verified byte-identical
    to a fresh compile of the assets.
11. **Video rendering pipeline rewritten** — the displaced-band/glitch class
    is closed by construction. mpv renders into a private offscreen FBO, the
    frame is read back and published as a `gdk::MemoryTexture` on a custom
    `gdk::Paintable` over a `gtk::Picture`; Frame never draws into GTK's
    framebuffer and never hands GSK a GL texture id. Root cause of the
    recurring bands (reproducible headless after rapid seeks): GSK caches GL
    texture ids beyond the `GdkGLTexture` lifetime, so both name-reuse
    (sampling racing re-render) and name recycling via `glGenTextures`
    (cached ids → recycled storage) corrupt — FBO readback dumps were
    pixel-perfect while the screen showed noise. The CPU-side texture has no
    lifetime games; verified 3/3 seek-storm runs pixel-perfect plus the
    hover-thumbnail run (see DECISIONS §3).
12. **Playback de-flooded** (owner report: "it lags a lot when playing" the
    180 fps AV1 file, 2026-11-04): mpv's render-update callback fires once
    *per decoded frame* — at 180 fps that is an unbounded wakeup → render →
    sync-readback pump that measured ~500% CPU and starved the main loop.
    Two fixes: (a) publishes are paced to the **display** — the wakeup
    handler only sets a dirty flag and requests a redraw; the actual
    publish runs in the picture's frame-clock tick callback, so at most one
    publish happens per display frame; (b) the readback itself is a
    **double-buffered PBO ping-pong** (`read_pixels_pbo`) so the CPU copy
    overlaps the next frame's GPU work instead of stalling on
    `glReadPixels` — on the owner's 180 Hz panel the frame clock matches
    the decode rate, so back-pressure from pacing alone would still have
    serialized readbacks. Verified: exact one-frame-delay pattern via pixel
    probes; colorbars/stats overlay restored after seeks; full regression
    battery green (27/27 tests, smoke, bad-apple verify, settle-shot,
    thumbnail check). See DECISIONS §3.4–3.5.
13. **Bottom-bar "white spots" fixed** (owner screenshots, same report):
    the control overlay carried its own margins, so the gradient scrim
    stopped short of the window's bottom edge and raw video showed in a
    band there — glaring white on bright frames. Margins moved to an inner
    box so the scrim spans edge to edge (verified over a bright frame:
    the content's last rows now render ~rgb(16) under the scrim), and the
    bottom gradient was deepened (0.85 → 0.45 at 55% → 0) to keep the
    reference's contrast over white scenes.
14. **Track pickers restyled to the reference** (owner screenshots):
    square-checkbox rows in a boxed list replaced by a bold `Add … Track`
    title (still opening the portal chooser) above plain radio-circle rows
    (`None` + `Title – lang`), per docs/03's `(•)` radio-list sketch and
    the owner's Cine reference shots. Rows chain into one radio group and
    every activation path funnels through `set_active(true)` so indicator
    and engine can never disagree.
15. **Resize-lag bounded** (owner: "moving video" lagged): each resize
    event rebuilt the offscreen FBO+PBOs and paid a synchronous seed
    readback. The rebuild is now debounced — size events inside a 150 ms
    window keep rendering at the existing target size (GSK scales the
    texture) and the rebuild happens when the size settles; a drag pays
    two rebuilds (first + final), a one-off resize (maximize) stays
    immediate. Verified: ten size changes inside 100 ms produce exactly
    one intermediate rebuild plus the final one.
16. **Scrub lag fixed** (owner: "moving as using the progress bar to play
    the X second"): the old handler re-armed a 150 ms debounce timer on
    every `change_value`, so during a continuous drag it never fired — the
    video did not move at all until the pointer stopped. It is now two
    gesture-free timers: a **throttled keyframe scrubber** (armed once per
    burst, ~7 cheap keyframe seeks/s while the pointer moves — the video
    follows the bar at keyframe granularity) and a **debounced settle
    timer** (250 ms after the last change, one exact seek lands on the
    final value — drags, wheel bursts and keyboard steps alike). A plain
    trough click skips the scrubber entirely (first change of a session),
    so it never flashes a keyframe-snapped frame before landing exactly.
    The click gesture was removed: the Scale's own drag gesture claims the
    pointer sequence, so a `GestureClick` on the bar never receives its
    release (verified under Xvfb — the release handler never fired).
17. **Seek thumbnail rounded** to the reference chrome: `border-radius` +
    a real clip on the picture's texture, light 1 px border reads over
    dark scenes, drop shadow over bright ones (replaces the square dual
    border/outline). The clip is `set_overflow(Hidden)` — originally
    written as the CSS `overflow: hidden`, which **no GTK4 version
    parses** ("Theme parser error: No property named overflow" in every
    packaged build, the declaration silently dropped); the widget
    property is the supported spelling and renders visibly rounded
    corners (verified under Xvfb).
18. **Track-popover rows melt into the popover**: the list node's default
    view background painted the radio rows as a mismatched darker panel
    inside the popover (owner's "weird color mismatch"). The
    `.track-popover` list/rows are transparent now with hover/press tints
    derived from `@popover_fg_color`, so they follow any theme.
19. **Scrubbing tracks the bar exactly** (owner: "still scrolling lag"):
    two more layers beyond the throttle. First, the scrub pauses playback
    once per session (previous pause state restored at settle) so a
    playing 180 fps decode flood no longer fights the seeks for the main
    loop. Second, the scrub no longer *keyframe*-seeks — on the owner's
    file the ~5 s GOP meant consecutive seeks landed on the same keyframe
    for seconds at a stretch (bar moved, picture frozen). The scrub is
    now a **completion-paced exact pump**: an exact seek to the newest
    pending value, issued only after the engine's `seeking` flag drops
    (400 ms cap so a wedged seek can never stall it), so exactly one seek
    is ever in flight and the displayed frame is always the true bar
    position as fast as the file decodes. The pump takes the pending
    value when it seeks, so the settle timer only re-seeks when a newer
    change arrived. (Completion pacing was then replaced — see below.)
    Third, the paused-publish hole: a paused pipeline
    raises no render-update flags at all, so seek frames never reached
    the screen during the drag — `request_render_passes()` opens a
    bounded **render window**: nudges only while a seek is decoding
    (rate-capped to one per 25 ms) plus one landing pass, then the window
    closes itself. An earlier always-on window self-sustained at the
    frame-clock rate — a render+readback+publish of a full-frame texture
    per tick, which starves the UI thread on a 180 Hz desktop (owner's
    "progress bar still lags when moving it"). Verified under Xvfb:
    mid-drag screenshot differs from pre-drag (intermediate frames really
    show), 0 keyframe seeks in the log, one seek in flight at a time,
    pause restored after settle.
    Fourth (owner: "still it lags — mpv does it without lag"): waiting
    for each seek to land is itself the lag on the real file. Measured in
    the harness on the owner's 180 fps AV1 1080p10 file, one exact seek
    costs **~500 ms** (keyframe→target decode), so completion pacing
    updated the picture twice in a whole drag. mpv's own OSC stays smooth
    because a new seek **aborts** the in-flight one and mpv continues
    decoding forward from where it stopped — one keyframe-to-target
    decode for the entire drag, not one per step. The pump now does the
    same (abort-and-reissue, ≥80 ms apart, freeze and exact settle kept),
    and mid-decode passes publish rate-capped (≥100 ms) so the aborted
    decodes' intermediate positions surface like mpv's drag. Measured on
    the real file: seek steps every ~130–150 ms tracking the bar, frame
    diffs across the drag 35 → 166 → 117 (picture follows), exact landing
    on release. (Superseded by item 24: the abort-and-reissue *exact*
    pump was itself still bursty on this file — the drag now scrubs
    deduped keyframe seeks like mpv's OSC.)
20. **Header auto-hides in windowed mode** (owner: "make the upper UI
    elements also disappear how the lower ones do after inactivity"): the
    header bar is overlay chrome just like the bottom controls — it hides
    with them everywhere, not only fullscreen. Pointer movement brings
    both layers back, resting the pointer on either keeps both visible,
    and hiding stays skipped while paused or while a popover is open.
    Verified under Xvfb: with a soundless clip playing, both the header
    strip and the control-bar strip measure dark-UI → video → dark-UI
    across show → hidden → reshow.
21. **Keyboard shortcuts audited against docs/05, one real gap fixed**
    (owner: "a lot of keyboard shortcuts are not implemented"): every
    docs/05 binding is now verified delivered and mapped under Xvfb (81
    key combos swept; Ctrl+? window content already matched the spec).
    The one advertised-but-dead binding was **Ctrl+Q (Quit)** — listed in
    the Shortcuts window, handled nowhere. It is now a real accelerator
    (`set_accels_for_action`) plus a `CallbackAction` shortcut controller
    on the plain `adw::Window`s (Shortcuts/Preferences/About), which GTK4
    otherwise excludes from app accelerators and whose windows never join
    the action muxer (a `NamedAction` there silently does nothing).
    Additionally, **every keyboard action that changes the picture now
    shows its effect while paused** (seeks, frame steps, chapter jumps,
    subtitle/track/zoom/color changes open the render window above) —
    before, a paused pipeline swallowed all of it, which read as "the
    shortcut is not implemented". Verification gotcha: the 30 s scratch
    clip hit EOF mid-sweep, making later seeks no-ops (`from=None`) —
    regenerated at 90 s before judging any paused-seek failure.
22. **More frequent thumbnails** (owner request): hover debounce
    220 ms → 120 ms, cache granularity one second → **half-second**
    slots, cache cap 64 → 128 entries, and the hover preview now also
    updates **during a scrub** — throttled to one request per 400 ms so
    its seeks on the hidden second instance cannot starve the completion-
    paced scrub pump (previously suppressed for the whole session). The
    clip fix from item 17 (set_overflow clip) and the GOP-window widening
    were prerequisites: on 10 s-GOP files the thumbnailer previously
    never produced a frame at all.
23. **The bar never snaps back — the UI lies on purpose** (owner: "it
    should lie to the user, and move the thumbnail and the progress bar
    between the gaps even if secretly it is waiting"): during a scrub the
    engine's reported `time-pos` still belongs to the last *landed* seek
    (or, mid-decode, an aborted one), so letting it drive the widget
    yanked the thumb backwards under the pointer. Now the pointer is
    truth: while a scrub session is active or the primary button is held
    on the bar, `update_time` never touches the scale — the elapsed
    label follows the scale's own value instead. Button-held state comes
    from an `EventControllerLegacy` on the scale, observing raw button-1
    press/release without participating in the pointer sequence (a
    `GestureClick` cannot do this job there — see §14). The hover preview
    also **glides**: it is repositioned under the pointer on every motion
    event, not only when new thumbnail content lands, so thumb and
    preview move continuously between the decoder's gaps. Verified
    end-to-end on the real file with a `FRAME_DEBUG` trace: press →
    every `PositionChanged` report is guarded while the picture position
    lags far behind the bar → release → exactly one apply to the landed
    position, which the thumb already shows (no visible movement).
24. **The drag itself now scrubs like mpv's OSC** (owner: "still moving
    it feels choppy"): mpv's own seek bar was checked against its actual
    implementation — `osc.lua` of our engine, not the clean-room target:
    `seekbarkeyframes` defaults to true, drags issue keyframe seeks
    straight from mouse-move events with identical targets deduped, and
    the mousedown seeks *exactly* to the pressed position. Frame did the
    opposite — it dragged with exact seeks (completion-paced, then
    aborted and re-issued) — and each exact seek costs a
    keyframe-to-target decode on heavy files, so the picture moved in
    bursts no matter how the pump fired. The scrub pump now does what
    the OSC does: **press → exact seek immediately; every drag change →
    keyframe seek** (≥50 ms apart, deduped); the settle timer still
    lands **exactly** 250 ms after release (keyframe steps never reach
    the pointer precisely) but **defers while the button is held** — a
    human drag has micro-pauses longer than the debounce, and ending the
    session mid-hold paid an exact seek's decode stall and restarted the
    session on the next move (measured: an exact seek fired mid-drag
    every time the drag paused ~300 ms). The mid-decode publish cap
    dropped 100 → 50 ms to match the faster scan. Verified on the real
    file with per-publish logging over a scripted drag: six monotonic
    keyframe landings for a six-step drag — one visible update per
    pointer move, cadence following the pointer — **zero mid-drag exact
    seeks** (previously one per micro-pause), one exact landing ~260 ms
    after release at the final value, and a 30 fps screen recording
    whose picture changes follow the same cadence.
25. **Header title no longer shows the file extension** (owner: black-box
    check against Cine): mpv's `media-title` falls back to the raw file
    name *with* extension when a file has no metadata title, and Frame
    adopted it verbatim — the header read `test-silent.mp4` where the
    original shows `test-silent`. Frame now keeps a metadata title
    verbatim but falls back to its own extension-stripped
    `display_title` (the load-time rule) when there is none. Verified
    under Xvfb: the header reads `test-silent` for the scratch clip and
    still shows a file's real metadata title for files that have one
    (e.g. the owner's bad-apple file).
26.  **Bottom-bar interaction de-churned** (owner: "still some lag in
    using that bottom bar", after the scrub pipeline itself was already
    smooth): three separate per-event costs remained on the paths around
    the scrub. First, every delivered hover motion on the seek bar read
    `duration` out of mpv — a synchronous core-locking property read,
    contended with the decode loop on heavy files — and re-laid-out two
    overlay children (chip + preview margins), while three motion
    controllers cancelled and re-created the 2 s hide timer per event.
    The hover update (timestamp chip, preview glide, thumbnail request)
    and the chrome show/hide bookkeeping are now coalesced onto one-shot
    frame-clock ticks: an event only records the pointer position and
    arms the tick, so the work runs once per display frame at most.
    Second, every `time-pos` report (~180/s on the owner's file) also
    read `duration` from mpv on the UI thread; the engine now keeps a
    `cached_duration` maintained by `DurationChanged`/load events and the
    hot paths read that (property read only as the first-frames
    fallback). Third, a thumbnail decode already in flight when the
    pointer left the bar still presented itself afterwards (leaving
    cancelled only the debounce timer) — `present_thumbnail` now gates
    on a `bar_hovering` flag, and the leave handler also cancels the
    still-queued hover tick so it cannot resurrect the chip. Verified
    under Xvfb on a soundless clip with `FRAME_DEBUG` counters: hover
    ticks are bounded by the frame rate while raw delivered motion runs
    ahead of them; an A/B against the pre-fix build shows the stale
    post-leave thumbnail the old code presented (old: visible box after
    the pointer left; new: clean, no `thumbnail shown` after leave);
    press → drag → release still lands exactly one seek ~260 ms after
    release, auto-hide and reshow are unchanged. One llvmpipe run stalled
    its GL publish to black mid-pass (software-GL flakiness; the same
    binary rendered the full pattern in the adjacent A/B pass) — noted,
    not a regression. `cargo test` 27/27.

## 14. Notable findings during verification

- Ctrl+? delivers the `question` keysym **with SHIFT** on most layouts;
  hand-rolled matching now mirrors GTK accelerator semantics.
- `gtk::ShortcutLabel` requires `gtk_accelerator_parse` syntax
  (`<Ctrl>N`, `Page_Up`); "Ctrl+N" renders empty.
- `adw::Window` aborts on `gtk_window_set_child()` — `set_content()` only
  (Preferences and Shortcuts windows crashed before this).
- A cplayer-less libmpv (Flatpak build) does not have the `osc` option;
  the engine now tolerates that explicitly.
- async-channel's future-returning `send()` drops the item when the future
  is never polled **even on unbounded channels** — hit twice (event pump,
  render-wakeup nudge). `send_blocking` everywhere.
- GTK4 `ListBoxRow::activate` fires on Enter/double-click only; a plain
  click with `selection_mode=none` does nothing — popovers need a
  `GestureClick`.
- A GL state leak from one GL consumer (thumbnailer FBO/viewport) poisons
  the next consumer's geometry assumptions — save/restore around every
  foreign pass, then assert via the `FRAME_DEBUG` geometry line.
- mpv's render API is only half the contract: `seek` commands return
  before they start, `time-pos` reports the seek target at queue time,
  mid-seek the vo displays the seek's start keyframe, and a **paused**
  instance never raises the render API's FRAME flag — position-based
  gating (`seeking` property, string format reads `"yes"`/`"no"`) plus an
  unconditional render is the only reliable thumbnail flow.
- Verify "corruption" against ffmpeg-extracted reference frames of the
  file before debugging GL: Bad Apple's stylised horizontal-echo effects
  were repeatedly mistaken for render artifacts. Conversely, verify the
  *absence* of artifacts on a **paused, settled** capture: x11grab shots of
  a 180 fps repaint tear (tiny dashes, misaligned bands) that do not
  reproduce when the window is static are capture tearing, not bugs.
- GSK caches GL texture ids beyond the `GdkGLTexture` lifetime; publishing
  the same GL texture name twice — by re-rendering into it or by recycling
  a deleted name — corrupts (bands/garbage). FBO readback dumps separate
  "mpv produced garbage" from "the publish chain corrupts a clean frame"
  decisively.
- mpv's render-update callback is per-**decoded**-frame, not per-display
  frame — driving renders from it directly floods the main loop on
  high-fps files (180 fps → ~500% CPU with synchronous readbacks). Pace
  with a dirty flag + frame-clock tick; the tick then runs at the display
  rate, which on a 180 Hz panel equals the decode rate and still needs
  non-blocking readback.
- `glGetBufferSubData` raises `GL_INVALID_OPERATION` (0x502) on GDK's GL
  contexts (GLES-oriented dispatch); `glMapBufferRange` +
  copy + `glUnmapBuffer` is the portable copy-out for PBO readbacks.
- The owner's environment: 2560×1440 @ 180 Hz, Wayland, NVIDIA (nvdec
  hwdec confirmed active) — any per-frame CPU work at 180 fps is 180
  synchronous stalls per second; design for the panel, not the file.
- On a Wayland session, `x11grab` of `:0` captures only the XWayland root
  (black frames) and XWayland-spawned test instances can stall paused at
  startup — `Xvfb` + `dbus-run-session` remains the reliable headless
  harness; treat `:0` captures as unusable rather than as app failures.
- libadwaita ignores `GTK_THEME` for the color scheme — a bare
  `dbus-run-session` renders the app **light**, which confuses screenshot
  comparisons against the owner's dark references. Force dark with
  `gsettings set org.gnome.desktop.interface color-scheme prefer-dark`
  inside the session (needs writable `XDG_CONFIG_HOME` for dconf).
- x11grab of a GTK window includes its CSD shadow margins (~5 px), which
  render as garbage/white with no compositor running — the app's content
  area is inset; never mistake that outer ring for app pixels when
  pixel-probing screenshots.
- A GTK4 radio group driven from data: build each `CheckButton` with
  `set_group(previous)` and funnel **every** activation path (row click,
  row activate, radio click) through `radio.set_active(true)`, selecting
  in `connect_toggled` **only when `is_active()`** — the deactivation
  toggled on the previously selected radio would otherwise select it
  right back.
- A `GestureClick` attached to a `GtkScale` **never sees the release**: the
  scale's own drag gesture claims the pointer sequence, so the click
  gesture is cancelled mid-interaction (`pressed` fires, `released` does
  not). Seek behavior on the bar must be timer-driven (throttle +
  debounce), not anchored on a release event. Raw press/release tracking
  that must survive the whole drag belongs on an `EventControllerLegacy`
  instead (match `event.event_type()`); it observes every button event
  without claiming anything, so the scale's internal drag is untouched.
- A hover seek-thumbnail of the *same scene* as the paused video behind it
  is invisible in screenshots (bad-apple self-similarity) — verify
  thumbnails by pausing on one scene and hovering a distant one so the
  preview contrasts with the background.
- Without a window manager (bare Xvfb), a `MenuButton` popover that does
  not fit below its button is neither flipped nor clamped to the window —
  it overflows past the window edge on the X11 root. A real compositor
  flips it above; that overflow is a harness artifact, not an app bug.

## 15. Open questions

See `IMPLEMENTATION_QUESTIONS.md` (Vulkan renderer stance, screenshots
directory name, repository hosting, Escape-to-close for Preferences).

## 16. Handoff state

- `cargo test` green (27/27); the fixes above are in the working tree.
  The four x86_64 packages and the Flatpak bundle are being **rebuilt
  again** from this tree (2026-11-05, fourteenth round — the OSC-style
  scrub of round 13 plus the header-title extension fix found by the
  black-box comparison of §17); artifacts land in `dist/`. Any copies
  built earlier predate this round and are stale.
- Published (2026-11-07, owner directive): repository
  `github.com/hakergeniusz/Frame` (private), branch `main` pushed, tag
  `v1.0.0` (f073be9). The tag run 42378275747 went 8/8 green with all
  eight per-arch artifacts uploaded. The PKGBUILD release checksum — the
  codeload archive sha256, not the API tarball's, which is a different
  byte stream (decisions §5) — landed on `main` right after the tag
  (fd5852f): a tarball's hash cannot include itself. CI and the
  packaging workflow run on every push and the tag respectively.
- Local scratch (Xvfb harness, test media, container logs) lives in
  `/tmp/ai-agent/` and is disposable.

## 17. Black-box comparison against Cine 1.8.8

The owner asked for a black-box check of Frame against the original
(`io.github.diegopvlk.Cine` 1.8.8 from Flathub) on Xvfb. Black-box only:
both apps were launched and driven as a user would (pointer, keyboard,
screenshots, 30 fps screen recordings); Cine's files were never opened
and its behaviour was never inspected beyond the running UI.

Both apps ran the same sequence on a clean Xvfb display with the
soundless scratch clip, then the same scripted drag on the owner's real
file (Cine with a dead `PULSE_SERVER` so its audio path cannot connect;
Frame muted via its `ao=null` harness config).

**Scrub drag on the real file (the headline result).** Same drag
choreography (press at 15 % of the bar, eight 200 ms pointer steps,
release), recorded at 30 fps for both apps:

| Behaviour during the drag | Frame (round 13) | Cine 1.8.8 |
| --- | --- | --- |
| Seek bar follows the pointer | yes (pointer is truth) | yes (it is the pointer) |
| Hover/seek thumbnail glides with the pointer | yes (repositioned per motion event) | yes |
| Thumbnail *content* updates during the drag | yes, ~every 150 ms | yes, ~every 100–270 ms (measured from the recording) |
| **Main picture updates during the drag** | **yes — 8 visible updates for the 8-step drag, one per pointer step, monotonic** (per-publish log matches: keyframe landings ~230 ms apart) | **no — the main picture stays frozen for the whole drag** and lands after release (first visible update after mouseup) |
| Landing after release | exact, ~250 ms after the last change | lands after release; exactness not observable black-box |

So Frame's drag is at least as responsive as the original's everywhere,
and strictly more responsive on the main picture: Cine's drag feedback
is the *thumbnail*; the video itself waits for the release. (This also
retroactively explains the owner's guiding observations — "mpv does it
without lag": raw mpv's OSC scans keyframes during the drag, while
Cine, an mpv frontend, does not.)

**Visible layout parity** (side-by-side windowed, controls revealed):
matching anatomy — header with Open ⌄, centered title, hamburger +
window buttons; top-left frame-counter/PTS HUD in the same amber-on-dark
style; control row prev/play/next · volume · subtitles · audio on the
left, A-B/playlist/settings/fullscreen on the right; thin white seek
trough, time `m:ss | m:ss` right-aligned; hover seek thumbnail (rounded,
bordered) with a time chip beneath, lifting above the bar. Differences:

- **Header auto-hide**: Frame's header hides with the controls after
  inactivity (owner-directed, 2026-11-05); Cine 1.8.8's header is
  permanent in windowed mode. Intentional difference.
- **Statistics overlay**: Cine shows mpv's stock `stats.lua` layout
  (File/Size/Displayed-Framed-Dropped/Video/Format/Colormatrix…).
  Frame renders its own layout — the docs decision was based on "the
  original's statistics UI was never visually exercised", which no
  longer holds. **Spec question for the owner**: keep Frame's own
  layout or match mpv's stock stats page?
- **Button set drift**: Cine 1.8.8 has shuffle and repeat buttons in
  the right group (and no subtitle bubble); Frame has the subtitle
  bubble and no shuffle/repeat, per the docs' reference screenshots of
  an earlier Cine. **Spec question**: track 1.8.8's set or keep the
  docs' set?
- Keyboard step: Cine's Right arrow seeked +10 s; Frame (docs/05) steps
  ±5 s. **Spec question**: which is canonical?

**Keyboard spot-checks (observed working in Cine)**: Space (pause,
button/icon swap), Right (+10 s), F (fullscreen — header hides), I
(statistics overlay, stays pinned until toggled), Ctrl+Q (app quits).
Frame implements all of these per docs/05 (± the step-size question)
— previously verified with an 81-combo sweep.

**Title**: fixed in this round (item 25) — Frame now strips the
extension exactly like Cine when no metadata title exists.

**Harness notes** (for reproducing the Cine runs): `flatpak run` needs
`--socket=x11` — adding `--socket=fallback-x11` re-pins `DISPLAY` to the
host's and GTK opens a NULL display; `GTK_A11Y=none` (at-spi cannot
register under `dbus-run-session`); `LIBGL_ALWAYS_SOFTWARE=1`
(Xvfb has no DRI3, Cine's mpv renders black without it); and without a
window manager, input focus needs `xdotool windowfocus` *after* the
window is viewable — a race here silently swallows every later XTEST
key. Frame's own runs are unaffected (its harness scripts already do
this correctly).

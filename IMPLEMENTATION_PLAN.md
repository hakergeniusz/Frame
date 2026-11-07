# IMPLEMENTATION_PLAN.md — Frame

Independent clean-room implementation of the approved functional specs in
`docs/`. This file is the requirements checklist and roadmap. Every
requirement has an ID used for traceability in tests and in the final
implementation report.

## 1. Technology selection (independent, from spec capabilities only)

| Layer | Choice | Reason |
| --- | --- | --- |
| Language | Rust (stable) | mandated by the brief |
| GUI toolkit | GTK 4 + libadwaita | docs 01/06 describe GNOME/libadwaita conventions (CSD header bars, pill buttons, Adw about/preferences/shortcuts windows, toasts) |
| Playback engine | libmpv via hand-written FFI (no wrapper crate) | the required capability set (frame stepping, A-B loop, speed, secondary subtitle tracks, subtitle delay/position/scale/color/font/background, audio delay, zoom, contrast/brightness/gamma/saturation, normalize volume, hwdec toggle, URL streaming with duration probing) maps 1:1 onto libmpv properties/commands; avoiding a wrapper crate keeps deps minimal and auditable |
| Video output | mpv render API (OpenGL) into a `GtkGLArea` | windowing-agnostic, works under Wayland/X11, honors the render pipeline prefs |
| Config/persistence | JSON files under `XDG_CONFIG_HOME/Frame/` | docs 01/06: Preferences exposes "Open Config Folder", i.e. user-visible file-based config; avoids GSettings schema packaging on 4 distros |
| Proc-address loader | `epoxy_get_proc_address` (already linked by GDK) | no extra dependency |

Feature level: compiled against GTK `v4_14` / libadwaita `v1_6` so runtime
requirements stay satisfiable on Debian stable.

## 2. Module layout

```
src/
  main.rs            entry point
  app.rs             GtkApplication, actions, single-instance, CLI/open routing
  config.rs          Settings + JSON persistence
  history.rs         Watch History store (persisted, date-grouped)
  resume.rs          per-video resume store + saved sessions
  engine/
    ffi.rs           libmpv C API declarations (hand-written)
    mpv.rs           MpvPlayer: engine thread, property observations, commands
  player/
    mod.rs           PlayerWindow + wiring
    control_bar.rs   bottom control overlay + seek bar
    popovers.rs      volume/subtitles/audio/chapters/playlist/settings popovers
    playlist.rs      per-window playlist model
    stats.rs         statistics page / pinned overlay
  ui/
    welcome.rs       WelcomeWindow
    preferences.rs   PreferencesWindow (6 groups + footer)
    about.rs, shortcuts.rs, url_dialog.rs, toasts.rs, file_dialogs.rs
assets/              original SVG icon + PNG sizes + desktop file
packaging/           debian/, fedora/, arch/, flatpak/
tests/               integration tests (headless)
.github/workflows/   build/release CI for amd64 + arm64
```

## 3. Requirements checklist

Statuses: ☐ todo · ◐ partial · ☑ done · ✗ not implemented (documented).

### Welcome window (docs/02, docs/01)

| ID | Requirement |
| --- | --- |
| W1 | Centered stack: app icon, bold app name, subtitle "Drag and drop files here" |
| W2 | `Open…` filled pill → file chooser (multiple video files), Ctrl+O |
| W3 | `Open Folder` tonal pill (folder chooser), Ctrl+I |
| W4 | `Open URL` tonal pill → URL dialog, Ctrl+U |
| W5 | Circular history button right of `Open…` with numeric badge |
| W6 | Header bar: ☰ menu left; min/max/close right |
| W7 | Primary menu: New Window, Preferences, Keyboard Shortcuts, About, ─, Save Session, Save Session and Close |
| W8 | Watch History popover: red clear-all icon, title, ✕; date-group headers (locale format); newest first per group; row = name + ⊗ remove; click row re-opens; persists across restarts |
| W9 | Whole window is a file drop target |
| W10 | Default size ≈ 1070×600; content centers on resize |

### Player window (docs/03, docs/06)

| ID | Requirement |
| --- | --- |
| P1 | Header `Open ⌄` menu: Open Files/Folder/URL, Add Files/Folder/URL, ─, Add Subtitle Track, Add Audio Track, ─, Watch History (with shortcuts) |
| P2 | Header title = media name; window title matches |
| P3 | ☰ primary menu identical to welcome window |
| P4 | Video surface fills window; black letterbox |
| P5 | Controls auto-hide during playback; reappear on pointer movement; stay visible while pointer rests on control bar |
| P6 | Video click mapping per Primary Click / Secondary Click preferences |
| P7 | Left control group: ⏮ ▶/⏸ ⏭ (prev/next disabled with 1-item playlist), volume, CC (subtitles popover), audio mixer |
| P8 | Right control group: shuffle (only ≥2 items), A-B loop, chapters, playlist, settings, fullscreen |
| P9 | Next at end of playlist: no wraparound (observed); robust on error states (no crash — docs/06) |
| P10 | Seek bar: full-width, round handle, `current | duration` m:ss, hover timestamp bubble, optional thumbnail (pref, OFF default) |
| P11 | Subtitles popover: Add Subtitle Track pill + radio track list with `None` |
| P12 | Audio popover with same shape (+ Add Audio Track) |
| P13 | Chapters menu (always available; empty state when media has none) |
| P14 | Playlist panel: queued items, current highlighted, add/remove |
| P15 | Settings popover: playback speed + loop controls |
| P16 | Toasts bottom-center: dark pill, bold text, ✕ dismiss, auto-dismiss (e.g. "File Error: no audio or video data played") |
| P17 | Normal/maximized/fullscreen supported; Escape exits fullscreen |

### Dialogs (docs/04)

| ID | Requirement |
| --- | --- |
| D1 | Preferences: single scrollable page ≈520 px, ⓘ header icon, groups Behavior / Performance / Session & resume / Subtitles / Mouse behavior / Language preferences + full-width `Open Config Folder >` footer; all rows/types/defaults per doc 04 |
| D2 | Open URL / Add URL dialogs: floating-label entry, action button disabled while empty, Open = new window (pref) / Add = append + play |
| D3 | Keyboard Shortcuts window: search field, category chips (General, Subtitles, Audio & Volume, Navigation, Display & Video, Playback, Miscellaneous), full shortcut list per doc 05 |
| D4 | About window: icon, name Frame, developer, version chip; Details / Report an Issue / Legal / Acknowledgements subpages |

### Keyboard shortcuts (docs/05)

| ID | Requirement |
| --- | --- |
| S1 | General: Ctrl+N/O/I/U, Ctrl+Shift+O/I/U, Ctrl+S/A/C/H, Ctrl+?, Q/Ctrl+W, Ctrl+Shift+S, Shift+Q, Ctrl+Q |
| S2 | Subtitles: C, Z/Shift+Z, Alt+Z/Ctrl+Z (secondary track), `,`/`.` (delay), PageUp/PageDown (position), Shift+G/Shift+F (scale) |
| S3 | Audio: ↑/↓ volume, A/Shift+A track, M mute, Ctrl+−/Ctrl+= audio delay |
| S4 | Navigation: J/L ±10 s, ←/→ ±5 s, Ctrl+[ / Ctrl+] frame step, Ctrl+←/Ctrl+→ chapters |
| S5 | Display & Video: F11/F fullscreen, Escape exit, =/+/− zoom, 1–8 contrast/brightness/gamma/saturation |
| S6 | Playback: K/P/Space play-pause, Ctrl+L A-B loop, Shift+L loop file, `[`/`]`/Backspace speed |
| S7 | Misc: S / Shift+S screenshots, I / Shift+I statistics |
| S8 | Shortcuts must not fire while a text entry has focus |

### Behaviors & lifecycle (docs/01, docs/06)

| ID | Requirement |
| --- | --- |
| B1 | Single instance; later launches (with or without args) are routed to the running instance |
| B2 | Pref "Open New Window for New Files" (ON): each open creates a new player window |
| B3 | Chooser runs attached to the (already created) window — cancelling leaves an empty player window (observed; docs/06) |
| B4 | Closing the last window exits the app |
| B5 | Ctrl+N creates an additional welcome window |
| B6 | URL open: modal dialog; metadata (title/duration) probed without full download; display title from URL tail with percent-escapes dropped (observed quirk: `Sample%20B.mp4` → `SampleB`); history stores raw URL |
| B7 | Files/folders chosen via the desktop file chooser (GTK FileDialog ⇒ portal under Flatpak) |
| B8 | CLI file args: routed to instance; inaccessible files surface a visible error toast (doc 06 suggestion) instead of silent retries |
| B9 | Clipboard text read asynchronously at startup; a plausible media URL is offered for opening (toast with Open action) |
| B10 | Screensaver inhibited while playing (portal Inhibit under Flatpak, org.freedesktop.ScreenSaver otherwise) |
| B11 | Persistence: watch history, all preferences, saved sessions, per-video resume (position + picture adjustments + subtitle delay) |
| B12 | Adding to playlist focuses/plays the newly added item (observed) |
| B13 | Playback failure → error toast; window/app keep working |
| B14 | Drag-and-drop: video files open (new window per pref); folder → folder playlist; subtitle/audio file → external track on current player; text URL → open |
| B15 | Open Folder: folder's video files (sorted, non-recursive) become the playlist |
| B16 | Dark theme default; follows system color scheme |
| B17 | Playlist rows offer remove (unobserved in original; usability minimum — recorded as assumption) |
| B18 | History clear-all: immediate clear + toast with Undo (unobserved; recorded as assumption) |

### Playback engine capabilities (docs/01 notes, docs/06)

| ID | Requirement |
| --- | --- |
| E1 | Frame stepping (Ctrl+[ / Ctrl+]) |
| E2 | Chapter navigation + chapter list |
| E3 | A-B loop set/clear (Ctrl+L, control-bar button) |
| E4 | Loop file (Shift+L) |
| E5 | Playback speed decrease/increase/reset |
| E6 | Multiple audio & subtitle tracks; secondary subtitle track (two simultaneous) |
| E7 | External subtitle/audio track loading |
| E8 | Subtitle delay, position, scale, color, font, background (prefs wired to engine) |
| E9 | Audio delay |
| E10 | Volume, mute, loudness normalization (pref) |
| E11 | Video zoom; contrast/brightness/gamma/saturation adjustments |
| E12 | Screenshots with/without subtitles → `~/Pictures/Frame/` (destination unobserved; assumption) |
| E13 | Statistics page + pinned statistics overlay (own implementation from engine properties) |
| E14 | URL streaming with duration probing before playback |
| E15 | Hardware decoding toggle (pref ON → `hwdec=auto`) |
| E16 | Graphics Offload toggle (mapped to GTK `GtkGraphicsOffload` wrapper; row copy preserved) |

### Packaging & release (brief §9–§11)

| ID | Requirement |
| --- | --- |
| PK1 | Debian `.deb` for amd64 + arm64 (metadata, deps, desktop entry, icons, copyright) |
| PK2 | Fedora `.rpm` for x86_64 + aarch64 |
| PK3 | Arch `PKGBUILD` for x86_64 + aarch64 |
| PK4 | Flatpak manifest `io.github.hakergeniusz.Frame`, least privilege, amd64 + aarch64; sandbox test |
| PK5 | Original icon: SVG master + 16/24/32/48/64/128/256/512 PNGs, installed under app ID; desktop entry references it |
| PK6 | CI workflows building amd64 + arm64 artifacts |
| PK7 | Version consistent across Cargo/deb/rpm/PKGBUILD/flatpak/about |
| PK8 | Dependency licensing audit: Frame app code MIT; mpv engine licence separate (LGPL-2.1-or-later with verified LGPL build / GPL-2.0-or-later for default builds) — never described as app-wide MIT; FFmpeg + transitive multimedia deps audited separately; licence texts + notices shipped |
| PK9 | Release artifact naming: `frame_<v>_amd64.deb`, `frame-<v>-1.x86_64.rpm`, `frame-<v>-x86_64.pkg.tar.zst`, flatpak bundle |

### Testing (brief §12)

| ID | Requirement |
| --- | --- |
| T1 | Unit tests: title derivation, playlist transitions, history grouping/persistence, settings roundtrip, resume store |
| T2 | Integration: headless GUI smoke test under Xvfb (launch, welcome window opens, URL open → player window, shortcuts window, prefs window) |
| T3 | Flatpak sandbox test (Xvfb) |
| T4 | Package install/remove verification (Debian/Fedora containers, Arch native) |
| T5 | Requirements coverage recorded in final report; unverified targets marked unverified |

## 4. Open specification questions (non-blocking; see IMPLEMENTATION_QUESTIONS.md)

All discovered ambiguities are non-blocking; each has a chosen default
recorded in IMPLEMENTATION_DECISIONS.md:

1. History badge meaning (docs/02, docs/06) — shown as entry count for today.
2. Clear-all semantics — immediate + Undo toast.
3. Add-to-playlist focus semantics — add jumps to new item.
4. Next/Prev wraparound — none.
5. A-B loop press cycle — A → B → clear.
6. Speed step factor — ×/÷ 1.1, clamp 0.1–10.
7. Volume step — 5 units.
8. Color-adjustment step — 5 units (−100..100).
9. Zoom step — 0.1 (×0.9/×1.1 equivalent on `video-zoom`).
10. Statistics contents — own overlay built from engine properties.
11. Context menu (Secondary Click option) — minimal sensible menu.
12. Screenshot destination — `~/Pictures/Frame/`.
13. Playlist remove/reorder UI — remove only.
14. Second argument-less launch — present most recent window (new welcome if none).
15. Multiple CLI files — one playlist in one new window (per pref).

## 5. Roadmap

1. ✅ Phase 1 — Requirements (this file).
2. Phase 2 — Cargo scaffold, GTK4/adw build, app identity, icon.
3. Phase 3 — Engine wrapper + playback core, playlist, toasts.
4. Phase 4 — Welcome/player UI, all popovers, dialogs, shortcuts.
5. Phase 5 — Packaging (deb/rpm/PKGBUILD/flatpak) + CI.
6. Phase 6 — Settings, history, sessions/resume, clipboard, screensaver, DnD.
7. Phase 7 — Verification: unit + headless GUI tests, container package tests, flatpak sandbox test.
8. Phase 8 — Final review, licensing audit, implementation report.

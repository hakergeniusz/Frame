# 06 — Behaviors & Edge Cases

Everything below was directly observed unless marked *(inferred)*.

## Opening & windows

- **File arguments.** `flatpak run io.github.diegopvlk.Cine /path/file.mp4`
  reaches the running/starting instance. If the path is not accessible
  inside the sandbox the app shows the welcome window and emits repeated
  “file error” retries in the background — it does **not** show an error
  dialog. *(A sandboxed build should decide whether raw CLI paths should be
  rejected with a visible message instead.)*
- **One window per media (default).** With “Open New Window for New Files”
  ON, every successful open created an additional window (observed with two
  different URLs → two player windows stacked).
- **Failed file-chooser attempts leave windows behind.** In the test
  environment the portal chooser failed on every attempt (“dismissed”);
  each attempt still left a new, empty player window on screen. Plausible
  reading *(inferred)*: with the pref ON the new window is created first and
  the chooser is attached to it, so cancellation leaves a blank player
  window. On a normal desktop cancelling would behave the same way — verify
  against the original.
- Closing the last window quits the app.

## Playlist dynamics

Observed sequence with one player window:

1. Opened `Sample A` by URL → player window, title `Sample A`, duration
   `0:20`, Next/Prev disabled/dimmed, no shuffle button.
2. `Add URL` with `Sample B` → title switched to `Sample B`, duration
   `0:15`, **shuffle button appeared** in the control bar, Next/Prev became
   active.

- So: adding to the playlist **focuses/starts the newly added item** (or the
  stalled first item errored and auto-advanced — an error toast for the
  previous item was on screen at that moment; both readings are possible,
  *(flag: verify exact add-vs-advance semantics)*).
- `Next` at the last item of a 2-item playlist did not change the title
  (no wraparound) — single observation on a stalled engine.
- Clicking `Previous` in this state **crashed the app** (SIGABRT, v1.8.8,
  stalled-engine condition). Almost certainly an environment-dependent bug,
  but a rewrite should make prev/next robust on error states.
- Shuffle only appears with ≥ 2 playlist items (observed appearing exactly
  at that point).

## Error handling & feedback

- Playback failures surface as a **bottom-center toast**:
  “File Error: no audio or video data played” with an ✕ dismiss and
  auto-dismiss timeout.
- The header/controls continue to work after a media error; the app does not
  close the window or quit.
- No visible error is shown for inaccessible files passed on the command
  line (silent welcome screen + background retries).

## Session & resume (from UI copy; not exercised)

- Menu → **Save Session** (Ctrl+Shift+S) / **Save Session and Close**
  (Shift+Q) persist the playback state.
- Preference **Restore Saved Session**: “Pick up where you left off after
  saving from the menu.”
- Preference **Save Video Position on Close**: “Also save options like
  brightness, subtitle delay, etc.” ⇒ per-video resume includes position
  *and* the picture adjustments + subtitle delay active at close time.
- These features were deliberately not exercised to avoid overwriting the
  user’s persisted data; exact file format/location unknown (the
  Preferences footer exposes an “Open Config Folder” shortcut).

## Watch history

- Entries persist across launches and are grouped by date headers
  (locale-formatted day, e.g. `11/02/2026`), newest first within a group.
- Rows show the media name — the **full URL** for URL sources, file name
  without extension for local files — plus a per-row remove (⊗); the popover
  header has a red-toned clear-all icon. *Clear-all interaction not
  exercised (data safety).*
- **URL opens are recorded even when playback fails** (observed directly:
  all four test URL opens appeared in history despite erroring; they were
  removed again via the per-row ⊗ buttons, restoring the pre-test state).
- The welcome history button carries a small numeric badge; its meaning is
  unresolved (showed `0` while one history entry existed).

## Engine-facing capabilities implied by the UI

The shortcut list and preferences require the playback layer to support:

- Frame-accurate stepping (Ctrl+[ / Ctrl+]), chapter navigation (Ctrl+←/→).
- A-B loop points set/clear (Ctrl+L), loop-file (Shift+L).
- Variable playback speed with reset ([, ], Backspace).
- Multiple audio & subtitle tracks incl. **secondary subtitle track**
  (Alt+Z / Ctrl+Z), external subtitle/audio track loading (menus “Add
  Subtitle/Audio Track”).
- Subtitle delay (, .), position (PageUp/PageDown), scale (Shift+F/G),
  color/font/background styling (Preferences).
- Audio delay (Ctrl+− / Ctrl+=), volume normalize (pref), mute.
- Video zoom (=, +, −) and color adjustments (1–8 keys).
- Screenshots with/without subtitles (S / Shift+S) — destination UI not
  observed.
- Statistics page + persistent overlay (I / Shift+I) — mpv-style stats.
- URL streaming with duration probing before playback (observed: duration
  shown at 0:00 for an HTTP source).
- Hardware decoding toggle, graphics offload toggle (DMABUF), volume
  normalization.

## Startup integration

- Clipboard text is read asynchronously at every startup *(seen in the
  app's log output, not the GUI; purpose
  inferred: detect a copied media URL and offer to open it; no banner was
  observed because the sandbox clipboard had no text)*.
- Screensaver is inhibited via the desktop portal when playback is
  attempted (portal inhibit call in the app's log output exactly when Play
  was pressed).
- The app runs fine as a sandboxed flatpak with no home/filesystem access:
  media enters via portal chooser, drag-and-drop, or URL; configuration is
  reachable by the user through “Open Config Folder”.

## Visual/design notes for the rewrite

- Dark theme by default; follows system color scheme.
- All chrome is CSD (no server decorations): header bars with circular
  buttons; menus and popovers use GNOME styling; buttons are pill-shaped.
- The welcome screen is a plain centered stack — no library grid, no
  thumbnails: Cine’s “library” is the Watch History popover only.
- Toast style: dark pill, bold message text, trailing ✕.
- The shortcuts window uses chips for navigation and grouped list rows with
  keycap badges.

## Explicitly not observed (open items for verification)

The fuller inventory of everything these documents do not cover — unseen
popovers, features never exercised, system integration, deliberate
clean-room boundaries — lives in [07](07-not-included.md).

1. Actual rendered video frames & window aspect handling (letterboxing was
   implied by the fixed-height control overlay but never seen with content).
2. Portal file chooser contents (native dialog; nothing app-specific to
   reimplement beyond filters — the filter list was not visible).
3. Playlist panel contents (button inert under stalled engine).
4. Settings-gear and volume popover contents (same).
5. Audio-menu and chapters-menu contents (Ctrl+A / Ctrl+C — not exercised).
6. Fullscreen transitions, drag-and-drop payload handling, screenshot
   destination, statistics overlay appearance.
7. Meaning of the history-button badge; clear-all confirmation semantics.
8. Exact add-to-playlist focus semantics (add-jumps vs error-auto-advance).
9. Shortcuts-window sections **Playback** and **Miscellaneous** were
   transcribed from the first observation session but the final scroll
   pages were lost to an environment crash before re-capture; their content
   in [05](05-keyboard-shortcuts.md) is from the verbatim first read.

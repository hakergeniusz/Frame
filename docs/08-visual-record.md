# 08 — Visual Record (replaces the removed screenshots)

Twenty-five screenshots were captured during the observation sessions as
evidence. They have been **removed from the repository**: as images of the
original app's UI they are derivative assets, which conflicts with the
clean-wall rule in the [README](README.md) (no copying of the original's
artwork). This page preserves what each of them showed, as text. Together
with the ASCII layouts in [02](02-welcome-window.md) and
[03](03-player-window.md) and the verbatim shortcut transcription in
[05](05-keyboard-shortcuts.md), it is the canonical visual record.

All frames were 1600×1000 captures from the isolated Xvfb session; the app
window occupied the ~1070×600 area at the top-left. No window manager ran,
so windows had no server-side decorations and stacked at (0,0).

## Welcome window & history

- **Welcome window** — Centered stack: rounded-square app icon (film-strip
  frame with play triangle, teal-to-green gradient border), bold `Cine`
  title, “Drag and drop files here” subtitle. Filled-pill `Open…` with the
  circular 🕐 history button at its right; tonal pills `Open Folder` and
  `Open URL` stacked below. Header bar: ☰ left; minimize/maximize/close
  right.
- **Primary menu open** — Dropdown under ☰: `New Window`, `Preferences`,
  `Keyboard Shortcuts`, `About Cine`, separator, `Save Session`,
  `Save Session and Close`.
- **Watch History popover (before cleanup)** — Red-toned clear-all icon at
  top-left, `Watch History` title, ✕ at right; a locale date-group header
  (`11/02/2026`); entry rows in rounded boxes with per-row ⊗ remove
  buttons. Captured while the four test URL entries were still present.
- **Watch History popover (after cleanup)** — Same popover back to the
  single pre-existing entry (`bad_apple_4in1_subtitled`), confirming the
  user's data was restored after testing.

## Dialogs

- **About window** — Adw-style about: app icon, `Cine`, developer
  `Diego Poviuk`, version chip `1.8.8`; rows `Details ›`,
  `Report an Issue ↗`, `Legal ›`, `Acknowledgements ›`.
- **Open URL dialog** — Small modal: ✕ top-right, single-line entry with
  floating `URL` label, `Open` pill button (disabled while empty).
- **Add URL dialog, empty entry** — Same layout with `Add` visibly
  disabled (greyed) while the entry is empty.

## Preferences (single scrollable page, ≈520 px, ⓘ header icon)

- **Top of page** — Group *Behavior*: `Open New Window for New Files` ON,
  `Seek Bar Thumbnail` OFF, `Normalize Volume` OFF. Group *Performance*:
  `Graphics Offload` OFF, `Hardware Acceleration` ON. Then *Session* rows
  (`Restore Saved Session`, `Save Video Position on Close`, both OFF) and
  the start of *Subtitles* (`Subtitle Color` white swatch with reset ↩,
  `Subtitle Font` = `Adwaita Sans SemiBold`).
- **Middle of page** — End of *Subtitles* (`Subtitle Scale` `1.00` steppers,
  `Subtitle Background` toggle with checkerboard preview) and group *Mouse
  behavior*: `Primary Click` = `Bypass`, `Secondary Click` = `Play/Pause`.
- **Bottom of page** — Group *Language preferences*: `Preferred Subtitle
  Languages` and `Preferred Audio Languages` inline editors (both empty),
  then the full-width footer button `Open Config Folder  >`.
- **Primary Click dropdown open** — `Play/Pause`, `Focus/Play/Pause`,
  `Bypass` ✓.
- **Secondary Click dropdown open** — `Play/Pause` ✓, `Context Menu`,
  `Bypass`.

## Player window

- **Single-item playlist** — Header `Open ⌄ | SampleA | ☰ ▁ □ ✕` (note the
  collapsed space in the URL-derived title); black video surface; control
  row: ⏮ ▶ ⏭ (prev/next dimmed) · 🔊 · crossed-out captions · 🎚 ···
  `ab`-arc, callout bubble, ☰, ⚙, ⤢ — five right-group buttons, no shuffle.
  Seek bar at `0:00 | 0:20` with round handle.
- **Open menu open** — Dropdown: `Open Files` `Open Folder` `Open URL`,
  `Add Files` `Add Folder` `Add URL`, separator, `Add Subtitle Track`,
  `Add Audio Track`, separator, `Watch History`.
- **Subtitles popover** — Anchored above the CC button: `[Add Subtitle
  Track]` pill, then a `(•) None` radio row.
- **Two-item playlist** — After `Add URL` of `Sample B`: title switched to
  `SampleB`, duration `0:00 | 0:15`, prev/next now active, and the 🔀
  shuffle button has appeared (six right-group buttons).

## Keyboard Shortcuts window

Ten overlapping frames page through one scrollable list. Every frame shows
the top area: search field (“Search shortcuts”) and the category chips
`General`, `Subtitles`, `Audio & Volume`, `Navigation`, `Display & Video`,
`Playback`, `Miscellaneous`. The pages covered, in order: General part 1,
General part 2, General end (Close Window / Save Session / Quit) +
Subtitles start, Subtitles part 1, Subtitles end + Audio start, Audio &
Volume, Audio end + Navigation start, Navigation end + Display start, and
Display & Video. **All row content is transcribed verbatim in
[05](05-keyboard-shortcuts.md)**; the images added nothing beyond it.

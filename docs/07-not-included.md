# 07 — Not Included: Gaps & Unobserved Surface

This page inventories everything the GUI observation **did not cover**, so an
implementer knows the boundary of what these documents specify. Items marked
✔ also appear on the verify-against-original checklist in
[06](06-behaviors-and-edge-cases.md).

## 1. Popovers & panels whose contents were never captured

| Element | Evidence it exists | Why unseen |
| --- | --- | --- |
| Playlist panel | ☰ control-bar button; “Add … to Playlist” actions | Button inert while the engine was stalled ✔ |
| Settings ⚙ popover | ⚙ control-bar button | Same ✔ |
| Volume popover/slider | 🔊 control-bar button; ↑/↓/M shortcuts | Did not display in the environment ✔ |
| Audio mixer popover | 🎚 control-bar button; Ctrl+A | Only its shape is inferred (mirrors the Subtitles popover) ✔ |
| Chapters menu | Ctrl+C; 🗨 control-bar button | Never opened; no chaptered media was played ✔ |
| Context (right-click) menu | Secondary Click preference offers “Context Menu” | Pref stayed on its default; the menu was never invoked |
| Seek-bar hover thumbnail | “Seek Bar Thumbnail” preference | Preference is OFF by default; left off |
| Screenshot flow (S / Shift+S) | Shortcut rows | Never pressed; destination, format, and feedback unknown ✔ |

## 2. Features proven by shortcuts/preferences but never exercised

- **A-B loop** (Ctrl+L, control-bar button): set/clear semantics and any
  on-screen feedback; interaction with Loop File.
- **Loop File** (Shift+L) and playlist-level repeat behavior.
- **Playback speed** (`[`, `]`, Backspace): rate limits, feedback display.
- **Frame stepping** (Ctrl+[ / Ctrl+]) and **chapter navigation**
  (Ctrl+←/→).
- **Audio track switching and delay** (A / Shift+A, Ctrl+− / Ctrl+=); whether
  the mixer popover lists embedded and external tracks together.
- **Secondary subtitle track** (Alt+Z / Ctrl+Z): how two simultaneous tracks
  are rendered, styled, and exposed in the Subtitles popover.
- **Subtitle styling in action**: delay/position/scale keys and the
  Preferences font/color/background options were recorded but never applied
  to visible subtitle text (no frames rendered).
- **Zoom and color adjustments** (=, +, −, keys 1–8): visual effect and any
  OSD confirmation.
- **Statistics page / persistent overlay** (I / Shift+I): contents unknown ✔.
- **Session save/restore** (Save Session; Restore Saved Session preference):
  deliberately not exercised to protect the user’s data — file format, exact
  contents (position? playlist? adjustments?), and restore UX unknown ✔.
- **Save Video Position on Close**: per its Preferences copy it stores the
  position *plus* brightness/contrast/gamma/saturation and subtitle delay;
  untested.
- **Drag-and-drop payload rules** (multi-file, folders, URLs/text, subtitle
  files): the welcome screen invites file drops; drops were never performed ✔.
- **Open Folder semantics**: playlist ordering, recursion, automatic
  sidecar (`.srt` etc.) detection.
- **URL streaming breadth**: only `http://` to a plain MP4 was tested; other
  protocols, adaptive streams (HLS/DASH), and non-media URL handling unknown.
- **Open URL validation**: whether the dialog rejects non-URL text.

## 3. Behaviors left unresolved ✔

Consolidated from [06](06-behaviors-and-edge-cases.md) for completeness:

- Meaning of the welcome history-button badge; clear-all confirmation
  semantics.
- Add-to-playlist focus semantics (add-jumps vs. error-auto-advance).
- Next/Previous wraparound at the playlist ends.
- Whether clicking a history entry re-opens that media.
- Empty-history empty state; playlist item removal and reordering UI.
- Command-line interface beyond file arguments (`--help`, other options).
- What a second, argument-less launch shows while an instance is running.

## 4. System integration not included

- **Window geometry persistence** across launches — not tested.
- **Real window-manager behavior** — no WM ran in the session; minimize/
  maximize animation, tiling, and decoration interplay are unknown.
- **MPRIS / media-key integration** — never observed; existence unknown.
- **Desktop notifications** — none appeared; unknown whether any are sent.
- **MIME associations / “Open With” integration** — outside GUI observation.
- **Accessibility tree** — the a11y bridge was disabled in the test
  environment; accessible roles/names were not captured.
- **Theming breadth** — only the dark scheme was seen; light, high-contrast,
  and RTL layouts were never rendered.
- **Touch/gesture input** — none available in the environment.

## 5. Deliberately unspecified (clean-room boundary)

- **Playback engine identity and internals.** The docs state required
  *capabilities* ([01](01-app-overview.md) “Platform notes”), never the
  original’s technology choice. Glosses that name reference technologies
  (“mpv-style stats”, “DMABUF graphics offload”) are rewriter
  interpretation, marked as such where they appear.
- **Codec/container support matrix** — only the two test MP4s were loaded.
- **Error-string catalog** — a single toast string was captured
  (“File Error: no audio or video data played”); the full set is unknown.
- **Persistence file formats** — history/session/config files were never
  read or written (user-data protection; also outside GUI-only scope).

## 6. Visual detail not captured

- Exact icon artwork — glyphs are described semantically (“ab under an arc”,
  callout bubble, crossed-out captions) and must be redrawn or substituted.
- App icon artwork beyond the small-size description in
  [01](01-app-overview.md).
- Colors/accents beyond “dark, follows system”; focus rings; hover states.
- Animation timings (control auto-hide fade, popover/toast transitions).
- Font stack and text-scaling behavior.

## 7. Environment artifacts excluded from the spec

Not app behavior; recorded only so a rewrite does not copy them:

- Black video surface / playback stalled at 0:00 (no GL under Xvfb).
- Portal file-chooser failure — but note its **side effect** (each failed
  attempt left an empty player window behind) is plausibly real app behavior
  and worth verifying on a desktop ✔.
- The SIGABRT crash on “Previous” under a stalled engine ✔.

See the README’s fidelity caveats for the full context.

## 8. Corrections made after the capture session

- The control bar’s **A-B loop** and **Chapters** buttons were present in
  the screenshots but missing from [03](03-player-window.md) as first
  written; added there afterwards (found during a screenshot
  re-verification).
- The URL-title quirk (`…/Sample%20B.mp4` → header title `SampleB`, encoded
  space dropped) is now flagged in [01](01-app-overview.md).

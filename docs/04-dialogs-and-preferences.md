# 04 — Dialogs & Preferences

## Preferences window

Opened from the primary menu or Ctrl+, A single-page Adw-style preferences
window (≈ 520 px wide, scrollable) with a warning-style ⓘ icon in the header
(left) and a close ✕ (right). Rows are grouped into rounded card groups.

### Group 1 — Behavior

| Row | Type | Default | Notes |
| --- | --- | --- | --- |
| Open New Window for New Files | toggle | **ON** | Opening a file/folder/URL spawns a new player window instead of replacing the current one. |
| Seek Bar Thumbnail | toggle | OFF | Hover preview thumbnails on the seek bar. |
| Normalize Volume | toggle | OFF | Loudness normalization during playback. |

### Group 2 — Performance

| Row | Type | Default | Subtitle / notes |
| --- | --- | --- | --- |
| Graphics Offload | toggle | OFF | “Reduces resource usage; disable if you encounter playback issues.” (The DMABUF graphics-offload reading is rewriter interpretation; the row copy does not name the mechanism.) |
| Hardware Acceleration | toggle | **ON** | “Use GPU for video decoding when available.” |

### Group 3 — Session & resume

| Row | Type | Default | Subtitle / notes |
| --- | --- | --- | --- |
| Restore Saved Session | toggle + ⓘ | OFF | “Pick up where you left off after saving from the menu.” Works with the menu’s *Save Session* / *Save Session and Close*. |
| Save Video Position on Close | toggle + ⓘ | OFF | “Also save options like brightness, subtitle delay, etc.” — i.e. per-video resume that stores playback position **and** the video adjustments/subtitle delay set during that session. |

### Group 4 — Subtitles

| Row | Type | Default | Notes |
| --- | --- | --- | --- |
| Subtitle Color | color swatch + reset ↩ | white | Renders as a rounded color button. |
| Subtitle Font | value + ⓘ + reset ↩ | `Adwaita Sans SemiBold` | Font picker behind the value. |
| Subtitle Scale | `1.00` with − / + steppers | 1.00 | Numeric stepper row. |
| Subtitle Background | toggle + checkerboard preview | OFF | Draws a background box behind subtitle text. |

### Group 5 — Mouse behavior

| Row | Type | Default | Options |
| --- | --- | --- | --- |
| Primary Click | dropdown | `Bypass` | `Play/Pause`, `Focus/Play/Pause`, `Bypass` |
| Secondary Click | dropdown | `Play/Pause` | `Play/Pause`, `Context Menu`, `Bypass` |

### Group 6 — Language preferences

| Row | Type | Notes |
| --- | --- | --- |
| Preferred Subtitle Languages | inline editor (pencil ✎) | Clicking the pencil turns the row into a text entry (placeholder shows the row title); commit with Enter. Used to auto-pick subtitle tracks matching these languages. Was empty by default. |
| Preferred Audio Languages | inline editor (pencil ✎) | Same pattern for audio tracks. |

### Footer

- **`Open Config Folder  >`** — full-width button; opens the app’s
  configuration folder in the system file manager.

## Open URL / Add URL dialogs

Two near-identical modal dialogs (same layout):

```
┌───────────── Open URL ──────────────┐   ┌───────────── Add URL ───────────────┐
│                                  ✕  │   │                                  ✕  │
│  ┌────────────────────────────────┐ │   │  ┌────────────────────────────────┐ │
│  │ URL                            │ │   │  │ URL                            │ │
│  └────────────────────────────────┘ │   │  └────────────────────────────────┘ │
│             ( Open )                │   │             ( Add )                 │
└─────────────────────────────────────┘   └─────────────────────────────────────┘
```

- Single-line text entry with a floating `URL` label; primary pill button
  (`Open` / `Add`).
- The action button is **disabled while the entry is empty** (observed on
  Add URL; Open URL presumably matches).
- `Open` loads the URL immediately (new window if the preference is on);
  `Add` appends to the current window’s playlist (and playback focused the
  newly added item in the one observed case).
- Invalid/failed media produce the standard error toast (see
  [03](03-player-window.md)).

## Keyboard Shortcuts window

Opened from the menu or Ctrl+? GNOME-standard shortcuts window:

- Top: search field (“Search shortcuts”) with a leading magnifier icon.
- Below: rounded **category chips** — `General`, `Subtitles`,
  `Audio & Volume`, `Navigation`, `Display & Video`, `Playback`,
  `Miscellaneous` — which jump to the matching section.
- Body: all categories in one scrollable list; each section has a bold
  heading and rows of `action name … key caps`. The full list is transcribed
  in [05-keyboard-shortcuts.md](05-keyboard-shortcuts.md).
- Close with the ✕ button in the window’s top-right.

## About window

Standard Adw about window:

- App icon, name **Cine**, developer **Diego Poviuk**, version chip **1.8.8**.
- Rows: **Details ›**, **Report an Issue ↗** (external link),
  **Legal ›**, **Acknowledgements ›** (standard subpages; not drilled into
  during observation).

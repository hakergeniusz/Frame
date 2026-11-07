# 02 — Welcome Window (Start Screen)

Shown when the app is launched without a media argument, and whenever a new
empty window is created.

## Layout

```
┌──────────────────────────────────────────────────────────────┐
│ ☰                                                      ▁ □ ✕ │  ← header bar (CSD)
│                                                              │
│                      [app icon]                              │
│                       Cine                                   │  ← large bold title
│             Drag and drop files here                         │  ← subtitle
│                                                              │
│              ┌──────────┐ ╭──╮                               │
│              │  Open…   │ │🕐│  ← filled pill + circular     │
│              └──────────┘ ╰──╯     history button             │
│              ┌──────────────┐                                 │
│              │ Open Folder  │   ← tonal pill                  │
│              └──────────────┘                                 │
│              ┌──────────────┐                                 │
│              │  Open URL    │   ← tonal pill                  │
│              └──────────────┘                                 │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

- Content is vertically centered; the whole window is a **drop target** for
  files.
- Header bar, left to right: primary menu button (☰); minimize, maximize,
  close.
- Default window size ≈ 1070×600 (observed at 1600×1000 screen); content
  centers on resize.

## Buttons

| Control | Behavior |
| --- | --- |
| `Open…` (filled pill) | Opens the portal file chooser for one or more video files; with “Open New Window for New Files” on, a new player window is created for the result. Ctrl+O does the same from anywhere. |
| `Open Folder` (tonal pill) | Chooser limited to folders; the folder’s video contents become the playlist. Ctrl+I. |
| `Open URL` (tonal pill) | Opens the Open URL dialog (see [04](04-dialogs-and-preferences.md)). Ctrl+U. |
| History button (🕐, circular, right of `Open…`) | Opens the **Watch History** popover. Carries a small numeric badge in its lower-right corner. Its exact meaning was not fully determined (the badge showed `0` while a history entry existed) — candidates: number of resumable/in-progress items or items added today. Verify against the original. |

## Primary menu (☰)

| Item | Shortcut |
| --- | --- |
| New Window | Ctrl+N |
| Preferences | Ctrl+, |
| Keyboard Shortcuts | Ctrl+? |
| About Cine | — |
| ─ *(separator)* | |
| Save Session | Shift+Ctrl+S |
| Save Session and Close | Shift+Q |

The menu is split into a general section and a session section. `Save
Session` persists the current playback state for later resumption (its
behavior is described via Preferences copy; not exercised in the session —
see [06](06-behaviors-and-edge-cases.md)).

## Watch History popover

Opened by the 🕐 button or `Watch History` (Ctrl+H, also present in the
player’s Open menu).

```
┌───────────────────────────────────────────┐
│ 🚫  Watch History                      ✕  │  ← clear-all icon (red-toned), title, close
│ 11/02/2026                                │  ← date group header (locale format)
│ ┌───────────────────────────────────────┐ │
│ │ bad_apple_4in1_subtitled           ⊗  │ │  ← entry row + per-row remove
│ └───────────────────────────────────────┘ │
└───────────────────────────────────────────┘
```

- Entries are **grouped by date** (a header per day, locale date format);
  within a group, **newest entries come first**.
- Each row shows the media name — the **full URL text for URL sources**
  (e.g. `http://127.0.0.1:8069/Sample%20B.mp4`), file name without extension
  for local files — and a per-row remove button (⊗).
- **URLs are recorded in history as soon as they are opened**, even if
  playback subsequently fails (observed: erroring URL opens appeared in
  history).
- The red-toned icon at the top-left of the popover is a **clear-all**
  action (icon = “history disabled” glyph). Clicking it was deliberately
  avoided during observation to protect user data; confirm its exact
  behavior (immediate clear vs. confirmation/undo toast) against the
  original.
- Clicking an entry re-opens that media (inferred from the row being a
  button-like list row; not exercised).
- History **persists across app restarts** (an entry from a previous session
  was present under today’s date).
- No empty-state text was captured (popover was only seen with ≥ 1 entry).

# 01 — App Overview

## Identity

- **Name:** Cine
- **App ID:** `io.github.diegopvlk.Cine` (installed from Flathub —
  install-time context, not GUI-observed)
- **Version observed:** 1.8.8
- **Author shown in About:** Diego Poviuk
- **Category:** desktop video player for local files, folders, and network URLs
- **Visual identity:** rounded-square app icon showing a film-strip frame with
  a play triangle, teal-to-green gradient border; dark theme by default
  (follows the system color scheme; a light scheme was not tested).

## UI toolkit characterization (from appearance only)

The UI follows GNOME / libadwaita conventions exactly: client-side-decorated
header bars with circular icon buttons, pill-shaped action buttons, grouped
rounded list rows in Preferences, Adw-style about window with subpages,
bottom-center toasts with a trailing close button, and a standard GNOME-style
Keyboard Shortcuts window with a search field and category chips. A
reimplementation should match these patterns and animations.

## Window model

- **Single instance, multiple windows.** Launching the app again while it is
  running (e.g. `flatpak run io.github.diegopvlk.Cine <file>`) is routed to
  the running instance.
- Two window types exist:
  1. **Welcome window** — the start/library screen (see
     [02-welcome-window.md](02-welcome-window.md)).
  2. **Player window** — one per playing/loaded media item (see
     [03-player-window.md](03-player-window.md)).
- With the preference **“Open New Window for New Files”** enabled (default),
  opening a file/folder/URL creates a **new** player window instead of
  reusing the current one. This was observed repeatedly: each successfully
  opened URL produced an additional window; each file-chooser attempt that
  failed still produced an extra empty player window.
- Closing the last window exits the application (observed: closing all
  windows terminated the process).
- `New Window` (Ctrl+N) in the primary menu creates an additional welcome
  window.

## Opening media

All content enters the app through one of these paths:

| Path | UI entry points |
| --- | --- |
| **Files** (one or more) | Welcome `Open…`, player menu `Open Files`, Ctrl+O, drag-and-drop onto a window |
| **Folder** | Welcome `Open Folder`, player menu `Open Folder`, Ctrl+I |
| **URL** | Welcome `Open URL`, player menu `Open URL`, Ctrl+U |
| **Add to playlist** (instead of replacing) | player menu `Add Files` / `Add Folder` / `Add URL` (Ctrl+Shift+O / I / U) |
| **External tracks** | player menu `Add Subtitle Track`, `Add Audio Track` |

Observed details:

- File and folder selection use the **desktop file-chooser portal** (the
  standard GTK portal dialog; the app renders no picker of its own).
- **URLs:** a small modal dialog with a single `URL` entry and an action
  button (`Open` or `Add`). The action button is **disabled while the entry
  is empty** (observed on the Add dialog). Plain `http(s)` URLs to a video
  file work; the app reads the stream’s metadata (title derived from the URL
  tail, duration displayed) without downloading the whole file.
- Media titles shown in the player header are the file name without
  extension for local files (`Sample A.mp4` → `Sample A`).
- **URL title derivation — verify.** For URL sources the title came from the
  URL tail with the extension stripped, but percent-decoding behaved
  unexpectedly: `…/Sample%20B.mp4` rendered in the header as **`SampleB`** —
  the encoded space was dropped rather than restored. The Watch History
  stores the full raw URL (`http://127.0.0.1:8069/Sample%20B.mp4`), so the
  decoding applies only to the display title. Verify the exact rule
  (decode-then-strip vs. strip-then-clean) against the original.
- The window **accepts drag-and-drop of files** (welcome screen subtitle:
  “Drag and drop files here”; drop targets were not exercised in the
  headless environment).

## Lifecycle & integration behaviors (observable)

- **Clipboard scan at startup.** Immediately after launch the app reads the
  system clipboard as text (an async clipboard read is issued on every
  launch; seen in the app's log output, not on screen). Purpose was not
  directly observed — most plausibly to offer
  opening a copied media URL. In the test environment the read simply failed
  silently (empty/unsupported clipboard).
- **Screensaver inhibit.** When playback is attempted the app calls the
  desktop portal’s screensaver-inhibition interface (an inhibit request
  appeared in the app's log output at the moment Play was pressed).
- **Sandbox-friendly file access.** The app does not appear to hold broad
  filesystem access; media passed by raw path on the command line was
  *invisible* to it (repeated retry with a “file error” warning, welcome
  screen shown). Access is gained through the portal file chooser /
  drag-and-drop, or via network URLs. The Preferences page includes an
  **“Open Config Folder”** button, implying user-facing configuration lives
  in a folder users can open directly.
- **Persistence.** At least the following survive restarts: watch history
  (entries grouped by date, seen for a previous session), all Preferences,
  and — when enabled — saved sessions / resume positions (feature exists but
  was not exercised to avoid touching the user’s data; semantics from its
  labels, see [04](04-dialogs-and-preferences.md)).

## Platform notes for the rewrite

- Keyboard-driven throughout; a complete shortcut map is in
  [05-keyboard-shortcuts.md](05-keyboard-shortcuts.md).
- The shortcut set implies capabilities that must exist in the playback
  engine wrapper: frame-stepping, chapters, secondary (dual) subtitle tracks,
  audio/subtitle delay compensation, video zoom, color adjustments
  (contrast/brightness/gamma/saturation), A-B looping, playback speed with
  reset, screenshot capture with/without subtitles, and a statistics/OSD
  overlay.

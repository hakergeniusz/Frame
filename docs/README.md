# Cine — Clean-Room Functional Documentation

Functional specification for a clean-room reimplementation of **Cine**
(flatpak `io.github.diegopvlk.Cine`, version 1.8.8, author Diego Poviuk).

Everything in these documents was derived from observing the **running
application** in an isolated Xvfb session — its GUI directly, and two
behaviors (the startup clipboard read and the screensaver-inhibit call,
noted in [01](01-app-overview.md)) from the running instance's own log
output. No source code, resources, or internal data files of the original
app were read.

## Clean wall for implementers

These docs specify behavior, not implementation. A rewrite must not
decompile, disassemble, or read the original app's source or resources, must
not copy its artwork (app icon, icon glyphs) or other binary assets, and
should re-author user-visible strings from the functional descriptions here
rather than treat them as assets to duplicate. What is fair game is
everything observable on screen: layout, flows, defaults, interaction rules,
and the wording of the strings documented in the pages below.

## Document index

| File | Contents |
| --- | --- |
| [01-app-overview.md](01-app-overview.md) | What the app is, window model, launch & lifecycle behavior |
| [02-welcome-window.md](02-welcome-window.md) | Start screen, primary menu, watch history |
| [03-player-window.md](03-player-window.md) | Player layout, control bar, popovers, toasts |
| [04-dialogs-and-preferences.md](04-dialogs-and-preferences.md) | Preferences, About, URL dialogs, shortcuts window |
| [05-keyboard-shortcuts.md](05-keyboard-shortcuts.md) | Complete shortcut reference (verbatim from the app's own shortcuts window) |
| [06-behaviors-and-edge-cases.md](06-behaviors-and-edge-cases.md) | Error handling, playlist dynamics, persistence, open questions |
| [07-not-included.md](07-not-included.md) | Everything the observation did **not** cover: unseen UI, unexercised features, unspecified surface |
| [08-visual-record.md](08-visual-record.md) | Textual record of the evidence screenshots (images removed — see below) |

## Method

- The app was launched under a private Xvfb display (`:69`, 1600×1000) with a
  private D-Bus session, driven with `xdotool`, and captured with ffmpeg
  `x11grab` screenshots.
- All visible windows, menus, dialogs, and popovers were opened and recorded;
  the full keyboard-shortcut window was paged through screen by screen.
- A test video was played by URL (local HTTP) because the sandboxed app could
  not be granted access to local test files without the desktop file-chooser
  portal.

## Fidelity caveats (read before implementing)

These are artifacts of the headless test environment, **not** app behavior:

1. **Video frames never rendered.** The playback engine accepted media (title
   and duration appeared, e.g. `0:20`) but never advanced past `0:00` and the
   video surface stayed black. This is a limitation of software rendering
   under Xvfb (no DRI3/GL), not an app feature. Everything else about the
   player UI was observable.
2. **Portal file chooser could not be displayed.** `Open…` / `Open Folder`
   request the desktop FileChooser portal; in this environment the portal's
   delegate crashed, so the file-picker UI itself was never seen. Side effect
   observed: each cancelled/failed chooser attempt left an additional empty
   player window behind (see [06-behaviors-and-edge-cases.md](06-behaviors-and-edge-cases.md)).
3. **Some control-bar popovers were inert** (settings gear, playlist, volume)
   while the engine was in its stalled state; the subtitles popover opened
   normally. Their existence and icons are documented; their exact contents
   are partially inferred (marked as such).
4. **One crash was observed** (SIGABRT on "Previous" while the engine was in
   the stalled/errored state). Treat as an environment artifact unless
   reproduced elsewhere.

Wherever a behavior was inferred rather than directly observed, the text says
so explicitly.

## Note on assets

The screenshots captured during the sessions were removed from this
repository: as images of the original app's UI they are derivative assets,
which the clean wall above forbids. Their content survives as text — the
ASCII layouts in [02](02-welcome-window.md)/[03](03-player-window.md), the
verbatim shortcut transcription in [05](05-keyboard-shortcuts.md), and the
per-image record in [08](08-visual-record.md).

# 03 — Player Window

One player window per media item (when “Open New Window for New Files” is
enabled, which is the default). The window shows the video surface filling
the whole window with overlaid chrome.

## Layout

```
┌──────────────────────────────────────────────────────────────┐
│ Open ⌄                Sample A                        ☰ ▁ □ ✕│  ← header bar
│                                                              │
│                       video surface                          │  ← black letterbox,
│                                                              │    click behavior per
│                                                              │    Primary Click pref
│ ⏮  ▶  ⏭   🔊  Ⓒ🇨  🎚       [0:11]      🔀 A·B 🗨 ☰ ⚙ ⤢ │  ← control overlay
│ ●──────────────────────────────────────────  0:00 │ 0:20     │  ← seek bar + times
└──────────────────────────────────────────────────────────────┘
```

## Header bar

| Element | Behavior |
| --- | --- |
| `Open ⌄` (left) | Menu button. Items: **Open Files** (Ctrl+O), **Open Folder** (Ctrl+I), **Open URL** (Ctrl+U), **Add Files** (Shift+Ctrl+O), **Add Folder** (Shift+Ctrl+I), **Add URL** (Shift+Ctrl+U), ─, **Add Subtitle Track**, **Add Audio Track**, ─, **Watch History** (Ctrl+H). “Open …” replaces the current media (new window if the preference says so); “Add …” appends to this window’s playlist. |
| Title (center) | Current media name, e.g. `Sample A`. |
| ☰ (right) | The same primary menu as the welcome window (New Window / Preferences / Keyboard Shortcuts / About Cine / Save Session / Save Session and Close). |
| Min / Max / Close | Standard CSD buttons. |

## Video area

- Fills the window; black letterbox around the video.
- **Controls auto-hide** while media is (attempted to be) played and reappear
  on pointer movement over the window; they stay visible while the pointer
  rests on the control bar.
- Mouse clicks on the video are mapped by the **Primary Click** preference
  (default `Bypass` = no action; alternatives `Play/Pause`,
  `Focus/Play/Pause`). The **Secondary Click** preference maps right-clicks
  (default `Play/Pause`; alternatives `Context Menu`, `Bypass`) — i.e. the
  player *has* a context menu that can be enabled.
- Double-click fullscreen behavior was not explicitly observed; fullscreen is
  available via button/shortcut (see below). Exit with Escape.

## Control overlay (bottom)

Left group:

| Button | Behavior |
| --- | --- |
| ⏮ Previous | Previous playlist item. Disabled/dimmed with a single-item playlist. |
| ▶ / ⏸ Play–Pause | Toggles playback. |
| ⏭ Next | Next playlist item. Disabled/dimmed with a single-item playlist; did not wrap at the end of a 2-item playlist (observed once, engine stalled). |
| 🔊 Volume | Volume control (icon with level rays; the popover/slider UI did not display in the test environment). Volume keys ↑/↓ and mute M exist. |
| Ⓒ🇨 Subtitles | Opens the **Subtitles popover** (same as Ctrl+S), see below. Icon shows a “captions off” state when no subtitle track is active. |
| 🎚 Audio mixer | Audio-menu button (mirrors the Subtitles popover; Ctrl+A opens the audio menu per the shortcut list). Contents not captured — inferred to list audio tracks with an “Add Audio Track” action, mirroring the subtitles popover. |

Right group (order as observed with ≥ 2 playlist items):
**🔀 shuffle, ⒶⒷ A-B loop, 🗨 chapters, ☰ playlist, ⚙ settings, ⤢ fullscreen**.
(With a single playlist item shuffle is absent — five buttons.)

| Button | Behavior |
| --- | --- |
| 🔀 Shuffle | Toggles shuffled playback order. **Only visible once the playlist has more than one item** (appeared after a second item was added; absent with a single item). |
| ⒶⒷ A-B Loop | A-B loop toggle (icon: `ab` under an arc; matches Ctrl+L “Set/Clear A-B Loop Points”). Press semantics (set A → set B → clear vs. single toggle) not observed — never activated. |
| 🗨 Chapters | Opens the chapters menu (icon: rounded callout bubble; matches Ctrl+C “Open Chapters Menu”). Present even for media without chapters. Contents not captured — never opened in the test environment. |
| ☰ Playlist | Opens the playlist panel. Did not open while the engine was stalled in the test environment — contents not captured. Inferred: list of queued items with the current one highlighted; this is also where “Add Files/Folder/URL” items land. |
| ⚙ Settings | Playback-settings popover (speed/loop candidates). Did not open while the engine was stalled; the underlying features all exist as shortcuts (speed [ ] Backspace, Loop File Shift+L). |
| ⤢ Fullscreen | Toggles fullscreen (F11 / F; Escape exits). |

## Seek bar

- Full-width horizontal slider below the button row; round handle.
- Right of the bar: `current | duration` in `m:ss` (e.g. `0:00 | 0:20`).
- **Hover bubble:** moving the pointer over the bar shows a small rounded
  bubble above it with the timestamp under the pointer (observed `0:11`).
- Optional hover **thumbnail preview** exists behind the
  “Seek Bar Thumbnail” preference (OFF by default; preview UI not observed).
- Clicking/dragging seeks (standard slider behavior; engine interaction not
  verifiable in the test environment).

## Subtitles popover (CC button or Ctrl+S)

```
┌───────────────────────────┐
│  [ Add Subtitle Track ]   │  ← pill button; opens the portal file chooser
│  (•) None                 │  ← radio list of subtitle tracks
└───────────────────────────┘
```

- Anchored above the CC button.
- Lists available subtitle tracks as radio items; `None` selected when no
  track is active.
- `Add Subtitle Track` loads an external subtitle file (e.g. `.srt`) via the
  portal chooser and adds it to the track list.
- An **Audio** popover with the same shape (track list + “Add Audio Track”)
  is implied by the mixer button and the Ctrl+A shortcut.
- A **Chapters** menu exists via Ctrl+C (“Open Chapters Menu”) and a
  dedicated control-bar button (right group above).

## Toasts (in-window notifications)

- Appear **bottom-center** as a rounded pill overlaying the control-bar area,
  e.g.:

  ```
  ╭─────────────────────────────────────────────╮
  │ File Error: no audio or video data played ✕ │
  ╰─────────────────────────────────────────────╯
  ```

  Observed when the engine failed to produce output for a URL.
- Toasts have a trailing ✕ dismiss button and auto-dismiss after a timeout
  (the same error toast disappeared on its own later).
- This is the app’s generic user-visible error surface for playback failures.

## Window states

- Normal, maximized, and fullscreen are supported (CSD buttons + shortcuts).
- The window title in the shell is the media name (matches header title).

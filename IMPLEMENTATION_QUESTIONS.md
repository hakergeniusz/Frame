# IMPLEMENTATION_QUESTIONS.md — Frame

Open questions for the project owner, plus questions raised during
implementation that the specs did not answer. Answers get recorded in
`IMPLEMENTATION_DECISIONS.md`.

## Open

1. **Vulkan GSK renderer.** Under GTK's Vulkan renderer the video surface
   shows corruption with the current mpv→GtkGLArea integration (GTK's
   Vulkan renderer leaves the GL context in a state mpv's render API does
   not expect, and GTK may present the GLArea through a different path).
   Real desktop sessions default to the GL renderer, where playback is
   verified correct. Should Frame force `GSK_RENDERER=gl` for its windows
   (via process environment) as a safeguard, or leave renderer selection to
   the desktop?
2. **Screenshots directory.** The spec says screenshots go to the user's
   Pictures directory; Frame uses `~/Pictures/Frame/`. Confirm the
   subdirectory name.
3. **Repository hosting.** The AppStream/desktop URLs point at
   `github.com/hakergeniusz/Frame` (not yet created). Confirm before
   release so the metadata validates.
4. **Escape-to-close for the Preferences window.** `docs/08` does not
   document whether the original's preferences window closes on Escape
   (libadwaita's stock preferences window does). Frame currently closes
   only via the header close button. Add Escape-to-close?
5. **Statistics overlay layout.** The black-box comparison (2026-11-05,
   report §17) shows Cine 1.8.8 displaying mpv's *stock* `stats.lua`
   overlay. Frame renders its own layout because the original's stats
   UI had never been visually exercised when `docs/07` was written —
   that no longer holds. Keep Frame's own layout, or match mpv's stock
   stats page?
6. **Shuffle / repeat buttons.** Cine 1.8.8's control row has shuffle
   and repeat buttons (and no subtitle bubble) on the right; the docs'
   reference screenshots (older Cine) show the subtitle bubble and no
   shuffle/repeat, which is what Frame implements. Track 1.8.8's
   button set, or keep the docs' set?
7. **Arrow-key seek step.** Cine 1.8.8 seeks ±10 s per Left/Right
   press; `docs/05` (and Frame) step ±5 s. Which is canonical?

## Answered during implementation

*(none yet — see `IMPLEMENTATION_DECISIONS.md` for decisions taken
autonomously where the docs were unambiguous)*

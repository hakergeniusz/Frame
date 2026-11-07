# Contributing to Frame

Thanks for your interest in Frame — an independent, clean-room media player.
Contributions of code, tests, packaging fixes, and documentation are welcome.

## Ground rules

- **Clean-room discipline.** Frame is implemented from the behavioural
  specifications in `docs/` only. Do not consult, copy, or link the source
  code, binaries, artwork, or other internal assets of the player that Frame
  reimplements. If you spot a behavioural question, raise it in
  `IMPLEMENTATION_QUESTIONS.md` instead of looking at the original.
- **Licensing.** All original code and assets are MIT. Do not copy code from
  GPL- or other-licensed projects into Frame's source tree. The libmpv
  engine stays *dynamically linked* and licence-separate — never describe
  the whole app or libmpv as MIT, and keep engine build-configuration claims
  verified (see `THIRD_PARTY_LICENSES.md`).
- **Match the surrounding style.** Rust edition 2024; comments explain *why*
  where it is not obvious; user-visible strings follow the wording in
  `docs/` where one exists.

## Building

```sh
cargo build --release
```

System requirements: GTK 4, libadwaita ≥ 1.5, libmpv ≥ 2.0 (development
packages), EGL/GLX headers, pkg-config.

## Testing

```sh
cargo test            # unit tests (headless, no display needed)
```

For interactive verification under a headless X server:

```sh
Xvfb :69 &
DISPLAY=:69 GDK_BACKEND=x11 LIBGL_ALWAYS_SOFTWARE=1 GSK_RENDERER=gl \
    ./target/release/frame some-video.mp4
```

`GSK_RENDERER=gl` keeps GTK's GL renderer under software rendering; the
Vulkan renderer has a known video-surface issue (see
`IMPLEMENTATION_DECISIONS.md`).

Every bug fix should come with a test or a documented headless
verification step; every documented behaviour in `docs/` maps to a
requirement ID in `IMPLEMENTATION_PLAN.md`.

## Packaging

Package files live in `packaging/` (`debian/`, `fedora/`, `arch/`,
`flatpak/`). Changes to icons, desktop metadata, install layout, or build
dependencies must be reflected in **all four** formats and verified for
**both** AMD64 and ARM64 where possible — see `packaging/README.md` for the
build commands. Never claim a build configuration (e.g. an LGPL mpv build)
is verified without checking its actual output.

## Sending changes

1. Fork / branch from `main`.
2. Keep commits focused; write commit messages that state the *why*.
3. Run `cargo test` and, if packaging was touched, the relevant
   `packaging/*/build.sh`.
4. Update `IMPLEMENTATION_DECISIONS.md` if you made a behavioural decision,
   and `IMPLEMENTATION_PLAN.md` requirement statuses if they changed.

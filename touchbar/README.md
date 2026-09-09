# Leapfrog Touch Bar

A compact T1Bridge renderer that follows Omarchy's current theme and active
workspace. The controls use only the active theme's semantic `selection` and
`accent` palette entries—not fixed literal hues.

Button labels are always pure white and every button fill is required to
provide at least 6:1 text contrast. Fills begin with those two semantic
palette entries, then move only as far as needed to satisfy that rule;
readability takes priority over preserving the palette color exactly.

The normal layout uses 22px icons for pictographic actions, with clear `ESC`
and `FN` text labels. Every action button is 143px wide in both the normal and
Fn/F1–F12 interfaces. The workspace tile is the only exception: it receives
the remaining width. The function toggle sits at the far-right edge.

The renderer reads only user-facing Omarchy state under
`~/.local/state/omarchy/current/`. It does not modify packaged Omarchy files.

## Commands

```sh
cargo run -- preview
cargo run -- select
cargo run -- stock
```

With no arguments the binary runs as a T1Bridge renderer. Press the on-screen
restore control to remove the custom selection and return immediately to the
packaged renderer.

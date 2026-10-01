# Leapfrog Touch Bar

A compact T1Bridge renderer that follows Omarchy's current theme. The
controls use only the active theme's semantic `selection` and `accent`
palette entries—not fixed literal hues—and neighbouring button groups
alternate between the two so each group stands apart from the next.

Button labels are always pure white and every button fill is required to
provide at least 6:1 text contrast. Fills begin with those two semantic
palette entries, then move only as far as needed to satisfy that rule;
readability takes priority over preserving the palette color exactly.

The normal layout uses 22px icons for pictographic actions, with clear `ESC`
and `FN` text labels. The keyboard backlight pair is drawn to match the
illumination keys on Apple keyboards: three rays over a dashed bar, short
for dimmer and long for brighter. A scissors-icon printscreen button sits
between the volume-up and Fn buttons and opens the same Omasnap screenshot
flow the keyboard's SUPER SHIFT S and Print keys run. Every button is 143px
wide in both the normal and Fn/F1–F12 interfaces, and each row is centred:
thirteen buttons in the normal layout, fourteen in the Fn/F1–F12 layout.

The renderer reads only user-facing Omarchy state under
`~/.local/state/omarchy/current/`. It does not modify packaged Omarchy files.
It launches the screenshot flow through `hyprctl --instance 0`, because
T1Bridge starts the renderer at boot, before Hyprland is running, so it
never inherits the environment a Wayland capture tool needs.

## Commands

```sh
cargo run -- preview
cargo run -- select
cargo run -- stock
```

With no arguments the binary runs as a T1Bridge renderer. Run `leapfrog stock`
(or `cargo run -- stock`) to remove the custom selection and return immediately
to the packaged renderer.

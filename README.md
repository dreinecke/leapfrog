# Leapfrog

Leapfrog is the integration layer that makes the Apple T1 hardware in Voyager,
a 2017 MacBook Pro running Omarchy, useful through
[T1Bridge](https://github.com/standardagents/t1bridge).

Leapfrog does not fork T1Bridge. The upstream packages provide the kernel
drivers and hardware services; this repository contains the machine-facing
parts built around them:

- `touchbar/` — an Omarchy-themed Touch Bar renderer with media, brightness,
  audio, Escape, Fn/F1–F12, and Touch ID feedback.
- `camera/` — a stable virtual FaceTime camera for applications that cannot
  consume the T1 camera's native H.264 stream.
- `system/` — the reviewed PAM and module configuration for Touch ID and the
  virtual camera.
- `theme-gallery/` — a local browser for checking the Omarchy colors and
  backgrounds that drive the renderer.
- `bin/leapfrog` — one status and maintenance command for the complete setup.

## Install the user-owned parts

The upstream T1Bridge, T1Bridge DKMS, T1Bridge-enabled fprintd/libfprint,
v4l2loopback, GStreamer, Rust, and standard C build tools must already be
installed. Then run:

```sh
./install.sh
```

The installer builds and selects `leapfrog-touchbar`, builds the patched camera
relay from a pinned upstream revision, enables `leapfrog-camera.service`, and
installs the `leapfrog` command. It does not use `sudo`.

The root-owned PAM and module files are documented in
[`system/README.md`](system/README.md). Their installer deliberately leaves a
root recovery shell open while fingerprint and password fallback are tested.

## Status

```sh
leapfrog status
```

## Enterprise mirror

Enterprise keeps the authoritative clone and copies it only to Voyager through
its ship-sync job. Run `hooks/install` once in the Enterprise clone so a
Leapfrog commit starts that sync automatically. The hook is deliberately a
no-op on every other machine.

## Data boundary

Apple firmware, T1 keybags, fingerprint enrollments, generated wallpaper
thumbnails, and credentials are machine state and are never stored here.

Unless a file says otherwise, Leapfrog is MIT licensed. The camera relay patch
is GPL-2.0-only because it modifies GPL-2.0-only upstream source. The Touch Bar
client attribution is recorded in `touchbar/THIRD-PARTY.md`.

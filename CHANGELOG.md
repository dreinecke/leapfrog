# Changelog

## 0.2.3 — 2026-09-13

- Replaced the Touch Bar's return-to-stock button with keyboard backlight
  controls. They sit just right of the workspace tile, which keeps that tile
  centred between six actions on either side, and their icons follow the
  illumination keys on Apple keyboards: three rays over a dashed bar, short
  for dimmer and long for brighter. Each press moves the keyboard in 10%
  steps, through T1Bridge when it offers the keyboard backlight and through
  `brightnessctl` otherwise. The stock renderer is still one `leapfrog stock`
  away.
- Button groups now alternate between the theme's selection and accent fills
  by position rather than by a fixed colour per control, so neighbouring
  groups always stand apart. The Fn row alternates the same way.

## 0.2.2 — 2026-09-13

- Kept the Touch Bar renderer alive through a cold boot. The T1 hardware can
  take half a minute to answer after the launcher starts the renderer, and a
  single refused connection made the renderer exit, which left T1Bridge on its
  built-in bar until the next manual restart. The renderer now waits up to 90
  seconds for the hardware and reconnects for the rest of the session, so a
  suspend or a hardware hiccup no longer costs the Omarchy bar either.

## 0.2.1 — 2026-09-10

- Added an inert staging-tree mode for Enterprise, so the exact marketplace
  lock plugin is guarded before it is copied to Voyager.

## 0.2.0 — 2026-09-10

- Prevented an unavailable T1 keybag service from causing an unlimited
  lock-screen fingerprint retry loop. Three immediate PAM failures now pause
  Touch ID for that lock session while password unlock remains available.

## 0.1.0 — 2026-09-09

- Named and collected the T1Bridge integration as Leapfrog.
- Added the Omarchy-themed Touch Bar renderer, FaceTime camera relay, Touch ID
  PAM configuration, theme gallery, installers, and status command.

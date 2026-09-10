# Changelog

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

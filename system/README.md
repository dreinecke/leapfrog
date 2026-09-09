# System-owned configuration

These files reproduce Voyager's root-owned Leapfrog configuration:

- `pam/` enables Touch ID for sudo, PolicyKit, and the Omarchy lock screen while
  retaining password fallback where applicable.
- `modprobe/v4l2-relayd.conf` creates `/dev/video10` as `FaceTime Camera`.
- `t1bridge/diagnostics.conf` disables upstream diagnostic capture by default.

Review the files first. Apply them from a visible terminal with:

```sh
sudo ./system/install
```

The installer backs up every replaced file under `/var/backups/leapfrog/` and
then leaves a root recovery shell open. Keep that shell open until fingerprint
authentication and password fallback have both been tested.

The upstream packages are not copied into this repository. Voyager currently
uses `t1bridge`, `t1bridge-dkms`, `libfprint-t1bridge`, `fprintd-t1bridge`,
`v4l2loopback-dkms`, and `v4l2loopback-utils`.

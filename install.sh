#!/usr/bin/env bash
set -euo pipefail

LEAPFROG_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RETIRED_DIR="$HOME/.local/share/leapfrog/retired"

for command_name in cargo install systemctl; do
  command -v "$command_name" >/dev/null 2>&1 || {
    printf 'Missing required command: %s\n' "$command_name" >&2
    exit 1
  }
done
command -v t1bridge >/dev/null 2>&1 || {
  printf 'Install upstream T1Bridge before Leapfrog.\n' >&2
  exit 1
}

cargo build --release --manifest-path "$LEAPFROG_DIR/touchbar/Cargo.toml"
install -Dm755 \
  "$LEAPFROG_DIR/touchbar/target/release/leapfrog-touchbar" \
  "$HOME/.local/bin/leapfrog-touchbar"
install -Dm755 "$LEAPFROG_DIR/bin/leapfrog" "$HOME/.local/bin/leapfrog"
"$LEAPFROG_DIR/camera/build-relay" "$HOME/.local/libexec/leapfrog-camera-relayd"
"$LEAPFROG_DIR/lock/install"

mkdir -p "$HOME/.config/t1bridge" "$HOME/.config/systemd/user" "$RETIRED_DIR"

renderer="$HOME/.config/t1bridge/renderer"
if [ "$(readlink "$renderer" 2>/dev/null || true)" = "$HOME/.local/bin/voyager-touchbar" ]; then
  unlink "$renderer"
fi
"$HOME/.local/bin/leapfrog-touchbar" select

old_unit="$HOME/.config/systemd/user/t1-camera-compat.service"
if systemctl --user cat t1-camera-compat.service >/dev/null 2>&1; then
  systemctl --user disable --now t1-camera-compat.service >/dev/null 2>&1 || true
fi
if [ -f "$old_unit" ]; then
  install -Dm644 "$old_unit" "$RETIRED_DIR/t1-camera-compat.service"
  unlink "$old_unit"
fi

install -Dm644 \
  "$LEAPFROG_DIR/camera/leapfrog-camera.service" \
  "$HOME/.config/systemd/user/leapfrog-camera.service"
systemctl --user daemon-reload
systemctl --user enable --now leapfrog-camera.service

for old_binary in \
  "$HOME/.local/bin/voyager-touchbar" \
  "$HOME/.local/libexec/t1-camera-relayd"; do
  if [ -f "$old_binary" ]; then
    install -Dm755 "$old_binary" "$RETIRED_DIR/$(basename "$old_binary")"
    unlink "$old_binary"
  fi
done

printf '\nLeapfrog installed. Run `leapfrog status` to inspect it.\n'

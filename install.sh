#!/usr/bin/env bash
set -euo pipefail
project_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
if [[ -f /run/.containerenv ]] && command -v distrobox-host-exec >/dev/null; then
    exec distrobox-host-exec bash "$project_dir/install.sh" "$@"
fi
if [[ ${1:-} == --permissions ]]; then
    if (( EUID != 0 )); then
        exec sudo -- bash "$project_dir/install.sh" --permissions
    fi
    install -m 644 -- "$project_dir/packaging/70-controllerapp.rules" /etc/udev/rules.d/70-controllerapp.rules
    install -m 644 -- "$project_dir/packaging/controllerapp.conf" /etc/modules-load.d/controllerapp.conf
    modprobe uinput
    udevadm control --reload-rules
    udevadm trigger --subsystem-match=misc --sysname-match=uinput
    udevadm trigger --subsystem-match=input --property-match=ID_INPUT_JOYSTICK=1
    udevadm settle
    exit 0
fi
if (( EUID == 0 )); then
    echo 'Run ./install.sh as your normal user; sudo is used only if needed.' >&2
    exit 1
fi
build_flags=()
if [[ ${1:-} == --private-assets ]] && [[ $# == 1 ]]; then
    build_flags=(--features private-assets)
    echo 'Local-only asset build: do not redistribute this binary without asset permission.'
elif [[ $# -gt 0 ]]; then
    echo 'Usage: ./install.sh [--private-assets | --permissions]' >&2
    exit 2
fi
cd -- "$project_dir"
for dependency in cargo pkg-config; do
    command -v "$dependency" >/dev/null || { echo "Missing dependency: $dependency" >&2; exit 1; }
done
pkg-config --atleast-version=4.18 gtk4
pkg-config --exists gtk4-layer-shell-0
cargo build --release --locked "${build_flags[@]}"
cargo test --locked "${build_flags[@]}"
if [[ ! -w /dev/uinput ]]; then
    bash "$project_dir/install.sh" --permissions
fi
if [[ ! -w /dev/uinput ]]; then
    echo 'uinput is still unavailable; log out and back into the local session, then reinstall.' >&2
    exit 1
fi
binary_dir="$HOME/.local/lib/controllerapp"
config_dir="${XDG_CONFIG_HOME:-$HOME/.config}"
install -d -- "$binary_dir" "$HOME/.local/bin" "$config_dir/autostart"
if "$binary_dir/controllerapp-linux" --status >/dev/null 2>&1; then
    "$binary_dir/controllerapp-linux" --quit
fi
# Rename a new inode, so an existing executable can finish cleanly.
install -m 755 -- target/release/controllerapp-linux "$binary_dir/controllerapp-linux.new"
mv -f -- "$binary_dir/controllerapp-linux.new" "$binary_dir/controllerapp-linux"
ln -sfn -- "$binary_dir/controllerapp-linux" "$HOME/.local/bin/controllerapp-linux"
desktop_binary="$binary_dir/controllerapp-linux"
desktop_binary=${desktop_binary//\\/\\\\}
desktop_binary=${desktop_binary//\"/\\\"}
desktop_binary=${desktop_binary//\$/\\\$}
desktop_binary=${desktop_binary//\`/\\\`}
desktop_binary=${desktop_binary//%/%%}
desktop_binary=${desktop_binary//&/\\&}
desktop_binary=${desktop_binary//|/\\|}
sed "s|@BINARY@|$desktop_binary|g" packaging/controllerapp-linux.desktop.in > "$config_dir/autostart/controllerapp-linux.desktop"
echo "Installed to: $binary_dir/controllerapp-linux"
echo 'Autostart enabled. Start with: controllerapp-linux'

#!/usr/bin/env bash
set -euo pipefail
if [[ -f /run/.containerenv ]] && command -v distrobox-host-exec >/dev/null; then
    project_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
    exec distrobox-host-exec bash "$project_dir/uninstall.sh" "$@"
fi
if (( EUID == 0 )); then echo 'Run as your normal user, not root.' >&2; exit 1; fi
binary="$HOME/.local/lib/controllerapp/controllerapp-linux"
if [[ -x "$binary" ]]; then "$binary" --quit 2>/dev/null || true; fi
rm -f -- "${XDG_CONFIG_HOME:-$HOME/.config}/autostart/controllerapp-linux.desktop"
if [[ -L "$HOME/.local/bin/controllerapp-linux" ]] && [[ $(readlink -- "$HOME/.local/bin/controllerapp-linux") == "$binary" ]]; then
    rm -- "$HOME/.local/bin/controllerapp-linux"
fi
rm -f -- "$binary"
echo 'Binary and autostart removed. Source code preserved.'
echo 'If installed by this project, optional rules can be removed with:'
echo 'sudo rm /etc/udev/rules.d/70-controllerapp.rules /etc/modules-load.d/controllerapp.conf'

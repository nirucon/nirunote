#!/usr/bin/env sh
set -eu
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
rm -f "$HOME/.local/bin/nirunote"
rm -f "$DATA/applications/nirunote.desktop"
rm -f "$DATA/icons/hicolor/scalable/apps/nirunote.svg"
rm -rf "$DATA/nirunote"
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$DATA/applications" >/dev/null 2>&1 || true
printf '%s\n' 'NIRUNOTE application files removed.'
printf '%s\n' 'Settings/recovery were intentionally kept. Remove ~/.config/NIRU and ~/.local/state/nirunote manually only if you also want to erase user state.'

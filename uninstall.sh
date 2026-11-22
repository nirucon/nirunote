#!/bin/sh
# NIRUNOTE uninstaller: removes the program, its desktop entry and icon.
#
#   ./uninstall.sh          keep settings, notes, recovery data and the
#                           personal word list
#   ./uninstall.sh --purge  also remove ~/.config/NIRU/NIRUNOTE.conf,
#                           ~/.local/state/nirunote (the scratchpad note and
#                           recovery data), ~/.cache/nirunote and the
#                           dictionaries
#
# Your documents are never touched.
set -eu
PURGE=0
for arg in "$@"; do
    case "$arg" in
        --purge) PURGE=1 ;;
        -h | --help) printf '%s\n' 'usage: ./uninstall.sh [--purge]'; exit 0 ;;
        *) printf 'nirunote uninstall: unknown option %s\n' "$arg" >&2; exit 2 ;;
    esac
done
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
rm -f "$HOME/.local/bin/nirunote" "$HOME/.local/bin/.nirunote.new" "$HOME/.local/bin/.nirunote.previous"
rm -f "$DATA/applications/nirunote.desktop"
rm -f "$DATA/icons/hicolor/scalable/apps/nirunote.svg"
# NIRUNOTE 0.x (Python) releases, if any are left.
rm -rf "$DATA/nirunote/releases"
rm -f "$DATA/nirunote/current"
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$DATA/applications" >/dev/null 2>&1 || :
if [ "$PURGE" = 1 ]; then
    rm -f "${XDG_CONFIG_HOME:-$HOME/.config}/NIRU/NIRUNOTE.conf"
    rmdir "${XDG_CONFIG_HOME:-$HOME/.config}/NIRU" 2>/dev/null || :
    rm -rf "${XDG_STATE_HOME:-$HOME/.local/state}/nirunote"
    rm -rf "${XDG_CACHE_HOME:-$HOME/.cache}/nirunote"
    rm -rf "$HOME/.local/share/nirunote"
    printf '%s\n' 'NIRUNOTE and its settings, scratchpad, recovery data and dictionaries were removed.'
else
    printf '%s\n' 'NIRUNOTE was removed. Settings, the scratchpad note, recovery data and dictionaries were kept (./uninstall.sh --purge removes them).'
fi

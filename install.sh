#!/bin/sh
# NIRUNOTE installer.
#
#   ./install.sh             install the prebuilt release binary (downloaded
#                            and verified against SHA256SUMS), or ./nirunote
#                            or target/release/nirunote when present
#   ./install.sh --build     build from this checkout with cargo first
#   ./install.sh --prebuilt  use ./nirunote beside this script
#   ./install.sh --yes       accepted for compatibility (nothing to confirm)
#
# Installs ~/.local/bin/nirunote, the desktop entry and the icon. It replaces
# a NIRUNOTE 0.x (Python) installation: its releases and launcher go, while
# the settings (~/.config/NIRU/NIRUNOTE.conf), the scratchpad, recovery data
# (~/.local/state/nirunote) and the dictionaries stay. No sudo is used.
set -eu

PREFIX="${NIRUNOTE_PREFIX:-$HOME/.local}"
BINDIR="$PREFIX/bin"
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
APPDIR="$DATA/applications"
ICONDIR="$DATA/icons/hicolor/scalable/apps"
BASE="$DATA/nirunote"
DICT_DIR="$HOME/.local/share/nirunote/dictionaries"
MODE=auto

for arg in "$@"; do
    case "$arg" in
        --build) MODE=build ;;
        --prebuilt) MODE=prebuilt ;;
        -y | --yes) ;;
        -h | --help)
            printf '%s\n' "usage: ./install.sh [--build|--prebuilt] [--yes]"
            exit 0
            ;;
        *) printf 'nirunote install: unknown option %s (try --help)\n' "$arg" >&2; exit 2 ;;
    esac
done

say() { printf '%s\n' "$*"; }
die() { say "nirunote install: $*" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1; }

[ "$(id -u)" != 0 ] || die 'run ./install.sh as your desktop user, not root'
HERE=$(cd -- "$(dirname -- "$0")" && pwd)
cd "$HERE"

VERSION="${NIRUNOTE_VERSION:-}"
if [ -z "$VERSION" ] && [ -f Cargo.toml ]; then
    VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
fi
[ -n "$VERSION" ] || die 'cannot determine the NIRUNOTE version (set NIRUNOTE_VERSION)'
say "NIRUNOTE $VERSION installer"

TMP=""
trap 'rm -rf "$TMP"' EXIT
verify() { # FILE SUMS
    need sha256sum || die 'sha256sum is required to verify the release binary'
    expected=$(awk '$2 == "nirunote" || $2 == "*nirunote" { print $1; exit }' "$2")
    [ -n "$expected" ] || die 'the checksum file does not list nirunote'
    actual=$(sha256sum "$1" | awk '{print $1}')
    [ "$expected" = "$actual" ] || die 'the NIRUNOTE binary failed SHA-256 verification'
    say 'SHA-256 verification passed.'
}

if [ "$MODE" = build ]; then
    need cargo || die 'Rust/cargo is required for --build'
    say 'Building NIRUNOTE…'
    cargo build --release --locked
    BIN_SRC=target/release/nirunote
elif [ "$MODE" = prebuilt ] || { [ -f ./nirunote ] && [ -x ./nirunote ]; }; then
    [ -f ./nirunote ] && [ -x ./nirunote ] || die './nirunote not found beside install.sh'
    BIN_SRC=./nirunote
    [ ! -f SHA256SUMS ] || verify "$BIN_SRC" SHA256SUMS
elif [ -x target/release/nirunote ]; then
    BIN_SRC=target/release/nirunote
else
    [ "$(uname -m)" = x86_64 ] || die "no prebuilt release for $(uname -m); use ./install.sh --build"
    need curl || need wget || die 'curl or wget is required to download the release binary'
    TMP=$(mktemp -d)
    URL="https://github.com/nirucon/nirunote/releases/download/v${VERSION}"
    say "Downloading NIRUNOTE ${VERSION}…"
    for f in nirunote SHA256SUMS; do
        if need curl; then
            curl -fsSL --retry 3 -o "$TMP/$f" "$URL/$f" || die "could not download $f"
        else
            wget -qO "$TMP/$f" "$URL/$f" || die "could not download $f"
        fi
    done
    verify "$TMP/nirunote" "$TMP/SHA256SUMS"
    BIN_SRC="$TMP/nirunote"
fi
chmod +x "$BIN_SRC" 2>/dev/null || :
got=$("$BIN_SRC" --version 2>/dev/null | awk '{print $NF}')
[ "$got" = "$VERSION" ] || die "the binary reports version '${got:-none}', expected $VERSION"
"$BIN_SRC" --core-self-test >/dev/null || die 'the core self-test failed; nothing was changed'

# The binary goes in beside the old one and replaces it in one rename, so a
# running NIRUNOTE keeps working and a failed check can roll back.
install -d -m 0755 "$BINDIR" "$APPDIR" "$ICONDIR"
install -m 0755 "$BIN_SRC" "$BINDIR/.nirunote.new"
if [ -e "$BINDIR/nirunote" ]; then
    cp -p "$BINDIR/nirunote" "$BINDIR/.nirunote.previous" 2>/dev/null || :
fi
mv -f "$BINDIR/.nirunote.new" "$BINDIR/nirunote"
if [ "$("$BINDIR/nirunote" --version 2>/dev/null | awk '{print $NF}')" != "$VERSION" ]; then
    if [ -e "$BINDIR/.nirunote.previous" ]; then
        mv -f "$BINDIR/.nirunote.previous" "$BINDIR/nirunote"
        die 'the installed binary failed --version; the previous one was restored'
    fi
    die 'the installed binary failed --version'
fi
rm -f "$BINDIR/.nirunote.previous"

sed "s|^Exec=.*|Exec=$BINDIR/nirunote %F|; s|^TryExec=.*|TryExec=$BINDIR/nirunote|" \
    assets/nirunote.desktop > "$APPDIR/nirunote.desktop"
chmod 0644 "$APPDIR/nirunote.desktop"
install -m 0644 assets/nirunote.svg "$ICONDIR/nirunote.svg"

# NIRUNOTE 0.x (Python): its releases and the `current` link are no longer
# used. The dictionaries and the personal word list stay.
if [ -d "$BASE/releases" ] || [ -L "$BASE/current" ]; then
    rm -rf "$BASE/releases"
    rm -f "$BASE/current"
    say 'Removed the NIRUNOTE 0.x (Python) releases; settings and notes are kept.'
fi

# Spell checking: a user-local Swedish dictionary when Hunspell is there and
# no working one is found (no sudo). Hunspell itself is optional.
spell_sv_ok() {
    need hunspell || return 1
    [ -s "$DICT_DIR/sv_SE.dic" ] && [ -s "$DICT_DIR/sv_SE.aff" ] || return 1
    [ "$(printf 'Hej\nasdasdasd\n' | hunspell -d "$DICT_DIR/sv_SE" -l 2>/dev/null || :)" = asdasdasd ]
}
fetch() {
    rm -f "$2.tmp"
    if need curl; then curl -fsSL "$1" -o "$2.tmp" && mv "$2.tmp" "$2"
    elif need wget; then wget -q "$1" -O "$2.tmp" && mv "$2.tmp" "$2"
    else return 1; fi
}
if need hunspell && ! spell_sv_ok; then
    mkdir -p "$DICT_DIR"
    say 'Preparing a user-local Swedish spelling dictionary…'
    raw=https://raw.githubusercontent.com/yeager/hunspell-sv/main
    fetch "$raw/sv_SE.dic" "$DICT_DIR/sv_SE.dic" || :
    fetch "$raw/sv_SE.aff" "$DICT_DIR/sv_SE.aff" || :
    fetch "$raw/LICENSE" "$DICT_DIR/LICENSE-hunspell-sv.txt" || :
fi
if spell_sv_ok; then
    say 'Swedish spell checking: OK'
elif ! need hunspell; then
    say 'Spell checking is optional: install hunspell (and a dictionary) to enable it.'
fi

if need update-desktop-database; then update-desktop-database "$APPDIR" >/dev/null 2>&1 || :; fi
if need gtk-update-icon-cache; then gtk-update-icon-cache -q -f -t "$DATA/icons/hicolor" >/dev/null 2>&1 || :; fi

say "Installed NIRUNOTE $VERSION at $BINDIR/nirunote"
say 'A NIRUNOTE window that is already open keeps its old version until it is closed.'

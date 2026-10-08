#!/usr/bin/env bash
set -Eeuo pipefail

VERSION="0.4.0"
APP="nirunote"
SRC_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
BASE="${HOME}/.local/share/nirunote"
RELEASES="${BASE}/releases"
RELEASE_DIR="${RELEASES}/${VERSION}"
CURRENT="${BASE}/current"
BIN_DIR="${HOME}/.local/bin"
DESKTOP_DIR="${HOME}/.local/share/applications"
ICON_DIR="${HOME}/.local/share/icons/hicolor/scalable/apps"
STATE_DIR="${HOME}/.local/state/nirunote"

echo "NIRUNOTE ${VERSION} installer"
echo "-------------------------"

need_pyside() {
  python3 - <<'PY' >/dev/null 2>&1
import PySide6
PY
}

if ! command -v python3 >/dev/null 2>&1 || ! need_pyside; then
  echo "Installing required runtime dependencies..."
  if command -v pacman >/dev/null 2>&1; then
    sudo pacman -S --needed python pyside6
  elif command -v apt-get >/dev/null 2>&1; then
    sudo apt-get update
    sudo apt-get install -y python3 python3-pyside6.qtcore python3-pyside6.qtgui python3-pyside6.qtwidgets
  else
    echo "ERROR: Unsupported package manager. Install Python 3 and PySide6, then rerun." >&2
    exit 1
  fi
fi

need_pyside || { echo "ERROR: PySide6 import failed after dependency setup." >&2; exit 1; }

# Spell checking: user-local, no sudo and no PyEnchant.
DICT_DIR="${BASE}/dictionaries"
mkdir -p "$DICT_DIR"
download_file() {
  local url="$1" dest="$2"
  rm -f "${dest}.tmp"
  if command -v curl >/dev/null 2>&1; then
    curl -L --fail --silent --show-error "$url" -o "${dest}.tmp" && mv "${dest}.tmp" "$dest"
  elif command -v wget >/dev/null 2>&1; then
    wget -q "$url" -O "${dest}.tmp" && mv "${dest}.tmp" "$dest"
  else return 1; fi
}
spell_sv_ok() {
  command -v hunspell >/dev/null 2>&1 || return 1
  [ -s "$DICT_DIR/sv_SE.dic" ] && [ -s "$DICT_DIR/sv_SE.aff" ] || return 1
  local bad
  bad="$(printf 'Hej\nasdasdasd\n' | hunspell -d "$DICT_DIR/sv_SE" -l 2>/dev/null || true)"
  [ "$bad" = "asdasdasd" ]
}
if ! spell_sv_ok; then
  echo "Preparing user-local Swedish spell dictionary (no sudo)..."
  download_file "https://raw.githubusercontent.com/yeager/hunspell-sv/main/sv_SE.dic" "$DICT_DIR/sv_SE.dic" || true
  download_file "https://raw.githubusercontent.com/yeager/hunspell-sv/main/sv_SE.aff" "$DICT_DIR/sv_SE.aff" || true
  download_file "https://raw.githubusercontent.com/yeager/hunspell-sv/main/LICENSE" "$DICT_DIR/LICENSE-hunspell-sv.txt" || true
fi
if spell_sv_ok; then
  echo "Swedish spell checking: OK"
elif ! command -v hunspell >/dev/null 2>&1; then
  echo "WARNING: hunspell is missing. Install it once with your distribution package manager to enable spelling."
else
  echo "WARNING: Swedish dictionary could not be prepared; NIRUNOTE will show SPELL?."
fi

mkdir -p "$RELEASES" "$BIN_DIR" "$DESKTOP_DIR" "$ICON_DIR" "$STATE_DIR"
rm -rf "${RELEASE_DIR}.staging"
STAGE="${RELEASE_DIR}.staging"
mkdir -p "$STAGE"
cp -a "$SRC_DIR"/. "$STAGE"/
rm -rf "$STAGE/nirunote/__pycache__"

echo "Validating Python..."
python3 -m py_compile "$STAGE/nirunote/app.py" "$STAGE/gui-smoke-test.py"

echo "Validating NIRUNOTE core primitives..."
if ! timeout --signal=TERM 8s python3 "$STAGE/nirunote/app.py" --core-self-test; then
  echo "ERROR: core self-test failed. Existing installation remains active." >&2
  rm -rf "$STAGE"; exit 1
fi

echo "Validating NIRUNOTE spell runtime..."
if command -v hunspell >/dev/null 2>&1 && [ -s "$DICT_DIR/sv_SE.dic" ] && [ -s "$DICT_DIR/sv_SE.aff" ]; then
  if ! timeout --signal=TERM 8s python3 "$STAGE/nirunote/app.py" --spell-self-test; then
    echo "ERROR: NIRUNOTE runtime cannot use the verified Swedish dictionary." >&2
    echo "Existing installation has not been activated/replaced." >&2
    rm -rf "$STAGE"
    exit 1
  fi
fi

echo "Validating GUI startup in an isolated profile..."
SMOKE_STATUS=0
if command -v timeout >/dev/null 2>&1; then
  QT_QPA_PLATFORM=offscreen timeout --signal=TERM --kill-after=2s 8s python3 "$STAGE/gui-smoke-test.py" || SMOKE_STATUS=$?
else
  QT_QPA_PLATFORM=offscreen python3 "$STAGE/gui-smoke-test.py" || SMOKE_STATUS=$?
fi
if [ "$SMOKE_STATUS" -ne 0 ]; then
  rm -rf "$STAGE"
  if [ "$SMOKE_STATUS" -eq 124 ]; then
    echo "ERROR: GUI validation timed out after 8 seconds. Existing NIRUNOTE installation was not changed." >&2
  else
    echo "ERROR: GUI validation failed with exit code $SMOKE_STATUS. Existing NIRUNOTE installation was not changed." >&2
  fi
  exit "$SMOKE_STATUS"
fi

echo "Installing release ${VERSION}..."
# Keep the previous same-version installation available for rollback.
BACKUP_DIR="${RELEASE_DIR}.previous"
if [ -e "$RELEASE_DIR" ]; then
  if [ -e "$BACKUP_DIR" ]; then
    echo "ERROR: existing backup at $BACKUP_DIR; refusing to overwrite." >&2
    exit 1
  fi
  mv "$RELEASE_DIR" "$BACKUP_DIR"
fi
if ! mv "$STAGE" "$RELEASE_DIR"; then
  [ ! -e "$BACKUP_DIR" ] || mv "$BACKUP_DIR" "$RELEASE_DIR"
  exit 1
fi

PREVIOUS=""
if [ -L "$CURRENT" ]; then
  PREVIOUS="$(readlink -f "$CURRENT" || true)"
fi
ln -sfn "$RELEASE_DIR" "${CURRENT}.new"
mv -Tf "${CURRENT}.new" "$CURRENT"

cat > "${BIN_DIR}/nirunote" <<'EOF'
#!/usr/bin/env bash
set -u
APP_HOME="${HOME}/.local/share/nirunote/current"
STATE="${HOME}/.local/state/nirunote"
mkdir -p "$STATE"
LOG="$STATE/launcher.log"
if [ ! -f "$APP_HOME/nirunote/app.py" ]; then
  printf '%s ERROR: active NIRUNOTE release is missing\n' "$(date '+%F %T')" >>"$LOG"
  exit 1
fi
exec python3 "$APP_HOME/nirunote/app.py" "$@" 2>>"$LOG"
EOF
chmod +x "${BIN_DIR}/nirunote"

sed   -e "s|^Exec=.*|Exec=${BIN_DIR}/nirunote %F|"   -e "s|^TryExec=.*|TryExec=${BIN_DIR}/nirunote|"   -e "s|^StartupNotify=.*|StartupNotify=false|"   "$RELEASE_DIR/nirunote.desktop" > "${DESKTOP_DIR}/nirunote.desktop"

if [ -f "$RELEASE_DIR/assets/nirunote.svg" ]; then
  cp "$RELEASE_DIR/assets/nirunote.svg" "${ICON_DIR}/nirunote.svg"
fi

command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$DESKTOP_DIR" >/dev/null 2>&1 || true
command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -f -t "${HOME}/.local/share/icons/hicolor" >/dev/null 2>&1 || true

# Final non-GUI checks through the actual installed entrypoint.
if ! "${BIN_DIR}/nirunote" --version >/dev/null 2>&1; then
  echo "ERROR: installed entrypoint failed." >&2
  if [ -n "$PREVIOUS" ] && [ -d "$PREVIOUS" ]; then
    ln -sfn "$PREVIOUS" "${CURRENT}.rollback"
    mv -Tf "${CURRENT}.rollback" "$CURRENT"
    echo "Rolled back to: $PREVIOUS"
  fi
  if [ -d "${RELEASE_DIR}.previous" ]; then
    rm -rf "$RELEASE_DIR"
    mv "${RELEASE_DIR}.previous" "$RELEASE_DIR"
  fi
  exit 1
fi

echo
echo "NIRUNOTE ${VERSION} installed successfully."
echo "Launch NIRUNOTE from your application launcher."
echo "Log: ${STATE_DIR}/launcher.log"

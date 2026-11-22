#!/bin/sh
# X11 end-to-end checks in a private Xvfb: first frame, memory and idle CPU,
# editing and saving, undo, recovery after a crash, find and replace, the
# Markdown helpers, Preview, links and Back, Quick Open, Outline, the
# read-only viewer with --heading, the scratchpad, settings in the Qt file,
# the clipboard and the unsaved-changes prompt.
# Usage: tests/xvfb.sh PATH/TO/nirunote [DISPLAY_NUMBER]
set -eu
BIN=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
SB=$(mktemp -d)
PIDS=""
cleanup() { for p in $PIDS; do kill "$p" 2>/dev/null || true; done; rm -rf "$SB"; }
trap cleanup EXIT INT TERM
export HOME="$SB/home" XDG_CONFIG_HOME="$SB/config" XDG_STATE_HOME="$SB/state"
export XDG_CACHE_HOME="$SB/cache" XDG_DATA_HOME="$SB/data"
mkdir -p "$HOME/Documents" "$XDG_CONFIG_HOME" "$XDG_STATE_HOME" "$XDG_CACHE_HOME"
HERE=$(cd "$(dirname "$0")" && pwd)
cp "$HERE/fixtures/showcase.md" "$HOME/Documents/showcase.md"
D=:${2:-$((70 + $$ % 20))}
Xvfb "$D" -screen 0 1400x900x24 -nolisten tcp >/dev/null 2>&1 &
PIDS="$PIDS $!"
export DISPLAY="$D"
sleep 1
ST="$SB/state.json"
fail() { echo "FAIL: $*" >&2; [ -f "$ST" ] && cat "$ST" >&2; exit 1; }
j() {
    python3 -c 'import json,sys
v = json.load(open(sys.argv[1]))[sys.argv[2]]
print(str(v).lower() if isinstance(v, bool) else v)' "$ST" "$1"
}
k() { xdotool key --delay 50 "$@"; sleep 0.4; }
t() { xdotool type --delay 25 -- "$1"; sleep 0.4; }
P=""
start() {
    rm -f "$ST"
    NIRUNOTE_STATE="$ST" NIRUNOTE_TIMING=1 "$BIN" "$@" 2>"$SB/err" &
    P=$!
    PIDS="$PIDS $P"
    n=0
    while [ ! -f "$ST" ]; do sleep 0.05; n=$((n + 1)); [ $n -lt 100 ] || fail "no window"; done
    xdotool mousemove 500 400
    sleep 0.4
}
quit() { k ctrl+q; n=0; while kill -0 "$P" 2>/dev/null; do sleep 0.1; n=$((n + 1)); [ $n -lt 30 ] || fail "did not quit"; done; }
DOC="$HOME/Documents"

# --- Start-up: first frame, memory, idle CPU, window class ----------------
start "$DOC/showcase.md"
ms=$(sed -n 's/.*after \([0-9]*\)\..*/\1/p' "$SB/err")
[ -n "$ms" ] || fail "no first frame"
[ "$ms" -lt 1000 ] || fail "first frame took $ms ms"
rss=$(awk '/VmRSS/ {print $2}' /proc/$P/status)
[ "$rss" -lt 60000 ] || fail "memory $rss kB"
s1=$(awk '{print $14 + $15}' /proc/$P/stat); sleep 3; s2=$(awk '{print $14 + $15}' /proc/$P/stat)
[ $((s2 - s1)) -le 2 ] || fail "idle CPU: $((s2 - s1)) ticks in 3 s"
W=$(xdotool search --class nirunote | head -1)
xprop -id "$W" WM_CLASS | grep -q '"nirunote", "NIRUNOTE"' || fail "WM_CLASS"
[ "$(j modified)" = false ] || fail "opening a file marked it modified"
[ ! -e "$XDG_STATE_HOME/nirunote/recovery.json" ] || fail "recovery written for an unchanged file"
echo "first frame ${ms} ms, ${rss} kB, idle $((s2 - s1)) ticks/3 s"
# Preview and Outline.
k ctrl+shift+e
[ "$(j previewing)" = true ] || fail "Preview"
[ "$(j headings)" -ge 20 ] || fail "headings in the Preview"
k ctrl+shift+e
k ctrl+shift+o
t "Lists"
k Return
[ "$(j overlay)" = "" ] || fail "outline stays open"
line=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["cursor"][0])' "$ST")
grep -n '^## Lists' "$DOC/showcase.md" | grep -q "^$((line + 1)):" || fail "outline jumped to line $line"
k ctrl+plus
grep -q '^font_size=18$' "$XDG_CONFIG_HOME/NIRU/NIRUNOTE.conf" || fail "font size not in NIRUNOTE.conf"
grep -q '^\[General\]$' "$XDG_CONFIG_HOME/NIRU/NIRUNOTE.conf" || fail "not a Qt settings file"
k ctrl+0
quit
grep -q '^geometry=@ByteArray(' "$XDG_CONFIG_HOME/NIRU/NIRUNOTE.conf" || fail "geometry"
grep -q '^\[cursor\]$' "$XDG_CONFIG_HOME/NIRU/NIRUNOTE.conf" || fail "cursor position"
echo "preview, outline, settings ok"

# --- A new note: typing, lists, Save As, undo to the saved state ----------
start --new
t "# Hello world"
k Return
t "- one"
k Return
t "two"
[ "$(j text)" = "$(printf '# Hello world\n- one\n- two')" ] || fail "list continuation"
[ "$(j modified)" = true ] || fail "not modified"
k ctrl+s
case "$(j overlay)" in chooser:Save*) ;; *) fail "no Save As";; esac
t "notes"
k Return
[ -f "$DOC/notes.md" ] || fail "Save As did not write notes.md"
[ "$(cat "$DOC/notes.md")" = "$(printf '# Hello world\n- one\n- two')" ] || fail "saved text"
[ "$(j modified)" = false ] || fail "modified after save"
[ "$(j title)" = notes.md ] || fail "title"
t " x"
sleep 2.6
grep -q '"text"' "$XDG_STATE_HOME/nirunote/recovery.json" || fail "no recovery snapshot"
k ctrl+z
[ "$(j modified)" = false ] || fail "undo to the saved text"
[ ! -e "$XDG_STATE_HOME/nirunote/recovery.json" ] || fail "recovery kept after undo"
k ctrl+shift+z
[ "$(j modified)" = true ] || fail "redo"
k ctrl+z
# Find and replace: Tab through the bar to All.
k ctrl+h
t "two"
k Tab
t "2"
k Tab Tab Tab Tab Tab Tab Return
case "$(j text)" in *"- 2"*) ;; *) fail "replace all";; esac
k Escape
[ "$(j find)" = false ] || fail "Esc closes the find bar"
k ctrl+End
k Return Return
k ctrl+b
t "b"
k End Return
k ctrl+shift+8
t "item"
case "$(j text)" in *"**b**"*"- item"*) ;; *) fail "Markdown helpers";; esac
k ctrl+s
[ "$(j modified)" = false ] || fail "save"
# The clipboard, both ways.
if command -v xclip >/dev/null 2>&1; then
    k ctrl+a ctrl+c
    [ "$(xclip -o -selection clipboard)" = "$(cat "$DOC/notes.md")" ] || fail "copy"
    printf 'pasted text' | xclip -loops 1 -selection clipboard
    k ctrl+End
    k ctrl+v
    case "$(j text)" in *"pasted text") ;; *) fail "paste";; esac
    k ctrl+z
    echo "clipboard ok"
fi
# Quick Open.
k ctrl+p
t "showcase"
k Return
[ "$(j title)" = showcase.md ] || fail "quick open"
# An external change reloads an unmodified file.
printf '# Changed outside\n' > "$DOC/showcase.md"
sleep 0.8
[ "$(j text)" = "# Changed outside" ] || fail "external reload"
cp "$HERE/fixtures/showcase.md" "$DOC/showcase.md"
sleep 0.6
# The unsaved prompt: Cancel keeps the window, Don't Save backs the text up.
t "zzz"
k ctrl+q
[ "$(j overlay)" = "form:Unsaved changes" ] || fail "no unsaved prompt"
k Escape
kill -0 "$P" || fail "Cancel closed the window"
k ctrl+q
k Left Return
n=0; while kill -0 "$P" 2>/dev/null; do sleep 0.1; n=$((n + 1)); [ $n -lt 30 ] || fail "Don't Save"; done
grep -q zzz "$XDG_STATE_HOME/nirunote/recovery-discarded.json" || fail "discarded text not kept"
echo "editing, saving, find, clipboard, quick open, prompts ok"

# --- Crash recovery: kill -9, Restore ---------------------------------------
start "$DOC/notes.md"
k ctrl+End
t " unsaved"
sleep 2.6
kill -9 "$P"
sleep 0.3
start "$DOC/notes.md"
[ "$(j overlay)" = "form:Recover unsaved text" ] || fail "no recovery prompt"
k Return
case "$(j text)" in *" unsaved") ;; *) fail "not restored";; esac
case "$(j status_left)" in *RECOVERED*) ;; *) fail "RECOVERED missing";; esac
grep -q unsaved "$DOC/notes.md" && fail "restore wrote the file"
k ctrl+s
quit
grep -q unsaved "$DOC/notes.md" || fail "save after restore"
echo "recovery ok"

# --- Links and Back in the Preview -------------------------------------------
printf '# A\n\nGo to [b](b.md).\n' > "$DOC/a.md"
printf '# B\n\nBack.\n' > "$DOC/b.md"
start "$DOC/a.md" --preview
k Tab Return
[ "$(j title)" = b.md ] || fail "link"
k alt+Left
[ "$(j title)" = a.md ] || fail "Back"
quit
echo "links ok"

# --- The read-only viewer at a heading, and the scratchpad -------------------
start "$DOC/showcase.md" --read-only --heading "Lists"
[ "$(j read_only)" = true ] && [ "$(j previewing)" = true ] || fail "viewer"
[ "$(j preview_scroll)" -gt 0 ] || fail "--heading"
W=$(xdotool search --class nirunote | head -1)
xprop -id "$W" _NET_WM_NAME | grep -q 'read-only' || fail "viewer title"
k ctrl+s
[ "$(j status_right)" = "Read-only viewer: saving is off" ] || fail "viewer saved"
k ctrl+shift+e
t "nope"
cmp -s "$DOC/showcase.md" "$HERE/fixtures/showcase.md" || fail "viewer changed the file"
[ "$(j modified)" = false ] || fail "viewer edited"
quit
start --scratchpad
[ "$(j path)" = "$XDG_STATE_HOME/nirunote/scratchpad.md" ] || fail "scratchpad path"
t "scratch note"
k ctrl+s
[ "$(cat "$XDG_STATE_HOME/nirunote/scratchpad.md")" = "scratch note" ] || fail "scratchpad save"
quit
echo "viewer and scratchpad ok"

# --- Spelling through Hunspell (a stand-in that knows one wrong word) -------
mkdir -p "$SB/bin" "$HOME/.local/share/nirunote/dictionaries"
cat > "$SB/bin/hunspell" <<'H'
#!/bin/sh
mode=$3
while IFS= read -r w; do
    case "$mode" in
        -l) case "$w" in asdasdasd | wrongg) echo "$w" ;; esac ;;
        -a) case "$w" in wrongg) echo "& wrongg 1 0: wrong" ;; *) echo "*" ;; esac ;;
    esac
done
H
chmod +x "$SB/bin/hunspell"
: > "$HOME/.local/share/nirunote/dictionaries/sv_SE.dic"
: > "$HOME/.local/share/nirunote/dictionaries/sv_SE.aff"
export PATH="$SB/bin:$PATH"
start --new
sleep 0.5
[ "$(j spell)" = "Spelling: sv" ] || fail "spelling status"
t "Hej wrongg"
sleep 0.8
k Left Left shift+F10 Down Return
[ "$(j text)" = "Hej wrong" ] || fail "spelling suggestion"
k ctrl+q
k Left Return
echo "spelling ok"
echo "PASS"

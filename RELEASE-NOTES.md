# NIRUNOTE 0.6.1

Installer fixes found by clean installs on fresh Arch, Void and Debian systems.

- Unattended installs work: `./install.sh --yes` (or `NIRUNOTE_ASSUME_YES=1`, or `NIRUX_ASSUME_YES=1`, which `nirux extras install` sets without a terminal) installs Python 3 and PySide6 without a prompt. Before, pacman's question was answered by an empty stdin and the install failed.
- Void Linux: the installer installs `python3-pyside6`, `python3-pyside6-gui` and `python3-pyside6-widgets` with `xbps-install` instead of stopping with "Unsupported package manager".
- Installing the same version a third time no longer stops at an existing `.previous` backup.

No application, document or settings changes.

# NIRUNOTE 0.6.0

Correctness, energy and maintainability release built from 0.5.0.

- Opening a file no longer shows it as modified, and the unsaved-changes prompt only appears after real edits. Undo back to the saved text clears the ● marker.
- Recovery snapshots are written only after real changes (about 2 s after typing stops, at most every 10 s while typing) instead of every 2.5 s. After a crash NIRUNOTE asks: Restore, Discard (keeps one backup) or Compare (side-by-side diff).
- An idle window no longer wakes up: 0.0 % CPU and no timer events unfocused; only the caret blinks when focused.
- The status line says "Spelling: sv", "Spelling: off" or "Spelling: unavailable (install hunspell-sv)"; click it to open the spelling settings.
- New heading outline (Ctrl+Shift+O) with type-to-filter, and fenced code blocks highlighted in the editor.
- Internals: readable source split into modules, regression tests, CI on Python 3.12, 3.13 and 3.14.

No document or settings format changes. Restart a running NIRUNOTE after updating.

## Validation limitations

Native checks on niruxwork (second instance with a temporary HOME, CPU/wakeups on real hardware, kill -9 recovery and Follow NIRUX after a theme change) are part of the NIRUX rollout acceptance. Hunspell runtime validation depends on the installed dictionary.

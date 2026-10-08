# NIRUNOTE

Minimal, keyboard-oriented Markdown editor for Linux, built with Python and PySide6.
Designed for personal desktop workflows, especially Arch/Omarchy and Debian.

**Version:** 0.4.2 — stabilization release. The project is a personal application, shared as-is.

## Features

- Markdown editing and styled Preview, with safe limited inline HTML (`small`, `br`, `kbd`, `sub`, `sup`, `mark`).
- Dark, light and Satie themes; focus mode and configurable text width.
- Find/replace, headings navigation, recent files, command palette.
- Local spell checking with Hunspell when installed and a dictionary is available.
- Recovery state, atomic saves, external file-change notification.

## Install

Dependencies: Python 3, PySide6 (Qt Widgets). Optional: Hunspell.

```bash
./install.sh
nirunote
```

The installer targets `~/.local/share/nirunote` and `~/.local/bin` and checks startup before activating the release. It may use your distribution's package manager for missing PySide6 dependencies. The optional Swedish spelling dictionary is downloaded to the user-local dictionary directory. Review `install.sh` before running it.

For a source-only run (with PySide6 installed):

```bash
python3 nirunote/app.py
```

## Tests

```bash
python3 -m compileall -q nirunote gui-smoke-test.py
python3 nirunote/app.py --core-self-test
QT_QPA_PLATFORM=offscreen python3 gui-smoke-test.py
```

The offscreen smoke test does not substitute for real Wayland/Hyprland GUI testing. In particular, test Preview scrolling through long documents before publishing a release.

## Data and privacy

Documents remain local; NIRUNOTE does not upload document content. Hunspell uses a local subprocess. Installer may fetch a spelling dictionary. Settings and recovery state use XDG user directories. Review the installation script and third-party dictionary license.

## Uninstall

Run `./uninstall.sh` and review the script first. Back up your documents and recovery state independently.

## Author and license

Ing Leif Nicklas Rudolfsson. MIT License; see [LICENSE](LICENSE).

## 0.7.0 (2026-11-10): A complete Preview and a read-only viewer

- New Markdown renderer for the Preview (no new dependency): CommonMark block
  structure (nested lists and quotes, loose and tight lists, ordered start
  numbers, setext headings, indented code, link reference definitions,
  entities, escapes, hard breaks) plus GitHub tables with alignment, task
  lists, strikethrough, autolinks, footnotes and alerts (`> [!NOTE]` and
  friends). Headings get GitHub-style anchors.
- Fenced code is highlighted with one cheap regex pass per block for common
  languages; colours come from the theme (Follow NIRUX uses the terminal
  palette). Very large documents skip it to save memory.
- Images: local files are decoded at most at the page width (32 MB limit);
  http(s) images are fetched in the background only while the Preview shows
  them (8 MB, 10 s limit). Preferences: "Remote images in Preview". Missing
  images show their alt text. SVG works when QtSvg is installed.
- Links: `#anchors` scroll, relative Markdown links open in the same window,
  `Alt+Left` goes back. javascript:, data: and file: links stay blocked.
- Long lines, long URLs and wide tables wrap instead of widening the page.
- `--read-only`: a viewer that starts in the Preview, shows a READ ONLY badge
  and "Viewer (read-only)", and cannot edit, save or write recovery or
  recent-file state. Changes on disk reload at once.
- `--heading TEXT` opens at a heading (title, part of it, or anchor slug);
  `--preview` starts in the Preview; `--help` lists the options.
- Find works in the Preview; `F3` / `Shift+F3`; `Esc` closes the find bar.
  The outline and Go to heading jump inside the Preview.
- `Ctrl`+wheel zooms the Preview as well.
- Lighter: the Preview is not rendered again when nothing changed; zooming
  re-lays out without re-rendering; the source highlighter caches its formats
  and, in the viewer, runs only when the source is shown; word counts on big
  documents wait for a pause in typing. Startup on a 1.9 MB file: 1.8 s to
  0.85 s.
- Formatting shortcuts no longer change the hidden source while the Preview
  is shown.

## 0.6.1 — 2026-11-07 — Installer for fresh systems

- `./install.sh --yes`, `NIRUNOTE_ASSUME_YES=1` or `NIRUX_ASSUME_YES=1` installs missing packages without a prompt (unattended `nirux extras install`).
- Void Linux dependencies through `xbps-install`.
- A repeated same-version install replaces the old `.previous` backup instead of refusing.

## 0.4.6 — 2026-10-10 — Robust Qt smoke tests

- Remove invalid offscreen initial-focus assumption.
- Ensure modal dialog closes on test failures to avoid installer timeouts.
- Keep keyboard action checks and safe Cancel behavior unchanged.

## 0.4.5 — 2026-10-10 — Cross-distribution GUI validation

- Remove the optional PySide6.QtTest dependency from installation smoke tests.
- Use native Qt key events to test Enter, Return, Space, Tab and Escape.
- Preserve the existing 0.4.4 dialog and theme behavior.

## 0.4.4 — 2026-10-10 — Keyboard accessibility

- Restore auto-default button behavior for keyboard activation of the unsaved changes dialog.
- Keep Cancel as initial default and Escape action; neutral theme styling unchanged.
- Add keyboard regression coverage for Enter, Space, Tab and Escape.

## 0.4.3 — 2026-10-10 — Dialog reliability

- Explicit unsaved-changes dialog outcomes across Qt platform styles; Escape cancels.
- Dialog buttons inherit the selected NIRUNOTE theme without native destructive-button coloring.

## 0.4.2 — 2026-10-08 — CI and release hardening

- Fixed Qt CI dependencies and pinned Ubuntu runner to 24.04.
- Updated GitHub Actions checkout/setup-python versions to avoid Node.js 20 deprecation.
- Added release preflight for version consistency and trailing whitespace.
- Added guarded GitHub Release packaging from the exact CI-tested commit.
- Kept existing editing behavior, install paths, and user data unchanged.

## 0.4.1 — GitHub publishing and preview regression tests

- Publish from a fresh clone of existing GitHub `main`, preserving remote history and avoiding unrelated local repositories.
- Stop safely if remote changes during publishing; never force-push.
- Added long-document Preview scroll-range and stable-refresh GUI regression checks.
- Kept existing application features, installation layout and user data handling unchanged.

# Changelog

## 0.6.0 — 2026-11-05

- Fix: opening a file (command line, Open, Recent, Quick Open, scratchpad,
  external reload) no longer marks it modified. Root cause: highlighter
  rehighlights emit textChanged, and the spell timer re-armed itself in a
  loop.
- Recovery is written only when the text changed: 2 s after the last edit,
  at most every 10 s while typing; nothing while idle. A crash now shows
  Restore / Discard / Compare with file, time and size difference.
- Idle CPU: no timers while idle (0.5.0: about 13–20 % CPU in Xvfb).
- Dirty state follows QTextDocument.isModified(): Undo back to the saved
  text clears it; Replace All is one undoable edit.
- Readable spelling status ("Spelling: sv", "Spelling: off",
  "Spelling: unavailable (install hunspell-sv)"), clickable; system
  hunspell dictionaries are found too.
- Heading outline popup (Ctrl+Shift+O) with type-to-filter; Go to heading
  jumps correctly with wrapped lines.
- Fenced code blocks are highlighted in the editor (monospace, subtle
  background).
- Code: reformatted with ruff and split into modules; regression tests and a
  Python 3.12/3.13/3.14 CI matrix.

## 0.5.0 — 2026-10-28

- Extract the Markdown renderer and Hunspell dictionary into tested modules,
  keeping the app CLI, installer layout and editor behavior unchanged.
- Improve the status line with saved/modified state, NIRUX theme label and
  recovery notice; make Quick Open cancellable and give editor, preview,
  status and dialogs accessible names.
- Restrict Preview links to http, https and mailto, escape link attributes and
  validate anchor clicks before opening them externally.
- Move spell-dictionary probing, Quick Open scanning, recovery writes and large
  Preview rendering off the GUI thread, with cancellation-by-generation and
  explicit large-document load limits.
- Added conditional Follow NIRUX detection and live theme updates without
  changing the default on systems where NIRUX is absent.
- Centralized theme palette resolution; unknown theme names now fall back to
  dark without silently rewriting the saved preference.
## 0.4.0 — 2026-10-08

- Based on 0.3.11, retaining editor and Preview features.
- Hunspell batch checks run in a background worker; cached results update highlighting on the Qt thread.
- Save As restores the original document path/title on failed save.
- Installer retains the previous same-version release as `.previous` rather than silently deleting it.
- Added GitHub publication script, MIT license, ignore rules, release documentation and CI.
- Preview scrolling remains a mandatory on-device release verification; no claim of a GUI fix without testing.

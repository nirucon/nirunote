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

## Unreleased

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

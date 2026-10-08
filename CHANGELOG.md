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

## 0.4.0 — 2026-10-08

- Based on 0.3.11, retaining editor and Preview features.
- Hunspell batch checks run in a background worker; cached results update highlighting on the Qt thread.
- Save As restores the original document path/title on failed save.
- Installer retains the previous same-version release as `.previous` rather than silently deleting it.
- Added GitHub publication script, MIT license, ignore rules, release documentation and CI.
- Preview scrolling remains a mandatory on-device release verification; no claim of a GUI fix without testing.

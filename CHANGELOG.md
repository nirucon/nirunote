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

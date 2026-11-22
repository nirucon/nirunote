# NIRUNOTE 1.0.0 release contract

- Baseline: 0.7.1 (Python). No document format change. Settings keys, defaults and the Qt INI format of `~/.config/NIRU/NIRUNOTE.conf` are unchanged and merged on write like QSettings.
- Recovery JSON keeps `text`, `path`, `saved_at`, `app_version` and `pid`; 0.x snapshots are read. The scratchpad uses `scratchpad-recovery.json`; discarded text is kept once in `recovery-discarded.json`.
- The command line, WM_CLASS `nirunote` and `nirunote --version` (last word is the version) are unchanged: NIRUX's manual viewer, the scratchpad slot and `nirux extras` depend on them.
- `./install.sh` installs the prebuilt static binary of the tag, verified against `SHA256SUMS`; it replaces a 0.x installation and keeps user data.
- Gates: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, `tests/xvfb.sh`, and the golden comparison against 0.7's renderer.
- The GitHub release is created by the tag workflow after CI passed for the exact main commit.
- MIT license.

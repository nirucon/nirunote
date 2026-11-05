# NIRUNOTE 0.6.0 release contract

- Baseline: 0.5.0. No document format change. Settings keys and defaults are unchanged.
- Recovery JSON keeps `text` and `path`; 0.6.0 adds `saved_at`, `app_version` and `pid`, which 0.5.0 ignores. 0.6.0 reads 0.5.0 snapshots. The scratchpad instance uses `scratchpad-recovery.json`; discarded text is kept once in `recovery-discarded.json`.
- Install under `~/.local/share/nirunote/releases/0.6.0` using `./install.sh`.
- Existing user documents, configuration and recovery data must remain intact.
- `scripts/publish-github.sh` clones current remote `main` into a temporary directory; no force push and no unrelated-history merge.
- Run GUI smoke test, regression tests and, on target Linux, long-document Preview scrolling before publishing.
- MIT license.

- GitHub Release must be created only after CI success for the exact main commit.

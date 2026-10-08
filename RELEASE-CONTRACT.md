# NIRUNOTE 0.4.1 release contract

- Baseline: 0.4.0; no document format or configuration migration.
- Install under `~/.local/share/nirunote/releases/0.4.1` using `./install.sh`.
- Existing user documents, configuration and recovery data must remain intact.
- `scripts/publish-github.sh` clones current remote `main` into a temporary directory; no force push and no unrelated-history merge.
- Run GUI smoke test, including long-document Preview scrolling, on target Linux before publishing.
- MIT license.

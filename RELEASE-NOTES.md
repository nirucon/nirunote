# NIRUNOTE 0.4.2

Maintenance and release reliability update, based on NIRUNOTE 0.4.1.

- Fix GitHub Actions Qt library dependencies and pin the runner to Ubuntu 24.04.
- Update GitHub Actions to Node.js 24-compatible checkout/setup-python versions.
- Add release integrity checks for consistent versions and trailing whitespace.
- Keep GitHub publication history-safe: clone remote main, run tests, review staged changes, never force-push.
- Add an opt-in GitHub Release script that requires successful CI for the exact published commit and packages that commit.
- Preserve the editor's existing features, configuration, documents and install layout.

Install: extract `NIRUNOTE-0.4.2.zip`, enter `nirunote-0.4.2`, and run `./install.sh`.

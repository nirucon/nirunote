#!/usr/bin/env bash
set -Eeuo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
REMOTE="https://github.com/nirucon/nirunote.git"
cd "$ROOT"
VERSION="$(cat VERSION)"
[[ "$VERSION" == "0.4.5" ]] || { echo 'Version mismatch' >&2; exit 1; }
for cmd in git python3 mktemp cp; do command -v "$cmd" >/dev/null || { echo "Missing: $cmd" >&2; exit 1; }; done
python3 scripts/release-check.py
python3 -m compileall -q nirunote gui-smoke-test.py
python3 nirunote/app.py --core-self-test
python3 -c 'import PySide6' || { echo 'PySide6 required for GUI verification' >&2; exit 1; }
QT_QPA_PLATFORM=offscreen python3 gui-smoke-test.py
WORK="$(mktemp -d -t nirunote-publish-XXXXXXXX)"
trap 'rm -rf -- "$WORK"' EXIT
git clone --branch main --single-branch "$REMOTE" "$WORK/repo"
cd "$WORK/repo"
[[ -z "$(git status --porcelain)" ]] || { echo 'Unexpected dirty clone' >&2; exit 1; }
FILES=(RELEASE-NOTES.md README.md CHANGELOG.md RELEASE-CONTRACT.md LICENSE VERSION .gitignore install.sh uninstall.sh run.sh gui-smoke-test.py nirunote.desktop)
for f in "${FILES[@]}"; do cp -- "$ROOT/$f" "$f"; done
for dir in nirunote assets scripts .github; do
  mkdir -p "$dir"
  cp -a "$ROOT/$dir/." "$dir/"
done
find nirunote scripts -type d -name __pycache__ -prune -exec rm -rf -- {} +
find . -name '*.pyc' -type f -delete
python3 scripts/release-check.py
git add -- "${FILES[@]}" nirunote assets scripts .github
git diff --cached --check
if git diff --cached --quiet; then echo 'No changes to publish'; exit 0; fi
git diff --cached --stat
read -r -p "Commit and push NIRUNOTE $VERSION to GitHub? [yes/NO] " ok
[[ "$ok" == yes ]] || { echo 'Cancelled'; exit 1; }
git -c user.name="${GIT_AUTHOR_NAME:-$(git config user.name)}" -c user.email="${GIT_AUTHOR_EMAIL:-$(git config user.email)}" commit -m "Release NIRUNOTE $VERSION"
git push origin HEAD:main
echo "Published NIRUNOTE $VERSION to GitHub main."
echo 'Wait for GitHub Actions to pass before creating a tagged GitHub Release.'

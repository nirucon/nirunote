#!/usr/bin/env bash
set -Eeuo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
REMOTE="https://github.com/nirucon/nirunote.git"
command -v git >/dev/null || { echo "git required"; exit 1; }
[ -f VERSION ] && [ "$(cat VERSION)" = "0.4.0" ] || { echo "Wrong version"; exit 1; }
if [ ! -d .git ]; then
  git init -b main
fi
BRANCH="$(git branch --show-current)"
[ "$BRANCH" = main ] || { echo "Expected main branch, got: $BRANCH"; exit 1; }
if git remote get-url origin >/dev/null 2>&1; then
  [ "$(git remote get-url origin)" = "$REMOTE" ] || { echo "origin points elsewhere; refusing"; exit 1; }
else
  git remote add origin "$REMOTE"
fi
python3 -m compileall -q nirunote gui-smoke-test.py
python3 nirunote/app.py --core-self-test
if python3 -c 'import PySide6' >/dev/null 2>&1; then
  QT_QPA_PLATFORM=offscreen python3 gui-smoke-test.py
else
  echo 'PySide6 missing; GUI smoke test unavailable. Refusing public push.' >&2
  exit 1
fi
git add README.md CHANGELOG.md RELEASE-CONTRACT.md LICENSE VERSION .gitignore install.sh uninstall.sh run.sh gui-smoke-test.py nirunote.desktop nirunote assets scripts .github
if ! git diff --cached --quiet; then
  git diff --cached --check
  git diff --cached --stat
  read -r -p 'Commit reviewed files? [yes/NO] ' ok
  [ "$ok" = yes ] || exit 1
  git commit -m 'Release NIRUNOTE 0.4.0'
fi
git fetch origin main 2>/dev/null || true
if git show-ref --verify --quiet refs/remotes/origin/main; then
  git merge-base --is-ancestor origin/main HEAD || { echo 'Remote has changes not in local history; refusing push'; exit 1; }
fi
read -r -p 'Push main to nirucon/nirunote? [yes/NO] ' ok
[ "$ok" = yes ] || exit 1
git push -u origin main

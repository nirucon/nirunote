#!/usr/bin/env bash
set -Eeuo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
VERSION="$(cat VERSION)"
TAG="v$VERSION"
for cmd in gh git python3 zip; do command -v "$cmd" >/dev/null || { echo "Missing: $cmd" >&2; exit 1; }; done
python3 scripts/release-check.py
# Only publish from the exact same revision as GitHub main.
REMOTE_SHA="$(gh api repos/nirucon/nirunote/commits/main --jq .sha)"
REMOTE_VERSION="$(gh api repos/nirucon/nirunote/contents/VERSION --jq .content | base64 -d | tr -d '\r\n')"
[[ "$REMOTE_VERSION" == "$VERSION" ]] || { echo "Remote main is $REMOTE_VERSION, not $VERSION" >&2; exit 1; }
if gh release view "$TAG" --repo nirucon/nirunote >/dev/null 2>&1; then
  echo "Release $TAG already exists. No changes made."; exit 0
fi
if gh api "repos/nirucon/nirunote/git/ref/tags/$TAG" >/dev/null 2>&1; then
  echo "Tag $TAG already exists; review before publishing." >&2; exit 1
fi
# Require a successful CI run for the current main SHA.
RUN_JSON="$(gh run list --repo nirucon/nirunote --commit "$REMOTE_SHA" --workflow 'Python checks' --limit 10 --json databaseId,status,conclusion,headSha)"
RUN_ID="$(printf '%s' "$RUN_JSON" | python3 -c 'import json,sys; a=json.load(sys.stdin); a=[r for r in a if r["status"]=="completed" and r["conclusion"]=="success"]; print(a[0]["databaseId"] if a else "")')"
[[ -n "$RUN_ID" ]] || { echo "No successful GitHub Actions run for main $REMOTE_SHA" >&2; exit 1; }
ARCHIVE="$(mktemp -d -t nirunote-release-XXXXXXXX)"
trap 'rm -rf -- "$ARCHIVE"' EXIT
# Export exactly the verified GitHub commit, not an arbitrary local source tree.
gh api "repos/nirucon/nirunote/zipball/$REMOTE_SHA" > "$ARCHIVE/source.zip"
python3 - "$ARCHIVE/source.zip" "$ARCHIVE/NIRUNOTE-$VERSION.zip" "$VERSION" <<'PY2'
import sys,zipfile
source,target,version=sys.argv[1:]
with zipfile.ZipFile(source) as src,zipfile.ZipFile(target,'w',zipfile.ZIP_DEFLATED) as out:
    for info in src.infolist():
        if info.is_dir(): continue
        rel=info.filename.split('/',1)[-1]
        if not rel or rel.startswith('.git/'): continue
        dest=f'nirunote-{version}/{rel}'
        new=zipfile.ZipInfo(dest)
        new.external_attr=info.external_attr
        new.compress_type=zipfile.ZIP_DEFLATED
        out.writestr(new,src.read(info))
PY2
read -r -p "Create GitHub Release $TAG from successful run $RUN_ID? [yes/NO] " ok
[[ "$ok" == yes ]] || { echo 'Cancelled'; exit 1; }
gh release create "$TAG" "$ARCHIVE/NIRUNOTE-$VERSION.zip" --repo nirucon/nirunote --target "$REMOTE_SHA" --title "NIRUNOTE $VERSION" --notes-file RELEASE-NOTES.md
echo "GitHub Release $TAG created."

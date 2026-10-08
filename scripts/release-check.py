#!/usr/bin/env python3
"""Release integrity checks: no GUI or network required."""
from pathlib import Path
import re, sys
root = Path(__file__).resolve().parent.parent
version = (root / 'VERSION').read_text().strip()
checks = {
    'nirunote/app.py': r"VERSION='([^']+)'",
    'install.sh': r'^VERSION="([^"]+)"',
    'README.md': r'\*\*Version:\*\* ([0-9.]+)',
    'RELEASE-CONTRACT.md': r'^# NIRUNOTE ([0-9.]+)',
}
errors=[]
for filename,pattern in checks.items():
    match=re.search(pattern,(root/filename).read_text(),re.M)
    if not match or match.group(1)!=version:
        errors.append(f'{filename}: expected {version}, got {match.group(1) if match else "missing"}')
for folder in ('nirunote','scripts'):
    for path in (root/folder).rglob('*'):
        if path.is_file() and path.suffix in ('.py','.sh'):
            for line_no,line in enumerate(path.read_text().splitlines(),1):
                if line.endswith((' ','\t')):
                    errors.append(f'{path.relative_to(root)}:{line_no}: trailing whitespace')
if errors:
    print('Release preflight FAILED:\n'+'\n'.join(errors),file=sys.stderr)
    sys.exit(1)
print(f'NIRUNOTE {version} release preflight: OK')

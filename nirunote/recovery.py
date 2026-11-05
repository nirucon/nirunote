"""Recovery snapshot helpers: reading, describing and diffing (no Qt)."""
from __future__ import annotations

import difflib
import json
import os
import time
from pathlib import Path


def read_snapshot(path):
    """Return the recovery snapshot at path as a dict, or None.

    Raises ValueError for a file that exists but cannot be parsed, so the
    caller can move it aside instead of overwriting it.
    """
    path = Path(path)
    try:
        if not path.is_file() or not path.stat().st_size:
            return None
        data = json.loads(path.read_text(encoding='utf-8'))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ValueError(str(error)) from error
    if not isinstance(data, dict) or not isinstance(data.get('text', ''), str):
        raise ValueError('unexpected recovery format')
    if not data.get('text'):
        return None
    if not isinstance(data.get('saved_at'), (int, float)):
        # 0.5.0 snapshots carry no timestamp; the file time is the write time.
        data['saved_at'] = path.stat().st_mtime
    return data


def owner_alive(data, own_pid=None):
    """True when the snapshot was written by another NIRUNOTE that still runs."""
    pid = data.get('pid')
    if not isinstance(pid, int) or pid <= 0 or pid == (own_pid or os.getpid()):
        return False
    try:
        cmdline = Path(f'/proc/{pid}/cmdline').read_bytes()
    except OSError:
        return False
    return b'nirunote' in cmdline.lower()


def disk_text(path):
    """Text of the original file, or None when it is missing or unreadable."""
    if not path:
        return None
    try:
        return Path(path).read_bytes().decode('utf-8-sig').replace('\r\n', '\n').replace('\r', '\n')
    except (OSError, UnicodeDecodeError):
        return None


def describe(data, on_disk):
    """Return (file label, time label, size label) for the recovery prompt."""
    path = data.get('path')
    name = Path(path).name if path else 'Untitled (never saved)'
    when = time.strftime('%Y-%m-%d %H:%M', time.localtime(data['saved_at']))
    chars = len(data['text'])
    if on_disk is None:
        size = f'{chars} characters' + (' · the file is missing on disk' if path else '')
    else:
        delta = chars - len(on_disk)
        if data['text'] == on_disk:
            size = f'{chars} characters · identical to the file on disk'
        else:
            size = f'{chars} characters · {delta:+d} compared with the file on disk'
    return name, when, size


def side_by_side(left_text, right_text):
    """Align two texts line by line.

    Returns (left_lines, right_lines, marks) of equal length; marks holds
    ' ' for equal rows, '~' for changed rows, '-' for rows only on the left
    and '+' for rows only on the right.
    """
    a = left_text.split('\n')
    b = right_text.split('\n')
    left, right, marks = [], [], []
    matcher = difflib.SequenceMatcher(None, a, b, autojunk=False)
    for tag, i1, i2, j1, j2 in matcher.get_opcodes():
        if tag == 'equal':
            left.extend(a[i1:i2])
            right.extend(b[j1:j2])
            marks.extend(' ' * (i2 - i1))
            continue
        for k in range(max(i2 - i1, j2 - j1)):
            has_left, has_right = i1 + k < i2, j1 + k < j2
            left.append(a[i1 + k] if has_left else '')
            right.append(b[j1 + k] if has_right else '')
            marks.append('~' if has_left and has_right else '-' if has_left else '+')
    return left, right, marks

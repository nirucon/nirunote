from __future__ import annotations

import shutil
import subprocess
import threading
from pathlib import Path

from PySide6.QtCore import QObject, Signal


class SpellResult(QObject):
    finished = Signal(object)

class HunspellDictionary:
    def __init__(self, base, personal=None):
        self.base=str(base); self.exe=shutil.which('hunspell'); self.cache={}
        self.personal=Path(personal) if personal else None
        self.personal_words=set()
        self.lock=threading.RLock()
        self.results=SpellResult()
        self.inflight=False
        if self.personal and self.personal.exists():
            try: self.personal_words={w.strip().casefold() for w in self.personal.read_text(encoding='utf-8').splitlines() if w.strip()}
            except Exception: self.personal_words=set()
        if not self.exe or not Path(self.base+'.dic').is_file() or not Path(self.base+'.aff').is_file():
            raise RuntimeError('hunspell or dictionary unavailable')
    def check(self, word):
        key=word.casefold()
        if key in self.personal_words: return True
        if key in self.cache: return self.cache[key]
        try:
            p=subprocess.run([self.exe,'-d',self.base,'-l'],input=word+'\n',text=True,capture_output=True,timeout=1.5)
            # hunspell -l reports spelling through stdout; that output is authoritative.
            ok=(not p.stdout.strip())
        except Exception: ok=True
        self.cache[key]=ok
        if len(self.cache)>12000: self.cache.clear()
        return ok
    def cached(self, word):
        key=word.casefold()
        if key in self.personal_words: return True
        with self.lock: return self.cache.get(key)
    def check_many(self, words):
        # Called from the worker only. Never block the Qt GUI event loop.
        with self.lock:
            pending={w.casefold():w for w in words if w and w.casefold() not in self.personal_words and w.casefold() not in self.cache}
        if not pending: return
        try:
            p=subprocess.run([self.exe,'-d',self.base,'-l'],input='\n'.join(pending.values())+'\n',text=True,capture_output=True,timeout=4)
            if p.returncode not in (0,1): raise RuntimeError('hunspell failed')
            bad={w.strip().casefold() for w in p.stdout.splitlines() if w.strip()}
            with self.lock:
                for key in pending: self.cache[key]=(key not in bad)
        except Exception:
            # Fail open: spelling errors must not interrupt writing.
            with self.lock:
                for key in pending: self.cache[key]=True

    def add_word(self, word):
        w=word.strip()
        if not w or not self.personal: return
        self.personal.parent.mkdir(parents=True,exist_ok=True)
        if w.casefold() not in self.personal_words:
            with self.personal.open('a',encoding='utf-8') as f: f.write(w+'\n')
            self.personal_words.add(w.casefold())
        self.cache.pop(w.casefold(),None)

    def suggest(self, word):
        try:
            p=subprocess.run([self.exe,'-d',self.base,'-a'],input=word+'\n',text=True,capture_output=True,timeout=2)
            for line in p.stdout.splitlines():
                if line.startswith('&') and ':' in line:
                    return [x.strip() for x in line.split(':',1)[1].split(',') if x.strip()]
                if line.startswith('#'): return []
        except Exception: pass
        return []


# Dictionaries are looked up in NIRUNOTE's user-local directory first (the
# installer downloads the Swedish one there), then in the usual distribution
# locations used by hunspell packages.
SYSTEM_DICTIONARY_DIRS = (
    Path('/usr/share/hunspell'),
    Path('/usr/share/myspell/dicts'),
    Path('/usr/share/myspell'),
)

# Debian/Ubuntu package names; the hint is only shown in the status line.
DICTIONARY_PACKAGES = {'sv_SE': 'hunspell-sv', 'en_US': 'hunspell-en-us'}


def dictionary_candidates(name, user_dir, system_dirs=SYSTEM_DICTIONARY_DIRS):
    """Return dictionary base paths (without .dic/.aff) that exist for name."""
    found = []
    for folder in (Path(user_dir), *system_dirs):
        base = folder / name
        if Path(str(base) + '.dic').is_file() and Path(str(base) + '.aff').is_file():
            found.append(base)
    return found


def language_code(base):
    """'…/sv_SE' -> 'sv'."""
    return Path(str(base)).name.split('_')[0].lower()


def unavailable_hint(names, user_dir, system_dirs=SYSTEM_DICTIONARY_DIRS):
    """Explain why no dictionary could be used, as a short install hint."""
    if not shutil.which('hunspell'):
        return 'install hunspell'
    for name in names:
        if not dictionary_candidates(name, user_dir, system_dirs):
            return 'install ' + DICTIONARY_PACKAGES.get(name, 'a hunspell dictionary')
    return 'dictionary check failed'


def spell_status_text(enabled, state, dictionary=None, hint='', large=False):
    """Human-readable status line text for spell checking.

    state is one of 'off', 'starting', 'ready', 'unavailable'.
    """
    if not enabled or state == 'off':
        return 'Spelling: off'
    if large:
        return 'Spelling: paused (large document)'
    if state == 'ready' and dictionary is not None:
        return 'Spelling: ' + language_code(dictionary.base)
    if state == 'starting':
        return 'Spelling: starting…'
    return 'Spelling: unavailable' + (f' ({hint})' if hint else '')

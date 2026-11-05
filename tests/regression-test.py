#!/usr/bin/env python3
"""NIRUNOTE regression tests (offscreen, isolated HOME/XDG, no network).

Run: QT_QPA_PLATFORM=offscreen python3 tests/regression-test.py
"""
import importlib.util
import os
import sys
import tempfile
import time
from pathlib import Path

TMP = tempfile.TemporaryDirectory(prefix='nirunote-regression-')
BASE = Path(TMP.name)
os.environ['QT_QPA_PLATFORM'] = os.environ.get('QT_QPA_PLATFORM', 'offscreen')
os.environ['HOME'] = str(BASE / 'home')
for key, sub in (('XDG_CONFIG_HOME', 'config'), ('XDG_STATE_HOME', 'state'),
                 ('XDG_DATA_HOME', 'data'), ('XDG_CACHE_HOME', 'cache')):
    os.environ[key] = str(BASE / sub)
Path(os.environ['HOME']).mkdir()

from PySide6.QtCore import QTimer, QEventLoop  # noqa: E402
from PySide6.QtWidgets import QApplication, QFileDialog  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('nirunote_app', ROOT / 'nirunote' / 'app.py')
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)

qapp = QApplication(['nirunote-regression-test'])
qapp.setApplicationName('NIRUNOTE-REGRESSION')
qapp.setOrganizationName('NIRU-REGRESSION')


class FakeDictionary:
    """Stands in for Hunspell: the spell-ready rehighlight is what used to dirty documents."""
    def __init__(self, base, personal=None):
        self.base = str(base)
        self.personal_words = set()
        self.cache = {}
        self.inflight = False
        self.results = mod.SpellResult()

    def check(self, word):
        return word.casefold() != 'asdasdasd'

    def cached(self, word):
        return self.check(word)

    def check_many(self, words):
        pass

    def suggest(self, word):
        return []


mod.HunspellDictionary = FakeDictionary
dict_dir = Path(os.environ['HOME']) / '.local/share/nirunote/dictionaries'
dict_dir.mkdir(parents=True)

PASSED = []


def spin(seconds):
    loop = QEventLoop()
    QTimer.singleShot(int(seconds * 1000), loop.quit)
    loop.exec()


def test(fn):
    started = time.monotonic()
    fn()
    PASSED.append(fn.__name__)
    print(f'ok  {fn.__name__} ({time.monotonic() - started:.1f}s)', flush=True)
    return fn


def fixture(name, text='# Title\n\nHej hello text.\n\n## Second\n\nMore text.\n', data=None):
    path = BASE / 'docs' / name
    path.parent.mkdir(exist_ok=True)
    path.write_bytes(data if data is not None else text.encode('utf-8'))
    return path


def recovery_file():
    return Path(os.environ['XDG_STATE_HOME']) / 'nirunote' / 'recovery.json'


def window():
    w = mod.Main(skip_session=True)
    w.show()
    return w


def dispose(w):
    # Never close(): closeEvent contains interactive unsaved-changes logic.
    w.hide()
    w.timer.stop() if hasattr(w, 'timer') else None
    w.deleteLater()
    qapp.processEvents()
    recovery_file().unlink(missing_ok=True)


def assert_clean(w, label):
    # Let the spell probe, highlighter, stats, spell and recovery timers run.
    spin(3.2)
    assert not w.dirty.text(), f'{label}: dirty marker shown after open'
    assert 'Modified' not in w.stat_left.text(), f'{label}: {w.stat_left.text()}'
    assert not recovery_file().exists(), f'{label}: recovery written for an unchanged document'


@test
def t1_open_paths_stay_clean():
    doc = fixture('open.md')
    # Command line / explicit file.
    w = window()
    w.load(str(doc))
    assert_clean(w, 'command line')
    # File -> Open.
    original = QFileDialog.getOpenFileName
    QFileDialog.getOpenFileName = staticmethod(lambda *a, **k: (str(doc), ''))
    try:
        w.open()
    finally:
        QFileDialog.getOpenFileName = original
    assert_clean(w, 'File > Open')
    # Recent.
    w.open_recent(str(doc))
    assert_clean(w, 'Open Recent')
    # External reload of an unmodified document.
    doc.write_text('# Title\n\nChanged on disk.\n', encoding='utf-8')
    w.external_file_changed(str(doc))
    assert w.editor.toPlainText().startswith('# Title\n\nChanged on disk.'), 'external reload failed'
    assert_clean(w, 'external reload')
    dispose(w)
    # Scratchpad.
    w = window()
    w.open_scratchpad()
    assert_clean(w, 'scratchpad')
    dispose(w)


@test
def t1_preferences_do_not_dirty():
    w = window()
    w.load(str(fixture('prefs.md')))
    spin(0.5)
    dialog = mod.Prefs(w)
    dialog.accept()
    assert_clean(w, 'preferences')
    dispose(w)


@test
def t1_real_edit_still_dirties():
    w = window()
    w.load(str(fixture('edit.md')))
    spin(0.5)
    w.editor.insertPlainText('x')
    spin(0.3)
    assert w.dirty.text(), 'real edit did not mark the document modified'
    dispose(w)


TMP.cleanup()
print(f'NIRUNOTE regression tests: OK ({len(PASSED)} tests)', flush=True)
sys.exit(0)

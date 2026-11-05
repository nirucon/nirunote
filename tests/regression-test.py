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
for key, sub in (
    ('XDG_CONFIG_HOME', 'config'),
    ('XDG_STATE_HOME', 'state'),
    ('XDG_DATA_HOME', 'data'),
    ('XDG_CACHE_HOME', 'cache'),
):
    os.environ[key] = str(BASE / sub)
Path(os.environ['HOME']).mkdir()

from PySide6.QtCore import QEvent, QEventLoop, QObject, QTimer  # noqa: E402
from PySide6.QtWidgets import QApplication, QFileDialog  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('nirunote_app', ROOT / 'nirunote' / 'app.py')
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
sys.path.insert(0, str(ROOT / 'nirunote'))

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
    w.recovery_debounce.stop()
    w.recovery_deadline.stop()
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
    assert w.editor.toPlainText().startswith('# Title\n\nChanged on disk.'), (
        'external reload failed'
    )
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


def count_recovery_writes(w):
    writes = []
    original = w.atomic_write

    def counting(path, text):
        if Path(path) == recovery_file():
            writes.append(time.monotonic())
        return original(path, text)

    w.atomic_write = counting
    return writes


@test
def t2_recovery_written_only_after_changes():
    w = window()
    w.load(str(fixture('recovery.md')))
    writes = count_recovery_writes(w)
    spin(3.0)
    assert writes == [], f'unchanged document wrote recovery {len(writes)} times'
    w.editor.insertPlainText('a')
    spin(1.0)
    assert writes == [], 'recovery must be debounced (about 2 s after the last edit)'
    spin(1.6)
    assert len(writes) == 1, f'expected one debounced write, got {len(writes)}'
    data = recovery_file().read_text(encoding='utf-8')
    assert '"text": "a# Title' in data, data[:80]
    # Idle while dirty: no further writes.
    spin(6.0)
    assert len(writes) == 1, f'idle dirty document rewrote recovery ({len(writes)} writes)'
    # Continuous typing: the debounce keeps restarting, the 10 s deadline still writes.
    for _ in range(24):
        w.editor.insertPlainText('b')
        spin(0.5)
    assert len(writes) == 2, f'expected one deadline write while typing, got {len(writes) - 1}'
    spin(2.6)
    assert len(writes) == 3, 'final debounced write after typing stopped missing'
    # Saving clears recovery and does not resurrect it.
    assert w.save()
    spin(2.6)
    assert not recovery_file().exists(), 'recovery file left behind after save'
    assert len(writes) == 3
    dispose(w)


class TimerCounter(QObject):
    """Counts every QTimerEvent delivered in the application (all timers, Qt's own included)."""

    def __init__(self):
        super().__init__()
        self.count = 0

    def eventFilter(self, obj, event):
        if event.type() == QEvent.Type.Timer:
            self.count += 1
        return False


def idle_timer_events(w, seconds=10.0):
    counter = TimerCounter()
    qapp.installEventFilter(counter)
    spin(seconds)
    qapp.removeEventFilter(counter)
    # spin() itself uses one single-shot timer.
    return counter.count - 1


@test
def t3_idle_has_no_periodic_timers():
    w = window()
    w.load(str(fixture('idle.md', '# Idle\n\n' + 'Hej hello text.\n' * 400)))
    spin(3.0)  # let load-time work (spell probe, stats) settle
    w.editor.clearFocus()
    w.setFocus()
    unfocused = idle_timer_events(w)
    print(f'    idle timer events in 10 s without editor focus: {unfocused}')
    assert unfocused <= 3, f'idle window fired {unfocused} timer events in 10 s'
    # A dirty document must stay idle too once its recovery snapshot is written.
    w.editor.setFocus()
    w.editor.insertPlainText('x')
    spin(2.5)
    w.editor.clearFocus()
    dirty = idle_timer_events(w)
    print(f'    idle timer events in 10 s, dirty document: {dirty}')
    assert dirty <= 3, f'dirty idle window fired {dirty} timer events in 10 s'
    dispose(w)


@test
def t4_spelling_status_is_readable():
    import spell as spellmod

    sst = spellmod.spell_status_text
    assert sst(False, 'off') == 'Spelling: off'
    assert sst(True, 'ready', FakeDictionary('/x/sv_SE')) == 'Spelling: sv'
    assert sst(True, 'ready', FakeDictionary('/x/en_US')) == 'Spelling: en'
    assert (
        sst(True, 'unavailable', None, 'install hunspell-sv')
        == 'Spelling: unavailable (install hunspell-sv)'
    )
    assert (
        sst(True, 'ready', FakeDictionary('/x/sv_SE'), large=True)
        == 'Spelling: paused (large document)'
    )
    old_path = os.environ['PATH']
    empty = BASE / 'empty-bin'
    empty.mkdir(exist_ok=True)
    try:
        os.environ['PATH'] = str(empty)
        assert spellmod.unavailable_hint(['sv_SE'], dict_dir, ()) == 'install hunspell'
        fake = empty / 'hunspell'
        fake.write_text('#!/bin/sh\n')
        fake.chmod(0o755)
        assert spellmod.unavailable_hint(['sv_SE'], dict_dir, ()) == 'install hunspell-sv'
    finally:
        os.environ['PATH'] = old_path
    # In the window: ready with the (fake) Swedish dictionary, off when disabled.
    for name in ('sv_SE.dic', 'sv_SE.aff'):
        (dict_dir / name).write_text('')
    w = window()
    w.load(str(fixture('spell.md')))
    spin(1.0)
    assert w.stat_spell.text() == 'Spelling: sv', w.stat_spell.text()
    assert 'SPELL' not in w.stat_left.text(), w.stat_left.text()
    w.spell_enabled = False
    w.init_spell()
    w.update_document_stats()
    assert w.stat_spell.text() == 'Spelling: off', w.stat_spell.text()
    # Clicking the status opens Preferences with the spelling setting focused.
    opened = []
    original = mod.Prefs.exec
    mod.Prefs.exec = lambda self: opened.append(self) or 0
    try:
        w.stat_spell.mousePressEvent(None)
    finally:
        mod.Prefs.exec = original
    assert opened and isinstance(opened[0], mod.Prefs), (
        'spelling status click did not open Preferences'
    )
    for name in ('sv_SE.dic', 'sv_SE.aff'):
        (dict_dir / name).unlink()
    dispose(w)


@test
def u1_modified_follows_undo():
    w = window()
    w.load(str(fixture('undo.md')))
    spin(0.5)
    doc = w.editor.document()
    assert not doc.isModified() and not w.dirty.text()
    w.editor.insertPlainText('typed ')
    spin(0.3)
    assert doc.isModified() and w.dirty.text() == '●', 'edit not marked'
    assert w.stat_left.text().startswith('Modified'), w.stat_left.text()
    spin(2.4)
    assert recovery_file().exists(), 'recovery not written after edit'
    w.editor.undo()
    spin(0.3)
    assert not doc.isModified() and not w.dirty.text(), (
        'undo to the saved text must clear the marker'
    )
    assert w.stat_left.text().startswith('Saved'), w.stat_left.text()
    assert not recovery_file().exists(), 'recovery kept after undo to saved text'
    w.editor.redo()
    assert w.dirty.text() == '●'
    assert w.save()
    assert not w.dirty.text()
    w.editor.undo()
    assert w.dirty.text() == '●', 'undo past the save point must mark modified'
    w.editor.redo()
    assert not w.dirty.text(), 'redo back to the save point must clear the marker'
    # Replace All is a real, undoable edit.
    w.find_input.setText('text')
    w.replace_input.setText('words')
    w.replace_all()
    assert 'words' in w.editor.toPlainText() and w.dirty.text() == '●'
    w.editor.undo()
    assert 'words' not in w.editor.toPlainText() and not w.dirty.text()
    # New from clipboard and recovered text count as unsaved.
    w.reset_doc('clip text', 'Clipboard.md')
    assert w.dirty.text() == '●'
    w.reset_doc()
    assert not w.dirty.text()
    dispose(w)


def write_snapshot(text, path=None, extra=None):
    import json

    recovery_file().parent.mkdir(parents=True, exist_ok=True)
    data = {'text': text, 'path': str(path) if path else None}
    data.update(extra or {})
    recovery_file().write_text(json.dumps(data), encoding='utf-8')


def answer_modal(action, nested=None):
    """Drive the next modal dialog: action(dialog) runs once it is shown."""
    seen = []

    def drive():
        dialog = qapp.activeModalWidget()
        if dialog is None:
            QTimer.singleShot(20, drive)
            return
        seen.append(dialog)
        if nested:
            QTimer.singleShot(50, lambda: nested(qapp.activeModalWidget()))
        action(dialog)

    QTimer.singleShot(0, drive)
    return seen


@test
def u2_recovery_prompt():
    import recovery as rec

    left, right, marks = rec.side_by_side('a\nb\nc', 'a\nB\nc\nd')
    assert marks == [' ', '~', ' ', '+'] and right[-1] == 'd' and left[-1] == '', (
        left,
        right,
        marks,
    )
    assert not rec.owner_alive({'pid': os.getpid()})

    doc = fixture('crash.md', '# Crash\n\nsaved line\n')
    other = fixture('other.md')
    # A 0.5.0-format snapshot (text + path only) survives opening another file.
    write_snapshot('# Crash\n\nsaved line\nunsaved line\n', doc)
    w = mod.Main(skip_session=True)
    w.show()
    assert w._startup_recovery is not None, 'snapshot not detected'
    w.load(str(other))
    assert recovery_file().exists(), 'opening a file deleted an undecided recovery snapshot'
    # The prompt shows file, time and size difference; Compare then Restore.
    compared = []

    def close_compare(dialog):
        compared.append(dialog)
        assert isinstance(dialog, mod.CompareDialog), dialog
        assert dialog.marks.count('+') == 1, dialog.marks
        dialog.reject()

    def restore_after_compare(dialog):
        labels = ' | '.join(label.text() for label in dialog.findChildren(mod.QLabel))
        assert 'crash.md' in labels and '+13 compared with the file on disk' in labels, labels
        dialog.compare_button.click()
        dialog.restore_button.click()

    answer_modal(restore_after_compare, nested=close_compare)
    assert w.offer_recovery() == 'restore'
    assert compared, 'Compare dialog did not open'
    assert w.editor.toPlainText().endswith('unsaved line\n'), 'recovered text not restored'
    assert w.path == str(doc) and w.dirty.text() == '●' and 'RECOVERED' in w.stat_left.text()
    assert doc.read_text() == '# Crash\n\nsaved line\n', 'restore must not write the file'
    dispose(w)

    # Discard keeps one backup copy.
    write_snapshot('lost text?', None)
    w = window()
    answer_modal(lambda dialog: dialog.discard_button.click())
    assert w.offer_recovery() == 'discard'
    backup = recovery_file().with_name('recovery-discarded.json')
    assert not recovery_file().exists() and 'lost text?' in backup.read_text(), (
        'discard lost the text'
    )
    assert not w.editor.toPlainText()
    dispose(w)

    # Escape restores (never drops text silently).
    write_snapshot('escape keeps me', None, {'saved_at': 1700000000})
    w = window()
    answer_modal(lambda dialog: dialog.reject())
    assert w.offer_recovery() == 'restore' and w.editor.toPlainText() == 'escape keeps me'
    dispose(w)

    # An unreadable snapshot is moved aside, not overwritten.
    recovery_file().write_text('{not json', encoding='utf-8')
    w = window()
    assert w._startup_recovery is None
    assert recovery_file().with_name('recovery-unreadable.json').read_text() == '{not json'
    dispose(w)

    # "Don't Save" keeps a backup of the dropped text.
    w = window()
    w.load(str(other))
    w.editor.insertPlainText('dropped ')
    original = mod.neutral_unsaved
    mod.neutral_unsaved = lambda parent: 'discard'
    try:
        assert w.maybe_save()
    finally:
        mod.neutral_unsaved = original
    assert 'dropped ' in backup.read_text()
    dispose(w)
    for leftover in recovery_file().parent.glob('recovery-*.json'):
        leftover.unlink()


@test
def u3_outline():
    from markdown import extract_headings

    text = '# One\n\npara\n## Two ##\n```\n# not a heading\n```\n### Three\n#nospace\n######  Six\n'
    assert extract_headings(text) == [
        (0, 1, 'One'),
        (3, 2, 'Two'),
        (7, 3, 'Three'),
        (9, 6, 'Six'),
    ], extract_headings(text)
    assert extract_headings('') == []
    w = window()
    w.word_wrap = True
    w.apply_editor_options()
    long_line = 'word ' * 400
    w.editor.setPlainText('# Start\n' + long_line + '\n## Target heading\nbody\n## Other\n')

    def filter_and_enter(dialog):
        assert isinstance(dialog, mod.OutlineDialog), dialog
        assert dialog.list.count() == 3
        dialog.search.setText('targ')
        assert dialog.list.count() == 1, dialog.list.count()
        dialog.search.returnPressed.emit()

    answer_modal(filter_and_enter)
    w.outline()
    assert w.editor.textCursor().blockNumber() == 2, w.editor.textCursor().blockNumber()
    # Arrow keys in the filter move the selection.
    from PySide6.QtGui import QKeyEvent

    def arrow_down(dialog):
        QApplication.sendEvent(
            dialog.search,
            QKeyEvent(
                QEvent.Type.KeyPress, mod.Qt.Key.Key_Down, mod.Qt.KeyboardModifier.NoModifier
            ),
        )
        assert dialog.list.currentRow() == 1, dialog.list.currentRow()
        dialog.jump()

    answer_modal(arrow_down)
    w.outline()
    assert w.editor.textCursor().blockNumber() == 2
    shortcuts = {a.shortcut().toString() for a in w.actions()}
    assert 'Ctrl+Shift+O' in shortcuts, shortcuts
    w.set_modified(False)
    dispose(w)


@test
def u4_code_blocks_are_highlighted():
    w = window()
    w.set_theme('dark')
    w.editor.setPlainText('# Head\n```\n# not a heading *x*\ncode line\n```\nafter *em*\n')
    qapp.processEvents()
    panel = mod.QColor(w.current_palette()['panel'])
    doc = w.editor.document()

    def formats(number):
        return doc.findBlockByNumber(number).layout().formats()

    for number in (1, 2, 3, 4):
        ranges = formats(number)
        assert ranges and all(r.format.background().color() == panel for r in ranges), (
            number,
            ranges,
        )
        assert all(r.format.fontFixedPitch() for r in ranges), number
    # Inside the fence no heading size or emphasis is applied.
    assert all(not r.format.fontItalic() and r.format.fontWeight() <= 400 for r in formats(2))
    # After the closing fence, normal Markdown highlighting resumes.
    assert any(r.format.fontItalic() for r in formats(5)), 'emphasis after the fence missing'
    assert not any(r.format.background().color() == panel for r in formats(5))
    # Typing an opening fence turns the following lines into code.
    c = mod.QTextCursor(doc.findBlockByNumber(5))
    c.insertText('```\n')
    qapp.processEvents()
    assert all(r.format.background().color() == panel for r in formats(6)), (
        'unclosed fence not highlighted'
    )
    w.set_modified(False)
    dispose(w)


@test
def restore_last_document_on_start():
    doc = fixture('last.md', '# Last\n\nreopened\n')
    settings = mod.QSettings('NIRU', 'NIRUNOTE')
    settings.setValue('restore_last_document', True)
    settings.setValue('last_document', str(doc))
    settings.sync()
    try:
        w = mod.Main()
        w.show()
        assert w.path == str(doc) and 'reopened' in w.editor.toPlainText(), 'last document not restored'
        assert str(doc) in w.watcher.files(), 'restored document is not watched for external changes'
        assert_clean(w, 'restore last document')
        dispose(w)
    finally:
        settings.setValue('restore_last_document', False)
        settings.sync()


TMP.cleanup()
print(f'NIRUNOTE regression tests: OK ({len(PASSED)} tests)', flush=True)
sys.exit(0)

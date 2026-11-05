#!/usr/bin/env python3
"""NIRUNOTE: minimal, keyboard-oriented Markdown editor (entry point and main window)."""

import os
import re
import shutil
import sys
import tempfile
import threading
import time
from pathlib import Path

from PySide6.QtCore import QFileSystemWatcher, QSettings, Qt, QTimer
from PySide6.QtGui import (
    QAction,
    QColor,
    QKeySequence,
    QShortcut,
    QTextCursor,
    QTextDocument,
    QTextFormat,
)
from PySide6.QtWidgets import (
    QApplication,
    QCheckBox,
    QDialog,
    QHBoxLayout,
    QInputDialog,
    QLabel,
    QLineEdit,
    QListWidget,
    QListWidgetItem,
    QMainWindow,
    QMenu,
    QPushButton,
    QStackedWidget,
    QTextBrowser,
    QTextEdit,
    QToolButton,
    QVBoxLayout,
    QWidget,
)

try:
    from .constants import (
        APP,
        LARGE_DOCUMENT_CHARS,
        RECOVERY_DEBOUNCE_MS,
        RECOVERY_MAX_DELAY_MS,
        VERSION,
    )
    from .dialogs import OutlineDialog, neutral_notice
    from .dialogs import CompareDialog, Prefs, RecoveryDialog, neutral_unsaved  # noqa: F401 (API)
    from .editor import MarkdownHighlighter, NoteEditor
    from .fileio import FileMixin, RecoveryMixin, RecoveryStore
    from .markdown import extract_headings, markdown_to_html
    from .preview import PreviewMixin
    from .settings import SettingsMixin
    from .spell import HunspellDictionary, SpellResult, dictionary_candidates  # noqa: F401
    from .spell import spell_status_text, unavailable_hint
    from .styles import WIDTHS, palette_for
    from .theme import load_nirux_palette, nirux_available
    from .theme_watcher import ThemeWatcher
    from .workers import run_async
except ImportError:
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    from constants import (
        APP,
        LARGE_DOCUMENT_CHARS,
        RECOVERY_DEBOUNCE_MS,
        RECOVERY_MAX_DELAY_MS,
        VERSION,
    )
    from dialogs import OutlineDialog, neutral_notice
    from dialogs import CompareDialog, Prefs, RecoveryDialog, neutral_unsaved  # noqa: F401 (API)
    from editor import MarkdownHighlighter, NoteEditor
    from fileio import FileMixin, RecoveryMixin, RecoveryStore
    from markdown import extract_headings, markdown_to_html
    from preview import PreviewMixin
    from settings import SettingsMixin
    from spell import HunspellDictionary, SpellResult, dictionary_candidates  # noqa: F401
    from spell import spell_status_text, unavailable_hint
    from styles import WIDTHS, palette_for
    from theme import load_nirux_palette, nirux_available
    from theme_watcher import ThemeWatcher
    from workers import run_async


class Main(FileMixin, RecoveryMixin, PreviewMixin, SettingsMixin, QMainWindow):
    """The NIRUNOTE window: editing, spelling, status and menus; see the mixins for the rest."""

    def __init__(self, skip_session=False, scratchpad=False):
        super().__init__()
        self.scratchpad = scratchpad
        self._startup_recovery = None
        self.settings = QSettings('NIRU', 'NIRUNOTE')
        self.path = None
        self.previewing = False
        self.focused = False
        self._loading = False
        self._formatting = 0
        self._external_mtime = None
        self._ignore_watch = False
        self.spell_enabled = self.settings.value('spell_enabled', True, type=bool)
        self.spell_language = self.settings.value('spell_language', 'sv_SE')
        self.spell_dict = None
        self.spell_deferred = False
        self._spell_generation = 0
        self._recovery_pending = False
        self._recovery_again = False
        self._text_generation = 0
        self._recovery_generation = 0
        self.recovery_store = RecoveryStore(lambda path, payload: self.atomic_write(path, payload))
        self.init_spell()
        self.load_settings()
        self.line_ending = '\n'
        self.had_bom = False
        self._external_prompt = False
        self.recovered = False
        self.setWindowTitle(APP)
        self.setMinimumSize(560, 400)
        self.setAcceptDrops(True)
        self.watcher = QFileSystemWatcher(self)
        self.watcher.fileChanged.connect(self.external_file_changed)
        self.theme_watcher = ThemeWatcher(self)
        self.theme_watcher.changed.connect(self._nirux_theme_changed)
        geo = self.settings.value('geometry')
        self.resize(1000, 760)
        if geo:
            self.restoreGeometry(geo)
        root = QWidget()
        self.v = QVBoxLayout(root)
        self.v.setContentsMargins(0, 0, 0, 0)
        self.v.setSpacing(0)
        self.top = QWidget()
        h = QHBoxLayout(self.top)
        h.setContentsMargins(12, 7, 12, 7)
        self.menu_btn = QToolButton()
        self.menu_btn.setText('☰')
        self.menu_btn.setPopupMode(QToolButton.ToolButtonPopupMode.InstantPopup)
        self.menu_btn.setProperty('headerButton', True)
        self.menu = QMenu(self)
        self.menu_btn.setMenu(self.menu)
        self.title = QLabel('Untitled.md')
        self.title.setAccessibleName('Current document')
        self.title.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.title.setToolTip('Click to rename / save')
        self.title.mousePressEvent = lambda e: self.rename()
        self.copy_btn = QToolButton()
        self.copy_btn.setText('⧉')
        self.copy_btn.setToolTip('Copy all')
        self.copy_btn.clicked.connect(self.copy_feedback)
        self.preview_btn = QToolButton()
        self.preview_btn.setText('◉')
        self.preview_btn.setToolTip('Edit / Preview')
        self.preview_btn.clicked.connect(self.toggle_preview)
        self.focus_btn = QToolButton()
        self.focus_btn.setText('⛶')
        self.focus_btn.setToolTip('Focus mode · F11')
        self.focus_btn.clicked.connect(self.focus)
        self.save_btn = QToolButton()
        self.save_btn.setText('⌑')
        self.save_btn.setToolTip('Save · Ctrl+S')
        self.save_btn.clicked.connect(self.save_feedback)
        self.dirty = QLabel('')
        self.dirty.setFixedWidth(16)
        h.addWidget(self.menu_btn)
        h.addStretch()
        h.addWidget(self.title)
        h.addStretch()
        h.addWidget(self.copy_btn)
        h.addWidget(self.preview_btn)
        h.addWidget(self.focus_btn)
        h.addWidget(self.save_btn)
        h.addWidget(self.dirty)
        self.v.addWidget(self.top)
        self.stack = QStackedWidget()
        self.editor_host = QWidget()
        eh = QHBoxLayout(self.editor_host)
        eh.setContentsMargins(24, 20, 24, 0)
        eh.setSpacing(0)
        eh.addStretch()
        self.editor = NoteEditor(self)
        self.editor.setAccessibleName('Markdown editor')
        self.editor.setFrameStyle(0)
        self.editor.setTabStopDistance(32)
        self.editor.setAcceptDrops(False)
        eh.addWidget(self.editor, 1)
        eh.addStretch()
        self.preview_host = QWidget()
        ph = QHBoxLayout(self.preview_host)
        ph.setContentsMargins(24, 20, 24, 0)
        ph.setSpacing(0)
        ph.addStretch()
        self.preview = QTextBrowser()
        self.preview.setAccessibleName('Markdown preview')
        self.preview.setOpenExternalLinks(False)
        self.preview.anchorClicked.connect(self.open_preview_link)
        self.preview.setFrameStyle(0)
        ph.addWidget(self.preview, 1)
        ph.addStretch()
        self.stack.addWidget(self.editor_host)
        self.stack.addWidget(self.preview_host)
        self.v.addWidget(self.stack, 1)
        self.status = QWidget()
        self.status.setAccessibleName('Document status')
        sh = QHBoxLayout(self.status)
        sh.setContentsMargins(14, 5, 14, 7)
        self.stat_left = QLabel('Saved · Markdown · 0 words · 0 chars')
        self.stat_left.setAccessibleName('Document statistics')
        self.stat_right = QLabel('Ln 1, Col 1')
        self.stat_right.setAccessibleName('Cursor position')
        self.stat_spell = QLabel('Spelling: off')
        self.stat_spell.setAccessibleName('Spelling status, click for spelling settings')
        self.stat_spell.setToolTip('Spelling settings')
        self.stat_spell.setCursor(Qt.CursorShape.PointingHandCursor)
        self.stat_spell.mousePressEvent = lambda e: self.prefs('spell')
        sh.addWidget(self.stat_left)
        sh.addSpacing(10)
        sh.addWidget(self.stat_spell)
        sh.addStretch()
        sh.addWidget(self.stat_right)
        self.v.addWidget(self.status)
        self.findbar = QWidget()
        fb = QHBoxLayout(self.findbar)
        fb.setContentsMargins(14, 5, 14, 7)
        self.find_input = QLineEdit()
        self.find_input.setAccessibleName('Find text')
        self.find_input.setPlaceholderText('Find')
        self.replace_input = QLineEdit()
        self.replace_input.setAccessibleName('Replace text')
        self.replace_input.setPlaceholderText('Replace')
        self.case_box = QCheckBox('Aa')
        self.word_box = QCheckBox('Word')
        prev = QPushButton('↑')
        nxt = QPushButton('↓')
        rep = QPushButton('Replace')
        allb = QPushButton('All')
        closeb = QPushButton('×')
        fb.addWidget(self.find_input, 2)
        fb.addWidget(self.replace_input, 2)
        fb.addWidget(self.case_box)
        fb.addWidget(self.word_box)
        fb.addWidget(prev)
        fb.addWidget(nxt)
        fb.addWidget(rep)
        fb.addWidget(allb)
        fb.addWidget(closeb)
        self.findbar.hide()
        self.v.insertWidget(self.v.indexOf(self.status), self.findbar)
        prev.clicked.connect(lambda: self.find_next(True))
        nxt.clicked.connect(lambda: self.find_next(False))
        rep.clicked.connect(self.replace_one)
        allb.clicked.connect(self.replace_all)
        closeb.clicked.connect(self.hide_find)
        self.find_input.returnPressed.connect(lambda: self.find_next(False))
        self.setCentralWidget(root)
        self.high = MarkdownHighlighter(self.editor.document(), self)
        self.spell_timer = QTimer(self)
        self.spell_timer.setSingleShot(True)
        self.spell_timer.setInterval(350)
        self.spell_timer.timeout.connect(self.finish_spell_pass)
        self.stats_timer = QTimer(self)
        self.stats_timer.setSingleShot(True)
        self.stats_timer.setInterval(180)
        self.stats_timer.timeout.connect(self.update_document_stats)
        self.preview_timer = QTimer(self)
        self.preview_timer.setSingleShot(True)
        self.preview_timer.setInterval(140)
        self.preview_timer.timeout.connect(self.update_preview)
        # Created before any load(): load() and changed() use them.
        # Recovery is event driven: a write is scheduled RECOVERY_DEBOUNCE_MS
        # after the last real edit, and at most RECOVERY_MAX_DELAY_MS after the
        # first unsaved edit while typing continues. Nothing is written when
        # the text generation has not changed since the last recovery write.
        self.recovery_debounce = QTimer(self)
        self.recovery_debounce.setSingleShot(True)
        self.recovery_debounce.setInterval(RECOVERY_DEBOUNCE_MS)
        self.recovery_debounce.timeout.connect(self.recover)
        self.recovery_deadline = QTimer(self)
        self.recovery_deadline.setSingleShot(True)
        self.recovery_deadline.setInterval(RECOVERY_MAX_DELAY_MS)
        self.recovery_deadline.timeout.connect(self.recover)
        self._preview_generation = 0
        self.editor.textChanged.connect(self.changed)
        self.editor.document().modificationChanged.connect(self.modification_changed)
        self.editor.cursorPositionChanged.connect(self.cursor_changed)
        self.esc_shortcut = QShortcut(QKeySequence('Escape'), self)
        self.esc_shortcut.activated.connect(lambda: self.set_focus(False) if self.focused else None)
        self.build_menu()
        self.apply_theme()
        self.configure_theme_watcher()
        self.apply_editor_options()
        self.read_startup_recovery()
        if (
            not skip_session
            and self.restore_last_document
            and self.last_document
            and Path(self.last_document).is_file()
        ):
            self.load(self.last_document, quiet=True)
        QTimer.singleShot(0, self.editor.setFocus)

    def paste_plain(self):
        self.editor.insertPlainText(QApplication.clipboard().text())

    def duplicate_line(self):
        c = self.editor.textCursor()
        c.beginEditBlock()
        c.movePosition(QTextCursor.MoveOperation.StartOfBlock)
        c.movePosition(QTextCursor.MoveOperation.EndOfBlock, QTextCursor.MoveMode.KeepAnchor)
        line = c.selectedText()
        c.clearSelection()
        c.movePosition(QTextCursor.MoveOperation.EndOfBlock)
        c.insertText('\n' + line)
        c.endEditBlock()

    def init_spell(self):
        self._spell_generation += 1
        generation = self._spell_generation
        self.spell_dict = None
        self.spell_deferred = True
        self.spell_hint = ''
        if not self.spell_enabled:
            self.spell_state = 'off'
            self.spell_deferred = False
            return
        self.spell_state = 'starting'
        root = Path.home() / '.local/share/nirunote/dictionaries'
        names = ['sv_SE', 'en_US'] if self.spell_language == 'auto' else [self.spell_language]

        def probe():
            for name in names:
                for base in dictionary_candidates(name, root) or [root / name]:
                    try:
                        d = HunspellDictionary(base, root / 'personal.txt')
                        check = 'Hej' if name == 'sv_SE' else 'Hello'
                        if d.check(check) and not d.check('asdasdasd'):
                            return d, ''
                    except Exception:
                        continue
            return None, unavailable_hint(names, root)

        def done(result):
            if generation != self._spell_generation or not self.spell_enabled:
                return
            dictionary, hint = result
            self.spell_dict = dictionary
            self.spell_deferred = False
            self.spell_state = 'ready' if dictionary is not None else 'unavailable'
            self.spell_hint = hint
            if dictionary is not None:
                dictionary.results.finished.connect(self.spell_finished)
            if hasattr(self, 'high'):
                self.rehighlight()
            self.update_document_stats()

        run_async(self, probe, done)

    def replace_cursor_word(self, cursor, text):
        cursor.insertText(text)

    def add_dictionary_word(self, word):
        if self.spell_dict:
            self.spell_dict.add_word(word)
            self.rehighlight()

    def ignore_word(self, word):
        if self.spell_dict:
            self.spell_dict.personal_words.add(word.casefold())
            self.spell_dict.cache.pop(word.casefold(), None)
            self.rehighlight()

    def copy_feedback(self):
        self.copy_all()
        old = self.copy_btn.text()
        self.copy_btn.setText('✓')
        QTimer.singleShot(900, lambda: self.copy_btn.setText(old))

    def save_feedback(self):
        if self.save():
            old = self.save_btn.text()
            self.save_btn.setText('✓')
            self.stat_right.setText('Saved')
            QTimer.singleShot(1000, lambda: self.save_btn.setText(old))
            QTimer.singleShot(1500, self.cursor_changed)

    def set_focus(self, on):
        on = bool(on)
        if on == self.focused:
            return
        self.focused = on
        self.top.setVisible(not on)
        self.status.setVisible(not on)
        policy = (
            Qt.ScrollBarPolicy.ScrollBarAlwaysOff if on else Qt.ScrollBarPolicy.ScrollBarAsNeeded
        )
        self.editor.setVerticalScrollBarPolicy(policy)
        self.preview.setVerticalScrollBarPolicy(policy)
        if on:
            self.showFullScreen()
            QApplication.setOverrideCursor(Qt.CursorShape.BlankCursor)
        else:
            while QApplication.overrideCursor() is not None:
                QApplication.restoreOverrideCursor()
            self.showNormal()
        (self.preview if self.previewing else self.editor).setFocus()

    def focus(self):
        self.set_focus(not self.focused)

    def insert_datetime(self):
        self.editor.insertPlainText(time.strftime('%Y-%m-%d %H:%M'))

    def outline(self):
        headings = extract_headings(self.editor.toPlainText())
        if not headings:
            self.stat_right.setText('No headings')
            QTimer.singleShot(1500, self.cursor_changed)
            return
        dialog = OutlineDialog(self, headings)
        if dialog.exec() and dialog.line is not None:
            self.jump_to_line(dialog.line)

    def jump_to_line(self, line):
        if self.previewing:
            self.toggle_preview()
        block = self.editor.document().findBlockByNumber(line)
        if not block.isValid():
            return
        self.editor.setTextCursor(QTextCursor(block))
        self.editor.centerCursor()
        self.editor.setFocus()

    def goto_heading(self):
        heads = extract_headings(self.editor.toPlainText())
        if not heads:
            self.stat_right.setText('No headings')
            QTimer.singleShot(1500, self.cursor_changed)
            return
        labels = [('  ' * (lvl - 1)) + title for _, lvl, title in heads]
        choice, ok = QInputDialog.getItem(self, 'Go to heading', 'Heading:', labels, 0, False)
        if ok:
            self.jump_to_line(heads[labels.index(choice)][0])

    def act(self, name, shortcut, fn, menu=None):
        a = QAction(name, self)
        if shortcut:
            a.setShortcut(QKeySequence(shortcut))
            a.setShortcutContext(Qt.ShortcutContext.ApplicationShortcut)
        a.triggered.connect(fn)
        self.addAction(a)
        (menu or self.menu).addAction(a)
        return a

    def build_menu(self):
        self.menu.clear()
        filem = self.menu.addMenu('File')
        editm = self.menu.addMenu('Edit')
        viewm = self.menu.addMenu('View')
        helpm = self.menu.addMenu('Help')
        self.act('New', 'Ctrl+N', self.new, filem)
        self.act('New from Clipboard', 'Ctrl+Shift+N', self.new_from_clipboard, filem)
        self.act('Open…', 'Ctrl+O', self.open, filem)
        self.recent_menu = filem.addMenu('Open Recent')
        self.rebuild_recent()
        filem.addSeparator()
        self.act('Save', 'Ctrl+S', self.save, filem)
        self.act('Save As…', 'Ctrl+Shift+S', self.save_as, filem)
        self.rename_action = self.act('Rename…', None, self.rename, filem)
        self.folder_action = self.act(
            'Open containing folder', None, self.open_containing_folder, filem
        )
        self.path_action = self.act('Copy file path', None, self.copy_file_path, filem)
        filem.addSeparator()
        self.act('Quit', 'Ctrl+Q', self.close, filem)
        self.act('Undo', 'Ctrl+Z', self.editor.undo, editm)
        self.act('Redo', 'Ctrl+Shift+Z', self.editor.redo, editm)
        editm.addSeparator()
        self.act('Select all', 'Ctrl+A', self.editor.selectAll, editm)
        self.act('Copy all', 'Ctrl+Shift+C', self.copy_all, editm)
        self.act('Paste as plain text', 'Ctrl+Shift+V', self.paste_plain, editm)
        self.act('Duplicate line', 'Ctrl+Shift+D', self.duplicate_line, editm)
        editm.addSeparator()
        self.act('Find…', 'Ctrl+F', self.find, editm)
        self.act('Replace…', 'Ctrl+H', self.replace, editm)
        editm.addSeparator()
        self.act('Bold', 'Ctrl+B', lambda: self.wrap_markdown('**', '**'), editm)
        self.act('Italic', 'Ctrl+I', lambda: self.wrap_markdown('*', '*'), editm)
        self.act('Link', 'Ctrl+K', self.insert_link, editm)
        self.act('Checkbox', 'Ctrl+Shift+X', self.toggle_checkbox, editm)
        self.act('Bullet list', 'Ctrl+Shift+8', lambda: self.prefix_line('- '), editm)
        self.act('Numbered list', 'Ctrl+Shift+7', lambda: self.prefix_line('1. '), editm)
        self.act('Quote', 'Ctrl+Shift+.', lambda: self.prefix_line('> '), editm)
        editm.addSeparator()
        self.act('Go to heading…', 'Ctrl+G', self.goto_heading, editm)
        self.act('Outline…', 'Ctrl+Shift+O', self.outline, editm)
        self.act('Insert date / time', None, self.insert_datetime, editm)
        self.act('Quick Open…', 'Ctrl+P', self.quick_open, filem)
        self.act('Command Palette…', 'Ctrl+Shift+P', self.command_palette, editm)
        self.act('Edit / Preview', 'Ctrl+Shift+E', self.toggle_preview, viewm)
        self.act('Focus mode', 'F11', self.focus, viewm)
        self.act('Toggle floating', 'Ctrl+Shift+F', self.toggle_float, viewm)
        viewm.addSeparator()
        self.wrap_action = self.act('Word wrap', None, self.toggle_wrap, viewm)
        self.wrap_action.setCheckable(True)
        self.wrap_action.setChecked(self.word_wrap)
        self.line_action = self.act('Highlight current line', None, self.toggle_line, viewm)
        self.line_action.setCheckable(True)
        self.line_action.setChecked(self.highlight_line)
        width = viewm.addMenu('Writing width')
        for n in WIDTHS:
            a = width.addAction(n)
            a.setCheckable(True)
            a.setChecked(n == self.writing_width)
            a.triggered.connect(lambda checked=False, x=n: self.set_width(x))
        tm = viewm.addMenu('Theme')
        for n in self.available_themes():
            a = tm.addAction(n if n == 'Follow NIRUX' else n.capitalize())
            a.triggered.connect(lambda checked=False, x=n: self.set_theme(x))
        viewm.addSeparator()
        self.act('Text size +', 'Ctrl++', lambda: self.resize_font(1), viewm)
        self.act('Text size -', 'Ctrl+-', lambda: self.resize_font(-1), viewm)
        self.act('Reset text size', 'Ctrl+0', self.reset_font, viewm)
        self.act('Preferences…', None, self.prefs, viewm)
        self.act('About NIRUNOTE', None, self.about, helpm)
        self.update_file_actions()

    def is_modified(self):
        # The document's own modification state is the single source of truth:
        # it is set by real edits only and cleared again when Undo/Redo returns
        # to the saved revision.
        return self.editor.document().isModified()

    def set_modified(self, modified):
        self.editor.document().setModified(modified)
        # setPlainText() on an empty document can emit modificationChanged(True)
        # without a matching False later; always resync from the document.
        self.modification_changed(self.editor.document().isModified())

    def modification_changed(self, _modified=None):
        modified = self.is_modified()
        self.dirty.setText('●' if modified else '')
        self.dirty.setToolTip('Unsaved changes' if modified else '')
        if not modified and not self._loading:
            # Undo back to the saved text: the file on disk already holds it.
            self.clear_recovery()
        if hasattr(self, 'stat_spell'):
            self.update_document_stats()

    def rehighlight(self):
        # QSyntaxHighlighter.rehighlight() re-applies formats through the
        # document, which emits contentsChanged/textChanged although no text
        # changed. Mark the pass so changed() can ignore it; otherwise every
        # formatting pass looks like an edit (T1) and re-arms the spell timer,
        # whose handler rehighlights again: a self-sustaining loop (T3).
        self._formatting += 1
        try:
            self.high.rehighlight()
        finally:
            self._formatting -= 1

    def changed(self):
        if self._formatting:
            return
        # Keep the typing/open path cheap. Full-document work is delayed and
        # preview rendering only happens while Preview is actually visible.
        self.spell_deferred = True
        if not self._loading:
            self._text_generation += 1
            self.schedule_recovery()
        self.stats_timer.start()
        if self.previewing:
            self.preview_timer.start()
        # Loading setPlainText emits textChanged too. Do not schedule a costly
        # spelling pass for that transient load event; load() leaves the
        # document ready and spelling will run after the first real edit.
        if not self._loading:
            self.spell_timer.start()

    def update_document_stats(self):
        txt = self.editor.toPlainText()
        chars = len(txt)
        self.stat_spell.setText(
            spell_status_text(
                self.spell_enabled,
                self.spell_state,
                self.spell_dict,
                self.spell_hint,
                chars > LARGE_DOCUMENT_CHARS,
            )
        )
        words = len(txt.split())
        read = f' · {max(1, (words + 219) // 220)} min' if self.show_reading_time and words else ''
        ro = (
            ' · READ ONLY'
            if self.path and Path(self.path).exists() and not os.access(self.path, os.W_OK)
            else ''
        )
        state = 'Modified' if self.is_modified() else 'Saved'
        theme = 'NIRUX' if self.theme == 'Follow NIRUX' else self.theme.capitalize()
        recovered = ' · RECOVERED' if self.recovered else ''
        self.stat_left.setText(
            f'{state} · {theme} · '
            + ('Markdown' if self.current_ext() in ('md', 'markdown') else 'Text')
            + f' · {words} words · {chars} chars'
            + read
            + ro
            + recovered
        )

    def finish_spell_pass(self):
        # Keep large documents syntax-only. For normal documents, resolve all
        # uncached spelling in one Hunspell process, then repaint from cache.
        if (
            self.editor.document().characterCount() <= LARGE_DOCUMENT_CHARS
            and self.spell_dict
            and self.spell_enabled
        ):
            txt = self.editor.toPlainText()
            words = set(re.findall(r"(?u)\b[^\W\d_][^\W\d_'-]*\b", txt))
            if not self.spell_dict.inflight:
                dictionary = self.spell_dict
                dictionary.inflight = True

                def spell_worker():
                    try:
                        dictionary.check_many(words)
                    finally:
                        dictionary.results.finished.emit(dictionary)

                threading.Thread(target=spell_worker, daemon=True, name='nirunote-spell').start()
        self.spell_deferred = False
        if self.editor.document().characterCount() <= LARGE_DOCUMENT_CHARS:
            self.rehighlight()

    def spell_finished(self, dictionary):
        dictionary.inflight = False
        if (
            dictionary is self.spell_dict
            and self.spell_enabled
            and self.editor.document().characterCount() <= LARGE_DOCUMENT_CHARS
        ):
            self.rehighlight()

    def cursor_changed(self):
        if self.previewing:
            self.stat_right.setText('Preview')
            self.update_line_highlight()
            return
        c = self.editor.textCursor()
        sel = c.selectedText().replace('\u2029', ' ').strip()
        suffix = f' · Selected {len(sel.split())} words' if sel else ''
        self.stat_right.setText(f'Ln {c.blockNumber() + 1}, Col {c.positionInBlock() + 1}{suffix}')
        self.update_line_highlight()
        if self.typewriter:
            self.editor.centerCursor()

    def update_line_highlight(self):
        if not self.highlight_line or self.previewing:
            self.editor.setExtraSelections([])
            return
        sel = QTextEdit.ExtraSelection()
        sel.format.setBackground(QColor(self.current_palette()['line']))
        sel.format.setProperty(QTextFormat.Property.FullWidthSelection, True)
        sel.cursor = self.editor.textCursor()
        sel.cursor.clearSelection()
        self.editor.setExtraSelections([sel])

    def reset_doc(self, text='', name=None):
        self.recovered = False
        self._loading = True
        self.path = None
        self.editor.setPlainText(text)
        self._loading = False
        self.title.setText(name or 'Untitled.' + self.default_format)
        self.set_modified(bool(text))
        self._text_generation += 1
        if text:
            self.schedule_recovery()
        self.editor.setFocus()
        self.cursor_changed()
        self.update_file_actions()

    def copy_all(self):
        c = self.editor.textCursor()
        c.select(QTextCursor.SelectionType.Document)
        QApplication.clipboard().setText(c.selectedText().replace('\u2029', '\n'))

    def show_find(self, replace=False):
        self.findbar.show()
        self.replace_input.setVisible(replace)
        self.find_input.setFocus()
        self.find_input.selectAll()

    def hide_find(self):
        self.findbar.hide()
        self.editor.setFocus()

    def find(self):
        self.show_find(False)

    def replace(self):
        self.show_find(True)

    def find_next(self, backward=False):
        q = self.find_input.text()
        if not q:
            return
        flags = QTextDocument.FindFlag(0)
        if backward:
            flags |= QTextDocument.FindFlag.FindBackward
        if self.case_box.isChecked():
            flags |= QTextDocument.FindFlag.FindCaseSensitively
        if self.word_box.isChecked():
            flags |= QTextDocument.FindFlag.FindWholeWords
        if not self.editor.find(q, flags):
            self.editor.moveCursor(
                QTextCursor.MoveOperation.End if backward else QTextCursor.MoveOperation.Start
            )
            self.editor.find(q, flags)

    def replace_one(self):
        c = self.editor.textCursor()
        if c.hasSelection() and c.selectedText() == self.find_input.text():
            c.insertText(self.replace_input.text())
        self.find_next(False)

    def replace_all(self):
        q = self.find_input.text()
        if not q:
            return
        text = self.editor.toPlainText()
        flags = 0 if self.case_box.isChecked() else re.I
        pat = re.escape(q)
        pat = (r'\b' + pat + r'\b') if self.word_box.isChecked() else pat
        new = re.sub(pat, lambda m: self.replace_input.text(), text, flags=flags)
        if new == text:
            return
        # One undoable edit instead of setPlainText(), which would reset the
        # undo stack and mark the document as saved.
        c = self.editor.textCursor()
        c.beginEditBlock()
        c.select(QTextCursor.SelectionType.Document)
        c.insertText(new)
        c.endEditBlock()

    def wrap_markdown(self, left, right):
        c = self.editor.textCursor()
        selected = c.selectedText().replace('\u2029', '\n')
        if selected:
            c.insertText(left + selected + right)
        else:
            c.insertText(left + right)
            c.movePosition(
                QTextCursor.MoveOperation.Left, QTextCursor.MoveMode.MoveAnchor, len(right)
            )
            self.editor.setTextCursor(c)

    def insert_link(self):
        c = self.editor.textCursor()
        selected = c.selectedText().replace('\u2029', ' ') or 'text'
        c.insertText(f'[{selected}](url)')
        c.movePosition(QTextCursor.MoveOperation.Left, QTextCursor.MoveMode.MoveAnchor, 4)
        self.editor.setTextCursor(c)

    def prefix_line(self, prefix):
        c = self.editor.textCursor()
        c.movePosition(QTextCursor.MoveOperation.StartOfBlock)
        c.insertText(prefix)

    def toggle_checkbox(self):
        c = self.editor.textCursor()
        c.select(QTextCursor.SelectionType.BlockUnderCursor)
        line = c.selectedText().replace('\u2029', '')
        if re.match(r'^\s*[-*+] \[[ xX]\] ', line):
            line = re.sub(r'^(\s*)[-*+] \[[ xX]\] ', r'\1', line)
        else:
            line = '- [ ] ' + line
        c.insertText(line)

    def command_palette(self):
        commands = [
            ('New', self.new),
            ('Open', self.open),
            ('Quick Open', self.quick_open),
            ('Save', self.save),
            ('Save As', self.save_as),
            ('Preview', self.toggle_preview),
            ('Focus mode', self.focus),
            ('Go to heading', self.goto_heading),
            ('Outline', self.outline),
            ('Insert date / time', self.insert_datetime),
            ('Preferences', self.prefs),
        ]
        d = QDialog(self)
        d.setWindowTitle('Command Palette')
        d.setAccessibleName('Command Palette')
        d.resize(480, 360)
        v = QVBoxLayout(d)
        search = QLineEdit()
        search.setAccessibleName('Command search')
        search.setPlaceholderText('Type a command')
        lst = QListWidget()
        lst.setAccessibleName('Commands')
        v.addWidget(search)
        v.addWidget(lst, 1)

        def fill(q=''):
            lst.clear()
            q = q.casefold()
            for name, _ in commands:
                if q in name.casefold():
                    item = QListWidgetItem(name)
                    lst.addItem(item)
            if lst.count():
                lst.setCurrentRow(0)

        fill()
        search.textChanged.connect(fill)
        search.returnPressed.connect(d.accept)
        lst.itemActivated.connect(lambda item: d.accept())
        if d.exec() and lst.currentItem():
            dict(commands)[lst.currentItem().text()]()

    def toggle_float(self):
        # Qt Tool windows are normally treated as floating by both stacking and tiling WMs.
        floating = bool(self.windowFlags() & Qt.WindowType.Tool)
        geo = self.geometry()
        self.hide()
        self.setWindowFlag(Qt.WindowType.Tool, not floating)
        self.setWindowFlag(Qt.WindowType.Window, floating)
        self.show()
        self.setGeometry(geo)
        self.raise_()
        self.activateWindow()

    def about(self):
        neutral_notice(
            self,
            'About NIRUNOTE',
            f'NIRUNOTE {VERSION}\n\nMinimal, suckless writing for Linux.\nMarkdown-first. Keyboard-driven. Fast.\n\nby Nicklas Rudolfsson',
        )

    def closeEvent(self, e):
        self.settings.setValue('geometry', self.saveGeometry())
        self.recover(sync=True)
        if self.path:
            self.settings.setValue(
                'cursor/' + str(Path(self.path).resolve()), self.editor.textCursor().position()
            )
            self.last_document = self.path
            self.save_settings()
        if self.maybe_save():
            # Saved, unchanged or "Don't Save" (backed up): no crash snapshot is needed.
            self.clear_recovery()
            e.accept()
        else:
            e.ignore()


def core_self_test():
    try:
        td = Path(tempfile.mkdtemp(prefix='nirunote-core-'))
        p = td / 'atomic.md'

        def aw(path, text):
            fd, tmp = tempfile.mkstemp(
                prefix='.' + path.name + '.', suffix='.tmp', dir=str(path.parent)
            )
            try:
                with os.fdopen(fd, 'w', encoding='utf-8', newline='') as f:
                    f.write(text)
                    f.flush()
                    os.fsync(f.fileno())
                os.replace(tmp, path)
            finally:
                if os.path.exists(tmp):
                    os.unlink(tmp)

        aw(p, 'alpha')
        aw(p, 'beta')
        if p.read_text(encoding='utf-8') != 'beta':
            raise RuntimeError('atomic save mismatch')
        for key in ('bg', 'fg', 'muted', 'panel', 'border', 'accent', 'select', 'line', 'md'):
            if key not in palette_for('dark'):
                raise RuntimeError('palette missing ' + key)
        if palette_for('unknown') != palette_for('dark'):
            raise RuntimeError('unknown theme fallback mismatch')
        if 'href="https://example.test"' not in markdown_to_html('[ok](https://example.test)'):
            raise RuntimeError('safe link missing')
        if markdown_to_html('[bad](javascript:alert(1))').find('href=') != -1:
            raise RuntimeError('javascript link remained')
        if markdown_to_html('[bad](data:text/html,x)').find('href=') != -1:
            raise RuntimeError('data link remained')
        if markdown_to_html('[bad](file:///etc/passwd)').find('href=') != -1:
            raise RuntimeError('file link remained')
        if '<small onclick=' not in markdown_to_html('<small onclick="x">safe</small>'):
            pass
        if '<small onclick=' in markdown_to_html('<small onclick="x">safe</small>'):
            raise RuntimeError('attribute HTML remained')
        old_config = os.environ.get('XDG_CONFIG_HOME')
        old_path = os.environ.get('PATH')
        try:
            with tempfile.TemporaryDirectory(prefix='nirunote-theme-') as theme_dir:
                os.environ['XDG_CONFIG_HOME'] = theme_dir
                os.environ['PATH'] = '/usr/bin:/bin'
                if nirux_available():
                    raise RuntimeError('empty config must not detect NIRUX')
                theme_root = Path(theme_dir)
                (theme_root / 'nirux/theme').mkdir(parents=True)
                (theme_root / 'niruwm').mkdir()
                (theme_root / 'nirux/theme/kitty.conf').write_text(
                    'background #101010\nforeground #eeeeee\ncolor4 #88aacc\ncolor8 #888888\nselection_background #333333\n',
                    encoding='utf-8',
                )
                (theme_root / 'niruwm/appearance.toml').write_text(
                    '[font]\nsize = 14\n', encoding='utf-8'
                )
                if not nirux_available():
                    raise RuntimeError('NIRUX files were not detected')
                nirux = load_nirux_palette()
                for key in (
                    'bg',
                    'fg',
                    'muted',
                    'panel',
                    'border',
                    'accent',
                    'select',
                    'line',
                    'md',
                ):
                    if key not in nirux:
                        raise RuntimeError('NIRUX palette missing ' + key)
                if nirux['bg'] != '#101010':
                    raise RuntimeError('NIRUX background mismatch')
                (theme_root / 'niruwm/appearance.toml').write_text(
                    'not valid = [', encoding='utf-8'
                )
                fallback = load_nirux_palette()
                if fallback['bg'] != '#101010':
                    raise RuntimeError('broken appearance should preserve kitty fallback')
        finally:
            if old_config is None:
                os.environ.pop('XDG_CONFIG_HOME', None)
            else:
                os.environ['XDG_CONFIG_HOME'] = old_config
            if old_path is None:
                os.environ.pop('PATH', None)
            else:
                os.environ['PATH'] = old_path
        shutil.rmtree(td, ignore_errors=True)
        print('NIRUNOTE core self-test: OK')
        return 0
    except Exception as e:
        print(f'NIRUNOTE core self-test: FAILED ({e})', file=sys.stderr)
        return 1


def spell_self_test():
    base = Path.home() / '.local/share/nirunote/dictionaries/sv_SE'
    try:
        d = HunspellDictionary(base)
        good = d.check('Hej')
        bad = not d.check('asdasdasd')
        if good and bad:
            print('NIRUNOTE spell runtime self-test: OK (SV)')
            return 0
        print(
            f'NIRUNOTE spell runtime self-test: FAILED (Hej={good}, asdasdasd_bad={bad})',
            file=sys.stderr,
        )
    except Exception as e:
        print(f'NIRUNOTE spell runtime self-test: FAILED ({e})', file=sys.stderr)
    return 1


def main():
    # Non-GUI probes are used by the installer/wrapper and must never create a window.
    if '--spell-self-test' in sys.argv:
        return spell_self_test()
    if '--core-self-test' in sys.argv:
        return core_self_test()
    if '--version' in sys.argv:
        print(f'{APP} {VERSION}')
        return 0
    if '--self-test' in sys.argv:
        # Non-interactive GUI probe. The installer supplies isolated XDG dirs.
        app = QApplication.instance() or QApplication([])
        app.setApplicationName(APP)
        app.setOrganizationName('NIRU')
        w = Main()
        w.apply_theme()
        w.apply_editor_options()
        w.update_line_highlight()
        w.update_preview()
        app.processEvents()
        w.hide()
        w.deleteLater()
        app.processEvents()
        app.quit()
        print(f'{APP} {VERSION}: GUI OK', flush=True)
        return 0
    scratch = '--scratchpad' in sys.argv
    force_new = '--new' in sys.argv
    focus_start = '--focus' in sys.argv
    open_arg = None
    if '--open' in sys.argv:
        try:
            open_arg = sys.argv[sys.argv.index('--open') + 1]
        except IndexError:
            open_arg = None
    files = [
        a
        for i, a in enumerate(sys.argv[1:])
        if not a.startswith('-') and (i == 0 or sys.argv[i] != '--open')
    ]
    explicit = open_arg or (files[0] if files else None)
    app = QApplication(sys.argv)
    app.setApplicationName(APP)
    app.setApplicationDisplayName(APP + ' Scratchpad' if scratch else APP)
    app.setOrganizationName('NIRU')
    app.setDesktopFileName('nirunote-scratchpad' if scratch else 'nirunote')
    w = Main(skip_session=scratch or force_new or bool(explicit), scratchpad=scratch)
    if scratch:
        w.open_scratchpad()
    elif explicit and Path(explicit).is_file():
        w.load(explicit)
    elif force_new:
        w.reset_doc()
    w.show()
    if focus_start:
        w.set_focus(True)
    w.raise_()
    w.activateWindow()
    QTimer.singleShot(0, w.offer_recovery)
    return app.exec()


if __name__ == '__main__':
    raise SystemExit(main())

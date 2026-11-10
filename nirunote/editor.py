"""Editor widgets: the Markdown highlighter and the plain-text editor."""

from __future__ import annotations

import re
import sys
from pathlib import Path

from PySide6.QtCore import Qt
from PySide6.QtGui import (
    QAction,
    QColor,
    QFont,
    QSyntaxHighlighter,
    QTextCharFormat,
    QTextCursor,
)
from PySide6.QtWidgets import (
    QPlainTextEdit,
)

try:
    from .constants import LARGE_DOCUMENT_CHARS
except ImportError:
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    from constants import LARGE_DOCUMENT_CHARS


HEADING_RE = re.compile(r'^(#{1,6})(\s+)(.*)$')
STRONG_RE = re.compile(r'\*\*([^*]+)\*\*')
EM_RE = re.compile(r'(?<!\*)\*([^*]+)\*(?!\*)')
CODE_RE = re.compile(r'`([^`]+)`')
LIST_RE = re.compile(r'^\s*([-*+]\s+|\d+[.)]\s+)')
TASK_RE = re.compile(r'^\s*[-*+]\s+(\[[ xX]\])')
QUOTE_RE = re.compile(r'^(>\s?).*$')
LINK_RE = re.compile(r'\[([^]]+)\]\(([^)]+)\)')
WORD_RE = re.compile(r"(?u)\b[^\W\d_][^\W\d_'-]*\b")
SPELL_SKIP_RE = re.compile(r'`[^`]*`|https?://\S+|www\.\S+')


class MarkdownHighlighter(QSyntaxHighlighter):
    def __init__(self, doc, window):
        super().__init__(doc)
        self.window = window
        self._formats = None
        self._format_key = None

    def fmt(self, col, bold=False, italic=False):
        f = QTextCharFormat()
        f.setForeground(QColor(col))
        f.setFontWeight(700 if bold else 400)
        f.setFontItalic(italic)
        return f

    CODE_STATE = 1

    def code_format(self, t, color):
        f = self.fmt(color)
        f.setBackground(QColor(t['panel']))
        f.setFontFamilies(['monospace'])
        f.setFontFixedPitch(True)
        f.setFontStyleHint(QFont.StyleHint.Monospace)
        return f

    def formats(self):
        """Character formats for the current palette and font size, built once."""
        key = (getattr(self.window, '_theme_epoch', 0), self.window.font_size)
        if self._formats is None or key != self._format_key:
            t = self.window.current_palette()
            size = self.window.font_size
            headings = {}
            for level in range(1, 7):
                f = self.fmt(t['accent'], True)
                bump = 4 if level == 1 else 2 if level == 2 else 1 if level == 3 else 0
                f.setFontPointSize(max(11, size + bump))
                headings[level] = f
            spell = QTextCharFormat()
            spell.setUnderlineStyle(QTextCharFormat.UnderlineStyle.SpellCheckUnderline)
            spell.setUnderlineColor(QColor(t['muted']))
            self._formats = {
                'palette': t,
                'fence': self.code_format(t, t['md']),
                'code': self.code_format(t, t['fg']),
                'md': self.fmt(t['md']),
                'md_bold': self.fmt(t['md'], True),
                'bold': self.fmt(t['fg'], True),
                'italic': self.fmt(t['fg'], False, True),
                'accent': self.fmt(t['accent']),
                'accent_bold': self.fmt(t['accent'], True),
                'quote': self.fmt(t['muted'], False, True),
                'headings': headings,
                'spell': spell,
            }
            self._format_key = key
        return self._formats

    def highlightBlock(self, text):
        F = self.formats()
        # Fenced code blocks: a block state carries "inside a fence" to the
        # next line. Code is shown verbatim in monospace on a subtle panel
        # background, without Markdown or spelling marks (no language parsing).
        inside = self.previousBlockState() == self.CODE_STATE
        if text.startswith(('```', '~~~')):
            self.setCurrentBlockState(-1 if inside else self.CODE_STATE)
            self.setFormat(0, len(text), F['fence'])
            return
        if inside:
            self.setCurrentBlockState(self.CODE_STATE)
            self.setFormat(0, len(text), F['code'])
            return
        self.setCurrentBlockState(-1)
        if not text:
            return
        # Syntax markers are deliberately muted; content remains dominant.
        first = text.lstrip()[:1]
        if first == '#':
            m = HEADING_RE.match(text)
            if m:
                level = len(m.group(1))
                self.setFormat(m.start(1), level, F['md'])
                self.setFormat(m.start(3), len(m.group(3)), F['headings'][level])
        if '*' in text:
            for m in STRONG_RE.finditer(text):
                self.setFormat(m.start(), 2, F['md'])
                self.setFormat(m.start(1), len(m.group(1)), F['bold'])
                self.setFormat(m.end() - 2, 2, F['md'])
            for m in EM_RE.finditer(text):
                self.setFormat(m.start(), 1, F['md'])
                self.setFormat(m.start(1), len(m.group(1)), F['italic'])
                self.setFormat(m.end() - 1, 1, F['md'])
        if '`' in text:
            for m in CODE_RE.finditer(text):
                self.setFormat(m.start(), m.end() - m.start(), F['accent'])
        if first in '-*+0123456789':
            m = LIST_RE.match(text)
            if m:
                self.setFormat(m.start(1), len(m.group(1)), F['md_bold'])
                m = TASK_RE.match(text)
                if m:
                    self.setFormat(m.start(1), len(m.group(1)), F['accent_bold'])
        if text[0] == '>':
            m = QUOTE_RE.match(text)
            if m:
                self.setFormat(m.start(), len(m.group(1)), F['md'])
                self.setFormat(m.end(1), len(text) - m.end(1), F['quote'])
        if '](' in text:
            for m in LINK_RE.finditer(text):
                self.setFormat(m.start(), m.end() - m.start(), F['accent'])
        # Local/offline live spelling; skip URLs and inline code.
        if (
            self.window.spell_dict
            and self.window.spell_enabled
            and not getattr(self.window, 'spell_deferred', False)
            and self.window.editor.document().characterCount() <= LARGE_DOCUMENT_CHARS
        ):
            excluded = [(x.start(), x.end()) for x in SPELL_SKIP_RE.finditer(text)]
            for m in WORD_RE.finditer(text):
                a, b = m.span()
                word = m.group()
                if any(x <= a and b <= y for x, y in excluded):
                    continue
                if a and text[a - 1] in '#[]()!*_>':
                    continue
                # Highlighting is paint-path code: cached lookup only.
                # Hunspell subprocess work is batched after idle instead.
                try:
                    correct = self.window.spell_dict.cached(word)
                except Exception:
                    correct = True
                if correct is False:
                    self.setFormat(a, b - a, F['spell'])


class NoteEditor(QPlainTextEdit):
    def __init__(self, owner):
        super().__init__()
        self.owner = owner

    def wheelEvent(self, e):
        if e.modifiers() & Qt.KeyboardModifier.ControlModifier:
            self.owner.resize_font(1 if e.angleDelta().y() > 0 else -1)
            e.accept()
            return
        super().wheelEvent(e)

    def keyPressEvent(self, e):
        if e.key() == Qt.Key.Key_Escape and self.owner.focused:
            self.owner.set_focus(False)
            return
        if self.isReadOnly():
            # The list/quote helpers below insert text programmatically, which
            # a read-only QPlainTextEdit would still accept.
            super().keyPressEvent(e)
            return

        # Indent/outdent Markdown list items without inserting literal tabs.
        if e.key() == Qt.Key.Key_Tab and not (e.modifiers() & ~Qt.KeyboardModifier.ShiftModifier):
            c = self.textCursor()
            line = c.block().text()
            if re.match(r'^\s*(?:[-*+]\s+(?:\[[ xX]\]\s+)?|\d+[.)]\s+)', line):
                c.movePosition(QTextCursor.MoveOperation.StartOfBlock)
                c.insertText('  ')
                return
        if e.key() == Qt.Key.Key_Backtab:
            c = self.textCursor()
            line = c.block().text()
            if line.startswith('  '):
                c.movePosition(QTextCursor.MoveOperation.StartOfBlock)
                c.movePosition(QTextCursor.MoveOperation.Right, QTextCursor.MoveMode.KeepAnchor, 2)
                c.removeSelectedText()
                return

        # Writing-oriented Markdown list / quote continuation.
        if e.key() in (Qt.Key.Key_Return, Qt.Key.Key_Enter) and not (
            e.modifiers() & Qt.KeyboardModifier.ShiftModifier
        ):
            c = self.textCursor()
            line = c.block().text()
            m = re.match(r'^(\s*)([-*+]\s+|\d+[.)]\s+|[-*+]\s+\[(?: |x|X)\]\s+)(.*)$', line)
            if m:
                indent, token, body = m.groups()
                if not body.strip():
                    c.select(QTextCursor.SelectionType.BlockUnderCursor)
                    c.removeSelectedText()
                    super().keyPressEvent(e)
                    return
                super().keyPressEvent(e)
                if re.match(r'\d+', token):
                    n = int(re.match(r'\d+', token).group()) + 1
                    sep = ')' if ')' in token else '.'
                    self.insertPlainText(f'{indent}{n}{sep} ')
                elif '[' in token:
                    self.insertPlainText(f'{indent}- [ ] ')
                else:
                    self.insertPlainText(indent + token)
                return
            qm = re.match(r'^(\s*>\s?)(.*)$', line)
            if qm:
                prefix, body = qm.groups()
                if not body.strip():
                    c.select(QTextCursor.SelectionType.BlockUnderCursor)
                    c.removeSelectedText()
                    super().keyPressEvent(e)
                    return
                super().keyPressEvent(e)
                self.insertPlainText(prefix)
                return
        super().keyPressEvent(e)

    def contextMenuEvent(self, e):
        menu = self.createStandardContextMenu()
        if self.owner.spell_dict and not self.isReadOnly():
            c = self.cursorForPosition(e.pos())
            c.select(QTextCursor.SelectionType.WordUnderCursor)
            word = c.selectedText()
            if word and word.isalpha() and not self.owner.spell_dict.check(word):
                suggestions = self.owner.spell_dict.suggest(word)[:5]
                first = menu.actions()[0] if menu.actions() else None
                menu.insertSeparator(first)
                ignore = QAction(f'Ignore “{word}” for this session', menu)
                ignore.triggered.connect(lambda checked=False, w=word: self.owner.ignore_word(w))
                menu.insertAction(first, ignore)
                add = QAction(f'Add “{word}” to dictionary', menu)
                add.triggered.connect(
                    lambda checked=False, w=word: self.owner.add_dictionary_word(w)
                )
                menu.insertAction(first, add)
                for suggestion in reversed(suggestions):
                    a = QAction(suggestion, menu)
                    a.triggered.connect(
                        lambda checked=False, x=suggestion, cur=QTextCursor(c): (
                            self.owner.replace_cursor_word(cur, x)
                        )
                    )
                    menu.insertAction(first, a)
        menu.exec(e.globalPos())

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


class MarkdownHighlighter(QSyntaxHighlighter):
    def __init__(self, doc, window):
        super().__init__(doc)
        self.window = window

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

    def highlightBlock(self, text):
        t = self.window.current_palette()
        # Fenced code blocks: a block state carries "inside a fence" to the
        # next line. Code is shown verbatim in monospace on a subtle panel
        # background, without Markdown or spelling marks (no language parsing).
        inside = self.previousBlockState() == self.CODE_STATE
        if text.startswith('```'):
            self.setCurrentBlockState(-1 if inside else self.CODE_STATE)
            self.setFormat(0, len(text), self.code_format(t, t['md']))
            return
        if inside:
            self.setCurrentBlockState(self.CODE_STATE)
            self.setFormat(0, len(text), self.code_format(t, t['fg']))
            return
        self.setCurrentBlockState(-1)
        # Syntax markers are deliberately muted; content remains dominant.
        for m in re.finditer(r'^(#{1,6})(\s+)(.*)$', text):
            level = len(m.group(1))
            self.setFormat(m.start(1), level, self.fmt(t['md']))
            f = self.fmt(t['accent'], True)
            f.setFontPointSize(
                max(
                    11,
                    self.window.font_size
                    + (4 if level == 1 else 2 if level == 2 else 1 if level == 3 else 0),
                )
            )
            self.setFormat(m.start(3), len(m.group(3)), f)
        for m in re.finditer(r'\*\*([^*]+)\*\*', text):
            self.setFormat(m.start(), 2, self.fmt(t['md']))
            self.setFormat(m.start(1), len(m.group(1)), self.fmt(t['fg'], True))
            self.setFormat(m.end() - 2, 2, self.fmt(t['md']))
        for m in re.finditer(r'(?<!\*)\*([^*]+)\*(?!\*)', text):
            self.setFormat(m.start(), 1, self.fmt(t['md']))
            self.setFormat(m.start(1), len(m.group(1)), self.fmt(t['fg'], False, True))
            self.setFormat(m.end() - 1, 1, self.fmt(t['md']))
        for m in re.finditer(r'`([^`]+)`', text):
            self.setFormat(m.start(), m.end() - m.start(), self.fmt(t['accent']))
        for m in re.finditer(r'^\s*([-*+]\s+|\d+[.)]\s+)', text):
            self.setFormat(m.start(1), len(m.group(1)), self.fmt(t['md'], True))
        for m in re.finditer(r'^\s*[-*+]\s+(\[[ xX]\])', text):
            self.setFormat(m.start(1), len(m.group(1)), self.fmt(t['accent'], True))
        for m in re.finditer(r'^(>\s?).*$', text):
            self.setFormat(m.start(), len(m.group(1)), self.fmt(t['md']))
            self.setFormat(m.end(1), len(text) - m.end(1), self.fmt(t['muted'], False, True))
        for m in re.finditer(r'\[([^]]+)\]\(([^)]+)\)', text):
            self.setFormat(m.start(), m.end() - m.start(), self.fmt(t['accent']))
        # Local/offline live spelling; skip URLs and inline code.
        if (
            self.window.spell_dict
            and self.window.spell_enabled
            and not getattr(self.window, 'spell_deferred', False)
            and self.window.editor.document().characterCount() <= LARGE_DOCUMENT_CHARS
        ):
            excluded = []
            for pat in (r'`[^`]*`', r'https?://\S+', r'www\.\S+'):
                excluded.extend((x.start(), x.end()) for x in re.finditer(pat, text))
            for m in re.finditer(r"(?u)\b[^\W\d_][^\W\d_'-]*\b", text):
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
                    f = QTextCharFormat()
                    f.setUnderlineStyle(QTextCharFormat.UnderlineStyle.SpellCheckUnderline)
                    f.setUnderlineColor(QColor(t['muted']))
                    self.setFormat(a, b - a, f)


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
        if self.owner.spell_dict:
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

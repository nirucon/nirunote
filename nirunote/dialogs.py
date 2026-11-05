"""NIRUNOTE dialogs: preferences, neutral notices, outline, recovery and compare."""

from __future__ import annotations

import sys
from pathlib import Path

from PySide6.QtCore import QEvent, Qt
from PySide6.QtGui import (
    QColor,
    QFont,
    QTextCursor,
    QTextFormat,
)
from PySide6.QtWidgets import (
    QApplication,
    QCheckBox,
    QComboBox,
    QDialog,
    QDialogButtonBox,
    QFileDialog,
    QFormLayout,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QListWidget,
    QListWidgetItem,
    QMessageBox,
    QPlainTextEdit,
    QPushButton,
    QSpinBox,
    QTextEdit,
    QVBoxLayout,
    QWidget,
)

try:
    from .styles import WIDTHS
    from .recovery import side_by_side, describe
except ImportError:
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    from styles import WIDTHS
    from recovery import side_by_side, describe


class Prefs(QDialog):
    def __init__(self, w):
        super().__init__(w)
        self.w = w
        self.setWindowTitle('NIRUNOTE Preferences')
        self.setModal(True)
        self.setMinimumWidth(390)
        f = QFormLayout(self)
        f.setContentsMargins(22, 22, 22, 22)
        f.setSpacing(12)
        self.theme = QComboBox()
        self.theme.addItems(w.available_themes())
        self.theme.setCurrentText(w.theme)
        self.fmt = QComboBox()
        self.fmt.addItems(['Markdown (.md)', 'Plain text (.txt)'])
        self.fmt.setCurrentIndex(0 if w.default_format == 'md' else 1)
        self.size = QSpinBox()
        self.size.setRange(10, 36)
        self.size.setValue(w.font_size)
        self.width = QComboBox()
        self.width.addItems(list(WIDTHS))
        self.width.setCurrentText(w.writing_width)
        self.wrap = QCheckBox()
        self.wrap.setChecked(w.word_wrap)
        self.line = QCheckBox()
        self.line.setChecked(w.highlight_line)
        self.autosave = QCheckBox()
        self.autosave.setChecked(w.autosave)
        self.restore = QCheckBox()
        self.restore.setChecked(w.restore)
        self.typewriter = QCheckBox()
        self.typewriter.setChecked(w.typewriter)
        self.spell = QCheckBox()
        self.spell.setChecked(w.spell_enabled)
        self.spell_lang = QComboBox()
        self.spell_lang.addItems(['sv_SE', 'en_US', 'auto'])
        self.spell_lang.setCurrentText(w.spell_language)
        self.save_mode = QComboBox()
        self.save_mode.addItems(['Last used folder', 'Custom folder'])
        self.save_mode.setCurrentIndex(1 if w.save_folder_mode == 'custom' else 0)
        self.save_folder = QLineEdit(w.default_save_folder)
        browse = QPushButton('Choose…')
        browse.clicked.connect(self.choose_folder)
        row = QWidget()
        rh = QHBoxLayout(row)
        rh.setContentsMargins(0, 0, 0, 0)
        rh.addWidget(self.save_folder, 1)
        rh.addWidget(browse)
        self.restore_last = QCheckBox()
        self.restore_last.setChecked(w.restore_last_document)
        self.reading_time = QCheckBox()
        self.reading_time.setChecked(w.show_reading_time)
        for label, widget in [
            ('Theme', self.theme),
            ('Default format', self.fmt),
            ('Default save location', self.save_mode),
            ('Custom save folder', row),
            ('Restore last document', self.restore_last),
            ('Show reading time', self.reading_time),
            ('Writing width', self.width),
            ('Font size', self.size),
            ('Word wrap', self.wrap),
            ('Highlight current line', self.line),
            ('Recovery autosave', self.autosave),
            ('Restore recovery', self.restore),
            ('Typewriter mode', self.typewriter),
            ('Spell checking', self.spell),
            ('Spell language', self.spell_lang),
        ]:
            f.addRow(label, widget)
        b = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel
        )
        b.accepted.connect(self.accept)
        b.rejected.connect(self.reject)
        f.addRow(b)

    def choose_folder(self):
        p = QFileDialog.getExistingDirectory(
            self, 'Choose default save folder', self.save_folder.text() or str(Path.home())
        )
        if p:
            self.save_folder.setText(p)

    def accept(self):
        w = self.w
        w.theme = self.theme.currentText()
        w.default_format = 'md' if self.fmt.currentIndex() == 0 else 'txt'
        w.writing_width = self.width.currentText()
        w.font_size = self.size.value()
        w.word_wrap = self.wrap.isChecked()
        w.highlight_line = self.line.isChecked()
        w.autosave = self.autosave.isChecked()
        w.restore = self.restore.isChecked()
        w.typewriter = self.typewriter.isChecked()
        w.spell_enabled = self.spell.isChecked()
        w.spell_language = self.spell_lang.currentText()
        w.save_folder_mode = 'custom' if self.save_mode.currentIndex() == 1 else 'last'
        w.default_save_folder = self.save_folder.text().strip()
        w.restore_last_document = self.restore_last.isChecked()
        w.show_reading_time = self.reading_time.isChecked()
        w.init_spell()
        w.rehighlight()
        w.update_document_stats()
        w.save_settings()
        w.apply_theme()
        w.configure_theme_watcher()
        w.apply_editor_options()
        super().accept()


class NeutralMessageBox(QMessageBox):
    """NIRUNOTE message box: deliberately iconless and monochrome."""

    def __init__(self, parent=None, title='', text=''):
        super().__init__(parent)
        self.setWindowTitle(title)
        self.setProperty('nirunoteDialog', True)
        self.setText(text)
        self.setIcon(QMessageBox.Icon.NoIcon)


def neutral_notice(parent, title, text, button='OK'):
    box = NeutralMessageBox(parent, title, text)
    box.addButton(button, QMessageBox.ButtonRole.AcceptRole)
    box.exec()


def neutral_unsaved(parent):
    """Explicit dialog result codes; never infer intent from native button roles."""
    box = QDialog(parent)
    box.setWindowTitle('Unsaved changes')
    box.setModal(True)
    box.setProperty('nirunoteDialog', True)
    layout = QVBoxLayout(box)
    layout.setContentsMargins(20, 18, 20, 18)
    layout.setSpacing(18)
    message = QLabel('Save changes before continuing?', box)
    layout.addWidget(message)
    buttons = QHBoxLayout()
    buttons.addStretch()
    discard = QPushButton("Don't Save", box)
    cancel = QPushButton('Cancel', box)
    save = QPushButton('Save', box)
    for button in (discard, cancel, save):
        # Let Enter activate the focused button on every Qt platform.
        button.setAutoDefault(True)
        button.setMinimumWidth(88)
        buttons.addWidget(button)
    layout.addLayout(buttons)
    discard.clicked.connect(lambda: box.done(2))
    cancel.clicked.connect(box.reject)
    save.clicked.connect(lambda: box.done(1))
    # Safe initial action; focused buttons respond to Enter/Return.
    cancel.setDefault(True)
    cancel.setFocus()
    result = box.exec()
    return {1: 'save', 2: 'discard'}.get(result, 'cancel')


class OutlineDialog(QDialog):
    """Heading outline popup: type to filter, Enter jumps to the heading."""

    def __init__(self, parent, headings):
        super().__init__(parent)
        self.headings = headings
        self.line = None
        self.setWindowTitle('Outline')
        self.setAccessibleName('Outline')
        self.resize(520, 440)
        layout = QVBoxLayout(self)
        self.search = QLineEdit()
        self.search.setAccessibleName('Filter headings')
        self.search.setPlaceholderText('Filter headings')
        self.list = QListWidget()
        self.list.setAccessibleName('Headings')
        layout.addWidget(self.search)
        layout.addWidget(self.list, 1)
        self.search.textChanged.connect(self.fill)
        self.search.returnPressed.connect(self.jump)
        self.list.itemActivated.connect(lambda _item: self.jump())
        self.search.installEventFilter(self)
        self.fill('')
        self.search.setFocus()

    def fill(self, query):
        self.list.clear()
        terms = query.casefold().split()
        for line, level, title in self.headings:
            if all(term in title.casefold() for term in terms):
                item = QListWidgetItem('    ' * (level - 1) + title)
                item.setData(Qt.ItemDataRole.UserRole, line)
                self.list.addItem(item)
        if self.list.count():
            self.list.setCurrentRow(0)

    def eventFilter(self, obj, event):
        # Up/Down/PageUp/PageDown in the filter field move the selection.
        if (
            obj is self.search
            and event.type() == QEvent.Type.KeyPress
            and event.key()
            in (Qt.Key.Key_Up, Qt.Key.Key_Down, Qt.Key.Key_PageUp, Qt.Key.Key_PageDown)
        ):
            QApplication.sendEvent(self.list, event)
            return True
        return super().eventFilter(obj, event)

    def jump(self):
        item = self.list.currentItem()
        if item is not None:
            self.line = item.data(Qt.ItemDataRole.UserRole)
            self.accept()


class CompareDialog(QDialog):
    """Read-only side-by-side view of the file on disk and the recovered text."""

    def __init__(self, parent, on_disk, recovered, palette):
        super().__init__(parent)
        self.setWindowTitle('Compare recovered text')
        self.setAccessibleName('Compare recovered text')
        self.setProperty('nirunoteDialog', True)
        self.resize(1000, 640)
        left_lines, right_lines, self.marks = side_by_side(on_disk, recovered)
        layout = QVBoxLayout(self)
        columns = QHBoxLayout()
        self.left = self._column(
            columns, 'On disk' if on_disk else 'On disk (empty or missing)', '\n'.join(left_lines)
        )
        self.right = self._column(columns, 'Recovered', '\n'.join(right_lines))
        layout.addLayout(columns, 1)
        changed = sum(1 for mark in self.marks if mark != ' ')
        layout.addWidget(QLabel(f'{changed} changed line(s)' if changed else 'No differences'))
        close_row = QHBoxLayout()
        close_row.addStretch()
        close = QPushButton('Close', self)
        close.setMinimumWidth(88)
        close.setDefault(True)
        close.clicked.connect(self.reject)
        close_row.addWidget(close)
        layout.addLayout(close_row)
        changed_bg = QColor(palette['select'])
        for editor, side in ((self.left, '-'), (self.right, '+')):
            selections = []
            block = editor.document().firstBlock()
            for mark in self.marks:
                if mark in ('~', side):
                    selection = QTextEdit.ExtraSelection()
                    selection.format.setBackground(changed_bg)
                    selection.format.setProperty(QTextFormat.Property.FullWidthSelection, True)
                    selection.cursor = QTextCursor(block)
                    selections.append(selection)
                block = block.next()
            editor.setExtraSelections(selections)
        # Rows are aligned, so scrolling one side scrolls the other.
        self.left.verticalScrollBar().valueChanged.connect(self.right.verticalScrollBar().setValue)
        self.right.verticalScrollBar().valueChanged.connect(self.left.verticalScrollBar().setValue)

    def _column(self, row, title, text):
        column = QVBoxLayout()
        column.addWidget(QLabel(title))
        view = QPlainTextEdit()
        view.setReadOnly(True)
        view.setLineWrapMode(QPlainTextEdit.LineWrapMode.NoWrap)
        view.setAccessibleName(title)
        font = QFont('monospace')
        font.setStyleHint(QFont.StyleHint.Monospace)
        view.setFont(font)
        view.setPlainText(text)
        column.addWidget(view, 1)
        row.addLayout(column, 1)
        return view


class RecoveryDialog(QDialog):
    """Restore / Discard / Compare prompt for a snapshot left by a crash."""

    def __init__(self, parent, data, on_disk, palette):
        super().__init__(parent)
        self.data, self.on_disk, self.palette = data, on_disk, palette
        self.choice = 'restore'
        self.setWindowTitle('Recover unsaved text')
        self.setAccessibleName('Recover unsaved text')
        self.setProperty('nirunoteDialog', True)
        self.setModal(True)
        name, when, size = describe(data, on_disk)
        layout = QVBoxLayout(self)
        layout.setContentsMargins(20, 18, 20, 18)
        layout.setSpacing(12)
        layout.addWidget(
            QLabel('NIRUNOTE found unsaved text from a session that did not close normally.')
        )
        details = QFormLayout()
        details.addRow('File', QLabel(name))
        details.addRow('Last snapshot', QLabel(when))
        details.addRow('Size', QLabel(size))
        layout.addLayout(details)
        note = QLabel('Discard keeps one backup copy (recovery-discarded.json).')
        layout.addWidget(note)
        buttons = QHBoxLayout()
        buttons.addStretch()
        self.discard_button = QPushButton('Discard', self)
        self.compare_button = QPushButton('Compare…', self)
        self.restore_button = QPushButton('Restore', self)
        for button in (self.discard_button, self.compare_button, self.restore_button):
            button.setAutoDefault(True)
            button.setMinimumWidth(88)
            buttons.addWidget(button)
        layout.addLayout(buttons)
        self.discard_button.clicked.connect(lambda: self._finish('discard'))
        self.restore_button.clicked.connect(lambda: self._finish('restore'))
        self.compare_button.clicked.connect(self.compare)
        # Escape (reject) keeps the text: Restore is the non-destructive answer.
        self.restore_button.setDefault(True)
        self.restore_button.setFocus()

    def _finish(self, choice):
        self.choice = choice
        self.accept()

    def reject(self):
        self.choice = 'restore'
        super().reject()

    def compare(self):
        CompareDialog(self, self.on_disk or '', self.data['text'], self.palette).exec()

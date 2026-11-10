"""Preferences, themes and editor options for the main window (mixin)."""

from __future__ import annotations

import sys
from pathlib import Path

from PySide6.QtGui import (
    QFont,
)
from PySide6.QtWidgets import (
    QPlainTextEdit,
)

try:
    from .dialogs import Prefs
    from .styles import WIDTHS, app_stylesheet, palette_for
    from .theme import load_nirux_palette, nirux_available
except ImportError:
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    from dialogs import Prefs
    from styles import WIDTHS, app_stylesheet, palette_for
    from theme import load_nirux_palette, nirux_available


class SettingsMixin:
    """Persisted preferences, theme application and editor display options."""

    def load_settings(self):
        """Read the persisted preferences (QSettings NIRU/NIRUNOTE); keys unchanged."""
        saved_theme = self.settings.value('theme', '')
        self.theme = saved_theme or ('Follow NIRUX' if nirux_available() else 'dark')
        self.default_format = self.settings.value('format', 'md')
        self.font_size = int(self.settings.value('font_size', 17))
        self.autosave = self.settings.value('autosave', True, type=bool)
        self.restore = self.settings.value('restore', True, type=bool)
        self.typewriter = self.settings.value('typewriter', False, type=bool)
        self.word_wrap = self.settings.value('word_wrap', True, type=bool)
        self.highlight_line = self.settings.value('highlight_line', True, type=bool)
        self.writing_width = self.settings.value('writing_width', 'Normal')
        self.recent = self.settings.value('recent', [], type=list) or []
        self.save_folder_mode = self.settings.value('save_folder_mode', 'last')
        self.default_save_folder = self.settings.value(
            'default_save_folder', str(Path.home() / 'Documents')
        )
        self.last_save_folder = self.settings.value(
            'last_save_folder', str(Path.home() / 'Documents')
        )
        self.last_open_folder = self.settings.value('last_open_folder', str(Path.home()))
        self.restore_last_document = self.settings.value('restore_last_document', False, type=bool)
        self.show_reading_time = self.settings.value('show_reading_time', False, type=bool)
        self.remote_images = self.settings.value('remote_images', True, type=bool)
        self.last_document = self.settings.value('last_document', '')

    def available_themes(self):
        themes = ['dark', 'light', 'satie']
        if nirux_available():
            themes.append('Follow NIRUX')
        return themes

    def current_palette(self):
        # The highlighter asks for the palette once per block and the line
        # highlight on every cursor move. "Follow NIRUX" used to re-read and
        # parse the NIRUX theme files on each call; resolve it once per theme
        # application instead (apply_theme() invalidates the cache, and the
        # theme watcher calls apply_theme() when the NIRUX files change).
        cached = getattr(self, '_palette_cache', None)
        if cached is None or cached[0] != self.theme:
            palette = (
                load_nirux_palette() if self.theme == 'Follow NIRUX' else palette_for(self.theme)
            )
            cached = self._palette_cache = (self.theme, palette)
        return dict(cached[1])

    def configure_theme_watcher(self):
        watcher = getattr(self, 'theme_watcher', None)
        if watcher is None:
            return
        if self.theme == 'Follow NIRUX':
            watcher.start()
        else:
            watcher.stop()

    def _nirux_theme_changed(self, _palette):
        if self.theme == 'Follow NIRUX':
            self.apply_theme()
            self.update_line_highlight()
            self.rehighlight()

    def apply_theme(self):
        self._palette_cache = None
        self._theme_epoch = getattr(self, '_theme_epoch', 0) + 1
        t = self.current_palette()
        self.setStyleSheet(app_stylesheet(t))
        f = QFont('monospace')
        f.setStyleHint(QFont.StyleHint.Monospace)
        f.setPointSize(self.font_size)
        self.editor.setFont(f)
        self.preview.setFont(QFont('sans-serif', self.font_size))
        if self.previewing:
            # Re-highlighting a large source costs real time; do it when the
            # editor is shown again (ensure_highlighting()).
            self._highlight_stale = True
        else:
            self.rehighlight()
        self.update_preview()
        self.update_line_highlight()

    def apply_editor_options(self):
        self.editor.setLineWrapMode(
            QPlainTextEdit.LineWrapMode.WidgetWidth
            if self.word_wrap
            else QPlainTextEdit.LineWrapMode.NoWrap
        )
        self.apply_width()
        self.update_line_highlight()

    def apply_width(self):
        width = WIDTHS.get(self.writing_width, 900)
        self.editor.setMaximumWidth(width)
        self.preview.setMaximumWidth(width)

    def set_width(self, x):
        self.writing_width = x
        self.save_settings()
        self.apply_width()

    def set_theme(self, x):
        self.theme = x
        self.save_settings()
        self.apply_theme()
        self.configure_theme_watcher()

    def resize_font(self, d):
        self.font_size = max(10, min(36, self.font_size + d))
        self.save_settings()
        self.apply_theme()

    def reset_font(self):
        self.font_size = 17
        self.save_settings()
        self.apply_theme()

    def toggle_wrap(self):
        self.word_wrap = not self.word_wrap
        self.save_settings()
        self.apply_editor_options()
        self.wrap_action.setChecked(self.word_wrap)

    def toggle_line(self):
        self.highlight_line = not self.highlight_line
        self.save_settings()
        self.update_line_highlight()
        self.line_action.setChecked(self.highlight_line)

    def save_settings(self):
        for k, v in [
            ('theme', self.theme),
            ('format', self.default_format),
            ('font_size', self.font_size),
            ('autosave', self.autosave),
            ('restore', self.restore),
            ('typewriter', self.typewriter),
            ('word_wrap', self.word_wrap),
            ('highlight_line', self.highlight_line),
            ('writing_width', self.writing_width),
            ('spell_enabled', self.spell_enabled),
            ('spell_language', self.spell_language),
            ('save_folder_mode', self.save_folder_mode),
            ('default_save_folder', self.default_save_folder),
            ('last_save_folder', self.last_save_folder),
            ('last_open_folder', self.last_open_folder),
            ('restore_last_document', self.restore_last_document),
            ('show_reading_time', self.show_reading_time),
            ('remote_images', self.remote_images),
            ('last_document', self.last_document),
        ]:
            self.settings.setValue(k, v)

    def prefs(self, section=None):
        dialog = Prefs(self)
        if section == 'spell':
            dialog.spell.setFocus()
        dialog.exec()

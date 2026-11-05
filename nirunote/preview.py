"""Markdown Preview for the main window (mixin)."""

from __future__ import annotations

import sys
from pathlib import Path

from PySide6.QtCore import QTimer, QUrl
from PySide6.QtGui import (
    QDesktopServices,
)

try:
    from .constants import LARGE_DOCUMENT_CHARS
    from .markdown import markdown_to_html, safe_link
    from .styles import preview_css
    from .workers import run_async
except ImportError:
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    from constants import LARGE_DOCUMENT_CHARS
    from markdown import markdown_to_html, safe_link
    from styles import preview_css
    from workers import run_async


class PreviewMixin:
    """Edit/Preview switching, cached and (for large documents) background rendering."""

    def open_preview_link(self, url):
        target = safe_link(url.toString())
        if target is not None:
            QDesktopServices.openUrl(QUrl(target))
        else:
            self.stat_right.setText('Blocked unsafe preview link')
            QTimer.singleShot(1500, self.cursor_changed)

    def toggle_preview(self):
        ratio = (
            self.editor.verticalScrollBar().value()
            / max(1, self.editor.verticalScrollBar().maximum())
            if not self.previewing
            else self.preview.verticalScrollBar().value()
            / max(1, self.preview.verticalScrollBar().maximum())
        )
        self.previewing = not self.previewing
        self.stack.setCurrentIndex(1 if self.previewing else 0)
        self.preview_btn.setProperty('active', self.previewing)
        self.preview_btn.style().unpolish(self.preview_btn)
        self.preview_btn.style().polish(self.preview_btn)
        self.update_preview()
        self.update_line_highlight()
        self.cursor_changed()
        target = self.preview if self.previewing else self.editor
        QTimer.singleShot(
            0,
            lambda: target.verticalScrollBar().setValue(
                int(target.verticalScrollBar().maximum() * ratio)
            ),
        )
        target.setFocus()

    def preview_html(self, text):
        t = self.current_palette()
        return '<style>' + preview_css(t) + '</style>' + markdown_to_html(text)

    def _apply_preview_html(self, rendered):
        if getattr(self, '_preview_html', None) == rendered:
            return
        bar = self.preview.verticalScrollBar()
        had_preview = hasattr(self, '_preview_html')
        old_max = bar.maximum()
        old_value = bar.value()
        ratio = (old_value / old_max) if had_preview and old_max > 0 else None
        self.preview.setHtml(rendered)
        self._preview_html = rendered
        self.preview.document().adjustSize()
        if ratio is not None:
            QTimer.singleShot(0, lambda b=bar, r=ratio: b.setValue(round(b.maximum() * r)))

    def update_preview(self):
        if not (hasattr(self, 'preview') and self.previewing):
            return
        text = self.editor.toPlainText()
        if len(text) > LARGE_DOCUMENT_CHARS:
            self._preview_generation += 1
            generation = self._preview_generation

            def done(rendered):
                if generation == self._preview_generation and self.previewing:
                    self._apply_preview_html(rendered)

            run_async(self, lambda: self.preview_html(text), done)
            return
        self._apply_preview_html(self.preview_html(text))

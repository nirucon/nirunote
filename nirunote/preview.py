"""Markdown Preview for the main window: the browser widget and the mixin."""

from __future__ import annotations

import sys
from pathlib import Path

from PySide6.QtCore import QRectF, Qt, QTimer, QUrl
from PySide6.QtGui import (
    QColor,
    QDesktopServices,
    QFont,
    QFontMetrics,
    QImage,
    QImageReader,
    QPainter,
    QTextDocument,
    QTextOption,
)
from PySide6.QtWidgets import QTextBrowser

try:
    from .constants import LARGE_DOCUMENT_CHARS
    from .markdown import find_heading, link_kind, render_markdown
    from .styles import preview_css
    from .workers import run_async
except ImportError:
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    from constants import LARGE_DOCUMENT_CHARS
    from markdown import find_heading, link_kind, render_markdown
    from styles import preview_css
    from workers import run_async

MAX_LOCAL_IMAGE_BYTES = 32 * 1024 * 1024
MAX_REMOTE_IMAGE_BYTES = 8 * 1024 * 1024
REMOTE_TIMEOUT_S = 10
IMAGE_CACHE_ENTRIES = 64
VIEW_EXTENSIONS = {'.md', '.markdown', '.mdown', '.mkd', '.txt', ''}


def fetch_remote(url):
    """Download URL (http/https) with a size limit; returns bytes or raises."""
    import urllib.request  # only when a remote image is shown: keeps startup lean

    request = urllib.request.Request(url, headers={'User-Agent': 'NIRUNOTE preview'})
    with urllib.request.urlopen(request, timeout=REMOTE_TIMEOUT_S) as response:
        length = response.headers.get('Content-Length')
        if length and length.isdigit() and int(length) > MAX_REMOTE_IMAGE_BYTES:
            raise ValueError('image too large')
        data = response.read(MAX_REMOTE_IMAGE_BYTES + 1)
    if len(data) > MAX_REMOTE_IMAGE_BYTES:
        raise ValueError('image too large')
    return data


def svg_image(data, width):
    """Render SVG bytes with QtSvg when it is installed (optional); else a null image."""
    try:
        from PySide6.QtSvg import QSvgRenderer
    except ImportError:
        return QImage()
    renderer = QSvgRenderer(data)
    if not renderer.isValid():
        return QImage()
    size = renderer.defaultSize()
    if size.width() > width:
        size = size.scaled(width, 10**6, Qt.AspectRatioMode.KeepAspectRatio)
    image = QImage(size, QImage.Format.Format_ARGB32_Premultiplied)
    image.fill(Qt.GlobalColor.transparent)
    painter = QPainter(image)
    renderer.render(painter)
    painter.end()
    return image


def looks_like_svg(data):
    head = bytes(data[:512]).lstrip().lower()
    return head.startswith(b'<svg') or (head.startswith(b'<?xml') and b'<svg' in bytes(data[:2048]).lower())


class PreviewBrowser(QTextBrowser):
    """QTextBrowser with bounded local/remote image loading and Ctrl+wheel zoom.

    Images are loaded only when the Preview lays them out. Local files are
    decoded at most at the viewport width; remote images are fetched once in
    a worker thread (http/https only, size and time limited) and cached.
    """

    def __init__(self, owner):
        super().__init__()
        self.owner = owner
        self.images = {}
        self.alts = {}
        self.pending = set()
        self.failed = set()
        self.base_dir = None
        self._relayout = QTimer(self)
        self._relayout.setSingleShot(True)
        self._relayout.setInterval(60)
        self._relayout.timeout.connect(self._refresh_images)

    def max_image_width(self):
        margin = int(self.document().documentMargin())
        return max(120, self.viewport().width() - 2 * margin - 24)

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
        super().keyPressEvent(e)

    def _cache(self, key, image):
        if len(self.images) >= IMAGE_CACHE_ENTRIES:
            self.images.pop(next(iter(self.images)))
        self.images[key] = image
        return image

    def _scaled(self, image):
        limit = self.max_image_width()
        if image.width() > limit:
            image = image.scaledToWidth(limit, Qt.TransformationMode.SmoothTransformation)
        return image

    def placeholder(self, key):
        """A small framed label for an image that cannot be shown."""
        alt = self.alts.get(key) or Path(QUrl(key).path()).name or 'image'
        text = f'🖼  {alt}'
        font = QFont(self.font())
        font.setPointSizeF(max(8.0, font.pointSizeF() * 0.8))
        metrics = QFontMetrics(font)
        width = min(self.max_image_width(), metrics.horizontalAdvance(text) + 20)
        height = metrics.height() + 10
        image = QImage(width, height, QImage.Format.Format_ARGB32_Premultiplied)
        image.fill(Qt.GlobalColor.transparent)
        palette = self.owner.current_palette()
        painter = QPainter(image)
        painter.setPen(QColor(palette['border']))
        painter.drawRect(0, 0, width - 1, height - 1)
        painter.setPen(QColor(palette['muted']))
        painter.setFont(font)
        painter.drawText(
            QRectF(10, 0, width - 14, height), Qt.AlignmentFlag.AlignVCenter, text
        )
        painter.end()
        return image

    def _local_path(self, url):
        if url.isLocalFile():
            return Path(url.toLocalFile())
        if url.scheme():
            return None
        path = Path(QUrl.fromPercentEncoding(url.path().encode()) if url.path() else '')
        if not path.is_absolute() and self.base_dir:
            path = Path(self.base_dir) / path
        return path

    def loadResource(self, kind, url):
        if kind != QTextDocument.ResourceType.ImageResource.value and kind != (
            QTextDocument.ResourceType.ImageResource
        ):
            return super().loadResource(kind, url)
        key = url.toString()
        if key in self.images:
            return self.images[key]
        if key in self.failed:
            return self._cache(key, self.placeholder(key))
        scheme = url.scheme().lower()
        if scheme in ('http', 'https'):
            self._fetch(key)
            image = QImage(1, 1, QImage.Format.Format_ARGB32_Premultiplied)
            image.fill(Qt.GlobalColor.transparent)
            return image
        path = self._local_path(url)
        image = QImage()
        try:
            if path is not None and path.is_file() and path.stat().st_size <= MAX_LOCAL_IMAGE_BYTES:
                reader = QImageReader(str(path))
                reader.setAutoTransform(True)
                size = reader.size()
                limit = self.max_image_width()
                if size.isValid() and size.width() > limit:
                    reader.setScaledSize(size.scaled(limit, 10**6, Qt.AspectRatioMode.KeepAspectRatio))
                image = reader.read()
                if image.isNull() and path.suffix.lower() in ('.svg', '.svgz'):
                    image = svg_image(path.read_bytes(), limit)
        except OSError:
            image = QImage()
        if image.isNull():
            self.failed.add(key)
            return self._cache(key, self.placeholder(key))
        return self._cache(key, image)

    def _fetch(self, key):
        if key in self.pending or not self.owner.remote_images:
            if not self.owner.remote_images:
                self.failed.add(key)
            return
        self.pending.add(key)

        def done(data):
            self.pending.discard(key)
            image = QImage()
            image.loadFromData(data)
            if image.isNull() and looks_like_svg(data):
                image = svg_image(data, self.max_image_width())
            if image.isNull():
                self.failed.add(key)
            else:
                self._cache(key, self._scaled(image))
            self._relayout.start()

        def failed(_error):
            self.pending.discard(key)
            self.failed.add(key)
            self._relayout.start()

        run_async(self, lambda: fetch_remote(key), done, failed)

    def _refresh_images(self):
        # Re-run layout so the fetched images replace their placeholders.
        # Images above the view change its height: when the view still shows
        # the heading it was sent to, go back to that heading, else keep the
        # scroll position.
        bar = self.verticalScrollBar()
        value = bar.value()
        held = self.owner._anchor_hold
        self.owner._apply_preview_html(self.owner._preview_html, force=True)
        if held and held[1] == value:
            QTimer.singleShot(0, lambda a=held[0]: self.owner.scroll_preview_to(a))
        else:
            QTimer.singleShot(0, lambda: bar.setValue(value))

    def reset_images(self):
        self.images.clear()
        self.failed.clear()


class PreviewMixin:
    """Edit/Preview switching, cached and (for large documents) background rendering."""

    def _build_preview(self):
        self.preview = PreviewBrowser(self)
        self.preview.setAccessibleName('Markdown preview')
        self.preview.setOpenLinks(False)
        self.preview.setOpenExternalLinks(False)
        self.preview.anchorClicked.connect(self.open_preview_link)
        self.preview.setFrameStyle(0)
        self.preview.setWordWrapMode(QTextOption.WrapMode.WrapAtWordBoundaryOrAnywhere)
        self.preview_headings = []
        self._preview_key = None
        self._pending_anchor = None
        self._anchor_hold = None
        self._nav_history = []
        return self.preview

    # Links and navigation --------------------------------------------------

    def open_preview_link(self, url):
        target = url.toString()
        kind = link_kind(target)
        if kind == 'web':
            QDesktopServices.openUrl(QUrl(target))
            return
        if kind == 'anchor':
            self.scroll_preview_to(target[1:], remember=True)
            return
        if kind == 'local' and self.path:
            raw, _, fragment = target.partition('#')
            path = (Path(self.path).parent / QUrl.fromPercentEncoding(raw.encode())).resolve()
            if path.is_file() and path.suffix.lower() in VIEW_EXTENSIONS:
                self.follow_local_link(path, fragment)
                return
            if path.is_dir() or not path.exists():
                self.flash_status(f'Not found: {raw}')
                return
            self.flash_status(f'Not a Markdown file: {path.name}')
            return
        self.flash_status('Blocked unsafe preview link')

    def flash_status(self, text, ms=1800):
        self.stat_right.setText(text)
        QTimer.singleShot(ms, self.cursor_changed)

    def _remember_position(self):
        self._nav_history.append((self.path, self.preview.verticalScrollBar().value()))
        del self._nav_history[:-50]

    def follow_local_link(self, path, fragment=''):
        if self.path and Path(self.path).resolve() == path:
            if fragment:
                self.scroll_preview_to(fragment, remember=True)
            return
        if not self.maybe_save():
            return
        self._remember_position()
        self.load(str(path))
        self._pending_anchor = fragment or None
        if not fragment:
            QTimer.singleShot(0, lambda: self.preview.verticalScrollBar().setValue(0))
        self.update_preview()

    def navigate_back(self):
        if not self._nav_history:
            self.flash_status('No previous location')
            return
        path, value = self._nav_history.pop()
        if path != self.path:
            if not self.maybe_save():
                return
            if path:
                self.load(path)
            self._pending_scroll = value
            self.update_preview()
        else:
            self.preview.verticalScrollBar().setValue(value)

    def scroll_preview_to(self, anchor, remember=False):
        """Scroll the Preview so the heading or footnote ANCHOR is at the top."""
        if not anchor:
            return False
        if remember:
            self._remember_position()
        names = {h[3] for h in self.preview_headings}
        if anchor not in names and not anchor.startswith('fn'):
            index = find_heading(self.preview_headings, anchor)
            if index is None:
                self.flash_status(f'No heading: {anchor}')
                return False
            anchor = self.preview_headings[index][3]
        self.preview.scrollToAnchor(anchor)
        self._anchor_hold = (anchor, self.preview.verticalScrollBar().value())
        return True

    # Rendering -------------------------------------------------------------

    def toggle_preview(self):
        ratio = (
            self.editor.verticalScrollBar().value()
            / max(1, self.editor.verticalScrollBar().maximum())
            if not self.previewing
            else self.preview.verticalScrollBar().value()
            / max(1, self.preview.verticalScrollBar().maximum())
        )
        self.set_previewing(not self.previewing)
        target = self.preview if self.previewing else self.editor
        QTimer.singleShot(
            0,
            lambda: target.verticalScrollBar().setValue(
                int(target.verticalScrollBar().maximum() * ratio)
            ),
        )

    def set_previewing(self, on):
        self.previewing = bool(on)
        self.stack.setCurrentIndex(1 if self.previewing else 0)
        self.preview_btn.setProperty('active', self.previewing)
        self.preview_btn.style().unpolish(self.preview_btn)
        self.preview_btn.style().polish(self.preview_btn)
        if not self.previewing:
            self.ensure_highlighting()
        if self.findbar.isVisible():
            self.replace_input.setVisible(self._find_replace and not self.previewing)
        self.update_preview()
        self.update_line_highlight()
        self.cursor_changed()
        (self.preview if self.previewing else self.editor).setFocus()

    def _preview_css(self):
        return preview_css(self.current_palette())

    def _apply_preview_html(self, rendered, force=False):
        if not force and getattr(self, '_preview_html', None) == rendered:
            return
        bar = self.preview.verticalScrollBar()
        had_preview = hasattr(self, '_preview_html') and not force
        old_max = bar.maximum()
        old_value = bar.value()
        ratio = (old_value / old_max) if had_preview and old_max > 0 else None
        self.preview.base_dir = str(Path(self.path).parent) if self.path else None
        self.preview.setHtml(rendered)
        self._preview_html = rendered
        anchor, self._pending_anchor = self._pending_anchor, None
        scroll = getattr(self, '_pending_scroll', None)
        self._pending_scroll = None
        if anchor:
            QTimer.singleShot(0, lambda a=anchor: self.scroll_preview_to(a))
        elif scroll is not None:
            QTimer.singleShot(0, lambda b=bar, v=scroll: b.setValue(v))
        elif ratio is not None:
            QTimer.singleShot(0, lambda b=bar, r=ratio: b.setValue(round(b.maximum() * r)))

    def _render_done(self, key, css, result):
        self.preview_headings = result.headings
        self.preview.alts = result.images
        self._preview_key = key
        self._apply_preview_html('<style>' + css + '</style>' + result.html)

    def update_preview(self):
        if not (hasattr(self, 'preview') and self.previewing):
            return
        text = self.editor.toPlainText()
        css = self._preview_css()
        key = (self.path, len(text), hash(text), hash(css))
        if key == self._preview_key:
            anchor, self._pending_anchor = self._pending_anchor, None
            if anchor:
                QTimer.singleShot(0, lambda: self.scroll_preview_to(anchor))
            return
        if len(text) > LARGE_DOCUMENT_CHARS:
            self._preview_generation += 1
            generation = self._preview_generation

            def done(result):
                if generation == self._preview_generation and self.previewing:
                    self._render_done(key, css, result)

            run_async(self, lambda: render_markdown(text), done)
            return
        self._render_done(key, css, render_markdown(text))

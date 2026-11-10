"""Document file I/O and crash recovery for the main window (mixins)."""

from __future__ import annotations

import json
import os
import re
import sys
import tempfile
import threading
import time
from pathlib import Path

from PySide6.QtCore import Qt, QUrl
from PySide6.QtGui import (
    QDesktopServices,
)
from PySide6.QtWidgets import (
    QApplication,
    QDialog,
    QFileDialog,
    QInputDialog,
    QLabel,
    QLineEdit,
    QListWidget,
    QListWidgetItem,
    QMessageBox,
    QVBoxLayout,
)

try:
    from .constants import MAX_LOAD_BYTES, VERSION
    from .dialogs import NeutralMessageBox, RecoveryDialog, neutral_notice, neutral_unsaved
    from .workers import run_async
    from . import recovery as recovery_snapshot
except ImportError:
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    from constants import MAX_LOAD_BYTES, VERSION
    from dialogs import NeutralMessageBox, RecoveryDialog, neutral_notice, neutral_unsaved
    from workers import run_async
    import recovery as recovery_snapshot


class RecoveryStore:
    """Serializes recovery writes (worker thread) against clears (GUI thread).

    A write scheduled before a clear must not resurrect a recovery file after
    the document was saved or discarded, so every clear bumps an epoch and a
    write only lands when its epoch is still current.
    """

    def __init__(self, write):
        self._write = write
        self._lock = threading.Lock()
        self.epoch = 0

    def write(self, path, payload, epoch):
        with self._lock:
            if epoch != self.epoch:
                return False
            path.parent.mkdir(parents=True, exist_ok=True)
            self._write(path, payload)
            return True

    def clear(self, path):
        with self._lock:
            self.epoch += 1
            try:
                path.unlink(missing_ok=True)
            except OSError:
                pass


class FileMixin:
    """Open, save, rename, recent files, Quick Open and the external-change watcher."""

    def suggested_filename(self):
        text = self.editor.toPlainText()
        m = re.search(r'^#{1,6}\s+(.+?)\s*$', text, re.M)
        base = m.group(1) if m else ' '.join(text.strip().split()[:6])
        base = re.sub(r'[\\\\/:*?"<>|]+', '', base).strip().rstrip('.')
        return (base[:80] or 'Untitled') + ('.md' if self.default_format == 'md' else '.txt')

    def atomic_write(self, path, text):
        p = Path(path)
        target = p.resolve() if p.is_symlink() else p
        target.parent.mkdir(parents=True, exist_ok=True)
        if target.exists() and not os.access(target, os.W_OK):
            raise PermissionError(f'{p.name} is read-only')
        payload = text.replace('\r\n', '\n').replace('\r', '\n')
        state_json = p.suffix.lower() == '.json' and 'nirunote' in str(p.parent)
        if self.line_ending == '\r\n' and not state_json:
            payload = payload.replace('\n', '\r\n')
        data = payload.encode('utf-8')
        data = (b'\xef\xbb\xbf' + data) if self.had_bom and not state_json else data
        fd, tmp = tempfile.mkstemp(
            prefix='.' + target.name + '.', suffix='.tmp', dir=str(target.parent)
        )
        try:
            with os.fdopen(fd, 'wb') as f:
                f.write(data)
                f.flush()
                os.fsync(f.fileno())
            os.replace(tmp, target)
            try:
                dfd = os.open(str(target.parent), os.O_DIRECTORY)
                os.fsync(dfd)
                os.close(dfd)
            except OSError:
                pass
        finally:
            if os.path.exists(tmp):
                os.unlink(tmp)

    def snapshot(self):
        if not self.editor.toPlainText().strip():
            return
        d = (
            Path(os.environ.get('XDG_STATE_HOME', Path.home() / '.local/state'))
            / 'nirunote/snapshots'
        )
        d.mkdir(parents=True, exist_ok=True)
        stamp = time.strftime('%Y%m%d-%H%M%S')
        base = (Path(self.path).stem if self.path else 'untitled')[:40]
        q = d / f'{base}-{stamp}.md'
        try:
            q.write_text(self.editor.toPlainText(), encoding='utf-8')
        except OSError:
            return
        files = sorted(d.glob('*.md'), key=lambda x: x.stat().st_mtime, reverse=True)
        for old in files[5:]:
            try:
                old.unlink()
            except OSError:
                pass

    def watch_current(self):
        for f in self.watcher.files():
            self.watcher.removePath(f)
        if self.path and Path(self.path).exists():
            self.watcher.addPath(self.path)
            try:
                self._external_mtime = Path(self.path).stat().st_mtime_ns
            except OSError:
                self._external_mtime = None

    def external_file_changed(self, path):
        if self._ignore_watch or self._external_prompt or not self.path or path != self.path:
            return
        p = Path(path)
        if not p.exists():
            neutral_notice(
                self,
                'File removed on disk',
                'This file was removed or moved outside NIRUNOTE. Your open text is retained. Use Save As to keep it.',
            )
            return
        try:
            mt = p.stat().st_mtime_ns
        except OSError:
            return
        if self._external_mtime == mt:
            return
        if self.read_only or not self.is_modified():
            self.load(self.path)
            return
        self._external_prompt = True
        box = NeutralMessageBox(self, 'File changed on disk', 'This file changed outside NIRUNOTE.')
        reloadb = box.addButton('Reload', QMessageBox.ButtonRole.ActionRole)
        keepb = box.addButton('Keep mine', QMessageBox.ButtonRole.AcceptRole)
        copyb = box.addButton('Save copy…', QMessageBox.ButtonRole.ActionRole)
        box.addButton('Cancel', QMessageBox.ButtonRole.RejectRole)
        box.exec()
        clicked = box.clickedButton()
        self._external_prompt = False
        if clicked == reloadb:
            self.load(self.path)
        elif clicked == copyb:
            self.save_as()
        else:
            self._external_mtime = mt
            self.watch_current()

    def update_file_actions(self):
        has_file = bool(self.path)
        for a in (
            getattr(self, 'rename_action', None),
            getattr(self, 'folder_action', None),
            getattr(self, 'path_action', None),
        ):
            if a:
                a.setEnabled(has_file)

    def rebuild_recent(self):
        self.recent_menu.clear()
        valid = [p for p in self.recent if Path(p).is_file()][:10]
        self.recent = valid
        if not valid:
            a = self.recent_menu.addAction('No recent files')
            a.setEnabled(False)
            return
        for p in valid:
            a = self.recent_menu.addAction(Path(p).name)
            a.setToolTip(p)
            a.triggered.connect(lambda checked=False, x=p: self.open_recent(x))
        self.recent_menu.addSeparator()
        self.recent_menu.addAction('Clear Recent').triggered.connect(self.clear_recent)

    def add_recent(self, p):
        p = str(Path(p).resolve())
        self.recent = [p] + [x for x in self.recent if x != p]
        self.recent = self.recent[:10]
        self.settings.setValue('recent', self.recent)
        self.rebuild_recent()

    def clear_recent(self):
        self.recent = []
        self.settings.setValue('recent', [])
        self.rebuild_recent()

    def current_ext(self):
        return Path(self.path).suffix.lower().lstrip('.') if self.path else self.default_format

    def maybe_save(self):
        if not self.is_modified():
            return True
        r = neutral_unsaved(self)
        if r == 'save':
            return self.save()
        if r == 'discard':
            self.backup_discarded_text()
            return True
        return False

    def new(self):
        if self.maybe_save():
            self.clear_recovery()
            self.reset_doc()

    def new_from_clipboard(self):
        if not self.maybe_save():
            return
        self.clear_recovery()
        self.reset_doc(QApplication.clipboard().text(), 'Clipboard.' + self.default_format)

    def open(self):
        if not self.maybe_save():
            return
        p, _ = QFileDialog.getOpenFileName(
            self,
            'Open text',
            self.last_open_folder,
            'Markdown (*.md *.markdown);;Text (*.txt);;All files (*)',
        )
        if p:
            self.last_open_folder = str(Path(p).parent)
            self.save_settings()
            self.load(p)

    def open_scratchpad(self):
        scratch_path = (
            Path(os.environ.get('XDG_STATE_HOME', Path.home() / '.local/state'))
            / 'nirunote/scratchpad.md'
        )
        scratch_path.parent.mkdir(parents=True, exist_ok=True)
        scratch_path.touch(exist_ok=True)
        self.load(scratch_path)
        self.setWindowTitle('NIRUNOTE Scratchpad')

    def open_recent(self, p):
        if self.maybe_save():
            self.load(p)

    def load(self, p, quiet=False):
        try:
            path = Path(p)
            size = path.stat().st_size
            if size > MAX_LOAD_BYTES:
                raise RuntimeError(f'Document is too large ({size} bytes).')
            raw = path.read_bytes()
            self.had_bom = raw.startswith(b'\xef\xbb\xbf')
            text = raw.decode('utf-8-sig')
            self.line_ending = '\r\n' if b'\r\n' in raw else '\n'
            self._loading = True
            self.editor.setPlainText(text)
            self._loading = False
            self.path = str(path)
            self.title.setText(path.name)
            self.set_modified(False)
            self.watch_current()
            if not self.read_only:
                self.clear_recovery()
                self.add_recent(path)
                self.last_document = self.path
                self.save_settings()
            self.update_document_stats()
            pos = int(self.settings.value('cursor/' + str(path.resolve()), 0))
            c = self.editor.textCursor()
            c.setPosition(min(pos, len(text)))
            self.editor.setTextCursor(c)
            self.cursor_changed()
            (self.preview if self.previewing else self.editor).setFocus()
            if self.read_only:
                self.setWindowTitle(f'{path.name} · read-only · NIRUNOTE')
            self.update_file_actions()
        except Exception as e:
            self._loading = False
            if not quiet:
                neutral_notice(self, 'Open failed', str(e))

    def save(self):
        if self.read_only:
            self.flash_status('Read-only viewer: saving is off')
            return False
        if not self.path:
            return self.save_as()
        try:
            self.snapshot()
            self._ignore_watch = True
            self.atomic_write(self.path, self.editor.toPlainText())
            self._ignore_watch = False
            self.set_modified(False)
            self.recovered = False
            self.clear_recovery()
            self.add_recent(self.path)
            self.last_document = self.path
            self.last_save_folder = str(Path(self.path).parent)
            self.save_settings()
            self.watch_current()
            self.update_file_actions()
            return True
        except Exception as e:
            neutral_notice(self, 'Save failed', str(e))
            return False

    def save_as(self):
        if self.read_only:
            self.flash_status('Read-only viewer: saving is off')
            return False
        ext = '.md' if self.default_format == 'md' else '.txt'
        base = (
            Path(self.default_save_folder).expanduser()
            if self.save_folder_mode == 'custom'
            else Path(self.last_save_folder).expanduser()
        )
        base = (
            base
            if base.is_dir()
            else (
                Path.home() / 'Documents' if (Path.home() / 'Documents').is_dir() else Path.home()
            )
        )
        p, _ = QFileDialog.getSaveFileName(
            self,
            'Save as',
            str(base / self.suggested_filename()),
            'Markdown (*.md *.markdown);;Text (*.txt)',
        )
        if not p:
            return False
        if not Path(p).suffix:
            p += ext
        old_path = self.path
        old_title = self.title.text()
        self.path = p
        if self.save():
            self.title.setText(Path(p).name)
            return True
        self.path = old_path
        self.title.setText(old_title)
        return False

    def rename(self):
        if self.read_only:
            return None
        if not self.path:
            return self.save_as()
        n, ok = QInputDialog.getText(self, 'Rename', 'New filename:', text=Path(self.path).name)
        if ok and n:
            try:
                q = Path(self.path).with_name(n)
                if q.exists() and q != Path(self.path):
                    raise FileExistsError(f'{q.name} already exists')
                Path(self.path).rename(q)
                self.path = str(q)
                self.title.setText(q.name)
                self.add_recent(q)
                self.watch_current()
            except Exception as e:
                neutral_notice(self, 'Rename failed', str(e))

    def open_containing_folder(self):
        if self.path:
            QDesktopServices.openUrl(QUrl.fromLocalFile(str(Path(self.path).parent)))

    def copy_file_path(self):
        if self.path:
            QApplication.clipboard().setText(str(Path(self.path)))

    def notes_root(self):
        p = (
            Path(self.default_save_folder).expanduser()
            if self.save_folder_mode == 'custom'
            else Path(self.last_save_folder).expanduser()
        )
        return p if p.is_dir() else Path.home()

    def quick_open(self):
        root = self.notes_root()
        files = []
        generation = self._spell_generation
        d = QDialog(self)
        d.setWindowTitle('Quick Open')
        d.setAccessibleName('Quick Open')
        d.resize(620, 460)
        v = QVBoxLayout(d)
        search = QLineEdit()
        search.setAccessibleName('Quick Open search')
        search.setPlaceholderText(f'Search {root}')
        status = QLabel('Scanning…')
        status.setAccessibleName('Quick Open status')
        lst = QListWidget()
        lst.setAccessibleName('Quick Open results')
        v.addWidget(search)
        v.addWidget(status)
        v.addWidget(lst, 1)

        def fill(q=''):
            lst.clear()
            terms = q.casefold().split()
            ranked = []
            for p in files:
                rel = str(p.relative_to(root))
                low = rel.casefold()
                if all(t in low for t in terms):
                    ranked.append((sum(low.find(t) for t in terms), rel, p))
            for _, rel, p in sorted(ranked)[:200]:
                item = QListWidgetItem(rel)
                item.setData(Qt.ItemDataRole.UserRole, str(p))
                lst.addItem(item)
            if lst.count():
                lst.setCurrentRow(0)
            status.setText(
                f'{len(ranked)} match(es)' if files else 'No Markdown or text files found'
            )

        cancel = threading.Event()
        d.finished.connect(lambda *_: cancel.set())

        def scan():
            found = []
            try:
                for p in root.rglob('*'):
                    if cancel.is_set():
                        break
                    if p.is_file() and p.suffix.lower() in ('.md', '.markdown', '.txt'):
                        found.append(p)
                    if len(found) >= 1500:
                        break
            except OSError:
                pass
            return found

        def loaded(found):
            if cancel.is_set():
                return
            files.extend(found)
            fill(search.text())

        def failed(error):
            status.setText(f'Scan failed: {error}')

        fill()
        search.textChanged.connect(fill)
        lst.itemActivated.connect(lambda item: d.accept())
        run_async(self, scan, loaded, failed)
        if d.exec() and lst.currentItem() and self.maybe_save():
            self.load(lst.currentItem().data(Qt.ItemDataRole.UserRole))

    def dragEnterEvent(self, e):
        urls = e.mimeData().urls()
        e.acceptProposedAction() if urls and urls[0].isLocalFile() and Path(
            urls[0].toLocalFile()
        ).suffix.lower() in ('.md', '.markdown', '.txt') else e.ignore()

    def dropEvent(self, e):
        p = e.mimeData().urls()[0].toLocalFile()
        if self.maybe_save():
            self.load(p)
            e.acceptProposedAction()


class RecoveryMixin:
    """Event-driven crash recovery snapshots and the startup recovery prompt."""

    def recovery_path(self):
        # The scratchpad instance runs next to the main window; keep its snapshot separate.
        name = 'scratchpad-recovery.json' if self.scratchpad else 'recovery.json'
        return (
            Path(os.environ.get('XDG_STATE_HOME', Path.home() / '.local/state')) / 'nirunote' / name
        )

    def schedule_recovery(self):
        if not self.autosave:
            return
        self.recovery_debounce.start()
        if not self.recovery_deadline.isActive():
            self.recovery_deadline.start()

    def recover(self, sync=False):
        self.recovery_debounce.stop()
        self.recovery_deadline.stop()
        if (
            not (self.autosave and self.is_modified())
            or self._text_generation == self._recovery_generation
        ):
            return
        if self._startup_recovery is not None:
            return
        if self._recovery_pending and not sync:
            self._recovery_again = True
            return
        generation = self._text_generation
        epoch = self.recovery_store.epoch
        payload = json.dumps(
            {
                'text': self.editor.toPlainText(),
                'path': self.path,
                'saved_at': time.time(),
                'app_version': VERSION,
                'pid': os.getpid(),
            },
            ensure_ascii=False,
        )
        path = self.recovery_path()
        self._recovery_generation = generation

        def write():
            return self.recovery_store.write(path, payload, epoch)

        if sync:
            try:
                write()
            except Exception:
                self._recovery_generation = -1
            return
        self._recovery_pending = True

        def finish():
            self._recovery_pending = False
            if self._recovery_again:
                self._recovery_again = False
                self.recover()

        def done(_):
            finish()

        def failed(_):
            # Retry on the next edit or deadline instead of losing the snapshot.
            if self._recovery_generation == generation:
                self._recovery_generation = -1
            finish()

        run_async(self, write, done, failed)

    def clear_recovery(self):
        self.recovery_debounce.stop()
        self.recovery_deadline.stop()
        self._recovery_generation = self._text_generation
        if self._startup_recovery is not None:
            return
        self.recovery_store.clear(self.recovery_path())

    def read_startup_recovery(self):
        """Remember a snapshot left by a crash; offer_recovery() asks about it.

        Until the user decided, the snapshot is never cleared or overwritten.
        """
        if not self.restore:
            return
        path = self.recovery_path()
        try:
            data = recovery_snapshot.read_snapshot(path)
        except ValueError:
            # Unreadable: keep it for inspection instead of overwriting it.
            try:
                os.replace(path, path.with_name(path.stem + '-unreadable.json'))
            except OSError:
                pass
            return
        if data and not recovery_snapshot.owner_alive(data):
            self._startup_recovery = data

    def offer_recovery(self):
        data = self._startup_recovery
        if not data:
            return None
        on_disk = recovery_snapshot.disk_text(data.get('path'))
        if on_disk is not None and on_disk == data['text']:
            # The file already holds this text (saved elsewhere); nothing to recover.
            self._startup_recovery = None
            self.move_recovery_aside()
            return 'identical'
        dialog = RecoveryDialog(self, data, on_disk, self.current_palette())
        dialog.exec()
        self._startup_recovery = None
        if dialog.choice == 'discard':
            self.move_recovery_aside()
            return 'discard'
        self.restore_snapshot(data)
        return 'restore'

    def restore_snapshot(self, data):
        path = data.get('path')
        known = bool(path and Path(path).is_file())
        self.reset_doc(
            data['text'], (Path(path).name + ' · recovered') if known else 'Recovered · unsaved'
        )
        if known:
            # Save goes back to the original file; the disk version was shown by Compare.
            self.path = str(path)
            self.watch_current()
            self.update_file_actions()
        self.recovered = True
        # The snapshot on disk already holds exactly this text.
        self.recovery_debounce.stop()
        self.recovery_deadline.stop()
        self._recovery_generation = self._text_generation
        self.update_document_stats()

    def discarded_recovery_path(self):
        return self.recovery_path().with_name('recovery-discarded.json')

    def move_recovery_aside(self):
        try:
            os.replace(self.recovery_path(), self.discarded_recovery_path())
        except OSError:
            pass

    def backup_discarded_text(self):
        # "Don't Save" keeps one copy of the dropped text next to the recovery file.
        if not self.editor.toPlainText():
            return
        payload = json.dumps(
            {
                'text': self.editor.toPlainText(),
                'path': self.path,
                'saved_at': time.time(),
                'app_version': VERSION,
            },
            ensure_ascii=False,
        )
        try:
            self.atomic_write(self.discarded_recovery_path(), payload)
        except Exception:
            pass

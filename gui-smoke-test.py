#!/usr/bin/env python3
import os, sys, tempfile, importlib.util
from pathlib import Path

with tempfile.TemporaryDirectory(prefix="nirunote-smoke-") as td:
    base = Path(td)
    os.environ["QT_QPA_PLATFORM"] = "offscreen"
    os.environ["XDG_CONFIG_HOME"] = str(base / "config")
    os.environ["XDG_STATE_HOME"] = str(base / "state")
    os.environ["XDG_DATA_HOME"] = str(base / "data")
    os.environ["XDG_CACHE_HOME"] = str(base / "cache")

    from PySide6.QtWidgets import QApplication

    root = Path(__file__).resolve().parent
    spec = importlib.util.spec_from_file_location("nirunote_app", root/"nirunote"/"app.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)

    qapp = QApplication(["nirunote-smoke-test"])
    qapp.setApplicationName("NIRUNOTE-SMOKE")
    qapp.setOrganizationName("NIRU-SMOKE")

    # Keyboard-only regression checks for the unsaved-changes dialog.
    from PySide6.QtCore import Qt, QTimer, QCoreApplication, QEvent
    from PySide6.QtGui import QKeyEvent
    from PySide6.QtWidgets import QDialog, QPushButton

    def send_key(key):
        widget=qapp.focusWidget()
        assert widget is not None, "No focused widget"
        QCoreApplication.postEvent(widget,QKeyEvent(QEvent.Type.KeyPress,key,Qt.KeyboardModifier.NoModifier))
        QCoreApplication.postEvent(widget,QKeyEvent(QEvent.Type.KeyRelease,key,Qt.KeyboardModifier.NoModifier))

    def check_unsaved_key(key, button_name, expected):
        def drive():
            dialog=qapp.activeModalWidget()
            assert isinstance(dialog,QDialog)
            buttons={b.text():b for b in dialog.findChildren(QPushButton)}
            assert set(buttons)=={"Don't Save",'Cancel','Save'}
            assert buttons['Cancel'].hasFocus(), 'Cancel must have initial focus'
            if button_name!='Cancel':
                buttons[button_name].setFocus()
                qapp.processEvents()
            assert buttons[button_name].hasFocus()
            QTest.keyClick(qapp.focusWidget(),key)
        QTimer.singleShot(0,drive)
        assert mod.neutral_unsaved(None)==expected

    for label,result in (("Don't Save",'discard'),('Cancel','cancel'),('Save','save')):
        check_unsaved_key(Qt.Key.Key_Return,label,result)
        check_unsaved_key(Qt.Key.Key_Enter,label,result)
        check_unsaved_key(Qt.Key.Key_Space,label,result)
    check_unsaved_key(Qt.Key.Key_Escape,'Cancel','cancel')

    def tab_drive():
        dialog=qapp.activeModalWidget()
        buttons={b.text():b for b in dialog.findChildren(QPushButton)}
        assert buttons['Cancel'].hasFocus()
        send_key(Qt.Key.Key_Tab)
        qapp.processEvents()
        assert buttons['Save'].hasFocus()
        send_key(Qt.Key.Key_Return)
    QTimer.singleShot(0,tab_drive)
    assert mod.neutral_unsaved(None)=='save'

    # Markdown renderer regression tests. Preserve ordinary paragraph folding,
    # but honor CommonMark-style explicit hard line breaks.
    hard = mod.markdown_to_html("**Rapportdatum:** 2026-09-28  \n**Författare:** Nicklas Rudolfsson  \n**E-post:** n@example.se")
    assert "2026-09-28<br><strong>Författare:</strong>" in hard, hard
    assert "Nicklas Rudolfsson<br><strong>E-post:</strong>" in hard, hard
    slash = mod.markdown_to_html("rad ett\\\nrad två")
    assert "rad ett<br>rad två" in slash, slash
    folded = mod.markdown_to_html("vanlig rad\nnästa rad")
    assert "vanlig rad nästa rad" in folded and "<br>" not in folded, folded

    # Safe inline HTML subset: useful Markdown HTML renders, active/unknown HTML does not.
    rich = mod.markdown_to_html("<small>Skapad med **NIRUNOTE** · [site](https://example.test)</small> <kbd>Ctrl+S</kbd> H<sub>2</sub>O<br>rad")
    assert "<small>Skapad med <strong>NIRUNOTE</strong>" in rich, rich
    assert "<kbd>Ctrl+S</kbd>" in rich and "H<sub>2</sub>O<br>rad" in rich, rich
    unsafe = mod.markdown_to_html("<script>alert(1)</script> <iframe src=x></iframe>")
    assert "&lt;script&gt;" in unsafe and "&lt;iframe" in unsafe, unsafe

    w = mod.Main()
    w.apply_theme()
    w.apply_editor_options()
    w.update_line_highlight()
    w.update_preview()
    w.copy_feedback()
    w.set_focus(True)
    w.set_focus(False)
    w.goto_heading if hasattr(w,'goto_heading') else None
    qapp.processEvents()

    # Long document Preview: verify scroll range and stable position on unchanged refresh.
    sample = ("# ORDNING test\n\n" + "\n".join(f"## Section {i}\n\nParagraph {i} with **Markdown**.\n" for i in range(450)))
    w.editor.setPlainText(sample)
    w.previewing = True
    w.stack.setCurrentIndex(1)
    w.update_preview()
    w.resize(850, 600)
    w.show()
    for _ in range(8):
        qapp.processEvents()
    bar = w.preview.verticalScrollBar()
    assert bar.maximum() > 0, "Long Preview must be scrollable"
    bar.setValue(bar.maximum() // 2)
    qapp.processEvents()
    middle = bar.value()
    w.update_preview()
    qapp.processEvents()
    assert abs(bar.value() - middle) <= 1, "Unchanged Preview moved scroll position"
    bar.setValue(bar.maximum())
    qapp.processEvents()
    assert bar.value() == bar.maximum(), "Cannot scroll to end of Preview"
    w.hide()

    # Do not call close(): Main.closeEvent intentionally contains user-facing
    # unsaved/recovery logic and must never be part of a non-interactive test.
    w.hide()
    w.deleteLater()
    qapp.processEvents()
    qapp.quit()

    print("NIRUNOTE GUI smoke test: OK", flush=True)
    sys.exit(0)

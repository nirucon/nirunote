from __future__ import annotations

import threading
from typing import Any, Callable

from PySide6.QtCore import QObject, Signal


class AsyncResult(QObject):
    done = Signal(object)
    failed = Signal(str)


def run_async(parent: QObject, function: Callable[..., Any], on_done=None, on_failed=None) -> AsyncResult:
    result = AsyncResult(parent)
    if on_done is not None:
        result.done.connect(on_done)
    if on_failed is not None:
        result.failed.connect(on_failed)

    def work() -> None:
        try:
            value = function()
        except Exception as error:
            result.failed.emit(str(error).strip() or error.__class__.__name__)
            return
        result.done.emit(value)

    threading.Thread(target=work, daemon=True, name='nirunote-worker').start()
    return result

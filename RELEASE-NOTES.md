# NIRUNOTE 0.4.5

- Fix installer GUI validation on distributions where PySide6.QtTest is not installed (including Void).
- Exercise keyboard events using QtCore/QtGui instead of an optional QtTest module.
- Retain the 0.4.4 keyboard activation fix, theme styling and safe Cancel default.
- No changes to documents, settings, or existing release rollback behavior.

The release is designed for Arch/Omarchy, Debian and Void with Python 3 and PySide6 QtWidgets; actual GUI compatibility should be checked on each target distribution.

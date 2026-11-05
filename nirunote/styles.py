"""Built-in palettes, writing widths and the Qt/Preview style sheets."""

from __future__ import annotations

THEMES = {
    'dark': {
        'bg': '#090909',
        'fg': '#ededed',
        'muted': '#858585',
        'panel': '#141414',
        'border': '#292929',
        'accent': '#bdbdbd',
        'select': '#333333',
        'line': '#0b0b0b',
        'md': '#6f6f6f',
    },
    'light': {
        'bg': '#f7f7f4',
        'fg': '#181818',
        'muted': '#777777',
        'panel': '#ecece8',
        'border': '#d2d2cc',
        'accent': '#444444',
        'select': '#dcdcd6',
        'line': '#f6f6f2',
        'md': '#9a9a92',
    },
    'satie': {
        'bg': '#ddd5c4',
        'fg': '#292b29',
        'muted': '#737168',
        'panel': '#cbc2af',
        'border': '#aaa18f',
        'accent': '#4f4c45',
        'select': '#bbb6a9',
        'line': '#ddd6c8',
        'md': '#8c887d',
    },
}
WIDTHS = {'Narrow': 720, 'Normal': 900, 'Wide': 1100, 'Full width': 16777215}


def palette_for(theme):
    return dict(THEMES.get(theme, THEMES['dark']))


def app_stylesheet(t):
    """Qt style sheet for the main window and dialogs, from a palette dict."""
    return f'''
QMainWindow,
QWidget,
QPlainTextEdit,
QTextBrowser,
QDialog {{
    background:{t['bg']};
    color:{t['fg']};
}}
QPlainTextEdit,
QTextBrowser {{
    border:none;
    padding:30px 18px;
    selection-background-color:{t['select']};
}}
QToolButton {{
    background:transparent;
    color:{t['muted']};
    border:none;
    border-radius:5px;
    padding:5px 8px;
}}
QToolButton:hover {{
    background:{t['panel']};
    color:{t['fg']};
}}
QMenu {{
    background:{t['panel']};
    color:{t['fg']};
    border:1px solid {t['border']};
    padding:6px;
}}
QMenu::item {{
    padding:7px 28px 7px 12px;
    border-radius:4px;
}}
QMenu::item:selected {{
    background:{t['select']};
}}
QMenu::separator {{
    height:1px;
    background:{t['border']};
    margin:5px 8px;
}}
QScrollBar:vertical {{
    background:transparent;
    width:4px;
    margin:0;
}}
QScrollBar::handle:vertical {{
    background:{t['border']};
    min-height:28px;
    border-radius:2px;
}}
QScrollBar::add-line:vertical,
QScrollBar::sub-line:vertical {{
    height:0;
}}
QScrollBar::add-page:vertical,
QScrollBar::sub-page:vertical {{
    background:transparent;
}}
QLabel {{
    color:{t['muted']};
}}
QLineEdit,
QComboBox,
QSpinBox {{
    background:{t['panel']};
    color:{t['fg']};
    border:1px solid {t['border']};
    border-radius:4px;
    padding:6px;
    selection-background-color:{t['select']};
    selection-color:{t['fg']};
}}
QLineEdit:focus,
QComboBox:focus,
QSpinBox:focus {{
    border:1px solid {t['muted']};
}}
QPushButton {{
    background:{t['panel']};
    color:{t['fg']};
    border:1px solid {t['border']};
    border-radius:4px;
    padding:6px 12px;
}}
QPushButton:hover {{
    background:{t['select']};
    border-color:{t['muted']};
}}
QPushButton:focus {{
    background:{t['select']};
    color:{t['fg']};
    border:2px solid {t['fg']};
    padding:5px 11px;
}}
QPushButton:pressed {{
    background:{t['select']};
    color:{t['fg']};
    border-color:{t['muted']};
}}
QPushButton:default {{
    background:{t['panel']};
    color:{t['fg']};
    border:1px solid {t['muted']};
}}
QPushButton:disabled {{
    color:{t['muted']};
    border-color:{t['border']};
}}
QCheckBox {{
    color:{t['fg']};
    spacing:7px;
}}
QCheckBox::indicator {{
    width:13px;
    height:13px;
    background:{t['bg']};
    border:1px solid {t['muted']};
    border-radius:2px;
}}
QCheckBox::indicator:checked {{
    background:{t['fg']};
    border-color:{t['fg']};
}}
QCheckBox::indicator:disabled {{
    background:{t['panel']};
    border-color:{t['border']};
}}
QListWidget {{
    background:{t['bg']};
    color:{t['fg']};
    border:1px solid {t['border']};
    selection-background-color:{t['select']};
    selection-color:{t['fg']};
}}
QMessageBox {{
    background:{t['bg']};
}}
QMessageBox QLabel {{
    color:{t['fg']};
    font-size:13px;
}}
QMessageBox[nirunoteDialog="true"],
QDialog[nirunoteDialog="true"] {{
    background:{t['bg']};
}}
QMessageBox[nirunoteDialog="true"] QLabel,
QDialog[nirunoteDialog="true"] QLabel {{
    color:{t['fg']};
    padding:4px 8px 8px 8px;
}}
QMessageBox[nirunoteDialog="true"] QPushButton,
QDialog[nirunoteDialog="true"] QPushButton {{
    min-width:88px;
    min-height:30px;
    background:{t['panel']};
    color:{t['fg']};
    border:1px solid {t['border']};
    border-radius:4px;
    padding:0px 12px;
}}
QMessageBox[nirunoteDialog="true"] QPushButton:hover,
QDialog[nirunoteDialog="true"] QPushButton:hover {{
    background:{t['select']};
    border-color:{t['muted']};
}}
QMessageBox[nirunoteDialog="true"] QPushButton:focus,
QDialog[nirunoteDialog="true"] QPushButton:focus {{
    background:{t['select']};
    color:{t['fg']};
    border:2px solid {t['fg']};
    font-weight:600;
}}
QPushButton:focus {{
    background:{t['select']};
    color:{t['fg']};
    border:2px solid {t['fg']};
    padding:5px 11px;
}}
QMessageBox[nirunoteDialog="true"] QPushButton:pressed,
QDialog[nirunoteDialog="true"] QPushButton:pressed {{
    background:{t['select']};
    border-color:{t['fg']};
}}
QDialogButtonBox QPushButton {{
    min-width:72px;
}}
QToolButton[headerButton="true"] {{
    min-width:28px;
    max-width:28px;
    min-height:28px;
    max-height:28px;
    padding:0px;
    margin:0px;
    border:0px;
    background:transparent;
    color:{t['muted']};
    font-size:13px;
}}
QToolButton[headerButton="true"]:hover {{
    background:{t['select']};
    color:{t['fg']};
    border-radius:4px;
}}
QToolButton[headerButton="true"]:pressed {{
    background:{t['panel']};
    color:{t['fg']};
}}
QToolButton::menu-indicator {{
    image:none;
    width:0px;
    height:0px;
}}
'''


def preview_css(t):
    """CSS for the Markdown Preview (QTextBrowser), from a palette dict."""
    return f'''
body {{
    line-height:1.40;
    color:{t['fg']};
    background:{t['bg']};
    margin:0;
}}
a {{
    color:{t['accent']};
}}
small {{
    font-size:.78em;
    color:{t['muted']};
}}
kbd {{
    font-family:monospace;
    font-size:.86em;
    background:{t['panel']};
    border:1px solid {t['border']};
    border-radius:3px;
    padding:1px 4px;
}}
sub,
sup {{
    font-size:.75em;
}}
mark {{
    background:{t['select']};
    color:{t['fg']};
    padding:0 2px;
}}
p {{
    margin:.16em 0 .68em;
}}
code {{
    background:{t['panel']};
    padding:2px 5px;
    border-radius:3px;
}}
pre {{
    background:{t['panel']};
    padding:12px 14px;
    margin:.55em 0 .85em;
    white-space:pre-wrap;
}}
pre code {{
    padding:0;
    background:transparent;
}}
blockquote {{
    color:{t['muted']};
    border-left:3px solid {t['border']};
    padding-left:12px;
    margin:.40em 0 .75em;
}}
h1 {{
    font-size:1.55em;
    margin:1.05em 0 .38em;
}}
h2 {{
    font-size:1.32em;
    margin:1em 0 .42em;
}}
h3 {{
    font-size:1.16em;
    margin:.82em 0 .30em;
}}
h4,
h5,
h6 {{
    margin:.72em 0 .28em;
}}
ul,
ol {{
    margin:.25em 0 .72em;
    padding-left:1.55em;
}}
li {{
    margin:.10em 0;
}}
hr {{
    border:0;
    border-top:1px solid {t['border']};
    margin:.9em 0 1.05em;
}}
'''

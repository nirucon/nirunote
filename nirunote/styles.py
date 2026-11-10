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
        'red': '#c97a7a',
        'green': '#8fb08f',
        'yellow': '#c6b37f',
        'blue': '#84a2c0',
        'magenta': '#ad94b8',
        'cyan': '#84afad',
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
        'red': '#a33b3b',
        'green': '#3d7a46',
        'yellow': '#86650c',
        'blue': '#2f5f8f',
        'magenta': '#7a4a8f',
        'cyan': '#2f7470',
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
        'red': '#8f3f35',
        'green': '#4d6b3f',
        'yellow': '#755a18',
        'blue': '#3f5a75',
        'magenta': '#6b4a6b',
        'cyan': '#3d6b66',
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
QScrollBar:horizontal {{
    background:transparent;
    height:4px;
    margin:0;
}}
QScrollBar::handle:horizontal {{
    background:{t['border']};
    min-width:28px;
    border-radius:2px;
}}
QScrollBar::add-line:horizontal,
QScrollBar::sub-line:horizontal {{
    width:0;
}}
QScrollBar::add-page:horizontal,
QScrollBar::sub-page:horizontal {{
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
QLabel#readOnlyBadge {{
    color:{t['bg']};
    background:{t['accent']};
    border-radius:3px;
    padding:1px 7px;
    font-size:10px;
    font-weight:600;
}}
QToolButton::menu-indicator {{
    image:none;
    width:0px;
    height:0px;
}}
'''


ANSI_DEFAULTS = {
    'red': '#c97a7a',
    'green': '#8fb08f',
    'yellow': '#c6b37f',
    'blue': '#84a2c0',
    'magenta': '#ad94b8',
    'cyan': '#84afad',
}


def preview_css(t):
    """CSS for the Markdown Preview (QTextBrowser), from a palette dict.

    QTextBrowser understands a subset of CSS: no padding on paragraphs, so
    boxes (code, quotes, alerts) are single-cell tables whose cells carry the
    background, the padding and the left bar.
    """
    try:
        from .theme import mix
    except ImportError:
        from theme import mix
    c = dict(ANSI_DEFAULTS)
    c.update({k: v for k, v in t.items() if k in ANSI_DEFAULTS})
    bg, fg, muted, border = t['bg'], t['fg'], t['muted'], t['border']
    code_bg = t['panel'] if t['panel'] != bg else mix(bg, fg, 0.05)
    quote_fg = mix(fg, muted, 0.45)
    odd = mix(bg, fg, 0.025)
    alerts = {
        'note': c['blue'],
        'tip': c['green'],
        'important': c['magenta'],
        'warning': c['yellow'],
        'caution': c['red'],
    }
    alert_css = ''.join(
        f"""
td.a-{name} {{
    border-left:3px solid {color};
    background:{mix(bg, color, 0.07)};
    padding:8px 14px 2px 14px;
}}
p.at-{name} {{
    color:{color};
    margin:0 0 4px 0;
}}"""
        for name, color in alerts.items()
    )
    return f"""
body {{
    color:{fg};
}}
a {{
    color:{t['accent']};
}}
small {{
    font-size:.82em;
    color:{muted};
}}
kbd {{
    font-family:monospace;
    font-size:.86em;
    background:{code_bg};
}}
sub,
sup {{
    font-size:.75em;
}}
mark {{
    background:{t['select']};
    color:{fg};
}}
p {{
    margin:0 0 12px 0;
    line-height:125%;
}}
p.end {{
    margin-bottom:0;
}}
li {{
    margin:0 0 3px 0;
    line-height:125%;
}}
li p {{
    margin:0 0 8px 0;
}}
li.last {{
    margin-bottom:12px;
}}
code {{
    font-family:monospace;
    font-size:.9em;
    background:{code_bg};
}}
pre {{
    font-family:monospace;
    font-size:.88em;
    margin:0;
    white-space:pre-wrap;
    line-height:125%;
}}
table.code {{
    margin:2px 0 14px 0;
}}
table.code td {{
    background:{code_bg};
    padding:10px 14px;
}}
p.lang {{
    font-size:.72em;
    color:{muted};
    margin:0 0 4px 0;
}}
table.quote,
table.alert {{
    margin:2px 0 14px 0;
}}
td.qbar {{
    border-left:3px solid {border};
    padding:2px 0 0 14px;
    color:{quote_fg};
}}
{alert_css}
table.grid {{
    border-collapse:collapse;
    margin:2px 0 14px 0;
}}
table.grid th,
table.grid td {{
    border:1px solid {border};
    padding:4px 10px 3px 10px;
    line-height:115%;
}}
table.grid th {{
    background:{code_bg};
    font-weight:600;
}}
tr.odd td {{
    background:{odd};
}}
h1, h2, h3, h4, h5, h6 {{
    color:{fg};
    font-weight:600;
}}
h1 {{
    font-size:1.75em;
    margin:18px 0 12px 0;
}}
h2 {{
    font-size:1.4em;
    margin:22px 0 10px 0;
}}
h3 {{
    font-size:1.18em;
    margin:18px 0 8px 0;
}}
h4 {{
    font-size:1.05em;
    margin:14px 0 6px 0;
}}
h5,
h6 {{
    font-size:1em;
    margin:12px 0 6px 0;
}}
h6 {{
    color:{muted};
}}
hr {{
    margin:10px 0 16px 0;
    background-color:{border};
}}
span.done {{
    color:{c['green']};
}}
span.todo {{
    color:{muted};
}}
ol.footnotes {{
    font-size:.88em;
    color:{muted};
}}
span.tk {{
    color:{c['magenta']};
}}
span.ts {{
    color:{c['green']};
}}
span.tc {{
    color:{muted};
    font-style:italic;
}}
span.tn {{
    color:{c['yellow']};
}}
span.tb {{
    color:{c['cyan']};
}}
span.tf {{
    color:{c['blue']};
}}
span.tp {{
    color:{c['yellow']};
}}
span.ta {{
    color:{c['green']};
}}
span.td {{
    color:{c['red']};
}}
span.th {{
    color:{c['cyan']};
}}
"""

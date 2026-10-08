#!/usr/bin/env python3
import os, sys, json, html, re, tempfile, time, subprocess, shutil, threading
from pathlib import Path
from PySide6.QtCore import Qt, QTimer, QSettings, QUrl, QSize, QFileSystemWatcher, QDir, Signal, QObject
from PySide6.QtGui import QAction, QFont, QKeySequence, QTextCharFormat, QColor, QSyntaxHighlighter, QTextCursor, QTextFormat, QShortcut, QDesktopServices, QTextDocument
from PySide6.QtWidgets import (QApplication,QMainWindow,QPlainTextEdit,QTextEdit,QTextBrowser,QFileDialog,QMessageBox,QMenu,QToolButton,QLabel,QWidget,QHBoxLayout,QVBoxLayout,QDialog,QFormLayout,QComboBox,QSpinBox,QCheckBox,QDialogButtonBox,QLineEdit,QInputDialog,QStackedWidget,QFrame,QPushButton,QListWidget,QListWidgetItem)



APP='NIRUNOTE'; VERSION='0.4.0'
LARGE_DOCUMENT_CHARS=50000
THEMES={
'dark': {'bg':'#090909','fg':'#ededed','muted':'#858585','panel':'#141414','border':'#292929','accent':'#bdbdbd','select':'#333333','line':'#0b0b0b','md':'#6f6f6f'},
'light': {'bg':'#f7f7f4','fg':'#181818','muted':'#777777','panel':'#ecece8','border':'#d2d2cc','accent':'#444444','select':'#dcdcd6','line':'#f6f6f2','md':'#9a9a92'},
'satie': {'bg':'#ddd5c4','fg':'#292b29','muted':'#737168','panel':'#cbc2af','border':'#aaa18f','accent':'#4f4c45','select':'#bbb6a9','line':'#ddd6c8','md':'#8c887d'} }
WIDTHS={'Narrow':720,'Normal':900,'Wide':1100,'Full width':16777215}

class SpellResult(QObject):
    finished = Signal(object)

class HunspellDictionary:
    def __init__(self, base, personal=None):
        self.base=str(base); self.exe=shutil.which('hunspell'); self.cache={}
        self.personal=Path(personal) if personal else None
        self.personal_words=set()
        self.lock=threading.RLock()
        self.results=SpellResult()
        self.inflight=False
        if self.personal and self.personal.exists():
            try: self.personal_words={w.strip().casefold() for w in self.personal.read_text(encoding='utf-8').splitlines() if w.strip()}
            except Exception: self.personal_words=set()
        if not self.exe or not Path(self.base+'.dic').is_file() or not Path(self.base+'.aff').is_file():
            raise RuntimeError('hunspell or dictionary unavailable')
    def check(self, word):
        key=word.casefold()
        if key in self.personal_words: return True
        if key in self.cache: return self.cache[key]
        try:
            p=subprocess.run([self.exe,'-d',self.base,'-l'],input=word+'\n',text=True,capture_output=True,timeout=1.5)
            # hunspell -l reports spelling through stdout; that output is authoritative.
            ok=(not p.stdout.strip())
        except Exception: ok=True
        self.cache[key]=ok
        if len(self.cache)>12000: self.cache.clear()
        return ok
    def cached(self, word):
        key=word.casefold()
        if key in self.personal_words: return True
        with self.lock: return self.cache.get(key)
    def check_many(self, words):
        # Called from the worker only. Never block the Qt GUI event loop.
        with self.lock:
            pending={w.casefold():w for w in words if w and w.casefold() not in self.personal_words and w.casefold() not in self.cache}
        if not pending: return
        try:
            p=subprocess.run([self.exe,'-d',self.base,'-l'],input='\n'.join(pending.values())+'\n',text=True,capture_output=True,timeout=4)
            if p.returncode not in (0,1): raise RuntimeError('hunspell failed')
            bad={w.strip().casefold() for w in p.stdout.splitlines() if w.strip()}
            with self.lock:
                for key in pending: self.cache[key]=(key not in bad)
        except Exception:
            # Fail open: spelling errors must not interrupt writing.
            with self.lock:
                for key in pending: self.cache[key]=True

    def add_word(self, word):
        w=word.strip()
        if not w or not self.personal: return
        self.personal.parent.mkdir(parents=True,exist_ok=True)
        if w.casefold() not in self.personal_words:
            with self.personal.open('a',encoding='utf-8') as f: f.write(w+'\n')
            self.personal_words.add(w.casefold())
        self.cache.pop(w.casefold(),None)

    def suggest(self, word):
        try:
            p=subprocess.run([self.exe,'-d',self.base,'-a'],input=word+'\n',text=True,capture_output=True,timeout=2)
            for line in p.stdout.splitlines():
                if line.startswith('&') and ':' in line:
                    return [x.strip() for x in line.split(':',1)[1].split(',') if x.strip()]
                if line.startswith('#'): return []
        except Exception: pass
        return []

class MarkdownHighlighter(QSyntaxHighlighter):
    def __init__(self, doc, window): super().__init__(doc); self.window=window
    def fmt(self,col,bold=False,italic=False):
        f=QTextCharFormat(); f.setForeground(QColor(col)); f.setFontWeight(700 if bold else 400); f.setFontItalic(italic); return f
    def highlightBlock(self,text):
        t=THEMES[self.window.theme]
        # Syntax markers are deliberately muted; content remains dominant.
        for m in re.finditer(r'^(#{1,6})(\s+)(.*)$',text):
            level=len(m.group(1))
            self.setFormat(m.start(1),level,self.fmt(t['md']))
            f=self.fmt(t['accent'],True)
            f.setFontPointSize(max(11, self.window.font_size + (4 if level==1 else 2 if level==2 else 1 if level==3 else 0)))
            self.setFormat(m.start(3),len(m.group(3)),f)
        for m in re.finditer(r'\*\*([^*]+)\*\*',text):
            self.setFormat(m.start(),2,self.fmt(t['md'])); self.setFormat(m.start(1),len(m.group(1)),self.fmt(t['fg'],True)); self.setFormat(m.end()-2,2,self.fmt(t['md']))
        for m in re.finditer(r'(?<!\*)\*([^*]+)\*(?!\*)',text):
            self.setFormat(m.start(),1,self.fmt(t['md'])); self.setFormat(m.start(1),len(m.group(1)),self.fmt(t['fg'],False,True)); self.setFormat(m.end()-1,1,self.fmt(t['md']))
        for m in re.finditer(r'`([^`]+)`',text): self.setFormat(m.start(),m.end()-m.start(),self.fmt(t['accent']))
        for m in re.finditer(r'^\s*([-*+]\s+|\d+[.)]\s+)',text): self.setFormat(m.start(1),len(m.group(1)),self.fmt(t['md'],True))
        for m in re.finditer(r'^\s*[-*+]\s+(\[[ xX]\])',text):
            self.setFormat(m.start(1),len(m.group(1)),self.fmt(t['accent'],True))
        for m in re.finditer(r'^(>\s?).*$',text): self.setFormat(m.start(),len(m.group(1)),self.fmt(t['md'])); self.setFormat(m.end(1),len(text)-m.end(1),self.fmt(t['muted'],False,True))
        for m in re.finditer(r'\[([^]]+)\]\(([^)]+)\)',text): self.setFormat(m.start(),m.end()-m.start(),self.fmt(t['accent']))
        if text.startswith('```'): self.setFormat(0,len(text),self.fmt(t['md']))
        # Local/offline live spelling; skip URLs and inline code.
        if self.window.spell_dict and self.window.spell_enabled and not getattr(self.window,'spell_deferred',False) and self.window.editor.document().characterCount() <= LARGE_DOCUMENT_CHARS:
            excluded=[]
            for pat in (r'`[^`]*`',r'https?://\S+',r'www\.\S+'):
                excluded.extend((x.start(),x.end()) for x in re.finditer(pat,text))
            for m in re.finditer(r"(?u)\b[^\W\d_][^\W\d_'-]*\b",text):
                a,b=m.span(); word=m.group()
                if any(x<=a and b<=y for x,y in excluded): continue
                if a and text[a-1] in '#[]()!*_>': continue
                # Highlighting is paint-path code: cached lookup only.
                # Hunspell subprocess work is batched after idle instead.
                try: correct=self.window.spell_dict.cached(word)
                except Exception: correct=True
                if correct is False:
                    f=QTextCharFormat()
                    f.setUnderlineStyle(QTextCharFormat.UnderlineStyle.SpellCheckUnderline)
                    f.setUnderlineColor(QColor(t['muted']))
                    self.setFormat(a,b-a,f)

class Prefs(QDialog):
    def __init__(self,w):
        super().__init__(w); self.w=w; self.setWindowTitle('NIRUNOTE Preferences'); self.setModal(True); self.setMinimumWidth(390)
        f=QFormLayout(self); f.setContentsMargins(22,22,22,22); f.setSpacing(12)
        self.theme=QComboBox(); self.theme.addItems(['dark','light','satie']); self.theme.setCurrentText(w.theme)
        self.fmt=QComboBox(); self.fmt.addItems(['Markdown (.md)','Plain text (.txt)']); self.fmt.setCurrentIndex(0 if w.default_format=='md' else 1)
        self.size=QSpinBox(); self.size.setRange(10,36); self.size.setValue(w.font_size)
        self.width=QComboBox(); self.width.addItems(list(WIDTHS)); self.width.setCurrentText(w.writing_width)
        self.wrap=QCheckBox(); self.wrap.setChecked(w.word_wrap)
        self.line=QCheckBox(); self.line.setChecked(w.highlight_line)
        self.autosave=QCheckBox(); self.autosave.setChecked(w.autosave)
        self.restore=QCheckBox(); self.restore.setChecked(w.restore)
        self.typewriter=QCheckBox(); self.typewriter.setChecked(w.typewriter)
        self.spell=QCheckBox(); self.spell.setChecked(w.spell_enabled)
        self.spell_lang=QComboBox(); self.spell_lang.addItems(['sv_SE','en_US','auto']); self.spell_lang.setCurrentText(w.spell_language)
        self.save_mode=QComboBox(); self.save_mode.addItems(['Last used folder','Custom folder']); self.save_mode.setCurrentIndex(1 if w.save_folder_mode=='custom' else 0)
        self.save_folder=QLineEdit(w.default_save_folder); browse=QPushButton('Choose…'); browse.clicked.connect(self.choose_folder); row=QWidget(); rh=QHBoxLayout(row); rh.setContentsMargins(0,0,0,0); rh.addWidget(self.save_folder,1); rh.addWidget(browse)
        self.restore_last=QCheckBox(); self.restore_last.setChecked(w.restore_last_document)
        self.reading_time=QCheckBox(); self.reading_time.setChecked(w.show_reading_time)
        for label,widget in [('Theme',self.theme),('Default format',self.fmt),('Default save location',self.save_mode),('Custom save folder',row),('Restore last document',self.restore_last),('Show reading time',self.reading_time),('Writing width',self.width),('Font size',self.size),('Word wrap',self.wrap),('Highlight current line',self.line),('Recovery autosave',self.autosave),('Restore recovery',self.restore),('Typewriter mode',self.typewriter),('Spell checking',self.spell),('Spell language',self.spell_lang)]: f.addRow(label,widget)
        b=QDialogButtonBox(QDialogButtonBox.StandardButton.Ok|QDialogButtonBox.StandardButton.Cancel); b.accepted.connect(self.accept); b.rejected.connect(self.reject); f.addRow(b)
    def choose_folder(self):
        p=QFileDialog.getExistingDirectory(self,'Choose default save folder',self.save_folder.text() or str(Path.home()))
        if p: self.save_folder.setText(p)
    def accept(self):
        w=self.w; w.theme=self.theme.currentText(); w.default_format='md' if self.fmt.currentIndex()==0 else 'txt'; w.writing_width=self.width.currentText(); w.font_size=self.size.value(); w.word_wrap=self.wrap.isChecked(); w.highlight_line=self.line.isChecked(); w.autosave=self.autosave.isChecked(); w.restore=self.restore.isChecked(); w.typewriter=self.typewriter.isChecked(); w.spell_enabled=self.spell.isChecked(); w.spell_language=self.spell_lang.currentText(); w.save_folder_mode='custom' if self.save_mode.currentIndex()==1 else 'last'; w.default_save_folder=self.save_folder.text().strip(); w.restore_last_document=self.restore_last.isChecked(); w.show_reading_time=self.reading_time.isChecked(); w.init_spell(); w.high.rehighlight(); w.changed(); w.save_settings(); w.apply_theme(); w.apply_editor_options(); super().accept()

def markdown_to_html(s):
    """Small dependency-free Markdown renderer for NIRUNOTE preview.

    It deliberately covers the writing primitives NIRUNOTE exposes while keeping
    blank source lines structural instead of rendering them as empty paragraphs.
    """
    def inline(text):
        # Safe, inert HTML subset commonly useful inside Markdown. Unknown HTML
        # remains escaped text; scripts, styles, iframes and attributes are never enabled.
        tokens=[]
        pat=re.compile(r'<\s*(/?)\s*(small|kbd|sub|sup|mark)\s*>|<\s*br\s*/?\s*>',re.I)
        def preserve(m):
            raw=m.group(0); low=raw.lower()
            if re.match(r'<\s*br',low): safe='<br>'
            else:
                closing=bool(m.group(1)); tag=m.group(2).lower(); safe=f'</{tag}>' if closing else f'<{tag}>'
            tokens.append(safe); return f'@@NIRUHTML{len(tokens)-1}@@'
        text=pat.sub(preserve,text)
        text=html.escape(text)
        text=re.sub(r'`([^`]+)`',r'<code>\1</code>',text)
        text=re.sub(r'\[([^]]+)\]\(([^)]+)\)',r'<a href="\2">\1</a>',text)
        text=re.sub(r'\*\*(.+?)\*\*',r'<strong>\1</strong>',text)
        text=re.sub(r'(?<!\*)\*([^*]+)\*(?!\*)',r'<em>\1</em>',text)
        for i,tag in enumerate(tokens): text=text.replace(f'@@NIRUHTML{i}@@',tag)
        return text

    out=[]; paragraph=[]; list_kind=None; in_code=False; code=[]
    def flush_paragraph():
        nonlocal paragraph
        if paragraph:
            # CommonMark-style paragraph folding: a normal source newline becomes
            # a space, while two trailing spaces or a trailing backslash request
            # an explicit hard line break. This keeps prose compact without
            # destroying deliberate metadata/address-style line breaks.
            parts=[]
            for source_line in paragraph:
                hard_break = source_line.endswith('  ') or source_line.endswith('\\')
                content = source_line[:-1] if source_line.endswith('\\') else source_line.rstrip()
                parts.append(inline(content.strip()) + ('<br>' if hard_break else ''))
            rendered=' '.join(parts)
            # Do not leave a visual space after an explicit <br>.
            rendered=rendered.replace('<br> ', '<br>')
            out.append('<p>'+rendered+'</p>')
            paragraph=[]
    def close_list():
        nonlocal list_kind
        if list_kind:
            out.append(f'</{list_kind}>'); list_kind=None
    def open_list(kind):
        nonlocal list_kind
        if list_kind != kind:
            close_list(); out.append(f'<{kind}>'); list_kind=kind

    for raw in s.splitlines():
        # Preserve trailing spaces here: Markdown uses two trailing spaces as
        # an explicit hard line break. Whitespace is normalized only after
        # the paragraph renderer has inspected the original source line.
        line=raw
        if line.startswith('```'):
            flush_paragraph(); close_list()
            if in_code:
                out.append('<pre><code>'+html.escape('\n'.join(code))+'</code></pre>'); code=[]; in_code=False
            else:
                in_code=True
            continue
        if in_code:
            code.append(raw); continue
        if not line.strip():
            flush_paragraph(); close_list(); continue
        m=re.match(r'^(#{1,6})\s+(.+)$',line)
        if m:
            flush_paragraph(); close_list(); level=len(m.group(1)); out.append(f'<h{level}>'+inline(m.group(2))+f'</h{level}>'); continue
        if re.match(r'^\s*(---+|___+|\*\*\*+)\s*$',line):
            flush_paragraph(); close_list(); out.append('<hr>'); continue
        m=re.match(r'^\s*>\s?(.*)$',line)
        if m:
            flush_paragraph(); close_list(); out.append('<blockquote>'+inline(m.group(1))+'</blockquote>'); continue
        m=re.match(r'^\s*[-*+]\s+\[([ xX])\]\s+(.*)$',line)
        if m:
            flush_paragraph(); open_list('ul'); mark='☑' if m.group(1).lower()=='x' else '☐'; out.append('<li>'+mark+' '+inline(m.group(2))+'</li>'); continue
        m=re.match(r'^\s*[-*+]\s+(.*)$',line)
        if m:
            flush_paragraph(); open_list('ul'); out.append('<li>'+inline(m.group(1))+'</li>'); continue
        m=re.match(r'^\s*\d+[.)]\s+(.*)$',line)
        if m:
            flush_paragraph(); open_list('ol'); out.append('<li>'+inline(m.group(1))+'</li>'); continue
        close_list(); paragraph.append(line)
    if in_code: out.append('<pre><code>'+html.escape('\n'.join(code))+'</code></pre>')
    flush_paragraph(); close_list()
    return '\n'.join(out)

class NoteEditor(QPlainTextEdit):
    def __init__(self, owner):
        super().__init__(); self.owner=owner

    def wheelEvent(self,e):
        if e.modifiers() & Qt.KeyboardModifier.ControlModifier:
            self.owner.resize_font(1 if e.angleDelta().y()>0 else -1); e.accept(); return
        super().wheelEvent(e)

    def keyPressEvent(self,e):
        if e.key()==Qt.Key.Key_Escape and self.owner.focused:
            self.owner.set_focus(False); return

        # Indent/outdent Markdown list items without inserting literal tabs.
        if e.key()==Qt.Key.Key_Tab and not (e.modifiers() & ~Qt.KeyboardModifier.ShiftModifier):
            c=self.textCursor(); line=c.block().text()
            if re.match(r'^\s*(?:[-*+]\s+(?:\[[ xX]\]\s+)?|\d+[.)]\s+)',line):
                c.movePosition(QTextCursor.MoveOperation.StartOfBlock); c.insertText('  '); return
        if e.key()==Qt.Key.Key_Backtab:
            c=self.textCursor(); line=c.block().text()
            if line.startswith('  '):
                c.movePosition(QTextCursor.MoveOperation.StartOfBlock); c.movePosition(QTextCursor.MoveOperation.Right,QTextCursor.MoveMode.KeepAnchor,2); c.removeSelectedText(); return

        # Writing-oriented Markdown list / quote continuation.
        if e.key() in (Qt.Key.Key_Return, Qt.Key.Key_Enter) and not (e.modifiers() & Qt.KeyboardModifier.ShiftModifier):
            c=self.textCursor(); line=c.block().text()
            m=re.match(r'^(\s*)([-*+]\s+|\d+[.)]\s+|[-*+]\s+\[(?: |x|X)\]\s+)(.*)$',line)
            if m:
                indent, token, body=m.groups()
                if not body.strip():
                    c.select(QTextCursor.SelectionType.BlockUnderCursor); c.removeSelectedText()
                    super().keyPressEvent(e); return
                super().keyPressEvent(e)
                if re.match(r'\d+',token):
                    n=int(re.match(r'\d+',token).group())+1
                    sep=')' if ')' in token else '.'
                    self.insertPlainText(f'{indent}{n}{sep} ')
                elif '[' in token:
                    self.insertPlainText(f'{indent}- [ ] ')
                else:
                    self.insertPlainText(indent+token)
                return
            qm=re.match(r'^(\s*>\s?)(.*)$',line)
            if qm:
                prefix,body=qm.groups()
                if not body.strip():
                    c.select(QTextCursor.SelectionType.BlockUnderCursor); c.removeSelectedText(); super().keyPressEvent(e); return
                super().keyPressEvent(e); self.insertPlainText(prefix); return
        super().keyPressEvent(e)

    def contextMenuEvent(self,e):
        menu=self.createStandardContextMenu()
        if self.owner.spell_dict:
            c=self.cursorForPosition(e.pos()); c.select(QTextCursor.SelectionType.WordUnderCursor); word=c.selectedText()
            if word and word.isalpha() and not self.owner.spell_dict.check(word):
                suggestions=self.owner.spell_dict.suggest(word)[:5]
                first=menu.actions()[0] if menu.actions() else None
                menu.insertSeparator(first)
                ignore=QAction(f'Ignore “{word}” for this session',menu)
                ignore.triggered.connect(lambda checked=False,w=word: self.owner.ignore_word(w))
                menu.insertAction(first,ignore)
                add=QAction(f'Add “{word}” to dictionary',menu)
                add.triggered.connect(lambda checked=False,w=word: self.owner.add_dictionary_word(w))
                menu.insertAction(first,add)
                for suggestion in reversed(suggestions):
                    a=QAction(suggestion,menu)
                    a.triggered.connect(lambda checked=False,x=suggestion,cur=QTextCursor(c): self.owner.replace_cursor_word(cur,x))
                    menu.insertAction(first,a)
        menu.exec(e.globalPos())



class NeutralMessageBox(QMessageBox):
    """NIRUNOTE message box: deliberately iconless and monochrome."""
    def __init__(self,parent=None,title='',text=''):
        super().__init__(parent)
        self.setWindowTitle(title)
        self.setProperty('nirunoteDialog',True)
        self.setText(text)
        self.setIcon(QMessageBox.Icon.NoIcon)

def neutral_notice(parent,title,text,button='OK'):
    box=NeutralMessageBox(parent,title,text)
    box.addButton(button,QMessageBox.ButtonRole.AcceptRole)
    box.exec()

def neutral_unsaved(parent):
    box=NeutralMessageBox(parent,'Unsaved changes','Save changes before continuing?')
    discard=box.addButton("Don't Save",QMessageBox.ButtonRole.DestructiveRole)
    cancel=box.addButton('Cancel',QMessageBox.ButtonRole.RejectRole)
    save=box.addButton('Save',QMessageBox.ButtonRole.AcceptRole)
    for b in (discard,cancel,save):
        b.setAutoDefault(False); b.setDefault(False)
    box.exec()
    clicked=box.clickedButton()
    if clicked is save: return 'save'
    if clicked is discard: return 'discard'
    return 'cancel'

class Main(QMainWindow):
    def __init__(self,skip_session=False):
        super().__init__(); self.settings=QSettings('NIRU','NIRUNOTE'); self.path=None; self.previewing=False; self.focused=False; self._loading=False; self._external_mtime=None; self._ignore_watch=False
        self.spell_enabled=self.settings.value('spell_enabled',True,type=bool); self.spell_language=self.settings.value('spell_language','sv_SE'); self.spell_dict=None; self.spell_deferred=False; self.init_spell();
        self.theme=self.settings.value('theme','dark'); self.default_format=self.settings.value('format','md'); self.font_size=int(self.settings.value('font_size',17)); self.autosave=self.settings.value('autosave',True,type=bool); self.restore=self.settings.value('restore',True,type=bool); self.typewriter=self.settings.value('typewriter',False,type=bool); self.word_wrap=self.settings.value('word_wrap',True,type=bool); self.highlight_line=self.settings.value('highlight_line',True,type=bool); self.writing_width=self.settings.value('writing_width','Normal'); self.recent=self.settings.value('recent',[],type=list) or []; self.save_folder_mode=self.settings.value('save_folder_mode','last'); self.default_save_folder=self.settings.value('default_save_folder',str(Path.home()/'Documents')); self.last_save_folder=self.settings.value('last_save_folder',str(Path.home()/'Documents')); self.last_open_folder=self.settings.value('last_open_folder',str(Path.home())); self.restore_last_document=self.settings.value('restore_last_document',False,type=bool); self.show_reading_time=self.settings.value('show_reading_time',False,type=bool); self.last_document=self.settings.value('last_document',''); self.line_ending='\n'; self.had_bom=False; self._external_prompt=False
        self.setWindowTitle(APP); self.setMinimumSize(560,400); self.setAcceptDrops(True); self.watcher=QFileSystemWatcher(self); self.watcher.fileChanged.connect(self.external_file_changed)
        geo=self.settings.value('geometry'); self.resize(1000,760)
        if geo: self.restoreGeometry(geo)
        root=QWidget(); self.v=QVBoxLayout(root); self.v.setContentsMargins(0,0,0,0); self.v.setSpacing(0)
        self.top=QWidget(); h=QHBoxLayout(self.top); h.setContentsMargins(12,7,12,7)
        self.menu_btn=QToolButton(); self.menu_btn.setText('☰'); self.menu_btn.setPopupMode(QToolButton.ToolButtonPopupMode.InstantPopup); self.menu_btn.setProperty('headerButton',True); self.menu=QMenu(self); self.menu_btn.setMenu(self.menu)
        self.title=QLabel('Untitled.md'); self.title.setAlignment(Qt.AlignmentFlag.AlignCenter); self.title.setToolTip('Click to rename / save'); self.title.mousePressEvent=lambda e:self.rename()
        self.copy_btn=QToolButton(); self.copy_btn.setText('⧉'); self.copy_btn.setToolTip('Copy all'); self.copy_btn.clicked.connect(self.copy_feedback)
        self.preview_btn=QToolButton(); self.preview_btn.setText('◉'); self.preview_btn.setToolTip('Edit / Preview'); self.preview_btn.clicked.connect(self.toggle_preview)
        self.focus_btn=QToolButton(); self.focus_btn.setText('⛶'); self.focus_btn.setToolTip('Focus mode · F11'); self.focus_btn.clicked.connect(self.focus)
        self.save_btn=QToolButton(); self.save_btn.setText('⌑'); self.save_btn.setToolTip('Save · Ctrl+S'); self.save_btn.clicked.connect(self.save_feedback)
        self.dirty=QLabel(''); self.dirty.setFixedWidth(16)
        h.addWidget(self.menu_btn); h.addStretch(); h.addWidget(self.title); h.addStretch(); h.addWidget(self.copy_btn); h.addWidget(self.preview_btn); h.addWidget(self.focus_btn); h.addWidget(self.save_btn); h.addWidget(self.dirty); self.v.addWidget(self.top)
        self.stack=QStackedWidget()
        self.editor_host=QWidget(); eh=QHBoxLayout(self.editor_host); eh.setContentsMargins(24,20,24,0); eh.setSpacing(0); eh.addStretch()
        self.editor=NoteEditor(self); self.editor.setFrameStyle(0); self.editor.setTabStopDistance(32); self.editor.setAcceptDrops(False); eh.addWidget(self.editor,1); eh.addStretch()
        self.preview_host=QWidget(); ph=QHBoxLayout(self.preview_host); ph.setContentsMargins(24,20,24,0); ph.setSpacing(0); ph.addStretch(); self.preview=QTextBrowser(); self.preview.setOpenExternalLinks(True); self.preview.setFrameStyle(0); ph.addWidget(self.preview,1); ph.addStretch()
        self.stack.addWidget(self.editor_host); self.stack.addWidget(self.preview_host); self.v.addWidget(self.stack,1)
        self.status=QWidget(); sh=QHBoxLayout(self.status); sh.setContentsMargins(14,5,14,7); self.stat_left=QLabel('Markdown · 0 words · 0 chars'); self.stat_right=QLabel('Ln 1, Col 1'); sh.addWidget(self.stat_left); sh.addStretch(); sh.addWidget(self.stat_right); self.v.addWidget(self.status)
        self.findbar=QWidget(); fb=QHBoxLayout(self.findbar); fb.setContentsMargins(14,5,14,7); self.find_input=QLineEdit(); self.find_input.setPlaceholderText('Find'); self.replace_input=QLineEdit(); self.replace_input.setPlaceholderText('Replace'); self.case_box=QCheckBox('Aa'); self.word_box=QCheckBox('Word'); prev=QPushButton('↑'); nxt=QPushButton('↓'); rep=QPushButton('Replace'); allb=QPushButton('All'); closeb=QPushButton('×'); fb.addWidget(self.find_input,2); fb.addWidget(self.replace_input,2); fb.addWidget(self.case_box); fb.addWidget(self.word_box); fb.addWidget(prev); fb.addWidget(nxt); fb.addWidget(rep); fb.addWidget(allb); fb.addWidget(closeb); self.findbar.hide(); self.v.insertWidget(self.v.indexOf(self.status),self.findbar); prev.clicked.connect(lambda:self.find_next(True)); nxt.clicked.connect(lambda:self.find_next(False)); rep.clicked.connect(self.replace_one); allb.clicked.connect(self.replace_all); closeb.clicked.connect(self.hide_find); self.find_input.returnPressed.connect(lambda:self.find_next(False)); self.setCentralWidget(root); self.high=MarkdownHighlighter(self.editor.document(),self); self.spell_timer=QTimer(self); self.spell_timer.setSingleShot(True); self.spell_timer.setInterval(350); self.spell_timer.timeout.connect(self.finish_spell_pass); self.stats_timer=QTimer(self); self.stats_timer.setSingleShot(True); self.stats_timer.setInterval(180); self.stats_timer.timeout.connect(self.update_document_stats); self.editor.textChanged.connect(self.changed); self.editor.cursorPositionChanged.connect(self.cursor_changed)
        self.esc_shortcut=QShortcut(QKeySequence('Escape'),self); self.esc_shortcut.activated.connect(lambda:self.set_focus(False) if self.focused else None)
        self.build_menu(); self.apply_theme(); self.apply_editor_options(); recovered=self.load_recovery();
        if not recovered and not skip_session and self.restore_last_document and self.last_document and Path(self.last_document).is_file(): self.load(self.last_document, quiet=True)
        self.timer=QTimer(self); self.timer.timeout.connect(self.recover); self.timer.start(2500); QTimer.singleShot(0,self.editor.setFocus)
    def paste_plain(self):
        self.editor.insertPlainText(QApplication.clipboard().text())

    def duplicate_line(self):
        c=self.editor.textCursor(); c.beginEditBlock()
        c.movePosition(QTextCursor.MoveOperation.StartOfBlock)
        c.movePosition(QTextCursor.MoveOperation.EndOfBlock,QTextCursor.MoveMode.KeepAnchor)
        line=c.selectedText()
        c.clearSelection(); c.movePosition(QTextCursor.MoveOperation.EndOfBlock); c.insertText('\n'+line)
        c.endEditBlock()


    def init_spell(self):
        self.spell_dict=None
        if not self.spell_enabled: return
        root=Path.home()/'.local/share/nirunote/dictionaries'
        names=(['sv_SE','en_US'] if self.spell_language=='auto' else [self.spell_language])
        for name in names:
            try:
                d=HunspellDictionary(root/name, root/'personal.txt')
                probe='Hej' if name=='sv_SE' else 'Hello'
                if d.check(probe) and not d.check('asdasdasd'):
                    self.spell_dict=d
                    d.results.finished.connect(self.spell_finished)
                    return
            except Exception: continue

    def replace_cursor_word(self,cursor,text):
        cursor.insertText(text)

    def add_dictionary_word(self,word):
        if self.spell_dict:
            self.spell_dict.add_word(word); self.high.rehighlight(); self.changed()

    def ignore_word(self,word):
        if self.spell_dict:
            self.spell_dict.personal_words.add(word.casefold()); self.spell_dict.cache.pop(word.casefold(),None)
            self.high.rehighlight()

    def copy_feedback(self):
        self.copy_all(); old=self.copy_btn.text(); self.copy_btn.setText('✓')
        QTimer.singleShot(900,lambda:self.copy_btn.setText(old))

    def save_feedback(self):
        if self.save():
            old=self.save_btn.text(); self.save_btn.setText('✓'); self.stat_right.setText('Saved')
            QTimer.singleShot(1000,lambda:self.save_btn.setText(old))
            QTimer.singleShot(1500,self.cursor_changed)

    def set_focus(self,on):
        on=bool(on)
        if on==self.focused: return
        self.focused=on; self.top.setVisible(not on); self.status.setVisible(not on)
        policy=Qt.ScrollBarPolicy.ScrollBarAlwaysOff if on else Qt.ScrollBarPolicy.ScrollBarAsNeeded
        self.editor.setVerticalScrollBarPolicy(policy); self.preview.setVerticalScrollBarPolicy(policy)
        if on:
            self.showFullScreen(); QApplication.setOverrideCursor(Qt.CursorShape.BlankCursor)
        else:
            while QApplication.overrideCursor() is not None: QApplication.restoreOverrideCursor()
            self.showNormal()
        (self.preview if self.previewing else self.editor).setFocus()

    def focus(self): self.set_focus(not self.focused)

    def insert_datetime(self):
        self.editor.insertPlainText(time.strftime('%Y-%m-%d %H:%M'))

    def goto_heading(self):
        heads=[]
        for i,line in enumerate(self.editor.toPlainText().splitlines()):
            m=re.match(r'^(#{1,6})\s+(.+)$',line)
            if m: heads.append((i,len(m.group(1)),m.group(2).strip()))
        if not heads:
            self.stat_right.setText('No headings'); QTimer.singleShot(1500,self.cursor_changed); return
        labels=[('  '*(lvl-1))+title for _,lvl,title in heads]
        choice,ok=QInputDialog.getItem(self,'Go to heading','Heading:',labels,0,False)
        if ok:
            line=heads[labels.index(choice)][0]; c=self.editor.textCursor(); c.movePosition(QTextCursor.MoveOperation.Start)
            c.movePosition(QTextCursor.MoveOperation.Down,QTextCursor.MoveMode.MoveAnchor,line)
            self.editor.setTextCursor(c); self.editor.centerCursor(); self.editor.setFocus()

    def suggested_filename(self):
        text=self.editor.toPlainText()
        m=re.search(r'^#{1,6}\s+(.+?)\s*$',text,re.M)
        base=m.group(1) if m else ' '.join(text.strip().split()[:6])
        base=re.sub(r'[\\\\/:*?"<>|]+','',base).strip().rstrip('.')
        return (base[:80] or 'Untitled') + ('.md' if self.default_format=='md' else '.txt')

    def atomic_write(self,path,text):
        p=Path(path); target=p.resolve() if p.is_symlink() else p; target.parent.mkdir(parents=True,exist_ok=True)
        if target.exists() and not os.access(target,os.W_OK): raise PermissionError(f'{p.name} is read-only')
        payload=text.replace('\r\n','\n').replace('\r','\n')
        state_json=(p.suffix.lower()=='.json' and 'nirunote' in str(p.parent))
        if self.line_ending=='\r\n' and not state_json: payload=payload.replace('\n','\r\n')
        data=payload.encode('utf-8'); data=(b'\xef\xbb\xbf'+data) if self.had_bom and not state_json else data
        fd,tmp=tempfile.mkstemp(prefix='.'+target.name+'.',suffix='.tmp',dir=str(target.parent))
        try:
            with os.fdopen(fd,'wb') as f: f.write(data); f.flush(); os.fsync(f.fileno())
            os.replace(tmp,target)
            try:
                dfd=os.open(str(target.parent),os.O_DIRECTORY); os.fsync(dfd); os.close(dfd)
            except OSError: pass
        finally:
            if os.path.exists(tmp): os.unlink(tmp)

    def snapshot(self):
        if not self.editor.toPlainText().strip(): return
        d=Path(os.environ.get('XDG_STATE_HOME',Path.home()/'.local/state'))/'nirunote/snapshots'; d.mkdir(parents=True,exist_ok=True)
        stamp=time.strftime('%Y%m%d-%H%M%S'); base=(Path(self.path).stem if self.path else 'untitled')[:40]; q=d/f'{base}-{stamp}.md'
        try: q.write_text(self.editor.toPlainText(),encoding='utf-8')
        except OSError: return
        files=sorted(d.glob('*.md'),key=lambda x:x.stat().st_mtime,reverse=True)
        for old in files[5:]:
            try: old.unlink()
            except OSError: pass

    def watch_current(self):
        for f in self.watcher.files(): self.watcher.removePath(f)
        if self.path and Path(self.path).exists():
            self.watcher.addPath(self.path)
            try:self._external_mtime=Path(self.path).stat().st_mtime_ns
            except OSError:self._external_mtime=None

    def external_file_changed(self,path):
        if self._ignore_watch or self._external_prompt or not self.path or path!=self.path: return
        p=Path(path)
        if not p.exists():
            neutral_notice(self,'File removed on disk','This file was removed or moved outside NIRUNOTE. Your open text is retained. Use Save As to keep it.')
            return
        try: mt=p.stat().st_mtime_ns
        except OSError:return
        if self._external_mtime==mt:return
        if not self.dirty.text(): self.load(self.path); return
        self._external_prompt=True
        box=NeutralMessageBox(self,'File changed on disk','This file changed outside NIRUNOTE.'); reloadb=box.addButton('Reload',QMessageBox.ButtonRole.ActionRole); keepb=box.addButton('Keep mine',QMessageBox.ButtonRole.AcceptRole); copyb=box.addButton('Save copy…',QMessageBox.ButtonRole.ActionRole); box.addButton('Cancel',QMessageBox.ButtonRole.RejectRole); box.exec(); clicked=box.clickedButton(); self._external_prompt=False
        if clicked==reloadb: self.load(self.path)
        elif clicked==copyb: self.save_as()
        else: self._external_mtime=mt; self.watch_current()

    def act(self,name,shortcut,fn,menu=None):
        a=QAction(name,self);
        if shortcut: a.setShortcut(QKeySequence(shortcut)); a.setShortcutContext(Qt.ShortcutContext.ApplicationShortcut)
        a.triggered.connect(fn); self.addAction(a); (menu or self.menu).addAction(a); return a
    def build_menu(self):
        self.menu.clear()
        filem=self.menu.addMenu('File'); editm=self.menu.addMenu('Edit'); viewm=self.menu.addMenu('View'); helpm=self.menu.addMenu('Help')
        self.act('New','Ctrl+N',self.new,filem); self.act('New from Clipboard','Ctrl+Shift+N',self.new_from_clipboard,filem); self.act('Open…','Ctrl+O',self.open,filem)
        self.recent_menu=filem.addMenu('Open Recent'); self.rebuild_recent()
        filem.addSeparator(); self.act('Save','Ctrl+S',self.save,filem); self.act('Save As…','Ctrl+Shift+S',self.save_as,filem)
        self.rename_action=self.act('Rename…',None,self.rename,filem); self.folder_action=self.act('Open containing folder',None,self.open_containing_folder,filem); self.path_action=self.act('Copy file path',None,self.copy_file_path,filem)
        filem.addSeparator(); self.act('Quit','Ctrl+Q',self.close,filem)
        self.act('Undo','Ctrl+Z',self.editor.undo,editm); self.act('Redo','Ctrl+Shift+Z',self.editor.redo,editm); editm.addSeparator(); self.act('Select all','Ctrl+A',self.editor.selectAll,editm); self.act('Copy all','Ctrl+Shift+C',self.copy_all,editm); self.act('Paste as plain text','Ctrl+Shift+V',self.paste_plain,editm); self.act('Duplicate line','Ctrl+Shift+D',self.duplicate_line,editm); editm.addSeparator()
        self.act('Find…','Ctrl+F',self.find,editm); self.act('Replace…','Ctrl+H',self.replace,editm); editm.addSeparator(); self.act('Bold','Ctrl+B',lambda:self.wrap_markdown('**','**'),editm); self.act('Italic','Ctrl+I',lambda:self.wrap_markdown('*','*'),editm); self.act('Link','Ctrl+K',self.insert_link,editm); self.act('Checkbox','Ctrl+Shift+X',self.toggle_checkbox,editm); self.act('Bullet list','Ctrl+Shift+8',lambda:self.prefix_line('- '),editm); self.act('Numbered list','Ctrl+Shift+7',lambda:self.prefix_line('1. '),editm); self.act('Quote','Ctrl+Shift+.',lambda:self.prefix_line('> '),editm); editm.addSeparator(); self.act('Go to heading…','Ctrl+G',self.goto_heading,editm); self.act('Insert date / time',None,self.insert_datetime,editm)
        self.act('Quick Open…','Ctrl+P',self.quick_open,filem); self.act('Command Palette…','Ctrl+Shift+P',self.command_palette,editm); self.act('Edit / Preview','Ctrl+Shift+E',self.toggle_preview,viewm); self.act('Focus mode','F11',self.focus,viewm); self.act('Toggle floating','Ctrl+Shift+F',self.toggle_float,viewm); viewm.addSeparator()
        self.wrap_action=self.act('Word wrap',None,self.toggle_wrap,viewm); self.wrap_action.setCheckable(True); self.wrap_action.setChecked(self.word_wrap)
        self.line_action=self.act('Highlight current line',None,self.toggle_line,viewm); self.line_action.setCheckable(True); self.line_action.setChecked(self.highlight_line)
        width=viewm.addMenu('Writing width')
        for n in WIDTHS:
            a=width.addAction(n); a.setCheckable(True); a.setChecked(n==self.writing_width); a.triggered.connect(lambda checked=False,x=n:self.set_width(x))
        tm=viewm.addMenu('Theme')
        for n in ['Dark','Light','Satie']:
            a=tm.addAction(n); a.triggered.connect(lambda checked=False,x=n:self.set_theme(x.lower()))
        viewm.addSeparator(); self.act('Text size +','Ctrl++',lambda:self.resize_font(1),viewm); self.act('Text size -','Ctrl+-',lambda:self.resize_font(-1),viewm); self.act('Reset text size','Ctrl+0',self.reset_font,viewm); self.act('Preferences…',None,self.prefs,viewm)
        self.act('About NIRUNOTE',None,self.about,helpm)
        self.update_file_actions()

    def update_file_actions(self):
        has_file=bool(self.path)
        for a in (getattr(self,'rename_action',None),getattr(self,'folder_action',None),getattr(self,'path_action',None)):
            if a: a.setEnabled(has_file)

    def rebuild_recent(self):
        self.recent_menu.clear()
        valid=[p for p in self.recent if Path(p).is_file()][:10]; self.recent=valid
        if not valid:
            a=self.recent_menu.addAction('No recent files'); a.setEnabled(False); return
        for p in valid:
            a=self.recent_menu.addAction(Path(p).name); a.setToolTip(p); a.triggered.connect(lambda checked=False,x=p:self.open_recent(x))
        self.recent_menu.addSeparator(); self.recent_menu.addAction('Clear Recent').triggered.connect(self.clear_recent)
    def add_recent(self,p):
        p=str(Path(p).resolve()); self.recent=[p]+[x for x in self.recent if x!=p]; self.recent=self.recent[:10]; self.settings.setValue('recent',self.recent); self.rebuild_recent()
    def clear_recent(self): self.recent=[]; self.settings.setValue('recent',[]); self.rebuild_recent()
    def apply_theme(self):
        t=THEMES.get(self.theme,THEMES['dark']); self.theme=self.theme if self.theme in THEMES else 'dark'
        self.setStyleSheet(f'''QMainWindow,QWidget,QPlainTextEdit,QTextBrowser,QDialog{{background:{t['bg']};color:{t['fg']};}} QPlainTextEdit,QTextBrowser{{border:none;padding:30px 18px;selection-background-color:{t['select']};}} QToolButton{{background:transparent;color:{t['muted']};border:none;border-radius:5px;padding:5px 8px;}} QToolButton:hover{{background:{t['panel']};color:{t['fg']};}} QMenu{{background:{t['panel']};color:{t['fg']};border:1px solid {t['border']};padding:6px;}} QMenu::item{{padding:7px 28px 7px 12px;border-radius:4px;}} QMenu::item:selected{{background:{t['select']};}} QMenu::separator{{height:1px;background:{t['border']};margin:5px 8px;}} QScrollBar:vertical{{background:transparent;width:4px;margin:0;}} QScrollBar::handle:vertical{{background:{t['border']};min-height:28px;border-radius:2px;}} QScrollBar::add-line:vertical,QScrollBar::sub-line:vertical{{height:0;}} QScrollBar::add-page:vertical,QScrollBar::sub-page:vertical{{background:transparent;}} QLabel{{color:{t['muted']};}}
QLineEdit,QComboBox,QSpinBox{{background:{t['panel']};color:{t['fg']};border:1px solid {t['border']};border-radius:4px;padding:6px;selection-background-color:{t['select']};selection-color:{t['fg']};}}
QLineEdit:focus,QComboBox:focus,QSpinBox:focus{{border:1px solid {t['muted']};}}
QPushButton{{background:{t['panel']};color:{t['fg']};border:1px solid {t['border']};border-radius:4px;padding:6px 12px;}}
QPushButton:hover{{background:{t['select']};border-color:{t['muted']};}}
QPushButton:focus{{background:{t['select']};color:{t['fg']};border:2px solid {t['fg']};padding:5px 11px;}}
QPushButton:pressed{{background:{t['select']};color:{t['fg']};border-color:{t['muted']};}}
QPushButton:default{{background:{t['panel']};color:{t['fg']};border:1px solid {t['muted']};}}
QPushButton:disabled{{color:{t['muted']};border-color:{t['border']};}}
QCheckBox{{color:{t['fg']};spacing:7px;}}
QCheckBox::indicator{{width:13px;height:13px;background:{t['bg']};border:1px solid {t['muted']};border-radius:2px;}}
QCheckBox::indicator:checked{{background:{t['fg']};border-color:{t['fg']};}}
QCheckBox::indicator:disabled{{background:{t['panel']};border-color:{t['border']};}}
QListWidget{{background:{t['bg']};color:{t['fg']};border:1px solid {t['border']};selection-background-color:{t['select']};selection-color:{t['fg']};}}
QMessageBox{{background:{t['bg']};}}
QMessageBox QLabel{{color:{t['fg']};font-size:13px;}}
QMessageBox[nirunoteDialog="true"]{{background:{t['bg']};}}
QMessageBox[nirunoteDialog="true"] QLabel{{color:{t['fg']};padding:4px 8px 8px 8px;}}
QMessageBox[nirunoteDialog="true"] QPushButton{{min-width:88px;min-height:30px;background:{t['panel']};color:{t['fg']};border:1px solid {t['border']};border-radius:4px;padding:0px 12px;}}
QMessageBox[nirunoteDialog="true"] QPushButton:hover{{background:{t['select']};border-color:{t['muted']};}}
QMessageBox[nirunoteDialog="true"] QPushButton:focus{{background:{t['select']};color:{t['fg']};border:2px solid {t['fg']};font-weight:600;}}
QPushButton:focus{{background:{t['select']};color:{t['fg']};border:2px solid {t['fg']};padding:5px 11px;}}
QMessageBox[nirunoteDialog="true"] QPushButton:pressed{{background:{t['select']};border-color:{t['fg']};}}
QDialogButtonBox QPushButton{{min-width:72px;}}
QToolButton[headerButton="true"]{{min-width:28px;max-width:28px;min-height:28px;max-height:28px;padding:0px;margin:0px;border:0px;background:transparent;color:{t['muted']};font-size:13px;}}
QToolButton[headerButton="true"]:hover{{background:{t['select']};color:{t['fg']};border-radius:4px;}}
QToolButton[headerButton="true"]:pressed{{background:{t['panel']};color:{t['fg']};}}
QToolButton::menu-indicator{{image:none;width:0px;height:0px;}}
''')
        f=QFont('monospace'); f.setStyleHint(QFont.StyleHint.Monospace); f.setPointSize(self.font_size); self.editor.setFont(f); self.preview.setFont(QFont('sans-serif',self.font_size)); self.high.rehighlight(); self.update_preview(); self.update_line_highlight()
    def apply_editor_options(self):
        self.editor.setLineWrapMode(QPlainTextEdit.LineWrapMode.WidgetWidth if self.word_wrap else QPlainTextEdit.LineWrapMode.NoWrap); self.apply_width(); self.update_line_highlight()
    def apply_width(self):
        width=WIDTHS.get(self.writing_width,900); self.editor.setMaximumWidth(width); self.preview.setMaximumWidth(width)
    def set_width(self,x): self.writing_width=x; self.save_settings(); self.apply_width()
    def set_theme(self,x): self.theme=x; self.save_settings(); self.apply_theme()
    def resize_font(self,d): self.font_size=max(10,min(36,self.font_size+d)); self.save_settings(); self.apply_theme()
    def reset_font(self): self.font_size=17; self.save_settings(); self.apply_theme()
    def toggle_wrap(self): self.word_wrap=not self.word_wrap; self.save_settings(); self.apply_editor_options(); self.wrap_action.setChecked(self.word_wrap)
    def toggle_line(self): self.highlight_line=not self.highlight_line; self.save_settings(); self.update_line_highlight(); self.line_action.setChecked(self.highlight_line)
    def save_settings(self):
        for k,v in [('theme',self.theme),('format',self.default_format),('font_size',self.font_size),('autosave',self.autosave),('restore',self.restore),('typewriter',self.typewriter),('word_wrap',self.word_wrap),('highlight_line',self.highlight_line),('writing_width',self.writing_width),('spell_enabled',self.spell_enabled),('spell_language',self.spell_language),('save_folder_mode',self.save_folder_mode),('default_save_folder',self.default_save_folder),('last_save_folder',self.last_save_folder),('last_open_folder',self.last_open_folder),('restore_last_document',self.restore_last_document),('show_reading_time',self.show_reading_time),('last_document',self.last_document)]: self.settings.setValue(k,v)
    def changed(self):
        # Keep the typing/open path cheap. Full-document work is delayed and
        # preview rendering only happens while Preview is actually visible.
        self.spell_deferred=True
        if not self._loading: self.dirty.setText('●')
        self.stats_timer.start()
        if self.previewing: self.update_preview()
        # Loading setPlainText emits textChanged too. Do not schedule a costly
        # spelling pass for that transient load event; load() leaves the
        # document ready and spelling will run after the first real edit.
        if not self._loading:
            self.spell_timer.start()

    def update_document_stats(self):
        txt=self.editor.toPlainText()
        chars=len(txt)
        spell=''
        if self.spell_enabled:
            if chars > LARGE_DOCUMENT_CHARS: spell=' · SPELL PAUSED (LARGE DOC)'
            elif self.spell_dict: spell=' · '+('SV' if Path(self.spell_dict.base).name=='sv_SE' else 'EN')
            else: spell=' · SPELL?'
        words=len(txt.split())
        read=(f' · {max(1,(words+219)//220)} min' if self.show_reading_time and words else '')
        ro=(' · READ ONLY' if self.path and Path(self.path).exists() and not os.access(self.path,os.W_OK) else '')
        self.stat_left.setText(('Markdown' if self.current_ext() in ('md','markdown') else 'Text')+f' · {words} words · {chars} chars'+read+spell+ro)

    def finish_spell_pass(self):
        # Keep large documents syntax-only. For normal documents, resolve all
        # uncached spelling in one Hunspell process, then repaint from cache.
        if self.editor.document().characterCount() <= LARGE_DOCUMENT_CHARS and self.spell_dict and self.spell_enabled:
            txt=self.editor.toPlainText()
            words=set(re.findall(r"(?u)\b[^\W\d_][^\W\d_'-]*\b",txt))
            if not self.spell_dict.inflight:
                dictionary=self.spell_dict
                dictionary.inflight=True
                def spell_worker():
                    try: dictionary.check_many(words)
                    finally: dictionary.results.finished.emit(dictionary)
                threading.Thread(target=spell_worker,daemon=True,name='nirunote-spell').start()
        self.spell_deferred=False
        if self.editor.document().characterCount() <= LARGE_DOCUMENT_CHARS:
            self.high.rehighlight()

    def spell_finished(self, dictionary):
        dictionary.inflight=False
        if dictionary is self.spell_dict and self.spell_enabled and self.editor.document().characterCount() <= LARGE_DOCUMENT_CHARS:
            self.high.rehighlight()

    def cursor_changed(self):
        if self.previewing:
            self.stat_right.setText('Preview'); self.update_line_highlight(); return
        c=self.editor.textCursor(); sel=c.selectedText().replace('\u2029',' ').strip()
        suffix=f' · Selected {len(sel.split())} words' if sel else ''
        self.stat_right.setText(f'Ln {c.blockNumber()+1}, Col {c.positionInBlock()+1}{suffix}'); self.update_line_highlight()
        if self.typewriter: self.editor.centerCursor()
    def update_line_highlight(self):
        if not self.highlight_line or self.previewing: self.editor.setExtraSelections([]); return
        sel=QTextEdit.ExtraSelection(); sel.format.setBackground(QColor(THEMES[self.theme]['line'])); sel.format.setProperty(QTextFormat.Property.FullWidthSelection,True); sel.cursor=self.editor.textCursor(); sel.cursor.clearSelection(); self.editor.setExtraSelections([sel])
    def current_ext(self): return (Path(self.path).suffix.lower().lstrip('.') if self.path else self.default_format)
    def maybe_save(self):
        if not self.dirty.text(): return True
        r=neutral_unsaved(self)
        if r=='save': return self.save()
        return r=='discard'
    def reset_doc(self,text='',name=None):
        self._loading=True; self.path=None; self.editor.setPlainText(text); self._loading=False; self.title.setText(name or 'Untitled.'+self.default_format); self.dirty.setText('●' if text else ''); self.editor.setFocus(); self.cursor_changed(); self.update_file_actions()
    def new(self):
        if self.maybe_save(): self.clear_recovery(); self.reset_doc()
    def new_from_clipboard(self):
        if not self.maybe_save(): return
        self.clear_recovery(); self.reset_doc(QApplication.clipboard().text(),'Clipboard.'+self.default_format)
    def open(self):
        if not self.maybe_save(): return
        p,_=QFileDialog.getOpenFileName(self,'Open text',self.last_open_folder,'Markdown (*.md *.markdown);;Text (*.txt);;All files (*)')
        if p: self.last_open_folder=str(Path(p).parent); self.save_settings(); self.load(p)
    def open_recent(self,p):
        if self.maybe_save(): self.load(p)
    def load(self,p,quiet=False):
        try:
            path=Path(p); raw=path.read_bytes(); self.had_bom=raw.startswith(b'\xef\xbb\xbf'); text=raw.decode('utf-8-sig'); self.line_ending='\r\n' if b'\r\n' in raw else '\n'
            self._loading=True; self.editor.setPlainText(text); self._loading=False; self.path=str(path); self.title.setText(path.name); self.dirty.setText(''); self.clear_recovery(); self.add_recent(path); self.watch_current(); self.last_document=self.path; self.save_settings(); self.update_document_stats()
            pos=int(self.settings.value('cursor/'+str(path.resolve()),0)); c=self.editor.textCursor(); c.setPosition(min(pos,len(text))); self.editor.setTextCursor(c); self.cursor_changed(); self.editor.setFocus(); self.update_file_actions()
        except Exception as e:
            self._loading=False
            if not quiet: neutral_notice(self,'Open failed',str(e))
    def save(self):
        if not self.path: return self.save_as()
        try:
            self.snapshot(); self._ignore_watch=True; self.atomic_write(self.path,self.editor.toPlainText()); self._ignore_watch=False
            self.dirty.setText(''); self.clear_recovery(); self.add_recent(self.path); self.last_document=self.path; self.last_save_folder=str(Path(self.path).parent); self.save_settings(); self.watch_current(); self.update_file_actions(); return True
        except Exception as e: neutral_notice(self,'Save failed',str(e)); return False
    def save_as(self):
        ext='.md' if self.default_format=='md' else '.txt'; base=Path(self.default_save_folder).expanduser() if self.save_folder_mode=='custom' else Path(self.last_save_folder).expanduser(); base=base if base.is_dir() else (Path.home()/'Documents' if (Path.home()/'Documents').is_dir() else Path.home()); p,_=QFileDialog.getSaveFileName(self,'Save as',str(base/self.suggested_filename()),'Markdown (*.md *.markdown);;Text (*.txt)')
        if not p:return False
        if not Path(p).suffix: p+=ext
        old_path=self.path
        old_title=self.title.text()
        self.path=p
        if self.save():
            self.title.setText(Path(p).name)
            return True
        self.path=old_path
        self.title.setText(old_title)
        return False
    def rename(self):
        if not self.path: return self.save_as()
        n,ok=QInputDialog.getText(self,'Rename','New filename:',text=Path(self.path).name)
        if ok and n:
            try:
                q=Path(self.path).with_name(n)
                if q.exists() and q != Path(self.path): raise FileExistsError(f'{q.name} already exists')
                Path(self.path).rename(q); self.path=str(q); self.title.setText(q.name); self.add_recent(q); self.watch_current()
            except Exception as e: neutral_notice(self,'Rename failed',str(e))
    def open_containing_folder(self):
        if self.path: QDesktopServices.openUrl(QUrl.fromLocalFile(str(Path(self.path).parent)))

    def copy_file_path(self):
        if self.path: QApplication.clipboard().setText(str(Path(self.path)))

    def copy_all(self): c=self.editor.textCursor(); c.select(QTextCursor.SelectionType.Document); QApplication.clipboard().setText(c.selectedText().replace('\u2029','\n'))
    def show_find(self,replace=False):
        self.findbar.show(); self.replace_input.setVisible(replace); self.find_input.setFocus(); self.find_input.selectAll()
    def hide_find(self): self.findbar.hide(); self.editor.setFocus()
    def find(self): self.show_find(False)
    def replace(self): self.show_find(True)
    def find_next(self,backward=False):
        q=self.find_input.text()
        if not q:return
        flags=QTextDocument.FindFlag(0)
        if backward: flags|=QTextDocument.FindFlag.FindBackward
        if self.case_box.isChecked(): flags|=QTextDocument.FindFlag.FindCaseSensitively
        if self.word_box.isChecked(): flags|=QTextDocument.FindFlag.FindWholeWords
        if not self.editor.find(q,flags):
            self.editor.moveCursor(QTextCursor.MoveOperation.End if backward else QTextCursor.MoveOperation.Start); self.editor.find(q,flags)
    def replace_one(self):
        c=self.editor.textCursor()
        if c.hasSelection() and c.selectedText()==self.find_input.text(): c.insertText(self.replace_input.text())
        self.find_next(False)
    def replace_all(self):
        q=self.find_input.text()
        if not q:return
        text=self.editor.toPlainText(); flags=0 if self.case_box.isChecked() else re.I; pat=re.escape(q); pat=(r'\b'+pat+r'\b') if self.word_box.isChecked() else pat; self.editor.setPlainText(re.sub(pat,lambda m:self.replace_input.text(),text,flags=flags))
    def wrap_markdown(self,left,right):
        c=self.editor.textCursor(); selected=c.selectedText().replace('\u2029','\n')
        if selected: c.insertText(left+selected+right)
        else: c.insertText(left+right); c.movePosition(QTextCursor.MoveOperation.Left,QTextCursor.MoveMode.MoveAnchor,len(right)); self.editor.setTextCursor(c)
    def insert_link(self):
        c=self.editor.textCursor(); selected=c.selectedText().replace('\u2029',' ') or 'text'; c.insertText(f'[{selected}](url)'); c.movePosition(QTextCursor.MoveOperation.Left,QTextCursor.MoveMode.MoveAnchor,4); self.editor.setTextCursor(c)
    def prefix_line(self,prefix):
        c=self.editor.textCursor(); c.movePosition(QTextCursor.MoveOperation.StartOfBlock); c.insertText(prefix)
    def toggle_checkbox(self):
        c=self.editor.textCursor(); c.select(QTextCursor.SelectionType.BlockUnderCursor); line=c.selectedText().replace('\u2029','')
        if re.match(r'^\s*[-*+] \[[ xX]\] ',line): line=re.sub(r'^(\s*)[-*+] \[[ xX]\] ',r'\1',line)
        else: line='- [ ] '+line
        c.insertText(line)
    def notes_root(self):
        p=Path(self.default_save_folder).expanduser() if self.save_folder_mode=='custom' else Path(self.last_save_folder).expanduser(); return p if p.is_dir() else Path.home()
    def quick_open(self):
        root=self.notes_root(); files=[]
        try:
            for p in root.rglob('*'):
                if p.is_file() and p.suffix.lower() in ('.md','.markdown','.txt'): files.append(p)
                if len(files)>=1500: break
        except OSError: pass
        d=QDialog(self); d.setWindowTitle('Quick Open'); d.resize(620,460); v=QVBoxLayout(d); search=QLineEdit(); search.setPlaceholderText(f'Search {root}'); lst=QListWidget(); v.addWidget(search); v.addWidget(lst,1)
        def fill(q=''):
            lst.clear(); terms=q.casefold().split(); ranked=[]
            for p in files:
                rel=str(p.relative_to(root)); low=rel.casefold()
                if all(t in low for t in terms): ranked.append((sum(low.find(t) for t in terms),rel,p))
            for _,rel,p in sorted(ranked)[:200]: item=QListWidgetItem(rel); item.setData(Qt.ItemDataRole.UserRole,str(p)); lst.addItem(item)
            if lst.count(): lst.setCurrentRow(0)
        fill(); search.textChanged.connect(fill); lst.itemActivated.connect(lambda item:(d.accept()))
        if d.exec() and lst.currentItem() and self.maybe_save(): self.load(lst.currentItem().data(Qt.ItemDataRole.UserRole))
    def command_palette(self):
        commands=[('New',self.new),('Open',self.open),('Quick Open',self.quick_open),('Save',self.save),('Save As',self.save_as),('Preview',self.toggle_preview),('Focus mode',self.focus),('Go to heading',self.goto_heading),('Insert date / time',self.insert_datetime),('Preferences',self.prefs)]
        d=QDialog(self); d.setWindowTitle('Command Palette'); d.resize(480,360); v=QVBoxLayout(d); search=QLineEdit(); search.setPlaceholderText('Type a command'); lst=QListWidget(); v.addWidget(search); v.addWidget(lst,1)
        def fill(q=''):
            lst.clear(); q=q.casefold()
            for name,_ in commands:
                if q in name.casefold(): item=QListWidgetItem(name); lst.addItem(item)
            if lst.count(): lst.setCurrentRow(0)
        fill(); search.textChanged.connect(fill); search.returnPressed.connect(d.accept); lst.itemActivated.connect(lambda item:d.accept())
        if d.exec() and lst.currentItem(): dict(commands)[lst.currentItem().text()]()
    def toggle_preview(self):
        ratio=self.editor.verticalScrollBar().value()/max(1,self.editor.verticalScrollBar().maximum()) if not self.previewing else self.preview.verticalScrollBar().value()/max(1,self.preview.verticalScrollBar().maximum())
        self.previewing=not self.previewing; self.stack.setCurrentIndex(1 if self.previewing else 0)
        self.preview_btn.setProperty('active',self.previewing); self.preview_btn.style().unpolish(self.preview_btn); self.preview_btn.style().polish(self.preview_btn)
        self.update_preview(); self.update_line_highlight(); self.cursor_changed(); target=(self.preview if self.previewing else self.editor); QTimer.singleShot(0,lambda:target.verticalScrollBar().setValue(int(target.verticalScrollBar().maximum()*ratio))); target.setFocus()
    def update_preview(self):
        if not (hasattr(self,'preview') and self.previewing):
            return
        # Build a stable preview snapshot. QTextBrowser.setHtml() is destructive
        # to its viewport/scroll state, so never call it again for identical
        # content. This is especially important for long Markdown documents.
        t=THEMES[self.theme]
        rendered=f"<style>body{{line-height:1.40;color:{t['fg']};background:{t['bg']};margin:0;}} a{{color:{t['accent']};}} small{{font-size:.78em;color:{t['muted']};}} kbd{{font-family:monospace;font-size:.86em;background:{t['panel']};border:1px solid {t['border']};border-radius:3px;padding:1px 4px;}} sub,sup{{font-size:.75em;}} mark{{background:{t['select']};color:{t['fg']};padding:0 2px;}} p{{margin:.16em 0 .68em;}} code{{background:{t['panel']};padding:2px 5px;border-radius:3px;}} pre{{background:{t['panel']};padding:12px 14px;margin:.55em 0 .85em;white-space:pre-wrap;}} pre code{{padding:0;background:transparent;}} blockquote{{color:{t['muted']};border-left:3px solid {t['border']};padding-left:12px;margin:.40em 0 .75em;}} h1{{font-size:1.55em;margin:1.05em 0 .38em;}} h2{{font-size:1.32em;margin:1em 0 .42em;}} h3{{font-size:1.16em;margin:.82em 0 .30em;}} h4,h5,h6{{margin:.72em 0 .28em;}} ul,ol{{margin:.25em 0 .72em;padding-left:1.55em;}} li{{margin:.10em 0;}} hr{{border:0;border-top:1px solid {t['border']};margin:.9em 0 1.05em;}}</style>"+markdown_to_html(self.editor.toPlainText())
        if getattr(self,'_preview_html',None)==rendered:
            return
        bar=self.preview.verticalScrollBar()
        had_preview=hasattr(self,'_preview_html')
        old_max=bar.maximum(); old_value=bar.value()
        ratio=(old_value/old_max) if had_preview and old_max > 0 else None
        self.preview.setHtml(rendered)
        self._preview_html=rendered
        # Force QTextDocument to complete layout before a possible position
        # restore. On the first render we intentionally do not touch the
        # scrollbar: Qt owns it and the user can scroll the complete document.
        self.preview.document().adjustSize()
        if ratio is not None:
            QTimer.singleShot(0,lambda b=bar,r=ratio:b.setValue(round(b.maximum()*r)))
    def toggle_float(self):
        # Qt Tool windows are normally treated as floating by both stacking and tiling WMs.
        floating=bool(self.windowFlags() & Qt.WindowType.Tool); geo=self.geometry(); self.hide(); self.setWindowFlag(Qt.WindowType.Tool,not floating); self.setWindowFlag(Qt.WindowType.Window, floating); self.show(); self.setGeometry(geo); self.raise_(); self.activateWindow()
    def prefs(self): Prefs(self).exec()
    def about(self): neutral_notice(self,'About NIRUNOTE',f'NIRUNOTE {VERSION}\n\nMinimal, suckless writing for Linux.\nMarkdown-first. Keyboard-driven. Fast.\n\nby Nicklas Rudolfsson')
    def recovery_path(self): return Path(os.environ.get('XDG_STATE_HOME',Path.home()/'.local/state'))/'nirunote/recovery.json'
    def recover(self):
        if self.autosave and self.dirty.text():
            p=self.recovery_path(); p.parent.mkdir(parents=True,exist_ok=True); self.atomic_write(p,json.dumps({'text':self.editor.toPlainText(),'path':self.path},ensure_ascii=False))
    def clear_recovery(self):
        try:self.recovery_path().unlink(missing_ok=True)
        except OSError:pass
    def load_recovery(self):
        p=self.recovery_path()
        if self.restore and p.exists() and p.stat().st_size:
            try:
                data=json.loads(p.read_text(encoding='utf-8')); text=data.get('text','')
                if text: self.reset_doc(text,'Recovered · unsaved'); return True
            except Exception: pass
        return False
    def dragEnterEvent(self,e):
        urls=e.mimeData().urls(); e.acceptProposedAction() if urls and urls[0].isLocalFile() and Path(urls[0].toLocalFile()).suffix.lower() in ('.md','.markdown','.txt') else e.ignore()
    def dropEvent(self,e):
        p=e.mimeData().urls()[0].toLocalFile()
        if self.maybe_save(): self.load(p); e.acceptProposedAction()
    def closeEvent(self,e):
        self.settings.setValue('geometry',self.saveGeometry()); self.recover();
        if self.path: self.settings.setValue('cursor/'+str(Path(self.path).resolve()),self.editor.textCursor().position()); self.last_document=self.path; self.save_settings()
        if self.maybe_save(): e.accept()
        else:e.ignore()

def core_self_test():
    try:
        td=Path(tempfile.mkdtemp(prefix='nirunote-core-')); p=td/'atomic.md'
        def aw(path,text):
            fd,tmp=tempfile.mkstemp(prefix='.'+path.name+'.',suffix='.tmp',dir=str(path.parent))
            try:
                with os.fdopen(fd,'w',encoding='utf-8',newline='') as f:
                    f.write(text); f.flush(); os.fsync(f.fileno())
                os.replace(tmp,path)
            finally:
                if os.path.exists(tmp): os.unlink(tmp)
        aw(p,'alpha'); aw(p,'beta')
        if p.read_text(encoding='utf-8')!='beta': raise RuntimeError('atomic save mismatch')
        shutil.rmtree(td,ignore_errors=True); print('NIRUNOTE core self-test: OK'); return 0
    except Exception as e:
        print(f'NIRUNOTE core self-test: FAILED ({e})',file=sys.stderr); return 1

def spell_self_test():
    base=Path.home()/'.local/share/nirunote/dictionaries/sv_SE'
    try:
        d=HunspellDictionary(base)
        good=d.check('Hej'); bad=not d.check('asdasdasd')
        if good and bad:
            print('NIRUNOTE spell runtime self-test: OK (SV)')
            return 0
        print(f'NIRUNOTE spell runtime self-test: FAILED (Hej={good}, asdasdasd_bad={bad})',file=sys.stderr)
    except Exception as e:
        print(f'NIRUNOTE spell runtime self-test: FAILED ({e})',file=sys.stderr)
    return 1

def main():
    # Non-GUI probes are used by the installer/wrapper and must never create a window.
    if '--spell-self-test' in sys.argv:
        return spell_self_test()
    if '--core-self-test' in sys.argv:
        return core_self_test()
    if '--version' in sys.argv:
        print(f'{APP} {VERSION}')
        return 0
    if '--self-test' in sys.argv:
        # Non-interactive GUI probe. The installer supplies isolated XDG dirs.
        app=QApplication.instance() or QApplication([])
        app.setApplicationName(APP); app.setOrganizationName('NIRU')
        w=Main()
        w.apply_theme(); w.apply_editor_options(); w.update_line_highlight(); w.update_preview()
        app.processEvents()
        w.hide()
        w.deleteLater()
        app.processEvents()
        app.quit()
        print(f'{APP} {VERSION}: GUI OK', flush=True)
        return 0
    scratch='--scratchpad' in sys.argv; force_new='--new' in sys.argv; focus_start='--focus' in sys.argv
    open_arg=None
    if '--open' in sys.argv:
        try: open_arg=sys.argv[sys.argv.index('--open')+1]
        except IndexError: open_arg=None
    files=[a for i,a in enumerate(sys.argv[1:]) if not a.startswith('-') and (i==0 or sys.argv[i] != '--open')]
    explicit=open_arg or (files[0] if files else None)
    app=QApplication(sys.argv)
    app.setApplicationName(APP)
    app.setApplicationDisplayName(APP+' Scratchpad' if scratch else APP)
    app.setOrganizationName('NIRU')
    app.setDesktopFileName('nirunote-scratchpad' if scratch else 'nirunote')
    w=Main(skip_session=scratch or force_new or bool(explicit))
    if scratch:
        scratch_path=Path(os.environ.get('XDG_STATE_HOME',Path.home()/'.local/state'))/'nirunote/scratchpad.md'; scratch_path.parent.mkdir(parents=True,exist_ok=True); scratch_path.touch(exist_ok=True); w.load(scratch_path); w.setWindowTitle('NIRUNOTE Scratchpad')
    elif explicit and Path(explicit).is_file(): w.load(explicit)
    elif force_new: w.reset_doc()
    w.show()
    if focus_start: w.set_focus(True)
    w.raise_(); w.activateWindow()
    return app.exec()
if __name__=='__main__':
    raise SystemExit(main())

from __future__ import annotations

import html
import re
from urllib.parse import urlparse


def safe_link(target):
    target=(target or '').strip()
    try:
        scheme=urlparse(target).scheme.lower()
    except Exception:
        return None
    return target if scheme in ('http','https','mailto') else None

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
        def link(m):
            target=safe_link(m.group(2))
            return f'<a href="{html.escape(target,quote=True)}">{m.group(1)}</a>' if target is not None else m.group(1)
        text=re.sub(r'\[([^]]+)\]\(([^)]+)\)',link,text)
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

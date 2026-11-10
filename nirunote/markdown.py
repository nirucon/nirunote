"""Small dependency-free Markdown renderer for the NIRUNOTE Preview.

It covers CommonMark and the GitHub extensions people write in practice
(tables, task lists, strikethrough, autolinks, footnotes and alerts) and emits
the HTML subset that QTextBrowser lays out well. Colours live in classes, so
the rendered HTML does not depend on the theme and can be cached.

Raw HTML is never passed through: a short list of formatting tags is re-emitted
without attributes, <img>/<a> are rebuilt from safe attributes only, layout
wrappers (<p>, <div>, <details> ...) are dropped and everything else stays
escaped text. Links are kept only for http(s), mailto, #anchors and relative
paths; javascript:, data: and file: targets become plain text.
"""

from __future__ import annotations

import html
import re
import sys
from pathlib import Path
from urllib.parse import urlparse

try:
    from .highlight import highlight_code
except ImportError:
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    from highlight import highlight_code


def safe_link(target):
    """Return TARGET when it is a web or mail link, else None."""
    target = (target or '').strip()
    try:
        scheme = urlparse(target).scheme.lower()
    except Exception:
        return None
    return target if scheme in ('http', 'https', 'mailto') else None


def link_kind(target):
    """Classify a link target: 'web', 'anchor', 'local' or None (unsafe)."""
    target = (target or '').strip()
    if not target:
        return None
    if target.startswith('#'):
        return 'anchor'
    if safe_link(target):
        return 'web'
    if target.startswith('//') or ':' in target.split('/', 1)[0].split('#', 1)[0]:
        return None
    return 'local'


def slugify(text, seen=None):
    """GitHub-style heading anchor; SEEN (dict) numbers duplicates -1, -2, ..."""
    slug = re.sub(r'[^\w\- ]', '', text.strip().lower().replace('`', '')).replace(' ', '-')
    if seen is not None:
        if slug in seen:
            seen[slug] += 1
            slug = f'{slug}-{seen[slug]}'
        else:
            seen[slug] = 0
    return slug


ALERTS = {
    'note': ('ℹ', 'Note'),
    'tip': ('✦', 'Tip'),
    'important': ('❢', 'Important'),
    'warning': ('⚠', 'Warning'),
    'caution': ('⊘', 'Caution'),
}

FENCE_RE = re.compile(r'^( {0,3})(`{3,}|~{3,})[ \t]*([^\s`]*)[^`]*$')
ATX_RE = re.compile(r'^ {0,3}(#{1,6})(?:[ \t]+(.*?))??(?:[ \t]+#+)?[ \t]*$')
SETEXT_RE = re.compile(r'^ {0,3}(=+|-+)[ \t]*$')
HR_RE = re.compile(r'^ {0,3}([-*_])(?:[ \t]*\1){2,}[ \t]*$')
QUOTE_RE = re.compile(r'^ {0,3}> ?(.*)$')
LIST_RE = re.compile(r'^( {0,3})([-+*]|\d{1,9}[.)])(?:([ \t]+)(.*)|[ \t]*$)')
TASK_RE = re.compile(r'^\[([ xX])\][ \t]+')
DELIM_CELL_RE = re.compile(r'^\s*:?-+:?\s*$')
REFDEF_RE = re.compile(
    r'^ {0,3}\[([^\]^][^\]]*)\]:[ \t]*(<[^>]*>|\S+)'
    r'(?:[ \t]+("[^"]*"|\'[^\']*\'|\([^)]*\)))?[ \t]*$'
)
FOOTDEF_RE = re.compile(r'^ {0,3}\[\^([^\]\s]+)\]:[ \t]?(.*)$')
TOKEN_RE = re.compile('\x02(\\d+)\x03')
CODE_SPAN_RE = re.compile(r'(?<![`\\])(`+)(?!`)(.+?)(?<!`)\1(?!`)', re.S)
ESCAPE_RE = re.compile(r'\\([!-/:-@\[-`{-~])')
ANGLE_LINK_RE = re.compile(r'<((?:https?|mailto):[^\s<>]+)>|<([\w.+-]+@[\w-]+(?:\.[\w-]+)+)>')
COMMENT_RE = re.compile(r'<!--.*?-->', re.S)
TAG_RE = re.compile(r'<(/?)([A-Za-z][A-Za-z0-9]*)((?:\s+[^<>]*?)?)\s*(/?)>')
ATTR_RE = re.compile(r'([A-Za-z_:][-\w:.]*)\s*=\s*("[^"]*"|\'[^\']*\'|[^\s"\'=<>`]+)')
FOOTREF_RE = re.compile(r'\[\^([^\]\s]+)\]')
BARE_URL_RE = re.compile(
    r'(?<![\w@/\x03])((?:https?://|www\.)[^\s<>\x02\x03]*[^\s<>\x02\x03.,:;!?"\'*_~)\]])'
)
BARE_MAIL_RE = re.compile(r'(?<![\w.+/-])([\w.+-]+@[\w-]+(?:\.[\w-]+)*\.[A-Za-z]{2,})(?![\w@])')
ENTITY_RE = re.compile(r'&(?:#[0-9]{1,7}|#[xX][0-9a-fA-F]{1,6}|[A-Za-z][A-Za-z0-9]{1,31});')
EMPHASIS = (
    (re.compile(r'(?<![*\w])\*\*\*(?=\S)(.+?)(?<=\S)\*\*\*(?!\*)', re.S), r'<strong><em>\1</em></strong>'),
    (re.compile(r'(?<![\w_])___(?=\S)(.+?)(?<=\S)___(?![\w_])', re.S), r'<strong><em>\1</em></strong>'),
    (re.compile(r'(?<!\*)\*\*(?=[^\s*])(.+?)(?<=[^\s])\*\*(?!\*)', re.S), r'<strong>\1</strong>'),
    (re.compile(r'(?<![\w_])__(?=[^\s_])(.+?)(?<=\S)__(?![\w_])', re.S), r'<strong>\1</strong>'),
    (re.compile(r'(?<!\*)\*(?=[^\s*])(.+?)(?<=[^\s*])\*(?!\*)', re.S), r'<em>\1</em>'),
    (re.compile(r'(?<![\w_])_(?=[^\s_])(.+?)(?<=[^\s_])_(?![\w_])', re.S), r'<em>\1</em>'),
    (re.compile(r'(?<!~)~~(?=\S)(.+?)(?<=\S)~~(?!~)', re.S), r'<s>\1</s>'),
)
FORMAT_TAGS = {
    'b', 'i', 'em', 'strong', 's', 'del', 'strike', 'ins', 'u', 'small', 'kbd', 'sub',
    'sup', 'mark', 'code', 'tt', 'var', 'samp', 'abbr', 'cite', 'q',
}  # fmt: skip
DROPPED_TAGS = {
    'p', 'div', 'span', 'details', 'summary', 'picture', 'source', 'center', 'section',
    'article', 'nav', 'header', 'footer', 'figure', 'figcaption', 'main', 'aside', 'font',
}  # fmt: skip


def _esc(text):
    return html.escape(text, quote=False)


def _attr(text):
    return html.escape(text, quote=True)


def _plain(fragment):
    """Visible text of a rendered inline fragment."""
    return html.unescape(re.sub(r'<[^>]*>', '', fragment))


def _split_row(line):
    """Split a table row on unescaped pipes outside code spans."""
    line = line.strip()
    if line.startswith('|'):
        line = line[1:]
    if line.endswith('|') and not line.endswith('\\|'):
        line = line[:-1]
    cells, cur, i, tick = [], [], 0, 0
    while i < len(line):
        c = line[i]
        if c == '\\' and i + 1 < len(line) and line[i + 1] == '|':
            cur.append('|')
            i += 2
            continue
        if c == '`':
            run = len(line[i:]) - len(line[i:].lstrip('`'))
            tick = 0 if tick == run else (run if not tick else tick)
            cur.append('`' * run)
            i += run
            continue
        if c == '|' and not tick:
            cells.append(''.join(cur).strip())
            cur = []
        else:
            cur.append(c)
        i += 1
    cells.append(''.join(cur).strip())
    return cells


class Rendered:
    """The result of one render: HTML plus the heading list (line, level, title, anchor)."""

    __slots__ = ('html', 'headings', 'images')

    def __init__(self, html_text, headings, images=None):
        self.html = html_text
        self.headings = headings
        self.images = images or {}


class _Renderer:
    def __init__(self, text):
        self.store = []
        self.refs = {}
        self.footnotes = {}
        self.footnote_order = []
        self.headings = []
        self.images = {}
        self.slugs = {}
        self.list_depth = 0
        self.highlight = True
        self.prefix = ''
        lines = text.replace('\r\n', '\n').replace('\r', '\n').split('\n')
        self.lines = self._collect_definitions(
            [(n, line.expandtabs(4) if '\t' in line else line) for n, line in enumerate(lines)]
        )

    # Definitions -----------------------------------------------------------

    def _collect_definitions(self, lines):
        """Take link reference and footnote definitions out of the line list."""
        kept = []
        fence = None
        i = 0
        while i < len(lines):
            no, line = lines[i]
            m = FENCE_RE.match(line.lstrip('> '))
            if m and (fence is None or (m.group(2)[0] == fence[0] and len(m.group(2)) >= len(fence))):
                fence = None if fence else m.group(2)
                kept.append(lines[i])
                i += 1
                continue
            if fence is None:
                fm = FOOTDEF_RE.match(line)
                if fm:
                    body = [fm.group(2)]
                    i += 1
                    while i < len(lines):
                        nxt = lines[i][1]
                        if nxt.startswith('    ') or (nxt.strip() and body[-1].strip()
                                                     and not FOOTDEF_RE.match(nxt)
                                                     and not REFDEF_RE.match(nxt)
                                                     and not self._starts_block(nxt)):
                            body.append(nxt[4:] if nxt.startswith('    ') else nxt)
                            i += 1
                        elif not nxt.strip() and i + 1 < len(lines) and lines[i + 1][1].startswith('    '):
                            body.append('')
                            i += 1
                        else:
                            break
                    self.footnotes.setdefault(fm.group(1).lower(), body)
                    continue
                rm = REFDEF_RE.match(line)
                if rm and (not kept or not kept[-1][1].strip() or ATX_RE.match(kept[-1][1])):
                    dest = rm.group(2)
                    if dest.startswith('<') and dest.endswith('>'):
                        dest = dest[1:-1]
                    title = rm.group(3)[1:-1] if rm.group(3) else ''
                    self.refs.setdefault(self._ref_key(rm.group(1)), (dest, title))
                    i += 1
                    continue
            kept.append(lines[i])
            i += 1
        return kept

    @staticmethod
    def _ref_key(label):
        return ' '.join(label.split()).casefold()

    # Block structure -------------------------------------------------------

    @staticmethod
    def _starts_block(line):
        """True when LINE starts a block that may interrupt a paragraph."""
        if ATX_RE.match(line) or FENCE_RE.match(line) or HR_RE.match(line) or QUOTE_RE.match(line):
            return True
        m = LIST_RE.match(line)
        if m and m.group(4) and m.group(4).strip():
            marker = m.group(2)
            return not marker[0].isdigit() or int(marker[:-1]) == 1
        return line.lstrip().startswith('<!--')

    def blocks(self, lines, tight=False):
        out = []
        para = []
        i = 0
        n = len(lines)

        def flush():
            if para:
                rendered = self.paragraph(para, tight)
                if rendered:
                    out.append(rendered)
                para.clear()

        while i < n:
            no, line = lines[i]
            if not line.strip():
                flush()
                i += 1
                continue
            indent = len(line) - len(line.lstrip(' '))
            if para:
                m = SETEXT_RE.match(line)
                if m and indent < 4:
                    level = 1 if m.group(1)[0] == '=' else 2
                    text = '\n'.join(t.strip() for _, t in para)
                    para.clear()
                    out.append(self.heading(level, text, lines[i - 1][0]))
                    i += 1
                    continue
                if '|' in para[-1][1] and self._table_at(lines, i - 1):
                    para.pop()
                    flush()
                    i -= 1
                    continue
                if indent >= 4 or not self._starts_block(line):
                    para.append((no, line))
                    i += 1
                    continue
                flush()
                continue
            if indent >= 4:
                code = []
                j = i
                while j < n and (not lines[j][1].strip() or lines[j][1].startswith('    ')):
                    code.append(lines[j][1][4:])
                    j += 1
                while code and not code[-1].strip():
                    code.pop()
                out.append(self.code_block('\n'.join(code), ''))
                i = j
                continue
            m = FENCE_RE.match(line)
            if m:
                fence, lang = m.group(2), m.group(3)
                strip = len(m.group(1))
                code = []
                j = i + 1
                while j < n:
                    t = lines[j][1]
                    close = re.match(r'^ {0,3}(`{3,}|~{3,})[ \t]*$', t)
                    if close and close.group(1)[0] == fence[0] and len(close.group(1)) >= len(fence):
                        j += 1
                        break
                    lead = len(t) - len(t.lstrip(' '))
                    code.append(t[min(lead, strip):])
                    j += 1
                out.append(self.code_block('\n'.join(code), html.unescape(lang)))
                i = j
                continue
            m = ATX_RE.match(line)
            if m:
                out.append(self.heading(len(m.group(1)), m.group(2) or '', no))
                i += 1
                continue
            if HR_RE.match(line):
                out.append('<hr>')
                i += 1
                continue
            if line.lstrip().startswith('<!--'):
                j = i
                while j < n and '-->' not in lines[j][1]:
                    j += 1
                rest = lines[j][1].split('-->', 1)[1] if j < n else ''
                i = j + 1
                if rest.strip():
                    para.append((lines[j][0], rest))
                continue
            m = QUOTE_RE.match(line)
            if m:
                inner = []
                j = i
                while j < n:
                    t = lines[j][1]
                    qm = QUOTE_RE.match(t)
                    if qm:
                        inner.append((lines[j][0], qm.group(1)))
                    elif t.strip() and inner and inner[-1][1].strip() and not self._starts_block(t):
                        inner.append((lines[j][0], t))
                    else:
                        break
                    j += 1
                out.append(self.quote(inner))
                i = j
                continue
            if LIST_RE.match(line):
                rendered, i = self.list_block(lines, i)
                out.append(rendered)
                continue
            if '|' in line and self._table_at(lines, i):
                rendered, i = self.table(lines, i)
                out.append(rendered)
                continue
            para.append((no, line))
            i += 1
        flush()
        return '\n'.join(out)

    def paragraph(self, para, tight):
        parts = []
        last = len(para) - 1
        for k, (_, line) in enumerate(para):
            parts.append(line.lstrip() if k < last else line.strip())
        body = self.inline('\n'.join(parts)).strip()
        prefix, self.prefix = self.prefix, ''
        if not body and not prefix:
            return ''
        body = prefix + body
        return body if tight else f'<p>{body}</p>'

    def heading(self, level, text, line_no):
        inner = self.inline(text.strip())
        title = ' '.join(_plain(inner).split())
        anchor = slugify(title, self.slugs)
        self.headings.append((line_no, level, title, anchor))
        return f'<h{level}><a name="{_attr(anchor)}"></a>{inner}</h{level}>'

    def code_block(self, code, lang):
        body = highlight_code(code, lang if self.highlight else '')
        label = f'<p class="lang">{_esc(lang)}</p>' if lang and len(lang) < 24 else ''
        return (
            '<table class="code" width="100%" cellspacing="0" cellpadding="0"><tr><td>'
            f'{label}<pre>{body}</pre></td></tr></table>'
        )

    @staticmethod
    def _end(body):
        """Drop the bottom margin of the last paragraph inside a box or list item."""
        if body.endswith('</p>'):
            k = body.rfind('<p>')
            if k >= 0 and '</p>' not in body[k:-4]:
                return body[:k] + '<p class="end">' + body[k + 3 :]
        return body

    def quote(self, inner):
        kind = None
        if inner:
            m = re.match(r'^\s*\[!(\w+)\]\s*$', inner[0][1])
            if m and m.group(1).lower() in ALERTS:
                kind = m.group(1).lower()
                inner = inner[1:]
        body = self._end(self.blocks(inner))
        if kind:
            icon, label = ALERTS[kind]
            body = f'<p class="at-{kind}"><b>{icon}&nbsp; {label}</b></p>' + body
            return (
                f'<table class="alert" width="100%" cellspacing="0" cellpadding="0">'
                f'<tr><td class="a-{kind}">{body}</td></tr></table>'
            )
        return (
            '<table class="quote" width="100%" cellspacing="0" cellpadding="0">'
            f'<tr><td class="qbar">{body}</td></tr></table>'
        )

    def _table_at(self, lines, i):
        if i + 1 >= len(lines):
            return False
        delim = lines[i + 1][1]
        if '-' not in delim or not ('|' in delim or '|' in lines[i][1]):
            return False
        cells = _split_row(delim)
        return all(DELIM_CELL_RE.match(c) for c in cells) and len(cells) == len(
            _split_row(lines[i][1])
        )

    def table(self, lines, i):
        header = _split_row(lines[i][1])
        aligns = []
        for cell in _split_row(lines[i + 1][1]):
            cell = cell.strip()
            left, right = cell.startswith(':'), cell.endswith(':')
            aligns.append('center' if left and right else 'right' if right else 'left')
        rows = []
        j = i + 2
        while j < len(lines):
            t = lines[j][1]
            if not t.strip() or (self._starts_block(t) and not t.lstrip().startswith('|')):
                break
            rows.append(_split_row(t))
            j += 1
        width = len(header)

        def row(cells, tag, cls=''):
            cells = (cells + [''] * width)[:width]
            out = ''.join(
                f'<{tag} align="{a}" valign="top">{self.inline(c) or "&nbsp;"}</{tag}>' for c, a in zip(cells, aligns)
            )
            return f'<tr{cls}>{out}</tr>'

        body = [row(header, 'th')]
        body += [row(r, 'td', ' class="odd"' if k % 2 else '') for k, r in enumerate(rows)]
        return (
            '<table class="grid" cellspacing="0" cellpadding="0">' + ''.join(body) + '</table>',
            j,
        )

    def list_block(self, lines, i):
        first = LIST_RE.match(lines[i][1])
        marker = first.group(2)
        ordered = marker[-1] in '.)'
        kind = marker[-1] if ordered else marker
        start = int(marker[:-1]) if ordered else 1
        items = []
        loose = False
        n = len(lines)
        while i < n:
            no, line = lines[i]
            m = LIST_RE.match(line)
            if not m or HR_RE.match(line):
                break
            mk = m.group(2)
            if (mk[-1] if mk[-1] in '.)' else mk) != kind or ((mk[-1] in '.)') != ordered):
                break
            base = len(m.group(1)) + len(mk)
            if m.group(3) is None:
                content, first_text = base + 1, ''
            elif len(m.group(3)) > 4:
                content, first_text = base + 1, ' ' * (len(m.group(3)) - 1) + m.group(4)
            else:
                content, first_text = base + len(m.group(3)), m.group(4)
            item = [(no, first_text)]
            j = i + 1
            fence = FENCE_RE.match(first_text) is not None
            while j < n:
                t = lines[j][1]
                if not t.strip():
                    item.append((lines[j][0], ''))
                    j += 1
                    continue
                lead = len(t) - len(t.lstrip(' '))
                if lead >= content:
                    item.append((lines[j][0], t[content:]))
                    j += 1
                    continue
                if (
                    not fence
                    and item[-1][1].strip()
                    and not self._starts_block(t)
                    and not LIST_RE.match(t)
                ):
                    item.append((lines[j][0], t.lstrip()))
                    j += 1
                    continue
                break
            k = len(item)
            while k > 1 and not item[k - 1][1].strip():
                k -= 1
            body = item[:k]
            in_fence = False
            for _, t in body[1:]:
                if FENCE_RE.match(t) or re.match(r'^ {0,3}(`{3,}|~{3,})\s*$', t):
                    in_fence = not in_fence
                elif not t.strip() and not in_fence:
                    loose = True
            items.append(body)
            i = j
            if k < len(item) and i < n and LIST_RE.match(lines[i][1]):
                loose = True
        task = all(TASK_RE.match(body[0][1]) for body in items)
        out = []
        self.list_depth += 1
        for index, body in enumerate(items):
            tm = TASK_RE.match(body[0][1])
            if tm:
                done = tm.group(1) != ' '
                glyph = '☑' if done else '☐'
                cls = 'done' if done else 'todo'
                self.prefix = f'<span class="{cls}">{glyph}</span>&nbsp; '
                body = [(body[0][0], body[0][1][tm.end():])] + body[1:]
            content = self.blocks(body, tight=not loose)
            if self.prefix:
                content = self.prefix + content
                self.prefix = ''
            last = index == len(items) - 1 and self.list_depth == 1
            out.append(f'<li class="last">' if last else '<li>')
            out.append(f'{self._end(content)}</li>')
        self.list_depth -= 1
        if ordered:
            attrs = f' start="{start}"' if start != 1 else ''
            return f'<ol{attrs}>' + ''.join(out) + '</ol>', i
        style = 'none' if task else ('disc', 'circle', 'square')[min(self.list_depth, 2)]
        return f'<ul style="list-style-type:{style}">' + ''.join(out) + '</ul>', i

    # Inline ----------------------------------------------------------------

    def _ph(self, fragment):
        self.store.append(fragment)
        return f'\x02{len(self.store) - 1}\x03'

    def _restore(self, text):
        while '\x02' in text:
            text = TOKEN_RE.sub(lambda m: self.store[int(m.group(1))], text)
        return text

    def inline(self, text):
        text = text.replace('\x02', '').replace('\x03', '')

        def code(m):
            body = m.group(2).replace('\n', ' ')
            if body.startswith(' ') and body.endswith(' ') and body.strip():
                body = body[1:-1]
            return self._ph('<code>' + _esc(body) + '</code>')

        text = CODE_SPAN_RE.sub(code, text)
        text = re.sub(r'\\\n', lambda m: self._ph('<br>'), text)
        text = ESCAPE_RE.sub(lambda m: self._ph(_esc(m.group(1))), text)
        text = COMMENT_RE.sub('', text)

        def angle(m):
            if m.group(1):
                return self._link_html(m.group(1), _esc(m.group(1)))
            return self._link_html('mailto:' + m.group(2), _esc(m.group(2)))

        text = ANGLE_LINK_RE.sub(angle, text)
        text = self._html_tags(text)
        text = FOOTREF_RE.sub(self._footref, text)
        text = self._links(text)
        return self._restore(self._finish(text))

    def _finish(self, text, autolink=True):
        """Bare URLs, entities, escaping, emphasis and line breaks (tokens stay)."""

        def bare(m):
            url = m.group(1)
            while url.endswith(')') and url.count(')') > url.count('('):
                url = url[:-1]
            tail = m.group(1)[len(url):]
            target = url if not url.startswith('www.') else 'http://' + url
            return self._link_html(target, _esc(url)) + tail

        if autolink and ('.' in text):
            text = BARE_URL_RE.sub(bare, text)
            if '@' in text:
                text = BARE_MAIL_RE.sub(
                    lambda m: self._link_html('mailto:' + m.group(1), _esc(m.group(1))), text
                )

        def entity(m):
            value = html.unescape(m.group(0))
            return self._ph(_esc(value)) if value != m.group(0) else m.group(0)

        text = ENTITY_RE.sub(entity, text)
        text = _esc(text)
        if '*' in text or '_' in text or '~' in text:
            for pattern, repl in EMPHASIS:
                text = pattern.sub(repl, text)
        text = re.sub(r'(?: {2,}|\t)\n', '<br>', text)
        return text.replace('\n', ' ')

    def _html_tags(self, text):
        open_links = 0

        def tag(m):
            nonlocal open_links
            closing, name, attrs = m.group(1), m.group(2).lower(), m.group(3) or ''
            if name == 'br':
                return self._ph('<br>')
            if name in FORMAT_TAGS:
                mapped = {'em': 'i', 'strong': 'b', 'strike': 's', 'del': 's', 'tt': 'code',
                          'samp': 'code', 'var': 'i', 'cite': 'i', 'ins': 'u', 'abbr': 'span',
                          'q': 'span'}.get(name, name)  # fmt: skip
                return self._ph(f'</{mapped}>' if closing else f'<{mapped}>')
            if name in DROPPED_TAGS:
                return ''
            values = {k.lower(): html.unescape(v.strip('"\'')) for k, v in ATTR_RE.findall(attrs)}
            if name == 'img' and not closing:
                return self._img_html(values.get('src', ''), values.get('alt', ''), values.get('width'))
            if name == 'a':
                if closing:
                    if open_links:
                        open_links -= 1
                        return self._ph('</a>')
                    return ''
                href = values.get('href', '')
                if link_kind(href):
                    open_links += 1
                    return self._ph(f'<a href="{_attr(href)}">')
                return ''
            return m.group(0)

        text = TAG_RE.sub(tag, text)
        return text + self._ph('</a>' * open_links) if open_links else text

    def _footref(self, m):
        key = m.group(1).lower()
        if key not in self.footnotes:
            return m.group(0)
        if key not in self.footnote_order:
            self.footnote_order.append(key)
        number = self.footnote_order.index(key) + 1
        slug = re.sub(r'[^\w-]', '', key)
        return self._ph(
            f'<a name="fnref-{slug}"></a><sup><a href="#fn-{slug}">{number}</a></sup>'
        )

    def _link_html(self, target, inner, title=''):
        kind = link_kind(target)
        if kind is None:
            return self._ph(inner)
        title_attr = f' title="{_attr(title)}"' if title else ''
        return self._ph(f'<a href="{_attr(target)}"{title_attr}>{inner}</a>')

    def _img_html(self, src, alt, width=None):
        src = (src or '').strip()
        kind = link_kind(src)
        if kind not in ('web', 'local') or src.lower().startswith('mailto:'):
            return self._ph(_esc(alt))
        self.images.setdefault(src, alt)
        size = ''
        if width and str(width).isdigit():
            size = f' width="{int(width)}"'
        return self._ph(f'<img src="{_attr(src)}" alt="{_attr(alt)}"{size}>')

    def _links(self, text):
        out = []
        i = 0
        length = len(text)
        while True:
            k = text.find('[', i)
            if k < 0:
                out.append(text[i:])
                break
            depth = 0
            j = k
            while j < length:
                c = text[j]
                if c == '[':
                    depth += 1
                elif c == ']':
                    depth -= 1
                    if depth == 0:
                        break
                j += 1
            if j >= length:
                out.append(text[i : k + 1])
                i = k + 1
                continue
            label = text[k + 1 : j]
            dest = title = None
            end = j + 1
            if text.startswith('(', j + 1):
                parsed = self._destination(text, j + 2)
                if parsed:
                    dest, title, end = parsed
            if dest is None:
                ref = None
                rm = re.match(r'\[([^\]]*)\]', text[j + 1 :])
                if rm:
                    ref = self.refs.get(self._ref_key(rm.group(1) or label))
                    if ref:
                        end = j + 1 + rm.end()
                if ref is None and not label.startswith('^'):
                    ref = self.refs.get(self._ref_key(label))
                    end = j + 1
                if ref:
                    dest, title = ref
            if dest is None:
                out.append(text[i : k + 1])
                i = k + 1
                continue
            image = k > i and text[k - 1] == '!'
            out.append(text[i : k - 1] if image else text[i:k])
            if image:
                alt = _plain(self._restore(label))
                out.append(self._img_html(dest, alt))
            else:
                inner = self._restore(self._finish(self._links(label), autolink=False))
                out.append(self._link_html(dest, inner, title or ''))
            i = end
        return ''.join(out)

    @staticmethod
    def _destination(text, pos):
        """Parse '(dest "title")' starting after '('; return (dest, title, end) or None."""
        length = len(text)
        while pos < length and text[pos] in ' \n':
            pos += 1
        if pos < length and text[pos] == '<':
            close = text.find('>', pos)
            if close < 0:
                return None
            dest = text[pos + 1 : close]
            pos = close + 1
        else:
            start = pos
            depth = 0
            while pos < length:
                c = text[pos]
                if c in ' \n':
                    break
                if c == '(':
                    depth += 1
                elif c == ')':
                    if depth == 0:
                        break
                    depth -= 1
                pos += 1
            dest = text[start:pos]
        while pos < length and text[pos] in ' \n':
            pos += 1
        title = ''
        if pos < length and text[pos] in '"\'(':
            closer = ')' if text[pos] == '(' else text[pos]
            close = text.find(closer, pos + 1)
            if close < 0:
                return None
            title = text[pos + 1 : close]
            pos = close + 1
            while pos < length and text[pos] in ' \n':
                pos += 1
        if pos >= length or text[pos] != ')':
            return None
        return TOKEN_RE.sub('', dest), title, pos + 1

    # Document --------------------------------------------------------------

    def render(self):
        body = self.blocks(self.lines)
        if self.footnote_order:
            notes = []
            for key in self.footnote_order:
                slug = re.sub(r'[^\w-]', '', key)
                content = self.blocks([(0, t) for t in self.footnotes[key]], tight=True)
                notes.append(
                    f'<li><a name="fn-{slug}"></a>{content} <a href="#fnref-{slug}">↩</a></li>'
                )
            body += '\n<hr><ol class="footnotes">' + ''.join(notes) + '</ol>'
        return body


# Code highlighting adds a text fragment per token to the Preview document;
# on very large documents that costs more memory than it is worth.
HIGHLIGHT_DOCUMENT_LIMIT = 300000


def render_markdown(text):
    """Render TEXT; returns Rendered(html, headings, images)."""
    renderer = _Renderer(text or '')
    renderer.highlight = len(text or '') <= HIGHLIGHT_DOCUMENT_LIMIT
    body = renderer.render()
    return Rendered(body, renderer.headings, renderer.images)


def markdown_to_html(s):
    return render_markdown(s).html


HEADING_RE = re.compile(r'^(#{1,6})\s+(.+?)\s*#*\s*$')


def extract_headings(text):
    """Return [(line_number, level, title)] for ATX headings outside code fences."""
    headings = []
    fence = None
    for number, line in enumerate(text.split('\n')):
        stripped = line.lstrip()
        if stripped.startswith(('```', '~~~')):
            mark = stripped[:3]
            if fence is None:
                fence = mark
            elif mark == fence:
                fence = None
            continue
        if fence:
            continue
        match = HEADING_RE.match(line)
        if match and match.group(2).strip('# '):
            headings.append((number, len(match.group(1)), match.group(2).strip()))
    return headings


def find_heading(headings, wanted):
    """Index into HEADINGS [(line, level, title, anchor)] for WANTED (text or slug)."""
    if not wanted:
        return None
    raw = wanted.strip().lstrip('#').strip()
    key = raw.casefold()
    slug = slugify(raw)
    for test in (
        lambda h: h[3] == raw or h[3] == slug,
        lambda h: h[2].casefold() == key,
        lambda h: h[3].startswith(slug) or h[2].casefold().startswith(key),
        lambda h: key in h[2].casefold() or (slug and slug in h[3]),
    ):
        for index, heading in enumerate(headings):
            if test(heading):
                return index
    return None

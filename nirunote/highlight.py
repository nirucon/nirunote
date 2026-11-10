"""Cheap regex syntax highlighting for fenced code in the Preview.

One compiled pattern per language, built on first use, classifies comments,
strings, numbers and words. The output is escaped HTML with <span class="tX">
tokens; the colours come from the Preview style sheet. Unknown languages and
very large blocks are shown as plain text.
"""

from __future__ import annotations

import html
import re

MAX_HIGHLIGHT_CHARS = 60000

_C_COMMENT = r'//[^\n]*|/\*.*?\*/'
_HASH_COMMENT = r'(?<![\w$#{])#[^\n]*'
_DQ = r'"(?:\\.|[^"\\\n])*"'
_SQ = r"'(?:\\.|[^'\\\n])*'"
_NUMBER = r'\b(?:0[xX][0-9a-fA-F_]+|0[bB][01_]+|\d[\d_]*(?:\.\d[\d_]*)?(?:[eE][+-]?\d+)?)\b'


def _words(text):
    return frozenset(text.split())


_C_FAMILY_TYPES = _words(
    'int long short char float double void bool unsigned signed size_t ssize_t uint8_t '
    'uint16_t uint32_t uint64_t int8_t int16_t int32_t int64_t auto const static'
)
LANGS = {
    'python': dict(
        comment=r'#[^\n]*',
        strings=r'[rRbBfFuU]{0,2}(?:"""[\s\S]*?"""|\'\'\'[\s\S]*?\'\'\'|' + _DQ + '|' + _SQ + ')',
        extra=[('tp', r'@[\w.]+')],
        keywords=_words(
            'and as assert async await break class continue def del elif else except finally '
            'for from global if import in is lambda nonlocal not or pass raise return try while '
            'with yield match case'
        ),
        builtins=_words(
            'True False None self cls print len range str int float list dict set tuple bool '
            'open super isinstance type object Exception ValueError KeyError'
        ),
    ),
    'shell': dict(
        comment=_HASH_COMMENT,
        strings=_DQ + '|' + r"'[^']*'",
        extra=[('tb', r'\$\{[^}\n]*\}|\$\([^)\n]*\)|\$[A-Za-z_]\w*|\$[0-9@#?*!$-]')],
        keywords=_words(
            'if then else elif fi for while until do done case esac in function return local '
            'export readonly set unset source exit select time'
        ),
        builtins=frozenset(),
        command_words=True,
    ),
    'fish': dict(
        comment=_HASH_COMMENT,
        strings=_DQ + '|' + r"'[^']*'",
        extra=[('tb', r'\$[A-Za-z_]\w*')],
        keywords=_words(
            'if else end for while in function return switch case begin and or not set '
            'functions source exit break continue'
        ),
        builtins=frozenset(),
        command_words=True,
    ),
    'c': dict(
        comment=_C_COMMENT,
        strings=_DQ + '|' + _SQ,
        extra=[('tp', r'^[ \t]*#[ \t]*\w+[^\n]*')],
        keywords=_words(
            'if else for while do switch case default break continue return goto struct union '
            'enum typedef sizeof extern inline volatile register class namespace template '
            'typename public private protected virtual override new delete using try catch '
            'throw this nullptr true false constexpr noexcept operator'
        ),
        builtins=_C_FAMILY_TYPES | _words('NULL std string vector'),
    ),
    'rust': dict(
        comment=_C_COMMENT,
        strings=r'r#*"[\s\S]*?"#*|b?' + _DQ + r"|b?'(?:\\.|[^\\'\n])'",
        extra=[('tp', r'#!?\[[^\]\n]*\]'), ('tf', r'\b\w+!'), ('tb', r"'[a-z_]\w*\b")],
        keywords=_words(
            'as async await break const continue crate dyn else enum extern false fn for if impl '
            'in let loop match mod move mut pub ref return self Self static struct super trait '
            'true type unsafe use where while'
        ),
        builtins=_words(
            'i8 i16 i32 i64 i128 isize u8 u16 u32 u64 u128 usize f32 f64 bool char str String '
            'Vec Option Some None Result Ok Err Box'
        ),
    ),
    'go': dict(
        comment=_C_COMMENT,
        strings=_DQ + '|`[^`]*`|' + _SQ,
        keywords=_words(
            'break case chan const continue default defer else fallthrough for func go goto if '
            'import interface map package range return select struct switch type var true false nil'
        ),
        builtins=_words(
            'int int8 int16 int32 int64 uint uint8 uint16 uint32 uint64 float32 float64 string '
            'bool byte rune error make len cap append new panic'
        ),
    ),
    'js': dict(
        comment=_C_COMMENT,
        strings=_DQ + '|' + _SQ + r'|`(?:\\.|[^`\\])*`',
        keywords=_words(
            'break case catch class const continue debugger default delete do else export '
            'extends finally for function if import in instanceof let new return super switch '
            'this throw try typeof var void while with yield async await of from as interface '
            'type enum implements readonly'
        ),
        builtins=_words(
            'true false null undefined NaN Infinity console window document Math JSON Promise '
            'Array Object String Number Boolean string number boolean any unknown never'
        ),
    ),
    'java': dict(
        comment=_C_COMMENT,
        strings=_DQ + '|' + _SQ,
        extra=[('tp', r'@\w+')],
        keywords=_words(
            'abstract break case catch class continue default do else enum extends final '
            'finally for if implements import instanceof interface new package private protected '
            'public return static super switch this throw throws try while val var fun object '
            'when true false null'
        ),
        builtins=_words('int long short byte char float double boolean void String'),
    ),
    'lua': dict(
        comment=r'--\[\[[\s\S]*?\]\]|--[^\n]*',
        strings=_DQ + '|' + _SQ + r'|\[\[[\s\S]*?\]\]',
        keywords=_words(
            'and break do else elseif end false for function goto if in local nil not or repeat '
            'return then true until while'
        ),
        builtins=_words('print pairs ipairs require table string math os io type tostring'),
    ),
    'sql': dict(
        comment=r'--[^\n]*|/\*.*?\*/',
        strings=_SQ + '|' + _DQ,
        keywords=_words(
            'select from where and or not insert into values update set delete create table '
            'drop alter add index primary key foreign references join left right inner outer on '
            'group by order having limit offset as distinct union all null is in like between '
            'case when then else end begin commit rollback'
        ),
        builtins=_words('integer int text varchar real blob boolean date count sum avg min max'),
        nocase=True,
    ),
    'json': dict(
        strings=None,
        extra=[('tf', r'"(?:\\.|[^"\\\n])*"(?=\s*:)'), ('ts', _DQ)],
        keywords=_words('true false null'),
        builtins=frozenset(),
    ),
    'toml': dict(
        comment=r'#[^\n]*',
        strings=r'"""[\s\S]*?"""|\'\'\'[\s\S]*?\'\'\'|' + _DQ + '|' + r"'[^'\n]*'",
        extra=[
            ('tk', r'^[ \t]*\[\[?[^\]\n]*\]\]?'),
            ('tf', r'^[ \t]*[\w.\-"\']+(?=[ \t]*=)'),
        ],
        keywords=_words('true false'),
        builtins=frozenset(),
    ),
    'ini': dict(
        comment=r'^[ \t]*[#;][^\n]*',
        strings=_DQ,
        extra=[('tk', r'^[ \t]*\[[^\]\n]*\]'), ('tf', r'^[ \t]*[\w.\-]+(?=[ \t]*[=:])')],
        keywords=_words('true false yes no on off'),
        builtins=frozenset(),
    ),
    'yaml': dict(
        comment=r'(?<!\S)#[^\n]*',
        strings=_DQ + '|' + _SQ,
        extra=[
            ('tf', r'^[ \t]*(?:- )?[\w.\-/ ]+(?=:(?:[ \t]|$))'),
            ('tp', r'[&*][\w-]+|!!?\w+'),
        ],
        keywords=_words('true false null yes no on off'),
        builtins=frozenset(),
    ),
    'css': dict(
        comment=r'/\*.*?\*/',
        strings=_DQ + '|' + _SQ,
        extra=[('tn', r'#[0-9a-fA-F]{3,8}\b'), ('tf', r'[\w-]+(?=\s*:[^:])'), ('tk', r'@[\w-]+')],
        keywords=_words('important inherit initial none auto'),
        builtins=frozenset(),
    ),
    'html': dict(
        comment=r'<!--[\s\S]*?-->',
        strings=_DQ + '|' + _SQ,
        extra=[('tk', r'</?[\w:-]+|/?>'), ('tf', r'\b[\w:-]+(?==)'), ('tb', r'&#?\w+;')],
        keywords=frozenset(),
        builtins=frozenset(),
        words=False,
    ),
    'make': dict(
        comment=r'#[^\n]*',
        strings=_DQ + '|' + _SQ,
        extra=[('tb', r'\$[({][^)}\n]*[)}]|\$[@<^*?]'), ('tf', r'^[\w.%/ -]+(?=:)')],
        keywords=_words('ifeq ifneq ifdef ifndef else endif include define endef export'),
        builtins=frozenset(),
    ),
    'diff': dict(lines=True),
}
ALIASES = {
    'py': 'python', 'python3': 'python', 'sh': 'shell', 'bash': 'shell', 'zsh': 'shell',
    'ksh': 'shell', 'console': 'shell', 'shell-session': 'shell', 'shellsession': 'shell',
    'terminal': 'shell', 'cpp': 'c', 'c++': 'c', 'cc': 'c', 'h': 'c', 'hpp': 'c', 'cxx': 'c',
    'objc': 'c', 'cs': 'java', 'csharp': 'java', 'kotlin': 'java', 'kt': 'java',
    'swift': 'java', 'scala': 'java', 'rs': 'rust', 'golang': 'go', 'javascript': 'js',
    'jsx': 'js', 'ts': 'js', 'tsx': 'js', 'typescript': 'js', 'mjs': 'js', 'jsonc': 'json',
    'json5': 'json', 'yml': 'yaml', 'conf': 'ini', 'cfg': 'ini', 'desktop': 'ini',
    'systemd': 'ini', 'properties': 'ini', 'editorconfig': 'ini', 'xml': 'html', 'svg': 'html',
    'xhtml': 'html', 'vue': 'html', 'scss': 'css', 'less': 'css', 'makefile': 'make',
    'mk': 'make', 'patch': 'diff', 'udiff': 'diff', 'postgres': 'sql', 'sqlite': 'sql',
    'mysql': 'sql',
}  # fmt: skip
_COMPILED = {}
_COMMAND_KEEP = _words('sudo exec time env nohup command builtin then do else elif ! if while until')


def _compiled(name):
    spec = LANGS[name]
    if name not in _COMPILED:
        parts = []
        if spec.get('comment'):
            parts.append(f'(?P<tc>{spec["comment"]})')
        if spec.get('strings'):
            parts.append(f'(?P<ts>{spec["strings"]})')
        for index, (cls, pattern) in enumerate(spec.get('extra', [])):
            parts.append(f'(?P<x{index}_{cls}>{pattern})')
        if spec.get('words', True):
            if spec.get('command_words'):
                parts.append(r'(?P<sep>\n|&&|\|\||[|;&(`]|\$\()')
                parts.append(r'(?P<w>[\w./~+-][\w./~+:=@%,-]*)')
            else:
                parts.append(f'(?P<tn>{_NUMBER})')
                parts.append(r'(?P<w>[A-Za-z_][\w]*)')
        flags = re.S | re.M | (re.I if spec.get('nocase') else 0)
        _COMPILED[name] = re.compile('|'.join(parts), flags)
    return _COMPILED[name]


def _span(cls, text):
    return f'<span class="{cls}">{html.escape(text, quote=False)}</span>'


def _diff(code):
    out = []
    for line in code.split('\n'):
        if line.startswith(('+++', '---', 'diff ', 'index ')):
            out.append(_span('tk', line))
        elif line.startswith('@@'):
            out.append(_span('th', line))
        elif line.startswith('+'):
            out.append(_span('ta', line))
        elif line.startswith('-'):
            out.append(_span('td', line))
        else:
            out.append(html.escape(line, quote=False))
    return '\n'.join(out)


def language(lang):
    lang = (lang or '').strip().lower().lstrip('.{').rstrip('}')
    lang = ALIASES.get(lang, lang)
    return lang if lang in LANGS else None


def highlight_code(code, lang):
    """Escaped HTML for CODE, highlighted when LANG is known and the block is small."""
    name = language(lang)
    if name is None or len(code) > MAX_HIGHLIGHT_CHARS:
        return html.escape(code, quote=False)
    spec = LANGS[name]
    if spec.get('lines'):
        return _diff(code)
    keywords = spec.get('keywords', frozenset())
    builtins = spec.get('builtins', frozenset())
    nocase = spec.get('nocase')
    commands = spec.get('command_words')
    out = []
    pos = 0
    at_command = True
    for m in _compiled(name).finditer(code):
        start, end = m.span()
        if start > pos:
            out.append(html.escape(code[pos:start], quote=False))
        kind = m.lastgroup
        text = m.group()
        pos = end
        if kind == 'sep':
            at_command = True
            out.append(html.escape(text, quote=False))
            continue
        if kind == 'w':
            word = text.lower() if nocase else text
            if word in keywords:
                out.append(_span('tk', text))
                at_command = commands and word in _COMMAND_KEEP
            elif commands:
                if '=' in text and at_command and not text.startswith('-'):
                    out.append(_span('tb', text))
                elif at_command and not text[0].isdigit():
                    out.append(_span('tf', text))
                    at_command = text in _COMMAND_KEEP
                else:
                    out.append(html.escape(text, quote=False))
            elif word in builtins:
                out.append(_span('tb', text))
            elif code[end : end + 1] == '(' or code[end : end + 2] == ' (' and name == 'c':
                out.append(_span('tf', text))
            else:
                out.append(html.escape(text, quote=False))
            continue
        if kind.startswith('x'):
            kind = kind.rsplit('_', 1)[1]
        out.append(_span(kind, text))
        if commands and kind != 'tc':
            at_command = False
    out.append(html.escape(code[pos:], quote=False))
    return ''.join(out)

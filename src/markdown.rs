//! A port of NIRUNOTE 0.7's dependency-free Markdown renderer.
//!
//! CommonMark plus the GitHub extensions people write (tables, task lists,
//! strikethrough, autolinks, footnotes, alerts), rendered to the small HTML
//! subset the Preview lays out. Raw HTML is never passed through: a short
//! list of formatting tags is re-emitted without attributes, `<img>` and
//! `<a>` are rebuilt from safe attributes only, layout wrappers are dropped
//! and everything else stays escaped text. Links are kept only for http(s),
//! mailto, #anchors and relative paths.
use crate::codehl::{esc, highlight_code};
use fancy_regex::{Captures, Regex};
use std::collections::HashMap;
use std::sync::OnceLock;

macro_rules! re {
    ($name:ident, $pat:expr) => {
        fn $name() -> &'static Regex {
            static R: OnceLock<Regex> = OnceLock::new();
            R.get_or_init(|| Regex::new($pat).unwrap())
        }
    };
}

re!(fence_re, r"^( {0,3})(`{3,}|~{3,})[ \t]*([^\s`]*)[^`]*$");
re!(
    atx_re,
    r"^ {0,3}(#{1,6})(?:[ \t]+(.*?))??(?:[ \t]+#+)?[ \t]*$"
);
re!(setext_re, r"^ {0,3}(=+|-+)[ \t]*$");
re!(hr_re, r"^ {0,3}([-*_])(?:[ \t]*\1){2,}[ \t]*$");
re!(quote_re, r"^ {0,3}> ?(.*)$");
re!(
    list_re,
    r"^( {0,3})([-+*]|\d{1,9}[.)])(?:([ \t]+)(.*)|[ \t]*$)"
);
re!(task_re, r"^\[([ xX])\][ \t]+");
re!(delim_cell_re, r"^\s*:?-+:?\s*$");
re!(
    refdef_re,
    r#"^ {0,3}\[([^\]^][^\]]*)\]:[ \t]*(<[^>]*>|\S+)(?:[ \t]+("[^"]*"|'[^']*'|\([^)]*\)))?[ \t]*$"#
);
re!(footdef_re, r"^ {0,3}\[\^([^\]\s]+)\]:[ \t]?(.*)$");
re!(token_re, r"\x02(\d+)\x03");
re!(code_span_re, r"(?s)(?<![`\\])(`+)(?!`)(.+?)(?<!`)\1(?!`)");
re!(escape_re, r"\\([!-/:-@\[-`{-~])");
re!(
    angle_link_re,
    r"<((?:https?|mailto):[^\s<>]+)>|<([\w.+-]+@[\w-]+(?:\.[\w-]+)+)>"
);
re!(comment_re, r"(?s)<!--.*?-->");
re!(
    tag_re,
    r"<(/?)([A-Za-z][A-Za-z0-9]*)((?:\s+[^<>]*?)?)\s*(/?)>"
);
re!(
    attr_re,
    r#"([A-Za-z_:][-\w:.]*)\s*=\s*("[^"]*"|'[^']*'|[^\s"'=<>`]+)"#
);
re!(footref_re, r"\[\^([^\]\s]+)\]");
re!(
    bare_url_re,
    r#"(?<![\w@/\x03])((?:https?://|www\.)[^\s<>\x02\x03]*[^\s<>\x02\x03.,:;!?"'*_~)\]])"#
);
re!(
    bare_mail_re,
    r"(?<![\w.+/-])([\w.+-]+@[\w-]+(?:\.[\w-]+)*\.[A-Za-z]{2,})(?![\w@])"
);
re!(
    entity_re,
    r"&(?:#[0-9]{1,7}|#[xX][0-9a-fA-F]{1,6}|[A-Za-z][A-Za-z0-9]{1,31});"
);
re!(close_fence_re, r"^ {0,3}(`{3,}|~{3,})[ \t]*$");
re!(close_fence_any_re, r"^ {0,3}(`{3,}|~{3,})\s*$");
re!(alert_re, r"^\s*\[!(\w+)\]\s*$");
re!(backslash_nl_re, r"\\\n");
re!(hard_break_re, r"(?: {2,}|\t)\n");
re!(tags_any_re, r"<[^>]*>");
re!(ref_label_re, r"^\[([^\]]*)\]");
re!(slug_strip_re, r"[^\w\- ]");
re!(footnote_slug_re, r"[^\w-]");
re!(heading_line_re, r"^(#{1,6})\s+(.+?)\s*#*\s*$");

fn emphasis() -> &'static [(Regex, &'static str, &'static str)] {
    static R: OnceLock<Vec<(Regex, &'static str, &'static str)>> = OnceLock::new();
    R.get_or_init(|| {
        [
            (
                r"(?s)(?<![*\w])\*\*\*(?=\S)(.+?)(?<=\S)\*\*\*(?!\*)",
                "<strong><em>",
                "</em></strong>",
            ),
            (
                r"(?s)(?<![\w_])___(?=\S)(.+?)(?<=\S)___(?![\w_])",
                "<strong><em>",
                "</em></strong>",
            ),
            (
                r"(?s)(?<!\*)\*\*(?=[^\s*])(.+?)(?<=[^\s])\*\*(?!\*)",
                "<strong>",
                "</strong>",
            ),
            (
                r"(?s)(?<![\w_])__(?=[^\s_])(.+?)(?<=\S)__(?![\w_])",
                "<strong>",
                "</strong>",
            ),
            (
                r"(?s)(?<!\*)\*(?=[^\s*])(.+?)(?<=[^\s*])\*(?!\*)",
                "<em>",
                "</em>",
            ),
            (
                r"(?s)(?<![\w_])_(?=[^\s_])(.+?)(?<=[^\s_])_(?![\w_])",
                "<em>",
                "</em>",
            ),
            (r"(?s)(?<!~)~~(?=\S)(.+?)(?<=\S)~~(?!~)", "<s>", "</s>"),
        ]
        .into_iter()
        .map(|(p, a, b)| (Regex::new(p).unwrap(), a, b))
        .collect()
    })
}

const FORMAT_TAGS: [&str; 21] = [
    "b", "i", "em", "strong", "s", "del", "strike", "ins", "u", "small", "kbd", "sub", "sup",
    "mark", "code", "tt", "var", "samp", "abbr", "cite", "q",
];
const DROPPED_TAGS: [&str; 18] = [
    "p",
    "div",
    "span",
    "details",
    "summary",
    "picture",
    "source",
    "center",
    "section",
    "article",
    "nav",
    "header",
    "footer",
    "figure",
    "figcaption",
    "main",
    "aside",
    "font",
];

pub const ALERTS: [(&str, &str, &str); 5] = [
    ("note", "ℹ", "Note"),
    ("tip", "✦", "Tip"),
    ("important", "❢", "Important"),
    ("warning", "⚠", "Warning"),
    ("caution", "⊘", "Caution"),
];

fn is_match(re: &Regex, s: &str) -> bool {
    re.is_match(s).unwrap_or(false)
}

fn caps<'t>(re: &Regex, s: &'t str) -> Option<Captures<'t>> {
    re.captures(s).ok().flatten()
}

fn g<'t>(c: &Captures<'t>, i: usize) -> &'t str {
    c.get(i).map_or("", |m| m.as_str())
}

/// Replace every match with what `f` returns.
fn sub(re: &Regex, text: &str, mut f: impl FnMut(&Captures) -> String) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for c in re.captures_iter(text) {
        let Ok(c) = c else { break };
        let m = c.get(0).unwrap();
        out.push_str(&text[last..m.start()]);
        out.push_str(&f(&c));
        last = m.end();
    }
    out.push_str(&text[last..]);
    out
}

pub fn attr(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            c => out.push(c),
        }
    }
    out
}

fn named_entity(name: &str) -> Option<&'static str> {
    Some(match name {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        "nbsp" => "\u{a0}",
        "copy" => "©",
        "reg" => "®",
        "trade" => "™",
        "hellip" => "…",
        "mdash" => "—",
        "ndash" => "–",
        "minus" => "−",
        "laquo" => "«",
        "raquo" => "»",
        "lsaquo" => "‹",
        "rsaquo" => "›",
        "ldquo" => "“",
        "rdquo" => "”",
        "bdquo" => "„",
        "lsquo" => "‘",
        "rsquo" => "’",
        "sbquo" => "‚",
        "bull" => "•",
        "middot" => "·",
        "deg" => "°",
        "plusmn" => "±",
        "times" => "×",
        "divide" => "÷",
        "euro" => "€",
        "pound" => "£",
        "yen" => "¥",
        "cent" => "¢",
        "curren" => "¤",
        "sect" => "§",
        "para" => "¶",
        "dagger" => "†",
        "Dagger" => "‡",
        "permil" => "‰",
        "prime" => "′",
        "Prime" => "″",
        "larr" => "←",
        "rarr" => "→",
        "uarr" => "↑",
        "darr" => "↓",
        "harr" => "↔",
        "lArr" => "⇐",
        "rArr" => "⇒",
        "uArr" => "⇑",
        "dArr" => "⇓",
        "hArr" => "⇔",
        "crarr" => "↵",
        "check" => "✓",
        "cross" => "✗",
        "star" => "☆",
        "starf" => "★",
        "hearts" => "♥",
        "spades" => "♠",
        "clubs" => "♣",
        "diams" => "♦",
        "frac12" => "½",
        "frac14" => "¼",
        "frac34" => "¾",
        "sup1" => "¹",
        "sup2" => "²",
        "sup3" => "³",
        "micro" => "µ",
        "ordf" => "ª",
        "ordm" => "º",
        "not" => "¬",
        "shy" => "\u{ad}",
        "macr" => "¯",
        "acute" => "´",
        "cedil" => "¸",
        "uml" => "¨",
        "iexcl" => "¡",
        "iquest" => "¿",
        "brvbar" => "¦",
        "infin" => "∞",
        "ne" => "≠",
        "le" => "≤",
        "ge" => "≥",
        "asymp" => "≈",
        "equiv" => "≡",
        "sum" => "∑",
        "prod" => "∏",
        "radic" => "√",
        "part" => "∂",
        "nabla" => "∇",
        "isin" => "∈",
        "empty" => "∅",
        "and" => "∧",
        "or" => "∨",
        "cap" => "∩",
        "cup" => "∪",
        "int" => "∫",
        "there4" => "∴",
        "forall" => "∀",
        "exist" => "∃",
        "loz" => "◊",
        "ensp" => "\u{2002}",
        "emsp" => "\u{2003}",
        "thinsp" => "\u{2009}",
        "zwnj" => "\u{200c}",
        "zwj" => "\u{200d}",
        "lrm" => "\u{200e}",
        "rlm" => "\u{200f}",
        "Agrave" => "À",
        "Aacute" => "Á",
        "Acirc" => "Â",
        "Atilde" => "Ã",
        "Auml" => "Ä",
        "Aring" => "Å",
        "AElig" => "Æ",
        "Ccedil" => "Ç",
        "Egrave" => "È",
        "Eacute" => "É",
        "Ecirc" => "Ê",
        "Euml" => "Ë",
        "Igrave" => "Ì",
        "Iacute" => "Í",
        "Icirc" => "Î",
        "Iuml" => "Ï",
        "ETH" => "Ð",
        "Ntilde" => "Ñ",
        "Ograve" => "Ò",
        "Oacute" => "Ó",
        "Ocirc" => "Ô",
        "Otilde" => "Õ",
        "Ouml" => "Ö",
        "Oslash" => "Ø",
        "Ugrave" => "Ù",
        "Uacute" => "Ú",
        "Ucirc" => "Û",
        "Uuml" => "Ü",
        "Yacute" => "Ý",
        "THORN" => "Þ",
        "szlig" => "ß",
        "agrave" => "à",
        "aacute" => "á",
        "acirc" => "â",
        "atilde" => "ã",
        "auml" => "ä",
        "aring" => "å",
        "aelig" => "æ",
        "ccedil" => "ç",
        "egrave" => "è",
        "eacute" => "é",
        "ecirc" => "ê",
        "euml" => "ë",
        "igrave" => "ì",
        "iacute" => "í",
        "icirc" => "î",
        "iuml" => "ï",
        "eth" => "ð",
        "ntilde" => "ñ",
        "ograve" => "ò",
        "oacute" => "ó",
        "ocirc" => "ô",
        "otilde" => "õ",
        "ouml" => "ö",
        "oslash" => "ø",
        "ugrave" => "ù",
        "uacute" => "ú",
        "ucirc" => "û",
        "uuml" => "ü",
        "yacute" => "ý",
        "thorn" => "þ",
        "yuml" => "ÿ",
        "OElig" => "Œ",
        "oelig" => "œ",
        "Scaron" => "Š",
        "scaron" => "š",
        "Yuml" => "Ÿ",
        "fnof" => "ƒ",
        "circ" => "ˆ",
        "tilde" => "˜",
        "Alpha" => "Α",
        "Beta" => "Β",
        "Gamma" => "Γ",
        "Delta" => "Δ",
        "Omega" => "Ω",
        "Sigma" => "Σ",
        "Pi" => "Π",
        "alpha" => "α",
        "beta" => "β",
        "gamma" => "γ",
        "delta" => "δ",
        "epsilon" => "ε",
        "lambda" => "λ",
        "mu" => "μ",
        "pi" => "π",
        "sigma" => "σ",
        "tau" => "τ",
        "phi" => "φ",
        "omega" => "ω",
        "theta" => "θ",
        _ => return None,
    })
}

/// HTML entity decoding (numeric references and the common named ones).
pub fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'&' {
            if let Some(semi) = s[i + 1..].find(';').map(|k| k + i + 1) {
                let body = &s[i + 1..semi];
                if body.len() <= 32 {
                    let decoded = if let Some(num) = body.strip_prefix('#') {
                        let v = if let Some(h) =
                            num.strip_prefix('x').or_else(|| num.strip_prefix('X'))
                        {
                            u32::from_str_radix(h, 16).ok()
                        } else {
                            num.parse::<u32>().ok()
                        };
                        v.map(|v| {
                            let v = match v {
                                0 => 0xfffd,
                                0x80..=0x9f => cp1252(v),
                                0xd800..=0xdfff => 0xfffd,
                                v if v > 0x10ffff => 0xfffd,
                                v => v,
                            };
                            char::from_u32(v).unwrap_or('\u{fffd}').to_string()
                        })
                    } else {
                        named_entity(body).map(str::to_string)
                    };
                    if let Some(d) = decoded {
                        out.push_str(&d);
                        i = semi + 1;
                        continue;
                    }
                }
            }
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn cp1252(v: u32) -> u32 {
    const T: [u32; 32] = [
        0x20ac, 0x81, 0x201a, 0x0192, 0x201e, 0x2026, 0x2020, 0x2021, 0x02c6, 0x2030, 0x0160,
        0x2039, 0x0152, 0x8d, 0x017d, 0x8f, 0x90, 0x2018, 0x2019, 0x201c, 0x201d, 0x2022, 0x2013,
        0x2014, 0x02dc, 0x2122, 0x0161, 0x203a, 0x0153, 0x9d, 0x017e, 0x0178,
    ];
    T[(v - 0x80) as usize]
}

fn plain(fragment: &str) -> String {
    unescape(&tags_any_re().replace_all(fragment, ""))
}

/// The URL scheme as Python's urlparse finds it.
fn scheme(target: &str) -> String {
    let Some(i) = target.find(':') else {
        return String::new();
    };
    let head = &target[..i];
    if i == 0
        || !head.starts_with(|c: char| c.is_ascii_alphabetic())
        || !head
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
    {
        return String::new();
    }
    head.to_ascii_lowercase()
}

pub fn safe_link(target: &str) -> bool {
    matches!(scheme(target.trim()).as_str(), "http" | "https" | "mailto")
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LinkKind {
    Web,
    Anchor,
    Local,
}

pub fn link_kind(target: &str) -> Option<LinkKind> {
    let t = target.trim();
    if t.is_empty() {
        return None;
    }
    if t.starts_with('#') {
        return Some(LinkKind::Anchor);
    }
    if safe_link(t) {
        return Some(LinkKind::Web);
    }
    let head = t.split('/').next().unwrap_or("");
    let head = head.split('#').next().unwrap_or("");
    if t.starts_with("//") || head.contains(':') {
        return None;
    }
    Some(LinkKind::Local)
}

pub fn slugify(text: &str, seen: Option<&mut HashMap<String, usize>>) -> String {
    let lower = text.trim().to_lowercase().replace('`', "");
    let mut slug = slug_strip_re().replace_all(&lower, "").replace(' ', "-");
    if let Some(seen) = seen {
        if let Some(n) = seen.get_mut(&slug) {
            *n += 1;
            slug = format!("{slug}-{n}");
        } else {
            seen.insert(slug.clone(), 0);
        }
    }
    slug
}

fn split_row(line: &str) -> Vec<String> {
    let mut line: Vec<char> = line.trim().chars().collect();
    if line.first() == Some(&'|') {
        line.remove(0);
    }
    if line.last() == Some(&'|') && !(line.len() >= 2 && line[line.len() - 2] == '\\') {
        line.pop();
    }
    let mut cells = Vec::new();
    let mut cur = String::new();
    let mut i = 0;
    let mut tick = 0usize;
    while i < line.len() {
        let c = line[i];
        if c == '\\' && i + 1 < line.len() && line[i + 1] == '|' {
            cur.push('|');
            i += 2;
            continue;
        }
        if c == '`' {
            let run = line[i..].iter().take_while(|c| **c == '`').count();
            tick = if tick == run {
                0
            } else if tick == 0 {
                run
            } else {
                tick
            };
            cur.extend(std::iter::repeat_n('`', run));
            i += run;
            continue;
        }
        if c == '|' && tick == 0 {
            cells.push(cur.trim().to_string());
            cur.clear();
        } else {
            cur.push(c);
        }
        i += 1;
    }
    cells.push(cur.trim().to_string());
    cells
}

fn expand_tabs(line: &str) -> String {
    let mut out = String::new();
    let mut col = 0;
    for c in line.chars() {
        if c == '\t' {
            let n = 4 - col % 4;
            out.extend(std::iter::repeat_n(' ', n));
            col += n;
        } else {
            out.push(c);
            col = if c == '\n' || c == '\r' { 0 } else { col + 1 };
        }
    }
    out
}

fn skip_chars(s: &str, n: usize) -> String {
    s.chars().skip(n).collect()
}

fn lead_spaces(s: &str) -> usize {
    s.len() - s.trim_start_matches(' ').len()
}

fn is_blank(s: &str) -> bool {
    s.trim().is_empty()
}

/// One heading: (source line, level, title, anchor).
pub type Heading = (usize, usize, String, String);

pub struct Rendered {
    pub html: String,
    pub headings: Vec<Heading>,
    /// Image sources and their alt texts.
    #[allow(dead_code)]
    pub images: HashMap<String, String>,
}

type Line = (usize, String);

struct Renderer {
    store: Vec<String>,
    refs: HashMap<String, (String, String)>,
    footnotes: HashMap<String, Vec<String>>,
    footnote_order: Vec<String>,
    headings: Vec<Heading>,
    images: HashMap<String, String>,
    slugs: HashMap<String, usize>,
    list_depth: usize,
    highlight: bool,
    prefix: String,
    lines: Vec<Line>,
}

fn ref_key(label: &str) -> String {
    label
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn starts_block(line: &str) -> bool {
    if is_match(atx_re(), line)
        || is_match(fence_re(), line)
        || is_match(hr_re(), line)
        || is_match(quote_re(), line)
    {
        return true;
    }
    if let Some(m) = caps(list_re(), line) {
        if m.get(4).is_some_and(|x| !is_blank(x.as_str())) {
            let marker = g(&m, 2);
            let first = marker.chars().next().unwrap_or('-');
            return !first.is_ascii_digit()
                || marker[..marker.len() - 1].parse::<u64>().ok() == Some(1);
        }
    }
    line.trim_start().starts_with("<!--")
}

impl Renderer {
    fn new(text: &str) -> Self {
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let lines: Vec<Line> = text
            .split('\n')
            .enumerate()
            .map(|(n, l)| {
                (
                    n,
                    if l.contains('\t') {
                        expand_tabs(l)
                    } else {
                        l.to_string()
                    },
                )
            })
            .collect();
        let mut r = Renderer {
            store: Vec::new(),
            refs: HashMap::new(),
            footnotes: HashMap::new(),
            footnote_order: Vec::new(),
            headings: Vec::new(),
            images: HashMap::new(),
            slugs: HashMap::new(),
            list_depth: 0,
            highlight: true,
            prefix: String::new(),
            lines: Vec::new(),
        };
        r.lines = r.collect_definitions(lines);
        r
    }

    fn collect_definitions(&mut self, lines: Vec<Line>) -> Vec<Line> {
        let mut kept: Vec<Line> = Vec::new();
        let mut fence: Option<String> = None;
        let mut i = 0;
        while i < lines.len() {
            let line = &lines[i].1;
            let stripped = line.trim_start_matches(['>', ' ']);
            if let Some(m) = caps(fence_re(), stripped) {
                let mark = g(&m, 2);
                let ok = match &fence {
                    None => true,
                    Some(f) => mark.starts_with(&f[..1]) && mark.len() >= f.len(),
                };
                if ok {
                    fence = if fence.is_some() {
                        None
                    } else {
                        Some(mark.to_string())
                    };
                    kept.push(lines[i].clone());
                    i += 1;
                    continue;
                }
            }
            if fence.is_none() {
                if let Some(fm) = caps(footdef_re(), line) {
                    let mut body = vec![g(&fm, 2).to_string()];
                    let key = g(&fm, 1).to_lowercase();
                    i += 1;
                    while i < lines.len() {
                        let nxt = &lines[i].1;
                        if nxt.starts_with("    ")
                            || (!is_blank(nxt)
                                && !is_blank(body.last().unwrap())
                                && !is_match(footdef_re(), nxt)
                                && !is_match(refdef_re(), nxt)
                                && !starts_block(nxt))
                        {
                            body.push(nxt.strip_prefix("    ").unwrap_or(nxt).to_string());
                            i += 1;
                        } else if is_blank(nxt)
                            && i + 1 < lines.len()
                            && lines[i + 1].1.starts_with("    ")
                        {
                            body.push(String::new());
                            i += 1;
                        } else {
                            break;
                        }
                    }
                    self.footnotes.entry(key).or_insert(body);
                    continue;
                }
                if let Some(rm) = caps(refdef_re(), line) {
                    let prev_ok = kept
                        .last()
                        .is_none_or(|k| is_blank(&k.1) || is_match(atx_re(), &k.1));
                    if prev_ok {
                        let mut dest = g(&rm, 2).to_string();
                        if dest.starts_with('<') && dest.ends_with('>') && dest.len() >= 2 {
                            dest = dest[1..dest.len() - 1].to_string();
                        }
                        let title = rm
                            .get(3)
                            .map(|t| {
                                let t = t.as_str();
                                t[1..t.len() - 1].to_string()
                            })
                            .unwrap_or_default();
                        self.refs.entry(ref_key(g(&rm, 1))).or_insert((dest, title));
                        i += 1;
                        continue;
                    }
                }
            }
            kept.push(lines[i].clone());
            i += 1;
        }
        kept
    }

    fn blocks(&mut self, lines: &[Line], tight: bool) -> String {
        let mut out: Vec<String> = Vec::new();
        let mut para: Vec<Line> = Vec::new();
        let mut i = 0;
        let n = lines.len();
        macro_rules! flush {
            () => {
                if !para.is_empty() {
                    let p = std::mem::take(&mut para);
                    let r = self.paragraph(&p, tight);
                    if !r.is_empty() {
                        out.push(r);
                    }
                }
            };
        }
        while i < n {
            let (no, line) = (&lines[i].0, &lines[i].1);
            let no = *no;
            if is_blank(line) {
                flush!();
                i += 1;
                continue;
            }
            let indent = lead_spaces(line);
            if !para.is_empty() {
                if let Some(m) = caps(setext_re(), line) {
                    if indent < 4 {
                        let level = if g(&m, 1).starts_with('=') { 1 } else { 2 };
                        let text = para
                            .iter()
                            .map(|(_, t)| t.trim())
                            .collect::<Vec<_>>()
                            .join("\n");
                        para.clear();
                        let h = self.heading(level, &text, lines[i - 1].0);
                        out.push(h);
                        i += 1;
                        continue;
                    }
                }
                if para.last().unwrap().1.contains('|') && self.table_at(lines, i - 1) {
                    para.pop();
                    flush!();
                    i -= 1;
                    continue;
                }
                if indent >= 4 || !starts_block(line) {
                    para.push((no, line.clone()));
                    i += 1;
                    continue;
                }
                flush!();
                continue;
            }
            if indent >= 4 {
                let mut code: Vec<String> = Vec::new();
                let mut j = i;
                while j < n && (is_blank(&lines[j].1) || lines[j].1.starts_with("    ")) {
                    code.push(skip_chars(&lines[j].1, 4));
                    j += 1;
                }
                while code.last().is_some_and(|c| is_blank(c)) {
                    code.pop();
                }
                out.push(self.code_block(&code.join("\n"), ""));
                i = j;
                continue;
            }
            if let Some(m) = caps(fence_re(), line) {
                let fence = g(&m, 2).to_string();
                let lang = g(&m, 3).to_string();
                let strip = g(&m, 1).len();
                let mut code: Vec<String> = Vec::new();
                let mut j = i + 1;
                while j < n {
                    let t = &lines[j].1;
                    if let Some(c) = caps(close_fence_re(), t) {
                        let cm = g(&c, 1);
                        if cm.starts_with(&fence[..1]) && cm.len() >= fence.len() {
                            j += 1;
                            break;
                        }
                    }
                    let lead = lead_spaces(t);
                    code.push(t[lead.min(strip)..].to_string());
                    j += 1;
                }
                out.push(self.code_block(&code.join("\n"), &unescape(&lang)));
                i = j;
                continue;
            }
            if let Some(m) = caps(atx_re(), line) {
                let level = g(&m, 1).len();
                let text = g(&m, 2).to_string();
                let h = self.heading(level, &text, no);
                out.push(h);
                i += 1;
                continue;
            }
            if is_match(hr_re(), line) {
                out.push("<hr>".into());
                i += 1;
                continue;
            }
            if line.trim_start().starts_with("<!--") {
                let mut j = i;
                while j < n && !lines[j].1.contains("-->") {
                    j += 1;
                }
                let rest = if j < n {
                    lines[j].1.split_once("-->").map_or("", |x| x.1).to_string()
                } else {
                    String::new()
                };
                i = j + 1;
                if !is_blank(&rest) {
                    para.push((lines[j].0, rest));
                }
                continue;
            }
            if is_match(quote_re(), line) {
                let mut inner: Vec<Line> = Vec::new();
                let mut j = i;
                while j < n {
                    let t = &lines[j].1;
                    if let Some(qm) = caps(quote_re(), t) {
                        inner.push((lines[j].0, g(&qm, 1).to_string()));
                    } else if !is_blank(t)
                        && inner.last().is_some_and(|l| !is_blank(&l.1))
                        && !starts_block(t)
                    {
                        inner.push((lines[j].0, t.clone()));
                    } else {
                        break;
                    }
                    j += 1;
                }
                out.push(self.quote(inner));
                i = j;
                continue;
            }
            if is_match(list_re(), line) {
                let (r, ni) = self.list_block(lines, i);
                out.push(r);
                i = ni;
                continue;
            }
            if line.contains('|') && self.table_at(lines, i) {
                let (r, ni) = self.table(lines, i);
                out.push(r);
                i = ni;
                continue;
            }
            para.push((no, line.clone()));
            i += 1;
        }
        flush!();
        out.join("\n")
    }

    fn paragraph(&mut self, para: &[Line], tight: bool) -> String {
        let last = para.len() - 1;
        let parts: Vec<&str> = para
            .iter()
            .enumerate()
            .map(|(k, (_, l))| if k < last { l.trim_start() } else { l.trim() })
            .collect();
        let body = self.inline(&parts.join("\n")).trim().to_string();
        let prefix = std::mem::take(&mut self.prefix);
        if body.is_empty() && prefix.is_empty() {
            return String::new();
        }
        let body = prefix + &body;
        if tight {
            body
        } else {
            format!("<p>{body}</p>")
        }
    }

    fn heading(&mut self, level: usize, text: &str, line_no: usize) -> String {
        let inner = self.inline(text.trim());
        let title = plain(&inner)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let anchor = slugify(&title, Some(&mut self.slugs));
        let a = attr(&anchor);
        self.headings.push((line_no, level, title, anchor));
        format!("<h{level}><a name=\"{a}\"></a>{inner}</h{level}>")
    }

    fn code_block(&mut self, code: &str, lang: &str) -> String {
        let body = highlight_code(code, if self.highlight { lang } else { "" });
        let label = if !lang.is_empty() && lang.chars().count() < 24 {
            format!("<p class=\"lang\">{}</p>", esc(lang))
        } else {
            String::new()
        };
        format!(
            "<table class=\"code\" width=\"100%\" cellspacing=\"0\" cellpadding=\"0\"><tr><td>{label}<pre>{body}</pre></td></tr></table>"
        )
    }

    fn end(body: String) -> String {
        if body.ends_with("</p>") {
            if let Some(k) = body.rfind("<p>") {
                if !body[k..body.len() - 4].contains("</p>") {
                    return format!("{}<p class=\"end\">{}", &body[..k], &body[k + 3..]);
                }
            }
        }
        body
    }

    fn quote(&mut self, mut inner: Vec<Line>) -> String {
        let mut kind: Option<(&str, &str, &str)> = None;
        if let Some(first) = inner.first() {
            if let Some(m) = caps(alert_re(), &first.1) {
                let k = g(&m, 1).to_lowercase();
                if let Some(a) = ALERTS.iter().find(|a| a.0 == k) {
                    kind = Some(*a);
                }
            }
            if kind.is_some() {
                inner.remove(0);
            }
        }
        let body = Self::end(self.blocks(&inner, false));
        if let Some((k, icon, label)) = kind {
            let body = format!("<p class=\"at-{k}\"><b>{icon}&nbsp; {label}</b></p>{body}");
            return format!(
                "<table class=\"alert\" width=\"100%\" cellspacing=\"0\" cellpadding=\"0\"><tr><td class=\"a-{k}\">{body}</td></tr></table>"
            );
        }
        format!(
            "<table class=\"quote\" width=\"100%\" cellspacing=\"0\" cellpadding=\"0\"><tr><td class=\"qbar\">{body}</td></tr></table>"
        )
    }

    fn table_at(&self, lines: &[Line], i: usize) -> bool {
        if i + 1 >= lines.len() {
            return false;
        }
        let delim = &lines[i + 1].1;
        if !delim.contains('-') || !(delim.contains('|') || lines[i].1.contains('|')) {
            return false;
        }
        let cells = split_row(delim);
        cells.iter().all(|c| is_match(delim_cell_re(), c))
            && cells.len() == split_row(&lines[i].1).len()
    }

    fn table(&mut self, lines: &[Line], i: usize) -> (String, usize) {
        let header = split_row(&lines[i].1);
        let aligns: Vec<&str> = split_row(&lines[i + 1].1)
            .iter()
            .map(|c| {
                let c = c.trim();
                let (l, r) = (c.starts_with(':'), c.ends_with(':'));
                if l && r {
                    "center"
                } else if r {
                    "right"
                } else {
                    "left"
                }
            })
            .collect();
        let mut rows = Vec::new();
        let mut j = i + 2;
        while j < lines.len() {
            let t = &lines[j].1;
            if is_blank(t) || (starts_block(t) && !t.trim_start().starts_with('|')) {
                break;
            }
            rows.push(split_row(t));
            j += 1;
        }
        let width = header.len();
        let row = |r: &mut Renderer, cells: &[String], tag: &str, cls: &str| {
            let mut cells = cells.to_vec();
            cells.resize(width, String::new());
            let mut out = String::new();
            for (c, a) in cells.iter().zip(aligns.iter()) {
                let mut inner = r.inline(c);
                if inner.is_empty() {
                    inner = "&nbsp;".into();
                }
                out.push_str(&format!(
                    "<{tag} align=\"{a}\" valign=\"top\">{inner}</{tag}>"
                ));
            }
            format!("<tr{cls}>{out}</tr>")
        };
        let mut body = vec![row(self, &header, "th", "")];
        for (k, r) in rows.iter().enumerate() {
            body.push(row(
                self,
                r,
                "td",
                if k % 2 == 1 { " class=\"odd\"" } else { "" },
            ));
        }
        (
            format!(
                "<table class=\"grid\" cellspacing=\"0\" cellpadding=\"0\">{}</table>",
                body.concat()
            ),
            j,
        )
    }

    fn list_block(&mut self, lines: &[Line], mut i: usize) -> (String, usize) {
        let first = caps(list_re(), &lines[i].1).unwrap();
        let marker = g(&first, 2).to_string();
        let last_ch = marker.chars().last().unwrap();
        let ordered = last_ch == '.' || last_ch == ')';
        let kind = if ordered {
            last_ch.to_string()
        } else {
            marker.clone()
        };
        let start: u64 = if ordered {
            marker[..marker.len() - 1].parse().unwrap_or(1)
        } else {
            1
        };
        let mut items: Vec<Vec<Line>> = Vec::new();
        let mut loose = false;
        let n = lines.len();
        while i < n {
            let (no, line) = (lines[i].0, &lines[i].1);
            let Some(m) = caps(list_re(), line) else {
                break;
            };
            if is_match(hr_re(), line) {
                break;
            }
            let mk = g(&m, 2);
            let mlast = mk.chars().last().unwrap();
            let mord = mlast == '.' || mlast == ')';
            let mkind = if mord {
                mlast.to_string()
            } else {
                mk.to_string()
            };
            if mkind != kind || mord != ordered {
                break;
            }
            let base = g(&m, 1).len() + mk.len();
            let (content, first_text) = match m.get(3) {
                None => (base + 1, String::new()),
                Some(sp) if sp.as_str().len() > 4 => {
                    (base + 1, " ".repeat(sp.as_str().len() - 1) + g(&m, 4))
                }
                Some(sp) => (base + sp.as_str().len(), g(&m, 4).to_string()),
            };
            let fence = is_match(fence_re(), &first_text);
            let mut item: Vec<Line> = vec![(no, first_text)];
            let mut j = i + 1;
            while j < n {
                let t = &lines[j].1;
                if is_blank(t) {
                    item.push((lines[j].0, String::new()));
                    j += 1;
                    continue;
                }
                let lead = lead_spaces(t);
                if lead >= content {
                    item.push((lines[j].0, t[content..].to_string()));
                    j += 1;
                    continue;
                }
                if !fence
                    && !is_blank(&item.last().unwrap().1)
                    && !starts_block(t)
                    && !is_match(list_re(), t)
                {
                    item.push((lines[j].0, t.trim_start().to_string()));
                    j += 1;
                    continue;
                }
                break;
            }
            let mut k = item.len();
            while k > 1 && is_blank(&item[k - 1].1) {
                k -= 1;
            }
            let body: Vec<Line> = item[..k].to_vec();
            let mut in_fence = false;
            for (_, t) in &body[1..] {
                if is_match(fence_re(), t) || is_match(close_fence_any_re(), t) {
                    in_fence = !in_fence;
                } else if is_blank(t) && !in_fence {
                    loose = true;
                }
            }
            items.push(body);
            i = j;
            if k < item.len() && i < n && is_match(list_re(), &lines[i].1) {
                loose = true;
            }
        }
        let task = items.iter().all(|b| is_match(task_re(), &b[0].1));
        let mut out = String::new();
        self.list_depth += 1;
        let count = items.len();
        for (index, mut body) in items.into_iter().enumerate() {
            if let Some(tm) = caps(task_re(), &body[0].1) {
                let done = g(&tm, 1) != " ";
                let (glyph, cls) = if done {
                    ("☑", "done")
                } else {
                    ("☐", "todo")
                };
                self.prefix = format!("<span class=\"{cls}\">{glyph}</span>&nbsp; ");
                let end = tm.get(0).unwrap().end();
                body[0].1 = body[0].1[end..].to_string();
            }
            let mut content = self.blocks(&body, !loose);
            if !self.prefix.is_empty() {
                content = std::mem::take(&mut self.prefix) + &content;
            }
            let last = index == count - 1 && self.list_depth == 1;
            out.push_str(if last { "<li class=\"last\">" } else { "<li>" });
            out.push_str(&Self::end(content));
            out.push_str("</li>");
        }
        self.list_depth -= 1;
        if ordered {
            let attrs = if start != 1 {
                format!(" start=\"{start}\"")
            } else {
                String::new()
            };
            return (format!("<ol{attrs}>{out}</ol>"), i);
        }
        let style = if task {
            "none"
        } else {
            ["disc", "circle", "square"][self.list_depth.min(2)]
        };
        (
            format!("<ul style=\"list-style-type:{style}\">{out}</ul>"),
            i,
        )
    }

    // ------------------------------------------------------------ inline

    fn ph(&mut self, fragment: String) -> String {
        self.store.push(fragment);
        format!("\x02{}\x03", self.store.len() - 1)
    }

    fn restore(&self, text: &str) -> String {
        let mut t = text.to_string();
        let mut guard = 0;
        while t.contains('\x02') && guard < 64 {
            guard += 1;
            t = sub(token_re(), &t, |c| {
                g(c, 1)
                    .parse::<usize>()
                    .ok()
                    .and_then(|i| self.store.get(i).cloned())
                    .unwrap_or_default()
            });
        }
        t
    }

    fn inline(&mut self, text: &str) -> String {
        let text = text.replace(['\x02', '\x03'], "");
        let text = sub(code_span_re(), &text, |c| {
            let mut body = g(c, 2).replace('\n', " ");
            if body.starts_with(' ') && body.ends_with(' ') && !is_blank(&body) {
                body = body[1..body.len() - 1].to_string();
            }
            self.ph(format!("<code>{}</code>", esc(&body)))
        });
        let text = sub(backslash_nl_re(), &text, |_| self.ph("<br>".into()));
        let text = sub(escape_re(), &text, |c| self.ph(esc(g(c, 1))));
        let text = comment_re().replace_all(&text, "").into_owned();
        let text = sub(angle_link_re(), &text, |c| {
            if let Some(u) = c.get(1) {
                let u = u.as_str();
                self.link_html(u, &esc(u), "")
            } else {
                let m = g(c, 2);
                self.link_html(&format!("mailto:{m}"), &esc(m), "")
            }
        });
        let text = self.html_tags(&text);
        let text = sub(footref_re(), &text, |c| self.footref(c));
        let text = self.links(&text);
        let fin = self.finish(&text, true);
        self.restore(&fin)
    }

    fn finish(&mut self, text: &str, autolink: bool) -> String {
        let mut text = text.to_string();
        if autolink && text.contains('.') {
            text = sub(bare_url_re(), &text, |c| {
                let whole = g(c, 1);
                let mut url = whole.to_string();
                while url.ends_with(')') && url.matches(')').count() > url.matches('(').count() {
                    url.pop();
                }
                let tail = whole[url.len()..].to_string();
                let target = if url.starts_with("www.") {
                    format!("http://{url}")
                } else {
                    url.clone()
                };
                self.link_html(&target, &esc(&url), "") + &tail
            });
            if text.contains('@') {
                text = sub(bare_mail_re(), &text, |c| {
                    let m = g(c, 1);
                    self.link_html(&format!("mailto:{m}"), &esc(m), "")
                });
            }
        }
        text = sub(entity_re(), &text, |c| {
            let whole = g(c, 0);
            let v = unescape(whole);
            if v != whole {
                self.ph(esc(&v))
            } else {
                whole.to_string()
            }
        });
        let mut text = esc(&text);
        if text.contains(['*', '_', '~']) {
            for (re, open, close) in emphasis() {
                text = sub(re, &text, |c| format!("{open}{}{close}", g(c, 1)));
            }
        }
        let text = hard_break_re().replace_all(&text, "<br>").into_owned();
        text.replace('\n', " ")
    }

    fn html_tags(&mut self, text: &str) -> String {
        let mut open_links = 0usize;
        let out = sub(tag_re(), text, |m| {
            let closing = !g(m, 1).is_empty();
            let name = g(m, 2).to_lowercase();
            let attrs = g(m, 3);
            if name == "br" {
                return self.ph("<br>".into());
            }
            if FORMAT_TAGS.contains(&name.as_str()) {
                let mapped = match name.as_str() {
                    "em" => "i",
                    "strong" => "b",
                    "strike" | "del" => "s",
                    "tt" | "samp" => "code",
                    "var" | "cite" => "i",
                    "ins" => "u",
                    "abbr" | "q" => "span",
                    other => other,
                };
                let t = if closing {
                    format!("</{mapped}>")
                } else {
                    format!("<{mapped}>")
                };
                return self.ph(t);
            }
            if DROPPED_TAGS.contains(&name.as_str()) {
                return String::new();
            }
            let mut values: HashMap<String, String> = HashMap::new();
            for a in attr_re().captures_iter(attrs).flatten() {
                let v = g(&a, 2).trim_matches(|c| c == '"' || c == '\'');
                values.insert(g(&a, 1).to_lowercase(), unescape(v));
            }
            if name == "img" && !closing {
                let src = values.get("src").cloned().unwrap_or_default();
                let alt = values.get("alt").cloned().unwrap_or_default();
                let width = values.get("width").cloned();
                return self.img_html(&src, &alt, width.as_deref());
            }
            if name == "a" {
                if closing {
                    if open_links > 0 {
                        open_links -= 1;
                        return self.ph("</a>".into());
                    }
                    return String::new();
                }
                let href = values.get("href").cloned().unwrap_or_default();
                if link_kind(&href).is_some() {
                    open_links += 1;
                    return self.ph(format!("<a href=\"{}\">", attr(&href)));
                }
                return String::new();
            }
            g(m, 0).to_string()
        });
        if open_links > 0 {
            let close = self.ph("</a>".repeat(open_links));
            out + &close
        } else {
            out
        }
    }

    fn footref(&mut self, m: &Captures) -> String {
        let key = g(m, 1).to_lowercase();
        if !self.footnotes.contains_key(&key) {
            return g(m, 0).to_string();
        }
        if !self.footnote_order.contains(&key) {
            self.footnote_order.push(key.clone());
        }
        let number = self.footnote_order.iter().position(|k| *k == key).unwrap() + 1;
        let slug = footnote_slug_re().replace_all(&key, "").into_owned();
        self.ph(format!(
            "<a name=\"fnref-{slug}\"></a><sup><a href=\"#fn-{slug}\">{number}</a></sup>"
        ))
    }

    fn link_html(&mut self, target: &str, inner: &str, title: &str) -> String {
        if link_kind(target).is_none() {
            return self.ph(inner.to_string());
        }
        let title_attr = if title.is_empty() {
            String::new()
        } else {
            format!(" title=\"{}\"", attr(title))
        };
        self.ph(format!(
            "<a href=\"{}\"{title_attr}>{inner}</a>",
            attr(target)
        ))
    }

    fn img_html(&mut self, src: &str, alt: &str, width: Option<&str>) -> String {
        let src = src.trim();
        let kind = link_kind(src);
        if !matches!(kind, Some(LinkKind::Web) | Some(LinkKind::Local))
            || src.to_lowercase().starts_with("mailto:")
        {
            return self.ph(esc(alt));
        }
        self.images
            .entry(src.to_string())
            .or_insert_with(|| alt.to_string());
        let size = match width {
            Some(w) if !w.is_empty() && w.bytes().all(|b| b.is_ascii_digit()) => {
                format!(" width=\"{}\"", w.parse::<u64>().unwrap_or(0))
            }
            _ => String::new(),
        };
        self.ph(format!(
            "<img src=\"{}\" alt=\"{}\"{size}>",
            attr(src),
            attr(alt)
        ))
    }

    fn links(&mut self, text: &str) -> String {
        let b = text.as_bytes();
        let length = b.len();
        let mut out = String::new();
        let mut i = 0;
        loop {
            let Some(k) = text[i..].find('[').map(|x| x + i) else {
                out.push_str(&text[i..]);
                break;
            };
            let mut depth: i64 = 0;
            let mut j = k;
            while j < length {
                match b[j] {
                    b'[' => depth += 1,
                    b']' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            if j >= length {
                out.push_str(&text[i..k + 1]);
                i = k + 1;
                continue;
            }
            let label = text[k + 1..j].to_string();
            let mut dest: Option<String> = None;
            let mut title = String::new();
            let mut end = j + 1;
            if text[j + 1..].starts_with('(') {
                if let Some((d, t, e)) = destination(text, j + 2) {
                    dest = Some(d);
                    title = t;
                    end = e;
                }
            }
            if dest.is_none() {
                let mut found: Option<(String, String)> = None;
                if let Some(rm) = caps(ref_label_re(), &text[j + 1..]) {
                    let inner = g(&rm, 1);
                    let key = ref_key(if inner.is_empty() { &label } else { inner });
                    found = self.refs.get(&key).cloned();
                    if found.is_some() {
                        end = j + 1 + rm.get(0).unwrap().end();
                    }
                }
                if found.is_none() && !label.starts_with('^') {
                    found = self.refs.get(&ref_key(&label)).cloned();
                    end = j + 1;
                }
                if let Some((d, t)) = found {
                    dest = Some(d);
                    title = t;
                }
            }
            let Some(dest) = dest else {
                out.push_str(&text[i..k + 1]);
                i = k + 1;
                continue;
            };
            let image = k > i && b[k - 1] == b'!';
            out.push_str(&text[i..if image { k - 1 } else { k }]);
            if image {
                let alt = plain(&self.restore(&label));
                let h = self.img_html(&dest, &alt, None);
                out.push_str(&h);
            } else {
                let inner_links = self.links(&label);
                let fin = self.finish(&inner_links, false);
                let inner = self.restore(&fin);
                let h = self.link_html(&dest, &inner, &title);
                out.push_str(&h);
            }
            i = end;
        }
        out
    }

    fn render(&mut self) -> String {
        let lines = std::mem::take(&mut self.lines);
        let mut body = self.blocks(&lines, false);
        if !self.footnote_order.is_empty() {
            let mut notes = String::new();
            let mut idx = 0;
            while idx < self.footnote_order.len() {
                let key = self.footnote_order[idx].clone();
                idx += 1;
                let slug = footnote_slug_re().replace_all(&key, "").into_owned();
                let fl: Vec<Line> = self.footnotes[&key]
                    .iter()
                    .map(|t| (0, t.clone()))
                    .collect();
                let content = self.blocks(&fl, true);
                notes.push_str(&format!(
                    "<li><a name=\"fn-{slug}\"></a>{content} <a href=\"#fnref-{slug}\">↩</a></li>"
                ));
            }
            body.push_str(&format!("\n<hr><ol class=\"footnotes\">{notes}</ol>"));
        }
        body
    }
}

/// Parse `(dest "title")` starting after `(`: (dest, title, end).
fn destination(text: &str, mut pos: usize) -> Option<(String, String, usize)> {
    let b = text.as_bytes();
    let length = b.len();
    while pos < length && (b[pos] == b' ' || b[pos] == b'\n') {
        pos += 1;
    }
    let dest;
    if pos < length && b[pos] == b'<' {
        let close = text[pos..].find('>')? + pos;
        dest = text[pos + 1..close].to_string();
        pos = close + 1;
    } else {
        let start = pos;
        let mut depth = 0;
        while pos < length {
            let c = b[pos];
            if c == b' ' || c == b'\n' {
                break;
            }
            if c == b'(' {
                depth += 1;
            } else if c == b')' {
                if depth == 0 {
                    break;
                }
                depth -= 1;
            }
            pos += 1;
        }
        dest = text[start..pos].to_string();
    }
    while pos < length && (b[pos] == b' ' || b[pos] == b'\n') {
        pos += 1;
    }
    let mut title = String::new();
    if pos < length && matches!(b[pos], b'"' | b'\'' | b'(') {
        let closer = if b[pos] == b'(' { ')' } else { b[pos] as char };
        let close = text[pos + 1..].find(closer)? + pos + 1;
        title = text[pos + 1..close].to_string();
        pos = close + 1;
        while pos < length && (b[pos] == b' ' || b[pos] == b'\n') {
            pos += 1;
        }
    }
    if pos >= length || b[pos] != b')' {
        return None;
    }
    Some((
        token_re().replace_all(&dest, "").into_owned(),
        title,
        pos + 1,
    ))
}

/// Code highlighting adds work per token; very large documents skip it.
const HIGHLIGHT_DOCUMENT_LIMIT: usize = 300000;

pub fn render_markdown(text: &str) -> Rendered {
    let mut r = Renderer::new(text);
    r.highlight = text.chars().count() <= HIGHLIGHT_DOCUMENT_LIMIT;
    let html = r.render();
    Rendered {
        html,
        headings: r.headings,
        images: r.images,
    }
}

/// [(line, level, title)] for ATX headings outside code fences.
pub fn extract_headings(text: &str) -> Vec<(usize, usize, String)> {
    let mut out = Vec::new();
    let mut fence: Option<&str> = None;
    for (n, line) in text.split('\n').enumerate() {
        let s = line.trim_start();
        if s.starts_with("```") || s.starts_with("~~~") {
            let mark = &s[..3];
            match fence {
                None => fence = Some(mark),
                Some(f) if f == mark => fence = None,
                _ => {}
            }
            continue;
        }
        if fence.is_some() {
            continue;
        }
        if let Some(m) = caps(heading_line_re(), line) {
            let title = g(&m, 2);
            if !title.trim_matches(['#', ' ']).is_empty() {
                out.push((n, g(&m, 1).len(), title.trim().to_string()));
            }
        }
    }
    out
}

/// Index into `headings` [(line, level, title, anchor)] for `wanted`.
pub fn find_heading(headings: &[Heading], wanted: &str) -> Option<usize> {
    if wanted.is_empty() {
        return None;
    }
    let raw = wanted.trim().trim_start_matches('#').trim();
    let key = raw.to_lowercase();
    let slug = slugify(raw, None);
    let tests: [&dyn Fn(&Heading) -> bool; 4] = [
        &|h| h.3 == raw || h.3 == slug,
        &|h| h.2.to_lowercase() == key,
        &|h| h.3.starts_with(&slug) || h.2.to_lowercase().starts_with(&key),
        &|h| h.2.to_lowercase().contains(&key) || (!slug.is_empty() && h.3.contains(&slug)),
    ];
    for t in tests {
        if let Some(i) = headings.iter().position(t) {
            return Some(i);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn html(t: &str) -> String {
        render_markdown(t).html
    }

    #[test]
    fn headings_and_anchors() {
        let r =
            render_markdown("# Lock & screensaver\n\n## Details\n\n## Details\n\nSetext\n---\n");
        let a: Vec<&str> = r.headings.iter().map(|h| h.3.as_str()).collect();
        assert_eq!(a, ["lock--screensaver", "details", "details-1", "setext"]);
        assert!(r.html.contains("<a name=\"details-1\"></a>"));
        assert_eq!(slugify("`nirux manual` keys", None), "nirux-manual-keys");
    }

    #[test]
    fn inline_formatting() {
        let out =
            html("*a* _b_ **c** __d__ ***e*** ~~f~~ `g*h*` snake_case_name \\*i\\* &copy; &bogus;");
        for part in [
            "<em>a</em>",
            "<em>b</em>",
            "<strong>c</strong>",
            "<strong>d</strong>",
            "<strong><em>e</em></strong>",
            "<s>f</s>",
            "<code>g*h*</code>",
            "snake_case_name",
            "*i*",
            "©",
            "&amp;bogus;",
        ] {
            assert!(out.contains(part), "{part} in {out}");
        }
    }

    #[test]
    fn lists_quotes_alerts_tables() {
        let out = html("- a\n  - b\n    - c\n- d\n\n3. x\n4. y\n\n- [x] done\n- [ ] todo\n");
        assert_eq!(out.matches("<ul").count(), 4, "{out}");
        assert!(out.contains("<ol start=\"3\">"), "{out}");
        assert!(out.contains('☑') && out.contains('☐') && out.contains("list-style-type:none"));
        assert_eq!(html("* one\n\n* two\n\n  more\n").matches("<p").count(), 3);
        assert_eq!(
            html("> a\n> > b\n> > > c\n")
                .matches("class=\"qbar\"")
                .count(),
            3
        );
        for kind in ["NOTE", "TIP", "IMPORTANT", "WARNING", "CAUTION"] {
            let out = html(&format!("> [!{kind}]\n> body\n"));
            assert!(out.contains(&format!("class=\"a-{}\"", kind.to_lowercase())));
            assert!(!out.contains("[!"));
        }
        let out = html("| L | C | R |\n|:--|:-:|--:|\n| a \\| b | `x|y` | 1 |\n");
        assert!(out.contains("align=\"left\"") && out.contains("align=\"center\""));
        assert!(out.contains("align=\"right\"") && out.contains("a | b"));
        assert!(out.contains("<code>x|y</code>"), "{out}");
        assert_eq!(html("a | b\n--|--\n1 | 2\n").matches("<td").count(), 2);
    }

    #[test]
    fn code_links_images_footnotes_html() {
        let out = html("```\n<b>raw</b>\n```\n\n    indented\n");
        assert!(out.contains("&lt;b&gt;raw&lt;/b&gt;") && out.contains("indented"));
        assert!(!out.contains("span class"));
        let out = html(
            "[a](https://x.test \"T\") [b][r] [c] <https://y.test> https://z.test/p_q_r www.w.test m@e.test [d](#sec) [e](other.md#h) [f](javascript:alert(1)) [g](file:///etc/passwd)\n\n[r]: https://r.test\n[c]: https://c.test\n",
        );
        for part in [
            "href=\"https://x.test\" title=\"T\"",
            "href=\"https://r.test\"",
            "href=\"https://c.test\"",
            "href=\"https://y.test\"",
            "href=\"https://z.test/p_q_r\"",
            "href=\"http://www.w.test\"",
            "href=\"mailto:m@e.test\"",
            "href=\"#sec\"",
            "href=\"other.md#h\"",
        ] {
            assert!(out.contains(part), "{part} in {out}");
        }
        assert!(!out.rsplit("href").next().unwrap().contains("javascript"));
        assert!(!out.contains("file:"));
        let r =
            render_markdown("![one](pic.png) ![two](https://i.test/a.png) ![three](javascript:x)");
        assert!(r.html.contains("<img src=\"pic.png\""));
        assert!(r.html.contains("<img src=\"https://i.test/a.png\""));
        assert!(r.html.contains("three") && !r.html.contains("javascript"));
        assert_eq!(r.images["pic.png"], "one");
        let out = html("Text[^n].\n\n[^n]: Note body.\n");
        assert!(out.contains("href=\"#fn-n\"") && out.contains("<a name=\"fn-n\"></a>Note body."));
        assert_eq!(html("a  \nb\\\nc\nd").matches("<br>").count(), 2);
        let out = html("<details>\n<summary>S</summary>\n\nBody\n</details>\n<!-- hidden -->\n<img src=\"x.png\" onerror=\"y\">");
        assert!(!out.contains("details") && !out.contains("hidden") && !out.contains("onerror"));
        assert!(!html("<script>alert(1)</script>").contains("<script>"));
        assert!(html("[ok](https://example.test)").contains("href=\"https://example.test\""));
        assert!(!html("[bad](data:text/html,x)").contains("href="));
    }

    #[test]
    fn headings_outside_fences_and_lookup() {
        let h = extract_headings("# One\n```\n# not\n```\n## Two ##\n#nope\n### Three\n");
        assert_eq!(
            h,
            vec![
                (0, 1, "One".to_string()),
                (4, 2, "Two".to_string()),
                (6, 3, "Three".to_string())
            ]
        );
        let r = render_markdown("# Key bindings\n\n## Lock screen\n");
        assert_eq!(find_heading(&r.headings, "lock"), Some(1));
        assert_eq!(find_heading(&r.headings, "#key-bindings"), Some(0));
        assert_eq!(find_heading(&r.headings, "nothing"), None);
    }

    /// `NIRUNOTE_GOLDEN=DIR cargo test -- --ignored`: every DIR/x.md must
    /// render to DIR/x.md.html (written by the Python renderer of 0.7).
    #[test]
    #[ignore]
    fn golden_directory() {
        let dir = std::env::var("NIRUNOTE_GOLDEN").expect("NIRUNOTE_GOLDEN");
        let mut bad = Vec::new();
        let mut n = 0;
        for e in std::fs::read_dir(&dir).unwrap().flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "md") {
                n += 1;
                let src = std::fs::read_to_string(&p).unwrap();
                let want = std::fs::read_to_string(format!("{}.html", p.display())).unwrap();
                let got = render_markdown(&src).html;
                if got.trim_end() != want.trim_end() {
                    let i = got
                        .bytes()
                        .zip(want.bytes())
                        .take_while(|(a, b)| a == b)
                        .count();
                    let lo = i.saturating_sub(80);
                    bad.push(format!(
                        "{}\n  got:  {:?}\n  want: {:?}",
                        p.display(),
                        got.get(lo..(i + 80).min(got.len())).unwrap_or(""),
                        want.get(lo..(i + 80).min(want.len())).unwrap_or("")
                    ));
                }
            }
        }
        assert!(
            bad.is_empty(),
            "{} of {n} differ:\n{}",
            bad.len(),
            bad.join("\n")
        );
    }

    #[test]
    fn the_showcase_matches_the_python_renderer() {
        let src = include_str!("../tests/fixtures/showcase.md");
        let r = render_markdown(src);
        assert!(r.headings.len() >= 20);
        assert!(r.html.contains("class=\"a-caution\"") && r.html.contains("fn-long"));
        let want = include_str!("../tests/fixtures/showcase.html");
        assert_eq!(r.html.trim_end(), want.trim_end());
    }
}

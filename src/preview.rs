//! The styled Preview: the renderer's HTML subset laid out on the canvas
//! with the look of NIRUNOTE 0.7's style sheet (sans-serif text, headings
//! 1.75/1.4/1.18/1.05 em, code boxes on the panel colour with the language
//! label, quotes with a left bar, alerts in their colour, grid tables with
//! alignment and shaded odd rows, lists with disc/circle/square markers,
//! task boxes, footnotes, images scaled to the page). Text can be selected,
//! copied and searched; links and heading anchors work.
use crate::canvas::{Canvas, Image, Rect};
use crate::fonts::{Fonts, BOLD, ITALIC, MONO, SANS};
use crate::images::{Images, Slot};
use crate::markdown::unescape;
use crate::theme::{mix, preview_colors, Palette};
use std::collections::HashMap;
use std::path::Path;

// ------------------------------------------------------------------ DOM

#[derive(Debug, Clone)]
pub enum Node {
    El {
        tag: String,
        attrs: Vec<(String, String)>,
        kids: Vec<Node>,
    },
    Text(String),
}

impl Node {
    fn attr(&self, name: &str) -> Option<&str> {
        match self {
            Node::El { attrs, .. } => attrs
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.as_str()),
            _ => None,
        }
    }
    fn tag(&self) -> &str {
        match self {
            Node::El { tag, .. } => tag,
            _ => "",
        }
    }
    fn kids(&self) -> &[Node] {
        match self {
            Node::El { kids, .. } => kids,
            _ => &[],
        }
    }
    fn class(&self) -> &str {
        self.attr("class").unwrap_or("")
    }
}

const VOID: [&str; 3] = ["br", "hr", "img"];

type Open = (String, Vec<(String, String)>, Vec<Node>);

/// A tolerant parser for the renderer's HTML: unknown closing tags are
/// ignored and open elements are closed at the end.
pub fn parse_html(html: &str) -> Vec<Node> {
    let mut stack: Vec<Open> = vec![("#root".into(), Vec::new(), Vec::new())];
    let b = html.as_bytes();
    let mut i = 0;
    let mut text_start = 0;
    let flush_text = |stack: &mut Vec<Open>, s: &str| {
        if !s.is_empty() {
            stack.last_mut().unwrap().2.push(Node::Text(unescape(s)));
        }
    };
    while i < b.len() {
        if b[i] == b'<' {
            if let Some(end) = html[i..].find('>').map(|e| e + i) {
                let inner = &html[i + 1..end];
                let closing = inner.starts_with('/');
                let body = inner.trim_start_matches('/').trim_end_matches('/');
                let name_end = body.find(|c: char| c.is_whitespace()).unwrap_or(body.len());
                let name = body[..name_end].to_ascii_lowercase();
                if !name.is_empty() && name.bytes().all(|c| c.is_ascii_alphanumeric()) {
                    flush_text(&mut stack, &html[text_start..i]);
                    if closing {
                        if let Some(pos) = stack.iter().rposition(|e| e.0 == name) {
                            if pos > 0 {
                                while stack.len() > pos {
                                    let (tag, attrs, kids) = stack.pop().unwrap();
                                    stack
                                        .last_mut()
                                        .unwrap()
                                        .2
                                        .push(Node::El { tag, attrs, kids });
                                }
                            }
                        }
                    } else {
                        let attrs = parse_attrs(&body[name_end..]);
                        if VOID.contains(&name.as_str()) {
                            stack.last_mut().unwrap().2.push(Node::El {
                                tag: name,
                                attrs,
                                kids: Vec::new(),
                            });
                        } else {
                            stack.push((name, attrs, Vec::new()));
                        }
                    }
                    i = end + 1;
                    text_start = i;
                    continue;
                }
            }
        }
        i += 1;
    }
    flush_text(&mut stack, &html[text_start..]);
    while stack.len() > 1 {
        let (tag, attrs, kids) = stack.pop().unwrap();
        stack
            .last_mut()
            .unwrap()
            .2
            .push(Node::El { tag, attrs, kids });
    }
    stack.pop().unwrap().2
}

fn parse_attrs(s: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        let start = i;
        while i < b.len() && b[i] != b'=' && !b[i].is_ascii_whitespace() {
            i += 1;
        }
        let name = s[start..i].to_ascii_lowercase();
        if name.is_empty() {
            i += 1;
            continue;
        }
        if i < b.len() && b[i] == b'=' {
            i += 1;
            let q = b.get(i).copied();
            let value = if q == Some(b'"') || q == Some(b'\'') {
                let close = s[i + 1..]
                    .find(q.unwrap() as char)
                    .map_or(s.len(), |c| c + i + 1);
                let v = &s[i + 1..close.min(s.len())];
                i = close + 1;
                v
            } else {
                let st = i;
                while i < b.len() && !b[i].is_ascii_whitespace() {
                    i += 1;
                }
                &s[st..i]
            };
            out.push((name, unescape(value)));
        } else {
            out.push((name, String::new()));
        }
    }
    out
}

// --------------------------------------------------------------- layout

#[derive(Clone, Debug)]
pub enum Item {
    Fill(Rect, u32),
    Text(Run),
    Image {
        r: Rect,
        key: String,
    },
    Placeholder {
        r: Rect,
        label: String,
    },
    /// A list marker: drawn, but not part of the text.
    Marker {
        x: f32,
        base: i32,
        text: String,
        face: u8,
        px: f32,
        color: u32,
    },
}

#[derive(Clone, Debug)]
pub struct Run {
    pub x: f32,
    pub top: i32,
    pub h: i32,
    pub base: i32,
    pub face: u8,
    pub px: f32,
    pub color: u32,
    pub bg: Option<u32>,
    pub under: bool,
    pub strike: bool,
    pub link: Option<usize>,
    /// Char offset of the run in the plain text.
    pub flat: usize,
    pub chars: usize,
    pub w: f32,
}

#[derive(Default)]
pub struct Layout {
    pub items: Vec<Item>,
    pub height: i32,
    pub anchors: HashMap<String, i32>,
    pub links: Vec<String>,
    /// The plain text (for copy and find) as characters.
    pub flat: Vec<char>,
}

#[derive(Clone, Copy, PartialEq)]
enum Shift {
    None,
    Sup,
    Sub,
}

#[derive(Clone)]
struct St {
    face: u8,
    px: f32,
    color: u32,
    bg: Option<u32>,
    under: bool,
    strike: bool,
    link: Option<usize>,
    shift: Shift,
    pre: bool,
    lh: f32,
}

enum Tok {
    Word(String, St),
    Space(St),
    Break,
    Img {
        key: String,
        alt: String,
        w: Option<i32>,
        st: St,
    },
    Anchor(String),
}

pub struct Ctx<'a> {
    pub fonts: &'a mut Fonts,
    pub pal: &'a Palette,
    pub images: &'a mut Images,
    pub base_dir: Option<&'a Path>,
    pub px: f32,
    pub scale: f32,
    pub max_img_w: i32,
}

struct Builder<'a, 'b> {
    c: &'b mut Ctx<'a>,
    out: Layout,
}

fn heading_style(level: u8) -> (f32, i32, i32) {
    // size (em), margin top, margin bottom
    match level {
        1 => (1.75, 18, 12),
        2 => (1.4, 22, 10),
        3 => (1.18, 18, 8),
        4 => (1.05, 14, 6),
        _ => (1.0, 12, 6),
    }
}

impl<'a, 'b> Builder<'a, 'b> {
    fn s(&self, v: i32) -> i32 {
        (v as f32 * self.c.scale).round() as i32
    }

    fn token_color(&self, cls: &str) -> Option<u32> {
        let p = self.c.pal;
        Some(match cls {
            "tk" => p.magenta,
            "ts" => p.green,
            "tc" => p.muted,
            "tn" | "tp" => p.yellow,
            "tb" | "th" => p.cyan,
            "tf" => p.blue,
            "ta" => p.green,
            "td" => p.red,
            "done" => p.green,
            "todo" => p.muted,
            _ => return None,
        })
    }

    /// Inline content to tokens.
    fn tokens(&mut self, nodes: &[Node], st: &St, out: &mut Vec<Tok>) {
        let pc = preview_colors(self.c.pal);
        for n in nodes {
            match n {
                Node::Text(t) => {
                    if st.pre {
                        let mut word = String::new();
                        for ch in t.chars() {
                            match ch {
                                '\n' => {
                                    if !word.is_empty() {
                                        out.push(Tok::Word(std::mem::take(&mut word), st.clone()));
                                    }
                                    out.push(Tok::Break);
                                }
                                ' ' => {
                                    if !word.is_empty() {
                                        out.push(Tok::Word(std::mem::take(&mut word), st.clone()));
                                    }
                                    out.push(Tok::Space(st.clone()));
                                }
                                '\t' => {
                                    if !word.is_empty() {
                                        out.push(Tok::Word(std::mem::take(&mut word), st.clone()));
                                    }
                                    for _ in 0..4 {
                                        out.push(Tok::Space(st.clone()));
                                    }
                                }
                                c => word.push(c),
                            }
                        }
                        if !word.is_empty() {
                            out.push(Tok::Word(word, st.clone()));
                        }
                    } else {
                        let mut word = String::new();
                        for ch in t.chars() {
                            if ch.is_whitespace() && ch != '\u{a0}' {
                                if !word.is_empty() {
                                    out.push(Tok::Word(std::mem::take(&mut word), st.clone()));
                                }
                                if !matches!(
                                    out.last(),
                                    Some(Tok::Space(_)) | None | Some(Tok::Break)
                                ) {
                                    out.push(Tok::Space(st.clone()));
                                }
                            } else {
                                word.push(ch);
                            }
                        }
                        if !word.is_empty() {
                            out.push(Tok::Word(word, st.clone()));
                        }
                    }
                }
                Node::El { tag, kids, .. } => {
                    let mut s = st.clone();
                    match tag.as_str() {
                        "br" => {
                            out.push(Tok::Break);
                            continue;
                        }
                        "img" => {
                            out.push(Tok::Img {
                                key: n.attr("src").unwrap_or("").to_string(),
                                alt: n.attr("alt").unwrap_or("").to_string(),
                                w: n.attr("width").and_then(|w| w.parse().ok()),
                                st: st.clone(),
                            });
                            continue;
                        }
                        "a" => {
                            if let Some(name) = n.attr("name") {
                                out.push(Tok::Anchor(name.to_string()));
                            }
                            if let Some(h) = n.attr("href") {
                                self.out.links.push(h.to_string());
                                s.link = Some(self.out.links.len() - 1);
                                s.color = self.c.pal.accent;
                                s.under = true;
                            }
                        }
                        "b" | "strong" => s.face |= BOLD,
                        "i" | "em" => s.face |= ITALIC,
                        "s" => s.strike = true,
                        "u" => s.under = true,
                        "small" => {
                            s.px *= 0.82;
                            s.color = self.c.pal.muted;
                        }
                        "kbd" => {
                            s.face = MONO | (s.face & (BOLD | ITALIC));
                            s.px *= 0.86;
                            s.bg = Some(pc.code_bg);
                        }
                        "code" => {
                            s.face = MONO | (s.face & (BOLD | ITALIC));
                            s.px *= 0.9;
                            s.bg = Some(pc.code_bg);
                        }
                        "sub" => {
                            s.px *= 0.75;
                            s.shift = Shift::Sub;
                        }
                        "sup" => {
                            s.px *= 0.75;
                            s.shift = Shift::Sup;
                        }
                        "mark" => {
                            s.bg = Some(self.c.pal.select);
                            s.color = self.c.pal.fg;
                        }
                        "span" => {
                            if let Some(c) = self.token_color(n.class()) {
                                s.color = c;
                            }
                            if n.class() == "tc" {
                                s.face |= ITALIC;
                            }
                        }
                        _ => {}
                    }
                    self.tokens(kids, &s, out);
                }
            }
        }
    }

    fn metrics(&mut self, st: &St) -> (i32, i32) {
        let m = self.c.fonts.metrics(st.face, st.px);
        (m.ascent, m.descent)
    }

    fn word_w(&mut self, w: &str, st: &St) -> f32 {
        self.c.fonts.width(st.face, st.px, w)
    }

    fn img_size(&mut self, key: &str, w: Option<i32>, max_w: i32) -> Option<(i32, i32)> {
        let slot = self.c.images.get(key, self.c.base_dir);
        match slot {
            Slot::Ready(img) => {
                let (iw, ih) = (img.w, img.h);
                let mut dw = w.filter(|w| *w > 0).unwrap_or(iw).min(max_w.max(16));
                if dw <= 0 {
                    dw = 1;
                }
                let dh = ((ih as i64 * dw as i64) / iw.max(1) as i64).max(1) as i32;
                Some((dw, dh))
            }
            Slot::Pending => Some((1, 1)),
            Slot::Failed => None,
        }
    }

    /// Lay out inline tokens in a box at (x, y) of width `w`; returns the
    /// height. With `measure` nothing is emitted: (natural width, widest
    /// word) are returned in the second value.
    fn inline(
        &mut self,
        toks: &[Tok],
        x: i32,
        y: i32,
        w: i32,
        align: &str,
        measure: bool,
    ) -> (i32, (f32, f32)) {
        // Break into lines of (token index range, widths).
        struct Piece {
            kind: u8, // 0 word, 1 space, 2 image, 3 placeholder, 4 anchor
            text: String,
            st: Option<St>,
            w: f32,
            asc: i32,
            desc: i32,
            key: String,
        }
        let mut lines: Vec<Vec<Piece>> = vec![Vec::new()];
        let mut lw = 0.0f32;
        let mut natural = 0.0f32;
        let mut widest = 0.0f32;
        let mut cur = 0.0f32;
        let width = w.max(8) as f32;
        let max_img = self.c.max_img_w.min(w.max(16));
        for t in toks {
            match t {
                Tok::Break => {
                    natural = natural.max(cur);
                    cur = 0.0;
                    lines.push(Vec::new());
                    lw = 0.0;
                }
                Tok::Anchor(name) => lines.last_mut().unwrap().push(Piece {
                    kind: 4,
                    text: name.clone(),
                    st: None,
                    w: 0.0,
                    asc: 0,
                    desc: 0,
                    key: String::new(),
                }),
                Tok::Space(st) => {
                    let sw = self.c.fonts.advance(st.face, st.px, ' ');
                    cur += sw;
                    if lines.last().unwrap().is_empty() && !st.pre {
                        continue;
                    }
                    let (a, d) = self.metrics(st);
                    lines.last_mut().unwrap().push(Piece {
                        kind: 1,
                        text: " ".into(),
                        st: Some(st.clone()),
                        w: sw,
                        asc: a,
                        desc: d,
                        key: String::new(),
                    });
                    lw += sw;
                }
                Tok::Word(text, st) => {
                    let ww = self.word_w(text, st);
                    cur += ww;
                    widest = widest.max(ww.min(width * 4.0));
                    let (a, d) = self.metrics(st);
                    let (a, d) = match st.shift {
                        Shift::Sup => (a + (st.px * 0.4) as i32, d),
                        Shift::Sub => (a, d + (st.px * 0.25) as i32),
                        Shift::None => (a, d),
                    };
                    if lw + ww > width && lw > 0.0 {
                        // Wrap: drop trailing spaces.
                        while lines.last().unwrap().last().is_some_and(|p| p.kind == 1) {
                            lines.last_mut().unwrap().pop();
                        }
                        lines.push(Vec::new());
                        lw = 0.0;
                    }
                    if ww > width {
                        // A word longer than the line breaks anywhere.
                        let mut part = String::new();
                        let mut pw = 0.0;
                        for ch in text.chars() {
                            let a2 = self.c.fonts.advance(st.face, st.px, ch);
                            if lw + pw + a2 > width && !part.is_empty() {
                                lines.last_mut().unwrap().push(Piece {
                                    kind: 0,
                                    text: std::mem::take(&mut part),
                                    st: Some(st.clone()),
                                    w: pw,
                                    asc: a,
                                    desc: d,
                                    key: String::new(),
                                });
                                lines.push(Vec::new());
                                lw = 0.0;
                                pw = 0.0;
                            }
                            part.push(ch);
                            pw += a2;
                        }
                        if !part.is_empty() {
                            lines.last_mut().unwrap().push(Piece {
                                kind: 0,
                                text: part,
                                st: Some(st.clone()),
                                w: pw,
                                asc: a,
                                desc: d,
                                key: String::new(),
                            });
                            lw += pw;
                        }
                        continue;
                    }
                    lines.last_mut().unwrap().push(Piece {
                        kind: 0,
                        text: text.clone(),
                        st: Some(st.clone()),
                        w: ww,
                        asc: a,
                        desc: d,
                        key: String::new(),
                    });
                    lw += ww;
                }
                Tok::Img {
                    key,
                    alt,
                    w: iw,
                    st,
                } => {
                    let (pw, ph, kind, text) = match self.img_size(key, *iw, max_img) {
                        Some((a, b)) => (a as f32, b, 2u8, String::new()),
                        None => {
                            let label = format!(
                                "▣  {}",
                                if alt.is_empty() {
                                    Path::new(key.split(['?', '#']).next().unwrap_or(key))
                                        .file_name()
                                        .map(|n| n.to_string_lossy().into_owned())
                                        .unwrap_or_else(|| "image".into())
                                } else {
                                    alt.clone()
                                }
                            );
                            let lpx = (st.px * 0.8).max(8.0);
                            let tw = self.c.fonts.width(SANS, lpx, &label) + 20.0;
                            let m = self.c.fonts.metrics(SANS, lpx);
                            (tw.min(max_img as f32), m.height + 10, 3u8, label)
                        }
                    };
                    cur += pw;
                    widest = widest.max(pw);
                    if lw + pw > width && lw > 0.0 {
                        lines.push(Vec::new());
                        lw = 0.0;
                    }
                    lines.last_mut().unwrap().push(Piece {
                        kind,
                        text,
                        st: Some(st.clone()),
                        w: pw,
                        asc: ph,
                        desc: 0,
                        key: key.clone(),
                    });
                    lw += pw;
                }
            }
        }
        natural = natural.max(cur);
        if measure {
            let n = lines.len() as i32;
            return (n, (natural, widest));
        }
        // Emit.
        let mut yy = y;
        let nlines = lines.len();
        for (li, mut line) in lines.into_iter().enumerate() {
            while line.last().is_some_and(|p| p.kind == 1) {
                line.pop();
            }
            if line.is_empty() && li + 1 == nlines && li > 0 {
                break;
            }
            let lh_factor = line
                .iter()
                .filter_map(|p| p.st.as_ref())
                .map(|s| s.lh.abs())
                .fold(0.0f32, f32::max);
            let lh_factor = if lh_factor <= 0.0 {
                toks.iter()
                    .find_map(|t| match t {
                        Tok::Word(_, s) | Tok::Space(s) => Some(s.lh.abs()),
                        _ => None,
                    })
                    .unwrap_or(1.25)
            } else {
                lh_factor
            };
            let mut asc = line.iter().map(|p| p.asc).max().unwrap_or(0);
            let mut desc = line.iter().map(|p| p.desc).max().unwrap_or(0);
            if asc == 0 && desc == 0 {
                let st = toks.iter().find_map(|t| match t {
                    Tok::Word(_, s) | Tok::Space(s) => Some(s.clone()),
                    _ => None,
                });
                let (a, d) = match st {
                    Some(s) => self.metrics(&s),
                    None => {
                        let m = self.c.fonts.metrics(SANS, self.c.px);
                        (m.ascent, m.descent)
                    }
                };
                asc = a;
                desc = d;
            }
            let text_h = asc + desc;
            let has_img = line.iter().any(|p| p.kind == 2 || p.kind == 3);
            let h = if has_img {
                text_h + 2
            } else {
                (text_h as f32 * lh_factor).round() as i32
            };
            let base = yy + (h - text_h) / 2 + asc;
            let total: f32 = line.iter().map(|p| p.w).sum();
            let mut px = x as f32
                + match align {
                    "center" => ((w as f32 - total) / 2.0).max(0.0),
                    "right" => (w as f32 - total).max(0.0),
                    _ => 0.0,
                };
            for p in line {
                match p.kind {
                    4 => {
                        self.out.anchors.entry(p.text).or_insert(yy);
                    }
                    2 => {
                        let r = Rect::new(px.round() as i32, base - p.asc, p.w as i32, p.asc);
                        self.out.items.push(Item::Image { r, key: p.key });
                        self.out.flat.push('\u{fffc}');
                        px += p.w;
                    }
                    3 => {
                        let r = Rect::new(px.round() as i32, base - p.asc, p.w as i32, p.asc);
                        self.out.items.push(Item::Placeholder { r, label: p.text });
                        px += p.w;
                    }
                    _ => {
                        let st = p.st.unwrap();
                        let shift = match st.shift {
                            Shift::Sup => -(st.px * 0.4) as i32,
                            Shift::Sub => (st.px * 0.25) as i32,
                            Shift::None => 0,
                        };
                        // Join with the previous run when the style matches.
                        if let Some(Item::Text(r)) = self.out.items.last_mut() {
                            if r.top == yy
                                && r.face == st.face
                                && r.px == st.px
                                && r.color == st.color
                                && r.bg == st.bg
                                && r.under == st.under
                                && r.strike == st.strike
                                && r.link == st.link
                                && r.base == base + shift
                                && (r.x + r.w - px).abs() < 0.5
                            {
                                r.chars += p.text.chars().count();
                                r.w += p.w;
                                self.out.flat.extend(p.text.chars());
                                px += p.w;
                                continue;
                            }
                        }
                        let flat = self.out.flat.len();
                        self.out.flat.extend(p.text.chars());
                        self.out.items.push(Item::Text(Run {
                            x: px,
                            top: yy,
                            h,
                            base: base + shift,
                            chars: p.text.chars().count(),
                            face: st.face,
                            px: st.px,
                            color: st.color,
                            bg: st.bg,
                            under: st.under,
                            strike: st.strike,
                            link: st.link,
                            flat,
                            w: p.w,
                        }));
                        px += p.w;
                    }
                }
            }
            self.out.flat.push('\n');
            yy += h;
        }
        (yy - y, (natural, widest))
    }

    fn is_inline(n: &Node) -> bool {
        match n {
            Node::Text(_) => true,
            Node::El { tag, .. } => !matches!(
                tag.as_str(),
                "p" | "h1"
                    | "h2"
                    | "h3"
                    | "h4"
                    | "h5"
                    | "h6"
                    | "pre"
                    | "ul"
                    | "ol"
                    | "li"
                    | "table"
                    | "tr"
                    | "td"
                    | "th"
                    | "hr"
            ),
        }
    }

    /// Lay out block content; `y` advances; `pending` is the bottom margin
    /// waiting to collapse with the next block's top margin.
    fn blocks(&mut self, nodes: &[Node], x: i32, w: i32, st: &St, y: &mut i32, pending: &mut i32) {
        let mut i = 0;
        while i < nodes.len() {
            if Self::is_inline(&nodes[i]) {
                let mut j = i;
                while j < nodes.len() && Self::is_inline(&nodes[j]) {
                    j += 1;
                }
                let only_space = nodes[i..j]
                    .iter()
                    .all(|n| matches!(n, Node::Text(t) if t.trim().is_empty()));
                if !only_space {
                    let mut toks = Vec::new();
                    self.tokens(&nodes[i..j], st, &mut toks);
                    *y += *pending;
                    *pending = 0;
                    let (h, _) = self.inline(&toks, x, *y, w, "left", false);
                    *y += h;
                }
                i = j;
                continue;
            }
            self.block(&nodes[i], x, w, st, y, pending);
            i += 1;
        }
    }

    fn margin(&self, y: &mut i32, pending: &mut i32, top: i32) {
        *y += (*pending).max(self.s(top));
        *pending = 0;
    }

    fn block(&mut self, n: &Node, x: i32, w: i32, st: &St, y: &mut i32, pending: &mut i32) {
        let pal = *self.c.pal;
        let pc = preview_colors(&pal);
        let tag = n.tag().to_string();
        match tag.as_str() {
            "p" => {
                let cls = n.class().to_string();
                let mut s = st.clone();
                let (mt, mb) = if cls == "end" {
                    (0, 0)
                } else if cls == "lang" {
                    s.px = st.px * 0.72;
                    s.color = pal.muted;
                    (0, 4)
                } else if let Some(kind) = cls.strip_prefix("at-") {
                    s.color = alert_color(&pal, kind);
                    (0, 4)
                } else if st.lh < 0.0 {
                    // Inside a list item: li p
                    (0, 8)
                } else {
                    (0, 12)
                };
                s.lh = 1.25;
                self.margin(y, pending, mt);
                let mut toks = Vec::new();
                self.tokens(n.kids(), &s, &mut toks);
                let (h, _) = self.inline(&toks, x, *y, w, "left", false);
                *y += h;
                *pending = self.s(mb);
            }
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                let level = tag.as_bytes()[1] - b'0';
                let (em, mt, mb) = heading_style(level);
                let mut s = st.clone();
                s.px = self.c.px * em;
                s.face = SANS | BOLD;
                s.color = if level == 6 { pal.muted } else { pal.fg };
                s.lh = 1.15;
                self.margin(y, pending, mt);
                let mut toks = Vec::new();
                self.tokens(n.kids(), &s, &mut toks);
                let (h, _) = self.inline(&toks, x, *y, w, "left", false);
                *y += h;
                *pending = self.s(mb);
            }
            "pre" => {
                let mut s = st.clone();
                s.face = MONO;
                s.px = self.c.px * 0.88;
                s.pre = true;
                s.lh = 1.25;
                self.margin(y, pending, 0);
                let mut toks = Vec::new();
                self.tokens(n.kids(), &s, &mut toks);
                let (h, _) = self.inline(&toks, x, *y, w, "left", false);
                *y += h;
            }
            "hr" => {
                self.margin(y, pending, 10);
                self.out
                    .items
                    .push(Item::Fill(Rect::new(x, *y, w, 1), pal.border));
                *y += 1;
                self.out.flat.push('\n');
                *pending = self.s(16);
            }
            "ul" | "ol" => {
                let ordered = tag == "ol";
                let style = n
                    .attr("style")
                    .and_then(|s| s.strip_prefix("list-style-type:"))
                    .unwrap_or(if ordered { "decimal" } else { "disc" })
                    .to_string();
                let mut s = st.clone();
                if n.class() == "footnotes" {
                    s.px *= 0.88;
                    s.color = pal.muted;
                }
                let start: i64 = n.attr("start").and_then(|v| v.parse().ok()).unwrap_or(1);
                let indent = self.s(40);
                let items: Vec<&Node> = n.kids().iter().filter(|k| k.tag() == "li").collect();
                for (k, li) in items.iter().enumerate() {
                    let last = li.class() == "last";
                    self.margin(y, pending, 0);
                    let marker = match style.as_str() {
                        "none" => String::new(),
                        "circle" => "◦".into(),
                        "square" => "▪".into(),
                        "decimal" => format!("{}.", start + k as i64),
                        _ => "•".into(),
                    };
                    let item_y = *y;
                    let mut ls = s.clone();
                    ls.lh = -1.25; // "inside a list item" for the p margins
                    let mut inner_pending = 0;
                    let mut yy = *y;
                    let first_item = self.out.items.len();
                    self.blocks(
                        li.kids(),
                        x + indent,
                        w - indent,
                        &ls,
                        &mut yy,
                        &mut inner_pending,
                    );
                    if !marker.is_empty() {
                        let first_run =
                            self.out.items[first_item..].iter().find_map(|it| match it {
                                Item::Text(r) => Some((r.top, r.h, r.base)),
                                _ => None,
                            });
                        let (top, h, base) = first_run.unwrap_or_else(|| {
                            let m = self.c.fonts.metrics(s.face, s.px);
                            let th = m.ascent + m.descent;
                            let h = (th as f32 * 1.25).round() as i32;
                            (item_y, h, item_y + (h - th) / 2 + m.ascent)
                        });
                        let mw = self.c.fonts.width(s.face, s.px, &marker);
                        let mx = (x + indent) as f32 - mw - self.s(6) as f32;
                        let _ = (top, h);
                        self.out.items.push(Item::Marker {
                            x: mx,
                            base,
                            text: marker.clone(),
                            face: s.face,
                            px: s.px,
                            color: s.color,
                        });
                    }
                    *y = yy.max(item_y + 1);
                    *pending = self.s(if last { 12 } else { 3 });
                }
            }
            "table" => {
                let cls = n.class().to_string();
                if cls == "grid" {
                    self.margin(y, pending, 2);
                    self.grid(n, x, w, st, y);
                    *pending = self.s(14);
                    return;
                }
                // A one-cell box: code, quote or alert.
                let td = n
                    .kids()
                    .iter()
                    .find(|k| k.tag() == "tr")
                    .and_then(|tr| tr.kids().iter().find(|k| k.tag() == "td"));
                let Some(td) = td else { return };
                self.margin(y, pending, 2);
                let mut s = st.clone();
                let (pt, pr, pb, pl, bg, bar) = match (cls.as_str(), td.class()) {
                    ("code", _) => (10, 14, 10, 14, Some(pc.code_bg), None),
                    ("quote", _) => {
                        s.color = pc.quote_fg;
                        (2, 0, 0, 14, None, Some(pal.border))
                    }
                    (_, c) if c.starts_with("a-") => {
                        let col = alert_color(&pal, &c[2..]);
                        (8, 14, 2, 14, Some(mix(pal.bg, col, 0.07)), Some(col))
                    }
                    _ => (0, 0, 0, 0, None, None),
                };
                let top = *y;
                let fill_at = self.out.items.len();
                let mut yy = top + self.s(pt);
                let mut inner_pending = 0;
                self.blocks(
                    td.kids(),
                    x + self.s(pl),
                    w - self.s(pl) - self.s(pr),
                    &s,
                    &mut yy,
                    &mut inner_pending,
                );
                let bottom = yy + self.s(pb);
                if let Some(bg) = bg {
                    self.out
                        .items
                        .insert(fill_at, Item::Fill(Rect::new(x, top, w, bottom - top), bg));
                }
                if let Some(bar) = bar {
                    // Over the background.
                    let at = fill_at + usize::from(bg.is_some());
                    self.out.items.insert(
                        at,
                        Item::Fill(Rect::new(x, top, self.s(3).max(2), bottom - top), bar),
                    );
                }
                *y = bottom;
                *pending = self.s(14);
            }
            _ => {
                // Unknown block: its children.
                self.blocks(n.kids(), x, w, st, y, pending);
            }
        }
    }

    fn grid(&mut self, n: &Node, x: i32, w: i32, st: &St, y: &mut i32) {
        let pal = *self.c.pal;
        let pc = preview_colors(&pal);
        let rows: Vec<&Node> = n.kids().iter().filter(|k| k.tag() == "tr").collect();
        let ncols = rows.iter().map(|r| r.kids().len()).max().unwrap_or(0);
        if ncols == 0 {
            return;
        }
        let (pt, pr, pb, pl) = (self.s(4), self.s(10), self.s(3), self.s(10));
        let mut cell_st = st.clone();
        cell_st.lh = 1.15;
        // Measure every cell: (natural, widest word).
        let mut nat = vec![0.0f32; ncols];
        let mut min = vec![0.0f32; ncols];
        for r in &rows {
            for (c, cell) in r.kids().iter().enumerate() {
                let mut s = cell_st.clone();
                if cell.tag() == "th" {
                    s.face |= BOLD;
                }
                let mut toks = Vec::new();
                self.tokens(cell.kids(), &s, &mut toks);
                let (_, (nw, ww)) = self.inline(&toks, 0, 0, 100_000, "left", true);
                nat[c] = nat[c].max(nw);
                min[c] = min[c].max(ww);
            }
        }
        let pad = (pl + pr + 1) as f32;
        let avail = (w - 1) as f32;
        let total_nat: f32 = nat.iter().map(|v| v + pad).sum();
        let widths: Vec<f32> = if total_nat <= avail {
            nat.iter().map(|v| v + pad).collect()
        } else {
            let mins: Vec<f32> = min
                .iter()
                .zip(&nat)
                .map(|(m, n)| m.min(*n).min(avail / ncols as f32 * 2.0) + pad)
                .collect();
            let total_min: f32 = mins.iter().sum();
            if total_min >= avail {
                mins
            } else {
                let extra = avail - total_min;
                let want: f32 = nat
                    .iter()
                    .zip(&mins)
                    .map(|(n, m)| (n + pad - m).max(0.0))
                    .sum::<f32>()
                    .max(1.0);
                nat.iter()
                    .zip(&mins)
                    .map(|(n, m)| m + extra * (n + pad - m).max(0.0) / want)
                    .collect()
            }
        };
        let widths: Vec<i32> = widths.iter().map(|v| v.ceil() as i32 + 1).collect();
        let table_w: i32 = widths.iter().sum::<i32>() + 1;
        let mut yy = *y;
        let start_y = yy;
        for r in &rows {
            let odd = r.class() == "odd";
            let row_top = yy;
            let fill_at = self.out.items.len();
            let mut row_h = 0;
            let mut cx = x;
            for (c, cell) in r.kids().iter().enumerate() {
                let cw = widths.get(c).copied().unwrap_or(40);
                let mut s = cell_st.clone();
                if cell.tag() == "th" {
                    s.face |= BOLD;
                }
                let align = cell.attr("align").unwrap_or("left").to_string();
                let mut toks = Vec::new();
                self.tokens(cell.kids(), &s, &mut toks);
                let (h, _) = self.inline(
                    &toks,
                    cx + 1 + pl,
                    row_top + 1 + pt,
                    cw - pl - pr - 1,
                    &align,
                    false,
                );
                row_h = row_h.max(h);
                cx += cw;
            }
            let rh = row_h + pt + pb + 1;
            // Backgrounds and borders under the cell text.
            let mut fills = Vec::new();
            let mut cx = x;
            for (c, cell) in r.kids().iter().enumerate() {
                let cw = widths.get(c).copied().unwrap_or(40);
                let bg = if cell.tag() == "th" {
                    Some(pc.code_bg)
                } else if odd {
                    Some(pc.odd)
                } else {
                    None
                };
                if let Some(bg) = bg {
                    fills.push(Item::Fill(
                        Rect::new(cx + 1, row_top + 1, cw - 1, rh - 1),
                        bg,
                    ));
                }
                fills.push(Item::Fill(Rect::new(cx, row_top, 1, rh + 1), pal.border));
                cx += cw;
            }
            fills.push(Item::Fill(Rect::new(cx, row_top, 1, rh + 1), pal.border));
            fills.push(Item::Fill(Rect::new(x, row_top, table_w, 1), pal.border));
            for (k, f) in fills.into_iter().enumerate() {
                self.out.items.insert(fill_at + k, f);
            }
            yy += rh;
        }
        self.out
            .items
            .push(Item::Fill(Rect::new(x, yy, table_w, 1), pal.border));
        *y = yy + 1;
        let _ = start_y;
    }
}

pub fn alert_color(p: &Palette, kind: &str) -> u32 {
    match kind {
        "note" => p.blue,
        "tip" => p.green,
        "important" => p.magenta,
        "warning" => p.yellow,
        "caution" => p.red,
        _ => p.border,
    }
}

/// Lay the document out for a page `w` pixels wide.
pub fn layout(html: &str, c: &mut Ctx, w: i32) -> Layout {
    let dom = parse_html(html);
    let base = St {
        face: SANS,
        px: c.px,
        color: c.pal.fg,
        bg: None,
        under: false,
        strike: false,
        link: None,
        shift: Shift::None,
        pre: false,
        lh: 1.25,
    };
    let mut b = Builder {
        c,
        out: Layout::default(),
    };
    let mut y = 0;
    let mut pending = 0;
    b.blocks(&dom, 0, w, &base, &mut y, &mut pending);
    b.out.height = y + pending;
    b.out
}

// ---------------------------------------------------------------- view

pub struct Preview {
    html: String,
    pub headings: Vec<crate::markdown::Heading>,
    pub layout: Layout,
    laid_for: Option<(i32, u32, u64)>,
    pub source_key: Option<u64>,
    pub scroll_y: i32,
    pub view_h: i32,
    pub sel: (usize, usize),
    pub hit: Option<(usize, usize)>,
    pub theme_epoch: u64,
}

impl Preview {
    pub fn new() -> Self {
        Preview {
            html: String::new(),
            headings: Vec::new(),
            layout: Layout::default(),
            laid_for: None,
            source_key: None,
            scroll_y: 0,
            view_h: 400,
            sel: (0, 0),
            hit: None,
            theme_epoch: 0,
        }
    }

    pub fn set_html(&mut self, html: &str, headings: Vec<crate::markdown::Heading>, key: u64) {
        self.html = html.to_string();
        self.headings = headings;
        self.source_key = Some(key);
        self.laid_for = None;
        self.sel = (0, 0);
        self.hit = None;
    }

    pub fn invalidate(&mut self) {
        self.laid_for = None;
    }

    pub fn ensure_layout(&mut self, c: &mut Ctx, w: i32) -> bool {
        let key = (w, c.px.to_bits(), self.theme_epoch);
        if self.laid_for == Some(key) {
            return false;
        }
        self.layout = Layout::default();
        self.layout = layout(&self.html, c, w);
        self.layout.items.shrink_to_fit();
        self.layout.flat.shrink_to_fit();
        // Give the layout's scratch memory back (big documents).
        #[cfg(target_env = "gnu")]
        unsafe {
            libc::malloc_trim(0);
        }
        self.laid_for = Some(key);
        self.clamp();
        true
    }

    pub fn max_scroll(&self) -> i32 {
        (self.layout.height - self.view_h + 30).max(0)
    }

    pub fn clamp(&mut self) {
        self.scroll_y = self.scroll_y.clamp(0, self.max_scroll());
    }

    pub fn scroll_by(&mut self, dy: i32) {
        self.scroll_y += dy;
        self.clamp();
    }

    pub fn anchor_y(&self, name: &str) -> Option<i32> {
        self.layout.anchors.get(name).copied()
    }

    pub fn selected_text(&self) -> String {
        let (a, b) = (self.sel.0.min(self.sel.1), self.sel.0.max(self.sel.1));
        let b = b.min(self.layout.flat.len());
        let a = a.min(b);
        self.layout.flat[a..b]
            .iter()
            .filter(|c| **c != '\u{fffc}')
            .collect::<String>()
            .trim_end_matches('\n')
            .to_string()
    }

    pub fn select_all(&mut self) {
        self.sel = (0, self.layout.flat.len());
    }

    /// The y of a plain-text offset (for find).
    pub fn y_of(&self, off: usize) -> Option<i32> {
        self.layout.items.iter().find_map(|it| match it {
            Item::Text(r) if r.chars > 0 && off >= r.flat && off < r.flat + r.chars => Some(r.top),
            _ => None,
        })
    }

    /// The plain-text offset under (x, y) in page coordinates.
    pub fn offset_at(&self, fonts: &mut Fonts, x: f32, y: i32) -> usize {
        let mut best: Option<(&Run, f32)> = None;
        for it in &self.layout.items {
            if let Item::Text(r) = it {
                if r.chars == 0 {
                    continue;
                }
                let dy = if y < r.top {
                    (r.top - y) as f32
                } else if y >= r.top + r.h {
                    (y - r.top - r.h + 1) as f32
                } else {
                    0.0
                };
                let dx = if x < r.x {
                    r.x - x
                } else if x > r.x + r.w {
                    x - r.x - r.w
                } else {
                    0.0
                };
                let d = dy * 1000.0 + dx;
                if best.is_none_or(|b| d < b.1) {
                    best = Some((r, d));
                }
            }
        }
        let Some((r, _)) = best else {
            return 0;
        };
        if y < r.top {
            return r.flat;
        }
        let mut cx = r.x;
        let text = &self.layout.flat[r.flat..r.flat + r.chars];
        for (i, &ch) in text.iter().enumerate() {
            let a = fonts.advance(r.face, r.px, ch);
            if x < cx + a / 2.0 {
                return r.flat + i;
            }
            cx += a;
        }
        r.flat + r.chars
    }

    pub fn link_at(&self, x: f32, y: i32) -> Option<String> {
        for it in &self.layout.items {
            if let Item::Text(r) = it {
                if let Some(l) = r.link {
                    if y >= r.top && y < r.top + r.h && x >= r.x && x < r.x + r.w {
                        return self.layout.links.get(l).cloned();
                    }
                }
            }
        }
        None
    }

    /// Find `q` from offset `from` (wrapping); returns the hit.
    pub fn find(
        &mut self,
        q: &str,
        from: usize,
        back: bool,
        case: bool,
        word: bool,
    ) -> Option<(usize, usize)> {
        let flat = &self.layout.flat;
        let norm = |c: char| -> char {
            if case {
                c
            } else {
                c.to_lowercase().next().unwrap_or(c)
            }
        };
        let needle: Vec<char> = q.chars().map(norm).collect();
        if needle.is_empty() || needle.len() > flat.len() {
            return None;
        }
        let hay: Vec<char> = flat.iter().map(|c| norm(*c)).collect();
        let ok = |i: usize| -> bool {
            if hay[i..i + needle.len()] != needle[..] {
                return false;
            }
            if word {
                let before = i == 0 || !crate::doc::is_word(hay[i - 1]);
                let after =
                    i + needle.len() >= hay.len() || !crate::doc::is_word(hay[i + needle.len()]);
                return before && after;
            }
            true
        };
        let last = hay.len() - needle.len();
        let found = if back {
            (0..from.min(last + 1)).rev().find(|i| ok(*i))
        } else {
            (from.min(last + 1)..=last).find(|i| ok(*i))
        };
        found.map(|i| (i, i + needle.len()))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &self,
        cv: &mut Canvas,
        fonts: &mut Fonts,
        images: &mut Images,
        pal: &Palette,
        area: Rect,
        focus_ring: Option<usize>,
    ) {
        cv.set_clip(area);
        let (sa, sb) = (self.sel.0.min(self.sel.1), self.sel.0.max(self.sel.1));
        let oy = area.y - self.scroll_y;
        let ox = area.x;
        let top = self.scroll_y - 50;
        let bottom = self.scroll_y + area.h + 50;
        for it in &self.layout.items {
            match it {
                Item::Fill(r, c) => {
                    if r.y + r.h < top || r.y > bottom {
                        continue;
                    }
                    cv.fill(Rect::new(r.x + ox, r.y + oy, r.w, r.h), *c);
                }
                Item::Image { r, key } => {
                    if r.y + r.h < top || r.y > bottom {
                        continue;
                    }
                    if let Some(img) = images.scaled(key, r.w) {
                        let img: &Image = &img;
                        cv.image(r.x + ox, r.y + oy, img, 255);
                    }
                }
                Item::Placeholder { r, label } => {
                    if r.y + r.h < top || r.y > bottom {
                        continue;
                    }
                    let rr = Rect::new(r.x + ox, r.y + oy, r.w, r.h);
                    cv.frame(rr, pal.border);
                    let px = (fonts.metrics(SANS, 10.0).height as f32).max(10.0);
                    let lpx = (r.h as f32 - 10.0).max(8.0) * 0.8;
                    let _ = px;
                    let m = fonts.metrics(SANS, lpx);
                    let base = rr.y + (rr.h - m.ascent - m.descent) / 2 + m.ascent;
                    let save = (rr.x + 10, rr.y, rr.w - 14, rr.h);
                    cv.set_clip(Rect::new(save.0, save.1, save.2, save.3).intersect(&area));
                    fonts.draw(cv, SANS, lpx, (rr.x + 10) as f32, base, label, pal.muted);
                    cv.set_clip(area);
                }
                Item::Marker {
                    x,
                    base,
                    text,
                    face,
                    px,
                    color,
                } => {
                    if *base < top || *base > bottom + 50 {
                        continue;
                    }
                    fonts.draw(cv, *face, *px, x + ox as f32, base + oy, text, *color);
                }
                Item::Text(r) => {
                    if r.top + r.h < top || r.top > bottom {
                        continue;
                    }
                    let y = r.top + oy;
                    let x0 = r.x + ox as f32;
                    if let Some(bg) = r.bg {
                        cv.fill(
                            Rect::new(x0.round() as i32, y + 1, r.w.round() as i32, r.h - 2),
                            bg,
                        );
                    }
                    let sel = r.chars > 0 && sa < r.flat + r.chars && sb > r.flat;
                    let hit = self
                        .hit
                        .filter(|(a, b)| r.chars > 0 && *a < r.flat + r.chars && *b > r.flat);
                    if sel || hit.is_some() {
                        let mut cx = x0;
                        for (i, &ch) in self.layout.flat[r.flat..r.flat + r.chars]
                            .iter()
                            .enumerate()
                        {
                            let a = fonts.advance(r.face, r.px, ch);
                            let off = r.flat + i;
                            let in_sel = off >= sa && off < sb;
                            let in_hit = hit.is_some_and(|(ha, hb)| off >= ha && off < hb);
                            if in_sel || in_hit {
                                cv.fill(
                                    Rect::new(
                                        cx.round() as i32,
                                        y,
                                        (cx + a).round() as i32 - cx.round() as i32,
                                        r.h,
                                    ),
                                    pal.select,
                                );
                            }
                            cx += a;
                        }
                    }
                    let base = r.base + oy;
                    let mut pen = x0;
                    for &ch in &self.layout.flat[r.flat..r.flat + r.chars] {
                        pen += fonts.draw_char(cv, r.face, r.px, pen, base, ch, r.color);
                    }
                    if r.under {
                        cv.fill(
                            Rect::new(x0.round() as i32, base + 2, r.w.round() as i32, 1),
                            r.color,
                        );
                    }
                    if r.strike {
                        let m = fonts.metrics(r.face, r.px);
                        cv.fill(
                            Rect::new(
                                x0.round() as i32,
                                base - m.ascent / 3,
                                r.w.round() as i32,
                                1,
                            ),
                            r.color,
                        );
                    }
                    if focus_ring.is_some() && r.link == focus_ring {
                        cv.frame(
                            Rect::new(x0.round() as i32 - 2, y, r.w.round() as i32 + 4, r.h),
                            pal.accent,
                        );
                    }
                }
            }
        }
        cv.reset_clip();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_parser_builds_a_tree_and_survives_stray_tags() {
        let dom = parse_html(
            "<p>a <b>b</p><ul style=\"list-style-type:disc\"><li>x</li></ul></i>t &amp; u",
        );
        assert_eq!(dom.len(), 3, "{dom:?}");
        assert_eq!(dom[0].tag(), "p");
        assert_eq!(dom[1].attr("style"), Some("list-style-type:disc"));
        assert!(matches!(&dom[2], Node::Text(t) if t == "t & u"));
    }
}

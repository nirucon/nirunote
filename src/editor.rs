//! The source editor: wrapping, the Markdown highlighting of NIRUNOTE 0.7
//! (muted syntax marks, bold headings in the accent colour and a larger
//! size, fenced code on the panel colour, spelling underlines), selection,
//! the caret and the editing commands, including the list and quote
//! continuation on Enter and list indenting with Tab.
use crate::canvas::{Canvas, Rect};
use crate::doc::{Doc, Kind as EditKind, Pos};
use crate::fonts::{Fonts, BOLD, ITALIC, MONO};
use crate::theme::Palette;
use fancy_regex::Regex;
use std::sync::OnceLock;

macro_rules! re {
    ($name:ident, $pat:expr) => {
        fn $name() -> &'static Regex {
            static R: OnceLock<Regex> = OnceLock::new();
            R.get_or_init(|| Regex::new($pat).unwrap())
        }
    };
}

re!(heading_re, r"^(#{1,6})(\s+)(.*)$");
re!(strong_re, r"\*\*([^*]+)\*\*");
re!(em_re, r"(?<!\*)\*([^*]+)\*(?!\*)");
re!(code_re, r"`([^`]+)`");
re!(list_re, r"^\s*([-*+]\s+|\d+[.)]\s+)");
re!(task_re, r"^\s*[-*+]\s+(\[[ xX]\])");
re!(quote_re, r"^(>\s?).*$");
re!(link_re, r"\[([^\]]+)\]\(([^)]+)\)");
re!(word_re, r"\b[^\W\d_][^\W\d_'-]*\b");
re!(spell_skip_re, r"`[^`]*`|https?://\S+|www\.\S+");
re!(
    list_item_re,
    r"^\s*(?:[-*+]\s+(?:\[[ xX]\]\s+)?|\d+[.)]\s+)"
);
re!(
    continue_re,
    r"^(\s*)([-*+]\s+\[(?: |x|X)\]\s+|[-*+]\s+|\d+[.)]\s+)(.*)$"
);
re!(quote_continue_re, r"^(\s*>\s?)(.*)$");
re!(number_re, r"^\d+");

pub const LARGE_DOCUMENT_CHARS: usize = 50000;
const TAB: f32 = 32.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LineKind {
    Text,
    Heading(u8),
    Fence,
    Code,
}

#[derive(Clone, Default)]
struct Lay {
    breaks: Vec<usize>,
    h: i32,
    vh: i32,
}

/// Styles of the highlighter, indexes into `styles()`.
const S_NORMAL: u8 = 0;
const S_MD: u8 = 1;
const S_MD_BOLD: u8 = 2;
const S_BOLD: u8 = 3;
const S_ITALIC: u8 = 4;
const S_ACCENT: u8 = 5;
const S_ACCENT_BOLD: u8 = 6;
const S_QUOTE: u8 = 7;
const S_HEADING: u8 = 8;
const S_FENCE: u8 = 9;
const S_CODE: u8 = 10;
const S_SPELL: u8 = 11;

#[derive(Clone, Copy)]
struct Style {
    fg: u32,
    face: u8,
    bg: Option<u32>,
    wavy: bool,
}

fn styles(p: &Palette) -> [Style; 12] {
    let s = |fg, face| Style {
        fg,
        face,
        bg: None,
        wavy: false,
    };
    [
        s(p.fg, MONO),
        s(p.md, MONO),
        s(p.md, MONO | BOLD),
        s(p.fg, MONO | BOLD),
        s(p.fg, MONO | ITALIC),
        s(p.accent, MONO),
        s(p.accent, MONO | BOLD),
        s(p.muted, MONO | ITALIC),
        s(p.accent, MONO | BOLD),
        Style {
            bg: Some(p.panel),
            ..s(p.md, MONO)
        },
        Style {
            bg: Some(p.panel),
            ..s(p.fg, MONO)
        },
        Style {
            wavy: true,
            ..s(p.fg, MONO)
        },
    ]
}

/// What the spell checker knows: Some(false) for a misspelled word.
pub trait SpellLookup {
    fn lookup(&self, word: &str) -> Option<bool>;
}

pub struct Editor {
    pub doc: Doc,
    pub cursor: Pos,
    pub anchor: Pos,
    goal_x: Option<f32>,
    pub scroll_y: i32,
    pub scroll_x: f32,
    /// Points of the editor font and pixels per point.
    pub pt: f32,
    pub scale: f32,
    pub wrap: bool,
    pub width: i32,
    pub view_h: i32,
    pub read_only: bool,
    kinds: Vec<LineKind>,
    lays: Vec<Lay>,
    tops: Vec<i32>,
    laid_for: (i32, u32, bool),
    pub total_h: i32,
}

pub fn is_fence(line: &str) -> bool {
    line.starts_with("```") || line.starts_with("~~~")
}

impl Editor {
    pub fn new(text: &str) -> Self {
        Editor {
            doc: Doc::new(text),
            cursor: Pos::default(),
            anchor: Pos::default(),
            goal_x: None,
            scroll_y: 0,
            scroll_x: 0.0,
            pt: 17.0,
            scale: 96.0 / 72.0,
            wrap: true,
            width: 600,
            view_h: 600,
            read_only: false,
            kinds: Vec::new(),
            lays: Vec::new(),
            tops: vec![0],
            laid_for: (0, 0, true),
            total_h: 0,
        }
    }

    pub fn set_text(&mut self, text: &str) {
        self.doc = Doc::new(text);
        self.cursor = Pos::default();
        self.anchor = Pos::default();
        self.goal_x = None;
        self.scroll_y = 0;
        self.scroll_x = 0.0;
        self.lays.clear();
        self.kinds.clear();
        self.laid_for = (0, 0, true);
    }

    pub fn px(&self) -> f32 {
        self.pt * self.scale
    }

    fn heading_px(&self, level: u8) -> f32 {
        let bump = match level {
            1 => 4.0,
            2 => 2.0,
            3 => 1.0,
            _ => 0.0,
        };
        (self.pt + bump).max(11.0) * self.scale
    }

    fn line_font(&self, k: LineKind) -> (u8, f32) {
        match k {
            LineKind::Heading(l) => (MONO | BOLD, self.heading_px(l)),
            _ => (MONO, self.px()),
        }
    }

    pub fn kind(&self, line: usize) -> LineKind {
        self.kinds.get(line).copied().unwrap_or(LineKind::Text)
    }

    fn compute_kinds(&self) -> Vec<LineKind> {
        let mut out = Vec::with_capacity(self.doc.lines.len());
        let mut inside = false;
        for l in &self.doc.lines {
            if is_fence(l) {
                out.push(LineKind::Fence);
                inside = !inside;
            } else if inside {
                out.push(LineKind::Code);
            } else if let Some(level) = heading_level(l) {
                out.push(LineKind::Heading(level));
            } else {
                out.push(LineKind::Text);
            }
        }
        out
    }

    fn lay_line(&self, fonts: &mut Fonts, i: usize) -> Lay {
        let (face, px) = self.line_font(self.kind(i));
        let vh = fonts.metrics(face, px).height.max(1);
        let text = &self.doc.lines[i];
        let mut breaks = Vec::new();
        let width = self.width.max(40) as f32;
        let mut x = 0.0f32;
        let mut start = 0usize;
        let mut last: Option<(usize, f32)> = None;
        for (b, c) in text.char_indices() {
            let a = if c == '\t' {
                ((x / TAB).floor() + 1.0) * TAB - x
            } else {
                fonts.advance(face, px, c)
            };
            if c == ' ' || c == '\t' {
                x += a;
                last = Some((b + c.len_utf8(), x));
                continue;
            }
            if self.wrap && x + a > width && b > start {
                match last.filter(|(lb, _)| *lb > start) {
                    Some((lb, lx)) => {
                        breaks.push(lb);
                        x -= lx;
                        start = lb;
                    }
                    None => {
                        breaks.push(b);
                        x = 0.0;
                        start = b;
                    }
                }
                last = None;
            }
            x += a;
        }
        Lay {
            h: vh * (breaks.len() as i32 + 1),
            vh,
            breaks,
        }
    }

    /// Bring the layout up to date with the text, the width and the size.
    pub fn sync(&mut self, fonts: &mut Fonts) {
        let key = (self.width, self.px().to_bits(), self.wrap);
        let new_kinds = self.compute_kinds();
        let full = key != self.laid_for || self.lays.is_empty();
        if full {
            self.doc.splices.clear();
            self.kinds = new_kinds;
            self.lays = (0..self.doc.lines.len())
                .map(|i| self.lay_line(fonts, i))
                .collect();
            self.laid_for = key;
        } else {
            // Changed lines get an empty layout (vh 0) and are laid out anew.
            for s in std::mem::take(&mut self.doc.splices) {
                let start = s.line.min(self.lays.len());
                let end = (s.line + s.old).min(self.lays.len());
                self.lays
                    .splice(start..end, (0..s.new).map(|_| Lay::default()));
            }
            let old_kinds = std::mem::replace(&mut self.kinds, new_kinds);
            self.lays.resize(self.doc.lines.len(), Lay::default());
            for i in 0..self.lays.len() {
                // Kinds shift with inserted lines; a changed kind relays too.
                if self.lays[i].vh == 0 || old_kinds.get(i) != Some(&self.kinds[i]) {
                    self.lays[i] = self.lay_line(fonts, i);
                }
            }
        }
        self.tops.clear();
        let mut y = 0;
        for l in &self.lays {
            self.tops.push(y);
            y += l.h;
        }
        self.tops.push(y);
        self.total_h = y;
    }

    pub fn max_scroll(&self) -> i32 {
        (self.total_h - self.view_h + 8).max(0)
    }

    pub fn line_top(&self, line: usize) -> i32 {
        self.tops.get(line).copied().unwrap_or(self.total_h)
    }

    fn line_at_y(&self, y: i32) -> usize {
        let n = self.lays.len();
        if n == 0 {
            return 0;
        }
        match self.tops[..n].binary_search(&y) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        }
        .min(n - 1)
    }

    fn vline(&self, p: Pos) -> usize {
        let l = &self.lays[p.line];
        l.breaks.iter().take_while(|b| **b <= p.col).count()
    }

    fn vrange(&self, line: usize, v: usize) -> (usize, usize) {
        let l = &self.lays[line];
        let a = if v == 0 { 0 } else { l.breaks[v - 1] };
        let b = l
            .breaks
            .get(v)
            .copied()
            .unwrap_or(self.doc.lines[line].len());
        (a, b)
    }

    fn measure(&self, fonts: &mut Fonts, line: usize, from: usize, to: usize) -> f32 {
        let (face, px) = self.line_font(self.kind(line));
        let mut x = 0.0f32;
        for c in self.doc.lines[line][from..to].chars() {
            x += if c == '\t' {
                ((x / TAB).floor() + 1.0) * TAB - x
            } else {
                fonts.advance(face, px, c)
            };
        }
        x
    }

    /// Top-left of the caret position in content coordinates and its height.
    pub fn caret_xy(&self, fonts: &mut Fonts, p: Pos) -> (f32, i32, i32) {
        if self.lays.is_empty() {
            return (0.0, 0, 20);
        }
        let v = self.vline(p);
        let (a, _) = self.vrange(p.line, v);
        let x = self.measure(fonts, p.line, a, p.col);
        let l = &self.lays[p.line];
        (x, self.tops[p.line] + v as i32 * l.vh, l.vh)
    }

    /// The text position under content coordinates (x, y).
    pub fn pos_at(&self, fonts: &mut Fonts, x: f32, y: i32) -> Pos {
        if self.lays.is_empty() {
            return Pos::default();
        }
        if y < 0 {
            return Pos::new(0, 0);
        }
        if y >= self.total_h {
            return self.doc.end();
        }
        let line = self.line_at_y(y);
        let l = &self.lays[line];
        let v = (((y - self.tops[line]) / l.vh.max(1)) as usize).min(l.breaks.len());
        self.pos_in_vline(fonts, line, v, x)
    }

    fn pos_in_vline(&self, fonts: &mut Fonts, line: usize, v: usize, x: f32) -> Pos {
        let (a, b) = self.vrange(line, v);
        let (face, px) = self.line_font(self.kind(line));
        let s = &self.doc.lines[line];
        let mut cx = 0.0f32;
        // Up to a wrap point the caret stays before the break.
        let end_limit = if v < self.lays[line].breaks.len() {
            s[..b].char_indices().last().map_or(b, |(i, _)| i)
        } else {
            b
        };
        for (i, c) in s[a..b].char_indices() {
            let w = if c == '\t' {
                ((cx / TAB).floor() + 1.0) * TAB - cx
            } else {
                fonts.advance(face, px, c)
            };
            if x < cx + w / 2.0 {
                return Pos::new(line, a + i);
            }
            cx += w;
        }
        Pos::new(line, end_limit.max(a))
    }

    pub fn has_selection(&self) -> bool {
        self.cursor != self.anchor
    }

    pub fn selection(&self) -> (Pos, Pos) {
        if self.cursor <= self.anchor {
            (self.cursor, self.anchor)
        } else {
            (self.anchor, self.cursor)
        }
    }

    pub fn selected_text(&self) -> String {
        let (a, b) = self.selection();
        self.doc.slice(a, b)
    }

    pub fn set_cursor(&mut self, p: Pos, extend: bool) {
        let p = self.doc.clamp(p);
        self.cursor = p;
        if !extend {
            self.anchor = p;
        }
        self.doc.break_merge();
    }

    pub fn select(&mut self, anchor: Pos, cursor: Pos) {
        self.anchor = self.doc.clamp(anchor);
        self.cursor = self.doc.clamp(cursor);
        self.doc.break_merge();
    }

    pub fn select_all(&mut self) {
        self.anchor = Pos::default();
        self.cursor = self.doc.end();
        self.goal_x = None;
        self.doc.break_merge();
    }

    /// Scroll so the caret is inside the view (and centred in typewriter
    /// mode).
    pub fn ensure_visible(&mut self, fonts: &mut Fonts, center: bool, view_w: i32) {
        let (x, y, h) = self.caret_xy(fonts, self.cursor);
        if center {
            self.scroll_y = (y + h / 2 - self.view_h / 2).clamp(0, self.max_scroll().max(0));
        } else if y < self.scroll_y {
            self.scroll_y = y;
        } else if y + h > self.scroll_y + self.view_h {
            self.scroll_y = y + h - self.view_h;
        }
        self.scroll_y = self.scroll_y.clamp(0, self.max_scroll());
        if !self.wrap {
            let vw = view_w.max(40) as f32;
            if x < self.scroll_x {
                self.scroll_x = (x - vw / 3.0).max(0.0);
            } else if x > self.scroll_x + vw - 4.0 {
                self.scroll_x = x - vw + vw / 3.0;
            }
        } else {
            self.scroll_x = 0.0;
        }
    }

    pub fn scroll_by(&mut self, dy: i32) {
        self.scroll_y = (self.scroll_y + dy).clamp(0, self.max_scroll());
    }

    pub fn line_height(&self, fonts: &mut Fonts) -> i32 {
        fonts.metrics(MONO, self.px()).height.max(1)
    }

    // ------------------------------------------------------------ motion

    pub fn move_h(&mut self, right: bool, word: bool, extend: bool) {
        self.goal_x = None;
        if self.has_selection() && !extend && !word {
            let (a, b) = self.selection();
            self.set_cursor(if right { b } else { a }, false);
            return;
        }
        let p = match (right, word) {
            (true, false) => self.doc.next(self.cursor),
            (false, false) => self.doc.prev(self.cursor),
            (true, true) => self.doc.word_right(self.cursor),
            (false, true) => self.doc.word_left(self.cursor),
        };
        self.set_cursor(p, extend);
    }

    /// Up and down by visual lines, keeping the column in pixels.
    pub fn move_v(&mut self, fonts: &mut Fonts, lines: i32, extend: bool) {
        if self.lays.is_empty() {
            return;
        }
        let (x, _, _) = self.caret_xy(fonts, self.cursor);
        let goal = *self.goal_x.get_or_insert(x);
        let mut line = self.cursor.line;
        let mut v = self.vline(self.cursor) as i64;
        let mut n = lines;
        while n != 0 {
            if n > 0 {
                if (v as usize) < self.lays[line].breaks.len() {
                    v += 1;
                } else if line + 1 < self.lays.len() {
                    line += 1;
                    v = 0;
                } else {
                    let end = self.doc.end();
                    self.cursor = end;
                    if !extend {
                        self.anchor = end;
                    }
                    self.doc.break_merge();
                    return;
                }
                n -= 1;
            } else {
                if v > 0 {
                    v -= 1;
                } else if line > 0 {
                    line -= 1;
                    v = self.lays[line].breaks.len() as i64;
                } else {
                    self.cursor = Pos::default();
                    if !extend {
                        self.anchor = self.cursor;
                    }
                    self.doc.break_merge();
                    return;
                }
                n += 1;
            }
        }
        let p = self.pos_in_vline(fonts, line, v as usize, goal);
        self.cursor = p;
        if !extend {
            self.anchor = p;
        }
        self.doc.break_merge();
    }

    pub fn page(&mut self, fonts: &mut Fonts, down: bool, extend: bool) {
        let lh = self.line_height(fonts);
        let n = ((self.view_h / lh).max(2) - 1) * if down { 1 } else { -1 };
        self.move_v(fonts, n, extend);
        self.scroll_by(n * lh);
    }

    pub fn home(&mut self, extend: bool) {
        self.goal_x = None;
        let v = self.vline(self.cursor);
        let (a, _) = self.vrange(self.cursor.line, v);
        let p = Pos::new(self.cursor.line, a);
        self.set_cursor(p, extend);
    }

    pub fn end(&mut self, extend: bool) {
        self.goal_x = None;
        let v = self.vline(self.cursor);
        let line = self.cursor.line;
        let (_, b) = self.vrange(line, v);
        let s = &self.doc.lines[line];
        let col = if v < self.lays[line].breaks.len() {
            s[..b].char_indices().last().map_or(b, |(i, _)| i)
        } else {
            b
        };
        self.set_cursor(Pos::new(line, col), extend);
    }

    pub fn doc_start(&mut self, extend: bool) {
        self.goal_x = None;
        self.set_cursor(Pos::default(), extend);
    }

    pub fn doc_end(&mut self, extend: bool) {
        self.goal_x = None;
        let e = self.doc.end();
        self.set_cursor(e, extend);
    }

    // ------------------------------------------------------------ editing

    fn sel_state(&self) -> (Pos, Pos) {
        (self.cursor, self.anchor)
    }

    /// Replace `a..b` with `text` and put the caret after it.
    pub fn replace_range(&mut self, a: Pos, b: Pos, text: &str, kind: EditKind) -> Pos {
        let sel = self.sel_state();
        let end = self.doc.replace(a, b, text, kind, sel);
        self.cursor = end;
        self.anchor = end;
        self.goal_x = None;
        end
    }

    pub fn insert(&mut self, text: &str) {
        if self.read_only {
            return;
        }
        let (a, b) = self.selection();
        let kind = if text.chars().count() == 1 && a == b && text != "\n" {
            EditKind::Typing
        } else {
            EditKind::Other
        };
        if a != b {
            self.doc.break_merge();
        }
        self.replace_range(a, b, text, kind);
    }

    pub fn backspace(&mut self, word: bool) {
        if self.read_only {
            return;
        }
        if self.has_selection() {
            let (a, b) = self.selection();
            self.doc.break_merge();
            self.replace_range(a, b, "", EditKind::Other);
            return;
        }
        let from = if word {
            self.doc.word_left(self.cursor)
        } else {
            self.doc.prev(self.cursor)
        };
        if from == self.cursor {
            return;
        }
        let c = self.cursor;
        self.replace_range(from, c, "", EditKind::Deleting);
    }

    pub fn delete(&mut self, word: bool) {
        if self.read_only {
            return;
        }
        if self.has_selection() {
            let (a, b) = self.selection();
            self.doc.break_merge();
            self.replace_range(a, b, "", EditKind::Other);
            return;
        }
        let to = if word {
            self.doc.word_right(self.cursor)
        } else {
            self.doc.next(self.cursor)
        };
        if to == self.cursor {
            return;
        }
        let c = self.cursor;
        self.replace_range(c, to, "", EditKind::Deleting);
    }

    pub fn undo(&mut self) -> bool {
        if self.read_only {
            return false;
        }
        match self.doc.undo() {
            Some((c, a)) => {
                self.cursor = self.doc.clamp(c);
                self.anchor = self.doc.clamp(a);
                self.goal_x = None;
                true
            }
            None => false,
        }
    }

    pub fn redo(&mut self) -> bool {
        if self.read_only {
            return false;
        }
        match self.doc.redo() {
            Some((c, a)) => {
                self.cursor = self.doc.clamp(c);
                self.anchor = self.doc.clamp(a);
                self.goal_x = None;
                true
            }
            None => false,
        }
    }

    /// Enter: continue a Markdown list or quote, or end it on an empty item.
    pub fn newline(&mut self, plain: bool) {
        if self.read_only {
            return;
        }
        let line = self.doc.lines[self.cursor.line].clone();
        if !plain && !self.has_selection() {
            if let Ok(Some(m)) = continue_re().captures(&line) {
                let indent = m.get(1).map_or("", |x| x.as_str()).to_string();
                let token = m.get(2).map_or("", |x| x.as_str()).to_string();
                let body = m.get(3).map_or("", |x| x.as_str());
                if body.trim().is_empty() {
                    self.end_item();
                    return;
                }
                self.doc.break_merge();
                let mark = self.doc.undo_depth();
                self.insert("\n");
                let cont = if let Ok(Some(n)) = number_re().find(&token) {
                    let v: u64 = n.as_str().parse().unwrap_or(0) + 1;
                    let sep = if token.contains(')') { ')' } else { '.' };
                    format!("{indent}{v}{sep} ")
                } else if token.contains('[') {
                    format!("{indent}- [ ] ")
                } else {
                    format!("{indent}{token}")
                };
                self.insert(&cont);
                self.doc.squash_since(mark);
                return;
            }
            if let Ok(Some(m)) = quote_continue_re().captures(&line) {
                let prefix = m.get(1).map_or("", |x| x.as_str()).to_string();
                let body = m.get(2).map_or("", |x| x.as_str());
                if body.trim().is_empty() {
                    self.end_item();
                    return;
                }
                self.doc.break_merge();
                let mark = self.doc.undo_depth();
                self.insert("\n");
                self.insert(&prefix);
                self.doc.squash_since(mark);
                return;
            }
        }
        self.doc.break_merge();
        self.insert("\n");
    }

    /// An empty list item or quote line: remove the line, then a newline.
    fn end_item(&mut self) {
        let l = self.cursor.line;
        self.doc.break_merge();
        let mark = self.doc.undo_depth();
        if l > 0 {
            let a = Pos::new(l - 1, self.doc.lines[l - 1].len());
            let b = Pos::new(l, self.doc.lines[l].len());
            self.replace_range(a, b, "", EditKind::Other);
        } else {
            let b = Pos::new(0, self.doc.lines[0].len());
            self.replace_range(Pos::default(), b, "", EditKind::Other);
        }
        self.insert("\n");
        self.doc.squash_since(mark);
    }

    /// Tab on a list item indents it; elsewhere a tab character.
    pub fn tab(&mut self) {
        if self.read_only {
            return;
        }
        let line = &self.doc.lines[self.cursor.line];
        if !self.has_selection() && list_item_re().is_match(line).unwrap_or(false) {
            let l = self.cursor.line;
            let (c, a) = (self.cursor, self.anchor);
            self.doc.break_merge();
            self.doc.replace(
                Pos::new(l, 0),
                Pos::new(l, 0),
                "  ",
                EditKind::Other,
                (c, a),
            );
            self.cursor = Pos::new(l, c.col + 2);
            self.anchor = self.cursor;
            return;
        }
        self.insert("\t");
    }

    /// Shift+Tab removes two leading spaces.
    pub fn backtab(&mut self) {
        if self.read_only {
            return;
        }
        let l = self.cursor.line;
        if self.doc.lines[l].starts_with("  ") {
            let (c, a) = (self.cursor, self.anchor);
            self.doc.break_merge();
            self.doc
                .replace(Pos::new(l, 0), Pos::new(l, 2), "", EditKind::Other, (c, a));
            self.cursor = Pos::new(l, c.col.saturating_sub(2));
            self.anchor = Pos::new(
                a.line,
                if a.line == l {
                    a.col.saturating_sub(2)
                } else {
                    a.col
                },
            );
            self.cursor = self.doc.clamp(self.cursor);
            self.anchor = self.doc.clamp(self.anchor);
        }
    }

    // ------------------------------------------------------------ drawing

    /// Character styles of one line (one entry per byte).
    fn line_styles(&self, line: usize, spell: Option<&dyn SpellLookup>) -> Vec<u8> {
        let text = &self.doc.lines[line];
        let mut st = vec![S_NORMAL; text.len()];
        let set = |st: &mut Vec<u8>, a: usize, b: usize, s: u8| {
            for v in st[a.min(text.len())..b.min(text.len())].iter_mut() {
                *v = s;
            }
        };
        match self.kind(line) {
            LineKind::Fence => {
                set(&mut st, 0, text.len(), S_FENCE);
                return st;
            }
            LineKind::Code => {
                set(&mut st, 0, text.len(), S_CODE);
                return st;
            }
            _ => {}
        }
        if text.is_empty() {
            return st;
        }
        let first = text.trim_start().chars().next().unwrap_or(' ');
        if first == '#' {
            if let Ok(Some(m)) = heading_re().captures(text) {
                let g1 = m.get(1).unwrap();
                set(&mut st, g1.start(), g1.end(), S_MD);
                let g3 = m.get(3).unwrap();
                set(&mut st, g3.start(), g3.end(), S_HEADING);
            }
        }
        if text.contains('*') {
            for m in strong_re().captures_iter(text).flatten() {
                let (s, e) = (m.get(0).unwrap().start(), m.get(0).unwrap().end());
                let g = m.get(1).unwrap();
                set(&mut st, s, s + 2, S_MD);
                set(&mut st, g.start(), g.end(), S_BOLD);
                set(&mut st, e - 2, e, S_MD);
            }
            for m in em_re().captures_iter(text).flatten() {
                let (s, e) = (m.get(0).unwrap().start(), m.get(0).unwrap().end());
                let g = m.get(1).unwrap();
                set(&mut st, s, s + 1, S_MD);
                set(&mut st, g.start(), g.end(), S_ITALIC);
                set(&mut st, e - 1, e, S_MD);
            }
        }
        if text.contains('`') {
            for m in code_re().find_iter(text).flatten() {
                set(&mut st, m.start(), m.end(), S_ACCENT);
            }
        }
        if "-*+0123456789".contains(first) {
            if let Ok(Some(m)) = list_re().captures(text) {
                let g = m.get(1).unwrap();
                set(&mut st, g.start(), g.end(), S_MD_BOLD);
                if let Ok(Some(t)) = task_re().captures(text) {
                    let g = t.get(1).unwrap();
                    set(&mut st, g.start(), g.end(), S_ACCENT_BOLD);
                }
            }
        }
        if text.starts_with('>') {
            if let Ok(Some(m)) = quote_re().captures(text) {
                let g = m.get(1).unwrap();
                set(&mut st, 0, g.end(), S_MD);
                set(&mut st, g.end(), text.len(), S_QUOTE);
            }
        }
        if text.contains("](") {
            for m in link_re().find_iter(text).flatten() {
                set(&mut st, m.start(), m.end(), S_ACCENT);
            }
        }
        if let Some(sp) = spell {
            let skip: Vec<(usize, usize)> = spell_skip_re()
                .find_iter(text)
                .flatten()
                .map(|m| (m.start(), m.end()))
                .collect();
            for m in word_re().find_iter(text).flatten() {
                let (a, b) = (m.start(), m.end());
                if skip.iter().any(|(x, y)| *x <= a && b <= *y) {
                    continue;
                }
                if a > 0 && "#[]()!*_>".contains(text.as_bytes()[a - 1] as char) {
                    continue;
                }
                if sp.lookup(m.as_str()) == Some(false) {
                    set(&mut st, a, b, S_SPELL);
                }
            }
        }
        st
    }

    /// The words of the text the spell checker should know about.
    pub fn words(&self) -> std::collections::HashSet<String> {
        let mut out = std::collections::HashSet::new();
        for (i, l) in self.doc.lines.iter().enumerate() {
            if matches!(self.kind(i), LineKind::Code | LineKind::Fence) {
                continue;
            }
            for m in word_re().find_iter(l).flatten() {
                out.insert(m.as_str().to_string());
            }
        }
        out
    }

    /// The word under a position (for spelling suggestions).
    pub fn word_at(&self, p: Pos) -> Option<(Pos, Pos, String)> {
        let line = &self.doc.lines[p.line];
        for m in word_re().find_iter(line).flatten() {
            if m.start() <= p.col && p.col <= m.end() {
                return Some((
                    Pos::new(p.line, m.start()),
                    Pos::new(p.line, m.end()),
                    m.as_str().to_string(),
                ));
            }
        }
        None
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &self,
        cv: &mut Canvas,
        fonts: &mut Fonts,
        pal: &Palette,
        area: Rect,
        focused: bool,
        highlight_line: bool,
        spell: Option<&dyn SpellLookup>,
        find_hit: Option<(Pos, Pos)>,
    ) {
        if self.lays.is_empty() {
            return;
        }
        let sts = styles(pal);
        cv.set_clip(area);
        let (sa, sb) = self.selection();
        let first = self.line_at_y(self.scroll_y);
        let cur_v = (self.cursor.line, self.vline(self.cursor));
        let mut line = first;
        while line < self.lays.len() {
            let top = self.tops[line] - self.scroll_y + area.y;
            if top > area.y + area.h {
                break;
            }
            let lay = &self.lays[line];
            let (face0, px) = self.line_font(self.kind(line));
            let m = fonts.metrics(face0, px);
            let text = &self.doc.lines[line];
            let st = self.line_styles(line, spell);
            for v in 0..=lay.breaks.len() {
                let y = top + v as i32 * lay.vh;
                if y + lay.vh < area.y {
                    continue;
                }
                if y > area.y + area.h {
                    break;
                }
                if highlight_line && (line, v) == cur_v && !self.has_selection() {
                    cv.fill(Rect::new(area.x - 4, y, area.w + 8, lay.vh), pal.line);
                }
                let (a, b) = self.vrange(line, v);
                let base = y + (lay.vh - m.ascent - m.descent) / 2 + m.ascent;
                let mut x = area.x as f32 - self.scroll_x;
                let mut cx = 0.0f32;
                for (i, c) in text[a..b].char_indices() {
                    let bi = a + i;
                    let s = sts[st[bi] as usize];
                    let face = if matches!(self.kind(line), LineKind::Heading(_)) {
                        s.face | BOLD
                    } else {
                        s.face
                    };
                    let w = if c == '\t' {
                        ((cx / TAB).floor() + 1.0) * TAB - cx
                    } else {
                        fonts.advance(face0, px, c)
                    };
                    let p = Pos::new(line, bi);
                    let xi = x.round() as i32;
                    let wi = (x + w).round() as i32 - xi;
                    if let Some(bg) = s.bg {
                        cv.fill(Rect::new(xi, y, wi, lay.vh), bg);
                    }
                    if let Some((fa, fb)) = find_hit {
                        if p >= fa && p < fb && !(p >= sa && p < sb) {
                            cv.fill(Rect::new(xi, y, wi, lay.vh), pal.select);
                        }
                    }
                    if p >= sa && p < sb {
                        cv.fill(Rect::new(xi, y, wi, lay.vh), pal.select);
                    }
                    if c != '\t' && x + w >= area.x as f32 && x <= (area.x + area.w) as f32 {
                        fonts.draw_char(cv, face, px, x, base, c, s.fg);
                    }
                    if s.wavy {
                        wavy(cv, xi, base + 2, wi, pal.muted);
                    }
                    x += w;
                    cx += w;
                }
                // The line break of a selected line.
                let nl = Pos::new(line, b);
                if b == text.len() && nl >= sa && nl < sb && line + 1 < self.lays.len() {
                    cv.fill(
                        Rect::new(x.round() as i32, y, (px * 0.4) as i32 + 2, lay.vh),
                        pal.select,
                    );
                }
            }
            line += 1;
        }
        // The caret.
        if focused {
            let (x, y, h) = self.caret_xy(fonts, self.cursor);
            let cx = area.x + (x - self.scroll_x).round() as i32;
            let cy = area.y + y - self.scroll_y;
            if cy + h >= area.y && cy <= area.y + area.h {
                cv.fill(Rect::new(cx, cy + 1, 2, h - 2), pal.fg);
            }
        }
        cv.reset_clip();
    }
}

fn wavy(cv: &mut Canvas, x: i32, y: i32, w: i32, color: u32) {
    for i in 0..w.max(0) {
        let dy = match (i / 2) % 4 {
            0 => 0,
            1 | 3 => 1,
            _ => 2,
        };
        cv.fill(Rect::new(x + i, y + dy, 1, 1), color);
    }
}

pub fn heading_level(line: &str) -> Option<u8> {
    let hashes = line.bytes().take_while(|b| *b == b'#').count();
    if !(1..=6).contains(&hashes) {
        return None;
    }
    let rest = &line[hashes..];
    rest.chars()
        .next()
        .filter(|c| c.is_whitespace())
        .map(|_| hashes as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enter_continues_lists_and_ends_empty_items() {
        let mut e = Editor::new("- one");
        e.set_cursor(Pos::new(0, 5), false);
        e.newline(false);
        assert_eq!(e.doc.text(), "- one\n- ");
        e.newline(false);
        assert_eq!(e.doc.text(), "- one\n", "an empty item ends the list");
        let mut e = Editor::new("3. three");
        e.set_cursor(Pos::new(0, 8), false);
        e.newline(false);
        assert_eq!(e.doc.text(), "3. three\n4. ");
        let mut e = Editor::new("- [x] done");
        e.set_cursor(Pos::new(0, 10), false);
        e.newline(false);
        assert_eq!(e.doc.text(), "- [x] done\n- [ ] ");
        let mut e = Editor::new("> quoted");
        e.set_cursor(Pos::new(0, 8), false);
        e.newline(false);
        assert_eq!(e.doc.text(), "> quoted\n> ");
        e.undo();
        assert_eq!(e.doc.text(), "> quoted", "one undo step");
    }

    #[test]
    fn tab_indents_list_items_only() {
        let mut e = Editor::new("- item\nplain");
        e.set_cursor(Pos::new(0, 3), false);
        e.tab();
        assert_eq!(e.doc.lines[0], "  - item");
        assert_eq!(e.cursor, Pos::new(0, 5));
        e.backtab();
        assert_eq!(e.doc.lines[0], "- item");
        e.set_cursor(Pos::new(1, 0), false);
        e.tab();
        assert_eq!(e.doc.lines[1], "\tplain");
    }

    #[test]
    fn fences_and_headings_are_classified() {
        let e = Editor::new("# T\n```\n# not\n```\ntext");
        let k = e.compute_kinds();
        assert_eq!(
            k,
            vec![
                LineKind::Heading(1),
                LineKind::Fence,
                LineKind::Code,
                LineKind::Fence,
                LineKind::Text
            ]
        );
    }
}

//! Small widgets drawn on the canvas: a one-line text field, buttons, check
//! boxes, choices, number fields, lists and the drop-down menu.
use crate::canvas::{Canvas, Rect};
use crate::fonts::{Fonts, BOLD, MONO};
use crate::theme::Palette;

pub const UI: u8 = MONO;

/// Keys as the widgets see them.
#[derive(Clone, Debug, PartialEq)]
pub enum Key {
    Char(char),
    Enter,
    Esc,
    Tab,
    BackTab,
    Backspace,
    Delete,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    F(u8),
    Insert,
    Other,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

/// What a text field asks of the application.
#[derive(Debug, PartialEq)]
pub enum FieldOut {
    None,
    Changed,
    Copy(String),
    Paste,
    Unhandled,
}

#[derive(Clone, Default, Debug)]
pub struct LineEdit {
    pub text: String,
    pub cursor: usize,
    pub anchor: usize,
    pub scroll: f32,
    pub placeholder: String,
}

fn prev_b(s: &str, i: usize) -> usize {
    s[..i].char_indices().last().map_or(0, |c| c.0)
}

fn next_b(s: &str, i: usize) -> usize {
    s[i..].chars().next().map_or(i, |c| i + c.len_utf8())
}

fn word_l(s: &str, i: usize) -> usize {
    let mut j = i;
    while j > 0 && s[..j].ends_with(|c: char| c.is_whitespace()) {
        j = prev_b(s, j);
    }
    while j > 0 && !s[..j].ends_with(|c: char| c.is_whitespace()) {
        j = prev_b(s, j);
    }
    j
}

fn word_r(s: &str, i: usize) -> usize {
    let mut j = i;
    while j < s.len() && !s[j..].starts_with(|c: char| c.is_whitespace()) {
        j = next_b(s, j);
    }
    while j < s.len() && s[j..].starts_with(|c: char| c.is_whitespace()) {
        j = next_b(s, j);
    }
    j
}

impl LineEdit {
    pub fn new(text: &str, placeholder: &str) -> Self {
        LineEdit {
            text: text.to_string(),
            cursor: text.len(),
            anchor: 0,
            scroll: 0.0,
            placeholder: placeholder.to_string(),
        }
    }

    pub fn set(&mut self, text: &str) {
        self.text = text.to_string();
        self.cursor = self.text.len();
        self.anchor = self.cursor;
    }

    pub fn select_all(&mut self) {
        self.anchor = 0;
        self.cursor = self.text.len();
    }

    fn sel(&self) -> (usize, usize) {
        (self.cursor.min(self.anchor), self.cursor.max(self.anchor))
    }

    pub fn selected(&self) -> &str {
        let (a, b) = self.sel();
        &self.text[a..b]
    }

    pub fn insert(&mut self, s: &str) {
        let s: String = s.chars().filter(|c| *c != '\n' && *c != '\r').collect();
        let (a, b) = self.sel();
        self.text.replace_range(a..b, &s);
        self.cursor = a + s.len();
        self.anchor = self.cursor;
    }

    pub fn key(&mut self, k: &Key, m: Mods) -> FieldOut {
        let len = self.text.len();
        let mv = |e: &mut LineEdit, to: usize, shift: bool| {
            e.cursor = to;
            if !shift {
                e.anchor = to;
            }
        };
        match k {
            Key::Char(c) if m.ctrl => match c.to_ascii_lowercase() {
                'a' => {
                    self.select_all();
                    FieldOut::None
                }
                'c' => FieldOut::Copy(self.selected().to_string()),
                'x' => {
                    let s = self.selected().to_string();
                    self.insert("");
                    if s.is_empty() {
                        FieldOut::None
                    } else {
                        FieldOut::Copy(s)
                    }
                }
                'v' => FieldOut::Paste,
                _ => FieldOut::Unhandled,
            },
            Key::Char(c) => {
                self.insert(&c.to_string());
                FieldOut::Changed
            }
            Key::Backspace => {
                if self.cursor == self.anchor {
                    let to = if m.ctrl {
                        word_l(&self.text, self.cursor)
                    } else {
                        prev_b(&self.text, self.cursor)
                    };
                    self.anchor = to;
                }
                self.insert("");
                FieldOut::Changed
            }
            Key::Delete => {
                if self.cursor == self.anchor {
                    let to = if m.ctrl {
                        word_r(&self.text, self.cursor)
                    } else {
                        next_b(&self.text, self.cursor)
                    };
                    self.anchor = to;
                }
                self.insert("");
                FieldOut::Changed
            }
            Key::Left => {
                let to = if m.ctrl {
                    word_l(&self.text, self.cursor)
                } else if self.cursor != self.anchor && !m.shift {
                    self.sel().0
                } else {
                    prev_b(&self.text, self.cursor)
                };
                mv(self, to, m.shift);
                FieldOut::None
            }
            Key::Right => {
                let to = if m.ctrl {
                    word_r(&self.text, self.cursor)
                } else if self.cursor != self.anchor && !m.shift {
                    self.sel().1
                } else {
                    next_b(&self.text, self.cursor)
                };
                mv(self, to, m.shift);
                FieldOut::None
            }
            Key::Home => {
                mv(self, 0, m.shift);
                FieldOut::None
            }
            Key::End => {
                mv(self, len, m.shift);
                FieldOut::None
            }
            Key::Insert if m.shift => FieldOut::Paste,
            _ => FieldOut::Unhandled,
        }
    }

    /// Put the caret at pixel `x` inside the field's text area.
    pub fn click(&mut self, fonts: &mut Fonts, px: f32, x: f32, extend: bool) {
        let mut cx = -self.scroll;
        let mut to = self.text.len();
        for (i, c) in self.text.char_indices() {
            let a = fonts.advance(UI, px, c);
            if x < cx + a / 2.0 {
                to = i;
                break;
            }
            cx += a;
        }
        self.cursor = to;
        if !extend {
            self.anchor = to;
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        cv: &mut Canvas,
        fonts: &mut Fonts,
        p: &Palette,
        r: Rect,
        px: f32,
        focused: bool,
    ) {
        cv.fill(r, p.panel);
        cv.frame(r, if focused { p.muted } else { p.border });
        let inner = r.inset(6);
        let m = fonts.metrics(UI, px);
        let base = r.y + (r.h - m.ascent - m.descent) / 2 + m.ascent;
        let caret_x = fonts.width(UI, px, &self.text[..self.cursor]);
        let w = inner.w as f32;
        if caret_x - self.scroll > w - 2.0 {
            self.scroll = caret_x - w + 2.0;
        }
        if caret_x < self.scroll {
            self.scroll = caret_x;
        }
        cv.set_clip(inner);
        if self.text.is_empty() && !self.placeholder.is_empty() {
            fonts.draw(cv, UI, px, inner.x as f32, base, &self.placeholder, p.muted);
        }
        let (a, b) = self.sel();
        if a != b && focused {
            let xa = fonts.width(UI, px, &self.text[..a]) - self.scroll;
            let xb = fonts.width(UI, px, &self.text[..b]) - self.scroll;
            cv.fill(
                Rect::new(inner.x + xa as i32, r.y + 4, (xb - xa) as i32, r.h - 8),
                p.select,
            );
        }
        fonts.draw(
            cv,
            UI,
            px,
            inner.x as f32 - self.scroll,
            base,
            &self.text,
            p.fg,
        );
        if focused {
            let cx = inner.x + (caret_x - self.scroll) as i32;
            cv.fill(Rect::new(cx, r.y + 5, 1, r.h - 10), p.fg);
        }
        cv.reset_clip();
    }
}

/// Text centred vertically in `r`, cut with "…" to fit.
#[allow(clippy::too_many_arguments)]
pub fn label(
    cv: &mut Canvas,
    fonts: &mut Fonts,
    face: u8,
    px: f32,
    r: Rect,
    s: &str,
    color: u32,
    align: i32,
) -> i32 {
    let m = fonts.metrics(face, px);
    let base = r.y + (r.h - m.ascent - m.descent) / 2 + m.ascent;
    let t = fonts.ellipsize(face, px, s, r.w as f32);
    let w = fonts.width(face, px, &t);
    let x = match align {
        0 => r.x as f32,
        1 => r.x as f32 + (r.w as f32 - w) / 2.0,
        _ => (r.x + r.w) as f32 - w,
    };
    fonts.draw(cv, face, px, x, base, &t, color);
    w.ceil() as i32
}

#[allow(clippy::too_many_arguments)]
pub fn button(
    cv: &mut Canvas,
    fonts: &mut Fonts,
    p: &Palette,
    r: Rect,
    px: f32,
    text: &str,
    focused: bool,
    default: bool,
) {
    cv.fill(r, if focused { p.select } else { p.panel });
    if focused {
        cv.frame(r, p.fg);
        cv.frame(r.inset(1), p.fg);
    } else {
        cv.frame(r, if default { p.muted } else { p.border });
    }
    label(
        cv,
        fonts,
        if focused { UI | BOLD } else { UI },
        px,
        r,
        text,
        p.fg,
        1,
    );
}

pub fn checkbox(cv: &mut Canvas, p: &Palette, r: Rect, on: bool, focused: bool) {
    let s = 14;
    let b = Rect::new(r.x, r.y + (r.h - s) / 2, s, s);
    cv.fill(b, if on { p.fg } else { p.bg });
    cv.frame(b, if on { p.fg } else { p.muted });
    if focused {
        cv.frame(b.inset(-3), p.muted);
    }
}

// ------------------------------------------------------------------ menus

#[derive(Clone, Debug)]
pub struct MenuItem {
    pub label: String,
    pub shortcut: String,
    pub action: Option<usize>,
    pub sub: Vec<MenuItem>,
    pub enabled: bool,
    pub checked: Option<bool>,
    pub separator: bool,
}

impl MenuItem {
    pub fn act(label: &str, shortcut: &str, action: usize) -> Self {
        MenuItem {
            label: label.into(),
            shortcut: shortcut.into(),
            action: Some(action),
            sub: Vec::new(),
            enabled: true,
            checked: None,
            separator: false,
        }
    }
    pub fn sub(label: &str, items: Vec<MenuItem>) -> Self {
        MenuItem {
            label: label.into(),
            shortcut: String::new(),
            action: None,
            sub: items,
            enabled: true,
            checked: None,
            separator: false,
        }
    }
    pub fn sep() -> Self {
        MenuItem {
            label: String::new(),
            shortcut: String::new(),
            action: None,
            sub: Vec::new(),
            enabled: false,
            checked: None,
            separator: true,
        }
    }
    pub fn enabled(mut self, on: bool) -> Self {
        self.enabled = on;
        self
    }
    pub fn checked(mut self, on: bool) -> Self {
        self.checked = Some(on);
        self
    }
}

/// An open menu and its open submenus.
pub struct Menu {
    pub levels: Vec<(Vec<MenuItem>, Option<usize>, Rect)>,
}

pub enum MenuOut {
    None,
    Close,
    Run(usize),
}

impl Menu {
    pub fn new(items: Vec<MenuItem>, at: (i32, i32)) -> Self {
        Menu {
            levels: vec![(items, None, Rect::new(at.0, at.1, 0, 0))],
        }
    }

    fn row_h(px: f32) -> i32 {
        (px * 2.3) as i32
    }

    /// Place every level (sizes depend on the font).
    pub fn layout(&mut self, fonts: &mut Fonts, px: f32, bounds: Rect) {
        let rh = Self::row_h(px);
        for i in 0..self.levels.len() {
            let items = &self.levels[i].0;
            let mut w = 120.0f32;
            for it in items {
                let lw = fonts.width(UI, px, &it.label) + fonts.width(UI, px, &it.shortcut);
                w = w.max(lw + 90.0);
            }
            let h: i32 = items
                .iter()
                .map(|it| if it.separator { 11 } else { rh })
                .sum::<i32>()
                + 12;
            let (mut x, mut y) = (self.levels[i].2.x, self.levels[i].2.y);
            if i > 0 {
                let parent = self.levels[i - 1].2;
                let pi = self.levels[i - 1].1.unwrap_or(0);
                x = parent.x + parent.w - 4;
                y = parent.y + 6 + Self::offset(&self.levels[i - 1].0, pi, rh);
                if x + w as i32 > bounds.x + bounds.w {
                    x = parent.x - w as i32 + 4;
                }
            }
            if y + h > bounds.y + bounds.h {
                y = (bounds.y + bounds.h - h).max(bounds.y);
            }
            x = x.clamp(bounds.x, (bounds.x + bounds.w - w as i32).max(bounds.x));
            self.levels[i].2 = Rect::new(x, y, w as i32, h);
        }
    }

    fn offset(items: &[MenuItem], idx: usize, rh: i32) -> i32 {
        items[..idx]
            .iter()
            .map(|it| if it.separator { 11 } else { rh })
            .sum()
    }

    fn item_at(&self, level: usize, x: i32, y: i32, px: f32) -> Option<usize> {
        let (items, _, r) = &self.levels[level];
        if !r.contains(x, y) {
            return None;
        }
        let rh = Self::row_h(px);
        let mut yy = r.y + 6;
        for (i, it) in items.iter().enumerate() {
            let h = if it.separator { 11 } else { rh };
            if y >= yy && y < yy + h {
                return (!it.separator).then_some(i);
            }
            yy += h;
        }
        None
    }

    pub fn draw(&self, cv: &mut Canvas, fonts: &mut Fonts, p: &Palette, px: f32) {
        let rh = Self::row_h(px);
        for (items, sel, r) in &self.levels {
            cv.fill(r.inset(-1), p.border);
            cv.fill(*r, p.panel);
            let mut y = r.y + 6;
            for (i, it) in items.iter().enumerate() {
                if it.separator {
                    cv.fill(Rect::new(r.x + 8, y + 5, r.w - 16, 1), p.border);
                    y += 11;
                    continue;
                }
                let row = Rect::new(r.x + 6, y, r.w - 12, rh);
                if *sel == Some(i) && it.enabled {
                    cv.fill(row, p.select);
                }
                let color = if it.enabled { p.fg } else { p.muted };
                if let Some(c) = it.checked {
                    let b = Rect::new(row.x + 6, row.y + (rh - 10) / 2, 10, 10);
                    cv.frame(b, color);
                    if c {
                        cv.fill(b.inset(2), color);
                    }
                }
                label(
                    cv,
                    fonts,
                    UI,
                    px,
                    Rect::new(row.x + 24, row.y, row.w - 34, rh),
                    &it.label,
                    color,
                    0,
                );
                if !it.sub.is_empty() {
                    label(
                        cv,
                        fonts,
                        UI,
                        px,
                        Rect::new(row.x, row.y, row.w - 8, rh),
                        "›",
                        color,
                        2,
                    );
                } else if !it.shortcut.is_empty() {
                    label(
                        cv,
                        fonts,
                        UI,
                        px,
                        Rect::new(row.x, row.y, row.w - 8, rh),
                        &it.shortcut,
                        p.muted,
                        2,
                    );
                }
                y += rh;
            }
        }
    }

    fn open_sub(&mut self, level: usize) {
        self.levels.truncate(level + 1);
        let Some(i) = self.levels[level].1 else {
            return;
        };
        let it = &self.levels[level].0[i];
        if !it.sub.is_empty() && it.enabled {
            let sub = it.sub.clone();
            self.levels.push((sub, None, Rect::default()));
        }
    }

    pub fn motion(&mut self, x: i32, y: i32, px: f32) -> bool {
        for level in (0..self.levels.len()).rev() {
            if let Some(i) = self.item_at(level, x, y, px) {
                if self.levels[level].1 != Some(i) || self.levels.len() != level + 1 {
                    self.levels[level].1 = Some(i);
                    if !self.levels[level].0[i].sub.is_empty() {
                        self.open_sub(level);
                    } else {
                        self.levels.truncate(level + 1);
                    }
                    return true;
                }
                return false;
            }
        }
        false
    }

    pub fn click(&mut self, x: i32, y: i32, px: f32) -> MenuOut {
        for level in (0..self.levels.len()).rev() {
            if self.levels[level].2.contains(x, y) {
                if let Some(i) = self.item_at(level, x, y, px) {
                    let it = &self.levels[level].0[i];
                    if !it.enabled {
                        return MenuOut::None;
                    }
                    if let Some(a) = it.action {
                        return MenuOut::Run(a);
                    }
                    self.levels[level].1 = Some(i);
                    self.open_sub(level);
                }
                return MenuOut::None;
            }
        }
        MenuOut::Close
    }

    pub fn key(&mut self, k: &Key) -> MenuOut {
        let level = self.levels.len() - 1;
        let n = self.levels[level].0.len();
        let step = |m: &mut Menu, d: i64| {
            let items = &m.levels[level].0;
            let mut i = m.levels[level]
                .1
                .map_or(if d > 0 { -1 } else { n as i64 }, |v| v as i64);
            for _ in 0..n {
                i = (i + d).rem_euclid(n as i64);
                if !items[i as usize].separator && items[i as usize].enabled {
                    break;
                }
            }
            m.levels[level].1 = Some(i as usize);
        };
        match k {
            Key::Down => step(self, 1),
            Key::Up => step(self, -1),
            Key::Home => {
                self.levels[level].1 = None;
                step(self, 1);
            }
            Key::End => {
                self.levels[level].1 = None;
                step(self, -1);
            }
            Key::Right => {
                if self.levels[level].1.is_some() {
                    self.open_sub(level);
                    if self.levels.len() > level + 1 {
                        let nl = self.levels.len() - 1;
                        self.levels[nl].1 = None;
                        let n2 = self.levels[nl].0.len();
                        let first = self.levels[nl]
                            .0
                            .iter()
                            .position(|i| !i.separator && i.enabled);
                        self.levels[nl].1 = first.or(if n2 > 0 { Some(0) } else { None });
                    }
                }
            }
            Key::Left => {
                if level > 0 {
                    self.levels.pop();
                } else {
                    return MenuOut::Close;
                }
            }
            Key::Esc => {
                if level > 0 {
                    self.levels.pop();
                } else {
                    return MenuOut::Close;
                }
            }
            Key::Enter | Key::Char(' ') => {
                if let Some(i) = self.levels[level].1 {
                    let it = &self.levels[level].0[i];
                    if !it.enabled {
                        return MenuOut::None;
                    }
                    if let Some(a) = it.action {
                        return MenuOut::Run(a);
                    }
                    self.open_sub(level);
                    let nl = self.levels.len() - 1;
                    if nl > level {
                        self.levels[nl].1 = self.levels[nl]
                            .0
                            .iter()
                            .position(|i| !i.separator && i.enabled);
                    }
                }
            }
            _ => {}
        }
        MenuOut::None
    }
}

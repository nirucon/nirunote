//! Dialogs inside the window, all keyboard-complete: forms (messages,
//! questions, Preferences, Rename), pickers with type-to-filter (Outline,
//! Go to heading, Command Palette, Quick Open), the file chooser (Open, Save
//! As, folders) and the side-by-side Compare of recovered text.
use crate::canvas::{Canvas, Rect};
use crate::fonts::{Fonts, BOLD};
use crate::theme::Palette;
use crate::ui::{button, checkbox, label, FieldOut, Key, LineEdit, Mods, UI};
use std::path::{Path, PathBuf};

pub enum Row {
    Text(String),
    Info(String, String),
    Field(String, LineEdit),
    Check(String, bool),
    Choice(String, Vec<String>, usize),
    Spin(String, i64, i64, i64),
    Folder(String, LineEdit),
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Focus {
    Row(usize),
    Choose(usize),
    Button(usize),
}

pub enum FormOut {
    None,
    Button(usize),
    Choose,
    Copy(String),
    Paste,
}

pub struct Form {
    pub title: String,
    pub rows: Vec<Row>,
    pub buttons: Vec<String>,
    pub default: usize,
    pub escape: usize,
    focus: Focus,
    pub width: i32,
    hits: Vec<(Rect, Focus)>,
}

impl Form {
    pub fn new(
        title: &str,
        rows: Vec<Row>,
        buttons: &[&str],
        default: usize,
        escape: usize,
    ) -> Self {
        let mut f = Form {
            title: title.into(),
            rows,
            buttons: buttons.iter().map(|s| s.to_string()).collect(),
            default,
            escape,
            focus: Focus::Button(default),
            width: 460,
            hits: Vec::new(),
        };
        if let Some(first) = f
            .focusables()
            .into_iter()
            .find(|x| matches!(x, Focus::Row(_)))
        {
            if matches!(
                f.rows
                    .iter()
                    .find(|r| !matches!(r, Row::Text(_) | Row::Info(..))),
                Some(Row::Field(..))
            ) {
                f.focus = first;
            }
        }
        f
    }

    pub fn focus_row(&mut self, i: usize) {
        self.focus = Focus::Row(i);
    }

    fn focusables(&self) -> Vec<Focus> {
        let mut v = Vec::new();
        for (i, r) in self.rows.iter().enumerate() {
            match r {
                Row::Text(_) | Row::Info(..) => {}
                Row::Folder(..) => {
                    v.push(Focus::Row(i));
                    v.push(Focus::Choose(i));
                }
                _ => v.push(Focus::Row(i)),
            }
        }
        for b in 0..self.buttons.len() {
            v.push(Focus::Button(b));
        }
        v
    }

    fn cycle(&mut self, d: i64) {
        let f = self.focusables();
        let i = f.iter().position(|x| *x == self.focus).unwrap_or(0) as i64;
        self.focus = f[(i + d).rem_euclid(f.len() as i64) as usize];
    }

    pub fn field(&self, i: usize) -> String {
        match &self.rows[i] {
            Row::Field(_, e) | Row::Folder(_, e) => e.text.clone(),
            _ => String::new(),
        }
    }

    pub fn check(&self, i: usize) -> bool {
        matches!(&self.rows[i], Row::Check(_, true))
    }

    pub fn choice(&self, i: usize) -> usize {
        match &self.rows[i] {
            Row::Choice(_, _, v) => *v,
            _ => 0,
        }
    }

    pub fn spin(&self, i: usize) -> i64 {
        match &self.rows[i] {
            Row::Spin(_, v, _, _) => *v,
            _ => 0,
        }
    }

    pub fn set_field(&mut self, i: usize, s: &str) {
        if let Row::Field(_, e) | Row::Folder(_, e) = &mut self.rows[i] {
            e.set(s);
        }
    }

    pub fn paste(&mut self, s: &str) {
        if let Focus::Row(i) = self.focus {
            if let Row::Field(_, e) | Row::Folder(_, e) = &mut self.rows[i] {
                e.insert(s);
            }
        }
    }

    pub fn key(&mut self, k: &Key, m: Mods) -> FormOut {
        match k {
            Key::Tab => {
                self.cycle(1);
                return FormOut::None;
            }
            Key::BackTab => {
                self.cycle(-1);
                return FormOut::None;
            }
            Key::Esc => return FormOut::Button(self.escape),
            _ => {}
        }
        match self.focus {
            Focus::Button(b) => match k {
                Key::Enter | Key::Char(' ') => return FormOut::Button(b),
                Key::Left | Key::Up => self.cycle(-1),
                Key::Right | Key::Down => self.cycle(1),
                _ => {}
            },
            Focus::Choose(_) => match k {
                Key::Enter | Key::Char(' ') => return FormOut::Choose,
                Key::Up => self.cycle(-1),
                Key::Down => self.cycle(1),
                _ => {}
            },
            Focus::Row(i) => {
                let row = &mut self.rows[i];
                match row {
                    Row::Field(_, e) | Row::Folder(_, e) => {
                        if *k == Key::Enter {
                            return FormOut::Button(self.default);
                        }
                        if matches!(k, Key::Up) {
                            self.cycle(-1);
                            return FormOut::None;
                        }
                        if matches!(k, Key::Down) {
                            self.cycle(1);
                            return FormOut::None;
                        }
                        match e.key(k, m) {
                            FieldOut::Copy(s) => return FormOut::Copy(s),
                            FieldOut::Paste => return FormOut::Paste,
                            _ => {}
                        }
                    }
                    Row::Check(_, on) => match k {
                        Key::Char(' ') => *on = !*on,
                        Key::Enter => return FormOut::Button(self.default),
                        Key::Up => self.cycle(-1),
                        Key::Down => self.cycle(1),
                        _ => {}
                    },
                    Row::Choice(_, opts, v) => match k {
                        Key::Left | Key::Up => *v = (*v + opts.len() - 1) % opts.len(),
                        Key::Right | Key::Down | Key::Char(' ') => *v = (*v + 1) % opts.len(),
                        Key::Enter => return FormOut::Button(self.default),
                        _ => {}
                    },
                    Row::Spin(_, v, lo, hi) => match k {
                        Key::Up | Key::Right => *v = (*v + 1).min(*hi),
                        Key::Down | Key::Left => *v = (*v - 1).max(*lo),
                        Key::PageUp => *v = (*v + 5).min(*hi),
                        Key::PageDown => *v = (*v - 5).max(*lo),
                        Key::Enter => return FormOut::Button(self.default),
                        _ => {}
                    },
                    _ => {}
                }
            }
        }
        FormOut::None
    }

    pub fn click(&mut self, fonts: &mut Fonts, px: f32, x: i32, y: i32) -> FormOut {
        let hit = self.hits.iter().find(|(r, _)| r.contains(x, y)).copied();
        let Some((r, f)) = hit else {
            return FormOut::None;
        };
        self.focus = f;
        match f {
            Focus::Button(b) => return FormOut::Button(b),
            Focus::Choose(_) => return FormOut::Choose,
            Focus::Row(i) => match &mut self.rows[i] {
                Row::Check(_, on) => *on = !*on,
                Row::Choice(_, opts, v) => *v = (*v + 1) % opts.len(),
                Row::Spin(_, v, lo, hi) => {
                    if x > r.x + r.w - 24 {
                        if y < r.y + r.h / 2 {
                            *v = (*v + 1).min(*hi);
                        } else {
                            *v = (*v - 1).max(*lo);
                        }
                    }
                }
                Row::Field(_, e) | Row::Folder(_, e) => {
                    e.click(fonts, px, (x - r.x - 6) as f32, false);
                }
                _ => {}
            },
        }
        FormOut::None
    }

    pub fn draw(&mut self, cv: &mut Canvas, fonts: &mut Fonts, p: &Palette, area: Rect, px: f32) {
        let mut rh = (px * 2.4) as i32;
        let mut gap = 10;
        let pad = 22;
        // Shrink the rows when the window is low (a scratchpad).
        let n_rows = self.rows.len() as i32 + 2;
        let need = n_rows * (rh + gap) + 2 * pad;
        if need > area.h - 20 {
            let f = (area.h - 20 - 2 * pad).max(40) as f32 / (n_rows * (rh + gap)) as f32;
            rh = ((rh as f32 * f) as i32).max((px * 1.6) as i32);
            gap = ((gap as f32 * f) as i32).max(2);
        }
        let label_w = self
            .rows
            .iter()
            .filter_map(|r| match r {
                Row::Info(l, _)
                | Row::Field(l, _)
                | Row::Check(l, _)
                | Row::Choice(l, _, _)
                | Row::Spin(l, _, _, _)
                | Row::Folder(l, _) => Some(fonts.width(UI, px, l).ceil() as i32 + 4),
                _ => None,
            })
            .max()
            .unwrap_or(0);
        let w = self.width.max(label_w + 260).min(area.w - 20);
        // Text rows wrap.
        let mut text_lines: Vec<Vec<String>> = Vec::new();
        for r in &self.rows {
            if let Row::Text(t) = r {
                text_lines.push(wrap(fonts, px, t, (w - 2 * pad) as f32));
            }
        }
        let mut h = pad + rh + 4;
        let mut ti = 0;
        for r in &self.rows {
            h += match r {
                Row::Text(_) => {
                    let n = text_lines[ti].len() as i32;
                    ti += 1;
                    n * (px * 1.5) as i32 + gap
                }
                _ => rh + gap,
            };
        }
        h += rh + pad + 6;
        let h = h.min(area.h - 20);
        let r = Rect::new(area.x + (area.w - w) / 2, area.y + (area.h - h) / 3, w, h);
        cv.fill(r.inset(-1), p.border);
        cv.fill(r, p.bg);
        self.hits.clear();
        let mut y = r.y + pad / 2;
        label(
            cv,
            fonts,
            UI | BOLD,
            px,
            Rect::new(r.x + pad, y, w - 2 * pad, rh),
            &self.title,
            p.fg,
            0,
        );
        y += rh + 4;
        let lx = r.x + pad;
        let vx = lx + if label_w > 0 { label_w + 16 } else { 0 };
        let vw = r.x + w - pad - vx;
        let mut ti = 0;
        for (i, row) in self.rows.iter_mut().enumerate() {
            let focused = self.focus == Focus::Row(i);
            match row {
                Row::Text(_) => {
                    for line in &text_lines[ti] {
                        let lh = (px * 1.5) as i32;
                        label(
                            cv,
                            fonts,
                            UI,
                            px,
                            Rect::new(lx, y, w - 2 * pad, lh),
                            line,
                            p.fg,
                            0,
                        );
                        y += lh;
                    }
                    ti += 1;
                    y += gap;
                    continue;
                }
                Row::Info(l, v) => {
                    label(
                        cv,
                        fonts,
                        UI,
                        px,
                        Rect::new(lx, y, label_w, rh),
                        l,
                        p.muted,
                        0,
                    );
                    label(cv, fonts, UI, px, Rect::new(vx, y, vw, rh), v, p.fg, 0);
                }
                Row::Field(l, e) => {
                    label(
                        cv,
                        fonts,
                        UI,
                        px,
                        Rect::new(lx, y, label_w, rh),
                        l,
                        p.muted,
                        0,
                    );
                    let fr = Rect::new(vx, y, vw, rh);
                    e.draw(cv, fonts, p, fr, px, focused);
                    self.hits.push((fr, Focus::Row(i)));
                }
                Row::Folder(l, e) => {
                    label(
                        cv,
                        fonts,
                        UI,
                        px,
                        Rect::new(lx, y, label_w, rh),
                        l,
                        p.muted,
                        0,
                    );
                    let bw = (fonts.width(UI, px, "Choose…") + 24.0) as i32;
                    let fr = Rect::new(vx, y, vw - bw - 6, rh);
                    e.draw(cv, fonts, p, fr, px, focused);
                    self.hits.push((fr, Focus::Row(i)));
                    let br = Rect::new(vx + vw - bw, y, bw, rh);
                    button(
                        cv,
                        fonts,
                        p,
                        br,
                        px,
                        "Choose…",
                        self.focus == Focus::Choose(i),
                        false,
                    );
                    self.hits.push((br, Focus::Choose(i)));
                }
                Row::Check(l, on) => {
                    label(
                        cv,
                        fonts,
                        UI,
                        px,
                        Rect::new(lx, y, label_w, rh),
                        l,
                        p.muted,
                        0,
                    );
                    let cr = Rect::new(vx, y, vw, rh);
                    checkbox(cv, p, Rect::new(vx + 3, y, 20, rh), *on, focused);
                    self.hits.push((cr, Focus::Row(i)));
                }
                Row::Choice(l, opts, v) => {
                    label(
                        cv,
                        fonts,
                        UI,
                        px,
                        Rect::new(lx, y, label_w, rh),
                        l,
                        p.muted,
                        0,
                    );
                    let cr = Rect::new(vx, y, vw, rh);
                    cv.fill(cr, p.panel);
                    cv.frame(cr, if focused { p.muted } else { p.border });
                    label(cv, fonts, UI, px, cr.inset(8), &opts[*v], p.fg, 0);
                    label(
                        cv,
                        fonts,
                        UI,
                        px,
                        Rect::new(cr.x, cr.y, cr.w - 8, cr.h),
                        "‹ ›",
                        p.muted,
                        2,
                    );
                    self.hits.push((cr, Focus::Row(i)));
                }
                Row::Spin(l, v, _, _) => {
                    label(
                        cv,
                        fonts,
                        UI,
                        px,
                        Rect::new(lx, y, label_w, rh),
                        l,
                        p.muted,
                        0,
                    );
                    let cr = Rect::new(vx, y, 120.min(vw), rh);
                    cv.fill(cr, p.panel);
                    cv.frame(cr, if focused { p.muted } else { p.border });
                    label(cv, fonts, UI, px, cr.inset(8), &v.to_string(), p.fg, 0);
                    label(
                        cv,
                        fonts,
                        UI,
                        px,
                        Rect::new(cr.x, cr.y, cr.w - 6, cr.h),
                        "▴▾",
                        p.muted,
                        2,
                    );
                    self.hits.push((cr, Focus::Row(i)));
                }
            }
            y += rh + gap;
        }
        // Buttons, right aligned.
        let by = r.y + h - pad / 2 - rh - 4;
        let mut bx = r.x + w - pad;
        for b in (0..self.buttons.len()).rev() {
            let bw = (fonts.width(UI, px, &self.buttons[b]) + 32.0).max(88.0) as i32;
            bx -= bw;
            let br = Rect::new(bx, by, bw, rh + 2);
            button(
                cv,
                fonts,
                p,
                br,
                px,
                &self.buttons[b],
                self.focus == Focus::Button(b),
                b == self.default,
            );
            self.hits.push((br, Focus::Button(b)));
            bx -= 8;
        }
    }
}

pub fn wrap(fonts: &mut Fonts, px: f32, text: &str, w: f32) -> Vec<String> {
    let mut out = Vec::new();
    for para in text.split('\n') {
        let mut line = String::new();
        for word in para.split(' ') {
            let cand = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            if fonts.width(UI, px, &cand) > w && !line.is_empty() {
                out.push(std::mem::take(&mut line));
                line = word.to_string();
            } else {
                line = cand;
            }
        }
        out.push(line);
    }
    out
}

// ------------------------------------------------------------------ picker

#[derive(Clone, Copy, PartialEq)]
pub enum Filter {
    /// Every term in the label, in the original order.
    Terms,
    /// Every term, ranked by where they occur, at most 200 shown.
    Ranked,
}

pub struct Picker {
    pub title: String,
    pub filter: LineEdit,
    pub items: Vec<(String, String)>,
    pub shown: Vec<usize>,
    pub sel: usize,
    scroll: usize,
    pub status: Option<String>,
    mode: Filter,
    rows_visible: usize,
    list_rect: Rect,
}

pub enum PickOut {
    None,
    Pick(String),
    Cancel,
    Copy(String),
    Paste,
}

impl Picker {
    pub fn new(title: &str, placeholder: &str, items: Vec<(String, String)>, mode: Filter) -> Self {
        let mut p = Picker {
            title: title.into(),
            filter: LineEdit::new("", placeholder),
            items,
            shown: Vec::new(),
            sel: 0,
            scroll: 0,
            status: None,
            mode,
            rows_visible: 10,
            list_rect: Rect::default(),
        };
        p.refilter();
        p
    }

    pub fn set_items(&mut self, items: Vec<(String, String)>) {
        self.items = items;
        self.refilter();
    }

    pub fn refilter(&mut self) {
        let q = self.filter.text.to_lowercase();
        let terms: Vec<&str> = q.split_whitespace().collect();
        match self.mode {
            Filter::Terms => {
                self.shown = (0..self.items.len())
                    .filter(|i| {
                        let l = self.items[*i].0.to_lowercase();
                        terms.iter().all(|t| l.contains(t))
                    })
                    .collect();
            }
            Filter::Ranked => {
                let mut ranked: Vec<(usize, String, usize)> = (0..self.items.len())
                    .filter_map(|i| {
                        let l = self.items[i].0.to_lowercase();
                        if terms.iter().all(|t| l.contains(t)) {
                            let score = terms.iter().map(|t| l.find(t).unwrap_or(0)).sum();
                            Some((score, self.items[i].0.clone(), i))
                        } else {
                            None
                        }
                    })
                    .collect();
                ranked.sort();
                self.status = Some(format!("{} match(es)", ranked.len()));
                self.shown = ranked.into_iter().take(200).map(|r| r.2).collect();
            }
        }
        self.sel = 0;
        self.scroll = 0;
    }

    pub fn key(&mut self, k: &Key, m: Mods) -> PickOut {
        let n = self.shown.len();
        match k {
            Key::Esc => return PickOut::Cancel,
            Key::Enter => {
                return match self.shown.get(self.sel) {
                    Some(i) => PickOut::Pick(self.items[*i].1.clone()),
                    None => PickOut::None,
                }
            }
            Key::Down => self.sel = (self.sel + 1).min(n.saturating_sub(1)),
            Key::Up => self.sel = self.sel.saturating_sub(1),
            Key::PageDown => self.sel = (self.sel + self.rows_visible).min(n.saturating_sub(1)),
            Key::PageUp => self.sel = self.sel.saturating_sub(self.rows_visible),
            _ => match self.filter.key(k, m) {
                FieldOut::Changed => self.refilter(),
                FieldOut::Copy(s) => return PickOut::Copy(s),
                FieldOut::Paste => return PickOut::Paste,
                _ => {}
            },
        }
        PickOut::None
    }

    pub fn paste(&mut self, s: &str) {
        self.filter.insert(s);
        self.refilter();
    }

    pub fn click(&mut self, x: i32, y: i32, double: bool, rh: i32) -> PickOut {
        if !self.list_rect.contains(x, y) {
            return PickOut::None;
        }
        let row = self.scroll + ((y - self.list_rect.y - 4) / rh.max(1)) as usize;
        if row < self.shown.len() {
            self.sel = row;
            if double {
                return PickOut::Pick(self.items[self.shown[row]].1.clone());
            }
        }
        PickOut::None
    }

    pub fn wheel(&mut self, down: bool) {
        let n = self.shown.len();
        if down {
            self.scroll = (self.scroll + 3).min(n.saturating_sub(self.rows_visible));
        } else {
            self.scroll = self.scroll.saturating_sub(3);
        }
    }

    pub fn draw(
        &mut self,
        cv: &mut Canvas,
        fonts: &mut Fonts,
        p: &Palette,
        area: Rect,
        px: f32,
        size: (i32, i32),
    ) {
        let rh = (px * 2.2) as i32;
        let w = size.0.min(area.w - 20);
        let h = size.1.min(area.h - 20);
        let r = Rect::new(area.x + (area.w - w) / 2, area.y + (area.h - h) / 4, w, h);
        cv.fill(r.inset(-1), p.border);
        cv.fill(r, p.bg);
        let mut y = r.y + 10;
        label(
            cv,
            fonts,
            UI | BOLD,
            px,
            Rect::new(r.x + 14, y, w - 28, rh),
            &self.title,
            p.fg,
            0,
        );
        y += rh + 2;
        self.filter.draw(
            cv,
            fonts,
            p,
            Rect::new(r.x + 12, y, w - 24, rh + 4),
            px,
            true,
        );
        y += rh + 12;
        if let Some(s) = &self.status {
            label(
                cv,
                fonts,
                UI,
                px,
                Rect::new(r.x + 14, y, w - 28, rh),
                s,
                p.muted,
                0,
            );
            y += rh;
        }
        let lr = Rect::new(r.x + 12, y, w - 24, r.y + h - 12 - y);
        self.list_rect = lr;
        cv.frame(lr, p.border);
        self.rows_visible = ((lr.h - 8) / rh).max(1) as usize;
        if self.sel < self.scroll {
            self.scroll = self.sel;
        }
        if self.sel >= self.scroll + self.rows_visible {
            self.scroll = self.sel + 1 - self.rows_visible;
        }
        cv.set_clip(lr.inset(1));
        for (k, idx) in self
            .shown
            .iter()
            .enumerate()
            .skip(self.scroll)
            .take(self.rows_visible + 1)
        {
            let ry = lr.y + 4 + ((k - self.scroll) as i32) * rh;
            let row = Rect::new(lr.x + 4, ry, lr.w - 8, rh);
            if k == self.sel {
                cv.fill(row, p.select);
            }
            label(
                cv,
                fonts,
                UI,
                px,
                row.inset(6),
                &self.items[*idx].0,
                p.fg,
                0,
            );
        }
        cv.reset_clip();
    }
}

// ---------------------------------------------------------------- chooser

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ChooseMode {
    Open,
    Save,
    Folder,
}

pub struct Chooser {
    pub mode: ChooseMode,
    pub title: String,
    pub dir: PathBuf,
    entries: Vec<(String, bool)>,
    shown: Vec<usize>,
    sel: Option<usize>,
    scroll: usize,
    pub name: LineEdit,
    hidden: bool,
    all_files: bool,
    list_active: bool,
    rows_visible: usize,
    list_rect: Rect,
    buttons: Vec<Rect>,
    pub error: Option<String>,
}

pub enum ChooseOut {
    None,
    Done(PathBuf),
    Cancel,
    Copy(String),
    Paste,
}

pub fn text_file(name: &str) -> bool {
    let l = name.to_lowercase();
    l.ends_with(".md") || l.ends_with(".markdown") || l.ends_with(".txt")
}

impl Chooser {
    pub fn new(mode: ChooseMode, title: &str, dir: &Path, name: &str) -> Self {
        let mut c = Chooser {
            mode,
            title: title.into(),
            dir: dir.to_path_buf(),
            entries: Vec::new(),
            shown: Vec::new(),
            sel: None,
            scroll: 0,
            name: LineEdit::new(
                name,
                match mode {
                    ChooseMode::Open => "Type to filter, or a path",
                    ChooseMode::Save => "File name",
                    ChooseMode::Folder => "Type to filter, or a path",
                },
            ),
            hidden: false,
            all_files: false,
            list_active: false,
            rows_visible: 10,
            list_rect: Rect::default(),
            buttons: Vec::new(),
            error: None,
        };
        if mode == ChooseMode::Save {
            // Select the name without the extension, like Qt does.
            let stem = name.rfind('.').unwrap_or(name.len());
            c.name.anchor = 0;
            c.name.cursor = stem;
        }
        c.read();
        c
    }

    fn read(&mut self) {
        let mut v: Vec<(String, bool)> = std::fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let n = e.file_name().to_string_lossy().into_owned();
                if !self.hidden && n.starts_with('.') {
                    return None;
                }
                let dir = e.path().is_dir();
                if !dir {
                    if self.mode == ChooseMode::Folder {
                        return None;
                    }
                    if !self.all_files && !text_file(&n) {
                        return None;
                    }
                }
                Some((n, dir))
            })
            .collect();
        v.sort_by(|a, b| {
            b.1.cmp(&a.1)
                .then(a.0.to_lowercase().cmp(&b.0.to_lowercase()))
        });
        self.entries = v;
        self.refilter();
    }

    fn refilter(&mut self) {
        let q = if self.mode == ChooseMode::Save || self.name.text.contains('/') {
            String::new()
        } else {
            self.name.text.to_lowercase()
        };
        self.shown = (0..self.entries.len())
            .filter(|i| q.is_empty() || self.entries[*i].0.to_lowercase().contains(&q))
            .collect();
        self.sel = if self.shown.is_empty() || self.mode == ChooseMode::Save {
            None
        } else {
            Some(0)
        };
        self.scroll = 0;
    }

    fn go(&mut self, dir: PathBuf) {
        self.dir = std::fs::canonicalize(&dir).unwrap_or(dir);
        if self.mode != ChooseMode::Save {
            self.name.set("");
        }
        self.list_active = false;
        self.error = None;
        self.read();
    }

    fn resolve(&self, t: &str) -> PathBuf {
        let t = t.trim();
        let p = if let Some(rest) = t.strip_prefix("~/") {
            crate::theme::home().join(rest)
        } else if t == "~" {
            crate::theme::home()
        } else {
            PathBuf::from(t)
        };
        if p.is_absolute() {
            p
        } else {
            self.dir.join(p)
        }
    }

    fn accept(&mut self) -> ChooseOut {
        let text = self.name.text.trim().to_string();
        let selected = self
            .sel
            .and_then(|s| self.shown.get(s))
            .map(|i| self.entries[*i].clone());
        if let (true, Some((n, dir))) = (
            (self.list_active
                || text.is_empty()
                || self.mode == ChooseMode::Open && !text.contains('/')),
            selected,
        ) {
            if dir && !(self.mode == ChooseMode::Folder && !self.list_active && text.is_empty()) {
                self.go(self.dir.join(n));
                return ChooseOut::None;
            }
            if !dir {
                if self.mode == ChooseMode::Save {
                    self.name.set(&n);
                    self.list_active = false;
                    return ChooseOut::Done(self.dir.join(n));
                }
                return ChooseOut::Done(self.dir.join(n));
            }
        }
        match self.mode {
            ChooseMode::Folder => {
                if text.is_empty() {
                    return ChooseOut::Done(self.dir.clone());
                }
                let p = self.resolve(&text);
                if p.is_dir() {
                    return ChooseOut::Done(p);
                }
                self.error = Some(format!("Not a folder: {text}"));
                ChooseOut::None
            }
            ChooseMode::Open => {
                if text.is_empty() {
                    return ChooseOut::None;
                }
                let p = self.resolve(&text);
                if p.is_dir() {
                    self.go(p);
                    return ChooseOut::None;
                }
                if p.is_file() {
                    return ChooseOut::Done(p);
                }
                self.error = Some(format!("Not found: {text}"));
                ChooseOut::None
            }
            ChooseMode::Save => {
                if text.is_empty() {
                    return ChooseOut::None;
                }
                let p = self.resolve(&text);
                if p.is_dir() {
                    let keep = self.name.text.clone();
                    self.go(p);
                    self.name.set(if keep.contains('/') { "" } else { &keep });
                    return ChooseOut::None;
                }
                ChooseOut::Done(p)
            }
        }
    }

    pub fn key(&mut self, k: &Key, m: Mods) -> ChooseOut {
        let n = self.shown.len();
        if m.ctrl {
            if let Key::Char(c) = k {
                match c.to_ascii_lowercase() {
                    'h' => {
                        self.hidden = !self.hidden;
                        self.read();
                        return ChooseOut::None;
                    }
                    't' if self.mode != ChooseMode::Folder => {
                        self.all_files = !self.all_files;
                        self.read();
                        return ChooseOut::None;
                    }
                    _ => {}
                }
            }
        }
        match k {
            Key::Esc => return ChooseOut::Cancel,
            Key::Enter => return self.accept(),
            Key::Down if n > 0 => {
                self.sel = Some(self.sel.map_or(0, |s| (s + 1).min(n - 1)));
                self.list_active = true;
            }
            Key::Up if n > 0 => {
                self.sel = Some(self.sel.map_or(0, |s| s.saturating_sub(1)));
                self.list_active = true;
            }
            Key::PageDown if n > 0 => {
                self.sel = Some(self.sel.map_or(0, |s| (s + self.rows_visible).min(n - 1)));
                self.list_active = true;
            }
            Key::PageUp if n > 0 => {
                self.sel = Some(self.sel.map_or(0, |s| s.saturating_sub(self.rows_visible)));
                self.list_active = true;
            }
            Key::Backspace if self.name.text.is_empty() || (m.alt) => {
                if let Some(parent) = self.dir.parent().map(Path::to_path_buf) {
                    self.go(parent);
                }
            }
            Key::Left if m.alt => {
                if let Some(parent) = self.dir.parent().map(Path::to_path_buf) {
                    self.go(parent);
                }
            }
            Key::Tab => {
                if let Some(i) = self.sel.and_then(|s| self.shown.get(s)) {
                    let (n, dir) = self.entries[*i].clone();
                    if dir {
                        self.go(self.dir.join(n));
                    } else {
                        self.name.set(&n);
                    }
                }
            }
            _ => match self.name.key(k, m) {
                FieldOut::Changed => {
                    self.list_active = false;
                    self.error = None;
                    let t = self.name.text.clone();
                    if t.ends_with('/') {
                        let p = self.resolve(&t);
                        if p.is_dir() {
                            self.go(p);
                            return ChooseOut::None;
                        }
                    }
                    self.refilter();
                }
                FieldOut::Copy(s) => return ChooseOut::Copy(s),
                FieldOut::Paste => return ChooseOut::Paste,
                _ => {}
            },
        }
        ChooseOut::None
    }

    pub fn paste(&mut self, s: &str) {
        self.name.insert(s);
        self.refilter();
    }

    pub fn click(&mut self, x: i32, y: i32, double: bool, rh: i32) -> ChooseOut {
        if let Some(b) = self.buttons.iter().position(|r| r.contains(x, y)) {
            return if b == 0 {
                ChooseOut::Cancel
            } else {
                self.list_active = self.sel.is_some() && self.mode != ChooseMode::Save;
                if self.mode == ChooseMode::Folder {
                    self.list_active = false;
                }
                self.accept()
            };
        }
        if !self.list_rect.contains(x, y) {
            return ChooseOut::None;
        }
        let row = self.scroll + ((y - self.list_rect.y - 4) / rh.max(1)) as usize;
        if row < self.shown.len() {
            self.sel = Some(row);
            self.list_active = true;
            let (n, dir) = self.entries[self.shown[row]].clone();
            if !dir && self.mode == ChooseMode::Save {
                self.name.set(&n);
            }
            if double {
                return self.accept();
            }
        }
        ChooseOut::None
    }

    pub fn wheel(&mut self, down: bool) {
        let n = self.shown.len();
        if down {
            self.scroll = (self.scroll + 3).min(n.saturating_sub(self.rows_visible));
        } else {
            self.scroll = self.scroll.saturating_sub(3);
        }
    }

    pub fn draw(&mut self, cv: &mut Canvas, fonts: &mut Fonts, p: &Palette, area: Rect, px: f32) {
        let rh = (px * 2.2) as i32;
        let w = 680.min(area.w - 20);
        let h = 520.min(area.h - 20);
        let r = Rect::new(area.x + (area.w - w) / 2, area.y + (area.h - h) / 4, w, h);
        cv.fill(r.inset(-1), p.border);
        cv.fill(r, p.bg);
        let mut y = r.y + 10;
        label(
            cv,
            fonts,
            UI | BOLD,
            px,
            Rect::new(r.x + 14, y, w - 28, rh),
            &self.title,
            p.fg,
            0,
        );
        y += rh;
        let dir = self.dir.to_string_lossy().into_owned();
        label(
            cv,
            fonts,
            UI,
            px,
            Rect::new(r.x + 14, y, w - 28, rh),
            &dir,
            p.muted,
            0,
        );
        y += rh + 2;
        let lh = h - (y - r.y) - 2 * rh - 40;
        let lr = Rect::new(r.x + 12, y, w - 24, lh);
        self.list_rect = lr;
        cv.frame(lr, p.border);
        self.rows_visible = ((lr.h - 8) / rh).max(1) as usize;
        if let Some(s) = self.sel {
            if s < self.scroll {
                self.scroll = s;
            }
            if s >= self.scroll + self.rows_visible {
                self.scroll = s + 1 - self.rows_visible;
            }
        }
        cv.set_clip(lr.inset(1));
        for (k, idx) in self
            .shown
            .iter()
            .enumerate()
            .skip(self.scroll)
            .take(self.rows_visible + 1)
        {
            let ry = lr.y + 4 + ((k - self.scroll) as i32) * rh;
            let row = Rect::new(lr.x + 4, ry, lr.w - 8, rh);
            if Some(k) == self.sel {
                cv.fill(row, if self.list_active { p.select } else { p.panel });
            }
            let (n, dir) = &self.entries[*idx];
            let t = if *dir { format!("{n}/") } else { n.clone() };
            label(
                cv,
                fonts,
                UI,
                px,
                row.inset(6),
                &t,
                if *dir { p.accent } else { p.fg },
                0,
            );
        }
        if self.shown.is_empty() {
            label(cv, fonts, UI, px, lr.inset(10), "Nothing here", p.muted, 1);
        }
        cv.reset_clip();
        y = lr.y + lr.h + 8;
        self.name.draw(
            cv,
            fonts,
            p,
            Rect::new(r.x + 12, y, w - 24, rh + 4),
            px,
            true,
        );
        y += rh + 10;
        let hint = match (&self.error, self.mode) {
            (Some(e), _) => e.clone(),
            (None, ChooseMode::Folder) => {
                "Enter: choose · ↑↓ then Enter: open folder · Backspace: up".into()
            }
            (None, _) => format!(
                "Enter: {} · Backspace: up · Ctrl+H hidden · Ctrl+T {}",
                if self.mode == ChooseMode::Save {
                    "save"
                } else {
                    "open"
                },
                if self.all_files {
                    "Markdown and text"
                } else {
                    "all files"
                }
            ),
        };
        let accept = match self.mode {
            ChooseMode::Open => "Open",
            ChooseMode::Save => "Save",
            ChooseMode::Folder => "Choose",
        };
        let bw = 90;
        let b2 = Rect::new(r.x + w - 12 - bw, y, bw, rh + 2);
        let b1 = Rect::new(b2.x - 8 - bw, y, bw, rh + 2);
        label(
            cv,
            fonts,
            UI,
            px * 0.9,
            Rect::new(r.x + 14, y, b1.x - r.x - 24, rh + 2),
            &hint,
            if self.error.is_some() { p.red } else { p.muted },
            0,
        );
        button(cv, fonts, p, b1, px, "Cancel", false, false);
        button(cv, fonts, p, b2, px, accept, false, true);
        self.buttons = vec![b1, b2];
    }
}

// ---------------------------------------------------------------- compare

pub struct Compare {
    pub left: Vec<String>,
    pub right: Vec<String>,
    pub marks: Vec<char>,
    pub left_title: String,
    pub scroll: usize,
    rows_visible: usize,
}

impl Compare {
    pub fn new(on_disk: &str, recovered: &str) -> Self {
        let (left, right, marks) = crate::files::side_by_side(on_disk, recovered);
        Compare {
            left,
            right,
            marks,
            left_title: if on_disk.is_empty() {
                "On disk (empty or missing)".into()
            } else {
                "On disk".into()
            },
            scroll: 0,
            rows_visible: 20,
        }
    }

    pub fn changed(&self) -> usize {
        self.marks.iter().filter(|m| **m != ' ').count()
    }

    /// true closes.
    pub fn key(&mut self, k: &Key) -> bool {
        let n = self.marks.len();
        let max = n.saturating_sub(self.rows_visible);
        match k {
            Key::Esc | Key::Enter | Key::Char(' ') => return true,
            Key::Down => self.scroll = (self.scroll + 1).min(max),
            Key::Up => self.scroll = self.scroll.saturating_sub(1),
            Key::PageDown => self.scroll = (self.scroll + self.rows_visible).min(max),
            Key::PageUp => self.scroll = self.scroll.saturating_sub(self.rows_visible),
            Key::Home => self.scroll = 0,
            Key::End => self.scroll = max,
            _ => {}
        }
        false
    }

    pub fn wheel(&mut self, down: bool) {
        let max = self.marks.len().saturating_sub(self.rows_visible);
        self.scroll = if down {
            (self.scroll + 3).min(max)
        } else {
            self.scroll.saturating_sub(3)
        };
    }

    pub fn draw(&mut self, cv: &mut Canvas, fonts: &mut Fonts, p: &Palette, area: Rect, px: f32) {
        let rh = (px * 1.5) as i32;
        let w = 1000.min(area.w - 20);
        let h = 640.min(area.h - 20);
        let r = Rect::new(area.x + (area.w - w) / 2, area.y + (area.h - h) / 2, w, h);
        cv.fill(r.inset(-1), p.border);
        cv.fill(r, p.bg);
        let th = (px * 2.2) as i32;
        label(
            cv,
            fonts,
            UI | BOLD,
            px,
            Rect::new(r.x + 14, r.y + 8, w - 28, th),
            "Compare recovered text",
            p.fg,
            0,
        );
        let colw = (w - 36) / 2;
        let top = r.y + 8 + th;
        label(
            cv,
            fonts,
            UI,
            px,
            Rect::new(r.x + 12, top, colw, th),
            &self.left_title,
            p.muted,
            0,
        );
        label(
            cv,
            fonts,
            UI,
            px,
            Rect::new(r.x + 24 + colw, top, colw, th),
            "Recovered",
            p.muted,
            0,
        );
        let body = Rect::new(r.x + 12, top + th, w - 24, h - (top + th - r.y) - th - 20);
        self.rows_visible = (body.h / rh).max(1) as usize;
        for side in 0..2 {
            let cr = Rect::new(body.x + side * (colw + 12), body.y, colw, body.h);
            cv.frame(cr, p.border);
            cv.set_clip(cr.inset(1));
            let lines = if side == 0 { &self.left } else { &self.right };
            for (k, line) in lines
                .iter()
                .enumerate()
                .skip(self.scroll)
                .take(self.rows_visible + 1)
            {
                let y = cr.y + 2 + ((k - self.scroll) as i32) * rh;
                let mk = self.marks[k];
                if mk == '~' || (side == 0 && mk == '-') || (side == 1 && mk == '+') {
                    cv.fill(Rect::new(cr.x + 1, y, cr.w - 2, rh), p.select);
                }
                let m = fonts.metrics(UI, px * 0.95);
                let base = y + (rh - m.ascent - m.descent) / 2 + m.ascent;
                fonts.draw(cv, UI, px * 0.95, (cr.x + 6) as f32, base, line, p.fg);
            }
            cv.reset_clip();
        }
        let n = self.changed();
        let status = if n > 0 {
            format!("{n} changed line(s) · Esc closes")
        } else {
            "No differences · Esc closes".into()
        };
        label(
            cv,
            fonts,
            UI,
            px,
            Rect::new(r.x + 14, r.y + h - th - 6, w - 28, th),
            &status,
            p.muted,
            0,
        );
    }
}

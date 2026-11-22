//! The text being edited: lines, edits, undo and redo, and the saved state.
//!
//! Every change is a replacement of a range by a text. Typing and deleting
//! merge into one undo step until the cursor moves or the kind of edit
//! changes, like Qt's editor. "Modified" is true when the current undo
//! state differs from the saved one, so undoing back to the saved text
//! clears it again.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default, Hash)]
pub struct Pos {
    pub line: usize,
    /// Byte offset in the line.
    pub col: usize,
}

impl Pos {
    pub const fn new(line: usize, col: usize) -> Self {
        Pos { line, col }
    }
}

#[derive(Clone, Debug)]
struct Edit {
    at: Pos,
    removed: String,
    inserted: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Typing,
    Deleting,
    Other,
}

#[derive(Clone, Debug)]
struct Group {
    edits: Vec<Edit>,
    before: (Pos, Pos),
    after: (Pos, Pos),
    id: u64,
    kind: Kind,
}

/// A change of lines for the layout cache: at `line`, `old` lines became `new`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Splice {
    pub line: usize,
    pub old: usize,
    pub new: usize,
}

pub struct Doc {
    pub lines: Vec<String>,
    undo: Vec<Group>,
    redo: Vec<Group>,
    next_id: u64,
    clean: Option<u64>,
    merge: bool,
    /// Bumped by every change, including undo and redo.
    pub generation: u64,
    pub splices: Vec<Splice>,
}

fn split_lines(s: &str) -> Vec<String> {
    s.split('\n').map(str::to_string).collect()
}

impl Doc {
    pub fn new(text: &str) -> Self {
        Doc {
            lines: split_lines(text),
            undo: Vec::new(),
            redo: Vec::new(),
            next_id: 1,
            clean: Some(0),
            merge: false,
            generation: 0,
            splices: Vec::new(),
        }
    }

    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    /// Characters (code points), with a newline between lines.
    #[cfg(test)]
    pub fn char_count(&self) -> usize {
        self.lines.iter().map(|l| l.chars().count()).sum::<usize>() + self.lines.len() - 1
    }

    pub fn byte_len(&self) -> usize {
        self.lines.iter().map(String::len).sum::<usize>() + self.lines.len() - 1
    }

    fn state(&self) -> u64 {
        self.undo.last().map_or(0, |g| g.id)
    }

    pub fn is_modified(&self) -> bool {
        self.clean != Some(self.state())
    }

    pub fn mark_saved(&mut self) {
        self.clean = Some(self.state());
    }

    /// A document that is unsaved from the start (new from clipboard).
    pub fn mark_unsaved(&mut self) {
        self.clean = None;
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// The next edit starts a new undo step.
    pub fn break_merge(&mut self) {
        self.merge = false;
    }

    pub fn end(&self) -> Pos {
        let l = self.lines.len() - 1;
        Pos::new(l, self.lines[l].len())
    }

    pub fn clamp(&self, p: Pos) -> Pos {
        let line = p.line.min(self.lines.len() - 1);
        let s = &self.lines[line];
        let mut col = p.col.min(s.len());
        while !s.is_char_boundary(col) {
            col -= 1;
        }
        Pos::new(line, col)
    }

    pub fn slice(&self, a: Pos, b: Pos) -> String {
        let (a, b) = if a <= b { (a, b) } else { (b, a) };
        if a.line == b.line {
            return self.lines[a.line][a.col..b.col].to_string();
        }
        let mut s = self.lines[a.line][a.col..].to_string();
        for l in &self.lines[a.line + 1..b.line] {
            s.push('\n');
            s.push_str(l);
        }
        s.push('\n');
        s.push_str(&self.lines[b.line][..b.col]);
        s
    }

    fn end_of(at: Pos, text: &str) -> Pos {
        match text.rfind('\n') {
            None => Pos::new(at.line, at.col + text.len()),
            Some(i) => Pos::new(at.line + text.matches('\n').count(), text.len() - i - 1),
        }
    }

    fn raw_remove(&mut self, a: Pos, b: Pos) -> String {
        let removed = self.slice(a, b);
        if a.line == b.line {
            self.lines[a.line].replace_range(a.col..b.col, "");
        } else {
            let tail = self.lines[b.line][b.col..].to_string();
            self.lines[a.line].truncate(a.col);
            self.lines[a.line].push_str(&tail);
            self.lines.drain(a.line + 1..=b.line);
        }
        removed
    }

    fn raw_insert(&mut self, at: Pos, text: &str) -> Pos {
        if !text.contains('\n') {
            self.lines[at.line].insert_str(at.col, text);
            return Pos::new(at.line, at.col + text.len());
        }
        let tail = self.lines[at.line].split_off(at.col);
        let mut parts = text.split('\n');
        self.lines[at.line].push_str(parts.next().unwrap_or(""));
        let mut new: Vec<String> = parts.map(str::to_string).collect();
        let last = new.len() - 1;
        let end = Pos::new(at.line + new.len(), new[last].len());
        new[last].push_str(&tail);
        let n = at.line + 1;
        self.lines.splice(n..n, new);
        end
    }

    fn apply(&mut self, at: Pos, removed_end: Pos, inserted: &str) -> (String, Pos) {
        let old = removed_end.line - at.line + 1;
        let removed = self.raw_remove(at, removed_end);
        let end = self.raw_insert(at, inserted);
        self.splices.push(Splice {
            line: at.line,
            old,
            new: end.line - at.line + 1,
        });
        self.generation += 1;
        (removed, end)
    }

    /// Replace `a..b` by `text`. `sel` is the cursor and anchor before the
    /// edit (for undo). Returns the end of the inserted text.
    pub fn replace(&mut self, a: Pos, b: Pos, text: &str, kind: Kind, sel: (Pos, Pos)) -> Pos {
        let (a, b) = if a <= b { (a, b) } else { (b, a) };
        let (a, b) = (self.clamp(a), self.clamp(b));
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        if a == b && text.is_empty() {
            return a;
        }
        let (removed, end) = self.apply(a, b, &text);
        let edit = Edit {
            at: a,
            removed,
            inserted: text,
        };
        self.redo.clear();
        if let Some(c) = self.clean {
            // The saved state was on the redo side: it can never come back.
            if c != 0 && !self.undo.iter().any(|g| g.id == c) {
                self.clean = None;
            }
        }
        let mergeable = self.merge
            && kind != Kind::Other
            && self.undo.last().is_some_and(|g| {
                g.kind == kind
                    && g.edits.len() < 400
                    && self.clean != Some(g.id)
                    && !edit.inserted.contains('\n')
            });
        if mergeable {
            let g = self.undo.last_mut().unwrap();
            g.edits.push(edit);
            g.after = (end, end);
        } else {
            let id = self.next_id;
            self.next_id += 1;
            self.undo.push(Group {
                edits: vec![edit],
                before: sel,
                after: (end, end),
                id,
                kind,
            });
        }
        self.merge = kind != Kind::Other;
        end
    }

    /// Merge the steps made since `mark` (the undo depth then) into one.
    pub fn squash_since(&mut self, mark: usize) {
        if self.undo.len() <= mark + 1 {
            return;
        }
        let tail: Vec<Group> = self.undo.drain(mark..).collect();
        let before = tail[0].before;
        let after = tail[tail.len() - 1].after;
        let id = tail[tail.len() - 1].id;
        let edits = tail.into_iter().flat_map(|g| g.edits).collect();
        self.undo.push(Group {
            edits,
            before,
            after,
            id,
            kind: Kind::Other,
        });
        self.merge = false;
    }

    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    /// Undo one step; returns the selection to restore (cursor, anchor).
    pub fn undo(&mut self) -> Option<(Pos, Pos)> {
        let g = self.undo.pop()?;
        for e in g.edits.iter().rev() {
            let end = Self::end_of(e.at, &e.inserted);
            self.apply(e.at, end, &e.removed);
        }
        let sel = g.before;
        self.redo.push(g);
        self.merge = false;
        Some(sel)
    }

    pub fn redo(&mut self) -> Option<(Pos, Pos)> {
        let g = self.redo.pop()?;
        for e in &g.edits {
            let end = Self::end_of(e.at, &e.removed);
            self.apply(e.at, end, &e.inserted);
        }
        let sel = g.after;
        self.undo.push(g);
        self.merge = false;
        Some(sel)
    }

    // ------------------------------------------------------- positions

    pub fn prev(&self, p: Pos) -> Pos {
        if p.col > 0 {
            let s = &self.lines[p.line];
            let mut c = p.col - 1;
            while !s.is_char_boundary(c) {
                c -= 1;
            }
            Pos::new(p.line, c)
        } else if p.line > 0 {
            Pos::new(p.line - 1, self.lines[p.line - 1].len())
        } else {
            p
        }
    }

    pub fn next(&self, p: Pos) -> Pos {
        let s = &self.lines[p.line];
        if p.col < s.len() {
            let mut c = p.col + 1;
            while !s.is_char_boundary(c) {
                c += 1;
            }
            Pos::new(p.line, c)
        } else if p.line + 1 < self.lines.len() {
            Pos::new(p.line + 1, 0)
        } else {
            p
        }
    }

    /// The start of the word before `p` (Ctrl+Left, Ctrl+Backspace).
    pub fn word_left(&self, p: Pos) -> Pos {
        if p.col == 0 {
            return self.prev(p);
        }
        let s = &self.lines[p.line][..p.col];
        let chars: Vec<(usize, char)> = s.char_indices().collect();
        let mut i = chars.len();
        while i > 0 && chars[i - 1].1.is_whitespace() {
            i -= 1;
        }
        if i > 0 {
            let word = is_word(chars[i - 1].1);
            while i > 0 && !chars[i - 1].1.is_whitespace() && is_word(chars[i - 1].1) == word {
                i -= 1;
            }
        }
        Pos::new(p.line, chars.get(i).map_or(s.len(), |c| c.0))
    }

    /// The end of the word after `p` (Ctrl+Right, Ctrl+Delete).
    pub fn word_right(&self, p: Pos) -> Pos {
        let s = &self.lines[p.line];
        if p.col >= s.len() {
            return self.next(p);
        }
        let rest = &s[p.col..];
        let chars: Vec<(usize, char)> = rest.char_indices().collect();
        let mut i = 0;
        if !chars.is_empty() && !chars[0].1.is_whitespace() {
            let word = is_word(chars[0].1);
            while i < chars.len() && !chars[i].1.is_whitespace() && is_word(chars[i].1) == word {
                i += 1;
            }
        }
        while i < chars.len() && chars[i].1.is_whitespace() {
            i += 1;
        }
        Pos::new(p.line, p.col + chars.get(i).map_or(rest.len(), |c| c.0))
    }

    /// The word around `p` (double-click).
    pub fn word_at(&self, p: Pos) -> (Pos, Pos) {
        let s = &self.lines[p.line];
        let chars: Vec<(usize, char)> = s.char_indices().collect();
        let i = chars
            .iter()
            .position(|c| c.0 >= p.col)
            .unwrap_or(chars.len());
        let idx = if i < chars.len() {
            i
        } else {
            i.saturating_sub(1)
        };
        if chars.is_empty() {
            return (p, p);
        }
        let kind = |c: char| {
            if c.is_whitespace() {
                0
            } else if is_word(c) {
                1
            } else {
                2
            }
        };
        let k = kind(chars[idx].1);
        let mut a = idx;
        while a > 0 && kind(chars[a - 1].1) == k {
            a -= 1;
        }
        let mut b = idx + 1;
        while b < chars.len() && kind(chars[b].1) == k {
            b += 1;
        }
        (
            Pos::new(p.line, chars[a].0),
            Pos::new(p.line, chars.get(b).map_or(s.len(), |c| c.0)),
        )
    }

    /// Position as Qt counted it: UTF-16 units from the start, one per line
    /// break. Used for the remembered cursor (`cursor/PATH`).
    pub fn utf16_offset(&self, p: Pos) -> usize {
        let mut n = 0;
        for l in &self.lines[..p.line] {
            n += l.encode_utf16().count() + 1;
        }
        n + self.lines[p.line][..p.col].encode_utf16().count()
    }

    pub fn pos_from_utf16(&self, mut off: usize) -> Pos {
        for (i, l) in self.lines.iter().enumerate() {
            let len = l.encode_utf16().count();
            if off <= len {
                let mut units = 0;
                for (b, c) in l.char_indices() {
                    if units >= off {
                        return Pos::new(i, b);
                    }
                    units += c.len_utf16();
                }
                return Pos::new(i, l.len());
            }
            off -= len + 1;
        }
        self.end()
    }

    /// The position `n` characters into the text (newlines count one).
    pub fn pos_from_char(&self, mut n: usize) -> Pos {
        for (i, l) in self.lines.iter().enumerate() {
            let len = l.chars().count();
            if n <= len {
                let b = l.char_indices().nth(n).map_or(l.len(), |c| c.0);
                return Pos::new(i, b);
            }
            n -= len + 1;
        }
        self.end()
    }

    pub fn char_offset(&self, p: Pos) -> usize {
        let mut n = 0;
        for l in &self.lines[..p.line] {
            n += l.chars().count() + 1;
        }
        n + self.lines[p.line][..p.col].chars().count()
    }
}

pub fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s() -> (Pos, Pos) {
        (Pos::default(), Pos::default())
    }

    #[test]
    fn undo_back_to_the_saved_text_clears_modified() {
        let mut d = Doc::new("# Title\n");
        assert!(!d.is_modified());
        d.replace(Pos::new(1, 0), Pos::new(1, 0), "a", Kind::Typing, s());
        d.replace(Pos::new(1, 1), Pos::new(1, 1), "b", Kind::Typing, s());
        assert!(d.is_modified());
        assert_eq!(d.text(), "# Title\nab");
        d.undo();
        assert_eq!(d.text(), "# Title\n");
        assert!(!d.is_modified(), "typing merged into one step");
        d.redo();
        assert!(d.is_modified());
        d.mark_saved();
        assert!(!d.is_modified());
        d.undo();
        assert!(d.is_modified(), "undo past the save point");
        d.redo();
        assert!(!d.is_modified(), "redo back to the save point");
    }

    #[test]
    fn multi_line_replacements_round_trip() {
        let mut d = Doc::new("one\ntwo\nthree");
        let end = d.replace(Pos::new(0, 1), Pos::new(2, 2), "X\nY", Kind::Other, s());
        assert_eq!(d.text(), "oX\nYree");
        assert_eq!(end, Pos::new(1, 1));
        d.undo();
        assert_eq!(d.text(), "one\ntwo\nthree");
        d.redo();
        assert_eq!(d.text(), "oX\nYree");
    }

    #[test]
    fn qt_cursor_offsets_count_utf16_and_newlines() {
        let d = Doc::new("åä\n😀x\n");
        let p = Pos::new(1, "😀".len());
        assert_eq!(d.utf16_offset(p), 3 + 2);
        assert_eq!(d.pos_from_utf16(5), p);
        assert_eq!(d.pos_from_char(4), p);
        assert_eq!(d.char_count(), 6);
    }

    #[test]
    fn words_are_found_left_and_right() {
        let d = Doc::new("hello, wide world");
        assert_eq!(d.word_right(Pos::new(0, 0)), Pos::new(0, 5));
        assert_eq!(d.word_left(Pos::new(0, 12)), Pos::new(0, 7));
        assert_eq!(d.word_at(Pos::new(0, 9)), (Pos::new(0, 7), Pos::new(0, 11)));
    }
}

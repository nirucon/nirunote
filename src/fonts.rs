//! Fonts: the editor's monospace and the Preview's sans-serif family, each in
//! regular, bold, italic and bold italic, found through fontconfig
//! (`fc-match`) once and remembered in `~/.cache/nirunote/fonts`, mapped
//! from disk (only the pages in use count as memory) and loaded only when a
//! style is first drawn. Glyph masks and advances are cached per
//! (face, character, size); a character missing from a face is taken from a
//! fallback font that fontconfig picks for it.
use crate::canvas::Canvas;
use ab_glyph::{point, Font, FontRef, PxScale, ScaleFont};
use std::collections::HashMap;
use std::path::PathBuf;

pub const MONO: u8 = 0;
pub const SANS: u8 = 4;
pub const BOLD: u8 = 1;
pub const ITALIC: u8 = 2;

const PATTERNS: [&str; 8] = [
    "monospace",
    "monospace:bold",
    "monospace:italic",
    "monospace:bold:italic",
    "sans-serif",
    "sans-serif:bold",
    "sans-serif:italic",
    "sans-serif:bold:italic",
];

struct Mask {
    w: i32,
    h: i32,
    x: i32,
    y: i32,
    coverage: Vec<u8>,
}

/// Vertical metrics of one face at one size, in whole pixels.
#[derive(Clone, Copy, Debug, Default)]
pub struct Metrics {
    pub ascent: i32,
    pub descent: i32,
    pub height: i32,
}

pub struct Fonts {
    faces: [Option<Option<FontRef<'static>>>; 8],
    paths: HashMap<String, String>,
    paths_dirty: bool,
    by_path: HashMap<String, FontRef<'static>>,
    fallback: HashMap<char, Option<FontRef<'static>>>,
    symbols: Option<Option<FontRef<'static>>>,
    masks: HashMap<(u8, char, u32), Option<Mask>>,
    advances: HashMap<(u8, char, u32), f32>,
    ascii: HashMap<(u8, u32), Box<[f32; 128]>>,
}

fn cache_file() -> PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| crate::theme::home().join(".cache"));
    base.join("nirunote/fonts")
}

pub fn fc_match(pattern: &str) -> Option<String> {
    let out = std::process::Command::new("fc-match")
        .args(["-f", "%{file}", pattern])
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success() && !p.is_empty()).then_some(p)
}

/// Map a font file read-only into memory for the rest of the run.
fn map_file(path: &str) -> Option<&'static [u8]> {
    use std::os::fd::AsRawFd;
    let f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len() as usize;
    if len == 0 {
        return None;
    }
    unsafe {
        let p = libc::mmap(
            std::ptr::null_mut(),
            len,
            libc::PROT_READ,
            libc::MAP_PRIVATE,
            f.as_raw_fd(),
            0,
        );
        if p == libc::MAP_FAILED {
            return None;
        }
        Some(std::slice::from_raw_parts(p as *const u8, len))
    }
}

const FALLBACK_FILES: [&str; 6] = [
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
    "/usr/share/fonts/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans.ttf",
];

impl Fonts {
    pub fn new() -> Self {
        let mut paths = HashMap::new();
        if let Ok(text) = std::fs::read_to_string(cache_file()) {
            for line in text.lines() {
                if let Some((k, v)) = line.split_once('\t') {
                    if std::path::Path::new(v).is_file() {
                        paths.insert(k.to_string(), v.to_string());
                    }
                }
            }
        }
        Fonts {
            faces: Default::default(),
            paths,
            paths_dirty: false,
            by_path: HashMap::new(),
            fallback: HashMap::new(),
            symbols: None,
            masks: HashMap::new(),
            advances: HashMap::new(),
            ascii: HashMap::new(),
        }
    }

    /// Resolve several faces at once: fontconfig runs in parallel for the
    /// ones not in the cache (the first start after an install).
    pub fn preload(&mut self, faces: &[u8]) {
        let missing: Vec<&str> = faces
            .iter()
            .map(|f| PATTERNS[*f as usize & 7])
            .filter(|p| !self.paths.contains_key(*p))
            .collect();
        if missing.len() > 1 {
            let found: Vec<(String, Option<String>)> = std::thread::scope(|s| {
                let hs: Vec<_> = missing
                    .iter()
                    .map(|p| s.spawn(move || (p.to_string(), fc_match(p))))
                    .collect();
                hs.into_iter().filter_map(|h| h.join().ok()).collect()
            });
            for (p, path) in found {
                if let Some(path) = path {
                    self.paths.insert(p, path);
                    self.paths_dirty = true;
                }
            }
        }
        for f in faces {
            self.face(*f);
        }
    }

    /// Write newly found font paths to the cache file.
    pub fn save_cache(&mut self) {
        if !self.paths_dirty {
            return;
        }
        self.paths_dirty = false;
        let mut lines: Vec<String> = self
            .paths
            .iter()
            .map(|(k, v)| format!("{k}\t{v}\n"))
            .collect();
        lines.sort();
        let file = cache_file();
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(file, lines.concat());
    }

    fn open(&mut self, path: &str) -> Option<FontRef<'static>> {
        if let Some(f) = self.by_path.get(path) {
            return Some(f.clone());
        }
        let data = map_file(path)?;
        let f = FontRef::try_from_slice(data).ok()?;
        self.by_path.insert(path.to_string(), f.clone());
        Some(f)
    }

    fn face(&mut self, face: u8) -> Option<FontRef<'static>> {
        let i = face as usize & 7;
        if let Some(f) = &self.faces[i] {
            return f.clone();
        }
        let pattern = PATTERNS[i];
        let path = match self.paths.get(pattern) {
            Some(p) => Some(p.clone()),
            None => {
                let p = fc_match(pattern);
                if let Some(p) = &p {
                    self.paths.insert(pattern.to_string(), p.clone());
                    self.paths_dirty = true;
                }
                p
            }
        };
        let mut f = path.and_then(|p| self.open(&p));
        if f.is_none() {
            // Bold or italic missing: the regular face of the family.
            if i & 3 != 0 {
                f = self.face(face & 4);
            } else {
                for p in FALLBACK_FILES {
                    if let Some(x) = self.open(p) {
                        f = Some(x);
                        break;
                    }
                }
            }
        }
        self.faces[i] = Some(f.clone());
        f
    }

    fn font_for(&mut self, face: u8, c: char) -> Option<(FontRef<'static>, bool)> {
        let f = self.face(face)?;
        if f.glyph_id(c).0 != 0 || c.is_ascii() || c.is_control() {
            return Some((f, true));
        }
        if face & 3 != 0 {
            if let Some(r) = self.face(face & 4) {
                if r.glyph_id(c).0 != 0 {
                    return Some((r, false));
                }
            }
        }
        if self.symbols.is_none() {
            let s = fc_match("DejaVu Sans").and_then(|p| self.open(&p));
            self.symbols = Some(s);
        }
        if let Some(Some(s)) = &self.symbols {
            if s.glyph_id(c).0 != 0 {
                return Some((s.clone(), false));
            }
        }
        if let Some(hit) = self.fallback.get(&c) {
            return hit.clone().map(|f| (f, false)).or(Some((f, true)));
        }
        let found = fc_match(&format!(":charset={:x}", c as u32))
            .and_then(|p| self.open(&p))
            .filter(|x| x.glyph_id(c).0 != 0);
        self.fallback.insert(c, found.clone());
        Some(found.map(|x| (x, false)).unwrap_or((f, true)))
    }

    pub fn metrics(&mut self, face: u8, px: f32) -> Metrics {
        let Some(f) = self.face(face) else {
            return Metrics {
                ascent: px as i32,
                descent: 0,
                height: px as i32 + 2,
            };
        };
        let s = f.as_scaled(PxScale::from(px));
        let ascent = s.ascent().ceil() as i32;
        let descent = (-s.descent()).ceil() as i32;
        let gap = s.line_gap().max(0.0).round() as i32;
        Metrics {
            ascent,
            descent,
            height: ascent + descent + gap,
        }
    }

    pub fn advance(&mut self, face: u8, px: f32, c: char) -> f32 {
        let bits = px.to_bits();
        if (c as u32) < 128 {
            if let Some(t) = self.ascii.get(&(face, bits)) {
                return t[c as usize];
            }
            let mut t = Box::new([0f32; 128]);
            if let Some(f) = self.face(face) {
                let s = f.as_scaled(PxScale::from(px));
                for (i, v) in t.iter_mut().enumerate().skip(32) {
                    *v = s.h_advance(f.glyph_id(i as u8 as char));
                }
            }
            let a = t[c as usize];
            self.ascii.insert((face, bits), t);
            return a;
        }
        let key = (face, c, bits);
        if let Some(a) = self.advances.get(&key) {
            return *a;
        }
        let a = match self.font_for(face, c) {
            Some((f, _)) => {
                let s = f.as_scaled(PxScale::from(px));
                s.h_advance(f.glyph_id(c))
            }
            None => px * 0.6,
        };
        self.advances.insert(key, a);
        a
    }

    pub fn width(&mut self, face: u8, px: f32, s: &str) -> f32 {
        s.chars().map(|c| self.advance(face, px, c)).sum()
    }

    /// The longest prefix of `s` that fits `max` pixels, with "…" when cut.
    pub fn ellipsize(&mut self, face: u8, px: f32, s: &str, max: f32) -> String {
        if self.width(face, px, s) <= max {
            return s.to_string();
        }
        let dots = self.advance(face, px, '…');
        let mut out = String::new();
        let mut w = 0.0;
        for c in s.chars() {
            let a = self.advance(face, px, c);
            if w + a + dots > max {
                break;
            }
            w += a;
            out.push(c);
        }
        out.push('…');
        out
    }

    /// Draw one character with its baseline at `base`; returns the advance.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_char(
        &mut self,
        cv: &mut Canvas,
        face: u8,
        px: f32,
        x: f32,
        base: i32,
        c: char,
        color: u32,
    ) -> f32 {
        let adv = self.advance(face, px, c);
        if c == ' ' || c == '\t' || c.is_control() {
            return adv;
        }
        let key = (face, c, px.to_bits());
        if !self.masks.contains_key(&key) {
            let m = self.rasterise(face, c, px);
            self.masks.insert(key, m);
        }
        if let Some(Some(m)) = self.masks.get(&key) {
            cv.mask(
                x.round() as i32 + m.x,
                base + m.y,
                m.w,
                m.h,
                &m.coverage,
                color,
            );
        }
        adv
    }

    /// Draw `s` with its baseline at `base`. Returns the width.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        cv: &mut Canvas,
        face: u8,
        px: f32,
        x: f32,
        base: i32,
        s: &str,
        color: u32,
    ) -> f32 {
        let mut pen = x;
        for c in s.chars() {
            pen += self.draw_char(cv, face, px, pen, base, c, color);
        }
        pen - x
    }

    fn rasterise(&mut self, face: u8, c: char, px: f32) -> Option<Mask> {
        let (f, own) = self.font_for(face, c)?;
        let glyph = f
            .glyph_id(c)
            .with_scale_and_position(PxScale::from(px), point(0.0, 0.0));
        let outline = f.outline_glyph(glyph)?;
        let b = outline.px_bounds();
        let (w, h) = (b.width().ceil() as i32, b.height().ceil() as i32);
        if w <= 0 || h <= 0 {
            return None;
        }
        let mut coverage = vec![0u8; (w * h) as usize];
        outline.draw(|gx, gy, v| {
            let i = (gy as i32 * w + gx as i32) as usize;
            if i < coverage.len() {
                coverage[i] = (v * 255.0).round().clamp(0.0, 255.0) as u8;
            }
        });
        // A fallback face has no bold: thicken by one pixel.
        if !own && face & BOLD != 0 {
            for row in 0..h {
                for col in (1..w).rev() {
                    let i = (row * w + col) as usize;
                    coverage[i] = coverage[i].max(coverage[i - 1]);
                }
            }
        }
        Some(Mask {
            w,
            h,
            x: b.min.x.floor() as i32,
            y: b.min.y.floor() as i32,
            coverage,
        })
    }

    /// Forget the glyph cache (after many size changes it would grow).
    pub fn trim(&mut self) {
        if self.masks.len() > 6000 {
            self.masks.clear();
        }
        if self.advances.len() > 20000 {
            self.advances.clear();
        }
    }
}

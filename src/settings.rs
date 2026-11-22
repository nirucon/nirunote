//! Preferences in `~/.config/NIRU/NIRUNOTE.conf`, the file Qt's QSettings
//! wrote for NIRUNOTE 0.x: the same INI dialect (sections, `%XX` escaped
//! keys, `\` for `/` in keys, quoted and escaped values, `@ByteArray(...)`,
//! `@Invalid()` for an empty list), the same keys and defaults. A write
//! re-reads the file and changes only the keys this window set, the way
//! QSettings merges, so the scratchpad and the main window can share it.
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Str(String),
    List(Vec<String>),
    Bytes(Vec<u8>),
}

impl Value {
    pub fn as_str(&self) -> String {
        match self {
            Value::Str(s) => s.clone(),
            Value::List(l) => l.join(", "),
            Value::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
        }
    }
    pub fn as_list(&self) -> Vec<String> {
        match self {
            Value::Str(s) if s.is_empty() => Vec::new(),
            Value::Str(s) => vec![s.clone()],
            Value::List(l) => l.clone(),
            Value::Bytes(_) => Vec::new(),
        }
    }
}

/// The settings file, as a map of full keys ("theme", "cursor/home/x.md").
pub struct Store {
    pub path: PathBuf,
    pub values: BTreeMap<String, Value>,
    changed: BTreeMap<String, Option<Value>>,
}

pub fn settings_path() -> PathBuf {
    crate::theme::config_home().join("NIRU/NIRUNOTE.conf")
}

impl Store {
    pub fn open(path: PathBuf) -> Self {
        let values = std::fs::read(&path)
            .map(|b| parse(&String::from_utf8_lossy(&b)))
            .unwrap_or_default();
        Store {
            path,
            values,
            changed: BTreeMap::new(),
        }
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.values.get(&normal_key(key))
    }

    pub fn str_or(&self, key: &str, default: &str) -> String {
        match self.get(key) {
            Some(Value::Str(s)) => s.clone(),
            Some(v) => v.as_str(),
            None => default.to_string(),
        }
    }

    pub fn bool_or(&self, key: &str, default: bool) -> bool {
        match self.get(key).map(|v| v.as_str()) {
            Some(s) => match s.to_ascii_lowercase().as_str() {
                "true" | "1" | "yes" | "on" => true,
                "false" | "0" | "no" | "off" | "" => false,
                _ => default,
            },
            None => default,
        }
    }

    pub fn int_or(&self, key: &str, default: i64) -> i64 {
        self.get(key)
            .and_then(|v| v.as_str().trim().parse::<f64>().ok())
            .map(|f| f as i64)
            .unwrap_or(default)
    }

    pub fn list(&self, key: &str) -> Vec<String> {
        self.get(key).map(|v| v.as_list()).unwrap_or_default()
    }

    pub fn set(&mut self, key: &str, v: Value) {
        let k = normal_key(key);
        if self.values.get(&k) == Some(&v) {
            return;
        }
        self.values.insert(k.clone(), v.clone());
        self.changed.insert(k, Some(v));
    }

    pub fn set_str(&mut self, key: &str, v: &str) {
        self.set(key, Value::Str(v.to_string()));
    }

    pub fn set_bool(&mut self, key: &str, v: bool) {
        self.set_str(key, if v { "true" } else { "false" });
    }

    pub fn set_int(&mut self, key: &str, v: i64) {
        self.set_str(key, &v.to_string());
    }

    /// Merge this window's changes into the file on disk and write it.
    pub fn sync(&mut self) {
        if self.changed.is_empty() {
            return;
        }
        let mut disk = std::fs::read(&self.path)
            .map(|b| parse(&String::from_utf8_lossy(&b)))
            .unwrap_or_default();
        for (k, v) in std::mem::take(&mut self.changed) {
            match v {
                Some(v) => {
                    disk.insert(k, v);
                }
                None => {
                    disk.remove(&k);
                }
            }
        }
        self.values = disk;
        let text = write(&self.values);
        if let Some(dir) = self.path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = crate::files::atomic_write_bytes(&self.path, text.as_bytes());
    }
}

/// QSettings drops leading, trailing and double slashes from keys.
pub fn normal_key(key: &str) -> String {
    key.split('/')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

fn escaped_key(key: &str) -> String {
    let mut out = String::new();
    for c in key.chars() {
        match c {
            '/' => out.push('\\'),
            'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-' | '.' => out.push(c),
            c if (c as u32) <= 0xff => out.push_str(&format!("%{:02X}", c as u32)),
            c => {
                let mut buf = [0u16; 2];
                for u in c.encode_utf16(&mut buf) {
                    out.push_str(&format!("%U{:04X}", u));
                }
            }
        }
    }
    out
}

fn unescaped_key(key: &str) -> String {
    let b: Vec<char> = key.chars().collect();
    let mut units: Vec<u16> = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == '\\' {
            units.push('/' as u16);
            i += 1;
        } else if c == '%' && i + 1 < b.len() {
            if b[i + 1] == 'U' && i + 5 < b.len() {
                let h: String = b[i + 2..(i + 6).min(b.len())].iter().collect();
                if let Ok(v) = u16::from_str_radix(&h, 16) {
                    units.push(v);
                    i += 6;
                    continue;
                }
            }
            let h: String = b[i + 1..(i + 3).min(b.len())].iter().collect();
            if let Ok(v) = u8::from_str_radix(&h, 16) {
                units.push(v as u16);
                i += 3;
                continue;
            }
            units.push('%' as u16);
            i += 1;
        } else {
            let mut buf = [0u16; 2];
            units.extend_from_slice(c.encode_utf16(&mut buf));
            i += 1;
        }
    }
    String::from_utf16_lossy(&units)
}

/// Parse the value side of `key=value` into one or more strings.
fn parse_value(raw: &str) -> Value {
    let chars: Vec<char> = raw.chars().collect();
    let mut items: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut any_quote = false;
    let mut list = false;
    let mut i = 0;
    // Leading spaces are not part of an unquoted value.
    while i < chars.len() {
        let c = chars[i];
        if c == '"' {
            quoted = !quoted;
            any_quote = true;
            i += 1;
            continue;
        }
        if c == '\\' && i + 1 < chars.len() {
            let n = chars[i + 1];
            i += 2;
            match n {
                'a' => cur.push('\x07'),
                'b' => cur.push('\x08'),
                'f' => cur.push('\x0c'),
                'n' => cur.push('\n'),
                'r' => cur.push('\r'),
                't' => cur.push('\t'),
                'v' => cur.push('\x0b'),
                '0' => cur.push('\0'),
                'x' => {
                    let mut v: u32 = 0;
                    let mut n = 0;
                    while i < chars.len() && chars[i].is_ascii_hexdigit() && n < 4 {
                        v = v * 16 + chars[i].to_digit(16).unwrap();
                        i += 1;
                        n += 1;
                    }
                    cur.push(char::from_u32(v).unwrap_or('\u{fffd}'));
                }
                '\n' => {}
                '\r' => {
                    if i < chars.len() && chars[i] == '\n' {
                        i += 1;
                    }
                }
                other => cur.push(other),
            }
            continue;
        }
        if c == ',' && !quoted {
            list = true;
            items.push(std::mem::take(&mut cur));
            i += 1;
            // ", " separates items.
            while i < chars.len() && chars[i] == ' ' {
                i += 1;
            }
            continue;
        }
        cur.push(c);
        i += 1;
    }
    let tidy = |s: String| -> String {
        if any_quote {
            s
        } else {
            s.trim().to_string()
        }
    };
    if list {
        items.push(cur);
        let items: Vec<String> = items.into_iter().map(|s| unat(&tidy(s))).collect();
        return Value::List(items);
    }
    let s = tidy(cur);
    if let Some(inner) = s
        .strip_prefix("@ByteArray(")
        .and_then(|r| r.strip_suffix(')'))
    {
        return Value::Bytes(inner.chars().map(|c| c as u32 as u8).collect());
    }
    if s == "@Invalid()" {
        return Value::List(Vec::new());
    }
    Value::Str(unat(&s))
}

fn unat(s: &str) -> String {
    s.strip_prefix("@@")
        .map(|r| format!("@{r}"))
        .unwrap_or_else(|| s.to_string())
}

fn escape_string(s: &str) -> String {
    let s = if s.starts_with('@') {
        format!("@{s}")
    } else {
        s.to_string()
    };
    let needs_quotes = s.contains([';', ',', '=', '"'])
        || s.starts_with(' ')
        || s.ends_with(' ')
        || s.starts_with('\t');
    let mut out = String::new();
    let mut after_hex = false;
    for c in s.chars() {
        let hexdigit = c.is_ascii_hexdigit();
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\x07' => out.push_str("\\a"),
            '\x08' => out.push_str("\\b"),
            '\x0c' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\x0b' => out.push_str("\\v"),
            '\0' => out.push_str("\\0"),
            c if (c as u32) < 0x20 || (after_hex && hexdigit) => {
                out.push_str(&format!("\\x{:x}", c as u32));
                after_hex = true;
                continue;
            }
            c => out.push(c),
        }
        after_hex = false;
    }
    if needs_quotes {
        format!("\"{out}\"")
    } else {
        out
    }
}

fn escape_bytes(b: &[u8]) -> String {
    let mut out = String::from("@ByteArray(");
    let mut after_hex = false;
    for &v in b {
        let c = v as char;
        match v {
            b'\\' => out.push_str("\\\\"),
            b'"' => out.push_str("\\\""),
            0 => {
                out.push_str("\\0");
                after_hex = false;
                continue;
            }
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            0x07 => out.push_str("\\a"),
            0x08 => out.push_str("\\b"),
            0x0c => out.push_str("\\f"),
            0x0b => out.push_str("\\v"),
            v if !(0x20..0x7f).contains(&v) || (after_hex && c.is_ascii_hexdigit()) => {
                out.push_str(&format!("\\x{v:x}"));
                after_hex = true;
                continue;
            }
            _ => out.push(c),
        }
        after_hex = false;
    }
    out.push(')');
    out
}

fn escape_value(v: &Value) -> String {
    match v {
        Value::Str(s) => escape_string(s),
        Value::Bytes(b) => escape_bytes(b),
        Value::List(l) if l.is_empty() => "@Invalid()".into(),
        Value::List(l) => l
            .iter()
            .map(|s| {
                let e = escape_string(s);
                if e.starts_with('"') {
                    e
                } else if s.contains(',') {
                    format!("\"{e}\"")
                } else {
                    e
                }
            })
            .collect::<Vec<_>>()
            .join(", "),
    }
}

pub fn parse(text: &str) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    let mut section = String::new();
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        let mut line = line.trim_start().to_string();
        // A value continued with a trailing backslash.
        while line.ends_with('\\') && !line.ends_with("\\\\") {
            line.pop();
            match lines.next() {
                Some(n) => line.push_str(n.trim_start()),
                None => break,
            }
        }
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            if let Some(end) = line.find(']') {
                let s = unescaped_key(&line[1..end]);
                section = if s.eq_ignore_ascii_case("general") {
                    String::new()
                } else {
                    s
                };
            }
            continue;
        }
        let Some(eq) = line.find('=') else {
            continue;
        };
        let key = unescaped_key(line[..eq].trim());
        let full = if section.is_empty() {
            key
        } else {
            format!("{section}/{key}")
        };
        out.insert(normal_key(&full), parse_value(&line[eq + 1..]));
    }
    out
}

pub fn write(values: &BTreeMap<String, Value>) -> String {
    // Sections in the order QSettings uses: [General] first, then by name.
    let mut sections: BTreeMap<String, Vec<(String, &Value)>> = BTreeMap::new();
    for (k, v) in values {
        let (sec, key) = match k.split_once('/') {
            Some((s, rest)) => (s.to_string(), rest.to_string()),
            None => (String::new(), k.clone()),
        };
        sections.entry(sec).or_default().push((key, v));
    }
    let mut out = String::new();
    let mut first = true;
    for (sec, entries) in &sections {
        if !first {
            out.push('\n');
        }
        first = false;
        if sec.is_empty() {
            out.push_str("[General]\n");
        } else {
            out.push_str(&format!("[{}]\n", escaped_key(sec)));
        }
        for (k, v) in entries {
            out.push_str(&format!("{}={}\n", escaped_key(k), escape_value(v)));
        }
    }
    out
}

/// Qt's saveGeometry(): the normal window rectangle (x, y, w, h).
pub fn geometry_rect(b: &[u8]) -> Option<(i32, i32, i32, i32)> {
    if b.len() < 4 + 4 + 16 + 16 {
        return None;
    }
    let rd = |o: usize| i32::from_be_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
    if rd(0) as u32 != 0x01D9_D0CB {
        return None;
    }
    // magic, version (u16 major, u16 minor), frame rect, normal rect.
    let (l, t, r, bo) = (rd(24), rd(28), rd(32), rd(36));
    let (w, h) = (r - l + 1, bo - t + 1);
    (w > 50 && h > 50).then_some((l, t, w, h))
}

/// Qt's saveGeometry() for a window at (x, y) of w x h, version 3.0.
pub fn geometry_bytes(x: i32, y: i32, w: i32, h: i32, sw: i32, sh: i32) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&0x01D9_D0CB_u32.to_be_bytes());
    b.extend_from_slice(&3u16.to_be_bytes());
    b.extend_from_slice(&0u16.to_be_bytes());
    for v in [x, y, x + w - 1, y + h - 1, x, y, x + w - 1, y + h - 1] {
        b.extend_from_slice(&v.to_be_bytes());
    }
    b.extend_from_slice(&0i32.to_be_bytes()); // screen number
    b.push(0); // maximized
    b.push(0); // full screen
    b.extend_from_slice(&sw.to_be_bytes()); // screen width
    for v in [x, y, x + w - 1, y + h - 1] {
        b.extend_from_slice(&v.to_be_bytes());
    }
    let _ = sh;
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "[General]\nautosave=true\nfont_size=17\ngeometry=@ByteArray(\\x1\\xd9\\xd0\\xcb\\0\\x3\\0\\0\\0\\0\\x5\\x82\\0\\0\\b\\x82\\0\\0\\t\\xf7\\0\\0\\v7\\0\\0\\x5\\x84\\0\\0\\b\\x84\\0\\0\\t\\xf5\\0\\0\\v5\\0\\0\\0\\0\\0\\0\\0\\0\\a\\x80\\0\\0\\x5\\x84\\0\\0\\b\\x84\\0\\0\\t\\xf5\\0\\0\\v5)\nrecent=/home/a/One.md, \"/home/a/b,c.md\"\nspell_language=sv_SE\ntheme=Follow NIRUX\n\n[cursor]\nhome\\a\\My%20notes%E5.md=42\n";

    #[test]
    fn qt_settings_files_read_back() {
        let v = parse(SAMPLE);
        assert_eq!(v["autosave"], Value::Str("true".into()));
        assert_eq!(v["theme"], Value::Str("Follow NIRUX".into()));
        assert_eq!(
            v["recent"],
            Value::List(vec!["/home/a/One.md".into(), "/home/a/b,c.md".into()])
        );
        assert_eq!(
            v["cursor/home/a/My notes\u{e5}.md"],
            Value::Str("42".into())
        );
        let Value::Bytes(g) = &v["geometry"] else {
            panic!("geometry")
        };
        let (x, y, w, h) = geometry_rect(g).unwrap();
        assert_eq!((x, y, w, h), (1412, 2180, 1138, 690));
    }

    #[test]
    fn writing_and_reading_is_lossless() {
        let mut m = BTreeMap::new();
        m.insert(
            "recent".into(),
            Value::List(vec!["/a/x.md".into(), "/b/y, z.md".into()]),
        );
        m.insert("one".into(), Value::List(vec![]));
        m.insert("theme".into(), Value::Str("@weird; \"quoted\"".into()));
        m.insert("cursor/home/n/å ö.md".into(), Value::Str("7".into()));
        m.insert(
            "geometry".into(),
            Value::Bytes(geometry_bytes(10, 20, 1000, 760, 1920, 1080)),
        );
        let text = write(&m);
        assert!(text.starts_with("[General]\n"), "{text}");
        assert!(text.contains("one=@Invalid()"), "{text}");
        assert_eq!(parse(&text), m, "{text}");
        let Value::Bytes(g) = &m["geometry"] else {
            unreachable!()
        };
        assert_eq!(geometry_rect(g), Some((10, 20, 1000, 760)));
    }

    #[test]
    fn a_single_recent_file_is_a_one_item_list() {
        let v = parse("[General]\nrecent=/home/a/only.md\n");
        assert_eq!(v["recent"].as_list(), vec!["/home/a/only.md".to_string()]);
        assert!(parse("recent=@Invalid()\n")["recent"].as_list().is_empty());
    }
}

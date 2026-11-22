//! Spell checking with Hunspell as a local subprocess, as in NIRUNOTE 0.7:
//! the user-local dictionary in `~/.local/share/nirunote/dictionaries`
//! first, then the distribution's; a personal word list (`personal.txt`);
//! unknown words are checked in one batch in a worker thread and painted
//! from the cache.
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

pub const SYSTEM_DIRS: [&str; 3] = [
    "/usr/share/hunspell",
    "/usr/share/myspell/dicts",
    "/usr/share/myspell",
];

pub struct Dictionary {
    pub base: String,
    exe: PathBuf,
    personal: Option<PathBuf>,
    pub personal_words: Mutex<HashSet<String>>,
    pub cache: Mutex<HashMap<String, bool>>,
}

pub fn hunspell() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join("hunspell"))
        .find(|p| p.is_file())
}

fn run(exe: &Path, args: &[&str], input: &str, timeout_ms: u64) -> Option<String> {
    use std::io::Write;
    let mut child = Command::new(exe)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdin = child.stdin.take()?;
    let input = input.to_string();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(input.as_bytes());
    });
    let mut stdout = child.stdout.take()?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        use std::io::Read;
        let mut s = String::new();
        let _ = stdout.read_to_string(&mut s);
        let _ = tx.send(s);
    });
    let out = rx
        .recv_timeout(std::time::Duration::from_millis(timeout_ms))
        .ok();
    if out.is_none() {
        let _ = child.kill();
    }
    let _ = child.wait();
    let _ = writer.join();
    out
}

impl Dictionary {
    pub fn open(base: &Path, personal: Option<&Path>) -> Option<Dictionary> {
        let exe = hunspell()?;
        let b = base.to_string_lossy().into_owned();
        if !Path::new(&format!("{b}.dic")).is_file() || !Path::new(&format!("{b}.aff")).is_file() {
            return None;
        }
        let mut words = HashSet::new();
        if let Some(p) = personal {
            if let Ok(t) = std::fs::read_to_string(p) {
                words = t
                    .lines()
                    .map(|w| w.trim().to_lowercase())
                    .filter(|w| !w.is_empty())
                    .collect();
            }
        }
        Some(Dictionary {
            base: b,
            exe,
            personal: personal.map(Path::to_path_buf),
            personal_words: Mutex::new(words),
            cache: Mutex::new(HashMap::new()),
        })
    }

    /// Check one word now (the probe and the context menu).
    pub fn check(&self, word: &str) -> bool {
        let key = word.to_lowercase();
        if self.personal_words.lock().unwrap().contains(&key) {
            return true;
        }
        if let Some(v) = self.cache.lock().unwrap().get(&key) {
            return *v;
        }
        let ok = run(
            &self.exe,
            &["-d", &self.base, "-l"],
            &format!("{word}\n"),
            1500,
        )
        .map(|o| o.trim().is_empty())
        .unwrap_or(true);
        let mut c = self.cache.lock().unwrap();
        if c.len() > 12000 {
            c.clear();
        }
        c.insert(key, ok);
        ok
    }

    /// The cached answer only: painting never waits for Hunspell.
    pub fn cached(&self, word: &str) -> Option<bool> {
        let key = word.to_lowercase();
        if self.personal_words.lock().unwrap().contains(&key) {
            return Some(true);
        }
        self.cache.lock().unwrap().get(&key).copied()
    }

    /// Resolve all uncached words in one Hunspell run (worker thread).
    pub fn check_many(&self, words: &HashSet<String>) {
        let pending: HashMap<String, String> = {
            let personal = self.personal_words.lock().unwrap();
            let cache = self.cache.lock().unwrap();
            words
                .iter()
                .filter(|w| !w.is_empty())
                .map(|w| (w.to_lowercase(), w.clone()))
                .filter(|(k, _)| !personal.contains(k) && !cache.contains_key(k))
                .collect()
        };
        if pending.is_empty() {
            return;
        }
        let input: String = pending.values().map(|w| format!("{w}\n")).collect();
        let out = run(&self.exe, &["-d", &self.base, "-l"], &input, 4000);
        let mut c = self.cache.lock().unwrap();
        match out {
            Some(o) => {
                let bad: HashSet<String> = o
                    .lines()
                    .map(|w| w.trim().to_lowercase())
                    .filter(|w| !w.is_empty())
                    .collect();
                for k in pending.keys() {
                    c.insert(k.clone(), !bad.contains(k));
                }
            }
            None => {
                // Fail open: spelling must never interrupt writing.
                for k in pending.keys() {
                    c.insert(k.clone(), true);
                }
            }
        }
    }

    pub fn add_word(&self, word: &str) {
        let w = word.trim();
        let Some(p) = &self.personal else { return };
        if w.is_empty() {
            return;
        }
        let key = w.to_lowercase();
        let mut words = self.personal_words.lock().unwrap();
        if !words.contains(&key) {
            if let Some(d) = p.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(p)
            {
                let _ = writeln!(f, "{w}");
            }
            words.insert(key.clone());
        }
        self.cache.lock().unwrap().remove(&key);
    }

    pub fn ignore(&self, word: &str) {
        let key = word.to_lowercase();
        self.personal_words.lock().unwrap().insert(key.clone());
        self.cache.lock().unwrap().remove(&key);
    }

    pub fn suggest(&self, word: &str) -> Vec<String> {
        let Some(out) = run(
            &self.exe,
            &["-d", &self.base, "-a"],
            &format!("{word}\n"),
            2000,
        ) else {
            return Vec::new();
        };
        for line in out.lines() {
            if line.starts_with('&') {
                if let Some((_, rest)) = line.split_once(':') {
                    return rest
                        .split(',')
                        .map(|x| x.trim().to_string())
                        .filter(|x| !x.is_empty())
                        .collect();
                }
            }
            if line.starts_with('#') {
                return Vec::new();
            }
        }
        Vec::new()
    }

    /// "…/sv_SE" -> "sv".
    pub fn language(&self) -> String {
        Path::new(&self.base)
            .file_name()
            .map(|n| {
                n.to_string_lossy()
                    .split('_')
                    .next()
                    .unwrap_or("")
                    .to_lowercase()
            })
            .unwrap_or_default()
    }
}

pub fn candidates(name: &str, user_dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let dirs = std::iter::once(user_dir.to_path_buf()).chain(SYSTEM_DIRS.iter().map(PathBuf::from));
    for d in dirs {
        let base = d.join(name);
        let b = base.to_string_lossy().into_owned();
        if Path::new(&format!("{b}.dic")).is_file() && Path::new(&format!("{b}.aff")).is_file() {
            found.push(base);
        }
    }
    found
}

pub fn unavailable_hint(names: &[&str], user_dir: &Path) -> String {
    if hunspell().is_none() {
        return "install hunspell".into();
    }
    for name in names {
        if candidates(name, user_dir).is_empty() {
            let pkg = match *name {
                "sv_SE" => "hunspell-sv",
                "en_US" => "hunspell-en-us",
                _ => "a hunspell dictionary",
            };
            return format!("install {pkg}");
        }
    }
    "dictionary check failed".into()
}

/// Find a working dictionary for the language setting (worker thread).
pub fn probe(language: &str) -> (Option<Arc<Dictionary>>, String) {
    let root = crate::files::dictionary_dir();
    let names: Vec<&str> = if language == "auto" {
        vec!["sv_SE", "en_US"]
    } else {
        vec![language]
    };
    for name in &names {
        let mut bases = candidates(name, &root);
        if bases.is_empty() {
            bases.push(root.join(name));
        }
        for base in bases {
            if let Some(d) = Dictionary::open(&base, Some(&root.join("personal.txt"))) {
                let check = if *name == "sv_SE" { "Hej" } else { "Hello" };
                if d.check(check) && !d.check("asdasdasd") {
                    return (Some(Arc::new(d)), String::new());
                }
            }
        }
    }
    (None, unavailable_hint(&names, &root))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    Off,
    Starting,
    Ready,
    Unavailable,
}

pub fn status_text(
    enabled: bool,
    state: State,
    dict: Option<&Dictionary>,
    hint: &str,
    large: bool,
) -> String {
    if !enabled || state == State::Off {
        return "Spelling: off".into();
    }
    if large {
        return "Spelling: paused (large document)".into();
    }
    match (state, dict) {
        (State::Ready, Some(d)) => format!("Spelling: {}", d.language()),
        (State::Starting, _) => "Spelling: starting…".into(),
        _ => {
            if hint.is_empty() {
                "Spelling: unavailable".into()
            } else {
                format!("Spelling: unavailable ({hint})")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_lines_say_what_is_missing() {
        assert_eq!(
            status_text(false, State::Off, None, "", false),
            "Spelling: off"
        );
        assert_eq!(
            status_text(true, State::Unavailable, None, "install hunspell-sv", false),
            "Spelling: unavailable (install hunspell-sv)"
        );
        assert_eq!(
            status_text(true, State::Starting, None, "", true),
            "Spelling: paused (large document)"
        );
    }
}

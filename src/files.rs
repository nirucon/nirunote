//! Files: loading with BOM and line-ending detection, atomic saves that keep
//! both, the save snapshots, crash-recovery snapshots (`recovery.json`) and
//! the Quick Open scan. File formats and locations are those of NIRUNOTE 0.7.
use std::path::{Path, PathBuf};

pub const MAX_LOAD_BYTES: u64 = 20 * 1024 * 1024;

pub fn state_dir() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| crate::theme::home().join(".local/state"))
        .join("nirunote")
}

pub fn dictionary_dir() -> PathBuf {
    crate::theme::home().join(".local/share/nirunote/dictionaries")
}

/// A loaded text and how it was stored.
pub struct Loaded {
    pub text: String,
    pub bom: bool,
    pub crlf: bool,
}

pub fn load(path: &Path) -> Result<Loaded, String> {
    let meta = std::fs::metadata(path).map_err(|e| describe(&e, path))?;
    if meta.len() > MAX_LOAD_BYTES {
        return Err(format!("Document is too large ({} bytes).", meta.len()));
    }
    let raw = std::fs::read(path).map_err(|e| describe(&e, path))?;
    let bom = raw.starts_with(b"\xef\xbb\xbf");
    let body = if bom { &raw[3..] } else { &raw[..] };
    let text = std::str::from_utf8(body)
        .map_err(|e| {
            format!(
                "'utf-8' codec can't decode the file at byte {}: it is not UTF-8 text.",
                e.valid_up_to() + if bom { 3 } else { 0 }
            )
        })?
        .to_string();
    let crlf = raw.windows(2).any(|w| w == b"\r\n");
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    Ok(Loaded { text, bom, crlf })
}

fn describe(e: &std::io::Error, path: &Path) -> String {
    format!("{}: {}", path.display(), e)
}

/// Write `data` next to `path` and rename it over: never a half file.
pub fn atomic_write_bytes(path: &Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let target = if path.is_symlink() {
        std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    } else {
        path.to_path_buf()
    };
    let dir = target
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    std::fs::create_dir_all(&dir)?;
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut n = 0u32;
    let (tmp, mut f) = loop {
        let tmp = dir.join(format!(".{name}.{}{n}.tmp", std::process::id()));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
        {
            Ok(f) => break (tmp, f),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && n < 100 => n += 1,
            Err(e) => return Err(e),
        }
    };
    let result = (|| {
        // Keep the permissions of the file being replaced.
        if let Ok(meta) = std::fs::metadata(&target) {
            let _ = f.set_permissions(meta.permissions());
        }
        f.write_all(data)?;
        f.flush()?;
        f.sync_all()?;
        std::fs::rename(&tmp, &target)?;
        if let Ok(d) = std::fs::File::open(&dir) {
            let _ = d.sync_all();
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Save a document: the line endings and BOM it had when it was opened.
pub fn save_text(path: &Path, text: &str, bom: bool, crlf: bool) -> Result<(), String> {
    let target = if path.is_symlink() {
        std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    } else {
        path.to_path_buf()
    };
    if target.exists() && !writable(&target) {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        return Err(format!("{name} is read-only"));
    }
    let mut payload = text.replace("\r\n", "\n").replace('\r', "\n");
    if crlf {
        payload = payload.replace('\n', "\r\n");
    }
    let mut data = Vec::with_capacity(payload.len() + 3);
    if bom {
        data.extend_from_slice(b"\xef\xbb\xbf");
    }
    data.extend_from_slice(payload.as_bytes());
    atomic_write_bytes(path, &data).map_err(|e| e.to_string())
}

pub fn writable(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(c) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    unsafe { libc::access(c.as_ptr(), libc::W_OK) == 0 }
}

/// Before each save the text goes to `snapshots/`; the newest five stay.
pub fn snapshot(text: &str, path: Option<&Path>) {
    if text.trim().is_empty() {
        return;
    }
    let dir = state_dir().join("snapshots");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let stem: String = path
        .and_then(|p| p.file_stem())
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "untitled".into())
        .chars()
        .take(40)
        .collect();
    let q = dir.join(format!("{stem}-{}.md", local_time("%Y%m%d-%H%M%S")));
    if std::fs::write(&q, text).is_err() {
        return;
    }
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "md"))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    files.sort_by_key(|a| std::cmp::Reverse(a.0));
    for (_, old) in files.into_iter().skip(5) {
        let _ = std::fs::remove_file(old);
    }
}

/// strftime in local time.
pub fn local_time(fmt: &str) -> String {
    format_time(fmt, now_secs())
}

pub fn now_secs() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

pub fn format_time(fmt: &str, secs: f64) -> String {
    let t = secs as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&t, &mut tm) };
    let Ok(f) = std::ffi::CString::new(fmt) else {
        return String::new();
    };
    let mut buf = [0u8; 128];
    let n = unsafe {
        libc::strftime(
            buf.as_mut_ptr() as *mut libc::c_char,
            buf.len(),
            f.as_ptr(),
            &tm,
        )
    };
    String::from_utf8_lossy(&buf[..n]).into_owned()
}

// --------------------------------------------------------------- recovery

pub struct Snapshot {
    pub text: String,
    pub path: Option<String>,
    pub saved_at: f64,
    pub pid: Option<i64>,
}

pub fn recovery_json(text: &str, path: Option<&str>, with_pid: bool) -> String {
    let mut m = serde_json::Map::new();
    m.insert("text".into(), text.into());
    m.insert(
        "path".into(),
        path.map_or(serde_json::Value::Null, |p| p.into()),
    );
    m.insert(
        "saved_at".into(),
        serde_json::Number::from_f64(now_secs())
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
    );
    m.insert("app_version".into(), env!("CARGO_PKG_VERSION").into());
    if with_pid {
        m.insert("pid".into(), (std::process::id() as i64).into());
    }
    serde_json::Value::Object(m).to_string()
}

/// The snapshot at `path`: Ok(None) when there is none or it is empty,
/// Err when the file exists but cannot be read (it is moved aside).
pub fn read_snapshot(path: &Path) -> Result<Option<Snapshot>, String> {
    let meta = match std::fs::metadata(path) {
        Ok(m) if m.is_file() => m,
        _ => return Ok(None),
    };
    if meta.len() == 0 {
        return Ok(None);
    }
    let raw = std::fs::read(path).map_err(|e| e.to_string())?;
    let text = String::from_utf8(raw).map_err(|e| e.to_string())?;
    let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let obj = v.as_object().ok_or("unexpected recovery format")?;
    let t = match obj.get("text") {
        None => String::new(),
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(_) => return Err("unexpected recovery format".into()),
    };
    if t.is_empty() {
        return Ok(None);
    }
    let saved_at = obj
        .get("saved_at")
        .and_then(|x| x.as_f64())
        .unwrap_or_else(|| {
            meta.modified()
                .ok()
                .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0.0, |d| d.as_secs_f64())
        });
    Ok(Some(Snapshot {
        text: t,
        path: obj
            .get("path")
            .and_then(|p| p.as_str())
            .filter(|p| !p.is_empty())
            .map(str::to_string),
        saved_at,
        pid: obj.get("pid").and_then(|p| p.as_i64()),
    }))
}

/// The snapshot was written by another NIRUNOTE that still runs.
pub fn owner_alive(s: &Snapshot) -> bool {
    let Some(pid) = s.pid.filter(|p| *p > 0) else {
        return false;
    };
    if pid == std::process::id() as i64 {
        return false;
    }
    std::fs::read(format!("/proc/{pid}/cmdline"))
        .map(|c| {
            String::from_utf8_lossy(&c)
                .to_lowercase()
                .contains("nirunote")
        })
        .unwrap_or(false)
}

/// The text of the original file, or None when it is missing or unreadable.
pub fn disk_text(path: Option<&str>) -> Option<String> {
    let raw = std::fs::read(path?).ok()?;
    let body = raw.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&raw);
    let t = String::from_utf8(body.to_vec()).ok()?;
    Some(t.replace("\r\n", "\n").replace('\r', "\n"))
}

/// (file label, time label, size label) for the recovery prompt.
pub fn describe_snapshot(s: &Snapshot, on_disk: Option<&str>) -> (String, String, String) {
    let name = s
        .path
        .as_deref()
        .and_then(|p| Path::new(p).file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Untitled (never saved)".into());
    let when = format_time("%Y-%m-%d %H:%M", s.saved_at);
    let chars = s.text.chars().count() as i64;
    let size = match on_disk {
        None => format!(
            "{chars} characters{}",
            if s.path.is_some() {
                " · the file is missing on disk"
            } else {
                ""
            }
        ),
        Some(d) if d == s.text => format!("{chars} characters · identical to the file on disk"),
        Some(d) => {
            let delta = chars - d.chars().count() as i64;
            format!("{chars} characters · {delta:+} compared with the file on disk")
        }
    };
    (name, when, size)
}

/// Align two texts line by line: (left, right, marks) where a mark is
/// ' ' equal, '~' changed, '-' only left, '+' only right.
pub fn side_by_side(left: &str, right: &str) -> (Vec<String>, Vec<String>, Vec<char>) {
    let a: Vec<&str> = left.split('\n').collect();
    let b: Vec<&str> = right.split('\n').collect();
    let (mut l, mut r, mut marks) = (Vec::new(), Vec::new(), Vec::new());
    for (tag, i1, i2, j1, j2) in opcodes(&a, &b) {
        if tag == ' ' {
            l.extend(a[i1..i2].iter().map(|s| s.to_string()));
            r.extend(b[j1..j2].iter().map(|s| s.to_string()));
            marks.extend(std::iter::repeat_n(' ', i2 - i1));
            continue;
        }
        for k in 0..(i2 - i1).max(j2 - j1) {
            let hl = i1 + k < i2;
            let hr = j1 + k < j2;
            l.push(if hl {
                a[i1 + k].to_string()
            } else {
                String::new()
            });
            r.push(if hr {
                b[j1 + k].to_string()
            } else {
                String::new()
            });
            marks.push(if hl && hr {
                '~'
            } else if hl {
                '-'
            } else {
                '+'
            });
        }
    }
    (l, r, marks)
}

/// Matching blocks of two line lists (longest common subsequence),
/// turned into opcodes like difflib's: (' ' or '!', i1, i2, j1, j2).
fn opcodes(a: &[&str], b: &[&str]) -> Vec<(char, usize, usize, usize, usize)> {
    let (n, m) = (a.len(), b.len());
    // Trim the common head and tail first: the usual case is a small edit.
    let mut head = 0;
    while head < n && head < m && a[head] == b[head] {
        head += 1;
    }
    let mut tail = 0;
    while tail < n - head && tail < m - head && a[n - 1 - tail] == b[m - 1 - tail] {
        tail += 1;
    }
    let (ai, bi) = (&a[head..n - tail], &b[head..m - tail]);
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    if !ai.is_empty() && !bi.is_empty() && ai.len() * bi.len() <= 4_000_000 {
        let (p, q) = (ai.len(), bi.len());
        let mut dp = vec![0u32; (p + 1) * (q + 1)];
        for i in (0..p).rev() {
            for j in (0..q).rev() {
                dp[i * (q + 1) + j] = if ai[i] == bi[j] {
                    dp[(i + 1) * (q + 1) + j + 1] + 1
                } else {
                    dp[(i + 1) * (q + 1) + j].max(dp[i * (q + 1) + j + 1])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < p && j < q {
            if ai[i] == bi[j] {
                pairs.push((i + head, j + head));
                i += 1;
                j += 1;
            } else if dp[(i + 1) * (q + 1) + j] >= dp[i * (q + 1) + j + 1] {
                i += 1;
            } else {
                j += 1;
            }
        }
    }
    let mut all: Vec<(usize, usize)> = (0..head).map(|k| (k, k)).collect();
    all.extend(pairs);
    all.extend((0..tail).map(|k| (n - tail + k, m - tail + k)));
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    let mut k = 0;
    while k < all.len() {
        let (pi, pj) = all[k];
        if pi > i || pj > j {
            out.push(('!', i, pi, j, pj));
        }
        let mut e = k;
        while e + 1 < all.len() && all[e + 1] == (all[e].0 + 1, all[e].1 + 1) {
            e += 1;
        }
        out.push((' ', pi, all[e].0 + 1, pj, all[e].1 + 1));
        i = all[e].0 + 1;
        j = all[e].1 + 1;
        k = e + 1;
    }
    if i < n || j < m {
        out.push(('!', i, n, j, m));
    }
    out
}

// ------------------------------------------------------------ quick open

/// Markdown and text files under `root` (at most 1500), for Quick Open.
pub fn scan_notes(root: &Path, cancel: &std::sync::atomic::AtomicBool) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut entries: Vec<_> = rd.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        let mut subdirs = Vec::new();
        for e in entries {
            let Ok(ft) = e.file_type() else { continue };
            let p = e.path();
            if ft.is_dir() {
                subdirs.push(p);
            } else if (ft.is_file() || (ft.is_symlink() && p.is_file()))
                && p.extension().is_some_and(|x| {
                    let x = x.to_string_lossy().to_lowercase();
                    x == "md" || x == "markdown" || x == "txt"
                })
            {
                found.push(p);
                if found.len() >= 1500 {
                    return found;
                }
            }
        }
        subdirs.reverse();
        stack.extend(subdirs);
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saving_keeps_bom_and_crlf_and_replaces_atomically() {
        let dir = std::env::temp_dir().join(format!("nirunote-files-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("a.md");
        std::fs::write(&p, b"\xef\xbb\xbfone\r\ntwo\r\n").unwrap();
        let l = load(&p).unwrap();
        assert!(l.bom && l.crlf);
        assert_eq!(l.text, "one\ntwo\n");
        save_text(&p, "one\nthree\n", l.bom, l.crlf).unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"\xef\xbb\xbfone\r\nthree\r\n");
        std::fs::write(dir.join("bad.md"), b"\xff\xfe").unwrap();
        assert!(load(&dir.join("bad.md")).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn compare_marks_changed_and_added_lines() {
        let (l, r, marks) = side_by_side("a\nb\nc", "a\nB\nc\nd");
        assert_eq!(marks, vec![' ', '~', ' ', '+']);
        assert_eq!(r.last().unwrap(), "d");
        assert_eq!(l.last().unwrap(), "");
    }

    #[test]
    fn snapshots_round_trip_and_describe() {
        let dir = std::env::temp_dir().join(format!("nirunote-rec-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("recovery.json");
        std::fs::write(&p, recovery_json("hello\n", Some("/x/crash.md"), true)).unwrap();
        let s = read_snapshot(&p).unwrap().unwrap();
        assert_eq!(s.text, "hello\n");
        assert!(!owner_alive(&s), "our own pid does not count");
        let (name, _, size) = describe_snapshot(&s, Some("hi"));
        assert_eq!(name, "crash.md");
        assert!(size.contains("+4 compared"), "{size}");
        std::fs::write(&p, "{not json").unwrap();
        assert!(read_snapshot(&p).is_err());
        std::fs::write(&p, "{\"text\": \"\", \"path\": null}").unwrap();
        assert!(read_snapshot(&p).unwrap().is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }
}

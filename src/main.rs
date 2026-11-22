//! NIRUNOTE: a minimal, keyboard-oriented Markdown editor with a styled
//! Preview, for the NIRU suite.
mod app;
mod canvas;
mod codehl;
mod dialogs;
mod doc;
mod editor;
mod files;
mod fonts;
mod images;
mod logo;
mod markdown;
mod preview;
mod settings;
mod spell;
mod theme;
mod ui;
mod x;

const USAGE: &str = "usage: nirunote [OPTIONS] [FILE]

  FILE                 open FILE (also: --open FILE)
  --read-only          open a viewer: starts in Preview, cannot edit or save
  --heading TEXT       scroll to the heading TEXT (its title or #anchor slug)
  --preview            start in Preview (editable)
  --new                start with an empty document
  --focus              start in focus mode (full screen)
  --scratchpad         open the scratchpad instance
  --version            print the version
  --help               show this help";

#[derive(Debug, Default, PartialEq)]
pub struct Args {
    pub files: Vec<String>,
    pub open: Option<String>,
    pub heading: Option<String>,
    pub scratchpad: bool,
    pub new: bool,
    pub focus: bool,
    pub read_only: bool,
    pub preview: bool,
    pub help: bool,
}

/// Parse the command line; unknown options are ignored for compatibility.
pub fn parse_args(argv: &[String]) -> Result<Args, String> {
    let mut a = Args::default();
    let mut i = 0;
    let mut only_files = false;
    while i < argv.len() {
        let arg = &argv[i];
        i += 1;
        if only_files || !arg.starts_with('-') || arg == "-" {
            a.files.push(arg.clone());
            continue;
        }
        if arg == "--" {
            only_files = true;
            continue;
        }
        let (name, value) = match arg.split_once('=') {
            Some((n, v)) => (n, Some(v.to_string())),
            None => (arg.as_str(), None),
        };
        if name == "--open" || name == "--heading" {
            let v = match value {
                Some(v) => v,
                None => {
                    if i >= argv.len() {
                        return Err(format!("{name} needs a value"));
                    }
                    i += 1;
                    argv[i - 1].clone()
                }
            };
            if name == "--open" {
                a.open = Some(v);
            } else {
                a.heading = Some(v);
            }
            continue;
        }
        match arg.as_str() {
            "--help" | "-h" => a.help = true,
            "--scratchpad" => a.scratchpad = true,
            "--new" => a.new = true,
            "--focus" => a.focus = true,
            "--read-only" => a.read_only = true,
            "--preview" => a.preview = true,
            _ => {}
        }
    }
    Ok(a)
}

fn core_self_test() -> i32 {
    let check = || -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("nirunote-core-{}", std::process::id()));
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let p = dir.join("atomic.md");
        files::atomic_write_bytes(&p, b"alpha").map_err(|e| e.to_string())?;
        files::atomic_write_bytes(&p, b"beta").map_err(|e| e.to_string())?;
        if std::fs::read(&p).map_err(|e| e.to_string())? != b"beta" {
            return Err("atomic save mismatch".into());
        }
        let _ = std::fs::remove_dir_all(&dir);
        if theme::palette_for("unknown") != theme::palette_for("dark") {
            return Err("unknown theme fallback mismatch".into());
        }
        let h = |s: &str| markdown::render_markdown(s).html;
        if !h("[ok](https://example.test)").contains("href=\"https://example.test\"") {
            return Err("safe link missing".into());
        }
        for bad in [
            "[bad](javascript:alert(1))",
            "[bad](data:text/html,x)",
            "[bad](file:///etc/passwd)",
        ] {
            if h(bad).contains("href=") {
                return Err(format!("unsafe link remained: {bad}"));
            }
        }
        if h("<small onclick=\"x\">safe</small>").contains("<small onclick=") {
            return Err("attribute HTML remained".into());
        }
        Ok(())
    };
    match check() {
        Ok(()) => {
            println!("NIRUNOTE core self-test: OK");
            0
        }
        Err(e) => {
            eprintln!("NIRUNOTE core self-test: FAILED ({e})");
            1
        }
    }
}

fn spell_self_test() -> i32 {
    let base = files::dictionary_dir().join("sv_SE");
    match spell::Dictionary::open(&base, None) {
        Some(d) => {
            let good = d.check("Hej");
            let bad = !d.check("asdasdasd");
            if good && bad {
                println!("NIRUNOTE spell runtime self-test: OK (SV)");
                0
            } else {
                eprintln!(
                    "NIRUNOTE spell runtime self-test: FAILED (Hej={good}, asdasdasd_bad={bad})"
                );
                1
            }
        }
        None => {
            eprintln!(
                "NIRUNOTE spell runtime self-test: FAILED (hunspell or dictionary unavailable)"
            );
            1
        }
    }
}

fn main() {
    let started = std::time::Instant::now();
    let argv: Vec<String> = std::env::args().skip(1).collect();
    // Probes for installers and wrappers never open a window.
    if argv.iter().any(|a| a == "--spell-self-test") {
        std::process::exit(spell_self_test());
    }
    if argv.iter().any(|a| a == "--core-self-test") {
        std::process::exit(core_self_test());
    }
    if argv.iter().any(|a| a == "--version" || a == "-V") {
        println!("{} {}", app::APP, app::VERSION);
        return;
    }
    if argv.iter().any(|a| a == "--self-test") {
        match x::X::open(0, 0, 200, 100, "NIRUNOTE self-test") {
            Ok(_) => {
                println!("{} {}: GUI OK", app::APP, app::VERSION);
                return;
            }
            Err(e) => {
                eprintln!("{} {}: GUI FAILED ({e})", app::APP, app::VERSION);
                std::process::exit(1);
            }
        }
    }
    let a = match parse_args(&argv) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("nirunote: {e}");
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    };
    if a.help {
        println!("{USAGE}");
        return;
    }
    let opts = app::Options {
        read_only: a.read_only && !a.scratchpad,
        scratchpad: a.scratchpad,
        preview: a.preview,
        focus: a.focus,
        new: a.new,
        file: a.open.clone().or_else(|| a.files.first().cloned()),
        heading: a.heading.clone(),
    };
    match app::App::new(opts, started).and_then(|mut w| w.run_loop()) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("nirunote: {e}");
            // As the 0.x launcher did: the reason a window did not start.
            let dir = files::state_dir();
            let _ = std::fs::create_dir_all(&dir);
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(dir.join("launcher.log"))
            {
                use std::io::Write;
                let _ = writeln!(f, "{} ERROR: {e}", files::local_time("%Y-%m-%d %H:%M:%S"));
            }
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_command_line_of_nirunote_0_7() {
        let a = parse_args(&v(&[
            "notes.md",
            "--read-only",
            "--heading",
            "Key bindings",
        ]))
        .unwrap();
        assert_eq!(a.files, vec!["notes.md"]);
        assert!(a.read_only);
        assert_eq!(a.heading.as_deref(), Some("Key bindings"));
        let a = parse_args(&v(&[
            "--heading=lock",
            "--preview",
            "--open",
            "x.md",
            "--unknown",
        ]))
        .unwrap();
        assert_eq!(a.heading.as_deref(), Some("lock"));
        assert!(a.preview);
        assert_eq!(a.open.as_deref(), Some("x.md"));
        assert!(a.files.is_empty());
        assert_eq!(
            parse_args(&v(&["--", "--weird.md"])).unwrap().files,
            vec!["--weird.md"]
        );
        assert!(parse_args(&v(&["--heading"])).is_err());
        assert!(core_self_test() == 0);
    }
}

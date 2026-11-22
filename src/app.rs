//! The NIRUNOTE window: header, editor and Preview pages, find bar, status
//! line, menus and dialogs; files, recovery, spelling, theme and the event
//! loop (X, an eventfd for the workers, inotify for the open file and the
//! NIRUX theme; no timers while idle).
use crate::canvas::{Canvas, Rect};
use crate::dialogs::{
    ChooseMode, ChooseOut, Chooser, Compare, Filter, Form, FormOut, PickOut, Picker, Row,
};
use crate::doc::Pos;
use crate::editor::{Editor, SpellLookup, LARGE_DOCUMENT_CHARS};
use crate::files;
use crate::fonts::{Fonts, BOLD, MONO, SANS};
use crate::images::{wake_up, Images};
use crate::markdown::{self, LinkKind};
use crate::preview::{Ctx, Preview};
use crate::settings::{self, Store, Value};
use crate::spell::{self, Dictionary, State as SpellState};
use crate::theme::{self, Palette, FOLLOW};
use crate::ui::{label, FieldOut, Key, LineEdit, Menu, MenuItem, MenuOut, Mods, UI};
use crate::x::{Pointer, Res, X};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::protocol::Event;

pub const APP: &str = "NIRUNOTE";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const WIDTHS: [(&str, i32); 4] = [
    ("Narrow", 720),
    ("Normal", 900),
    ("Wide", 1100),
    ("Full width", 16_777_215),
];
const VIEW_EXTENSIONS: [&str; 6] = ["md", "markdown", "mdown", "mkd", "txt", ""];
const RECOVERY_DEBOUNCE: Duration = Duration::from_millis(2000);
const RECOVERY_MAX_DELAY: Duration = Duration::from_millis(10000);

#[derive(Default, Clone)]
pub struct Options {
    pub read_only: bool,
    pub scratchpad: bool,
    pub preview: bool,
    pub focus: bool,
    pub new: bool,
    pub file: Option<String>,
    pub heading: Option<String>,
}

/// Everything in Preferences, with the keys and defaults of NIRUNOTE 0.7.
#[derive(Clone)]
struct Prefs {
    theme: String,
    format: String,
    font_size: i64,
    autosave: bool,
    restore: bool,
    typewriter: bool,
    word_wrap: bool,
    highlight_line: bool,
    writing_width: String,
    spell_enabled: bool,
    spell_language: String,
    save_folder_mode: String,
    default_save_folder: String,
    last_save_folder: String,
    last_open_folder: String,
    restore_last_document: bool,
    show_reading_time: bool,
    remote_images: bool,
    last_document: String,
    recent: Vec<String>,
}

impl Prefs {
    fn load(s: &Store) -> Self {
        let home = theme::home();
        let docs = home.join("Documents").to_string_lossy().into_owned();
        let saved_theme = s.str_or("theme", "");
        Prefs {
            theme: if saved_theme.is_empty() {
                if theme::nirux_available() {
                    FOLLOW.into()
                } else {
                    "dark".into()
                }
            } else {
                saved_theme
            },
            format: s.str_or("format", "md"),
            font_size: s.int_or("font_size", 17).clamp(6, 72),
            autosave: s.bool_or("autosave", true),
            restore: s.bool_or("restore", true),
            typewriter: s.bool_or("typewriter", false),
            word_wrap: s.bool_or("word_wrap", true),
            highlight_line: s.bool_or("highlight_line", true),
            writing_width: s.str_or("writing_width", "Normal"),
            spell_enabled: s.bool_or("spell_enabled", true),
            spell_language: s.str_or("spell_language", "sv_SE"),
            save_folder_mode: s.str_or("save_folder_mode", "last"),
            default_save_folder: s.str_or("default_save_folder", &docs),
            last_save_folder: s.str_or("last_save_folder", &docs),
            last_open_folder: s.str_or("last_open_folder", &home.to_string_lossy()),
            restore_last_document: s.bool_or("restore_last_document", false),
            show_reading_time: s.bool_or("show_reading_time", false),
            remote_images: s.bool_or("remote_images", true),
            last_document: s.str_or("last_document", ""),
            recent: s.list("recent"),
        }
    }

    fn save(&self, s: &mut Store) {
        s.set_str("theme", &self.theme);
        s.set_str("format", &self.format);
        s.set_int("font_size", self.font_size);
        s.set_bool("autosave", self.autosave);
        s.set_bool("restore", self.restore);
        s.set_bool("typewriter", self.typewriter);
        s.set_bool("word_wrap", self.word_wrap);
        s.set_bool("highlight_line", self.highlight_line);
        s.set_str("writing_width", &self.writing_width);
        s.set_bool("spell_enabled", self.spell_enabled);
        s.set_str("spell_language", &self.spell_language);
        s.set_str("save_folder_mode", &self.save_folder_mode);
        s.set_str("default_save_folder", &self.default_save_folder);
        s.set_str("last_save_folder", &self.last_save_folder);
        s.set_str("last_open_folder", &self.last_open_folder);
        s.set_bool("restore_last_document", self.restore_last_document);
        s.set_bool("show_reading_time", self.show_reading_time);
        s.set_bool("remote_images", self.remote_images);
        s.set_str("last_document", &self.last_document);
    }
}

/// Commands of the menu, the palette and the keyboard.
#[derive(Clone, Debug, PartialEq)]
enum Cmd {
    New,
    NewFromClipboard,
    Open,
    OpenRecent(String),
    ClearRecent,
    Save,
    SaveAs,
    Rename,
    OpenFolder,
    CopyPath,
    Quit,
    Undo,
    Redo,
    SelectAll,
    CopyAll,
    PastePlain,
    DuplicateLine,
    Find,
    FindNext,
    FindPrev,
    Replace,
    Bold,
    Italic,
    Link,
    Checkbox,
    Bullet,
    Numbered,
    Quote,
    GotoHeading,
    Outline,
    Back,
    InsertDate,
    QuickOpen,
    Palette,
    TogglePreview,
    Focus,
    Float,
    Wrap,
    LineHighlight,
    Width(String),
    Theme(String),
    FontUp,
    FontDown,
    FontReset,
    Prefs,
    About,
    // Context menu
    Cut,
    Copy,
    Paste,
    Delete,
    Suggest(Pos, Pos, String),
    IgnoreWord(String),
    AddWord(String),
}

/// What to do once unsaved changes are saved or discarded.
#[derive(Clone, Debug)]
enum After {
    New,
    NewFromClipboard,
    Open,
    Load(PathBuf),
    Quit,
    Follow(PathBuf, String),
    Back(Option<String>, i32),
}

enum FormKind {
    Notice,
    Unsaved(After),
    External,
    Recovery,
    Prefs,
    Rename,
    Overwrite(PathBuf),
}

enum PickKind {
    Outline(Vec<usize>),
    Goto(Vec<usize>),
    Palette,
    QuickOpen(Arc<AtomicBool>),
}

enum ChooseKind {
    Open,
    SaveAs,
    PrefsFolder,
}

enum Overlay {
    Menu(Menu, Vec<Cmd>),
    Form(Form, FormKind),
    Picker(Picker, PickKind),
    Chooser(Chooser, ChooseKind),
    Compare(Compare),
}

enum Msg {
    Spell(u64, Option<Arc<Dictionary>>, String),
    SpellChecked,
    Scan(Vec<PathBuf>, PathBuf),
    Recovery(u64, bool),
    Render(u64, markdown::Rendered, u64),
}

#[derive(Clone, Copy, PartialEq)]
enum PasteTo {
    Editor,
    Find,
    Overlay,
    NewDoc,
}

enum ScrollFix {
    Anchor(String),
    Y(i32),
    Ratio(f32),
}

struct FindBar {
    visible: bool,
    replace: bool,
    q: LineEdit,
    r: LineEdit,
    case: bool,
    word: bool,
    focus: u8,
    hits: Vec<(Rect, u8)>,
}

#[derive(Clone, Copy, PartialEq)]
enum Drag {
    None,
    Editor,
    Preview,
    Bar(bool),
}

struct SpellLook<'a>(&'a Dictionary);

impl SpellLookup for SpellLook<'_> {
    fn lookup(&self, w: &str) -> Option<bool> {
        self.0.cached(w)
    }
}

pub struct App {
    x: X,
    cv: Canvas,
    fonts: Fonts,
    pal: Palette,
    store: Store,
    prefs: Prefs,
    ed: Editor,
    pv: Preview,
    images: Images,
    opts: Options,
    path: Option<PathBuf>,
    title: String,
    bom: bool,
    crlf: bool,
    previewing: bool,
    focus_mode: bool,
    floating: bool,
    recovered: bool,
    find: FindBar,
    find_focused: bool,
    overlays: Vec<Overlay>,
    flash: Option<(String, Instant)>,
    copy_fb: Option<Instant>,
    save_fb: Option<Instant>,
    spell_at: Option<Instant>,
    stats_at: Option<Instant>,
    preview_at: Option<Instant>,
    rec_debounce: Option<Instant>,
    rec_deadline: Option<Instant>,
    theme_at: Option<Instant>,
    stats: (usize, usize),
    // spelling
    spell_state: SpellState,
    spell_dict: Option<Arc<Dictionary>>,
    spell_hint: String,
    spell_gen: u64,
    spell_inflight: bool,
    spell_deferred: bool,
    // recovery
    text_gen: u64,
    rec_gen: u64,
    rec_pending: bool,
    rec_again: bool,
    rec_epoch: Arc<Mutex<u64>>,
    startup_recovery: Option<files::Snapshot>,
    // files
    inotify: i32,
    file_wd: i32,
    theme_wds: Vec<i32>,
    external_mtime: Option<std::time::SystemTime>,
    external_prompt: bool,
    removed_noticed: bool,
    after_save: Option<After>,
    // preview navigation
    nav: Vec<(Option<String>, i32)>,
    scroll_fix: Option<ScrollFix>,
    anchor_hold: Option<(String, i32)>,
    render_gen: u64,
    // workers
    wake: i32,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    // clipboard
    clip_text: String,
    primary_text: String,
    paste_to: Option<PasteTo>,
    incoming: Option<Vec<u8>>,
    drop_source: Option<Window>,
    // mouse
    drag: Drag,
    clicks: (Instant, u8, i32, i32),
    press_at: (i32, i32),
    hover_link: Option<String>,
    focus_link: Option<usize>,
    pointer: (i32, i32),
    // window
    scale: f32,
    geom: (i32, i32, i32, i32),
    win_focused: bool,
    dirty: bool,
    quit: bool,
    started: Instant,
    shown: bool,
    dead: Option<u32>,
    state_file: Option<PathBuf>,
}

fn wake_fd() -> i32 {
    unsafe { libc::eventfd(0, libc::EFD_NONBLOCK | libc::EFD_CLOEXEC) }
}

fn spawn_detached(cmd: &str, args: &[&str]) {
    use std::os::unix::process::CommandExt;
    let mut c = std::process::Command::new(cmd);
    c.args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    unsafe {
        c.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    if let Ok(mut child) = c.spawn() {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
        None => String::new(),
    }
}

fn width_px(name: &str) -> i32 {
    WIDTHS.iter().find(|w| w.0 == name).map_or(900, |w| w.1)
}

fn available_themes() -> Vec<String> {
    let mut v = vec!["dark".to_string(), "light".into(), "satie".into()];
    if theme::nirux_available() {
        v.push(FOLLOW.into());
    }
    v
}

fn theme_label(t: &str) -> String {
    if t == FOLLOW {
        t.to_string()
    } else {
        capitalize(t)
    }
}

impl App {
    pub fn new(opts: Options, started: Instant) -> Res<Self> {
        let store = Store::open(settings::settings_path());
        let prefs = Prefs::load(&store);
        let pal = theme::resolve(&prefs.theme);
        let (mut gx, mut gy, mut gw, mut gh) = (0, 0, 1000, 760);
        if let Some(Value::Bytes(b)) = store.get("geometry") {
            if let Some((x, y, w, h)) = settings::geometry_rect(b) {
                (gx, gy, gw, gh) = (x, y, w.max(560), h.max(400));
            }
        }
        let x = X::open(gx as i16, gy as i16, gw as u16, gh as u16, APP)?;
        let scale = x.xft_dpi().unwrap_or(96.0) / 96.0;
        let mut fonts = Fonts::new();
        let first_faces: &[u8] = if opts.read_only || opts.preview {
            &[MONO, SANS, SANS | BOLD]
        } else {
            &[MONO, MONO | BOLD]
        };
        fonts.preload(first_faces);
        let wake = wake_fd();
        let inotify = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
        let (tx, rx) = std::sync::mpsc::channel();
        let mut ed = Editor::new("");
        ed.pt = prefs.font_size as f32;
        ed.scale = scale * 96.0 / 72.0;
        ed.wrap = prefs.word_wrap;
        ed.read_only = opts.read_only;
        let remote = prefs.remote_images;
        let ext = prefs.format.clone();
        let mut app = App {
            cv: Canvas::new(gw, gh),
            x,
            fonts,
            pal,
            store,
            prefs,
            ed,
            pv: Preview::new(),
            images: Images::new(wake, remote),
            opts: opts.clone(),
            path: None,
            title: format!("Untitled.{ext}"),
            bom: false,
            crlf: false,
            previewing: false,
            focus_mode: false,
            floating: false,
            recovered: false,
            find: FindBar {
                visible: false,
                replace: false,
                q: LineEdit::new("", "Find"),
                r: LineEdit::new("", "Replace"),
                case: false,
                word: false,
                focus: 0,
                hits: Vec::new(),
            },
            find_focused: false,
            overlays: Vec::new(),
            flash: None,
            copy_fb: None,
            save_fb: None,
            spell_at: None,
            stats_at: None,
            preview_at: None,
            rec_debounce: None,
            rec_deadline: None,
            theme_at: None,
            stats: (0, 0),
            spell_state: SpellState::Off,
            spell_dict: None,
            spell_hint: String::new(),
            spell_gen: 0,
            spell_inflight: false,
            spell_deferred: true,
            text_gen: 0,
            rec_gen: 0,
            rec_pending: false,
            rec_again: false,
            rec_epoch: Arc::new(Mutex::new(0)),
            startup_recovery: None,
            inotify,
            file_wd: -1,
            theme_wds: Vec::new(),
            external_mtime: None,
            external_prompt: false,
            removed_noticed: false,
            after_save: None,
            nav: Vec::new(),
            scroll_fix: None,
            anchor_hold: None,
            render_gen: 0,
            wake,
            tx,
            rx,
            clip_text: String::new(),
            primary_text: String::new(),
            paste_to: None,
            incoming: None,
            drop_source: None,
            drag: Drag::None,
            clicks: (Instant::now() - Duration::from_secs(5), 0, -1, -1),
            press_at: (0, 0),
            hover_link: None,
            focus_link: None,
            pointer: (0, 0),
            scale,
            geom: (gx, gy, gw, gh),
            win_focused: true,
            dirty: true,
            quit: false,
            started,
            shown: false,
            dead: None,
            state_file: std::env::var_os("NIRUNOTE_STATE").map(PathBuf::from),
        };
        app.init_spell();
        app.configure_theme_watch();
        if app.opts.scratchpad {
            let _ = app.x.set_title("NIRUNOTE Scratchpad");
        }
        app.read_startup_recovery();
        app.start_document();
        app.x.set_pointer(Pointer::Text);
        app.x.map()?;
        Ok(app)
    }

    /// The command line's document, as NIRUNOTE 0.7 decided it.
    fn start_document(&mut self) {
        let o = self.opts.clone();
        let read_only = o.read_only && !o.scratchpad;
        let skip_session = o.scratchpad || o.new || o.file.is_some() || read_only;
        if !skip_session
            && self.prefs.restore_last_document
            && !self.prefs.last_document.is_empty()
            && Path::new(&self.prefs.last_document).is_file()
        {
            let p = PathBuf::from(&self.prefs.last_document);
            self.load(&p, true);
        }
        if o.scratchpad {
            let p = files::state_dir().join("scratchpad.md");
            if let Some(d) = p.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            if !p.exists() {
                let _ = std::fs::write(&p, "");
            }
            self.load(&p, false);
            let _ = self.x.set_title("NIRUNOTE Scratchpad");
        } else if let Some(f) = &o.file {
            let p = PathBuf::from(f);
            if p.is_file() {
                self.load(&p, false);
            } else if read_only {
                self.title = p
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| f.clone());
                self.flash_for(&format!("Not found: {f}"), 6000);
            }
        } else if o.new {
            self.reset_doc("", None);
        }
        if read_only || o.preview {
            self.set_previewing(true);
        }
        if let Some(h) = &o.heading {
            if self.path.is_some() {
                self.open_at_heading(h);
            }
        }
        self.update_stats();
    }

    fn ui_px(&self) -> f32 {
        13.0 * self.scale
    }

    fn s(&self, v: i32) -> i32 {
        (v as f32 * self.scale).round() as i32
    }

    /// For the end-to-end test: what the window shows, after every frame
    /// (only when NIRUNOTE_STATE names a file).
    fn dump_state(&self) {
        let Some(f) = &self.state_file else { return };
        let overlay = match self.overlays.last() {
            None => String::new(),
            Some(Overlay::Menu(..)) => "menu".into(),
            Some(Overlay::Form(f, _)) => format!("form:{}", f.title),
            Some(Overlay::Picker(p, _)) => format!("picker:{}", p.title),
            Some(Overlay::Chooser(c, _)) => format!("chooser:{}", c.title),
            Some(Overlay::Compare(_)) => "compare".into(),
        };
        let v = serde_json::json!({
            "title": self.title,
            "path": self.path.as_ref().map(|p| p.to_string_lossy().into_owned()),
            "modified": self.is_modified(),
            "previewing": self.previewing,
            "focus_mode": self.focus_mode,
            "read_only": self.opts.read_only,
            "overlay": overlay,
            "status_left": self.status_left(),
            "status_right": self.status_right(),
            "spell": self.spell_status(),
            "text": self.ed.doc.text(),
            "cursor": [self.ed.cursor.line, self.ed.cursor.col],
            "selection": self.ed.selected_text(),
            "preview_scroll": self.pv.scroll_y,
            "preview_selection": self.pv.selected_text(),
            "find": self.find.visible,
            "theme": self.prefs.theme,
            "font_size": self.prefs.font_size,
            "headings": self.pv.headings.len(),
        });
        let tmp = f.with_extension("tmp");
        if std::fs::write(&tmp, v.to_string()).is_ok() {
            let _ = std::fs::rename(&tmp, f);
        }
    }

    // ------------------------------------------------------------ geometry

    fn header_h(&self) -> i32 {
        if self.focus_mode {
            0
        } else {
            self.s(42)
        }
    }

    fn status_h(&self) -> i32 {
        if self.focus_mode {
            0
        } else {
            self.s(30)
        }
    }

    fn find_h(&self) -> i32 {
        if self.find.visible {
            self.s(42)
        } else {
            0
        }
    }

    /// The page (editor or Preview widget) inside the window.
    fn page_rect(&self) -> Rect {
        let top = self.header_h() + self.s(20);
        let bottom = self.cv.h - self.status_h() - self.find_h();
        let avail = self.cv.w - 2 * self.s(24);
        let maxw = width_px(&self.prefs.writing_width);
        let maxw = if maxw > 100_000 { maxw } else { self.s(maxw) };
        let w = avail.min(maxw).max(80);
        let x = (self.cv.w - w) / 2;
        Rect::new(x, top, w, (bottom - top).max(40))
    }

    /// The text area of the page (padding 30/18 plus the document margin).
    fn text_rect(&self) -> Rect {
        let p = self.page_rect();
        let (ph, pv) = (self.s(22), self.s(34));
        Rect::new(
            p.x + ph,
            p.y + pv,
            (p.w - 2 * ph).max(40),
            (p.h - pv - self.s(30)).max(20),
        )
    }

    // ------------------------------------------------------------ status

    fn flash_for(&mut self, text: &str, ms: u64) {
        self.flash = Some((text.to_string(), Instant::now() + Duration::from_millis(ms)));
        self.dirty = true;
    }

    fn flash_status(&mut self, text: &str) {
        self.flash_for(text, 1800);
    }

    fn is_modified(&self) -> bool {
        self.ed.doc.is_modified()
    }

    fn current_ext(&self) -> String {
        match &self.path {
            Some(p) => p
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_default(),
            None => self.prefs.format.clone(),
        }
    }

    fn update_stats(&mut self) {
        let text = self.ed.doc.text();
        self.stats = (text.split_whitespace().count(), text.chars().count());
        self.dirty = true;
    }

    fn status_left(&self) -> String {
        let (words, chars) = self.stats;
        let read = if self.prefs.show_reading_time && words > 0 {
            format!(" · {} min", words.div_ceil(220).max(1))
        } else {
            String::new()
        };
        let ro = match &self.path {
            Some(p) if !self.opts.read_only && p.exists() && !files::writable(p) => " · READ ONLY",
            _ => "",
        };
        let state = if self.opts.read_only {
            "Viewer (read-only)"
        } else if self.is_modified() {
            "Modified"
        } else {
            "Saved"
        };
        let th = if self.prefs.theme == FOLLOW {
            "NIRUX".to_string()
        } else {
            capitalize(&self.prefs.theme)
        };
        let kind = if matches!(self.current_ext().as_str(), "md" | "markdown") {
            "Markdown"
        } else {
            "Text"
        };
        format!(
            "{state} · {th} · {kind} · {words} words · {chars} chars{read}{ro}{}",
            if self.recovered { " · RECOVERED" } else { "" }
        )
    }

    fn spell_status(&self) -> String {
        spell::status_text(
            self.prefs.spell_enabled,
            self.spell_state,
            self.spell_dict.as_deref(),
            &self.spell_hint,
            self.stats.1 > LARGE_DOCUMENT_CHARS,
        )
    }

    fn status_right(&self) -> String {
        if let Some((t, until)) = &self.flash {
            if Instant::now() < *until {
                return t.clone();
            }
        }
        if self.previewing {
            return "Preview".into();
        }
        let c = self.ed.cursor;
        let col = self.ed.doc.lines[c.line][..c.col].chars().count() + 1;
        let mut s = format!("Ln {}, Col {col}", c.line + 1);
        if self.ed.has_selection() {
            let sel = self.ed.selected_text();
            let n = sel.split_whitespace().count();
            if !sel.trim().is_empty() {
                s.push_str(&format!(" · Selected {n} words"));
            }
        }
        s
    }

    // ------------------------------------------------------------ spelling

    fn init_spell(&mut self) {
        self.spell_gen += 1;
        let gen = self.spell_gen;
        self.spell_dict = None;
        self.spell_deferred = true;
        self.spell_hint.clear();
        if !self.prefs.spell_enabled {
            self.spell_state = SpellState::Off;
            self.spell_deferred = false;
            return;
        }
        self.spell_state = SpellState::Starting;
        let lang = self.prefs.spell_language.clone();
        let tx = self.tx.clone();
        let wake = self.wake;
        std::thread::spawn(move || {
            let (d, hint) = spell::probe(&lang);
            let _ = tx.send(Msg::Spell(gen, d, hint));
            wake_up(wake);
        });
    }

    fn spell_pass(&mut self) {
        if self.stats.1 <= LARGE_DOCUMENT_CHARS && self.prefs.spell_enabled {
            if let Some(d) = self.spell_dict.clone() {
                if !self.spell_inflight {
                    self.spell_inflight = true;
                    let words = self.ed.words();
                    let tx = self.tx.clone();
                    let wake = self.wake;
                    std::thread::spawn(move || {
                        d.check_many(&words);
                        let _ = tx.send(Msg::SpellChecked);
                        wake_up(wake);
                    });
                }
            }
        }
        self.spell_deferred = false;
        self.dirty = true;
    }

    // ------------------------------------------------------------ theme

    fn configure_theme_watch(&mut self) {
        for wd in self.theme_wds.drain(..) {
            unsafe { libc::inotify_rm_watch(self.inotify, wd) };
        }
        if self.prefs.theme != FOLLOW || self.inotify < 0 {
            return;
        }
        let p = theme::theme_paths();
        let mut dirs: Vec<PathBuf> = Vec::new();
        for f in [&p.appearance, &p.config, &p.kitty] {
            if let Some(d) = f.parent() {
                if d.is_dir() && !dirs.iter().any(|x| x == d) {
                    dirs.push(d.to_path_buf());
                }
            }
        }
        for d in dirs {
            if let Some(wd) = self.add_watch(&d) {
                self.theme_wds.push(wd);
            }
        }
    }

    fn add_watch(&self, dir: &Path) -> Option<i32> {
        use std::os::unix::ffi::OsStrExt;
        let c = std::ffi::CString::new(dir.as_os_str().as_bytes()).ok()?;
        let wd = unsafe {
            libc::inotify_add_watch(
                self.inotify,
                c.as_ptr(),
                libc::IN_CLOSE_WRITE
                    | libc::IN_MOVED_TO
                    | libc::IN_CREATE
                    | libc::IN_DELETE
                    | libc::IN_MOVED_FROM,
            )
        };
        (wd >= 0).then_some(wd)
    }

    fn apply_theme(&mut self) {
        self.pal = theme::resolve(&self.prefs.theme);
        self.ed.pt = self.prefs.font_size as f32;
        self.pv.theme_epoch += 1;
        self.pv.invalidate();
        self.dirty = true;
    }

    fn set_theme(&mut self, t: &str) {
        self.prefs.theme = t.to_string();
        self.save_settings();
        self.apply_theme();
        self.configure_theme_watch();
    }

    fn resize_font(&mut self, d: i64) {
        self.prefs.font_size = (self.prefs.font_size + d).clamp(10, 36);
        self.save_settings();
        self.apply_theme();
        self.fonts.trim();
    }

    fn save_settings(&mut self) {
        self.prefs.save(&mut self.store);
        self.store.sync();
    }

    // ------------------------------------------------------------ files

    fn watch_current(&mut self) {
        if self.file_wd >= 0 {
            unsafe { libc::inotify_rm_watch(self.inotify, self.file_wd) };
            self.file_wd = -1;
        }
        self.removed_noticed = false;
        let Some(p) = self.path.clone() else { return };
        if !p.exists() {
            return;
        }
        let target = std::fs::canonicalize(&p).unwrap_or(p.clone());
        if let Some(d) = target.parent() {
            self.file_wd = self.add_watch(d).unwrap_or(-1);
        }
        self.external_mtime = std::fs::metadata(&p).and_then(|m| m.modified()).ok();
    }

    fn add_recent(&mut self, p: &Path) {
        let r = std::fs::canonicalize(p)
            .unwrap_or_else(|_| p.to_path_buf())
            .to_string_lossy()
            .into_owned();
        self.prefs.recent.retain(|x| *x != r);
        self.prefs.recent.insert(0, r);
        self.prefs.recent.truncate(10);
        self.store
            .set("recent", Value::List(self.prefs.recent.clone()));
    }

    fn cursor_key(p: &Path) -> String {
        let r = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        format!("cursor/{}", r.to_string_lossy())
    }

    fn load(&mut self, p: &Path, quiet: bool) -> bool {
        match files::load(p) {
            Ok(l) => {
                self.bom = l.bom;
                self.crlf = l.crlf;
                self.ed.set_text(&l.text);
                self.path = Some(p.to_path_buf());
                self.title = p
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.recovered = false;
                self.watch_current();
                if !self.opts.read_only {
                    self.clear_recovery();
                    self.add_recent(p);
                    self.prefs.last_document = p.to_string_lossy().into_owned();
                    self.save_settings();
                }
                let pos = self.store.int_or(&Self::cursor_key(p), 0).max(0) as usize;
                let at = self.ed.doc.pos_from_utf16(pos);
                self.ed.set_cursor(at, false);
                self.ed.sync(&mut self.fonts);
                let tr = self.text_rect();
                self.ed.view_h = tr.h;
                self.ed.width = tr.w;
                self.ed.sync(&mut self.fonts);
                self.ed
                    .ensure_visible(&mut self.fonts, self.prefs.typewriter, tr.w);
                if self.opts.read_only {
                    let _ = self
                        .x
                        .set_title(&format!("{} · read-only · NIRUNOTE", self.title));
                }
                self.text_gen += 1;
                self.rec_gen = self.text_gen;
                self.update_stats();
                self.spell_at = Some(Instant::now() + Duration::from_millis(350));
                if self.previewing {
                    self.update_preview();
                }
                self.dirty = true;
                true
            }
            Err(e) => {
                if !quiet {
                    self.notice("Open failed", &e);
                }
                false
            }
        }
    }

    fn reset_doc(&mut self, text: &str, name: Option<&str>) {
        self.recovered = false;
        self.path = None;
        self.watch_current();
        self.ed.set_text(text);
        self.bom = false;
        self.crlf = false;
        self.title = name
            .map(str::to_string)
            .unwrap_or_else(|| format!("Untitled.{}", self.prefs.format));
        if !text.is_empty() {
            self.ed.doc.mark_unsaved();
        }
        self.text_gen += 1;
        if !text.is_empty() {
            self.schedule_recovery();
        }
        self.update_stats();
        if self.previewing {
            self.update_preview();
        }
        self.dirty = true;
    }

    fn suggested_filename(&self) -> String {
        let text = self.ed.doc.text();
        let heading = text.lines().find_map(|l| {
            let h = l.trim_start_matches('#');
            let n = l.len() - h.len();
            ((1..=6).contains(&n) && h.starts_with(|c: char| c.is_whitespace()))
                .then(|| h.trim().to_string())
                .filter(|t| !t.is_empty())
        });
        let base = heading.unwrap_or_else(|| {
            text.split_whitespace()
                .take(6)
                .collect::<Vec<_>>()
                .join(" ")
        });
        let base: String = base
            .chars()
            .filter(|c| !"\\/:*?\"<>|".contains(*c))
            .collect();
        let base = base.trim().trim_end_matches('.').to_string();
        let base: String = base.chars().take(80).collect();
        let base = if base.is_empty() {
            "Untitled".to_string()
        } else {
            base
        };
        format!(
            "{base}{}",
            if self.prefs.format == "md" {
                ".md"
            } else {
                ".txt"
            }
        )
    }

    fn save(&mut self) -> bool {
        if self.opts.read_only {
            self.flash_status("Read-only viewer: saving is off");
            return false;
        }
        let Some(p) = self.path.clone() else {
            self.save_as();
            return false;
        };
        let text = self.ed.doc.text();
        files::snapshot(&text, Some(&p));
        match files::save_text(&p, &text, self.bom, self.crlf) {
            Ok(()) => {
                self.ed.doc.mark_saved();
                self.recovered = false;
                self.clear_recovery();
                self.add_recent(&p);
                self.prefs.last_document = p.to_string_lossy().into_owned();
                if let Some(d) = p.parent() {
                    let d = std::fs::canonicalize(d).unwrap_or(d.to_path_buf());
                    self.prefs.last_save_folder = d.to_string_lossy().into_owned();
                }
                self.save_settings();
                self.watch_current();
                self.title = p
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.dirty = true;
                if let Some(a) = self.after_save.take() {
                    self.run_after(a);
                }
                true
            }
            Err(e) => {
                self.after_save = None;
                self.notice("Save failed", &e);
                false
            }
        }
    }

    fn save_as(&mut self) {
        if self.opts.read_only {
            self.flash_status("Read-only viewer: saving is off");
            return;
        }
        let base = PathBuf::from(expand(if self.prefs.save_folder_mode == "custom" {
            &self.prefs.default_save_folder
        } else {
            &self.prefs.last_save_folder
        }));
        let base = if base.is_dir() {
            base
        } else if theme::home().join("Documents").is_dir() {
            theme::home().join("Documents")
        } else {
            theme::home()
        };
        let name = self.suggested_filename();
        self.overlays.push(Overlay::Chooser(
            Chooser::new(ChooseMode::Save, "Save as", &base, &name),
            ChooseKind::SaveAs,
        ));
        self.dirty = true;
    }

    fn save_to(&mut self, mut p: PathBuf) {
        if p.extension().is_none() {
            let ext = if self.prefs.format == "md" {
                "md"
            } else {
                "txt"
            };
            let mut s = p.into_os_string();
            s.push(format!(".{ext}"));
            p = PathBuf::from(s);
        }
        let old = (self.path.clone(), self.title.clone());
        self.path = Some(p);
        if !self.save() {
            self.path = old.0;
            self.title = old.1;
            self.watch_current();
        }
    }

    fn notice(&mut self, title: &str, text: &str) {
        self.overlays.push(Overlay::Form(
            Form::new(title, vec![Row::Text(text.to_string())], &["OK"], 0, 0),
            FormKind::Notice,
        ));
        self.dirty = true;
    }

    /// Run `after` now, or once the unsaved changes are saved or discarded.
    fn maybe_save_then(&mut self, after: After) {
        if self.opts.read_only || !self.is_modified() {
            self.run_after(after);
            return;
        }
        let mut f = Form::new(
            "Unsaved changes",
            vec![Row::Text("Save changes before continuing?".into())],
            &["Don't Save", "Cancel", "Save"],
            1,
            1,
        );
        f.width = 420;
        self.overlays
            .push(Overlay::Form(f, FormKind::Unsaved(after)));
        self.dirty = true;
    }

    fn run_after(&mut self, a: After) {
        match a {
            After::New => {
                self.clear_recovery();
                self.reset_doc("", None);
            }
            After::NewFromClipboard => {
                self.clear_recovery();
                if self.x.owns(self.x.atoms.clipboard) {
                    let t = self.clip_text.clone();
                    self.reset_doc(&t, Some(&format!("Clipboard.{}", self.prefs.format)));
                } else {
                    self.paste_to = Some(PasteTo::NewDoc);
                    self.x.request_text(self.x.atoms.clipboard);
                }
            }
            After::Open => {
                let dir = PathBuf::from(&self.prefs.last_open_folder);
                let dir = if dir.is_dir() { dir } else { theme::home() };
                self.overlays.push(Overlay::Chooser(
                    Chooser::new(ChooseMode::Open, "Open text", &dir, ""),
                    ChooseKind::Open,
                ));
            }
            After::Load(p) => {
                self.load(&p, false);
            }
            After::Quit => {
                self.clear_recovery();
                self.quit = true;
            }
            After::Follow(p, frag) => {
                self.remember_position();
                if self.load(&p, false) {
                    if frag.is_empty() {
                        self.scroll_fix = Some(ScrollFix::Y(0));
                    } else {
                        self.scroll_fix = Some(ScrollFix::Anchor(frag));
                    }
                    self.update_preview();
                }
            }
            After::Back(path, y) => {
                if let Some(p) = path {
                    self.load(Path::new(&p), false);
                }
                self.scroll_fix = Some(ScrollFix::Y(y));
                self.update_preview();
            }
        }
        self.dirty = true;
    }

    fn rename(&mut self) {
        if self.opts.read_only {
            return;
        }
        let Some(p) = &self.path else {
            self.save_as();
            return;
        };
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut f = Form::new(
            "Rename",
            vec![Row::Field("New filename:".into(), LineEdit::new(&name, ""))],
            &["Cancel", "OK"],
            1,
            0,
        );
        f.focus_row(0);
        if let Some(Row::Field(_, e)) = f.rows.get_mut(0) {
            e.select_all();
        }
        self.overlays.push(Overlay::Form(f, FormKind::Rename));
        self.dirty = true;
    }

    fn do_rename(&mut self, n: &str) {
        let Some(p) = self.path.clone() else { return };
        if n.is_empty() {
            return;
        }
        let q = p.with_file_name(n);
        if q.exists() && q != p {
            self.notice("Rename failed", &format!("{n} already exists"));
            return;
        }
        match std::fs::rename(&p, &q) {
            Ok(()) => {
                self.path = Some(q.clone());
                self.title = n.to_string();
                self.add_recent(&q);
                self.store.sync();
                self.watch_current();
            }
            Err(e) => self.notice("Rename failed", &e.to_string()),
        }
    }

    fn notes_root(&self) -> PathBuf {
        let p = PathBuf::from(expand(if self.prefs.save_folder_mode == "custom" {
            &self.prefs.default_save_folder
        } else {
            &self.prefs.last_save_folder
        }));
        if p.is_dir() {
            p
        } else {
            theme::home()
        }
    }

    fn quick_open(&mut self) {
        let root = self.notes_root();
        let mut pk = Picker::new(
            "Quick Open",
            &format!("Search {}", root.display()),
            Vec::new(),
            Filter::Ranked,
        );
        pk.status = Some("Scanning…".into());
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, wake, c2, r2) = (self.tx.clone(), self.wake, cancel.clone(), root.clone());
        std::thread::spawn(move || {
            let found = files::scan_notes(&r2, &c2);
            let _ = tx.send(Msg::Scan(found, r2));
            wake_up(wake);
        });
        self.overlays
            .push(Overlay::Picker(pk, PickKind::QuickOpen(cancel)));
        self.dirty = true;
    }

    fn open_containing_folder(&self) {
        if let Some(d) = self.path.as_ref().and_then(|p| p.parent()) {
            let d = std::fs::canonicalize(d).unwrap_or(d.to_path_buf());
            spawn_detached("xdg-open", &[&d.to_string_lossy()]);
        }
    }

    // --------------------------------------------------------- recovery

    fn recovery_path(&self) -> PathBuf {
        files::state_dir().join(if self.opts.scratchpad {
            "scratchpad-recovery.json"
        } else {
            "recovery.json"
        })
    }

    fn discarded_path(&self) -> PathBuf {
        files::state_dir().join("recovery-discarded.json")
    }

    fn schedule_recovery(&mut self) {
        if !self.prefs.autosave || self.opts.read_only {
            return;
        }
        let now = Instant::now();
        self.rec_debounce = Some(now + RECOVERY_DEBOUNCE);
        if self.rec_deadline.is_none() {
            self.rec_deadline = Some(now + RECOVERY_MAX_DELAY);
        }
    }

    fn recover(&mut self, sync: bool) {
        self.rec_debounce = None;
        self.rec_deadline = None;
        if !(self.prefs.autosave && self.is_modified()) || self.text_gen == self.rec_gen {
            return;
        }
        if self.startup_recovery.is_some() || self.opts.read_only {
            return;
        }
        if self.rec_pending && !sync {
            self.rec_again = true;
            return;
        }
        let gen = self.text_gen;
        let epoch_now = *self.rec_epoch.lock().unwrap();
        let payload = files::recovery_json(
            &self.ed.doc.text(),
            self.path.as_ref().map(|p| p.to_string_lossy()).as_deref(),
            true,
        );
        let path = self.recovery_path();
        self.rec_gen = gen;
        let epoch = self.rec_epoch.clone();
        let write = move || -> bool {
            let e = epoch.lock().unwrap();
            if *e != epoch_now {
                return false;
            }
            files::atomic_write_bytes(&path, payload.as_bytes()).is_ok()
        };
        if sync {
            if !write() {
                self.rec_gen = u64::MAX;
            }
            return;
        }
        self.rec_pending = true;
        let (tx, wake) = (self.tx.clone(), self.wake);
        std::thread::spawn(move || {
            let ok = write();
            let _ = tx.send(Msg::Recovery(gen, ok));
            wake_up(wake);
        });
    }

    fn clear_recovery(&mut self) {
        self.rec_debounce = None;
        self.rec_deadline = None;
        self.rec_gen = self.text_gen;
        if self.startup_recovery.is_some() || self.opts.read_only {
            return;
        }
        let mut e = self.rec_epoch.lock().unwrap();
        *e += 1;
        let _ = std::fs::remove_file(self.recovery_path());
    }

    fn read_startup_recovery(&mut self) {
        if !self.prefs.restore || self.opts.read_only {
            return;
        }
        let path = self.recovery_path();
        match files::read_snapshot(&path) {
            Err(_) => {
                let _ = std::fs::rename(
                    &path,
                    path.with_file_name(format!(
                        "{}-unreadable.json",
                        path.file_stem()
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_default()
                    )),
                );
            }
            Ok(Some(s)) if !files::owner_alive(&s) => self.startup_recovery = Some(s),
            _ => {}
        }
    }

    fn offer_recovery(&mut self) {
        let Some(s) = &self.startup_recovery else {
            return;
        };
        let on_disk = files::disk_text(s.path.as_deref());
        if on_disk.as_deref() == Some(s.text.as_str()) {
            self.startup_recovery = None;
            self.move_recovery_aside();
            return;
        }
        let (name, when, size) = files::describe_snapshot(s, on_disk.as_deref());
        let f = Form::new(
            "Recover unsaved text",
            vec![
                Row::Text(
                    "NIRUNOTE found unsaved text from a session that did not close normally."
                        .into(),
                ),
                Row::Info("File".into(), name),
                Row::Info("Last snapshot".into(), when),
                Row::Info("Size".into(), size),
                Row::Text("Discard keeps one backup copy (recovery-discarded.json).".into()),
            ],
            &["Discard", "Compare…", "Restore"],
            2,
            2,
        );
        self.overlays.push(Overlay::Form(f, FormKind::Recovery));
        self.dirty = true;
    }

    fn move_recovery_aside(&self) {
        let _ = std::fs::rename(self.recovery_path(), self.discarded_path());
    }

    fn restore_snapshot(&mut self, s: files::Snapshot) {
        let known = s.path.as_deref().is_some_and(|p| Path::new(p).is_file());
        let label = if known {
            format!(
                "{} · recovered",
                Path::new(s.path.as_deref().unwrap())
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            )
        } else {
            "Recovered · unsaved".into()
        };
        self.reset_doc(&s.text, Some(&label));
        if known {
            self.path = s.path.map(PathBuf::from);
            self.watch_current();
        }
        self.recovered = true;
        self.rec_debounce = None;
        self.rec_deadline = None;
        self.rec_gen = self.text_gen;
        self.update_stats();
    }

    fn backup_discarded(&self) {
        let t = self.ed.doc.text();
        if t.is_empty() {
            return;
        }
        let payload = files::recovery_json(
            &t,
            self.path.as_ref().map(|p| p.to_string_lossy()).as_deref(),
            false,
        );
        let _ = files::atomic_write_bytes(&self.discarded_path(), payload.as_bytes());
    }

    // ------------------------------------------------------------ edits

    /// After every change of the text.
    fn changed(&mut self) {
        self.spell_deferred = true;
        self.text_gen += 1;
        self.schedule_recovery();
        let large = self.ed.doc.byte_len() > LARGE_DOCUMENT_CHARS;
        self.stats_at = Some(Instant::now() + Duration::from_millis(if large { 900 } else { 180 }));
        self.spell_at = Some(Instant::now() + Duration::from_millis(350));
        if self.previewing {
            self.preview_at = Some(Instant::now() + Duration::from_millis(140));
        }
        self.after_cursor();
        if !self.is_modified() {
            // Undo back to the saved text: the file on disk holds it.
            self.clear_recovery();
        }
    }

    fn after_cursor(&mut self) {
        self.ed.sync(&mut self.fonts);
        let tr = self.text_rect();
        self.ed
            .ensure_visible(&mut self.fonts, self.prefs.typewriter, tr.w);
        if self.ed.has_selection() {
            self.primary_text = self.ed.selected_text();
            self.x.own(self.x.atoms.primary);
        }
        self.dirty = true;
    }

    fn editable(&self) -> bool {
        !self.opts.read_only && !self.previewing
    }

    fn edit<F: FnOnce(&mut Editor)>(&mut self, f: F) {
        if !self.editable() {
            return;
        }
        let g = self.ed.doc.generation;
        f(&mut self.ed);
        if self.ed.doc.generation != g {
            self.changed();
        } else {
            self.after_cursor();
        }
    }

    fn wrap_markdown(&mut self, l: &str, r: &str) {
        let (l, r) = (l.to_string(), r.to_string());
        self.edit(|e| {
            let sel = e.selected_text();
            if !sel.is_empty() {
                e.insert(&format!("{l}{sel}{r}"));
            } else {
                e.insert(&format!("{l}{r}"));
                for _ in 0..r.chars().count() {
                    e.move_h(false, false, false);
                }
            }
        });
    }

    fn insert_link(&mut self) {
        self.edit(|e| {
            let sel = e.selected_text().replace('\n', " ");
            let sel = if sel.is_empty() {
                "text".to_string()
            } else {
                sel
            };
            e.insert(&format!("[{sel}](url)"));
            for _ in 0..4 {
                e.move_h(false, false, false);
            }
        });
    }

    fn prefix_line(&mut self, prefix: &str) {
        let prefix = prefix.to_string();
        self.edit(|e| {
            let (c, a) = (e.cursor, e.anchor);
            let l = c.line;
            e.replace_range(
                Pos::new(l, 0),
                Pos::new(l, 0),
                &prefix,
                crate::doc::Kind::Other,
            );
            let shift = |p: Pos| {
                if p.line == l {
                    Pos::new(l, p.col + prefix.len())
                } else {
                    p
                }
            };
            e.cursor = shift(c);
            e.anchor = shift(a);
        });
    }

    fn toggle_checkbox(&mut self) {
        self.edit(|e| {
            let l = e.cursor.line;
            let line = e.doc.lines[l].clone();
            let indent = line.len() - line.trim_start().len();
            let rest = &line[indent..];
            let marked = rest.len() >= 6
                && matches!(rest.as_bytes()[0], b'-' | b'*' | b'+')
                && rest[1..].starts_with(" [")
                && matches!(rest.as_bytes()[3], b' ' | b'x' | b'X')
                && rest[4..].starts_with("] ");
            let new = if marked {
                format!("{}{}", &line[..indent], &rest[6..])
            } else {
                format!("- [ ] {line}")
            };
            let end = Pos::new(l, line.len());
            e.replace_range(Pos::new(l, 0), end, &new, crate::doc::Kind::Other);
        });
    }

    fn duplicate_line(&mut self) {
        self.edit(|e| {
            let (c, a) = (e.cursor, e.anchor);
            let l = c.line;
            let line = e.doc.lines[l].clone();
            let end = Pos::new(l, line.len());
            e.replace_range(end, end, &format!("\n{line}"), crate::doc::Kind::Other);
            e.cursor = c;
            e.anchor = a;
        });
    }

    fn insert_text(&mut self, t: &str) {
        let t = t.to_string();
        self.edit(|e| e.insert(&t));
    }

    fn copy(&mut self, text: String) {
        if text.is_empty() {
            return;
        }
        self.clip_text = text;
        self.x.own(self.x.atoms.clipboard);
    }

    fn paste(&mut self, to: PasteTo) {
        if self.x.owns(self.x.atoms.clipboard) {
            let t = self.clip_text.clone();
            self.deliver_paste(to, &t);
            return;
        }
        self.paste_to = Some(to);
        self.x.request_text(self.x.atoms.clipboard);
    }

    fn paste_primary(&mut self) {
        if self.x.owns(self.x.atoms.primary) {
            let t = self.primary_text.clone();
            self.deliver_paste(PasteTo::Editor, &t);
            return;
        }
        self.paste_to = Some(PasteTo::Editor);
        self.x.request_text(self.x.atoms.primary);
    }

    fn deliver_paste(&mut self, to: PasteTo, text: &str) {
        match to {
            PasteTo::Editor => self.insert_text(text),
            PasteTo::Find => {
                if self.find.focus == 1 {
                    self.find.r.insert(text);
                } else {
                    self.find.q.insert(text);
                }
            }
            PasteTo::Overlay => match self.overlays.last_mut() {
                Some(Overlay::Form(f, _)) => f.paste(text),
                Some(Overlay::Picker(p, _)) => p.paste(text),
                Some(Overlay::Chooser(c, _)) => c.paste(text),
                _ => {}
            },
            PasteTo::NewDoc => {
                let name = format!("Clipboard.{}", self.prefs.format);
                self.reset_doc(text, Some(&name));
            }
        }
        self.dirty = true;
    }

    // ------------------------------------------------------------ find

    fn show_find(&mut self, replace: bool) {
        let replace = replace && !self.opts.read_only;
        self.find.visible = true;
        self.find.replace = replace;
        let sel = if self.previewing {
            self.pv.selected_text()
        } else {
            self.ed.selected_text()
        };
        if !sel.is_empty() && !sel.contains('\n') && sel.chars().count() < 200 {
            self.find.q.set(&sel);
        }
        self.find.q.select_all();
        self.find.focus = 0;
        self.find_focused = true;
        self.dirty = true;
    }

    fn hide_find(&mut self) {
        self.find.visible = false;
        self.find_focused = false;
        self.pv.hit = None;
        self.dirty = true;
    }

    fn matches_at(&self, hay: &[char], i: usize, needle: &[char]) -> bool {
        if i + needle.len() > hay.len() {
            return false;
        }
        let eq = |a: char, b: char| {
            if self.find.case {
                a == b
            } else {
                a.to_lowercase().eq(b.to_lowercase())
            }
        };
        if !hay[i..i + needle.len()]
            .iter()
            .zip(needle)
            .all(|(a, b)| eq(*a, *b))
        {
            return false;
        }
        if self.find.word {
            let before = i == 0 || !crate::doc::is_word(hay[i - 1]);
            let after =
                i + needle.len() >= hay.len() || !crate::doc::is_word(hay[i + needle.len()]);
            return before && after;
        }
        true
    }

    fn find_next(&mut self, back: bool) -> bool {
        let q = self.find.q.text.clone();
        if q.is_empty() {
            return false;
        }
        let needle: Vec<char> = q.chars().collect();
        if self.previewing {
            let (a, b) = (
                self.pv.sel.0.min(self.pv.sel.1),
                self.pv.sel.0.max(self.pv.sel.1),
            );
            let from = if back { a } else { b };
            let (case, word) = (self.find.case, self.find.word);
            let hit = self.pv.find(&q, from, back, case, word).or_else(|| {
                let r = self
                    .pv
                    .find(&q, if back { usize::MAX } else { 0 }, back, case, word);
                if r.is_some() {
                    self.flash_for("Search wrapped", 1000);
                }
                r
            });
            match hit {
                Some((a, b)) => {
                    self.pv.sel = (a, b);
                    self.pv.hit = Some((a, b));
                    if let Some(y) = self.pv.y_of(a) {
                        if y < self.pv.scroll_y || y > self.pv.scroll_y + self.pv.view_h - 40 {
                            self.pv.scroll_y = y - self.pv.view_h / 3;
                            self.pv.clamp();
                        }
                    }
                    self.dirty = true;
                    true
                }
                None => {
                    self.flash_status(&format!("Not found: {q}"));
                    false
                }
            }
        } else {
            let text: Vec<char> = self.ed.doc.text().chars().collect();
            let (sa, sb) = self.ed.selection();
            let from = self.ed.doc.char_offset(if back { sa } else { sb });
            let scan = |app: &App, from: usize, back: bool| -> Option<usize> {
                if back {
                    (0..from.min(text.len() + 1))
                        .rev()
                        .find(|i| app.matches_at(&text, *i, &needle))
                } else {
                    (from..text.len()).find(|i| app.matches_at(&text, *i, &needle))
                }
            };
            let mut hit = scan(self, from, back);
            if hit.is_none() {
                hit = scan(self, if back { text.len() } else { 0 }, back);
                if hit.is_some() {
                    self.flash_for("Search wrapped", 1000);
                }
            }
            match hit {
                Some(i) => {
                    let a = self.ed.doc.pos_from_char(i);
                    let b = self.ed.doc.pos_from_char(i + needle.len());
                    self.ed.select(a, b);
                    self.after_cursor();
                    true
                }
                None => {
                    self.flash_status(&format!("Not found: {q}"));
                    false
                }
            }
        }
    }

    fn replace_one(&mut self) {
        if !self.editable() {
            return;
        }
        if self.ed.has_selection() && self.ed.selected_text() == self.find.q.text {
            let r = self.find.r.text.clone();
            self.insert_text(&r);
        }
        self.find_next(false);
    }

    fn replace_all(&mut self) {
        let q = self.find.q.text.clone();
        if q.is_empty() || !self.editable() {
            return;
        }
        let text: Vec<char> = self.ed.doc.text().chars().collect();
        let needle: Vec<char> = q.chars().collect();
        let rep = self.find.r.text.clone();
        let mut out = String::with_capacity(text.len());
        let mut i = 0;
        let mut n = 0;
        while i < text.len() {
            if self.matches_at(&text, i, &needle) {
                out.push_str(&rep);
                i += needle.len();
                n += 1;
            } else {
                out.push(text[i]);
                i += 1;
            }
        }
        if n == 0 {
            return;
        }
        self.edit(|e| {
            e.doc.break_merge();
            let end = e.doc.end();
            e.replace_range(Pos::default(), end, &out, crate::doc::Kind::Other);
        });
    }

    // ---------------------------------------------------------- headings

    fn current_headings(&self) -> Vec<(usize, usize, String)> {
        if self.previewing && !self.pv.headings.is_empty() {
            return self
                .pv
                .headings
                .iter()
                .map(|h| (h.0, h.1, h.2.clone()))
                .collect();
        }
        markdown::extract_headings(&self.ed.doc.text())
    }

    fn outline(&mut self) {
        let heads = self.current_headings();
        if heads.is_empty() {
            self.flash_for("No headings", 1500);
            return;
        }
        let items = heads
            .iter()
            .enumerate()
            .map(|(i, h)| ("    ".repeat(h.1 - 1) + &h.2, i.to_string()))
            .collect();
        let lines = heads.iter().map(|h| h.0).collect();
        self.overlays.push(Overlay::Picker(
            Picker::new("Outline", "Filter headings", items, Filter::Terms),
            PickKind::Outline(lines),
        ));
        self.dirty = true;
    }

    fn goto_heading(&mut self) {
        let heads = self.current_headings();
        if heads.is_empty() {
            self.flash_for("No headings", 1500);
            return;
        }
        let items = heads
            .iter()
            .enumerate()
            .map(|(i, h)| ("  ".repeat(h.1 - 1) + &h.2, i.to_string()))
            .collect();
        let lines = heads.iter().map(|h| h.0).collect();
        self.overlays.push(Overlay::Picker(
            Picker::new("Go to heading", "Heading:", items, Filter::Terms),
            PickKind::Goto(lines),
        ));
        self.dirty = true;
    }

    fn jump_to_line(&mut self, line: usize) {
        if self.previewing {
            if let Some(h) = self.pv.headings.iter().find(|h| h.0 == line) {
                let a = h.3.clone();
                self.scroll_preview_to(&a, true);
                return;
            }
            self.toggle_preview();
        }
        if line >= self.ed.doc.lines.len() {
            return;
        }
        self.ed.set_cursor(Pos::new(line, 0), false);
        self.ed.sync(&mut self.fonts);
        self.ed.scroll_y = self.ed.line_top(line).min(self.ed.max_scroll());
        self.find_focused = false;
        self.dirty = true;
    }

    fn open_at_heading(&mut self, wanted: &str) {
        if self.previewing {
            self.scroll_fix = Some(ScrollFix::Anchor(wanted.to_string()));
            self.update_preview();
            return;
        }
        let heads: Vec<markdown::Heading> = self
            .current_headings()
            .into_iter()
            .map(|(n, l, t)| {
                let s = markdown::slugify(&t, None);
                (n, l, t, s)
            })
            .collect();
        match markdown::find_heading(&heads, wanted) {
            None => self.flash_for(&format!("No heading: {wanted}"), 4000),
            Some(i) => {
                let line = heads[i].0;
                self.ed.set_cursor(Pos::new(line, 0), false);
                let tr = self.text_rect();
                self.ed.width = tr.w;
                self.ed.view_h = tr.h;
                self.ed.sync(&mut self.fonts);
                self.ed.scroll_y = self.ed.line_top(line).min(self.ed.max_scroll());
            }
        }
    }

    // ------------------------------------------------------------ preview

    fn set_previewing(&mut self, on: bool) {
        self.previewing = on;
        if !on {
            self.pv.hit = None;
        }
        self.update_preview();
        self.focus_link = None;
        self.find_focused = false;
        self.dirty = true;
    }

    fn toggle_preview(&mut self) {
        let ratio = if self.previewing {
            self.pv.scroll_y as f32 / self.pv.max_scroll().max(1) as f32
        } else {
            self.ed.scroll_y as f32 / self.ed.max_scroll().max(1) as f32
        };
        let to = !self.previewing;
        self.set_previewing(to);
        if to {
            if self.scroll_fix.is_none() {
                self.scroll_fix = Some(ScrollFix::Ratio(ratio));
            }
        } else {
            self.ed.sync(&mut self.fonts);
            self.ed.scroll_y = (self.ed.max_scroll() as f32 * ratio) as i32;
        }
    }

    fn source_key(&self, text: &str) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.path.hash(&mut h);
        text.hash(&mut h);
        h.finish()
    }

    fn update_preview(&mut self) {
        self.preview_at = None;
        if !self.previewing {
            return;
        }
        let text = self.ed.doc.text();
        let key = self.source_key(&text);
        if self.pv.source_key == Some(key) {
            return;
        }
        // Keep the reading position when the same document re-renders.
        if self.pv.source_key.is_some() && self.scroll_fix.is_none() && self.pv.max_scroll() > 0 {
            let r = self.pv.scroll_y as f32 / self.pv.max_scroll() as f32;
            self.scroll_fix = Some(ScrollFix::Ratio(r));
        }
        if text.chars().count() > LARGE_DOCUMENT_CHARS {
            self.render_gen += 1;
            let gen = self.render_gen;
            let (tx, wake) = (self.tx.clone(), self.wake);
            std::thread::spawn(move || {
                let r = markdown::render_markdown(&text);
                let _ = tx.send(Msg::Render(gen, r, key));
                wake_up(wake);
            });
            return;
        }
        let r = markdown::render_markdown(&text);
        self.pv.set_html(&r.html, r.headings, key);
        self.dirty = true;
    }

    fn preview_page(&self) -> (Rect, i32) {
        let tr = self.text_rect();
        let p = self.page_rect();
        let area = Rect::new(
            p.x + self.s(22),
            p.y + self.s(34),
            tr.w,
            (p.h - self.s(34)).max(20),
        );
        let w = area.w - self.s(4);
        (area, w)
    }

    fn layout_preview(&mut self) {
        let (area, w) = self.preview_page();
        self.pv.view_h = area.h;
        let px = self.prefs.font_size as f32 * self.scale * 96.0 / 72.0;
        let base = self
            .path
            .as_ref()
            .and_then(|p| p.parent())
            .map(Path::to_path_buf);
        if self.images.collect() {
            self.pv.invalidate();
        }
        let max_img_w = (w - self.s(24)).max(120);
        let mut ctx = Ctx {
            fonts: &mut self.fonts,
            pal: &self.pal,
            images: &mut self.images,
            base_dir: base.as_deref(),
            px,
            scale: self.scale,
            max_img_w,
        };
        let relaid = self.pv.ensure_layout(&mut ctx, w);
        if relaid {
            if let Some(fix) = self.scroll_fix.take() {
                match fix {
                    ScrollFix::Anchor(a) => {
                        self.scroll_preview_to(&a, false);
                    }
                    ScrollFix::Y(y) => {
                        self.pv.scroll_y = y;
                        self.pv.clamp();
                    }
                    ScrollFix::Ratio(r) => {
                        self.pv.scroll_y = (self.pv.max_scroll() as f32 * r).round() as i32;
                        self.pv.clamp();
                    }
                }
            } else if let Some((a, y)) = self.anchor_hold.clone() {
                // Images above the target changed the page: stay at the
                // heading while the view has not moved.
                if self.pv.scroll_y == y {
                    self.scroll_preview_to(&a, false);
                }
            }
        }
    }

    fn remember_position(&mut self) {
        self.nav.push((
            self.path.as_ref().map(|p| p.to_string_lossy().into_owned()),
            self.pv.scroll_y,
        ));
        if self.nav.len() > 50 {
            self.nav.remove(0);
        }
    }

    fn scroll_preview_to(&mut self, anchor: &str, remember: bool) -> bool {
        if anchor.is_empty() {
            return false;
        }
        if remember {
            self.remember_position();
        }
        let mut name = anchor.to_string();
        let known = self.pv.headings.iter().any(|h| h.3 == name);
        if !known && !name.starts_with("fn") {
            match markdown::find_heading(&self.pv.headings, &name) {
                Some(i) => name = self.pv.headings[i].3.clone(),
                None => {
                    self.flash_status(&format!("No heading: {anchor}"));
                    return false;
                }
            }
        }
        if let Some(y) = self.pv.anchor_y(&name) {
            self.pv.scroll_y = y;
            self.pv.clamp();
        }
        self.anchor_hold = Some((name, self.pv.scroll_y));
        self.dirty = true;
        true
    }

    fn open_link(&mut self, target: &str) {
        match markdown::link_kind(target) {
            Some(LinkKind::Web) => spawn_detached("xdg-open", &[target.trim()]),
            Some(LinkKind::Anchor) => {
                let t = target.trim()[1..].to_string();
                self.scroll_preview_to(&t, true);
            }
            Some(LinkKind::Local) if self.path.is_some() => {
                let (raw, frag) = target.split_once('#').unwrap_or((target, ""));
                let dir = self
                    .path
                    .as_ref()
                    .and_then(|p| p.parent())
                    .map(Path::to_path_buf)
                    .unwrap_or_default();
                let joined = dir.join(crate::x::decode_path(raw));
                let path = std::fs::canonicalize(&joined).unwrap_or(joined);
                let ext = path
                    .extension()
                    .map(|e| e.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                if path.is_file() && VIEW_EXTENSIONS.contains(&ext.as_str()) {
                    let same = self
                        .path
                        .as_ref()
                        .and_then(|p| std::fs::canonicalize(p).ok())
                        .is_some_and(|p| p == path);
                    if same {
                        if !frag.is_empty() {
                            let f = frag.to_string();
                            self.scroll_preview_to(&f, true);
                        }
                        return;
                    }
                    self.maybe_save_then(After::Follow(path, frag.to_string()));
                } else if path.is_dir() || !path.exists() {
                    self.flash_status(&format!("Not found: {raw}"));
                } else {
                    self.flash_status(&format!(
                        "Not a Markdown file: {}",
                        path.file_name()
                            .map(|n| n.to_string_lossy())
                            .unwrap_or_default()
                    ));
                }
            }
            _ => self.flash_status("Blocked unsafe preview link"),
        }
    }

    fn navigate_back(&mut self) {
        let Some((path, y)) = self.nav.pop() else {
            self.flash_status("No previous location");
            return;
        };
        let cur = self.path.as_ref().map(|p| p.to_string_lossy().into_owned());
        if path != cur {
            self.maybe_save_then(After::Back(path, y));
        } else {
            self.pv.scroll_y = y;
            self.pv.clamp();
            self.dirty = true;
        }
    }

    // ------------------------------------------------------------ view

    fn set_focus_mode(&mut self, on: bool) {
        if on == self.focus_mode {
            return;
        }
        self.focus_mode = on;
        self.x.set_fullscreen(on);
        self.x
            .set_pointer(if on { Pointer::Blank } else { Pointer::Text });
        self.dirty = true;
    }

    fn toggle_float(&mut self) {
        self.floating = !self.floating;
        self.x.set_utility(self.floating);
    }

    fn about(&mut self) {
        self.notice(
            "About NIRUNOTE",
            &format!(
                "NIRUNOTE {VERSION}\n\nMinimal, suckless writing for Linux.\nMarkdown-first. Keyboard-driven. Fast.\n\nby Nicklas Rudolfsson"
            ),
        );
    }

    fn prefs_dialog(&mut self, spell_focus: bool) {
        let p = &self.prefs;
        let themes = available_themes();
        let ti = themes.iter().position(|t| *t == p.theme).unwrap_or(0);
        let widths: Vec<String> = WIDTHS.iter().map(|w| w.0.to_string()).collect();
        let wi = widths
            .iter()
            .position(|w| *w == p.writing_width)
            .unwrap_or(1);
        let langs = vec!["sv_SE".to_string(), "en_US".into(), "auto".into()];
        let li = langs
            .iter()
            .position(|l| *l == p.spell_language)
            .unwrap_or(0);
        let rows = vec![
            Row::Choice(
                "Theme".into(),
                themes.iter().map(|t| theme_label(t)).collect(),
                ti,
            ),
            Row::Choice(
                "Default format".into(),
                vec!["Markdown (.md)".into(), "Plain text (.txt)".into()],
                usize::from(p.format != "md"),
            ),
            Row::Choice(
                "Default save location".into(),
                vec!["Last used folder".into(), "Custom folder".into()],
                usize::from(p.save_folder_mode == "custom"),
            ),
            Row::Folder(
                "Custom save folder".into(),
                LineEdit::new(&p.default_save_folder, ""),
            ),
            Row::Check("Restore last document".into(), p.restore_last_document),
            Row::Check("Show reading time".into(), p.show_reading_time),
            Row::Check("Remote images in Preview".into(), p.remote_images),
            Row::Choice("Writing width".into(), widths, wi),
            Row::Spin("Font size".into(), p.font_size.clamp(10, 36), 10, 36),
            Row::Check("Word wrap".into(), p.word_wrap),
            Row::Check("Highlight current line".into(), p.highlight_line),
            Row::Check("Recovery autosave".into(), p.autosave),
            Row::Check("Restore recovery".into(), p.restore),
            Row::Check("Typewriter mode".into(), p.typewriter),
            Row::Check("Spell checking".into(), p.spell_enabled),
            Row::Choice("Spell language".into(), langs, li),
        ];
        let mut f = Form::new("NIRUNOTE Preferences", rows, &["Cancel", "OK"], 1, 0);
        f.width = 560;
        f.focus_row(if spell_focus { 14 } else { 0 });
        self.overlays.push(Overlay::Form(f, FormKind::Prefs));
        self.dirty = true;
    }

    fn apply_prefs(&mut self, f: &Form) {
        let themes = available_themes();
        self.prefs.theme = themes[f.choice(0).min(themes.len() - 1)].clone();
        self.prefs.format = if f.choice(1) == 0 { "md" } else { "txt" }.into();
        self.prefs.save_folder_mode = if f.choice(2) == 1 { "custom" } else { "last" }.into();
        self.prefs.default_save_folder = f.field(3).trim().to_string();
        self.prefs.restore_last_document = f.check(4);
        self.prefs.show_reading_time = f.check(5);
        let remote = f.check(6);
        if remote != self.prefs.remote_images {
            self.prefs.remote_images = remote;
            self.images.remote = remote;
            self.images.reset();
            self.pv.source_key = None;
        }
        self.prefs.writing_width = WIDTHS[f.choice(7).min(3)].0.into();
        self.prefs.font_size = f.spin(8);
        self.prefs.word_wrap = f.check(9);
        self.prefs.highlight_line = f.check(10);
        self.prefs.autosave = f.check(11);
        self.prefs.restore = f.check(12);
        self.prefs.typewriter = f.check(13);
        self.prefs.spell_enabled = f.check(14);
        self.prefs.spell_language = ["sv_SE", "en_US", "auto"][f.choice(15).min(2)].into();
        self.init_spell();
        self.ed.wrap = self.prefs.word_wrap;
        self.save_settings();
        self.apply_theme();
        self.configure_theme_watch();
        self.update_stats();
        self.update_preview();
    }

    // ------------------------------------------------------------ menus

    fn menu_items(&self, cmds: &mut Vec<Cmd>) -> Vec<MenuItem> {
        let mut a = |c: Cmd| -> usize {
            cmds.push(c);
            cmds.len() - 1
        };
        let ro = self.opts.read_only;
        let has_file = self.path.is_some();
        let mut recent: Vec<MenuItem> = self
            .prefs
            .recent
            .iter()
            .filter(|p| Path::new(p).is_file())
            .take(10)
            .map(|p| {
                let name = Path::new(p)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                MenuItem::act(&name, "", a(Cmd::OpenRecent(p.clone())))
            })
            .collect();
        if recent.is_empty() {
            recent.push(MenuItem::act("No recent files", "", 0).enabled(false));
        } else {
            recent.push(MenuItem::sep());
            recent.push(MenuItem::act("Clear Recent", "", a(Cmd::ClearRecent)));
        }
        let file = vec![
            MenuItem::act("New", "Ctrl+N", a(Cmd::New)),
            MenuItem::act(
                "New from Clipboard",
                "Ctrl+Shift+N",
                a(Cmd::NewFromClipboard),
            ),
            MenuItem::act("Open…", "Ctrl+O", a(Cmd::Open)),
            MenuItem::sub("Open Recent", recent),
            MenuItem::sep(),
            MenuItem::act("Save", "Ctrl+S", a(Cmd::Save)).enabled(!ro),
            MenuItem::act("Save As…", "Ctrl+Shift+S", a(Cmd::SaveAs)).enabled(!ro),
            MenuItem::act("Rename…", "", a(Cmd::Rename)).enabled(!ro && has_file),
            MenuItem::act("Open containing folder", "", a(Cmd::OpenFolder)).enabled(has_file),
            MenuItem::act("Copy file path", "", a(Cmd::CopyPath)).enabled(has_file),
            MenuItem::sep(),
            MenuItem::act("Quit", "Ctrl+Q", a(Cmd::Quit)),
            MenuItem::act("Quick Open…", "Ctrl+P", a(Cmd::QuickOpen)),
        ];
        let edit = vec![
            MenuItem::act("Undo", "Ctrl+Z", a(Cmd::Undo)).enabled(!ro),
            MenuItem::act("Redo", "Ctrl+Shift+Z", a(Cmd::Redo)).enabled(!ro),
            MenuItem::sep(),
            MenuItem::act("Select all", "Ctrl+A", a(Cmd::SelectAll)),
            MenuItem::act("Copy all", "Ctrl+Shift+C", a(Cmd::CopyAll)),
            MenuItem::act("Paste as plain text", "Ctrl+Shift+V", a(Cmd::PastePlain)).enabled(!ro),
            MenuItem::act("Duplicate line", "Ctrl+Shift+D", a(Cmd::DuplicateLine)).enabled(!ro),
            MenuItem::sep(),
            MenuItem::act("Find…", "Ctrl+F", a(Cmd::Find)),
            MenuItem::act("Find next", "F3", a(Cmd::FindNext)),
            MenuItem::act("Find previous", "Shift+F3", a(Cmd::FindPrev)),
            MenuItem::act("Replace…", "Ctrl+H", a(Cmd::Replace)).enabled(!ro),
            MenuItem::sep(),
            MenuItem::act("Bold", "Ctrl+B", a(Cmd::Bold)).enabled(!ro),
            MenuItem::act("Italic", "Ctrl+I", a(Cmd::Italic)).enabled(!ro),
            MenuItem::act("Link", "Ctrl+K", a(Cmd::Link)).enabled(!ro),
            MenuItem::act("Checkbox", "Ctrl+Shift+X", a(Cmd::Checkbox)).enabled(!ro),
            MenuItem::act("Bullet list", "Ctrl+Shift+8", a(Cmd::Bullet)).enabled(!ro),
            MenuItem::act("Numbered list", "Ctrl+Shift+7", a(Cmd::Numbered)).enabled(!ro),
            MenuItem::act("Quote", "Ctrl+Shift+.", a(Cmd::Quote)).enabled(!ro),
            MenuItem::sep(),
            MenuItem::act("Go to heading…", "Ctrl+G", a(Cmd::GotoHeading)),
            MenuItem::act("Outline…", "Ctrl+Shift+O", a(Cmd::Outline)),
            MenuItem::act("Back", "Alt+Left", a(Cmd::Back)),
            MenuItem::act("Insert date / time", "", a(Cmd::InsertDate)).enabled(!ro),
            MenuItem::act("Command Palette…", "Ctrl+Shift+P", a(Cmd::Palette)),
        ];
        let widths = WIDTHS
            .iter()
            .map(|w| {
                MenuItem::act(w.0, "", a(Cmd::Width(w.0.into())))
                    .checked(w.0 == self.prefs.writing_width)
            })
            .collect();
        let themes = available_themes()
            .into_iter()
            .map(|t| {
                let on = t == self.prefs.theme;
                MenuItem::act(&theme_label(&t), "", a(Cmd::Theme(t))).checked(on)
            })
            .collect();
        let view = vec![
            MenuItem::act("Edit / Preview", "Ctrl+Shift+E", a(Cmd::TogglePreview)),
            MenuItem::act("Focus mode", "F11", a(Cmd::Focus)),
            MenuItem::act("Toggle floating", "Ctrl+Shift+F", a(Cmd::Float)),
            MenuItem::sep(),
            MenuItem::act("Word wrap", "", a(Cmd::Wrap)).checked(self.prefs.word_wrap),
            MenuItem::act("Highlight current line", "", a(Cmd::LineHighlight))
                .checked(self.prefs.highlight_line),
            MenuItem::sub("Writing width", widths),
            MenuItem::sub("Theme", themes),
            MenuItem::sep(),
            MenuItem::act("Text size +", "Ctrl++", a(Cmd::FontUp)),
            MenuItem::act("Text size -", "Ctrl+-", a(Cmd::FontDown)),
            MenuItem::act("Reset text size", "Ctrl+0", a(Cmd::FontReset)),
            MenuItem::act("Preferences…", "", a(Cmd::Prefs)),
        ];
        let help = vec![MenuItem::act("About NIRUNOTE", "", a(Cmd::About))];
        vec![
            MenuItem::sub("File", file),
            MenuItem::sub("Edit", edit),
            MenuItem::sub("View", view),
            MenuItem::sub("Help", help),
        ]
    }

    fn open_menu(&mut self) {
        let mut cmds = Vec::new();
        let items = self.menu_items(&mut cmds);
        let at = (self.s(12), self.header_h().max(self.s(8)));
        let mut m = Menu::new(items, at);
        m.levels[0].1 = Some(0);
        self.overlays.push(Overlay::Menu(m, cmds));
        self.dirty = true;
    }

    fn context_menu(&mut self, x: i32, y: i32) {
        let mut cmds: Vec<Cmd> = Vec::new();
        let mut a = |c: Cmd| -> usize {
            cmds.push(c);
            cmds.len() - 1
        };
        let mut items = Vec::new();
        let ro = self.opts.read_only || self.previewing;
        if !ro {
            if let Some(d) = self.spell_dict.clone() {
                let tr = self.text_rect();
                let p = self.ed.pos_at(
                    &mut self.fonts,
                    (x - tr.x) as f32 + self.ed.scroll_x,
                    y - tr.y + self.ed.scroll_y,
                );
                if let Some((wa, wb, word)) = self.ed.word_at(p) {
                    if word.chars().all(char::is_alphabetic) && !d.check(&word) {
                        for s in d.suggest(&word).into_iter().take(5) {
                            items.push(MenuItem::act(&s, "", a(Cmd::Suggest(wa, wb, s.clone()))));
                        }
                        items.push(MenuItem::act(
                            &format!("Add “{word}” to dictionary"),
                            "",
                            a(Cmd::AddWord(word.clone())),
                        ));
                        items.push(MenuItem::act(
                            &format!("Ignore “{word}” for this session"),
                            "",
                            a(Cmd::IgnoreWord(word.clone())),
                        ));
                        items.push(MenuItem::sep());
                    }
                }
            }
        }
        let sel = if self.previewing {
            self.pv.sel.0 != self.pv.sel.1
        } else {
            self.ed.has_selection()
        };
        items.push(
            MenuItem::act("Undo", "Ctrl+Z", a(Cmd::Undo)).enabled(!ro && self.ed.doc.can_undo()),
        );
        items.push(
            MenuItem::act("Redo", "Ctrl+Shift+Z", a(Cmd::Redo))
                .enabled(!ro && self.ed.doc.can_redo()),
        );
        items.push(MenuItem::sep());
        items.push(MenuItem::act("Cut", "Ctrl+X", a(Cmd::Cut)).enabled(!ro && sel));
        items.push(MenuItem::act("Copy", "Ctrl+C", a(Cmd::Copy)).enabled(sel));
        items.push(MenuItem::act("Paste", "Ctrl+V", a(Cmd::Paste)).enabled(!ro));
        items.push(MenuItem::act("Delete", "", a(Cmd::Delete)).enabled(!ro && sel));
        items.push(MenuItem::sep());
        items.push(MenuItem::act("Select All", "Ctrl+A", a(Cmd::SelectAll)));
        let mut m = Menu::new(items, (x, y));
        m.levels[0].1 = None;
        self.overlays.push(Overlay::Menu(m, cmds));
        self.dirty = true;
    }

    fn palette(&mut self) {
        let names = [
            "New",
            "Open",
            "Quick Open",
            "Save",
            "Save As",
            "Preview",
            "Focus mode",
            "Go to heading",
            "Outline",
            "Insert date / time",
            "Preferences",
        ];
        let items = names
            .iter()
            .map(|n| (n.to_string(), n.to_string()))
            .collect();
        self.overlays.push(Overlay::Picker(
            Picker::new("Command Palette", "Type a command", items, Filter::Terms),
            PickKind::Palette,
        ));
        self.dirty = true;
    }

    fn run(&mut self, c: Cmd) {
        match c {
            Cmd::New => self.maybe_save_then(After::New),
            Cmd::NewFromClipboard => self.maybe_save_then(After::NewFromClipboard),
            Cmd::Open => self.maybe_save_then(After::Open),
            Cmd::OpenRecent(p) => self.maybe_save_then(After::Load(PathBuf::from(p))),
            Cmd::ClearRecent => {
                self.prefs.recent.clear();
                self.store.set("recent", Value::List(Vec::new()));
                self.store.sync();
            }
            Cmd::Save => self.save_feedback(),
            Cmd::SaveAs => self.save_as(),
            Cmd::Rename => self.rename(),
            Cmd::OpenFolder => self.open_containing_folder(),
            Cmd::CopyPath => {
                if let Some(p) = &self.path {
                    let s = p.to_string_lossy().into_owned();
                    self.copy(s);
                }
            }
            Cmd::Quit => self.close(),
            Cmd::Undo => self.edit(|e| {
                e.undo();
            }),
            Cmd::Redo => self.edit(|e| {
                e.redo();
            }),
            Cmd::SelectAll => {
                if self.previewing {
                    self.pv.select_all();
                } else {
                    self.ed.select_all();
                    self.after_cursor();
                }
            }
            Cmd::CopyAll => {
                let t = self.ed.doc.text();
                self.copy(t);
            }
            Cmd::PastePlain | Cmd::Paste => {
                if self.editable() {
                    self.paste(PasteTo::Editor);
                }
            }
            Cmd::DuplicateLine => self.duplicate_line(),
            Cmd::Find => self.show_find(false),
            Cmd::FindNext => {
                self.find_next(false);
            }
            Cmd::FindPrev => {
                self.find_next(true);
            }
            Cmd::Replace => self.show_find(true),
            Cmd::Bold => self.wrap_markdown("**", "**"),
            Cmd::Italic => self.wrap_markdown("*", "*"),
            Cmd::Link => self.insert_link(),
            Cmd::Checkbox => self.toggle_checkbox(),
            Cmd::Bullet => self.prefix_line("- "),
            Cmd::Numbered => self.prefix_line("1. "),
            Cmd::Quote => self.prefix_line("> "),
            Cmd::GotoHeading => self.goto_heading(),
            Cmd::Outline => self.outline(),
            Cmd::Back => self.navigate_back(),
            Cmd::InsertDate => {
                if self.editable() {
                    let t = files::local_time("%Y-%m-%d %H:%M");
                    self.insert_text(&t);
                }
            }
            Cmd::QuickOpen => self.quick_open(),
            Cmd::Palette => self.palette(),
            Cmd::TogglePreview => self.toggle_preview(),
            Cmd::Focus => {
                let f = !self.focus_mode;
                self.set_focus_mode(f);
            }
            Cmd::Float => self.toggle_float(),
            Cmd::Wrap => {
                self.prefs.word_wrap = !self.prefs.word_wrap;
                self.ed.wrap = self.prefs.word_wrap;
                self.save_settings();
            }
            Cmd::LineHighlight => {
                self.prefs.highlight_line = !self.prefs.highlight_line;
                self.save_settings();
            }
            Cmd::Width(w) => {
                self.prefs.writing_width = w;
                self.save_settings();
                self.pv.invalidate();
            }
            Cmd::Theme(t) => self.set_theme(&t),
            Cmd::FontUp => self.resize_font(1),
            Cmd::FontDown => self.resize_font(-1),
            Cmd::FontReset => {
                self.prefs.font_size = 17;
                self.save_settings();
                self.apply_theme();
            }
            Cmd::Prefs => self.prefs_dialog(false),
            Cmd::About => self.about(),
            Cmd::Cut => {
                if self.editable() && self.ed.has_selection() {
                    let t = self.ed.selected_text();
                    self.copy(t);
                    self.edit(|e| e.delete(false));
                }
            }
            Cmd::Copy => {
                let t = if self.previewing {
                    self.pv.selected_text()
                } else {
                    self.ed.selected_text()
                };
                self.copy(t);
            }
            Cmd::Delete => self.edit(|e| e.delete(false)),
            Cmd::Suggest(a, b, s) => self.edit(|e| {
                e.doc.break_merge();
                e.replace_range(a, b, &s, crate::doc::Kind::Other);
            }),
            Cmd::IgnoreWord(w) => {
                if let Some(d) = &self.spell_dict {
                    d.ignore(&w);
                }
            }
            Cmd::AddWord(w) => {
                if let Some(d) = &self.spell_dict {
                    d.add_word(&w);
                }
            }
        }
        self.dirty = true;
    }

    fn save_feedback(&mut self) {
        if self.save() {
            self.save_fb = Some(Instant::now() + Duration::from_millis(1000));
            self.flash_for("Saved", 1500);
        }
    }

    /// Closing the window: what NIRUNOTE 0.7's closeEvent did.
    fn close(&mut self) {
        let (x, y, w, h) = self.geom;
        let (sw, sh) = {
            let s = &self.x.c.setup().roots[0];
            (s.width_in_pixels as i32, s.height_in_pixels as i32)
        };
        self.store.set(
            "geometry",
            Value::Bytes(settings::geometry_bytes(x, y, w, h, sw, sh)),
        );
        if self.opts.read_only {
            self.store.sync();
            self.quit = true;
            return;
        }
        self.recover(true);
        if let Some(p) = self.path.clone() {
            let off = self.ed.doc.utf16_offset(self.ed.cursor) as i64;
            self.store.set_int(&Self::cursor_key(&p), off);
            self.prefs.last_document = p.to_string_lossy().into_owned();
            self.save_settings();
        } else {
            self.store.sync();
        }
        self.maybe_save_then(After::Quit);
    }

    // ------------------------------------------------------- overlays

    fn overlay_key(&mut self, k: Key, m: Mods) {
        let Some(top) = self.overlays.pop() else {
            return;
        };
        match top {
            Overlay::Menu(mut menu, cmds) => match menu.key(&k) {
                MenuOut::Close => {}
                MenuOut::Run(i) => {
                    if let Some(c) = cmds.get(i).cloned() {
                        self.run(c);
                    }
                }
                MenuOut::None => self.overlays.push(Overlay::Menu(menu, cmds)),
            },
            Overlay::Form(mut f, kind) => {
                let out = f.key(&k, m);
                self.form_out(f, kind, out);
            }
            Overlay::Picker(mut p, kind) => {
                let out = p.key(&k, m);
                self.picker_out(p, kind, out);
            }
            Overlay::Chooser(mut c, kind) => {
                let out = c.key(&k, m);
                self.chooser_out(c, kind, out);
            }
            Overlay::Compare(mut c) => {
                if !c.key(&k) {
                    self.overlays.push(Overlay::Compare(c));
                }
            }
        }
        self.dirty = true;
    }

    fn form_out(&mut self, f: Form, kind: FormKind, out: FormOut) {
        match out {
            FormOut::None => self.overlays.push(Overlay::Form(f, kind)),
            FormOut::Copy(s) => {
                self.copy(s);
                self.overlays.push(Overlay::Form(f, kind));
            }
            FormOut::Paste => {
                self.overlays.push(Overlay::Form(f, kind));
                self.paste(PasteTo::Overlay);
            }
            FormOut::Choose => {
                let dir = PathBuf::from(expand(&f.field(3)));
                let dir = if dir.is_dir() { dir } else { theme::home() };
                self.overlays.push(Overlay::Form(f, kind));
                self.overlays.push(Overlay::Chooser(
                    Chooser::new(ChooseMode::Folder, "Choose default save folder", &dir, ""),
                    ChooseKind::PrefsFolder,
                ));
            }
            FormOut::Button(b) => match kind {
                FormKind::Notice => {}
                FormKind::Unsaved(after) => match b {
                    2 => {
                        self.after_save = Some(after);
                        if self.path.is_some() {
                            self.save();
                        } else {
                            self.save_as();
                        }
                    }
                    0 => {
                        self.backup_discarded();
                        self.run_after(after);
                    }
                    _ => {}
                },
                FormKind::External => match b {
                    0 => {
                        if let Some(p) = self.path.clone() {
                            self.load(&p, false);
                        }
                    }
                    2 => self.save_as(),
                    _ => {
                        self.watch_current();
                    }
                },
                FormKind::Recovery => match b {
                    1 => {
                        let s = self.startup_recovery.as_ref();
                        let disk = s
                            .and_then(|s| files::disk_text(s.path.as_deref()))
                            .unwrap_or_default();
                        let text = s.map(|s| s.text.clone()).unwrap_or_default();
                        self.overlays.push(Overlay::Form(f, kind));
                        self.overlays
                            .push(Overlay::Compare(Compare::new(&disk, &text)));
                    }
                    0 => {
                        self.startup_recovery = None;
                        self.move_recovery_aside();
                    }
                    _ => {
                        if let Some(s) = self.startup_recovery.take() {
                            self.restore_snapshot(s);
                        }
                    }
                },
                FormKind::Prefs => {
                    if b == 1 {
                        self.apply_prefs(&f);
                    }
                }
                FormKind::Rename => {
                    if b == 1 {
                        let n = f.field(0);
                        self.do_rename(n.trim());
                    }
                }
                FormKind::Overwrite(p) => {
                    if b == 1 {
                        self.save_to(p);
                    } else {
                        self.after_save = None;
                    }
                }
            },
        }
    }

    fn picker_out(&mut self, p: Picker, kind: PickKind, out: PickOut) {
        match out {
            PickOut::None => self.overlays.push(Overlay::Picker(p, kind)),
            PickOut::Copy(s) => {
                self.copy(s);
                self.overlays.push(Overlay::Picker(p, kind));
            }
            PickOut::Paste => {
                self.overlays.push(Overlay::Picker(p, kind));
                self.paste(PasteTo::Overlay);
            }
            PickOut::Cancel => {
                if let PickKind::QuickOpen(c) = kind {
                    c.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            }
            PickOut::Pick(data) => match kind {
                PickKind::Outline(lines) | PickKind::Goto(lines) => {
                    if let Some(l) = data.parse::<usize>().ok().and_then(|i| lines.get(i)) {
                        self.jump_to_line(*l);
                    }
                }
                PickKind::Palette => {
                    let c = match data.as_str() {
                        "New" => Cmd::New,
                        "Open" => Cmd::Open,
                        "Quick Open" => Cmd::QuickOpen,
                        "Save" => Cmd::Save,
                        "Save As" => Cmd::SaveAs,
                        "Preview" => Cmd::TogglePreview,
                        "Focus mode" => Cmd::Focus,
                        "Go to heading" => Cmd::GotoHeading,
                        "Outline" => Cmd::Outline,
                        "Insert date / time" => Cmd::InsertDate,
                        _ => Cmd::Prefs,
                    };
                    self.run(c);
                }
                PickKind::QuickOpen(c) => {
                    c.store(true, std::sync::atomic::Ordering::Relaxed);
                    self.maybe_save_then(After::Load(PathBuf::from(data)));
                }
            },
        }
    }

    fn chooser_out(&mut self, c: Chooser, kind: ChooseKind, out: ChooseOut) {
        match out {
            ChooseOut::None => self.overlays.push(Overlay::Chooser(c, kind)),
            ChooseOut::Copy(s) => {
                self.copy(s);
                self.overlays.push(Overlay::Chooser(c, kind));
            }
            ChooseOut::Paste => {
                self.overlays.push(Overlay::Chooser(c, kind));
                self.paste(PasteTo::Overlay);
            }
            ChooseOut::Cancel => {
                if matches!(kind, ChooseKind::SaveAs) {
                    self.after_save = None;
                }
            }
            ChooseOut::Done(p) => match kind {
                ChooseKind::Open => {
                    if let Some(d) = p.parent() {
                        self.prefs.last_open_folder = d.to_string_lossy().into_owned();
                        self.save_settings();
                    }
                    self.load(&p, false);
                }
                ChooseKind::SaveAs => {
                    let mut target = p.clone();
                    if target.extension().is_none() {
                        let ext = if self.prefs.format == "md" {
                            "md"
                        } else {
                            "txt"
                        };
                        let mut s = target.into_os_string();
                        s.push(format!(".{ext}"));
                        target = PathBuf::from(s);
                    }
                    let same = self.path.as_ref() == Some(&target);
                    if target.exists() && !same {
                        let name = target
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        self.overlays.push(Overlay::Form(
                            Form::new(
                                "Confirm Save As",
                                vec![Row::Text(format!(
                                    "{name} already exists.\nDo you want to replace it?"
                                ))],
                                &["No", "Yes"],
                                0,
                                0,
                            ),
                            FormKind::Overwrite(target),
                        ));
                    } else {
                        self.save_to(target);
                    }
                }
                ChooseKind::PrefsFolder => {
                    if let Some(Overlay::Form(f, FormKind::Prefs)) = self.overlays.last_mut() {
                        f.set_field(3, &p.to_string_lossy());
                    }
                }
            },
        }
    }

    // ------------------------------------------------------------ keys

    fn key_of(&self, sym: u32) -> Key {
        match sym {
            0xff0d | 0xff8d => Key::Enter,
            0xff1b => Key::Esc,
            0xff09 => Key::Tab,
            0xfe20 => Key::BackTab,
            0xff08 => Key::Backspace,
            0xffff | 0xff9f => Key::Delete,
            0xff51 | 0xff96 => Key::Left,
            0xff52 | 0xff97 => Key::Up,
            0xff53 | 0xff98 => Key::Right,
            0xff54 | 0xff99 => Key::Down,
            0xff50 | 0xff95 => Key::Home,
            0xff57 | 0xff9c => Key::End,
            0xff55 | 0xff9a => Key::PageUp,
            0xff56 | 0xff9b => Key::PageDown,
            0xff63 | 0xff9e => Key::Insert,
            0xffbe..=0xffc9 => Key::F((sym - 0xffbe + 1) as u8),
            s => match keysym_char(s) {
                Some(c) => Key::Char(c),
                None => Key::Other,
            },
        }
    }

    /// Application shortcuts (they work everywhere but in dialogs).
    fn shortcut(&self, sym: u32, base: u32, m: Mods, k: &Key) -> Option<Cmd> {
        let letter = keysym_char(base).map(|c| c.to_ascii_lowercase());
        if m.ctrl && !m.alt {
            let plus = matches!(sym, 0x2b | 0x3d | 0xffab) || base == 0x2b;
            let minus = matches!(sym, 0x2d | 0xffad | 0x5f) || base == 0x2d;
            if plus {
                return Some(Cmd::FontUp);
            }
            if minus && !m.shift {
                return Some(Cmd::FontDown);
            }
            if m.shift {
                return match letter? {
                    'n' => Some(Cmd::NewFromClipboard),
                    's' => Some(Cmd::SaveAs),
                    'z' => Some(Cmd::Redo),
                    'c' => Some(Cmd::CopyAll),
                    'v' => Some(Cmd::PastePlain),
                    'd' => Some(Cmd::DuplicateLine),
                    'x' => Some(Cmd::Checkbox),
                    '8' => Some(Cmd::Bullet),
                    '7' => Some(Cmd::Numbered),
                    '.' => Some(Cmd::Quote),
                    'o' => Some(Cmd::Outline),
                    'p' => Some(Cmd::Palette),
                    'e' => Some(Cmd::TogglePreview),
                    'f' => Some(Cmd::Float),
                    _ => None,
                };
            }
            return match letter? {
                'n' => Some(Cmd::New),
                'a' => Some(Cmd::SelectAll),
                'o' => Some(Cmd::Open),
                's' => Some(Cmd::Save),
                'q' => Some(Cmd::Quit),
                'z' => Some(Cmd::Undo),
                'y' => Some(Cmd::Redo),
                'f' => Some(Cmd::Find),
                'h' => Some(Cmd::Replace),
                'b' => Some(Cmd::Bold),
                'i' => Some(Cmd::Italic),
                'k' => Some(Cmd::Link),
                'g' => Some(Cmd::GotoHeading),
                'p' => Some(Cmd::QuickOpen),
                '0' => Some(Cmd::FontReset),
                _ => None,
            };
        }
        match k {
            Key::F(3) => Some(if m.shift {
                Cmd::FindPrev
            } else {
                Cmd::FindNext
            }),
            Key::F(11) => Some(Cmd::Focus),
            Key::Left if m.alt && !m.ctrl => Some(Cmd::Back),
            _ => None,
        }
    }

    fn key_press(&mut self, code: u8, state: KeyButMask) {
        let sym = self.x.keysym(code, state);
        if (0xffe1..=0xffee).contains(&sym) || sym == 0xfe03 || sym == 0xff7e {
            return; // modifiers
        }
        let m = Mods {
            ctrl: state.contains(KeyButMask::CONTROL),
            shift: state.contains(KeyButMask::SHIFT),
            alt: state.contains(KeyButMask::MOD1),
        };
        if (0xfe50..=0xfe5f).contains(&sym) {
            self.dead = Some(sym);
            return;
        }
        let mut k = self.key_of(sym);
        if let (Some(d), Key::Char(c)) = (self.dead.take(), &k) {
            if !m.ctrl {
                if let Some(x) = compose(d, *c) {
                    k = Key::Char(x);
                }
            }
        }
        // Shift+Tab arrives as ISO_Left_Tab; Shift+Tab with Tab keysym too.
        if k == Key::Tab && m.shift {
            k = Key::BackTab;
        }
        let base = self.x.base_keysym(code);
        self.dirty = true;
        if !self.overlays.is_empty() {
            self.overlay_key(k, m);
            return;
        }
        // The menu (F10 or the Menu key) and the context menu.
        if (k == Key::F(10) && !m.shift) || sym == 0xff67 && !m.shift {
            self.open_menu();
            return;
        }
        if (k == Key::F(10) && m.shift) || (sym == 0xff67 && m.shift) {
            let (x, y, _) = self.ed.caret_xy(&mut self.fonts, self.ed.cursor);
            let tr = self.text_rect();
            self.context_menu(tr.x + x as i32, tr.y + y - self.ed.scroll_y + 20);
            return;
        }
        // Text fields keep their editing keys.
        let field_key = self.find_focused
            && m.ctrl
            && matches!(&k, Key::Char(c) if "acxvz".contains(c.to_ascii_lowercase()))
            && !m.shift;
        if !field_key {
            if let Some(c) = self.shortcut(sym, base, m, &k) {
                self.run(c);
                return;
            }
        }
        if k == Key::Esc {
            if self.find_focused || (self.find.visible && !self.focus_mode) {
                self.hide_find();
                return;
            }
            if self.focus_mode {
                self.set_focus_mode(false);
            }
            return;
        }
        if self.find_focused {
            self.find_key(k, m);
            return;
        }
        if self.previewing {
            self.preview_key(k, m);
        } else {
            self.editor_key(k, m);
        }
    }

    /// The find bar's controls in Tab order: 0 find, 1 replace, 2 Aa,
    /// 3 Word, 4 previous, 5 next, 6 Replace, 7 All, 8 close.
    fn find_controls(&self) -> Vec<u8> {
        let mut v = vec![0u8];
        if self.find.replace && !self.previewing {
            v.push(1);
        }
        v.extend([2, 3, 4, 5]);
        if !self.opts.read_only {
            v.extend([6, 7]);
        }
        v.push(8);
        v
    }

    fn find_activate(&mut self, id: u8) {
        match id {
            2 => self.find.case = !self.find.case,
            3 => self.find.word = !self.find.word,
            4 => {
                self.find_next(true);
            }
            5 => {
                self.find_next(false);
            }
            6 => self.replace_one(),
            7 => self.replace_all(),
            8 => self.hide_find(),
            _ => {}
        }
    }

    fn find_key(&mut self, k: Key, m: Mods) {
        let ctl = self.find_controls();
        let field = self.find.focus <= 1;
        match k {
            Key::Tab | Key::BackTab => {
                let i = ctl.iter().position(|c| *c == self.find.focus).unwrap_or(0) as i64;
                let d = if k == Key::Tab { 1 } else { -1 };
                self.find.focus = ctl[(i + d).rem_euclid(ctl.len() as i64) as usize];
            }
            Key::Enter if self.find.focus == 0 => {
                self.find_next(m.shift);
            }
            Key::Enter if self.find.focus == 1 => self.replace_one(),
            Key::Enter | Key::Char(' ') if !field => {
                let id = self.find.focus;
                self.find_activate(id);
            }
            Key::Up if field => {
                self.find_next(true);
            }
            Key::Down if field => {
                self.find_next(false);
            }
            _ if field => {
                let e = if self.find.focus == 1 {
                    &mut self.find.r
                } else {
                    &mut self.find.q
                };
                match e.key(&k, m) {
                    FieldOut::Copy(s) => self.copy(s),
                    FieldOut::Paste => self.paste(PasteTo::Find),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn preview_key(&mut self, k: Key, m: Mods) {
        let lh = (self.prefs.font_size as f32 * self.scale * 1.8) as i32;
        match k {
            Key::Up => self.pv.scroll_by(-lh),
            Key::Down => self.pv.scroll_by(lh),
            Key::PageUp => self.pv.scroll_by(-(self.pv.view_h - lh)),
            Key::PageDown | Key::Char(' ') => self.pv.scroll_by(self.pv.view_h - lh),
            Key::Home if m.ctrl || !m.shift => self.pv.scroll_y = 0,
            Key::End => self.pv.scroll_y = self.pv.max_scroll(),
            Key::Char('c') | Key::Char('C') if m.ctrl => {
                let t = self.pv.selected_text();
                self.copy(t);
            }
            Key::Insert if m.ctrl => {
                let t = self.pv.selected_text();
                self.copy(t);
            }
            Key::Tab | Key::BackTab => {
                let n = self.pv.layout.links.len();
                if n > 0 {
                    let back = k == Key::BackTab;
                    self.focus_link = Some(match self.focus_link {
                        None => {
                            if back {
                                n - 1
                            } else {
                                0
                            }
                        }
                        Some(i) => {
                            if back {
                                (i + n - 1) % n
                            } else {
                                (i + 1) % n
                            }
                        }
                    });
                    // Scroll to it.
                    let l = self.focus_link;
                    let y = self.pv.layout.items.iter().find_map(|it| match it {
                        crate::preview::Item::Text(r) if r.link == l => Some(r.top),
                        _ => None,
                    });
                    if let Some(y) = y {
                        if y < self.pv.scroll_y || y > self.pv.scroll_y + self.pv.view_h - 40 {
                            self.pv.scroll_y = y - self.pv.view_h / 3;
                            self.pv.clamp();
                        }
                    }
                }
            }
            Key::Enter => {
                if let Some(t) = self
                    .focus_link
                    .and_then(|i| self.pv.layout.links.get(i).cloned())
                {
                    self.open_link(&t);
                }
            }
            _ => {}
        }
        self.dirty = true;
    }

    fn editor_key(&mut self, k: Key, m: Mods) {
        let ro = self.opts.read_only;
        let f = &mut self.fonts;
        let g = self.ed.doc.generation;
        let shift = m.shift;
        match k {
            Key::Char(c) if m.ctrl => match c.to_ascii_lowercase() {
                'c' => {
                    let t = self.ed.selected_text();
                    self.copy(t);
                }
                'x' if !ro => {
                    let t = self.ed.selected_text();
                    if !t.is_empty() {
                        self.copy(t);
                        self.ed.delete(false);
                    }
                }
                'v' if !ro => self.paste(PasteTo::Editor),
                _ => {}
            },
            Key::Char(c) if !m.alt || c as u32 > 0x7f => {
                if !ro {
                    self.ed.insert(&c.to_string());
                }
            }
            Key::Enter => {
                if !ro {
                    self.ed.newline(shift);
                }
            }
            Key::Tab => {
                if !ro {
                    self.ed.tab();
                }
            }
            Key::BackTab => {
                if !ro {
                    self.ed.backtab();
                }
            }
            Key::Backspace => self.ed.backspace(m.ctrl),
            Key::Delete if shift && !ro => {
                let t = self.ed.selected_text();
                if !t.is_empty() {
                    self.copy(t);
                    self.ed.delete(false);
                }
            }
            Key::Delete => self.ed.delete(m.ctrl),
            Key::Insert if m.ctrl => {
                let t = self.ed.selected_text();
                self.copy(t);
            }
            Key::Insert if shift && !ro => self.paste(PasteTo::Editor),
            Key::Left => self.ed.move_h(false, m.ctrl, shift),
            Key::Right => self.ed.move_h(true, m.ctrl, shift),
            Key::Up if m.ctrl => {
                let lh = self.ed.line_height(f);
                self.ed.scroll_by(-lh);
                self.dirty = true;
                return;
            }
            Key::Down if m.ctrl => {
                let lh = self.ed.line_height(f);
                self.ed.scroll_by(lh);
                self.dirty = true;
                return;
            }
            Key::Up => self.ed.move_v(f, -1, shift),
            Key::Down => self.ed.move_v(f, 1, shift),
            Key::PageUp => self.ed.page(f, false, shift),
            Key::PageDown => self.ed.page(f, true, shift),
            Key::Home if m.ctrl => self.ed.doc_start(shift),
            Key::End if m.ctrl => self.ed.doc_end(shift),
            Key::Home => self.ed.home(shift),
            Key::End => self.ed.end(shift),
            _ => {}
        }
        if self.ed.doc.generation != g {
            self.changed();
        } else {
            self.after_cursor();
        }
    }

    // ------------------------------------------------------------ mouse

    fn header_buttons(&self) -> [Rect; 6] {
        let h = self.header_h();
        let b = self.s(28);
        let y = (h - b) / 2;
        let right = self.cv.w - self.s(12) - self.s(16);
        let gap = self.s(4);
        let save = Rect::new(right - b, y, b, b);
        let focus = Rect::new(save.x - gap - b, y, b, b);
        let prev = Rect::new(focus.x - gap - b, y, b, b);
        let copy = Rect::new(prev.x - gap - b, y, b, b);
        let menu = Rect::new(self.s(12), y, b, b);
        let title = Rect::new(menu.x + b + 20, 0, (copy.x - 20) - (menu.x + b + 20), h);
        [menu, copy, prev, focus, save, title]
    }

    fn button_press(&mut self, b: u8, x: i32, y: i32, state: KeyButMask) {
        self.dirty = true;
        let m = Mods {
            ctrl: state.contains(KeyButMask::CONTROL),
            shift: state.contains(KeyButMask::SHIFT),
            alt: state.contains(KeyButMask::MOD1),
        };
        let now = Instant::now();
        let (t, n, cx, cy) = self.clicks;
        let count = if now.duration_since(t) < Duration::from_millis(400)
            && (x - cx).abs() < 5
            && (y - cy).abs() < 5
            && b == 1
        {
            n % 3 + 1
        } else {
            1
        };
        if b == 1 {
            self.clicks = (now, count, x, y);
        }
        let px = self.ui_px();
        if let Some(top) = self.overlays.pop() {
            match top {
                Overlay::Menu(mut menu, cmds) => {
                    if b == 1 || b == 3 {
                        match menu.click(x, y, px) {
                            MenuOut::Run(i) => {
                                if let Some(c) = cmds.get(i).cloned() {
                                    self.run(c);
                                }
                            }
                            MenuOut::Close => {}
                            MenuOut::None => self.overlays.push(Overlay::Menu(menu, cmds)),
                        }
                    } else {
                        self.overlays.push(Overlay::Menu(menu, cmds));
                    }
                }
                Overlay::Form(mut f, kind) => {
                    if b == 1 {
                        let out = f.click(&mut self.fonts, px, x, y);
                        self.form_out(f, kind, out);
                    } else {
                        self.overlays.push(Overlay::Form(f, kind));
                    }
                }
                Overlay::Picker(mut p, kind) => {
                    let rh = (px * 2.2) as i32;
                    match b {
                        1 => {
                            let out = p.click(x, y, count >= 2, rh);
                            self.picker_out(p, kind, out);
                        }
                        4 | 5 => {
                            p.wheel(b == 5);
                            self.overlays.push(Overlay::Picker(p, kind));
                        }
                        _ => self.overlays.push(Overlay::Picker(p, kind)),
                    }
                }
                Overlay::Chooser(mut c, kind) => {
                    let rh = (px * 2.2) as i32;
                    match b {
                        1 => {
                            let out = c.click(x, y, count >= 2, rh);
                            self.chooser_out(c, kind, out);
                        }
                        4 | 5 => {
                            c.wheel(b == 5);
                            self.overlays.push(Overlay::Chooser(c, kind));
                        }
                        _ => self.overlays.push(Overlay::Chooser(c, kind)),
                    }
                }
                Overlay::Compare(mut c) => {
                    if b == 4 || b == 5 {
                        c.wheel(b == 5);
                    }
                    self.overlays.push(Overlay::Compare(c));
                }
            }
            return;
        }
        // Header.
        if !self.focus_mode && y < self.header_h() && b == 1 {
            let [menu, copy, prev, focus, save, title] = self.header_buttons();
            if menu.contains(x, y) {
                self.open_menu();
            } else if copy.contains(x, y) {
                let t = self.ed.doc.text();
                self.copy(t);
                self.copy_fb = Some(Instant::now() + Duration::from_millis(900));
            } else if prev.contains(x, y) {
                self.toggle_preview();
            } else if focus.contains(x, y) {
                self.set_focus_mode(true);
            } else if save.contains(x, y) && !self.opts.read_only {
                self.save_feedback();
            } else if title.contains(x, y) && !self.opts.read_only {
                self.rename();
            }
            return;
        }
        // Status line: the spelling text opens the spelling settings.
        if !self.focus_mode && y >= self.cv.h - self.status_h() && b == 1 {
            let (sx, sw) = self.spell_hit();
            if x >= sx && x < sx + sw && !self.opts.read_only {
                self.prefs_dialog(true);
            }
            return;
        }
        // Find bar.
        if self.find.visible
            && y >= self.cv.h - self.status_h() - self.find_h()
            && y < self.cv.h - self.status_h()
        {
            if b == 1 {
                self.find_click(x, y);
            }
            return;
        }
        self.find_focused = false;
        let page = self.page_rect();
        if b == 4 || b == 5 {
            if m.ctrl {
                self.resize_font(if b == 4 { 1 } else { -1 });
                return;
            }
            let lh = self.ed.line_height(&mut self.fonts);
            let d = if b == 4 { -3 * lh } else { 3 * lh };
            if self.previewing {
                self.pv.scroll_by(d);
            } else {
                self.ed.scroll_by(d);
            }
            return;
        }
        if b == 6 || b == 7 {
            if !self.previewing && !self.ed.wrap {
                self.ed.scroll_x = (self.ed.scroll_x + if b == 7 { 40.0 } else { -40.0 }).max(0.0);
            }
            return;
        }
        // The scroll bar (right edge of the page).
        if b == 1
            && x >= page.x + page.w - self.s(10)
            && x < page.x + page.w + self.s(6)
            && y > page.y
        {
            self.drag = Drag::Bar(self.previewing);
            self.bar_drag(y);
            return;
        }
        if self.previewing {
            let (area, _) = self.preview_page();
            let px_ = (x - area.x) as f32;
            let py = y - area.y + self.pv.scroll_y;
            match b {
                1 => {
                    self.press_at = (x, y);
                    let off = self.pv.offset_at(&mut self.fonts, px_, py);
                    if m.shift {
                        self.pv.sel.1 = off;
                    } else {
                        self.pv.sel = (off, off);
                    }
                    if count == 2 {
                        let (a, bb) = word_bounds(&self.pv.layout.flat, off);
                        self.pv.sel = (a, bb);
                    } else if count == 3 {
                        let (a, bb) = line_bounds(&self.pv.layout.flat, off);
                        self.pv.sel = (a, bb);
                    }
                    self.drag = Drag::Preview;
                }
                3 => self.context_menu(x, y),
                _ => {}
            }
            return;
        }
        let tr = self.text_rect();
        let p = self.ed.pos_at(
            &mut self.fonts,
            (x - tr.x) as f32 + self.ed.scroll_x,
            y - tr.y + self.ed.scroll_y,
        );
        match b {
            1 => {
                match count {
                    2 => {
                        let (a, bb) = self.ed.doc.word_at(p);
                        self.ed.select(a, bb);
                    }
                    3 => {
                        let end = if p.line + 1 < self.ed.doc.lines.len() {
                            Pos::new(p.line + 1, 0)
                        } else {
                            Pos::new(p.line, self.ed.doc.lines[p.line].len())
                        };
                        self.ed.select(Pos::new(p.line, 0), end);
                    }
                    _ => self.ed.set_cursor(p, m.shift),
                }
                self.drag = Drag::Editor;
                self.after_cursor();
            }
            2 => {
                if !self.opts.read_only {
                    self.ed.set_cursor(p, false);
                    self.paste_primary();
                }
            }
            3 => {
                if !self.ed.has_selection() {
                    self.ed.set_cursor(p, false);
                }
                self.context_menu(x, y);
            }
            _ => {}
        }
    }

    fn bar_drag(&mut self, y: i32) {
        let page = self.page_rect();
        let frac = ((y - page.y) as f32 / page.h.max(1) as f32).clamp(0.0, 1.0);
        if self.previewing {
            self.pv.scroll_y = (self.pv.max_scroll() as f32 * frac) as i32;
        } else {
            self.ed.scroll_y = (self.ed.max_scroll() as f32 * frac) as i32;
        }
        self.dirty = true;
    }

    fn motion(&mut self, x: i32, y: i32) {
        self.pointer = (x, y);
        if let Some(Overlay::Menu(m, _)) = self.overlays.last_mut() {
            if m.motion(x, y, 13.0 * self.scale) {
                self.dirty = true;
            }
            return;
        }
        match self.drag {
            Drag::Editor => {
                let tr = self.text_rect();
                if y < tr.y {
                    self.ed.scroll_by(-self.s(20));
                } else if y > tr.y + tr.h {
                    self.ed.scroll_by(self.s(20));
                }
                let p = self.ed.pos_at(
                    &mut self.fonts,
                    (x - tr.x) as f32 + self.ed.scroll_x,
                    y - tr.y + self.ed.scroll_y,
                );
                self.ed.cursor = p;
                self.dirty = true;
            }
            Drag::Preview => {
                let (area, _) = self.preview_page();
                if y < area.y {
                    self.pv.scroll_by(-self.s(20));
                } else if y > area.y + area.h {
                    self.pv.scroll_by(self.s(20));
                }
                let off = self.pv.offset_at(
                    &mut self.fonts,
                    (x - area.x) as f32,
                    y - area.y + self.pv.scroll_y,
                );
                self.pv.sel.1 = off;
                self.dirty = true;
            }
            Drag::Bar(_) => self.bar_drag(y),
            Drag::None => {
                if self.focus_mode {
                    return;
                }
                let over_page = self.page_rect().contains(x, y)
                    && self.overlays.is_empty()
                    && y > self.header_h();
                let link = if self.previewing && over_page {
                    let (area, _) = self.preview_page();
                    self.pv
                        .link_at((x - area.x) as f32, y - area.y + self.pv.scroll_y)
                } else {
                    None
                };
                let ptr = if link.is_some() {
                    Pointer::Hand
                } else if over_page {
                    Pointer::Text
                } else {
                    Pointer::Arrow
                };
                self.x.set_pointer(ptr);
                self.hover_link = link;
            }
        }
    }

    fn button_release(&mut self, b: u8, x: i32, y: i32) {
        if b != 1 {
            return;
        }
        let drag = std::mem::replace(&mut self.drag, Drag::None);
        if drag == Drag::Preview {
            let moved = (x - self.press_at.0).abs() > 3 || (y - self.press_at.1).abs() > 3;
            if self.pv.sel.0 != self.pv.sel.1 {
                self.primary_text = self.pv.selected_text();
                self.x.own(self.x.atoms.primary);
            } else if !moved {
                let (area, _) = self.preview_page();
                if let Some(l) = self
                    .pv
                    .link_at((x - area.x) as f32, y - area.y + self.pv.scroll_y)
                {
                    self.open_link(&l);
                }
            }
        }
        if drag == Drag::Editor && self.ed.has_selection() {
            self.primary_text = self.ed.selected_text();
            self.x.own(self.x.atoms.primary);
        }
        self.dirty = true;
    }

    fn spell_hit(&mut self) -> (i32, i32) {
        let px = self.ui_px();
        let left = self.status_left();
        let x = self.s(14) + self.fonts.width(UI, px, &left) as i32 + self.s(10);
        let w = self.fonts.width(UI, px, &self.spell_status()) as i32;
        (x, w)
    }

    fn find_click(&mut self, x: i32, y: i32) {
        let hit = self
            .find
            .hits
            .iter()
            .find(|(r, _)| r.contains(x, y))
            .copied();
        let Some((r, id)) = hit else { return };
        let px = self.ui_px();
        match id {
            0 => {
                self.find.focus = 0;
                self.find_focused = true;
                self.find
                    .q
                    .click(&mut self.fonts, px, (x - r.x - 6) as f32, false);
            }
            1 => {
                self.find.focus = 1;
                self.find_focused = true;
                self.find
                    .r
                    .click(&mut self.fonts, px, (x - r.x - 6) as f32, false);
            }
            id => self.find_activate(id),
        }
        self.dirty = true;
    }

    // ------------------------------------------------------------ X events

    fn event(&mut self, ev: Event) {
        match ev {
            Event::Expose(e) if e.count == 0 => self.dirty = true,
            Event::ConfigureNotify(e) if e.window == self.x.win => {
                let (w, h) = (e.width as i32, e.height as i32);
                if w != self.cv.w || h != self.cv.h {
                    self.cv.resize(w, h);
                    self.dirty = true;
                }
                // Root coordinates for the saved geometry.
                if let Ok(Ok(t)) = self
                    .x
                    .c
                    .translate_coordinates(self.x.win, self.x.root, 0, 0)
                    .map(|c| c.reply())
                {
                    if !self.focus_mode {
                        self.geom = (t.dst_x as i32, t.dst_y as i32, w, h);
                    }
                } else if !self.focus_mode {
                    self.geom.2 = w;
                    self.geom.3 = h;
                }
            }
            Event::KeyPress(e) => self.key_press(e.detail, e.state),
            Event::ButtonPress(e) => {
                self.button_press(e.detail, e.event_x as i32, e.event_y as i32, e.state)
            }
            Event::ButtonRelease(e) => {
                self.button_release(e.detail, e.event_x as i32, e.event_y as i32)
            }
            Event::MotionNotify(e) => self.motion(e.event_x as i32, e.event_y as i32),
            Event::FocusIn(_) => {
                self.win_focused = true;
                self.dirty = true;
            }
            Event::FocusOut(_) => {
                self.win_focused = false;
                self.dirty = true;
            }
            Event::MappingNotify(_) => self.x.reload_keymap(),
            Event::ClientMessage(e) => {
                let d = e.data.as_data32();
                let a = &self.x.atoms;
                if e.type_ == a.wm_protocols && d[0] == a.wm_delete {
                    if self.overlays.is_empty() {
                        self.close();
                    }
                } else if e.type_ == a.xdnd_position {
                    let (src, win, status, copy) = (d[0], self.x.win, a.xdnd_status, a.xdnd_copy);
                    self.x.send_client(src, status, [win, 1, 0, 0, copy]);
                } else if e.type_ == a.xdnd_drop {
                    self.drop_source = Some(d[0]);
                    let _ = self.x.c.convert_selection(
                        self.x.win,
                        a.xdnd_selection,
                        a.uri_list,
                        a.sel_prop,
                        d[2],
                    );
                    let _ = self.x.c.flush();
                }
            }
            Event::SelectionRequest(e) => {
                let text = if e.selection == self.x.atoms.primary {
                    self.primary_text.clone()
                } else {
                    self.clip_text.clone()
                };
                self.x.answer_text(&e, &text);
            }
            Event::SelectionNotify(e) => self.got_selection(e.property, e.selection),
            Event::PropertyNotify(e) => {
                if e.window == self.x.root && e.atom == self.x.atoms.niruwm_theme {
                    if self.prefs.theme == FOLLOW {
                        self.theme_at = Some(Instant::now() + Duration::from_millis(180));
                    }
                } else if e.window == self.x.win
                    && e.atom == self.x.atoms.sel_prop
                    && e.state == Property::NEW_VALUE
                    && self.incoming.is_some()
                {
                    let piece = self.x.take_piece();
                    if piece.is_empty() {
                        let data = self.incoming.take().unwrap_or_default();
                        self.selection_text(&data);
                    } else if let Some(buf) = &mut self.incoming {
                        buf.extend_from_slice(&piece);
                    }
                } else if e.state == Property::DELETE {
                    self.x.incr_next(e.window, e.atom);
                }
            }
            _ => {}
        }
    }

    fn got_selection(&mut self, property: u32, selection: u32) {
        if property == x11rb::NONE {
            self.paste_to = None;
            if selection == self.x.atoms.xdnd_selection {
                self.finish_drop();
            }
            return;
        }
        let Some(data) = self.x.take_selection() else {
            return;
        };
        if selection == self.x.atoms.xdnd_selection {
            let paths = crate::x::parse_uris(&data.unwrap_or_default());
            self.finish_drop();
            if let Some(p) = paths
                .into_iter()
                .find(|p| crate::dialogs::text_file(&p.to_string_lossy()))
            {
                if self.overlays.is_empty() {
                    self.maybe_save_then(After::Load(p));
                }
            }
            return;
        }
        match data {
            None => self.incoming = Some(Vec::new()),
            Some(d) => self.selection_text(&d),
        }
    }

    fn finish_drop(&mut self) {
        if let Some(src) = self.drop_source.take() {
            let (win, fin, copy) = (
                self.x.win,
                self.x.atoms.xdnd_finished,
                self.x.atoms.xdnd_copy,
            );
            self.x.send_client(src, fin, [win, 1, copy, 0, 0]);
        }
    }

    fn selection_text(&mut self, data: &[u8]) {
        let text = String::from_utf8_lossy(data).into_owned();
        if let Some(to) = self.paste_to.take() {
            self.deliver_paste(to, &text);
        }
    }

    fn note(&mut self, m: Msg) {
        match m {
            Msg::Spell(gen, d, hint) => {
                if gen != self.spell_gen || !self.prefs.spell_enabled {
                    return;
                }
                self.spell_state = if d.is_some() {
                    SpellState::Ready
                } else {
                    SpellState::Unavailable
                };
                self.spell_dict = d;
                self.spell_hint = hint;
                self.spell_deferred = false;
                self.spell_pass();
            }
            Msg::SpellChecked => {
                self.spell_inflight = false;
            }
            Msg::Scan(found, root) => {
                if let Some(Overlay::Picker(p, PickKind::QuickOpen(c))) = self.overlays.last_mut() {
                    if c.load(std::sync::atomic::Ordering::Relaxed) {
                        return;
                    }
                    let items: Vec<(String, String)> = found
                        .iter()
                        .map(|f| {
                            (
                                f.strip_prefix(&root)
                                    .unwrap_or(f)
                                    .to_string_lossy()
                                    .into_owned(),
                                f.to_string_lossy().into_owned(),
                            )
                        })
                        .collect();
                    let empty = items.is_empty();
                    p.set_items(items);
                    if empty {
                        p.status = Some("No Markdown or text files found".into());
                    }
                }
            }
            Msg::Recovery(gen, ok) => {
                self.rec_pending = false;
                if !ok && self.rec_gen == gen {
                    self.rec_gen = u64::MAX;
                }
                if self.rec_again {
                    self.rec_again = false;
                    self.recover(false);
                }
            }
            Msg::Render(gen, r, key) => {
                if gen == self.render_gen && self.previewing {
                    self.pv.set_html(&r.html, r.headings, key);
                }
            }
        }
        self.dirty = true;
    }

    fn inotify_events(&mut self) {
        let mut buf = [0u8; 8192];
        let mut file_event = false;
        let mut theme_event = false;
        let name = self
            .path
            .as_ref()
            .map(|p| std::fs::canonicalize(p).unwrap_or(p.clone()))
            .and_then(|p| p.file_name().map(|n| n.to_os_string()));
        loop {
            let n = unsafe {
                libc::read(
                    self.inotify,
                    buf.as_mut_ptr() as *mut libc::c_void,
                    buf.len(),
                )
            };
            if n <= 0 {
                break;
            }
            let mut off = 0usize;
            while off + 16 <= n as usize {
                let wd = i32::from_ne_bytes(buf[off..off + 4].try_into().unwrap());
                let len = u32::from_ne_bytes(buf[off + 12..off + 16].try_into().unwrap()) as usize;
                let raw = &buf[off + 16..(off + 16 + len).min(n as usize)];
                use std::os::unix::ffi::OsStrExt;
                let ename = std::ffi::OsStr::from_bytes(
                    &raw[..raw.iter().position(|b| *b == 0).unwrap_or(raw.len())],
                );
                if wd == self.file_wd && name.as_deref() == Some(ename) {
                    file_event = true;
                }
                if self.theme_wds.contains(&wd) {
                    theme_event = true;
                }
                off += 16 + len;
            }
        }
        if theme_event && self.prefs.theme == FOLLOW {
            self.theme_at = Some(Instant::now() + Duration::from_millis(180));
        }
        if file_event {
            self.external_changed();
        }
    }

    fn external_changed(&mut self) {
        if self.external_prompt {
            return;
        }
        let Some(p) = self.path.clone() else { return };
        if !p.exists() {
            if !self.removed_noticed {
                self.removed_noticed = true;
                self.notice(
                    "File removed on disk",
                    "This file was removed or moved outside NIRUNOTE. Your open text is retained. Use Save As to keep it.",
                );
            }
            return;
        }
        self.removed_noticed = false;
        let mt = std::fs::metadata(&p).and_then(|m| m.modified()).ok();
        if mt.is_none() || mt == self.external_mtime {
            return;
        }
        if self.opts.read_only || !self.is_modified() {
            let keep = self.pv.scroll_y;
            let (c, a) = (self.ed.cursor, self.ed.anchor);
            self.load(&p, true);
            // Stay where the reader was.
            self.ed.cursor = self.ed.doc.clamp(c);
            self.ed.anchor = self.ed.doc.clamp(a);
            if self.previewing && self.scroll_fix.is_none() {
                self.scroll_fix = Some(ScrollFix::Y(keep));
            }
            return;
        }
        self.external_mtime = mt;
        let f = Form::new(
            "File changed on disk",
            vec![Row::Text("This file changed outside NIRUNOTE.".into())],
            &["Reload", "Keep mine", "Save copy…", "Cancel"],
            1,
            3,
        );
        self.overlays.push(Overlay::Form(f, FormKind::External));
        self.dirty = true;
    }

    // ------------------------------------------------------------ timers

    fn timers(&mut self) {
        let now = Instant::now();
        let due = |t: &Option<Instant>| t.is_some_and(|t| now >= t);
        if due(&self.flash.as_ref().map(|f| f.1)) {
            self.flash = None;
            self.dirty = true;
        }
        if due(&self.copy_fb) {
            self.copy_fb = None;
            self.dirty = true;
        }
        if due(&self.save_fb) {
            self.save_fb = None;
            self.dirty = true;
        }
        if due(&self.stats_at) {
            self.stats_at = None;
            self.update_stats();
        }
        if due(&self.spell_at) {
            self.spell_at = None;
            self.spell_pass();
        }
        if due(&self.preview_at) {
            self.update_preview();
        }
        if due(&self.rec_debounce) || due(&self.rec_deadline) {
            self.recover(false);
        }
        if due(&self.theme_at) {
            self.theme_at = None;
            self.configure_theme_watch();
            self.apply_theme();
        }
    }

    fn next_deadline(&self) -> Option<Instant> {
        [
            self.flash.as_ref().map(|f| f.1),
            self.copy_fb,
            self.save_fb,
            self.stats_at,
            self.spell_at,
            self.preview_at,
            self.rec_debounce,
            self.rec_deadline,
            self.theme_at,
        ]
        .into_iter()
        .flatten()
        .min()
    }

    pub fn run_loop(&mut self) -> Res<()> {
        let xfd = {
            use std::os::fd::AsRawFd;
            self.x.c.stream().as_raw_fd()
        };
        let mut offered = false;
        while !self.quit {
            while let Some(ev) = self.x.c.poll_for_event()? {
                self.event(ev);
                if self.quit {
                    break;
                }
            }
            while let Ok(m) = self.rx.try_recv() {
                self.note(m);
            }
            if self.images.collect() {
                self.pv.invalidate();
                self.dirty = true;
            }
            self.timers();
            if self.quit {
                break;
            }
            if self.dirty {
                self.draw();
                self.x.present(&self.cv)?;
                self.dirty = false;
                self.dump_state();
                if !self.shown {
                    self.shown = true;
                    if std::env::var_os("NIRUNOTE_TIMING").is_some() {
                        eprintln!(
                            "nirunote: first frame after {:.1} ms",
                            self.started.elapsed().as_secs_f64() * 1000.0
                        );
                    }
                    if self.opts.focus {
                        self.set_focus_mode(true);
                    }
                    self.fonts.save_cache();
                    let _ = self.x.set_icon(&crate::logo::window_icons());
                }
                if !offered {
                    offered = true;
                    self.offer_recovery();
                    if self.dirty {
                        continue;
                    }
                }
            }
            self.x.c.flush()?;
            let timeout = match self.next_deadline() {
                Some(t) => {
                    let ms = t.saturating_duration_since(Instant::now()).as_millis() as i32 + 1;
                    ms.max(1)
                }
                None => -1,
            };
            let mut fds = [
                libc::pollfd {
                    fd: xfd,
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: self.wake,
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: self.inotify,
                    events: libc::POLLIN,
                    revents: 0,
                },
            ];
            let n = if self.inotify >= 0 { 3 } else { 2 };
            unsafe { libc::poll(fds.as_mut_ptr(), n, timeout) };
            if fds[1].revents & libc::POLLIN != 0 {
                let mut v = 0u64;
                unsafe { libc::read(self.wake, &mut v as *mut u64 as *mut libc::c_void, 8) };
                self.dirty = true;
            }
            if n == 3 && fds[2].revents & libc::POLLIN != 0 {
                self.inotify_events();
            }
        }
        self.store.sync();
        Ok(())
    }

    // ------------------------------------------------------------ drawing

    fn draw(&mut self) {
        let p = self.pal;
        let (w, h) = (self.cv.w, self.cv.h);
        self.cv.reset_clip();
        self.cv.fill(Rect::new(0, 0, w, h), p.bg);
        let px = self.ui_px();
        if !self.focus_mode {
            self.draw_header();
        }
        let page = self.page_rect();
        if self.previewing {
            self.layout_preview();
            let (area, _) = self.preview_page();
            let focus_link = self.focus_link;
            self.pv.draw(
                &mut self.cv,
                &mut self.fonts,
                &mut self.images,
                &p,
                area,
                focus_link,
            );
            if !self.focus_mode {
                self.scrollbar(
                    page,
                    self.pv.scroll_y,
                    self.pv.layout.height,
                    self.pv.view_h,
                );
            }
        } else {
            let tr = self.text_rect();
            self.ed.width = if self.ed.wrap { tr.w } else { 1_000_000 };
            self.ed.view_h = tr.h;
            self.ed.sync(&mut self.fonts);
            self.ed.scroll_y = self.ed.scroll_y.clamp(0, self.ed.max_scroll());
            let spell_dict = if self.prefs.spell_enabled
                && !self.spell_deferred
                && self.stats.1 <= LARGE_DOCUMENT_CHARS
            {
                self.spell_dict.clone()
            } else {
                None
            };
            let look = spell_dict.as_deref().map(SpellLook);
            let focused = self.win_focused && !self.find_focused && self.overlays.is_empty();
            self.ed.draw(
                &mut self.cv,
                &mut self.fonts,
                &p,
                Rect::new(tr.x, tr.y, tr.w, tr.h),
                focused,
                self.prefs.highlight_line,
                look.as_ref().map(|l| l as &dyn SpellLookup),
                None,
            );
            if !self.focus_mode {
                self.scrollbar(page, self.ed.scroll_y, self.ed.total_h, self.ed.view_h);
            }
        }
        if self.find.visible {
            self.draw_find();
        }
        if !self.focus_mode {
            self.draw_status();
        }
        // Overlays, the topmost last.
        if !self.overlays.is_empty() {
            self.cv.tint(Rect::new(0, 0, w, h), 0x000000, 60);
        }
        let area = Rect::new(0, 0, w, h);
        let mut overlays = std::mem::take(&mut self.overlays);
        for o in overlays.iter_mut() {
            match o {
                Overlay::Menu(m, _) => {
                    m.layout(&mut self.fonts, px, area);
                    m.draw(&mut self.cv, &mut self.fonts, &p, px);
                }
                Overlay::Form(f, _) => f.draw(&mut self.cv, &mut self.fonts, &p, area, px),
                Overlay::Picker(pk, kind) => {
                    let size = match kind {
                        PickKind::QuickOpen(_) => (620, 460),
                        PickKind::Palette => (480, 360),
                        _ => (520, 440),
                    };
                    pk.draw(&mut self.cv, &mut self.fonts, &p, area, px, size)
                }
                Overlay::Chooser(c, _) => c.draw(&mut self.cv, &mut self.fonts, &p, area, px),
                Overlay::Compare(c) => c.draw(&mut self.cv, &mut self.fonts, &p, area, px),
            }
        }
        self.overlays = overlays;
    }

    fn scrollbar(&mut self, page: Rect, scroll: i32, total: i32, view: i32) {
        if total <= view || view <= 0 {
            return;
        }
        let track = Rect::new(
            page.x + page.w - self.s(4),
            page.y + self.s(4),
            self.s(4),
            page.h - self.s(8),
        );
        let hh = ((track.h as f32 * view as f32 / total as f32) as i32).max(self.s(28));
        let max = (total - view).max(1);
        let y = track.y + ((track.h - hh) as f32 * scroll as f32 / max as f32) as i32;
        self.cv
            .fill(Rect::new(track.x, y, track.w, hh), self.pal.border);
    }

    fn draw_header(&mut self) {
        let p = self.pal;
        let px = self.ui_px();
        let [menu, copy, prev, focus, save, title] = self.header_buttons();
        let (mx, my) = self.pointer;
        let hover = |r: &Rect| r.contains(mx, my);
        for (r, kind) in [(menu, 0), (copy, 1), (prev, 2), (focus, 3), (save, 4)] {
            if kind == 4 && self.opts.read_only {
                continue;
            }
            let active = kind == 2 && self.previewing;
            if hover(&r) || active {
                self.cv.fill(r, p.select);
            }
            let color = if hover(&r) || active { p.fg } else { p.muted };
            let done =
                (kind == 1 && self.copy_fb.is_some()) || (kind == 4 && self.save_fb.is_some());
            crate::logo::icon(
                &mut self.cv,
                if done { 9 } else { kind },
                r,
                color,
                self.scale,
            );
        }
        // Title and the READ ONLY badge.
        let tw = self.fonts.width(UI, px, &self.title) as i32;
        let badge = "READ ONLY";
        let bpx = 10.0 * self.scale;
        let bw = if self.opts.read_only {
            self.fonts.width(UI | BOLD, bpx, badge) as i32 + self.s(14)
        } else {
            0
        };
        let total = tw + if bw > 0 { bw + self.s(8) } else { 0 };
        let tx = title.x + (title.w - total).max(0) / 2;
        let t = self.title.clone();
        label(
            &mut self.cv,
            &mut self.fonts,
            UI,
            px,
            Rect::new(tx, title.y, title.w.min(tw + 4), title.h),
            &t,
            p.muted,
            0,
        );
        if bw > 0 {
            let br = Rect::new(tx + tw + self.s(8), title.h / 2 - self.s(9), bw, self.s(18));
            self.cv.fill(br, p.accent);
            label(
                &mut self.cv,
                &mut self.fonts,
                UI | BOLD,
                bpx,
                br,
                badge,
                p.bg,
                1,
            );
        }
        // The unsaved marker.
        if self.is_modified() && !self.opts.read_only {
            let r = Rect::new(save.x + save.w + self.s(4), save.y, self.s(14), save.h);
            let rad = self.s(4);
            crate::logo::dot(&mut self.cv, r.x + r.w / 2, r.y + r.h / 2, rad, p.fg);
        }
    }

    fn draw_status(&mut self) {
        let p = self.pal;
        let px = self.ui_px();
        let h = self.status_h();
        let y = self.cv.h - h;
        let r = Rect::new(self.s(14), y, self.cv.w - self.s(28), h);
        let left = self.status_left();
        let lw = label(&mut self.cv, &mut self.fonts, UI, px, r, &left, p.muted, 0);
        if !self.opts.read_only {
            let sp = self.spell_status();
            let sr = Rect::new(r.x + lw + self.s(10), y, r.w - lw - self.s(10), h);
            label(&mut self.cv, &mut self.fonts, UI, px, sr, &sp, p.muted, 0);
        }
        let right = self.status_right();
        let rw = self.fonts.width(UI, px, &right) as i32;
        self.cv.fill(
            Rect::new(r.x + r.w - rw - self.s(8), y, rw + self.s(8), h),
            p.bg,
        );
        label(&mut self.cv, &mut self.fonts, UI, px, r, &right, p.muted, 2);
    }

    fn draw_find(&mut self) {
        let p = self.pal;
        let px = self.ui_px();
        let h = self.find_h();
        let y = self.cv.h - self.status_h() - h;
        let rh = h - self.s(12);
        let ry = y + self.s(5);
        let mut x = self.s(14);
        let right_edge = self.cv.w - self.s(14);
        self.find.hits.clear();
        let show_replace = self.find.replace && !self.previewing;
        let show_buttons = !self.opts.read_only;
        let fixed = self.s(70)
            + self.s(80)
            + 2 * self.s(34)
            + if show_buttons {
                self.s(90) + self.s(54)
            } else {
                0
            }
            + self.s(34)
            + self.s(60);
        let fields = if show_replace { 2 } else { 1 };
        let fw = ((right_edge - x - fixed) / fields).max(self.s(80));
        let q = Rect::new(x, ry, fw, rh);
        let fq = self.find_focused && self.find.focus == 0;
        self.find
            .q
            .draw(&mut self.cv, &mut self.fonts, &p, q, px, fq);
        self.find.hits.push((q, 0));
        x += fw + self.s(8);
        if show_replace {
            let r = Rect::new(x, ry, fw, rh);
            let fr = self.find_focused && self.find.focus == 1;
            self.find
                .r
                .draw(&mut self.cv, &mut self.fonts, &p, r, px, fr);
            self.find.hits.push((r, 1));
            x += fw + self.s(8);
        }
        for (id, text, on) in [(2u8, "Aa", self.find.case), (3, "Word", self.find.word)] {
            let w = self.s(if id == 2 { 54 } else { 70 });
            let r = Rect::new(x, ry, w, rh);
            let fo = self.find_focused && self.find.focus == id;
            crate::ui::checkbox(&mut self.cv, &p, Rect::new(x, ry, 20, rh), on, fo);
            label(
                &mut self.cv,
                &mut self.fonts,
                UI,
                px,
                Rect::new(x + 22, ry, w - 22, rh),
                text,
                p.fg,
                0,
            );
            self.find.hits.push((r, id));
            x += w + self.s(4);
        }
        let mut buttons: Vec<(u8, &str)> = vec![(4, "↑"), (5, "↓")];
        if show_buttons {
            buttons.push((6, "Replace"));
            buttons.push((7, "All"));
        }
        buttons.push((8, "×"));
        for (id, t) in buttons {
            let w = match id {
                6 => self.s(84),
                7 => self.s(50),
                _ => self.s(30),
            };
            let r = Rect::new(x, ry, w, rh);
            let fo = self.find_focused && self.find.focus == id;
            crate::ui::button(&mut self.cv, &mut self.fonts, &p, r, px, t, fo, false);
            self.find.hits.push((r, id));
            x += w + self.s(4);
        }
    }
}

fn expand(s: &str) -> String {
    if let Some(rest) = s.strip_prefix("~/") {
        return theme::home().join(rest).to_string_lossy().into_owned();
    }
    if s == "~" {
        return theme::home().to_string_lossy().into_owned();
    }
    s.to_string()
}

fn word_bounds(flat: &[char], off: usize) -> (usize, usize) {
    let off = off.min(flat.len());
    let mut a = off;
    while a > 0 && crate::doc::is_word(flat[a - 1]) {
        a -= 1;
    }
    let mut b = off;
    while b < flat.len() && crate::doc::is_word(flat[b]) {
        b += 1;
    }
    (a, b)
}

fn line_bounds(flat: &[char], off: usize) -> (usize, usize) {
    let off = off.min(flat.len());
    let mut a = off;
    while a > 0 && flat[a - 1] != '\n' {
        a -= 1;
    }
    let mut b = off;
    while b < flat.len() && flat[b] != '\n' {
        b += 1;
    }
    (a, b)
}

pub fn keysym_char(sym: u32) -> Option<char> {
    match sym {
        0x20..=0x7e | 0xa0..=0xff => char::from_u32(sym),
        0xffb0..=0xffb9 => char::from_u32(sym - 0xffb0 + b'0' as u32),
        0xffaa => Some('*'),
        0xffab => Some('+'),
        0xffad => Some('-'),
        0xffae => Some('.'),
        0xffaf => Some('/'),
        0xffac => Some(','),
        0xff80 => Some(' '),
        0x20ac => Some('€'),
        0x0100_0000..=0x0110_ffff => char::from_u32(sym - 0x0100_0000),
        _ => None,
    }
}

fn dead_alone(sym: u32) -> Option<char> {
    Some(match sym {
        0xfe50 => '`',
        0xfe51 => '´',
        0xfe52 => '^',
        0xfe53 => '~',
        0xfe57 => '¨',
        _ => return None,
    })
}

/// A dead key followed by a letter (Swedish and other layouts).
pub fn compose(dead: u32, c: char) -> Option<char> {
    if c == ' ' {
        return dead_alone(dead);
    }
    let table: &[(char, char)] = match dead {
        0xfe50 => &[
            ('a', 'à'),
            ('e', 'è'),
            ('i', 'ì'),
            ('o', 'ò'),
            ('u', 'ù'),
            ('A', 'À'),
            ('E', 'È'),
            ('I', 'Ì'),
            ('O', 'Ò'),
            ('U', 'Ù'),
        ],
        0xfe51 => &[
            ('a', 'á'),
            ('e', 'é'),
            ('i', 'í'),
            ('o', 'ó'),
            ('u', 'ú'),
            ('y', 'ý'),
            ('A', 'Á'),
            ('E', 'É'),
            ('I', 'Í'),
            ('O', 'Ó'),
            ('U', 'Ú'),
            ('Y', 'Ý'),
        ],
        0xfe52 => &[
            ('a', 'â'),
            ('e', 'ê'),
            ('i', 'î'),
            ('o', 'ô'),
            ('u', 'û'),
            ('A', 'Â'),
            ('E', 'Ê'),
            ('I', 'Î'),
            ('O', 'Ô'),
            ('U', 'Û'),
        ],
        0xfe53 => &[
            ('a', 'ã'),
            ('n', 'ñ'),
            ('o', 'õ'),
            ('A', 'Ã'),
            ('N', 'Ñ'),
            ('O', 'Õ'),
        ],
        0xfe57 => &[
            ('a', 'ä'),
            ('e', 'ë'),
            ('i', 'ï'),
            ('o', 'ö'),
            ('u', 'ü'),
            ('y', 'ÿ'),
            ('A', 'Ä'),
            ('E', 'Ë'),
            ('I', 'Ï'),
            ('O', 'Ö'),
            ('U', 'Ü'),
        ],
        _ => &[],
    };
    table
        .iter()
        .find(|(k, _)| *k == c)
        .map(|(_, v)| *v)
        .or_else(|| dead_alone(dead).map(|_| c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dead_keys_compose_swedish_and_others() {
        assert_eq!(compose(0xfe57, 'a'), Some('ä'));
        assert_eq!(compose(0xfe51, 'e'), Some('é'));
        assert_eq!(compose(0xfe50, ' '), Some('`'));
        assert_eq!(keysym_char(0x01000000 + 0x20ac), Some('€'));
    }

    #[test]
    fn word_and_line_bounds_in_the_preview_text() {
        let flat: Vec<char> = "one two\nthree".chars().collect();
        assert_eq!(word_bounds(&flat, 5), (4, 7));
        assert_eq!(line_bounds(&flat, 10), (8, 13));
    }
}

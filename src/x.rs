//! The X11 side: one window, the keyboard map, presenting the canvas with
//! MIT-SHM (one shared-memory copy per frame) or `PutImage` slices, text on
//! CLIPBOARD and PRIMARY (with INCR for big texts in both directions), file
//! drops (XDND), full screen and the floating (utility) window type.
use crate::canvas::Canvas;
use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::shm::ConnectionExt as _;
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

pub type Res<T> = Result<T, Box<dyn std::error::Error>>;

pub struct Atoms {
    pub wm_protocols: Atom,
    pub wm_delete: Atom,
    pub net_wm_name: Atom,
    pub net_wm_icon: Atom,
    pub net_wm_pid: Atom,
    pub utf8: Atom,
    pub niruwm_theme: Atom,
    pub clipboard: Atom,
    pub primary: Atom,
    pub targets: Atom,
    pub incr: Atom,
    pub text_plain: Atom,
    pub text_plain_utf8: Atom,
    pub uri_list: Atom,
    pub sel_prop: Atom,
    pub xdnd_aware: Atom,
    pub xdnd_position: Atom,
    pub xdnd_status: Atom,
    pub xdnd_drop: Atom,
    pub xdnd_finished: Atom,
    pub xdnd_selection: Atom,
    pub xdnd_copy: Atom,
    pub net_wm_state: Atom,
    pub fullscreen: Atom,
    pub window_type: Atom,
    pub type_normal: Atom,
    pub type_utility: Atom,
}

struct Shm {
    seg: u32,
    addr: *mut u8,
    size: usize,
}

/// A big text another program is fetching from us piece by piece (INCR).
pub struct Outgoing {
    pub requestor: Window,
    pub property: Atom,
    pub target: Atom,
    pub data: Vec<u8>,
    pub sent: usize,
}

pub struct X {
    pub c: RustConnection,
    pub root: Window,
    pub win: Window,
    pub depth: u8,
    gc: Gcontext,
    pub atoms: Atoms,
    shm: Option<Shm>,
    shm_ok: bool,
    min_keycode: u8,
    per: u8,
    keysyms: Vec<u32>,
    cursors: [u32; 4],
    current_cursor: usize,
    pub outgoing: Vec<Outgoing>,
}

fn atom(c: &RustConnection, name: &str) -> Res<Atom> {
    Ok(c.intern_atom(false, name.as_bytes())?.reply()?.atom)
}

/// Pointer shapes over the window.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Pointer {
    Arrow = 0,
    Text = 1,
    Hand = 2,
    Blank = 3,
}

impl X {
    pub fn open(x: i16, y: i16, w: u16, h: u16, title: &str) -> Res<Self> {
        let (c, screen) = x11rb::connect(None)?;
        let s = &c.setup().roots[screen];
        let root = s.root;
        let depth = s.root_depth;
        let visual = s.root_visual;
        let black = s.black_pixel;
        // Ask for every atom first and read the answers after: one round trip.
        let names = [
            "WM_PROTOCOLS",
            "WM_DELETE_WINDOW",
            "_NET_WM_NAME",
            "_NET_WM_ICON",
            "_NET_WM_PID",
            "UTF8_STRING",
            "_NIRUWM_THEME",
            "CLIPBOARD",
            "PRIMARY",
            "TARGETS",
            "INCR",
            "text/plain",
            "text/plain;charset=utf-8",
            "text/uri-list",
            "_NIRUNOTE_SELECTION",
            "XdndAware",
            "XdndEnter",
            "XdndPosition",
            "XdndStatus",
            "XdndLeave",
            "XdndDrop",
            "XdndFinished",
            "XdndSelection",
            "XdndActionCopy",
            "_NET_WM_STATE",
            "_NET_WM_STATE_FULLSCREEN",
            "_NET_WM_WINDOW_TYPE",
            "_NET_WM_WINDOW_TYPE_NORMAL",
            "_NET_WM_WINDOW_TYPE_UTILITY",
        ];
        let cookies: Vec<_> = names
            .iter()
            .map(|n| c.intern_atom(false, n.as_bytes()))
            .collect::<Result<_, _>>()?;
        let mut v = Vec::with_capacity(names.len());
        for ck in cookies {
            v.push(ck.reply()?.atom);
        }
        let atoms = Atoms {
            wm_protocols: v[0],
            wm_delete: v[1],
            net_wm_name: v[2],
            net_wm_icon: v[3],
            net_wm_pid: v[4],
            utf8: v[5],
            niruwm_theme: v[6],
            clipboard: v[7],
            primary: v[8],
            targets: v[9],
            incr: v[10],
            text_plain: v[11],
            text_plain_utf8: v[12],
            uri_list: v[13],
            sel_prop: v[14],
            xdnd_aware: v[15],
            xdnd_position: v[17],
            xdnd_status: v[18],
            xdnd_drop: v[20],
            xdnd_finished: v[21],
            xdnd_selection: v[22],
            xdnd_copy: v[23],
            net_wm_state: v[24],
            fullscreen: v[25],
            window_type: v[26],
            type_normal: v[27],
            type_utility: v[28],
        };
        let _ = atom;
        let win = c.generate_id()?;
        c.create_window(
            depth,
            win,
            root,
            x,
            y,
            w,
            h,
            0,
            WindowClass::INPUT_OUTPUT,
            visual,
            &CreateWindowAux::new()
                .background_pixel(black)
                .bit_gravity(Gravity::NORTH_WEST)
                .event_mask(
                    EventMask::EXPOSURE
                        | EventMask::KEY_PRESS
                        | EventMask::KEY_RELEASE
                        | EventMask::BUTTON_PRESS
                        | EventMask::BUTTON_RELEASE
                        | EventMask::POINTER_MOTION
                        | EventMask::LEAVE_WINDOW
                        | EventMask::STRUCTURE_NOTIFY
                        | EventMask::FOCUS_CHANGE
                        | EventMask::PROPERTY_CHANGE,
                ),
        )?;
        // The theme property on the root window changes when the user picks
        // another NIRU theme.
        c.change_window_attributes(
            root,
            &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
        )?;
        // WM_CLASS "nirunote": NIRUWM's scratchpad slot 1 and window rules
        // match it (`--class nirunote`).
        c.change_property8(
            PropMode::REPLACE,
            win,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            b"nirunote\0NIRUNOTE\0",
        )?;
        c.change_property32(
            PropMode::REPLACE,
            win,
            atoms.xdnd_aware,
            AtomEnum::ATOM,
            &[5],
        )?;
        c.change_property32(
            PropMode::REPLACE,
            win,
            atoms.wm_protocols,
            AtomEnum::ATOM,
            &[atoms.wm_delete],
        )?;
        c.change_property32(
            PropMode::REPLACE,
            win,
            atoms.net_wm_pid,
            AtomEnum::CARDINAL,
            &[std::process::id()],
        )?;
        c.change_property32(
            PropMode::REPLACE,
            win,
            atoms.window_type,
            AtomEnum::ATOM,
            &[atoms.type_normal],
        )?;
        // WM_NORMAL_HINTS: the minimum size of the Qt version (560 x 400).
        let mut hints = [0u32; 18];
        hints[0] = 16; // PMinSize
        hints[5] = 560;
        hints[6] = 400;
        c.change_property32(
            PropMode::REPLACE,
            win,
            AtomEnum::WM_NORMAL_HINTS,
            AtomEnum::WM_SIZE_HINTS,
            &hints,
        )?;
        let gc = c.generate_id()?;
        c.create_gc(gc, win, &CreateGCAux::new().graphics_exposures(0))?;
        let shm_ok = c
            .extension_information(x11rb::protocol::shm::X11_EXTENSION_NAME)?
            .is_some()
            && c.shm_query_version()
                .ok()
                .and_then(|r| r.reply().ok())
                .is_some()
            && std::env::var_os("NIRUNOTE_NO_SHM").is_none();
        let setup = c.setup();
        let (min_keycode, max_keycode) = (setup.min_keycode, setup.max_keycode);
        let map = c
            .get_keyboard_mapping(min_keycode, max_keycode - min_keycode + 1)?
            .reply()?;
        let mut x = X {
            c,
            root,
            win,
            depth,
            gc,
            atoms,
            shm: None,
            shm_ok,
            min_keycode,
            per: map.keysyms_per_keycode,
            keysyms: map.keysyms,
            cursors: [0; 4],
            current_cursor: 0,
            outgoing: Vec::new(),
        };
        x.make_cursors();
        x.set_title(title)?;
        Ok(x)
    }

    pub fn reload_keymap(&mut self) {
        let setup = self.c.setup();
        let (min, max) = (setup.min_keycode, setup.max_keycode);
        if let Ok(Ok(map)) = self
            .c
            .get_keyboard_mapping(min, max - min + 1)
            .map(|c| c.reply())
        {
            self.min_keycode = min;
            self.per = map.keysyms_per_keycode;
            self.keysyms = map.keysyms;
        }
    }

    pub fn set_title(&self, title: &str) -> Res<()> {
        self.c.change_property8(
            PropMode::REPLACE,
            self.win,
            self.atoms.net_wm_name,
            self.atoms.utf8,
            title.as_bytes(),
        )?;
        self.c.change_property8(
            PropMode::REPLACE,
            self.win,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            title.as_bytes(),
        )?;
        Ok(())
    }

    /// `_NET_WM_ICON`: width, height, then ARGB pixels, for every size.
    pub fn set_icon(&self, sizes: &[(u32, u32, Vec<u32>)]) -> Res<()> {
        let mut data = Vec::new();
        for (w, h, px) in sizes {
            data.push(*w);
            data.push(*h);
            data.extend_from_slice(px);
        }
        self.c.change_property32(
            PropMode::REPLACE,
            self.win,
            self.atoms.net_wm_icon,
            AtomEnum::CARDINAL,
            &data,
        )?;
        Ok(())
    }

    pub fn map(&self) -> Res<()> {
        self.c.map_window(self.win)?;
        self.c.flush()?;
        Ok(())
    }

    /// Floating on/off the way Qt's Tool flag does it: hide, change the
    /// window type, show again, so the window manager places it anew.
    pub fn set_utility(&self, on: bool) {
        let t = if on {
            self.atoms.type_utility
        } else {
            self.atoms.type_normal
        };
        let _ = self.c.unmap_window(self.win);
        let _ = self.c.change_property32(
            PropMode::REPLACE,
            self.win,
            self.atoms.window_type,
            AtomEnum::ATOM,
            &[t],
        );
        let _ = self.c.map_window(self.win);
        let _ = self.c.flush();
    }

    /// The keysym of a key, with Shift (column 1) or AltGr (columns 4/5).
    pub fn keysym(&self, keycode: u8, state: KeyButMask) -> u32 {
        let idx = (keycode.saturating_sub(self.min_keycode)) as usize * self.per as usize;
        let shift = state.contains(KeyButMask::SHIFT) ^ state.contains(KeyButMask::LOCK);
        let altgr = state.contains(KeyButMask::MOD5);
        let col = match (altgr, shift) {
            (true, false) => 4,
            (true, true) => 5,
            (false, true) => 1,
            (false, false) => 0,
        };
        let get = |c: usize| self.keysyms.get(idx + c).copied().unwrap_or(0);
        // The number pad follows Num Lock (Mod2).
        let kp = get(1);
        if (0xff80..=0xffbd).contains(&kp) && state.contains(KeyButMask::MOD2) {
            return if state.contains(KeyButMask::SHIFT) {
                get(0)
            } else {
                kp
            };
        }
        let k = get(col);
        if k != 0 {
            return k;
        }
        let base = get(if altgr { 4 } else { 0 });
        if base != 0 {
            if shift && (0x61..=0x7a).contains(&base) {
                return base - 0x20;
            }
            return base;
        }
        get(0)
    }

    /// The unshifted keysym of a key: Ctrl+Shift+8 is "8" on every layout.
    pub fn base_keysym(&self, keycode: u8) -> u32 {
        let idx = (keycode.saturating_sub(self.min_keycode)) as usize * self.per as usize;
        self.keysyms.get(idx).copied().unwrap_or(0)
    }

    fn make_cursors(&mut self) {
        // Glyphs of the X cursor font: left_ptr 68, xterm 152, hand2 60.
        let Ok(font) = self.c.generate_id() else {
            return;
        };
        if self.c.open_font(font, b"cursor").is_err() {
            return;
        }
        for (slot, glyph) in [(0usize, 68u16), (1, 152), (2, 60)] {
            if let Ok(cur) = self.c.generate_id() {
                if self
                    .c
                    .create_glyph_cursor(
                        cur,
                        font,
                        font,
                        glyph,
                        glyph + 1,
                        0,
                        0,
                        0,
                        0xffff,
                        0xffff,
                        0xffff,
                    )
                    .is_ok()
                {
                    self.cursors[slot] = cur;
                }
            }
        }
        let _ = self.c.close_font(font);
        // An invisible pointer for focus mode: a 1x1 empty bitmap.
        if let (Ok(pm), Ok(cur)) = (self.c.generate_id(), self.c.generate_id()) {
            if self.c.create_pixmap(1, pm, self.win, 1, 1).is_ok()
                && self
                    .c
                    .create_cursor(cur, pm, pm, 0, 0, 0, 0, 0, 0, 0, 0)
                    .is_ok()
            {
                self.cursors[3] = cur;
            }
            let _ = self.c.free_pixmap(pm);
        }
        self.current_cursor = usize::MAX;
    }

    pub fn set_pointer(&mut self, p: Pointer) {
        let i = p as usize;
        if self.current_cursor == i || self.cursors[i] == 0 {
            return;
        }
        self.current_cursor = i;
        let _ = self.c.change_window_attributes(
            self.win,
            &ChangeWindowAttributesAux::new().cursor(self.cursors[i]),
        );
    }

    fn ensure_shm(&mut self, bytes: usize) -> bool {
        if !self.shm_ok {
            return false;
        }
        if self.shm.as_ref().is_some_and(|s| s.size >= bytes) {
            return true;
        }
        self.drop_shm();
        let size = bytes.next_multiple_of(1 << 20);
        unsafe {
            let id = libc::shmget(libc::IPC_PRIVATE, size, libc::IPC_CREAT | 0o600);
            if id < 0 {
                self.shm_ok = false;
                return false;
            }
            let addr = libc::shmat(id, std::ptr::null(), 0);
            if addr as isize == -1 {
                libc::shmctl(id, libc::IPC_RMID, std::ptr::null_mut());
                self.shm_ok = false;
                return false;
            }
            let Ok(seg) = self.c.generate_id() else {
                return false;
            };
            let attached = self
                .c
                .shm_attach(seg, id as u32, false)
                .ok()
                .and_then(|ck| ck.check().ok())
                .is_some();
            libc::shmctl(id, libc::IPC_RMID, std::ptr::null_mut());
            if !attached {
                libc::shmdt(addr);
                self.shm_ok = false;
                return false;
            }
            self.shm = Some(Shm {
                seg,
                addr: addr as *mut u8,
                size,
            });
        }
        true
    }

    fn drop_shm(&mut self) {
        if let Some(s) = self.shm.take() {
            let _ = self.c.shm_detach(s.seg);
            unsafe { libc::shmdt(s.addr as *const libc::c_void) };
        }
    }

    /// Copy the canvas to the window and wait until the server has read it.
    pub fn present(&mut self, cv: &Canvas) -> Res<()> {
        let bytes = cv.as_bytes();
        let (w, h) = (cv.w as u16, cv.h as u16);
        if self.ensure_shm(bytes.len()) {
            let s = self.shm.as_ref().unwrap();
            unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), s.addr, bytes.len()) };
            self.c.shm_put_image(
                self.win,
                self.gc,
                w,
                h,
                0,
                0,
                w,
                h,
                0,
                0,
                self.depth,
                ImageFormat::Z_PIXMAP.into(),
                false,
                s.seg,
                0,
            )?;
        } else {
            let max = self.c.maximum_request_bytes().saturating_sub(64).max(4096);
            let row_bytes = cv.w as usize * 4;
            let rows = (max / row_bytes).max(1);
            let mut y = 0usize;
            while y < cv.h as usize {
                let n = rows.min(cv.h as usize - y);
                self.c.put_image(
                    ImageFormat::Z_PIXMAP,
                    self.win,
                    self.gc,
                    w,
                    n as u16,
                    0,
                    y as i16,
                    0,
                    self.depth,
                    &bytes[y * row_bytes..(y + n) * row_bytes],
                )?;
                y += n;
            }
        }
        self.c.get_input_focus()?.reply()?;
        Ok(())
    }

    /// Ask the window manager to toggle full screen (EWMH).
    pub fn set_fullscreen(&self, on: bool) {
        let ev = ClientMessageEvent::new(
            32,
            self.win,
            self.atoms.net_wm_state,
            [u32::from(on), self.atoms.fullscreen, 0, 1, 0],
        );
        let _ = self.c.send_event(
            false,
            self.root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            ev,
        );
        let _ = self.c.flush();
    }

    pub fn send_client(&self, to: Window, kind: Atom, data: [u32; 5]) {
        let ev = ClientMessageEvent::new(32, to, kind, data);
        let _ = self.c.send_event(false, to, EventMask::NO_EVENT, ev);
        let _ = self.c.flush();
    }

    /// Xft.dpi from the X resources, when set.
    pub fn xft_dpi(&self) -> Option<f32> {
        let r = self
            .c
            .get_property(
                false,
                self.root,
                AtomEnum::RESOURCE_MANAGER,
                AtomEnum::STRING,
                0,
                1 << 16,
            )
            .ok()?
            .reply()
            .ok()?;
        let text = String::from_utf8_lossy(&r.value).to_string();
        text.lines()
            .find_map(|l| l.strip_prefix("Xft.dpi:"))
            .and_then(|v| v.trim().parse::<f32>().ok())
            .filter(|v| (48.0..=480.0).contains(v))
    }
}

// ------------------------------------------------------------ selections

impl X {
    pub fn own(&self, which: Atom) {
        let _ = self
            .c
            .set_selection_owner(self.win, which, x11rb::CURRENT_TIME);
        let _ = self.c.flush();
    }

    pub fn owns(&self, which: Atom) -> bool {
        self.c
            .get_selection_owner(which)
            .ok()
            .and_then(|c| c.reply().ok())
            .is_some_and(|r| r.owner == self.win)
    }

    /// Ask for text from a selection (CLIPBOARD or PRIMARY); the answer is
    /// a SelectionNotify.
    pub fn request_text(&self, which: Atom) {
        let _ = self.c.convert_selection(
            self.win,
            which,
            self.atoms.utf8,
            self.atoms.sel_prop,
            x11rb::CURRENT_TIME,
        );
        let _ = self.c.flush();
    }

    /// Read (and delete) our selection property. `Some(None)` means the
    /// owner sends the data in pieces (INCR) that follow as PropertyNotify.
    pub fn take_selection(&self) -> Option<Option<Vec<u8>>> {
        let r = self
            .c
            .get_property(
                true,
                self.win,
                self.atoms.sel_prop,
                AtomEnum::ANY,
                0,
                1 << 26,
            )
            .ok()?
            .reply()
            .ok()?;
        if r.type_ == self.atoms.incr {
            return Some(None);
        }
        Some(Some(r.value))
    }

    /// One INCR piece; empty means the transfer is complete.
    pub fn take_piece(&self) -> Vec<u8> {
        self.c
            .get_property(
                true,
                self.win,
                self.atoms.sel_prop,
                AtomEnum::ANY,
                0,
                1 << 26,
            )
            .ok()
            .and_then(|c| c.reply().ok())
            .map(|r| r.value)
            .unwrap_or_default()
    }

    fn chunk(&self) -> usize {
        (self.c.maximum_request_bytes() / 4).clamp(4096, 256 * 1024)
    }

    /// Answer a paste request with plain text (INCR when it is big).
    pub fn answer_text(&mut self, e: &SelectionRequestEvent, text: &str) {
        let a = &self.atoms;
        let mut prop = if e.property == x11rb::NONE {
            e.target
        } else {
            e.property
        };
        let text_targets = [
            a.utf8,
            u32::from(AtomEnum::STRING),
            a.text_plain,
            a.text_plain_utf8,
        ];
        if e.target == a.targets {
            let t = [
                a.targets,
                a.utf8,
                u32::from(AtomEnum::STRING),
                a.text_plain,
                a.text_plain_utf8,
            ];
            let _ =
                self.c
                    .change_property32(PropMode::REPLACE, e.requestor, prop, AtomEnum::ATOM, &t);
        } else if text_targets.contains(&e.target) {
            let bytes = text.as_bytes();
            if bytes.len() > self.chunk() {
                let _ = self.c.change_window_attributes(
                    e.requestor,
                    &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
                );
                let _ = self.c.change_property32(
                    PropMode::REPLACE,
                    e.requestor,
                    prop,
                    self.atoms.incr,
                    &[bytes.len() as u32],
                );
                self.outgoing.retain(|o| o.requestor != e.requestor);
                self.outgoing.push(Outgoing {
                    requestor: e.requestor,
                    property: prop,
                    target: e.target,
                    data: bytes.to_vec(),
                    sent: 0,
                });
            } else {
                let _ =
                    self.c
                        .change_property8(PropMode::REPLACE, e.requestor, prop, e.target, bytes);
            }
        } else {
            prop = x11rb::NONE;
        }
        let ev = SelectionNotifyEvent {
            response_type: SELECTION_NOTIFY_EVENT,
            sequence: 0,
            time: e.time,
            requestor: e.requestor,
            selection: e.selection,
            target: e.target,
            property: prop,
        };
        let _ = self
            .c
            .send_event(false, e.requestor, EventMask::NO_EVENT, ev);
        let _ = self.c.flush();
    }

    /// A requestor deleted the property of an INCR transfer: send on.
    pub fn incr_next(&mut self, window: Window, property: Atom) {
        let chunk = self.chunk();
        let Some(i) = self
            .outgoing
            .iter()
            .position(|o| o.requestor == window && o.property == property)
        else {
            return;
        };
        let o = &mut self.outgoing[i];
        let end = (o.sent + chunk).min(o.data.len());
        let piece = o.data[o.sent..end].to_vec();
        let done = piece.is_empty();
        o.sent = end;
        let (req, prop, target) = (o.requestor, o.property, o.target);
        let _ = self
            .c
            .change_property8(PropMode::REPLACE, req, prop, target, &piece);
        if done {
            self.outgoing.remove(i);
        }
        let _ = self.c.flush();
    }
}

pub fn decode_path(s: &str) -> std::path::PathBuf {
    use std::os::unix::ffi::OsStringExt;
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hex = |c: u8| (c as char).to_digit(16);
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    std::path::PathBuf::from(std::ffi::OsString::from_vec(out))
}

/// Paths from text/uri-list.
pub fn parse_uris(data: &[u8]) -> Vec<std::path::PathBuf> {
    let text = String::from_utf8_lossy(data);
    let mut v = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("file://") {
            let rest = rest.strip_prefix("localhost").unwrap_or(rest);
            v.push(decode_path(rest));
        } else if line.starts_with('/') {
            v.push(std::path::PathBuf::from(line));
        }
    }
    v
}

impl Drop for X {
    fn drop(&mut self) {
        self.drop_shm();
        let _ = self.c.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_lists_become_paths() {
        let v = parse_uris(
            b"file:///home/a/My%20notes.md\r\n# comment\r\nfile://localhost/tmp/x.md\r\n",
        );
        assert_eq!(
            v,
            vec![
                std::path::PathBuf::from("/home/a/My notes.md"),
                std::path::PathBuf::from("/tmp/x.md")
            ]
        );
    }
}

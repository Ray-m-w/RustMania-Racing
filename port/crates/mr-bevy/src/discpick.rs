//! Row 15: the in-window disc picker (Windows).
//!
//! When no readable disc is found (discsel.rs `resolve`), a small window
//! asks for the disc image: type or paste (Ctrl+V) the path to the .cue (or
//! its .bin) and press Enter, or press 1..9 for a .cue found beside the
//! executable or in the data folder; Esc (or closing the window) quits.
//! The path is opened with mr-disc; a readable one is written to the data
//! folder's settings.txt as `disc = PATH` (only that line changes) and the
//! game starts in the same process (the Redump SHA-1 check then runs as for
//! any disc: a warning, never a refusal).
//!
//! The window is plain Win32 (user32 / gdi32 / kernel32 through a small
//! extern block, no crate): Bevy's winit event loop can be created only once
//! per process, and the game needs it. It shows the built-in 5x7 font
//! picture of discsel.rs.
//!
//! Tool flags: `--shot PATH` writes the picker's first picture and exits 2;
//! `--picker-keys KEYS` feeds keys without a window (`<enter>`, `<esc>`,
//! `<bs>`, `<paste>` = Ctrl+V, other characters as typed) and, when they do
//! not pick a disc, writes `--picker-shot PATH` (if given) and exits 2.

use std::path::{Path, PathBuf};

/// What a key did.
#[derive(Debug, PartialEq)]
pub enum Step {
    Continue,
    /// A readable disc: the track .bin mr-disc opened.
    Picked(PathBuf),
    Quit,
}

pub struct Picker {
    pub text: String,
    pub choices: Vec<PathBuf>,
    /// The last attempt's result (error lines).
    pub status: Vec<String>,
    pub reason: String,
    /// settings.txt to write (None = nothing written).
    pub settings: Option<PathBuf>,
}

const MAX_TEXT: usize = 1024;

impl Picker {
    pub fn new(reason: &str) -> Picker {
        let mut dirs: Vec<PathBuf> = Vec::new();
        dirs.extend(crate::paths::exe_dir());
        dirs.extend(crate::paths::data_dir().map(Path::to_path_buf));
        Picker {
            text: String::new(),
            choices: find_cues(&dirs),
            status: Vec::new(),
            reason: reason.to_string(),
            settings: crate::paths::data_file("settings.txt"),
        }
    }

    /// One key as WM_CHAR delivers it: printable characters, 0x08
    /// backspace, 0x0D Enter, 0x1B Esc, 0x16 Ctrl+V.
    pub fn key(&mut self, c: char, paste: &dyn Fn() -> Option<String>) -> Step {
        match c {
            '\u{1b}' => return Step::Quit,
            '\u{08}' => {
                self.text.pop();
            }
            '\u{16}' => {
                if let Some(t) = paste() {
                    let line = t.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
                    for ch in line.chars().filter(|c| !c.is_control()) {
                        if self.text.len() < MAX_TEXT {
                            self.text.push(ch);
                        }
                    }
                }
            }
            '\r' | '\n' => {
                if self.text.trim().is_empty() {
                    self.status = vec!["TYPE OR PASTE A PATH FIRST.".into()];
                } else {
                    let t = self.text.clone();
                    return self.try_path(&t);
                }
            }
            '1'..='9' if self.text.is_empty() => {
                let i = c as usize - '1' as usize;
                if let Some(p) = self.choices.get(i).cloned() {
                    return self.try_path(&p.display().to_string());
                }
                self.text.push(c);
            }
            c if !c.is_control() => {
                if self.text.len() < MAX_TEXT {
                    self.text.push(c);
                }
            }
            _ => {}
        }
        Step::Continue
    }

    fn try_path(&mut self, raw: &str) -> Step {
        let clean = raw.trim().trim_matches('"').trim();
        let mut p = PathBuf::from(clean);
        if p.is_relative() {
            if let Ok(cwd) = std::env::current_dir() {
                p = cwd.join(p);
            }
        }
        if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("chd")) {
            self.status = vec!["A .CHD MUST BE EXTRACTED FIRST (CHDMAN EXTRACTCD); THEN GIVE ITS .CUE.".into()];
            return Step::Continue;
        }
        if clean.contains('#') {
            self.status = vec!["SETTINGS.TXT CANNOT HOLD A PATH WITH # IN IT: RENAME THE FOLDER OR FILE.".into()];
            return Step::Continue;
        }
        match mr_disc::Disc::open(&p) {
            Ok(d) => {
                if let Some(s) = &self.settings {
                    if let Err(e) = write_disc_setting(s, &p) {
                        eprintln!("disc picker: cannot write {}: {e}", s.display());
                    } else {
                        eprintln!("disc picker: disc = {} written to {}", p.display(), s.display());
                    }
                }
                Step::Picked(d.bin_path().to_path_buf())
            }
            Err(e) => {
                self.status = vec![format!("CANNOT READ {}: {e}", p.display())];
                Step::Continue
            }
        }
    }

    /// The picture's lines with their colours.
    pub fn lines(&self) -> Vec<(String, [u8; 3])> {
        const W: [u8; 3] = [230, 230, 230];
        const Y: [u8; 3] = [255, 220, 80];
        const R: [u8; 3] = [255, 110, 100];
        const G: [u8; 3] = [140, 230, 140];
        let mut out = Vec::new();
        let mut put = |s: &str, col: [u8; 3]| {
            for l in wrap(&s.to_ascii_uppercase()) {
                out.push((l, col));
            }
        };
        put("MUPPET RACEMANIA NEEDS YOUR OWN DISC IMAGE", Y);
        put("", W);
        put("TYPE OR PASTE (CTRL+V) THE PATH TO YOUR .CUE (OR .BIN), THEN PRESS ENTER:", W);
        put(&format!("> {}_", self.text), G);
        put("", W);
        if !self.choices.is_empty() {
            put("OR PRESS A NUMBER FOR A DISC IMAGE FOUND HERE:", W);
            for (i, c) in self.choices.iter().enumerate() {
                put(&format!("  {}  {}", i + 1, c.display()), W);
            }
            put("", W);
        }
        for s in &self.status {
            put(s, R);
        }
        if !self.status.is_empty() {
            put("", W);
        }
        if let Some(s) = &self.settings {
            put(&format!("THE PATH IS SAVED AS DISC = ... IN {}.", s.display()), W);
        }
        put("A .CHD MUST BE EXTRACTED FIRST (CHDMAN EXTRACTCD). SEE README-PORT.TXT.", W);
        put(&format!("WHY: {}", self.reason), W);
        put("", W);
        put("ESC QUITS.", W);
        out
    }

    pub fn pixels(&self) -> Vec<u8> {
        crate::discsel::render_colored(&self.lines())
    }
}

fn wrap(s: &str) -> Vec<String> {
    let cols = crate::discsel::COLS;
    let mut out = Vec::new();
    let mut line = String::new();
    for ch in s.chars() {
        if line.chars().count() == cols {
            out.push(std::mem::take(&mut line));
            line.push_str("  ");
        }
        line.push(ch);
    }
    out.push(line);
    out
}

/// The .cue files in `dirs` (sorted, at most nine).
pub fn find_cues(dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for d in dirs {
        let Ok(rd) = std::fs::read_dir(d) else { continue };
        let mut here: Vec<PathBuf> = rd
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("cue")))
            .collect();
        here.sort();
        for p in here {
            let key = p.canonicalize().unwrap_or_else(|_| p.clone());
            if !out.iter().any(|q| q.canonicalize().unwrap_or_else(|_| q.clone()) == key) {
                out.push(p);
            }
        }
    }
    out.truncate(9);
    out
}

/// Set `disc = PATH` in a settings file: the first `disc` line is replaced,
/// otherwise the line is appended (a missing file starts as the default
/// settings text). Every other line stays as it is.
pub fn write_disc_setting(file: &Path, disc: &Path) -> std::io::Result<()> {
    let text = match std::fs::read_to_string(file) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => crate::settings::Settings::default().to_text(),
        Err(e) => return Err(e),
    };
    let new_line = format!("disc = {}", disc.display());
    let mut done = false;
    let mut out: Vec<String> = Vec::new();
    for l in text.lines() {
        let body = l.split('#').next().unwrap_or("");
        let is_disc = body.split_once('=').is_some_and(|(k, _)| k.trim().eq_ignore_ascii_case("disc"));
        if is_disc && !done {
            out.push(new_line.clone());
            done = true;
        } else {
            out.push(l.to_string());
        }
    }
    if !done {
        out.push(new_line);
    }
    let mut s = out.join("\n");
    s.push('\n');
    std::fs::write(file, s)
}

/// `--picker-keys` tokens.
pub fn parse_keys(s: &str) -> Vec<char> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(c) = rest.chars().next() {
        let tok = [("<enter>", '\r'), ("<esc>", '\u{1b}'), ("<bs>", '\u{08}'), ("<paste>", '\u{16}')]
            .into_iter()
            .find(|(t, _)| rest.starts_with(t));
        match tok {
            Some((t, k)) => {
                out.push(k);
                rest = &rest[t.len()..];
            }
            None => {
                out.push(c);
                rest = &rest[c.len_utf8()..];
            }
        }
    }
    out
}

/// Run the picker. Returns the track .bin of the chosen disc; exits the
/// process (code 2) on Esc / close, or after a tool run that picks nothing.
pub fn run(reason: &str) -> PathBuf {
    eprintln!("Muppet RaceMania: {reason}. Choose the disc image in the window (or set `disc = <your .cue>` in settings.txt in the data folder).");
    let mut p = Picker::new(reason);
    if let Some(keys) = crate::settings::arg_value("--picker-keys") {
        for k in parse_keys(&keys) {
            match p.key(k, &clipboard_text) {
                Step::Continue => {}
                Step::Picked(bin) => return bin,
                Step::Quit => std::process::exit(2),
            }
        }
        if let Some(shot) = crate::settings::arg_value("--picker-shot") {
            let _ = std::fs::write(&shot, png(&p));
        }
        std::process::exit(2);
    }
    if let Some(shot) = crate::settings::arg_value("--shot") {
        let _ = std::fs::write(&shot, png(&p));
        std::process::exit(2);
    }
    match win::run(p) {
        Some(bin) => bin,
        None => std::process::exit(2),
    }
}

fn png(p: &Picker) -> Vec<u8> {
    mr_assets::img::png_rgba(crate::discsel::IW as u32, crate::discsel::IH as u32, &p.pixels())
}

/// The clipboard's text (CF_UNICODETEXT), if any.
pub fn clipboard_text() -> Option<String> {
    win::clipboard()
}

#[allow(non_snake_case, clippy::upper_case_acronyms)]
mod win {
    use super::{Picker, Step};
    use std::cell::RefCell;
    use std::ffi::c_void;
    use std::path::PathBuf;

    type WndProc = unsafe extern "system" fn(isize, u32, usize, isize) -> isize;

    #[repr(C)]
    struct WNDCLASSEXW {
        cbSize: u32,
        style: u32,
        lpfnWndProc: WndProc,
        cbClsExtra: i32,
        cbWndExtra: i32,
        hInstance: isize,
        hIcon: isize,
        hCursor: isize,
        hbrBackground: isize,
        lpszMenuName: *const u16,
        lpszClassName: *const u16,
        hIconSm: isize,
    }
    #[repr(C)]
    #[derive(Default)]
    struct POINT {
        x: i32,
        y: i32,
    }
    #[repr(C)]
    #[derive(Default)]
    struct RECT {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }
    #[repr(C)]
    #[derive(Default)]
    struct MSG {
        hwnd: isize,
        message: u32,
        wParam: usize,
        lParam: isize,
        time: u32,
        pt: POINT,
        lPrivate: u32,
    }
    #[repr(C)]
    struct PAINTSTRUCT {
        hdc: isize,
        fErase: i32,
        rcPaint: RECT,
        fRestore: i32,
        fIncUpdate: i32,
        rgbReserved: [u8; 32],
    }
    #[repr(C)]
    struct BITMAPINFO {
        biSize: u32,
        biWidth: i32,
        biHeight: i32,
        biPlanes: u16,
        biBitCount: u16,
        biCompression: u32,
        biSizeImage: u32,
        biXPelsPerMeter: i32,
        biYPelsPerMeter: i32,
        biClrUsed: u32,
        biClrImportant: u32,
        bmiColors: [u32; 1],
    }

    #[cfg(windows)]
    #[link(name = "user32")]
    extern "system" {
        fn RegisterClassExW(c: *const WNDCLASSEXW) -> u16;
        fn CreateWindowExW(ex: u32, class: *const u16, name: *const u16, style: u32, x: i32, y: i32, w: i32, h: i32, parent: isize, menu: isize, inst: isize, param: *mut c_void) -> isize;
        fn DefWindowProcW(h: isize, m: u32, w: usize, l: isize) -> isize;
        fn GetMessageW(m: *mut MSG, h: isize, a: u32, b: u32) -> i32;
        fn TranslateMessage(m: *const MSG) -> i32;
        fn DispatchMessageW(m: *const MSG) -> isize;
        fn PostQuitMessage(code: i32);
        fn InvalidateRect(h: isize, r: *const RECT, erase: i32) -> i32;
        fn BeginPaint(h: isize, p: *mut PAINTSTRUCT) -> isize;
        fn EndPaint(h: isize, p: *const PAINTSTRUCT) -> i32;
        fn DestroyWindow(h: isize) -> i32;
        fn LoadCursorW(inst: isize, name: *const u16) -> isize;
        fn GetClientRect(h: isize, r: *mut RECT) -> i32;
        fn AdjustWindowRect(r: *mut RECT, style: u32, menu: i32) -> i32;
        fn OpenClipboard(h: isize) -> i32;
        fn CloseClipboard() -> i32;
        fn GetClipboardData(format: u32) -> isize;
    }
    #[cfg(windows)]
    #[link(name = "gdi32")]
    extern "system" {
        fn StretchDIBits(hdc: isize, xd: i32, yd: i32, wd: i32, hd: i32, xs: i32, ys: i32, ws: i32, hs: i32, bits: *const c_void, bmi: *const BITMAPINFO, usage: u32, rop: u32) -> i32;
    }
    #[cfg(windows)]
    #[link(name = "kernel32")]
    extern "system" {
        fn GetModuleHandleW(name: *const u16) -> isize;
        fn GlobalLock(h: isize) -> *mut c_void;
        fn GlobalUnlock(h: isize) -> i32;
    }

    const WM_DESTROY: u32 = 0x0002;
    const WM_SIZE: u32 = 0x0005;
    const WM_PAINT: u32 = 0x000F;
    const WM_CLOSE: u32 = 0x0010;
    const WM_ERASEBKGND: u32 = 0x0014;
    const WM_CHAR: u32 = 0x0102;
    const WS_OVERLAPPEDWINDOW: u32 = 0x00CF_0000;
    const WS_VISIBLE: u32 = 0x1000_0000;
    const CW_USEDEFAULT: i32 = 0x8000_0000_u32 as i32;
    const CF_UNICODETEXT: u32 = 13;

    struct State {
        picker: Picker,
        /// BGRA, top-down.
        bgra: Vec<u8>,
        result: Option<Step>,
    }

    thread_local! {
        static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    fn bgra(p: &Picker) -> Vec<u8> {
        let mut px = p.pixels();
        for c in px.chunks_exact_mut(4) {
            c.swap(0, 2);
        }
        px
    }

    #[cfg(windows)]
    pub fn clipboard() -> Option<String> {
        // SAFETY: the clipboard is opened, read through its locked global
        // handle (a NUL-terminated UTF-16 string for CF_UNICODETEXT) and
        // closed again; nothing is kept past CloseClipboard.
        unsafe {
            if OpenClipboard(0) == 0 {
                return None;
            }
            let mut out = None;
            let h = GetClipboardData(CF_UNICODETEXT);
            if h != 0 {
                let p = GlobalLock(h) as *const u16;
                if !p.is_null() {
                    let mut n = 0;
                    while *p.add(n) != 0 && n < 65536 {
                        n += 1;
                    }
                    out = Some(String::from_utf16_lossy(std::slice::from_raw_parts(p, n)));
                    GlobalUnlock(h);
                }
            }
            CloseClipboard();
            out
        }
    }

    #[cfg(not(windows))]
    pub fn clipboard() -> Option<String> {
        None
    }

    #[cfg(not(windows))]
    pub fn run(p: Picker) -> Option<PathBuf> {
        // No picker window elsewhere: the Bevy "no disc" screen.
        crate::discsel::no_disc_screen(&p.reason)
    }

    #[cfg(windows)]
    unsafe extern "system" fn wndproc(h: isize, m: u32, w: usize, l: isize) -> isize {
        match m {
            WM_CHAR => {
                let c = char::from_u32(w as u32).unwrap_or('\0');
                let done = STATE.with(|s| {
                    let mut s = s.borrow_mut();
                    let Some(st) = s.as_mut() else { return false };
                    let step = st.picker.key(c, &clipboard);
                    st.bgra = bgra(&st.picker);
                    if step == Step::Continue {
                        false
                    } else {
                        st.result = Some(step);
                        true
                    }
                });
                if done {
                    DestroyWindow(h);
                } else {
                    InvalidateRect(h, std::ptr::null(), 0);
                }
                0
            }
            WM_PAINT => {
                let mut ps: PAINTSTRUCT = std::mem::zeroed();
                let hdc = BeginPaint(h, &mut ps);
                let mut r = RECT::default();
                GetClientRect(h, &mut r);
                STATE.with(|s| {
                    if let Some(st) = s.borrow().as_ref() {
                        let (iw, ih) = (crate::discsel::IW as i32, crate::discsel::IH as i32);
                        // the picture scaled by a whole factor, centred (black around)
                        let (cw, ch) = (r.right - r.left, r.bottom - r.top);
                        let k = (cw / iw).min(ch / ih).max(1);
                        let (dw, dh) = (iw * k, ih * k);
                        let bmi = BITMAPINFO {
                            biSize: 40,
                            biWidth: iw,
                            biHeight: -ih,
                            biPlanes: 1,
                            biBitCount: 32,
                            biCompression: 0,
                            biSizeImage: 0,
                            biXPelsPerMeter: 0,
                            biYPelsPerMeter: 0,
                            biClrUsed: 0,
                            biClrImportant: 0,
                            bmiColors: [0],
                        };
                        StretchDIBits(hdc, (cw - dw) / 2, (ch - dh) / 2, dw, dh, 0, 0, iw, ih, st.bgra.as_ptr() as *const c_void, &bmi, 0, 0x00CC_0020);
                    }
                });
                EndPaint(h, &ps);
                0
            }
            WM_SIZE => {
                InvalidateRect(h, std::ptr::null(), 1);
                0
            }
            WM_ERASEBKGND => DefWindowProcW(h, m, w, l),
            WM_CLOSE => {
                STATE.with(|s| {
                    if let Some(st) = s.borrow_mut().as_mut() {
                        st.result.get_or_insert(Step::Quit);
                    }
                });
                DestroyWindow(h);
                0
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(h, m, w, l),
        }
    }

    #[cfg(windows)]
    pub fn run(p: Picker) -> Option<PathBuf> {
        let px = bgra(&p);
        STATE.with(|s| *s.borrow_mut() = Some(State { picker: p, bgra: px, result: None }));
        let class = wide("MuppetRaceManiaDiscPicker");
        let title = wide("Muppet RaceMania (Rust): choose your disc image");
        // SAFETY: standard Win32 window: the class and title buffers outlive
        // the window; the window procedure only touches this thread's STATE.
        unsafe {
            let inst = GetModuleHandleW(std::ptr::null());
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: 0x0003, // CS_HREDRAW | CS_VREDRAW
                lpfnWndProc: wndproc,
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: inst,
                hIcon: 0,
                hCursor: LoadCursorW(0, 32512 as *const u16), // IDC_ARROW
                hbrBackground: 0,
                lpszMenuName: std::ptr::null(),
                lpszClassName: class.as_ptr(),
                hIconSm: 0,
            };
            RegisterClassExW(&wc);
            let mut r = RECT { left: 0, top: 0, right: (crate::discsel::IW * 2) as i32, bottom: (crate::discsel::IH * 2) as i32 };
            AdjustWindowRect(&mut r, WS_OVERLAPPEDWINDOW, 0);
            let h = CreateWindowExW(0, class.as_ptr(), title.as_ptr(), WS_OVERLAPPEDWINDOW | WS_VISIBLE, CW_USEDEFAULT, CW_USEDEFAULT, r.right - r.left, r.bottom - r.top, 0, 0, inst, std::ptr::null_mut());
            if h == 0 {
                eprintln!("disc picker: cannot create the window");
                return None;
            }
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, 0, 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        let st = STATE.with(|s| s.borrow_mut().take())?;
        match st.result {
            Some(Step::Picked(bin)) => Some(bin),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_and_settings() {
        assert_eq!(parse_keys("a<bs>b<enter><paste><esc>"), vec!['a', '\u{08}', 'b', '\r', '\u{16}', '\u{1b}']);
        let tmp = std::env::temp_dir().join(format!("mrm-discpick-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let s = tmp.join("settings.txt");
        std::fs::write(&s, "# top\nmsaa = 2\n# the disc\ndisc = \nvsync = true\n").unwrap();
        write_disc_setting(&s, Path::new("D:\\G\\M (USA).cue")).unwrap();
        assert_eq!(std::fs::read_to_string(&s).unwrap(), "# top\nmsaa = 2\n# the disc\ndisc = D:\\G\\M (USA).cue\nvsync = true\n");
        let (pairs, bad) = crate::settings::parse_text(&std::fs::read_to_string(&s).unwrap());
        assert!(bad.is_empty() && pairs.contains(&("disc".into(), "D:\\G\\M (USA).cue".into())));
        std::fs::write(&s, "msaa = 2").unwrap();
        write_disc_setting(&s, Path::new("x.cue")).unwrap();
        assert_eq!(std::fs::read_to_string(&s).unwrap(), "msaa = 2\ndisc = x.cue\n");
        let n = tmp.join("new.txt");
        write_disc_setting(&n, Path::new("y.cue")).unwrap();
        assert!(std::fs::read_to_string(&n).unwrap().contains("\ndisc = y.cue\n"));

        // keys: typing, backspace, paste (quoted path), a bad path, a choice
        std::fs::write(tmp.join("b.cue"), "").unwrap();
        std::fs::write(tmp.join("a.CUE"), "").unwrap();
        std::fs::write(tmp.join("c.bin"), "").unwrap();
        let mut p = Picker { text: String::new(), choices: find_cues(&[tmp.clone(), tmp.clone()]), status: vec![], reason: "no disc is set".into(), settings: None };
        assert_eq!(p.choices, vec![tmp.join("a.CUE"), tmp.join("b.cue")]);
        let none = || None;
        for c in "ab".chars() {
            assert_eq!(p.key(c, &none), Step::Continue);
        }
        p.key('\u{08}', &none);
        assert_eq!(p.text, "a");
        p.key('\u{08}', &none);
        p.key('\u{16}', &|| Some("\"C:\\nope\\x.chd\"\r\nmore".into()));
        assert_eq!(p.text, "\"C:\\nope\\x.chd\"");
        assert_eq!(p.key('\r', &none), Step::Continue);
        assert!(p.status[0].contains(".CHD"));
        p.text.clear();
        assert_eq!(p.key('2', &none), Step::Continue, "an empty .cue is not a disc");
        assert!(p.status[0].starts_with("CANNOT READ"));
        assert_eq!(p.key('7', &none), Step::Continue);
        assert_eq!(p.text, "7", "no seventh choice: typed");
        assert_eq!(p.key('\u{1b}', &none), Step::Quit);
        let lines = p.lines();
        assert!(lines.iter().all(|(l, _)| l.chars().count() <= crate::discsel::COLS));
        assert!(lines.len() * 9 + 8 <= crate::discsel::IH);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}

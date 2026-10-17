//! Never freeze (Session 7): any panic, anywhere in the app (a Bevy system,
//! the render or audio threads, or the simulation: `mr_sim`'s
//! `unreachable_in_scope!` for a game branch the port does not have yet),
//! ends the program with a message instead of leaving a hung window.
//!
//! The panic hook writes `crash.txt` in the data folder (paths.rs; the message,
//! where it was raised, the thread, the course, the tick, the race clock and
//! mode, the command line and a backtrace when the build has one), prints
//! the same to stderr, shows a message box (Windows; not with
//! `MRM_NO_CRASH_DIALOG` set or under the tool flags that run unattended)
//! and exits with code 101. A simulation tick that fails without a panic
//! reaching the hook ends the same way (`fatal`).
//!
//! Debug builds only: `MRM_TEST_PANIC=N` panics in the simulation system at
//! app tick N (`MRM_TEST_PANIC=session:N` takes the session-error path
//! instead), to check this behaviour.

use std::io::Write;
use std::sync::atomic::{AtomicI32, AtomicI64, AtomicU64, Ordering};
use std::sync::OnceLock;

static TICK: AtomicI64 = AtomicI64::new(0);
static SIM_TICKS: AtomicU64 = AtomicU64::new(0);
static CLOCK: AtomicI32 = AtomicI32::new(0);
static MODE: AtomicI32 = AtomicI32::new(-1);
static COURSE: OnceLock<String> = OnceLock::new();
static HZ: AtomicI32 = AtomicI32::new(0);

/// The simulation's state for the crash log (sim_tick, every tick).
pub fn note(tick: i64, s: &mr_sim::session::Session) {
    TICK.store(tick, Ordering::Relaxed);
    SIM_TICKS.store(s.ticks, Ordering::Relaxed);
    CLOCK.store(s.race_clock(), Ordering::Relaxed);
    MODE.store(s.globals.mode as i32, Ordering::Relaxed);
    HZ.store(s.logic_hz() as i32, Ordering::Relaxed);
    COURSE.get_or_init(|| {
        let c = crate::courses::info();
        if crate::title::on_title() {
            format!("title (course {:#X})", c.id)
        } else {
            format!("{} (course {})", c.name, c.id)
        }
    });
}

/// Install the hook (after the plugins, which may set their own: this one
/// runs the previous hook first).
pub fn install() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        prev(info);
        let msg = info
            .payload()
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| info.payload().downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_else(|| "(no message)".into());
        let at = info.location().map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column())).unwrap_or_default();
        report(&msg, &at);
    }));
}

/// A fatal error outside a panic (a failed simulation tick): log and exit.
pub fn fatal(msg: &str) -> ! {
    report(msg, "")
}

fn report(msg: &str, at: &str) -> ! {
    // One report even if several threads panic at once.
    static ONCE: std::sync::Mutex<bool> = std::sync::Mutex::new(false);
    let first = ONCE.lock().map(|mut g| !std::mem::replace(&mut *g, true)).unwrap_or(false);
    if !first {
        // Another thread is writing the report and will exit.
        loop {
            std::thread::park();
        }
    }
    let bt = std::backtrace::Backtrace::force_capture();
    let thread = std::thread::current().name().unwrap_or("(unnamed)").to_string();
    let mode = MODE.load(Ordering::Relaxed);
    let text = format!(
        "Muppet RaceMania (Rust) {} stopped on an error.\n\
         time (UTC): {}\n\
         message: {msg}\n\
         at: {}\n\
         thread: {thread}\n\
         course: {}\n\
         logic: {} Hz; app tick {}, simulation ticks {}, race clock {}, game mode {}\n\
         command line: {}\n\
         \n\
         backtrace:\n{bt}\n",
        env!("CARGO_PKG_VERSION"),
        utc_now(),
        if at.is_empty() { "(no panic location: a failed simulation tick)" } else { at },
        COURSE.get().map(|s| s.as_str()).unwrap_or("(no race yet)"),
        HZ.load(Ordering::Relaxed),
        TICK.load(Ordering::Relaxed),
        SIM_TICKS.load(Ordering::Relaxed),
        CLOCK.load(Ordering::Relaxed),
        if mode < 0 { "-".to_string() } else { format!("{mode:#X}") },
        crate::settings::args().into_iter().collect::<Vec<_>>().join(" "),
    );
    let path = crate::paths::data_file("crash.txt").or_else(|| std::env::current_exe().ok().map(|p| p.with_file_name("crash.txt"))).unwrap_or_else(|| "crash.txt".into());
    let written = std::fs::File::create(&path).and_then(|mut f| f.write_all(text.as_bytes())).is_ok();
    let where_ = if written { format!("Details: {}", path.display()) } else { format!("(could not write {})", path.display()) };
    eprintln!("\n=== Muppet RaceMania stopped on an error: {msg}\n=== {where_}");
    if dialog_wanted() {
        message_box(&format!("Muppet RaceMania stopped on an error and has to close.\n\n{msg}\n\n{where_}"));
    }
    std::process::exit(101);
}

/// No dialog for unattended runs: MRM_NO_CRASH_DIALOG, or the tool flags.
fn dialog_wanted() -> bool {
    if std::env::var_os("MRM_NO_CRASH_DIALOG").is_some() {
        return false;
    }
    const TOOLS: [&str; 8] = ["--shot", "--route", "--replay", "--replay-capture", "--latency-log", "--fps-log", "--fx-shots", "--title-shot"];
    !crate::settings::args().into_iter().any(|a| TOOLS.contains(&a.as_str()))
}

#[cfg(windows)]
fn message_box(text: &str) {
    #[link(name = "user32")]
    extern "system" {
        fn MessageBoxW(hwnd: *mut core::ffi::c_void, text: *const u16, caption: *const u16, kind: u32) -> i32;
    }
    let w = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let (t, c) = (w(text), w("Muppet RaceMania"));
    // MB_OK | MB_ICONERROR | MB_TOPMOST | MB_SETFOREGROUND
    unsafe {
        MessageBoxW(core::ptr::null_mut(), t.as_ptr(), c.as_ptr(), 0x0 | 0x10 | 0x40000 | 0x10000);
    }
}

#[cfg(not(windows))]
fn message_box(_text: &str) {}

/// Debug builds: the `MRM_TEST_PANIC` trigger (see the module docs).
pub fn test_trigger(tick: i64) {
    if !cfg!(debug_assertions) {
        return;
    }
    static AT: OnceLock<Option<(bool, i64)>> = OnceLock::new();
    let at = AT.get_or_init(|| {
        let v = std::env::var("MRM_TEST_PANIC").ok()?;
        match v.strip_prefix("session:") {
            Some(n) => n.parse().ok().map(|n| (true, n)),
            None => v.parse().ok().map(|n| (false, n)),
        }
    });
    if let Some((session, n)) = *at {
        if tick == n {
            if session {
                fatal(&format!("simulation tick failed: OutOfScope(\"MRM_TEST_PANIC session:{n}\")"));
            }
            panic!("MRM_TEST_PANIC: test panic at app tick {n}");
        }
    }
}

/// UTC "YYYY-MM-DD hh:mm:ss" from the system clock (civil-from-days).
fn utc_now() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}", rem / 3600, rem % 3600 / 60, rem % 60)
}

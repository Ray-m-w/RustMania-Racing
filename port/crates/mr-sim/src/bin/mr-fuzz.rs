//! Headless stability fuzz (Session 7): every race course, the stunt
//! courses' trial (phase 3) and the title,
//! at 30 and 60 Hz logic, with the player's pad mashed at random (all 16
//! bits: holds and taps, idle stretches, reversing, turning round to drive
//! the wrong way, TRIANGLE / SELECT / shoulder buttons, START), through the
//! whole race, past the finish into the results, and on some runs past the
//! 10-minute limit. Every tick also runs the app's low-latency look-ahead
//! (snapshot, a tick with the next buttons, restore) on some runs. Any
//! panic (`unreachable_in_scope!` or other) is reported with course, rate,
//! seed, tick and the panic's source location; the seed reproduces the run.
//!
//! ```text
//! cargo run --release --offline -p mr-sim --bin mr-fuzz -- [--seeds N] [--first S] [--long N] [--hz 30|60] [COURSE...]
//! ```
//! `--seeds N` runs per course and rate (default 4); `--long N` of them
//! (default 1) run 12 minutes of race clock (the 10-minute limit), the
//! others until 30 s after the player's finish (or 9 minutes). COURSE: a
//! name or index; TITLE for the title (course 0x1E). `--test-drive`: TEST
//! DRIVE runs (phase 11, on the frontend course).

use std::cell::RefCell;
use std::sync::{Arc, Mutex};

use mr_sim::course::CourseInfo;
use mr_sim::session::{Session, SessionConfig, RNG_START};

thread_local! {
    static LAST_PANIC: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// xorshift32: the fuzz's own random stream (independent of the game's).
struct Rng(u32);
impl Rng {
    fn next(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }
    fn below(&mut self, n: u32) -> u32 {
        self.next() % n.max(1)
    }
    fn chance(&mut self, pct: u32) -> bool {
        self.below(100) < pct
    }
}

const SELECT: u16 = 0x0001;
const START: u16 = 0x0008;
const UP: u16 = 0x0010;
const RIGHT: u16 = 0x0020;
const DOWN: u16 = 0x0040;
const LEFT: u16 = 0x0080;
const TRIANGLE: u16 = 0x1000;
const CIRCLE: u16 = 0x2000;
const CROSS: u16 = 0x4000;
const SQUARE: u16 = 0x8000;

/// The player's pad: segments of a behaviour, each a few ticks to a few
/// seconds long.
struct Pad {
    rng: Rng,
    left: u32,
    mask: u16,
    tap: u16,
    tap_every: u32,
    t: u32,
}

impl Pad {
    fn new(seed: u32) -> Pad {
        Pad { rng: Rng(seed | 1), left: 0, mask: 0, tap: 0, tap_every: 0, t: 0 }
    }

    fn next(&mut self) -> u16 {
        if self.left == 0 {
            let r = &mut self.rng;
            self.left = match r.below(4) {
                0 => 1 + r.below(4),
                1 => 5 + r.below(30),
                2 => 30 + r.below(120),
                _ => 120 + r.below(600),
            };
            let steer = match r.below(4) {
                0 => LEFT,
                1 => RIGHT,
                _ => 0,
            };
            self.mask = match r.below(12) {
                // idle
                0 => 0,
                // drive
                1..=3 => CROSS | steer,
                // reverse
                4 => CIRCLE | steer,
                // turn round: full lock with throttle (drives the wrong way
                // once it comes round)
                5 => CROSS | if r.chance(50) { LEFT } else { RIGHT },
                // jump / brake mixes
                6 => CROSS | SQUARE | steer,
                // anything at all
                7 | 8 => r.next() as u16,
                // the face and shoulder buttons alone
                9 => (r.next() as u16) & 0xFF0F,
                // TRIANGLE held (camera lock) while driving
                10 => CROSS | TRIANGLE | steer,
                // menu-ish: d-pad, START, SELECT
                _ => [UP, DOWN, START, SELECT, START | CROSS][r.below(5) as usize],
            };
            // Taps over the hold: TRIANGLE, SELECT, L1/L2/R1/R2, START.
            self.tap = match r.below(6) {
                0 => TRIANGLE,
                1 => SELECT,
                2 => 0x0F00 & (r.next() as u16),
                3 => START,
                _ => 0,
            };
            self.tap_every = 2 + r.below(40);
        }
        self.left -= 1;
        self.t += 1;
        let tap = if self.tap != 0 && self.t % self.tap_every == 0 { self.tap } else { 0 };
        self.mask | tap
    }
}

#[derive(Clone, Debug)]
struct Outcome {
    course: String,
    hz: u8,
    seed: u32,
    ticks: u64,
    clock: i32,
    finished: bool,
    mode: i16,
    mixed: bool,
    panic: Option<(u64, String)>,
}

fn panic_text(msg: String) -> String {
    let at = LAST_PANIC.with(|p| p.borrow_mut().take()).unwrap_or_default();
    if at.is_empty() {
        msg
    } else {
        format!("{msg} [{at}]")
    }
}

/// Phase 6: the session kind besides races: ADVENTURE (game mode 0xB) or
/// the "BEAT x" race (race flag 0x10 against the course's character).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Race,
    Adventure,
    Beat,
    /// Phase 6b: what COURSE SECRETS starts on the course (drawn per run):
    /// arenas BOSS CHALLENGE / BATTLE AS BOSS (ARENA/BATTLE: DOGFIGHT /
    /// DOOZER BATTLE); race courses REVERSE RACE / BATTLE RACE; stunt
    /// courses the reversed trial / BATTLE RACE.
    Secrets,
    /// Phase 7: two players (split screen): 2 PLAYER RACE / BATTLE / STUNT
    /// RACE, and on some runs CHICKEN CHASE (race flag 0x40, any course);
    /// player 2's pad mashed by its own stream.
    Two,
    /// Phase 8: the attract demo (all AI, no pad; ends at 0x4651 race clock
    /// or kart 0's finish); stunt courses run as plain races.
    Demo,
    /// Phase 11: TEST DRIVE (game mode 0xE on the frontend course, TITLE
    /// only): a random character in its own vehicle (a random loaded one
    /// when that is not loaded).
    TestDrive,
}

fn run(info: &CourseInfo, title: bool, hz: u8, seed: u32, long: bool, kind: Kind) -> Outcome {
    let name = if title { "TITLE".to_string() } else { info.name.clone() };
    let mut out = Outcome { course: name, hz, seed, ticks: 0, clock: 0, finished: false, mode: 0, mixed: false, panic: None };
    let mut disc = mr_disc::Disc::open_default().expect("open the owner's disc");
    let mut r = Rng(seed.wrapping_mul(0x9E37_79B9) ^ 0x5EED);
    let rand_seed = r.next();
    let rng_start = RNG_START + r.below(0x5DB);
    let mut cfg = if title { SessionConfig::title(rand_seed, rng_start, hz) } else { SessionConfig { logic_hz: hz, ..SessionConfig::course(info) } };
    if title && kind == Kind::TestDrive {
        let exe = mr_sim::tables::ExeImage::parse(&disc.read("SLUS_012.37").expect("exe")).expect("exe");
        cfg.test_drive = true;
        cfg.character = r.below(25) as u8;
        cfg.vehicle = mr_sim::ai::character_entry(&exe, cfg.character).1;
        while !mr_sim::title::vehicle_loaded(cfg.vehicle) {
            cfg.vehicle = r.below(31) as u8;
        }
        out.course = "TEST DRIVE".into();
    }
    if !title {
        cfg.rand_seed = rand_seed;
        cfg.rng_start = rng_start;
        // ADVENTURE: the save's secrets drawn (none / some / all found)
        let flags = [0x101u16, 0x13D, 0x1FF][r.below(3) as usize];
        match kind {
            Kind::Race | Kind::TestDrive => {}
            Kind::Adventure => {
                cfg.adventure = true;
                cfg.course_flags = flags;
                cfg.characters = 0xFF;
                cfg.doozer_vehicle = [6, 25, 27][r.below(3) as usize];
            }
            Kind::Secrets => {
                let pick = r.below(2) == 0;
                match info.race_type {
                    2 if info.id == 0x1B => cfg.race_flags = if pick { 0x80 } else { 0x100 },
                    2 if mr_sim::secrets::bosses(info.id).1 > 0 => {
                        if pick {
                            cfg.boss_challenge = true;
                        } else {
                            cfg.race_flags = 0x20;
                            cfg.boss_pick = r.below(mr_sim::secrets::bosses(info.id).1 as u32) as u8;
                        }
                    }
                    2 => {}
                    3 => cfg.race_flags = if pick { 4 } else { 1 },
                    _ => cfg.race_flags = if pick { if info.id == 0x1F { 10 } else { 2 } } else { 1 },
                }
                cfg.course_flags = flags;
            }
            Kind::Demo => cfg.demo = info.race_type != 3,
            Kind::Two => {
                cfg.players = 2;
                cfg.character = r.below(8) as u8;
                let exe = mr_sim::tables::ExeImage::parse(&disc.read("SLUS_012.37").expect("exe")).expect("exe");
                cfg.vehicle = mr_sim::ai::character_entry(&exe, cfg.character).1;
                cfg.p2_character = r.below(8) as u8;
                cfg.p2_alt_controls = r.chance(30);
                if r.chance(40) {
                    cfg.race_flags = 0x40;
                } else if r.chance(40) {
                    // phase 13: COURSE SECRETS' "- 2 PLAYERS" modes
                    cfg.race_flags = [2u16, 1, 8, 4, 0x80, 0x100][r.below(6) as usize];
                } else if r.chance(30) {
                    // phase 13: TOURNAMENT - 2 PLAYERS (three karts)
                    cfg.league = true;
                    cfg.opponents = 1;
                    cfg.draw_grid = true;
                    let c = (8 + r.below(16)) as u8;
                    cfg.lineup = Some([(c, 0xFF), (c, mr_sim::ai::character_entry(&exe, c).1.min(30)), (0, 0), (0, 0), (0, 0)]);
                }
            }
            Kind::Beat => {
                let c = (8 + r.below(16)) as u8;
                cfg.race_flags = 0x10;
                cfg.opponents = 1;
                cfg.lineup = Some([(c, mr_sim::ai::character_entry(&mr_sim::tables::ExeImage::parse(&disc.read("SLUS_012.37").expect("exe")).expect("exe"), c).1.min(30)); 5]);
                cfg.draw_grid = true;
                cfg.tagged = c as i8;
                cfg.laps = 3;
            }
        }
    }
    let made = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| Session::with_config(&mut disc, cfg)));
    let mut s = match made {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            out.panic = Some((0, format!("load: {e}")));
            return out;
        }
        Err(p) => {
            let msg = p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|x| x.to_string())).unwrap_or_default();
            out.panic = Some((0, panic_text(format!("load panic: {msg}"))));
            return out;
        }
    };
    let mut pad = Pad::new(r.next());
    let mut pad2 = Pad::new(r.next());
    let lookahead = r.chance(50);
    // Mixed runs: the AI drives the player's kart in stretches (so races
    // reach the finish and the results), the pad mashes in between.
    let mixed = !title && r.chance(50);
    let mut auto_left = 0u32;
    let per_s: i32 = if hz == 60 { 60 } else { 30 };
    // Race clock: 300 a second.
    let limit = if long { 12 * 60 * 300 } else { 9 * 60 * 300 };
    let title_ticks = (90 * per_s) as u64;
    let mut after = 0;
    let mut next_b = pad.next();
    loop {
        let b = next_b;
        next_b = pad.next();
        if kind == Kind::Two {
            s.pad2 = pad2.next();
        }
        if mixed && s.kart.finished == 0 {
            if auto_left == 0 {
                s.autopilot = !s.autopilot;
                auto_left = if s.autopilot { 20 + r.below(60) } else { 3 + r.below(15) } * per_s as u32;
            }
            auto_left -= 1;
        }
        if lookahead {
            // pacing.rs `predict`: the coming tick on a throw-away copy.
            let snap = s.snapshot();
            let _ = s.tick(next_b);
            s.restore(&snap);
        }
        if let Err(e) = s.tick(b) {
            out.panic = Some((s.ticks, panic_text(e.to_string())));
            break;
        }
        if s.demo_over() {
            break;
        }
        if title {
            if s.ticks >= title_ticks {
                break;
            }
            continue;
        }
        if s.kart.finished != 0 {
            after += 1;
            if !long && after > 30 * per_s {
                break;
            }
        }
        if s.race_clock() > limit || s.ticks > (14 * 60 * per_s) as u64 {
            break;
        }
    }
    out.ticks = s.ticks;
    out.clock = s.race_clock();
    out.finished = s.kart.finished != 0;
    out.mode = s.globals.mode;
    out.mixed = mixed;
    out
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut seeds = 4u32;
    let mut first = 1u32;
    let mut long_n = 1u32;
    let mut hzs = vec![30u8, 60];
    let mut kind = Kind::Race;
    let mut names = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--seeds" => seeds = it.next().and_then(|x| x.parse().ok()).expect("--seeds N"),
            "--first" => first = it.next().and_then(|x| x.parse().ok()).expect("--first S"),
            "--long" => long_n = it.next().and_then(|x| x.parse().ok()).expect("--long N"),
            "--hz" => hzs = vec![it.next().and_then(|x| x.parse().ok()).expect("--hz 30|60")],
            "--adventure" => kind = Kind::Adventure,
            "--beat" => kind = Kind::Beat,
            "--secrets" => kind = Kind::Secrets,
            "--two" => kind = Kind::Two,
            "--demo" => kind = Kind::Demo,
            "--test-drive" => kind = Kind::TestDrive,
            _ => names.push(a.to_ascii_uppercase()),
        }
    }
    std::env::set_current_dir(mr_sim::oracle::repo_root()).ok();
    // Panics are caught per tick (Session::tick_dt); keep their source
    // location for the report instead of printing each.
    std::panic::set_hook(Box::new(|info| {
        let at = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
        if std::env::var("MR_FUZZ_BT").is_ok() {
            eprintln!("{}", std::backtrace::Backtrace::force_capture());
        }
        LAST_PANIC.with(|p| *p.borrow_mut() = Some(at));
    }));
    let mut disc = mr_disc::Disc::open_default().expect("open the owner's disc");
    let exe = mr_sim::tables::ExeImage::parse(&disc.read("SLUS_012.37").expect("exe")).expect("exe");
    // Race courses and (phase 3) the stunt courses' trial.
    let mut courses: Vec<(CourseInfo, bool)> = CourseInfo::races(&exe).into_iter().chain(CourseInfo::stunts(&exe)).chain(CourseInfo::arenas(&exe)).map(|c| (c, false)).collect();
    if let Some(t) = CourseInfo::read(&exe, mr_sim::title::TITLE_COURSE) {
        courses.push((t, true));
    }
    if kind == Kind::Secrets || kind == Kind::Two {
        courses.retain(|(_, t)| !*t);
    }
    if kind == Kind::TestDrive {
        courses.retain(|(_, t)| *t);
    }
    // (the demo: FUN_80089CA8's courses, races and arenas)
    if kind == Kind::Demo {
        courses.retain(|(c, t)| !*t && c.race_type != 3);
    }
    if !names.is_empty() {
        courses.retain(|(c, t)| names.iter().any(|n| (*t && n == "TITLE") || (!*t && (n.eq_ignore_ascii_case(&c.name) || n == &c.id.to_string()))));
    }
    let mut jobs = Vec::new();
    for (c, t) in &courses {
        for &h in &hzs {
            for k in 0..seeds {
                jobs.push((c.clone(), *t, h, first + k, !*t && k < long_n));
            }
        }
    }
    // Longest first.
    jobs.sort_by_key(|j| !j.4);
    let total = jobs.len();
    let jobs = Arc::new(Mutex::new(jobs));
    let out = Arc::new(Mutex::new(Vec::new()));
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(12);
    let t0 = std::time::Instant::now();
    let hs: Vec<_> = (0..threads)
        .map(|_| {
            let jobs = jobs.clone();
            let out = out.clone();
            std::thread::spawn(move || loop {
                let job = jobs.lock().unwrap().pop();
                let Some((c, t, h, seed, long)) = job else { break };
                let o = run(&c, t, h, seed, long, kind);
                if let Some((tick, m)) = &o.panic {
                    eprintln!("PANIC {} {} Hz seed {}: tick {tick}: {m}", o.course, o.hz, o.seed);
                }
                out.lock().unwrap().push(o);
            })
        })
        .collect();
    for h in hs {
        h.join().unwrap();
    }
    let mut out = std::mem::take(&mut *out.lock().unwrap());
    out.sort_by(|a, b| (a.course.clone(), a.hz, a.seed).cmp(&(b.course.clone(), b.hz, b.seed)));
    let ticks: u64 = out.iter().map(|o| o.ticks).sum();
    let panics: Vec<&Outcome> = out.iter().filter(|o| o.panic.is_some()).collect();
    println!("| course | Hz | runs (mixed autopilot) | ticks | player finished | timed out (10 min) | panics |");
    println!("|---|---|---|---|---|---|---|");
    let mut keys: Vec<(String, u8)> = out.iter().map(|o| (o.course.clone(), o.hz)).collect();
    keys.dedup();
    for (c, h) in keys {
        let v: Vec<&Outcome> = out.iter().filter(|o| o.course == c && o.hz == h).collect();
        println!(
            "| {c} | {h} | {} ({}) | {} | {} | {} | {} |",
            v.len(),
            v.iter().filter(|o| o.mixed).count(),
            v.iter().map(|o| o.ticks).sum::<u64>(),
            v.iter().filter(|o| o.finished && o.clock <= 180_000).count(),
            v.iter().filter(|o| o.clock > 179_999).count(),
            v.iter().filter(|o| o.panic.is_some()).count()
        );
    }
    println!("\n{} runs ({} jobs), {} ticks in {:.1} s on {threads} threads; {} panics", out.len(), total, ticks, t0.elapsed().as_secs_f64(), panics.len());
    for o in panics {
        let (t, m) = o.panic.as_ref().unwrap();
        println!("- {} {} Hz seed {} tick {t}: {m}", o.course, o.hz, o.seed);
    }
}

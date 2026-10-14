//! Headless course survey (Session 6, track P): an AI-driven race (the
//! player's kart on autopilot too) on every race course of the game's
//! course table, at 30 Hz and 60 Hz logic. Reports per course and rate:
//! whether it loads, the first out-of-scope panic (`unreachable_in_scope!`
//! or any other) with its tick, laps completed per kart, the player's
//! finish, and karts that got stuck (moved less than 1/8 of a world unit
//! per tick... i.e. under STUCK_DIST over STUCK_SECONDS while racing).
//!
//! ```text
//! cargo run --release --offline -p mr-sim --bin mr-survey -- [--all] [--hz 30|60] [--seed N] [COURSE...]
//! ```
//! `--all` adds the arenas / stunt courses (they are expected to refuse).
//! Output: a markdown table on stdout.

use std::sync::{Arc, Mutex};

use mr_sim::course::CourseInfo;
use mr_sim::session::{Session, SessionConfig};

const STUCK_SECONDS: f64 = 10.0;
/// World units (TER units; a kart is ~ 1500 wide).
const STUCK_DIST: i64 = 3000;
/// Race-clock limit: 9 minutes (the game hands a kart to the AI at 10).
const LIMIT_CLOCK: i32 = 9 * 60 * 300;

#[derive(Default, Clone)]
struct Report {
    course: String,
    id: u8,
    hz: u8,
    load: Option<String>,
    panic: Option<(u64, String)>,
    laps: Vec<u8>,
    finished: Vec<u8>,
    player_finish: Option<i32>,
    /// (slot, first second, last second) per stuck episode.
    stuck: Vec<(usize, f64, f64)>,
    ticks: u64,
    secs: f64,
}

fn run(info: &CourseInfo, hz: u8, seed: Option<u32>) -> Report {
    let mut r = Report { course: info.name.clone(), id: info.id, hz, ..Default::default() };
    let mut disc = match mr_disc::Disc::open_default() {
        Ok(d) => d,
        Err(e) => {
            r.load = Some(format!("disc: {e:?}"));
            return r;
        }
    };
    let mut cfg = SessionConfig { logic_hz: hz, ..SessionConfig::course(info) };
    if let Some(s) = seed {
        cfg.rand_seed = s;
    }
    let t0 = std::time::Instant::now();
    let made = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| Session::with_config(&mut disc, cfg)));
    let mut s = match made {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            r.load = Some(e.to_string());
            return r;
        }
        Err(p) => {
            let msg = p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|x| x.to_string())).unwrap_or_default();
            r.load = Some(format!("panic: {msg}"));
            return r;
        }
    };
    r.load = None;
    s.autopilot = true;
    let n = 1 + s.others.len();
    let per_s = if hz == 60 { 60.0 } else { 30.0 };
    let window = (STUCK_SECONDS * per_s) as usize;
    let mut hist: Vec<std::collections::VecDeque<[i32; 3]>> = vec![Default::default(); n];
    let mut stuck_since: Vec<Option<u64>> = vec![None; n];
    let mut after_finish = 0u64;
    loop {
        if let Err(e) = s.tick(0) {
            r.panic = Some((s.ticks, e.to_string()));
            break;
        }
        let karts: Vec<&mr_sim::KartState> = std::iter::once(&s.kart).chain(s.others.iter().map(|o| &o.kart)).collect();
        if let Ok(spec) = std::env::var("MR_TRACEK") {
            // slot:first_tick-last_tick
            let (sl, r) = spec.split_once(':').unwrap();
            let (a, b) = r.split_once('-').unwrap();
            let (sl, a, b): (usize, u64, u64) = (sl.parse().unwrap(), a.parse().unwrap(), b.parse().unwrap());
            if (a..=b).contains(&s.ticks) {
                let k = &karts[sl];
                eprintln!("t{} k{sl}: W ({}, {}, {}) pyr ({}, {}, {}) face {:08X} TER ({}, {}, {}) v ({}, {}, {}) hdg {} seg {}/{} aim ({}, {}, {}) btn {:04X} fwd {} side {} gc {} jump {} wall {} wf {} surf {} float {} est {} e4 {}", s.ticks, k.x, k.y, k.z, k.pitch, k.heading, k.roll, k.floor_face_ptr, k.x >> 5, k.y >> 5, k.z >> 5, k.vx, k.vy, k.vz, k.heading, k.path_seg, k.path_pt, k.ai_tx, k.ai_ty, k.ai_tz, k.buttons, k.fwd_speed, k.side_speed, k.ground_contact, k.jump_state, k.wall_hit, k.wall_flag, k.surface, k.float_state, k.speed_est, k.object_e4);
            }
        }
        let racing = s.countdown() == 0;
        for (i, k) in karts.iter().enumerate() {
            if !racing || k.finished != 0 {
                hist[i].clear();
                continue;
            }
            hist[i].push_back([k.x, k.y, k.z]);
            if hist[i].len() > window {
                let a = hist[i].pop_front().unwrap();
                let d = (k.x - a[0]).unsigned_abs() as i64 + (k.y - a[1]).unsigned_abs() as i64 + (k.z - a[2]).unsigned_abs() as i64;
                let now = s.ticks as f64 / per_s;
                if d < STUCK_DIST {
                    match stuck_since[i] {
                        Some(e) => r.stuck[e as usize].2 = now,
                        None => {
                            // A new episode unless the last one of this kart
                            // ended within the window (one episode).
                            match r.stuck.iter().rposition(|x| x.0 == i && now - x.2 <= STUCK_SECONDS) {
                                Some(e) => stuck_since[i] = Some(e as u64),
                                None => {
                                    if std::env::var("MR_STUCK").is_ok() {
                                        eprintln!("stuck k{i} at {now:.1}s: pos ({}, {}, {}) TER ({}, {}, {}) hdg {} seg {}/{} cp {} lap {} btn {:04X} fwd {} gc {} jump {} wall {} flags {:04X} pause {:02X} surf {}", k.x, k.y, k.z, k.x >> 5, k.y >> 5, k.z >> 5, k.heading, k.path_seg, k.path_pt, k.course_point, k.lap, k.buttons, k.fwd_speed, k.ground_contact, k.jump_state, k.wall_hit, k.flags, k.pause_bits, k.surface);
                                    }
                                    r.stuck.push((i, now, now));
                                    stuck_since[i] = Some((r.stuck.len() - 1) as u64);
                                }
                            }
                        }
                    }
                } else {
                    stuck_since[i] = None;
                }
            }
        }
        if s.kart.finished != 0 {
            if r.player_finish.is_none() {
                r.player_finish = Some(s.kart.race_time);
                r.laps = karts.iter().map(|k| k.lap).collect();
                r.finished = karts.iter().map(|k| k.finished).collect();
            }
            after_finish += 1;
            // Keep the AI field running a little (trackside camera, results).
            if after_finish as f64 > 10.0 * per_s {
                break;
            }
        }
        if s.race_clock() > LIMIT_CLOCK {
            break;
        }
    }
    if r.laps.is_empty() {
        let karts: Vec<&mr_sim::KartState> = std::iter::once(&s.kart).chain(s.others.iter().map(|o| &o.kart)).collect();
        r.laps = karts.iter().map(|k| k.lap).collect();
        r.finished = karts.iter().map(|k| k.finished).collect();
    }
    r.ticks = s.ticks;
    r.secs = t0.elapsed().as_secs_f64();
    r
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let all = args.iter().any(|a| a == "--all");
    let mut hzs = vec![30u8, 60];
    let mut seed = None;
    let mut names = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--all" => {}
            "--hz" => hzs = vec![it.next().and_then(|x| x.parse().ok()).expect("--hz 30|60")],
            "--seed" => seed = it.next().and_then(|x| u32::from_str_radix(x.trim_start_matches("0x"), 16).ok()),
            _ => names.push(a.clone()),
        }
    }
    let mut disc = mr_disc::Disc::open_default().expect("open the owner's disc");
    let exe = mr_sim::tables::ExeImage::parse(&disc.read("SLUS_012.37").expect("exe")).expect("exe");
    let mut courses: Vec<CourseInfo> = if all { CourseInfo::all(&exe) } else { CourseInfo::races(&exe) };
    if !names.is_empty() {
        courses.retain(|c| names.iter().any(|n| n.eq_ignore_ascii_case(&c.name) || n == &c.id.to_string()));
    }
    let jobs: Vec<(CourseInfo, u8)> = courses.iter().flat_map(|c| hzs.iter().map(move |&h| (c.clone(), h))).collect();
    let jobs = Arc::new(Mutex::new(jobs.into_iter().enumerate().collect::<Vec<_>>()));
    let out = Arc::new(Mutex::new(Vec::new()));
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(8);
    let hs: Vec<_> = (0..threads)
        .map(|_| {
            let jobs = jobs.clone();
            let out = out.clone();
            std::thread::spawn(move || loop {
                let Some((i, (c, h))) = jobs.lock().unwrap().pop() else { break };
                let r = run(&c, h, seed);
                eprintln!("done {} {} Hz: {}", c.name, h, r.panic.as_ref().map(|p| p.1.as_str()).unwrap_or("ok"));
                out.lock().unwrap().push((i, r));
            })
        })
        .collect();
    for h in hs {
        h.join().unwrap();
    }
    let mut out = std::mem::take(&mut *out.lock().unwrap());
    out.sort_by_key(|x| x.0);
    println!("| course | id | cup/slot | Hz | loads | first panic (tick) | laps per kart (player first) | player finish | stuck karts (slot@s) |");
    println!("|---|---|---|---|---|---|---|---|---|");
    for (_, r) in out {
        let info = courses.iter().find(|c| c.id == r.id).unwrap();
        let load = match &r.load {
            None => "yes".to_string(),
            Some(e) => format!("NO: {e}"),
        };
        let panic = r.panic.as_ref().map(|(t, m)| format!("{} ({t})", m.replace('|', "/"))).unwrap_or_else(|| "-".into());
        let laps = r.laps.iter().zip(r.finished.iter()).map(|(l, f)| format!("{l}{}", match *f { 0 => "", 1 => "F", _ => "K" })).collect::<Vec<_>>().join(" ");
        let fin = r.player_finish.map(|t| format!("{}:{:05.2}", t / 18000, (t % 18000) as f64 / 300.0)).unwrap_or_else(|| "-".into());
        let stuck = if r.stuck.is_empty() { "-".to_string() } else { r.stuck.iter().map(|(i, a, b)| format!("{i}@{a:.0}-{b:.0}s")).collect::<Vec<_>>().join(" ") };
        println!("| {} | {} | {}/{} | {} | {} | {} | {} | {} | {} |", r.course, r.id, info.cup, info.slot, r.hz, load, panic, laps, fin, stuck);
    }
}

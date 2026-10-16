//! Verification of the camera (src/camera.rs) against oracle captures
//! (tools/oracle/capture.py format). Teacher forcing: per tick (the VBlanks
//! where the records change), the oracle kart + globals of the tick and the
//! oracle camera of the previous tick in, our camera compared with the
//! oracle camera of the tick. Captures: lap1P, c_steer (chase), start_probe /
//! start_game (intro from the race load), race3P (chase, finish, trackside).
//! Plus a free run of the Session's camera against start_probe.
//!
//! Needs the local, gitignored inputs: analysis/oracle/* and the disc
//! (disc/SLUS_012.37 and the image). Missing inputs skip with a message.
//! CAMDBG=<tick> prints one teacher-forced tick.
//!
//!     cargo test -p mr-sim --test camera -- --nocapture

use mr_sim::camera::*;
use mr_sim::world::World;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const KART0: u32 = 0x800C_30A8;
const KART_SIZE: usize = 0x504;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// One oracle capture: per-VBlank concatenation of regions.
struct Capture {
    regions: Vec<(u32, usize, usize)>, // (addr, len, offset in frame)
    frame: usize,
    vb: BTreeMap<u32, usize>, // vblank -> frame index
    raw: Vec<u8>,
    ram0: Option<Vec<u8>>,
}

/// Integers inside the first JSON array after `"key":`, in order.
fn json_ints(text: &str, key: &str) -> Vec<i64> {
    let k = format!("\"{}\":", key);
    let start = text.find(&k).expect("key") + k.len();
    let bytes = text.as_bytes();
    let mut i = start;
    while bytes[i] != b'[' {
        i += 1;
    }
    let mut depth = 0;
    let mut out = Vec::new();
    let mut num = String::new();
    loop {
        let c = bytes[i] as char;
        match c {
            '[' => depth += 1,
            ']' => depth -= 1,
            _ => {}
        }
        if c == '-' || c.is_ascii_digit() {
            num.push(c);
        } else if !num.is_empty() {
            out.push(num.parse().unwrap());
            num.clear();
        }
        i += 1;
        if depth == 0 {
            break;
        }
    }
    out
}

impl Capture {
    fn load(name: &str) -> Option<Capture> {
        let d = root().join("analysis/oracle").join(name);
        let meta = std::fs::read_to_string(d.join("meta.json")).ok()?;
        let raw = std::fs::read(d.join("frames.bin")).ok()?;
        let r = json_ints(&meta, "regions");
        let mut regions = Vec::new();
        let mut off = 0;
        for p in r.chunks(2) {
            regions.push((p[0] as u32, p[1] as usize, off));
            off += p[1] as usize;
        }
        let vb = json_ints(&meta, "vb").into_iter().enumerate().map(|(i, v)| (v as u32, i)).collect();
        let ram0 = std::fs::read(d.join("ram0.bin")).ok();
        Some(Capture { regions, frame: off, vb, raw, ram0 })
    }

    fn rd(&self, vb: u32, addr: u32, n: usize) -> Option<&[u8]> {
        let base = self.vb.get(&vb)? * self.frame;
        for &(a, len, off) in &self.regions {
            if a <= addr && addr as usize + n <= a as usize + len {
                let o = base + off + (addr - a) as usize;
                return Some(&self.raw[o..o + n]);
            }
        }
        None
    }

    fn i32(&self, vb: u32, addr: u32) -> Option<i32> {
        Some(i32::from_le_bytes(self.rd(vb, addr, 4)?.try_into().unwrap()))
    }
}

fn tables() -> Option<CameraTables> {
    let exe = std::fs::read(root().join("disc/SLUS_012.37")).ok()?;
    CameraTables::from_exe(&exe)
}

fn ram_trail(ram: &[u8]) -> [TrailEntry; TRAIL_LEN] {
    // kart0+0x2DC -> 0x800D4098 on DOCK.
    let p = u32::from_le_bytes(ram[(KART0 as usize + 0x2DC) & 0x1F_FFFF..][..4].try_into().unwrap());
    let mut t = [TrailEntry::default(); TRAIL_LEN];
    // The game's 12 entries; the sim's longer ring repeats the oldest.
    for (i, e) in t.iter_mut().enumerate() {
        let o = (p as usize & 0x1F_FFFF) + 16 * i.min(mr_sim::camera::GAME_TRAIL_LEN - 1);
        let w = |j: usize| i32::from_le_bytes(ram[o + 4 * j..o + 4 * j + 4].try_into().unwrap());
        *e = TrailEntry { pos: [w(0), w(1), w(2)], count: w(3) };
    }
    t
}

#[derive(Default, Clone)]
struct Stat {
    max: i64,
    first: Option<u32>,
    n_bad: u32,
}

struct Report {
    fields: Vec<(&'static str, Stat)>,
    ticks: u32,
    /// Oracle camera mode -> ticks.
    modes: BTreeMap<i16, u32>,
    walk_rand_ticks: u32,
}

impl Report {
    fn new() -> Self {
        Report { fields: Vec::new(), ticks: 0, modes: BTreeMap::new(), walk_rand_ticks: 0 }
    }
    fn add(&mut self, name: &'static str, tick: u32, err: i64) {
        let s = match self.fields.iter_mut().find(|(n, _)| *n == name) {
            Some((_, s)) => s,
            None => {
                self.fields.push((name, Stat::default()));
                &mut self.fields.last_mut().unwrap().1
            }
        };
        let e = err.abs();
        if e > s.max {
            s.max = e;
        }
        if e != 0 {
            s.n_bad += 1;
            if s.first.is_none() {
                s.first = Some(tick);
            }
        }
    }
    fn print(&self, title: &str) {
        eprintln!("== {} ({} ticks, oracle modes {:?}, path walks needing rand {})", title, self.ticks, self.modes, self.walk_rand_ticks);
        for (n, s) in &self.fields {
            eprintln!(
                "  {:<14} max|err| {:>8}  mismatching ticks {:>5}  first {}",
                n,
                s.max,
                s.n_bad,
                s.first.map(|t| t.to_string()).unwrap_or("-".into())
            );
        }
    }
    fn max(&self, name: &str) -> i64 {
        self.fields.iter().find(|(n, _)| *n == name).map(|(_, s)| s.max).unwrap_or(0)
    }
}

fn ang(a: i16, b: i16) -> i64 {
    let mut d = (a as i32 - b as i32) & 0xFFF;
    if d > 2048 {
        d -= 4096;
    }
    d as i64
}

/// The camera's environment during teacher forcing: ground results from the
/// capture (mode 0) or the TER (mode 2 cuts), rand() chosen so the drawn
/// bit equals the oracle's (mode changes), the TER path walk, and the
/// random byte stream from the captured pointer.
struct OracleEnv<'a> {
    captured: CameraGround,
    use_ter: bool,
    world: Option<&'a World>,
    rand_a: u8,
    rng: u32,
    /// The path walk wanted rand() (a branch with several successors).
    walk_rand: bool,
}

impl CameraEnv for OracleEnv<'_> {
    fn ground(&mut self, x: i32, y: i32, z: i32) -> CameraGround {
        match (self.use_ter, self.world) {
            (true, Some(w)) => {
                let q = w.ter.ground(x, y, z);
                CameraGround {
                    found: q.nearest.is_some(),
                    nearest_y: q.nearest_y,
                    below_y: q.below.map(|s| s.y).unwrap_or(q.nearest_y),
                    below_found: q.below.is_some() || q.nearest.is_some(),
                    alt_found: q.alt.is_some(),
                    alt_y: q.alt.map(|s| s.y).unwrap_or(0),
                    type3_min_y: q.water_y,
                }
            }
            _ => self.captured,
        }
    }
    fn rand(&mut self) -> i32 {
        (self.rand_a as i32) << 14
    }
    fn walk(&mut self, seg: i16, pt: i16, n: i16, backward: bool) -> (i16, i16) {
        let w = self.world.expect("path walk needs the TER");
        let flag = &mut self.walk_rand;
        let mut rand = || {
            *flag = true;
            0
        };
        let wk = if backward {
            mr_sim::ai::walk_backward(w, seg, pt, n, &mut rand)
        } else {
            mr_sim::ai::walk_forward(w, seg, pt, n, false, &mut rand)
        };
        (wk.seg, wk.pt)
    }
    fn path_point(&mut self, seg: i16, pt: i16) -> [i32; 3] {
        let ter = &self.world.expect("path point needs the TER").ter;
        let first = ter.path_segments[seg as u16 as usize].first;
        let p = &ter.path_points[first.wrapping_add(pt) as u16 as usize];
        [p.x, p.y, p.z]
    }
    fn rng_byte(&mut self) -> u8 {
        let b = self.world.expect("random bytes need the exe").rng_byte(self.rng);
        self.rng = self.rng.wrapping_add(1);
        b
    }
}

struct Opts<'a> {
    /// 1: the history push holds the kart position from the start of the
    /// pushing tick (the game), 0: from its end (for comparison).
    push_delay: u32,
    /// The capture has the globals region (ground results, countdown, ...).
    full: bool,
    /// First VBlank (race load for the start captures).
    from_vb: u32,
    /// The history ring at `from_vb`.
    ring0: [TrailEntry; TRAIL_LEN],
    world: Option<&'a World>,
}

/// Teacher-force one capture: every tick from `from_vb`, oracle kart and
/// globals of the tick, oracle camera of the previous tick.
fn run(cap: &Capture, t: &CameraTables, o: &Opts) -> Report {
    let mut rep = Report::new();
    // Tick boundaries: the VBlanks where the kart record or the camera
    // record changed. A tick is normally 2 VBlanks, but the oracle drops
    // frames (dt = 3 at lap1P VBlanks 1373 and 1393), so "tick k = VBlank
    // 2k" is not exact. Camera byte 0x6E alone does not count: the render
    // (FUN_80078F38 @0x800790F4) clears that cut flag and redraws.
    let vbs: Vec<u32> = cap.vb.keys().copied().filter(|&v| v >= o.from_vb).collect();
    let mut ticks: Vec<u32> = vec![vbs[0]];
    let key = |vb: u32| {
        let mut cur = cap.rd(vb, KART0, KART_SIZE).unwrap().to_vec();
        let mut c = cap.rd(vb, ADDR_CAMERA0, CAMERA_SIZE).unwrap().to_vec();
        c[0x6E] = 0;
        cur.extend_from_slice(&c);
        cur
    };
    for w in vbs.windows(2) {
        if key(w[0]) != key(w[1]) {
            ticks.push(w[1]);
        }
    }
    let kart_pos = |vb: u32| -> [i32; 3] {
        let rec = cap.rd(vb, KART0, 12).unwrap();
        [0, 1, 2].map(|i| i32::from_le_bytes(rec[4 * i..4 * i + 4].try_into().unwrap()))
    };
    let countdown = |vb: u32| if o.full { cap.i32(vb, 0x800B_0C70).unwrap() } else { 0 };
    let mut ring = o.ring0;
    let mut walk_rand_ticks = 0;
    for n in 1..ticks.len() {
        let (vb, prev_vb) = (ticks[n], ticks[n - 1]);
        let k = n as u32;
        let dt = cap.i32(vb, 0x800B_0AEC).unwrap();
        // The kart update pushes the history while the countdown is 0; the
        // value it sees is the previous frame's end value (the main loop
        // steps the countdown after the race frame).
        if countdown(prev_vb) == 0 {
            let src = if o.push_delay == 1 { prev_vb } else { vb };
            push_trail(&mut ring, kart_pos(src), dt);
        }
        let rec = cap.rd(vb, KART0, KART_SIZE).unwrap();
        let kart = CameraKart::from_record(rec, Some(ring));
        let globals = if o.full {
            CameraGlobals::from_reader(&|a, n| {
                cap.rd(vb, a, n).map(|s| s.to_vec()).unwrap_or_else(|| {
                    // Pointer targets outside the capture (only the cut-scene
                    // check uses them; both pointers are null in a race).
                    vec![0; n]
                })
            })
        } else {
            let mut g = CameraGlobals::dock_race();
            g.dt = dt;
            g.game_state = i16::from_le_bytes(cap.rd(vb, 0x800B_0D46, 2).unwrap().try_into().unwrap());
            g
        };
        let captured = if o.full {
            CameraGround {
                found: cap.i32(vb, 0x800B_0B0C).unwrap() != 0,
                nearest_y: cap.i32(vb, 0x800B_0B24).unwrap(),
                below_y: cap.i32(vb, 0x800B_0B04).unwrap(),
                below_found: cap.i32(vb, 0x800B_0AE0).unwrap() != 0,
                alt_found: cap.i32(vb, 0x800B_0C84).unwrap() != 0,
                alt_y: cap.i32(vb, 0x800B_0C9C).unwrap(),
                type3_min_y: cap.i32(vb, 0x800B_0A54).unwrap(),
            }
        } else {
            CameraGround::default()
        };
        let mut cam = CameraState::from_bytes(cap.rd(prev_vb, ADDR_CAMERA0, CAMERA_SIZE).unwrap());
        let want = CameraState::from_bytes(cap.rd(vb, ADDR_CAMERA0, CAMERA_SIZE).unwrap());
        // TRIANGLE's watch target: the other karts' records of the tick (a
        // capture with all six), the course's look points.
        let lp: Vec<[i32; 3]> = o.world.map(|w| w.course.look_points.clone()).unwrap_or_default();
        let watched = resolve_watch(kart.watch_target, |n| cap.rd(vb, KART0 + (KART_SIZE * n) as u32, KART_SIZE), &lp, &[]);
        let inp = CameraInput { kart, globals, watched };
        // The camera is the frame's last reader of the random byte pointer:
        // when it draws a byte, the captured end-of-frame pointer is one past.
        let rng = if o.full { (cap.i32(vb, 0x800B_0BCC).unwrap() as u32).wrapping_sub(1) } else { 0 };
        let mut env = OracleEnv { captured, use_ter: want.mode == 2, world: o.world, rand_a: want.rand_a, rng, walk_rand: false };
        tick(&mut cam, &inp, t, &mut env);
        if std::env::var("CAMDBG").ok().map(|v| v.parse::<u32>().unwrap()) == Some(k) {
            eprintln!("tick {k} vb {vb} prev {prev_vb} countdown {} dt {dt} got {:?} want {:?} ring {:?}", countdown(vb), (cam.pos, cam.weight, cam.mode), (want.pos, want.weight, want.mode), &ring[..10]);
        }
        if env.walk_rand {
            walk_rand_ticks += 1;
        }

        rep.ticks += 1;
        rep.add("pos.x", k, (cam.pos[0] - want.pos[0]) as i64);
        rep.add("pos.z", k, (cam.pos[2] - want.pos[2]) as i64);
        rep.add("pos_div32.x", k, (cam.pos_div32[0] - want.pos_div32[0]) as i64);
        rep.add("pos_div32.y", k, (cam.pos_div32[1] - want.pos_div32[1]) as i64);
        rep.add("pos_div32.z", k, (cam.pos_div32[2] - want.pos_div32[2]) as i64);
        rep.add("weight.x", k, (cam.weight[0] - want.weight[0]) as i64);
        rep.add("weight.y", k, (cam.weight[1] - want.weight[1]) as i64);
        rep.add("weight.z", k, (cam.weight[2] - want.weight[2]) as i64);
        rep.add("yaw", k, ang(cam.yaw, want.yaw));
        rep.add("roll", k, ang(cam.roll, want.roll));
        rep.add("mode", k, (cam.mode - want.mode) as i64);
        if o.full {
            rep.add("pos.y", k, (cam.pos[1] - want.pos[1]) as i64);
            rep.add("pitch", k, ang(cam.pitch, want.pitch));
            let mut mm = 0i64;
            for r in 0..3 {
                for c in 0..3 {
                    mm = mm.max((cam.matrix[r][c] as i64 - want.matrix[r][c] as i64).abs());
                }
            }
            rep.add("matrix", k, mm);
            let mut fm = 0i64;
            for i in 0..3 {
                fm = fm.max((cam.forward[i] as i64 - want.forward[i] as i64).abs());
            }
            rep.add("forward", k, fm);
            rep.add("sub_state/timer", k, ((cam.sub_state - want.sub_state).abs() + (cam.sub_timer - want.sub_timer).abs()) as i64);
            rep.add("roll_ref", k, (cam.roll_ref - want.roll_ref) as i64);
            // Whole record byte-exact?
            rep.add("record_bytes", k, (cam.to_bytes() != want.raw) as i64);
        }
        *rep.modes.entry(want.mode).or_insert(0) += 1;
    }
    rep.walk_rand_ticks = walk_rand_ticks;
    rep
}

#[test]
fn tables_and_primitives() {
    let Some(t) = tables() else {
        eprintln!("skip: disc/SLUS_012.37 missing");
        return;
    };
    assert_eq!(t.cos_offset, 1024);
    assert_eq!(t.sin12(1024), 4096);
    assert_eq!(t.ratan2(0, 0), 0);
    assert_eq!(t.ratan2(100, 100), 512);
    assert_eq!(t.ratan2(100, -100), 1536);
    assert_eq!(t.ratan2(-100, 0), -1024);
    assert_eq!(t.sqrt0(0), 0);
    for v in [1, 4, 100, 1 << 20, 123_456_789] {
        let s = t.sqrt0(v) as f64;
        // SquareRoot0 is a table approximation (about 0.3% low at worst).
        assert!((s - (v as f64).sqrt()).abs() <= 1.0 + 5e-3 * s, "sqrt0({v}) = {s}");
    }
    // Same tables in the oracle's RAM image.
    if let Some(cap) = Capture::load("lap1P") {
        let r = CameraTables::from_ram(cap.ram0.as_ref().unwrap()).unwrap();
        assert_eq!((r.sin, r.ratan, r.sqrt), (t.sin.clone(), t.ratan.clone(), t.sqrt.clone()));
    }
    // CameraState round trip.
    let mut b = [0u8; CAMERA_SIZE];
    for (i, x) in b.iter_mut().enumerate() {
        *x = (i * 37 + 11) as u8;
    }
    assert_eq!(CameraState::from_bytes(&b).to_bytes(), b);
}

fn first_vb(cap: &Capture) -> u32 {
    *cap.vb.keys().next().unwrap()
}

/// The ring at the capture's first VBlank: ram0's (savestate restore), or
/// 12 copies of the kart position.
fn start_ring(cap: &Capture) -> [TrailEntry; TRAIL_LEN] {
    match &cap.ram0 {
        Some(r) => ram_trail(r),
        None => {
            let rec = cap.rd(first_vb(cap), KART0, 12).unwrap();
            let p = [0, 1, 2].map(|j| i32::from_le_bytes(rec[4 * j..4 * j + 4].try_into().unwrap()));
            [TrailEntry { pos: p, count: 2 }; TRAIL_LEN]
        }
    }
}

#[test]
fn teacher_forced_lap1p() {
    let (Some(t), Some(cap)) = (tables(), Capture::load("lap1P")) else {
        eprintln!("skip: analysis/oracle/lap1P or disc/SLUS_012.37 missing");
        return;
    };
    let ring0 = start_ring(&cap);
    let mut o = Opts { push_delay: 0, full: true, from_vb: first_vb(&cap), ring0, world: None };
    let r0 = run(&cap, &t, &o);
    r0.print("lap1P, history push = kart position at end of the pushing tick");
    o.push_delay = 1;
    let r1 = run(&cap, &t, &o);
    r1.print("lap1P, history push = kart position at start of the pushing tick");
    // The game pushes the kart position from the start of the tick
    // (before the kart moves); with that, every byte of the camera record
    // matches on every tick of the lap.
    for (f, _) in &r1.fields {
        assert_eq!(r1.max(f), 0, "lap1P field {f}");
    }
}

#[test]
fn teacher_forced_c_steer() {
    let (Some(t), Some(cap)) = (tables(), Capture::load("c_steer")) else {
        eprintln!("skip: analysis/oracle/c_steer or disc/SLUS_012.37 missing");
        return;
    };
    let o = Opts { push_delay: 1, full: false, from_vb: first_vb(&cap), ring0: start_ring(&cap), world: None };
    let r = run(&cap, &t, &o);
    r.print("c_steer (no ground globals captured: x/z/yaw/roll/weights only)");
    for f in ["pos.x", "pos.z", "weight.x", "weight.z", "yaw", "roll", "mode"] {
        assert_eq!(r.max(f), 0, "c_steer field {f}");
    }
}

fn disc() -> Option<mr_disc::Disc> {
    std::env::set_current_dir(root()).ok()?;
    mr_disc::Disc::open_default().ok()
}

/// The race load in a start capture: countdown 480 with game state 1.
fn load_vb(cap: &Capture) -> u32 {
    *cap.vb.keys().find(|&&v| cap.i32(v, 0x800B_0C70) == Some(480) && cap.rd(v, 0x800B_0D46, 2) == Some(&[1, 0][..])).unwrap()
}

/// The oracle's race load from the course menu (start_probe: six karts,
/// no input; start_game: the start minigame played). From the load: the
/// intro camera (mode 10) while the countdown is >= 300, then the chase
/// camera through the start game and GO. The history ring at the load is
/// the session's race-load ring (the race-load test checks the records).
fn teacher_forced_start(name: &str, cfg: mr_sim::session::SessionConfig) {
    let (Some(t), Some(cap), Some(mut d)) = (tables(), Capture::load(name), disc()) else {
        eprintln!("skip: analysis/oracle/{name}, disc/SLUS_012.37 or the disc missing");
        return;
    };
    let s = mr_sim::session::Session::with_config(&mut d, cfg).unwrap();
    let o = Opts { push_delay: 1, full: true, from_vb: load_vb(&cap), ring0: s.kart.trail, world: Some(&s.world) };
    let r = run(&cap, &t, &o);
    r.print(&format!("{name} from the race load"));
    assert!(r.modes.get(&10).copied().unwrap_or(0) > 80, "intro ticks");
    for (f, _) in &r.fields {
        assert_eq!(r.max(f), 0, "{name} field {f}");
    }
}

#[test]
fn teacher_forced_start_probe() {
    let mut cfg = mr_sim::session::SessionConfig::dock_default();
    cfg.rand_seed = 0x832B_2260;
    cfg.rng_start = 0x8001_FC4E;
    teacher_forced_start("start_probe", cfg);
}

#[test]
fn teacher_forced_start_game() {
    teacher_forced_start("start_game", mr_sim::session::SessionConfig::dock_solo());
}

/// The oracle's 3-lap race (savestate slot 2, race running): the chase
/// camera, the player's finish at VBlank ~7043, then the trackside cameras
/// (mode 2) through the results (game state 10).
#[test]
fn teacher_forced_race3p() {
    let (Some(t), Some(cap), Some(mut d)) = (tables(), Capture::load("race3P"), disc()) else {
        eprintln!("skip: analysis/oracle/race3P, disc/SLUS_012.37 or the disc missing");
        return;
    };
    let world = World::dock(&mut d).unwrap();
    let o = Opts { push_delay: 1, full: true, from_vb: first_vb(&cap), ring0: start_ring(&cap), world: Some(&world) };
    let r = run(&cap, &t, &o);
    r.print("race3P");
    assert!(r.modes.get(&2).copied().unwrap_or(0) > 300, "trackside ticks");
    for (f, _) in &r.fields {
        assert_eq!(r.max(f), 0, "race3P field {f}");
    }
}

/// Free run: the session's own camera from the race load against the
/// oracle's camera record, frame by frame (oracle buttons and dt), through
/// the intro, the start game, GO and the first race ticks (to frame 372,
/// where an oracle AI kart fires an item: items are off in the session).
#[test]
fn session_camera_free_run_start_probe() {
    let (Some(cap), Some(mut d)) = (Capture::load("start_probe"), disc()) else {
        eprintln!("skip: analysis/oracle/start_probe or the disc missing");
        return;
    };
    let mut cfg = mr_sim::session::SessionConfig::dock_default();
    cfg.rand_seed = 0x832B_2260;
    cfg.rng_start = 0x8001_FC4E;
    let mut s = mr_sim::session::Session::with_config(&mut d, cfg).unwrap();
    let load = load_vb(&cap);
    s.water = mr_sim::water::WaterAnim::from_bytes(cap.rd(load, 0x800A_FEF0, 0x20).unwrap());
    let key = |vb: u32| (cap.i32(vb, 0x800B_0C70).unwrap(), cap.i32(vb, 0x800B_0A94).unwrap());
    let vbs: Vec<u32> = cap.vb.keys().copied().filter(|&v| v >= load).collect();
    let ends: Vec<u32> = vbs.windows(2).filter(|w| key(w[0]) != key(w[1])).map(|w| w[0]).collect();
    let mut modes = BTreeMap::new();
    let mut first_bad = None;
    let mut n = 0;
    for (j, &vb) in ends.iter().enumerate().take(372) {
        let btn = u16::from_le_bytes(cap.rd(vb, 0x800B_0CE8, 2).unwrap().try_into().unwrap());
        s.tick_dt(btn, cap.i32(vb, 0x800B_0AEC).unwrap()).unwrap();
        let mut want = cap.rd(vb, ADDR_CAMERA0, CAMERA_SIZE).unwrap().to_vec();
        let mut got = s.camera.to_bytes().to_vec();
        // 0x6E: the render clears the cut flag (FUN_80078F38). 0x4C: the
        // pre-race orbit angle (mode 1, before the load), never touched by
        // modes 10 / 0 / 2; the session starts it at 0.
        for i in [0x4C, 0x4D, 0x4E, 0x4F, 0x6E] {
            want[i] = 0;
            got[i] = 0;
        }
        *modes.entry(s.camera.mode).or_insert(0) += 1;
        n += 1;
        if got != want && first_bad.is_none() {
            let o = CameraState::from_bytes(&want);
            first_bad = Some(j);
            let diff: Vec<String> = (0..CAMERA_SIZE).filter(|&i| got[i] != want[i]).map(|i| format!("{i:#x}:{:02x}/{:02x}", got[i], want[i])).collect();
            eprintln!("bytes {diff:?}");
            eprintln!(
                "frame {j} vb {vb}: mode {} / {}, pos {:?} / {:?}, pyr {:?} / {:?}",
                s.camera.mode,
                o.mode,
                s.camera.pos,
                o.pos,
                (s.camera.pitch, s.camera.yaw, s.camera.roll),
                (o.pitch, o.yaw, o.roll)
            );
        }
    }
    eprintln!("session camera free run (start_probe): {n} frames, modes {modes:?}, first mismatch {first_bad:?}");
    assert_eq!(first_bad, None);
}

/// The session after the player's finish: on autopilot the player finishes,
/// the camera switches to the trackside cameras (mode 2) and keeps cutting
/// through the results (game state 10).
#[test]
fn session_trackside_camera_after_finish() {
    let Some(mut d) = disc() else {
        eprintln!("skip: no disc");
        return;
    };
    let mut s = mr_sim::session::Session::new(&mut d).unwrap();
    s.autopilot = true;
    let mut t = 0;
    let mut intro = 0;
    while s.kart.finished == 0 {
        s.tick(0).unwrap();
        if s.camera.mode == 10 {
            intro += 1;
        }
        t += 1;
        assert!(t < 8000, "the player did not finish");
    }
    assert!(intro > 80, "intro camera ticks: {intro}");
    let mut cuts = 0;
    let mut p = s.camera.pos;
    for _ in 0..600 {
        s.tick(0).unwrap();
        assert_eq!(s.camera.mode, 2);
        if s.camera.pos != p {
            cuts += 1;
            p = s.camera.pos;
        }
    }
    assert_eq!(s.globals.mode, 10);
    eprintln!("trackside: {cuts} cuts in 600 ticks after the finish");
    assert!(cuts >= 2, "cuts: {cuts}");
}

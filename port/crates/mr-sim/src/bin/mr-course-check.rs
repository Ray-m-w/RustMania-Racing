//! Session 6, track P: one course capture (track O's analysis/oracle/courses/
//! NAME: the course menu -> 1 PLAYER RACE, Kermit / RACER, player idle to VB
//! ~3400, 30 Hz) against the port.
//!
//! 1. Race load: a session with the BIOS seed and random pointer the oracle
//!    had at the VBlank before its race load (the first mode-1 VBlank, as
//!    analysis/oracle/start_probe for DOCK) builds the same six records
//!    (every named field) and the same globals.
//! 2. Free run: every frame (the last VBlank of each (countdown, clock)
//!    step) the session ticks with the oracle's pad word and dt; per kart
//!    the first frame a motion field differs.
//! 3. Teacher forced: every frame the session restarts from the oracle's
//!    state of the previous frame (Session::load_mem); per kart and field
//!    the frames that differ after one tick.
//!
//! ```text
//! cargo run --release --offline -p mr-sim --bin mr-course-check -- [NAME...] [--frames N] [--fields]
//! ```

use mr_sim::course::CourseInfo;
use mr_sim::kart::{KartState, FIELD_OFFSETS, KART_ARRAY, RECORD_SIZE};
use mr_sim::oracle::{self, Capture};
use mr_sim::session::{Session, SessionConfig};

const MOTION: [&str; 16] = ["x", "y", "z", "pitch", "heading", "roll", "vx", "vy", "vz", "fwd_speed", "side_speed", "lap", "buttons", "race_pos", "look_target", "item_fire"];

/// The camera fields that differ between the port's camera and the game's
/// (camera 0, 0x800C4F40) at a frame end.
fn camera_diff(s: &Session, m: &mr_sim::mem::Mem) -> Vec<&'static str> {
    let o = mr_sim::camera::CameraState::from_bytes(m.slice(0x800C_4F40, mr_sim::camera::CAMERA_SIZE));
    let c = &s.camera;
    let mut d = Vec::new();
    for (name, same) in [
        ("cam.mode", c.mode == o.mode),
        ("cam.pos", c.pos == o.pos),
        ("cam.pitch", c.pitch == o.pitch),
        ("cam.yaw", c.yaw == o.yaw),
        ("cam.roll", c.roll == o.roll),
        ("cam.weight", c.weight == o.weight),
        ("cam.matrix", c.matrix == o.matrix),
        ("cam.forward", c.forward == o.forward),
        ("cam.sub", (c.sub_state, c.sub_timer) == (o.sub_state, o.sub_timer)),
        ("cam.rand", (c.rand_a, c.rand_b) == (o.rand_a, o.rand_b)),
    ] {
        if !same {
            d.push(name);
        }
    }
    d
}

struct Ends {
    /// The first VBlank of the race load (countdown 480, mode 1).
    load: u32,
    /// Frame ends (the last VBlank of each (countdown, clock) step).
    ends: Vec<u32>,
}

fn frames(cap: &Capture) -> Option<Ends> {
    // (a mode-3 capture's clock plan must not leak into the next course)
    CLOCKS.with(|c| c.borrow_mut().clear());
    let key = |vb: u32| (cap.i32(vb, 0x800B_0C70).unwrap(), cap.i32(vb, 0x800B_0A94).unwrap());
    let load = *cap.vbs.iter().find(|&&v| (key(v).0 == 480 || key(v).0 == 300) && matches!(cap.read(v, 0x800B_0D46, 2).unwrap(), [1, 0] | [2, 0] | [3, 0]))?;
    let vbs: Vec<u32> = cap.vbs.iter().copied().filter(|&v| v >= load).collect();
    let mut ends: Vec<u32> = vbs.windows(2).filter(|w| key(w[0]) != key(w[1])).map(|w| w[0]).collect();
    // The stunt trial (mode 3): its VBlank samples fall either side of the
    // frame boundary (the load takes 3 VBlanks): a frame end is the last
    // sample after the frame's own work (the random pointer 0x800B0BCC,
    // stepped once a frame, has changed), which may already hold the next
    // frame's clock step (the frame counter 0x800B0D90 changed). The
    // session is told where the clock steps go (clock_plan).
    if cap.read(load, 0x800B_0D46, 2).unwrap() == [3, 0] {
        ends = mode3_ends(cap, &vbs, load, (0, 0));
    }
    Some(Ends { load, ends })
}

/// Mode 3 frame ends from `start` (whose (frames, clock steps) are
/// `base`): the last sample of each value of the random pointer 0x800B0BCC
/// (stepped once a frame, after the frame's work); the frame counter
/// 0x800B0D90 counts the clock steps (CLOCKS, clock_plan).
fn mode3_ends(cap: &Capture, vbs: &[u32], start: u32, base: (u32, u32)) -> Vec<u32> {
    let (mut r, mut c) = base;
    let (mut lp, mut lf) = (cap.i32(start, 0x800B_0BCC).unwrap(), cap.i32(start, 0x800B_0D90).unwrap());
    let mut at: Vec<(u32, u32, u32)> = Vec::new();
    for &v in vbs.iter().filter(|&&v| v > start) {
        let (p, f) = (cap.i32(v, 0x800B_0BCC).unwrap(), cap.i32(v, 0x800B_0D90).unwrap());
        if p != lp {
            r += 1;
        }
        if f != lf {
            c += 1;
        }
        (lp, lf) = (p, f);
        at.push((v, r, c));
    }
    let mut ends = Vec::new();
    let mut clocks = std::collections::HashMap::new();
    clocks.insert(start, base);
    if base.0 > 0 || base.1 > 0 {
        ends.push(start);
    }
    for w in at.windows(2) {
        if w[0].1 >= 1 && w[1].1 != w[0].1 {
            ends.push(w[0].0);
            clocks.insert(w[0].0, (w[0].1, w[0].2));
        }
    }
    CLOCKS.with(|c| *c.borrow_mut() = clocks);
    ends
}

/// The game's catch-up reads the leader as kart 0x800D8F94, which the
/// ranking (FUN_800837F8) only writes once the race clock runs: before
/// that a stale index from the previous session can name a kart slot this
/// race does not have (the "BEAT x" race after an ADVENTURE: slot 3, whose
/// stale record still holds a progress). That progress, else None.
fn stale_leader(m: &mr_sim::mem::Mem, karts: usize) -> Option<i32> {
    let i = m.i16(0x800D_8F94);
    (i >= 0 && i as usize >= karts && (i as usize) < 6).then(|| m.i32(KART_ARRAY + 0x504 * i as u32 + 0x388))
}

thread_local! {
    /// Mode 3 captures: per frame-end VBlank, (frames done, clock steps
    /// done) since the load (`frames`).
    static CLOCKS: std::cell::RefCell<std::collections::HashMap<u32, (u32, u32)>> = Default::default();
}

/// The clock plan for a tick from the state at `from` to the one at `to`
/// (mode 3 captures only): a clock step at the start when the frame's own
/// has not run yet, one at the end when `to` holds the next frame's.
fn clock_plan(from: Option<u32>, to: u32) -> Option<(bool, bool)> {
    CLOCKS.with(|c| {
        let c = c.borrow();
        let (r0, c0) = match from {
            Some(f) => *c.get(&f)?,
            None => *c.values().min()?,
        };
        let (_, c1) = *c.get(&to)?;
        // Tick from r0 frames done: frame r0 (0 = the load frame, no clock).
        let start = c0 < r0 || (r0 >= 1 && c0 == 0);
        let end = c1 > c0 + start as u32;
        Some((start, end))
    })
}

/// A session at the oracle's race load.
pub fn session_at_load(disc: &mut mr_disc::Disc, info: &CourseInfo, cap: &Capture, load: u32) -> Session {
    let pre = cap.mem_at(load - 1, None);
    let mut cfg = SessionConfig::course(info);
    if info.id == mr_sim::course::DOCK {
        cfg = SessionConfig::dock_default();
    }
    cfg.rand_seed = pre.u32(0x8000_9010);
    cfg.rng_start = pre.u32(0x800B_0BCC);
    cfg.logic_hz = 30;
    cfg.faces = Some(pre.u32(0x800B_0FA4));
    // The path / progress round robin 0x800C3058[karts] keeps counting
    // between races.
    cfg.round_robin_start = pre.u8(0x800C_3058 + info.karts as u32);
    // The course load (token colours, turntable spins) ran before the
    // capture, from a seed some menu frames before the race load's: the
    // session draws them itself from that seed (track S), found from the
    // menu state (`--course-load`); MR_CLRAM copies them from RAM instead.
    let use_model = std::env::var("MR_CLRAM").is_err();
    if use_model {
        let probe = Session::with_config(disc, cfg.clone()).expect("session");
        let m0 = cap.mem_at(cap.vbs[0], None);
        if let Some(&j) = course_load_search(&probe.world, info, &m0, 1).first() {
            cfg.course_load_seed = Some(mr_sim::session::bios_rand_rewind(m0.u32(0x8000_9010), j as u32));
        }
    }
    let mut s = Session::with_config(disc, cfg).expect("session");
    let m = cap.mem_at(load, None);

    s.water = mr_sim::water::WaterAnim::from_bytes(cap.read(load, 0x800A_FEF0, 0x20).unwrap());
    // Course-load state the session derives from seeds before the race
    // load: the game drew it at its own course load, before the capture.
    let o = mr_sim::objects::ObjectState::read(&m);
    // The phases (render side) run on through the menus: from RAM.
    s.globals.objects.turntable_phase = o.turntable_phase;
    if !use_model {
        s.globals.objects.turntable_spin = o.turntable_spin;
    }
    // Particles, sparks, flat quads and projectiles left in RAM by the
    // menus run on into the race (the race load does not clear them).
    if std::env::var("MR_NOFX").is_err() {
        if let Some(fx) = s.globals.fx.as_mut() {
            fx.particles = mr_sim::particles::Particles::read(&m, fx.particles.types.clone());
            fx.sparks = mr_sim::particles::Sparks::read(&m);
            fx.flats = mr_sim::particles::Flats::read(&m);
        }
        s.globals.items.projectiles = mr_sim::items::ItemState::read(&m).projectiles;
        // The round-robin slot 0x800B07E0 runs on from the menus too.
        if std::env::var("MR_NONEXTPROJ").is_err() {
            s.globals.items.next_projectile = mr_sim::items::ItemState::read(&m).next_projectile;
        }
    }
    // The pickups' colours / timers come from the course load (before the
    // capture).
    // The pickups' animation (grow, bob: FUN_8007BBF0) runs on through
    // the menus after the course load: from RAM; their colours are the
    // session's own course-load draws (model) unless MR_CLRAM.
    if std::env::var("MR_NOPICK").is_err() {
        let ram = mr_sim::items::ItemState::read(&m).pickups;
        let own = s.globals.items.pickups.clone();
        s.globals.items.pickups = ram;
        if use_model {
            let n_tok = s.globals.items.tokens as usize;
            let first = mr_sim::items::FIRST_TOKEN;
            for (i, (p, o)) in s.globals.items.pickups.iter_mut().zip(own.iter()).enumerate().skip(first).take(n_tok) {
                if p.colour != o.colour {
                    println!("      course-load model: pickup {i} colour {} vs RAM {}", o.colour, p.colour);
                }
                p.colour = o.colour;
            }
        }
    }
    // The lifts run on between races too (and have moved the TER).
    let po = mr_sim::objects::ObjectState::read(&pre);
    s.globals.objects.lifts = po.lifts;
    s.globals.objects.lift_offsets = po.lift_offsets;
    // The captures do not hold the lifts (0x800DD708): `pre` has the menu
    // state's (ram0). The course menu keeps running FUN_800859EC, so the
    // lifts step once per menu frame (the frame counter 0x800B0D90 advances
    // by dt 0x800B0AEC) up to the race load (track S, oracle
    // analysis/oracle/stuck/DISCO_lifts: lift 0's timer runs down by 2 every
    // 2 VBlanks in the menu and it starts down before the race load).
    if std::env::var("MR_NOMENULIFTS").is_err() && !s.globals.objects.lifts.is_empty() {
        let objs = s.world.course.objects.lift_objects.clone();
        let dt0 = s.globals.dt;
        let mut last = cap.mem_at(cap.vbs[0], None).i32(0x800B_0D90);
        for &v in cap.vbs.iter().filter(|&&v| v > cap.vbs[0] && v < load) {
            let mv = cap.mem_at(v, None);
            let f = mv.i32(0x800B_0D90);
            if f != last {
                s.globals.dt = mv.i32(0x800B_0AEC);
                mr_sim::lifts::frame(&mut s.globals, &objs);
                last = f;
            }
        }
        s.globals.dt = dt0;
    }
    s.sync_ter();
    // The weather runs on between races too (drops in flight, next slot).
    if s.globals.objects.weather.is_some() {
        s.globals.objects.weather = mr_sim::objects::ObjectState::read(&pre).weather;
    }
    // The camera's position before the first race frame is the menu's (the
    // jets' smoke and the sounds test against it in the load frame).
    if std::env::var("MR_NOCAM").is_err() {
        let c = mr_sim::camera::CameraState::from_bytes(pre.slice(0x800C_4F40, mr_sim::camera::CAMERA_SIZE));
        s.camera.pos = c.pos;
        s.camera.pos_div32 = c.pos_div32;
        s.camera.roll = c.roll;
    }
    if std::env::var("MR_NOPEEK").is_ok() {
        s.camera_peek = false;
    }
    // FUN_8007A9DC's blink timers run on between races (as the water).
    for k in 0..4 {
        s.globals.items.blink[k] = pre.u8(0x800B_07B0 + k as u32);
    }
    s
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut nframes = usize::MAX;
    let mut names = Vec::new();
    let mut show_fields = false;
    let mut dir_mode: Option<String> = None;
    let mut adv_load: Option<String> = None;
    let mut course_load_mode = false;
    let mut seq_mode = false;
    let mut from_vb = 0u32;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--frames" => nframes = it.next().and_then(|x| x.parse().ok()).unwrap(),
            "--fields" => show_fields = true,
            "--dir" => dir_mode = it.next().cloned(),
            "--adv-load" => adv_load = it.next().cloned(),
            "--course-load" => course_load_mode = true,
            "--seq" => seq_mode = true,
            // --dir: start at this VBlank (e.g. the "BEAT x" race after an
            // ADVENTURE capture's tag).
            "--from" => from_vb = it.next().and_then(|x| x.parse().ok()).unwrap(),
            _ => names.push(a.to_ascii_uppercase()),
        }
    }
    let root = oracle::repo_root();
    std::env::set_current_dir(&root).unwrap();
    let dir = root.join("analysis/oracle/courses");
    if names.is_empty() {
        let mut v: Vec<String> = std::fs::read_dir(&dir).map(|r| r.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_string()).collect()).unwrap_or_default();
        v.sort();
        names = v;
    }
    let mut disc = mr_disc::Disc::open_default().expect("disc");
    let exe = mr_sim::tables::ExeImage::parse(&disc.read("SLUS_012.37").unwrap()).unwrap();
    if course_load_mode {
        course_load(&mut disc, &exe, &dir, &names);
        return;
    }
    if let Some(d) = adv_load {
        // Phase 6: an ADVENTURE capture's race load (game mode 0xB, from
        // the course menu): the session built from the VBlank before
        // (seeds, save bits, the Doozer's vehicle) against the loaded
        // records.
        let name = names.first().expect("--adv-load DIR COURSE");
        let info = CourseInfo::find(&exe, name).expect("course");
        let cap = Capture::open(root.join(&d)).expect("capture");
        let load = *cap.vbs.iter().find(|&&v| cap.read(v, 0x800B_0D46, 2).unwrap() == [11, 0] && cap.read(v, 0x800B_0D80, 1).unwrap()[0] & 1 != 0).expect("no ADVENTURE load");
        let pre = cap.mem_at(load - 1, None);
        let mut cfg = SessionConfig::course(&info);
        cfg.rand_seed = pre.u32(0x8000_9010);
        cfg.rng_start = pre.u32(0x800B_0BCC);
        cfg.faces = Some(pre.u32(0x800B_0FA4));
        cfg.round_robin_start = pre.u8(0x800C_3058 + 1);
        cfg.adventure = true;
        cfg.course_flags = pre.u16(mr_sim::globals::course_flags_addr(&pre));
        cfg.characters = pre.u32(0x800E_02BC);
        cfg.doozer_vehicle = pre.u8(0x800B_0A14 + pre.u8(0x800B_0A10) as u32);
        let s0 = Session::with_config(&mut disc, cfg).expect("session");
        let m = cap.mem_at(load, None);
        println!("## {d} ({name}): ADVENTURE load at VB {load}, {} karts (oracle {})", 1 + s0.others.len(), m.i16(0x800B_0DFE));
        println!("   random state after the load: port ({:08X}, {:08X}) oracle ({:08X}, {:08X})", s0.globals.rng, s0.rand_seed(), m.u32(0x800B_0BCC), m.u32(0x8000_9010));
        let mut bad = 0;
        for (slot, k) in std::iter::once(&s0.kart).chain(s0.others.iter().map(|o| &o.kart)).enumerate() {
            let o = KartState::from_record(cap.read(load, KART_ARRAY + (RECORD_SIZE * slot) as u32, RECORD_SIZE).unwrap());
            let dd: Vec<_> = k.diff(&o).into_iter().filter(|d| !d.starts_with("trail") && !d.starts_with("model_slot") && !d.starts_with("char_slot") && !d.starts_with("token_colour")).collect();
            if !dd.is_empty() {
                bad += 1;
                println!("   load kart {slot} ({} {}): {dd:?}", k.character, k.vehicle);
            }
        }
        println!("   ADVENTURE load: {}", if bad == 0 { "exact (every kart, every named field)".to_string() } else { format!("{bad} karts differ") });
        return;
    }
    if let Some(d) = dir_mode {
        // A capture that starts mid-race (a grid savestate, maybe poked:
        // tools/oracle/capture_poked.py): the session loads the oracle's
        // state at the first frame end (Session::load_mem) and runs from
        // there.
        let name = names.first().expect("--dir DIR COURSE");
        let info = CourseInfo::find(&exe, name).expect("course");
        let cap = Capture::open(root.join(&d)).expect("capture");
        // Frames: the (countdown, clock, frame counter 0x800B0D90) steps of
        // the race (mode 1) and of the results after the finish (mode 10:
        // the race clock stops, the frame counter runs on).
        let key = |vb: u32| (cap.i32(vb, 0x800B_0C70).unwrap(), cap.i32(vb, 0x800B_0A94).unwrap(), cap.i32(vb, 0x800B_0D90).unwrap());
        let mode = |v: u32| cap.read(v, 0x800B_0D46, 2).unwrap().to_vec();
        let vbs: Vec<u32> = cap.vbs.iter().copied().filter(|&v| v >= from_vb).filter(|&v| mode(v) == [1, 0] || mode(v) == [2, 0] || mode(v) == [3, 0] || mode(v) == [10, 0] || mode(v) == [11, 0]).collect();
        let mut ends: Vec<u32> = vbs.windows(2).filter(|w| key(w[0]) != key(w[1])).map(|w| w[0]).collect();
        // A stunt-trial capture (mode 3): its samples hold the next frame's
        // clock step (mode3_ends; the first sample taken as frame 0 with
        // its next clock step done).
        if mode(vbs[0]) == [3, 0] {
            ends = mode3_ends(&cap, &vbs, vbs[0], (0, 1));
        }
        // An ADVENTURE capture (game mode 0xB, phase 6) from the course
        // menu: from the race load on (0x800B0D80 bit 0), sampled as mode 3.
        if from_vb > 0 && std::env::var("MR_KEY_ENDS").is_err() {
            // A capture taken on from another session (the "BEAT x" race
            // after an ADVENTURE capture's tag): its VBlank samples
            // straddle the frame boundary as mode 3's; frame ends by the
            // random pointer.
            ends = mode3_ends(&cap, &vbs, vbs[0], (0, 1));
        }
        if mode(vbs[0]) == [11, 0] {
            let vbs: Vec<u32> = vbs.iter().copied().filter(|&v| cap.read(v, 0x800B_0D80, 1).unwrap()[0] & 1 != 0).collect();
            ends = mode3_ends(&cap, &vbs, vbs[0], (0, 1));
        }
        let n = ends.len().min(nframes);
        println!("## {d} ({name}): {} frames from VB {}", ends.len(), ends[0]);
        let mk = |disc: &mut mr_disc::Disc| {
            let mut cfg = SessionConfig::course(&info);
            cfg.logic_hz = 30;
            let m = cap.mem_at(ends[0], None);
            cfg.faces = Some(m.u32(0x800B_0FA4));
            // The karts' own characters / vehicles (+0x4B2 / +0x4B1): the
            // driver models' pivots feed the render pass (look points,
            // driver positions) the contacts and the battle AI read.
            let cv = |i: u32| (m.u8(KART_ARRAY + 0x504 * i + 0x4B2), m.u8(KART_ARRAY + 0x504 * i + 0x4B1));
            (cfg.character, cfg.vehicle) = cv(0);
            cfg.lineup = Some([cv(1), cv(2), cv(3), cv(4), cv(5)]);
            let mut s = Session::with_config(disc, cfg).expect("session");
            // A capture with the kart count poked down (0x800B0DFE, e.g.
            // tools/oracle/routes/tri_solo_*.json): the race frame runs
            // that many karts.
            let nk = (m.i16(0x800B_0DFE) + m.i16(0x800B_0C98)).max(1) as usize;
            s.others.truncate(nk - 1);
            s.load_mem(&m);
            s
        };
        if seq_mode {
            let mut s = mk(&mut disc);
            seq_run(&mut s, &cap);
            return;
        }
        let mut s = mk(&mut disc);
        s.leader_progress_override = stale_leader(&cap.mem_at(ends[0], None), 1 + s.others.len());
        if ends.len() > 1 && clock_plan(Some(ends[0]), ends[1]).is_some_and(|p| !p.0) {
            s.round_robin_stepped(&cap.mem_at(ends[0], None));
        }
        free_run(&mut s, &cap, &ends[1..], n - 1, show_fields);
        let mut s = mk(&mut disc);
        teacher_forced(&mut s, &cap, &ends, n, show_fields);
        return;
    }
    for name in names {
        let Some(info) = CourseInfo::find(&exe, &name) else {
            println!("## {name}: not a course");
            continue;
        };
        // DOCK: Session 3's start_probe (the same route from the course menu).
        let cdir = if name == "DOCK" && !dir.join("DOCK").exists() { root.join("analysis/oracle/start_probe") } else { dir.join(&name) };
        let Ok(cap) = Capture::open(cdir) else {
            println!("## {name}: no capture");
            continue;
        };
        let Some(f) = frames(&cap) else {
            println!("## {name}: no race load in the capture");
            continue;
        };
        println!("## {name} (course {}): race load at VB {}, {} frames", info.id, f.load, f.ends.len());
        // 1. Race load.
        let s0 = session_at_load(&mut disc, &info, &cap, f.load);
        // The load can span VBlanks (kart 5 still blank at the first one):
        // compare at the first VBlank holding the loader's final random
        // state.
        let done = (f.load..f.load + 8).find(|&v| cap.has(v) && cap.mem_at(v, None).u32(0x8000_9010) == s0.rand_seed()).unwrap_or(f.load);
        println!("   load complete at VB {done}");
        let m = cap.mem_at(done, None);
        println!("   random state after the load: port ({:08X}, {:08X}) oracle ({:08X}, {:08X})", s0.globals.rng, s0.rand_seed(), m.u32(0x800B_0BCC), m.u32(0x8000_9010));
        let mut load_bad = 0;
        for (slot, k) in std::iter::once(&s0.kart).chain(s0.others.iter().map(|o| &o.kart)).enumerate() {
            let o = KartState::from_record(cap.read(done, KART_ARRAY + (RECORD_SIZE * slot) as u32, RECORD_SIZE).unwrap());
            let d: Vec<_> = k.diff(&o).into_iter().filter(|d| !d.starts_with("trail") && !d.starts_with("model_slot") && !d.starts_with("char_slot") && !d.starts_with("token_colour")).collect();
            if !d.is_empty() {
                load_bad += 1;
                println!("   load kart {slot} ({} {}): {d:?}", k.character, k.vehicle);
            }
        }
        let og = mr_sim::objects::ObjectState::read(&m);
        if og.jets != s0.globals.objects.jets {
            println!("   load jets differ: port {:?} oracle {:?}", s0.globals.objects.jets, og.jets);
        }
        println!("   race load: {}", if load_bad == 0 { "exact (6 karts, every named field)".to_string() } else { format!("{load_bad} karts differ") });
        // 2. Free run.
        let n = f.ends.len().min(nframes);
        let mut s = session_at_load(&mut disc, &info, &cap, f.load);
        free_run(&mut s, &cap, &f.ends, n, show_fields);
        // 3. Teacher forced.
        let mut s = session_at_load(&mut disc, &info, &cap, f.load);
        teacher_forced(&mut s, &cap, &f.ends, n, show_fields);
    }
}


/// Free run from the session as given (it stands at `ends[0]`'s state or the
/// race load): per kart the first frame a motion field differs.
fn free_run(s: &mut Session, cap: &Capture, ends: &[u32], n: usize, show_fields: bool) {
    let mut first: Vec<Option<(usize, Vec<&str>)>> = vec![None; 1 + s.others.len()];
    let mut err = None;
    let mut rng_first = None;
    let mut hidden_first: Option<usize> = None;
    let mut cam_first: Option<(usize, Vec<&str>)> = None;
    for (j, &vb) in ends.iter().enumerate().take(n) {
        let m = cap.mem_at(vb, None);
        let mut dt = m.i32(0x800B_0AEC);
        if let Some(p) = clock_plan(if j == 0 { None } else { Some(ends[j - 1]) }, vb) {
            s.clock_plan = Some(p);
            s.clock_end_dt = Some(dt);
            let prev = if j == 0 { CLOCKS.with(|c| c.borrow().iter().min_by_key(|e| *e.1).map(|e| *e.0)) } else { Some(ends[j - 1]) };
            if let Some(pv) = prev {
                dt = cap.mem_at(pv, None).i32(0x800B_0AEC);
            }
        }
        if let Err(e) = s.tick_dt(m.u16(0x800B_0CE8), dt) {
            err = Some((j, e.to_string()));
            break;
        }
        if s.globals.race_clock != 0 {
            s.leader_progress_override = None;
        }
        if cam_first.is_none() && cap.read(vb, 0x800C_4F40, mr_sim::camera::CAMERA_SIZE).is_some() {
            let d = camera_diff(s, &m);
            if !d.is_empty() {
                cam_first = Some((j, d));
            }
        }
        if std::env::var("MR_GDIFF").is_ok() {
            // Analysis: the race globals the port carries vs the oracle's.
            let o = mr_sim::globals::RaceGlobals::read(&m);
            let g = &s.globals;
            let mut d = Vec::new();
            for (n, same) in [
                ("mode", g.mode == o.mode), ("race_clock", g.race_clock == o.race_clock), ("countdown", g.countdown == o.countdown),
                ("race_flags", g.race_flags == o.race_flags), ("challenge", g.challenge == o.challenge), ("race_over", g.race_over == o.race_over),
                ("frame", g.frame == o.frame), ("end_mode", g.end_mode == o.end_mode), ("results_clock", g.results_clock == o.results_clock),
                ("adv", g.adv == o.adv), ("pickups", g.items.pickups == o.items.pickups), ("special", (g.items.special, g.items.special_clock) == (o.items.special, o.items.special_clock)),
                ("blink", g.items.blink == o.items.blink), ("lap_threshold", g.lap_threshold == o.lap_threshold),
            ] {
                if !same {
                    d.push(n);
                }
            }
            if !d.is_empty() {
                println!("      gdiff {j}: {d:?} adv {:?} vs {:?}", g.adv, o.adv);
            }
            for (slot, k) in std::iter::once(&s.kart).chain(s.others.iter().map(|o| &o.kart)).enumerate() {
                let ok = KartState::from_record(m.slice(KART_ARRAY + (RECORD_SIZE * slot) as u32, RECORD_SIZE));
                let kd: Vec<_> = k.diff(&ok).into_iter().filter(|x| !x.starts_with("rumble") && !x.starts_with("trail")).collect();
                if !kd.is_empty() {
                    println!("      kdiff {j} k{slot}: {kd:?}");
                }
            }
        }
        if rng_first.is_none() && (s.globals.rng, s.rand_seed()) != (m.u32(0x800B_0BCC), m.u32(0x8000_9010)) {
            rng_first = Some(j);
            if show_fields {
                println!("      free run frame {j}: pointer {:+}, seed {}", s.globals.rng.wrapping_sub(m.u32(0x800B_0BCC)) as i32, if s.rand_seed() == m.u32(0x8000_9010) { "same".to_string() } else { format!("{:08X} vs {:08X}", s.rand_seed(), m.u32(0x8000_9010)) });
            }
        }
        if let Ok(w) = std::env::var("MR_TRACE") {
            let (a, b) = w.split_once('-').map(|(a, b)| (a.parse::<usize>().unwrap(), b.parse::<usize>().unwrap())).unwrap();
            if (a..=b).contains(&j) {
                let ob = cap.read(vb, KART_ARRAY + 0x4C, 2).map(|_| ()).is_some();
                let _ = ob;
                let kb: Vec<String> = s.kart_records().map(|(slot, _, _, r)| {
                    let o = cap.read(vb, KART_ARRAY + (RECORD_SIZE * slot) as u32, RECORD_SIZE).unwrap();
                    format!("{:04X}/{:04X} cp {}/{}", u16::from_le_bytes([r[0x44E], r[0x44F]]), u16::from_le_bytes([o[0x44E], o[0x44F]]), i16::from_le_bytes([r[0x488], r[0x489]]), i16::from_le_bytes([o[0x488], o[0x489]]))
                }).collect();
                println!("      fr {j}: ptr {:+} seed {} cd {} btn {:?}", s.globals.rng.wrapping_sub(m.u32(0x800B_0BCC)) as i32, if s.rand_seed() == m.u32(0x8000_9010) { "=" } else { "X" }, s.countdown(), kb);
            }
        }
        if std::env::var("MR_HIDDEN").is_ok() && hidden_first.is_none() {
            let oi = mr_sim::items::ItemState::read(&m);
            let pi = &s.globals.items;
            let mut what = Vec::new();
            if oi.pickups != pi.pickups { what.push("pickups"); }
            if oi.projectiles != pi.projectiles { what.push("projectiles"); }
            if oi.blink != pi.blink { what.push("blink"); }
            let oo = mr_sim::objects::ObjectState::read(&m);
            if oo.weather != s.globals.objects.weather { what.push("weather"); }
            // (TER group states are not comparable: the captures do not
            // hold the TER, ObjectState::read sees the menu's copy.)
            if oo.jets != s.globals.objects.jets {
                what.push("jets");
                println!("        jets: port {:?}
              oracle {:?}", s.globals.objects.jets, oo.jets);
            }
            if let Some(fx) = s.globals.fx.as_ref() {
                let op = mr_sim::particles::Particles::read(&m, fx.particles.types.clone());
                if !same_particles(&op, &fx.particles) { what.push("particles"); }
                if mr_sim::particles::Sparks::read(&m) != fx.sparks { what.push("sparks"); }
            }
            if !what.is_empty() {
                println!("      hidden state first differs at frame {j}: {what:?}");
                for (i, (a, b)) in s.globals.items.projectiles.iter().zip(oi.projectiles.iter()).enumerate() {
                    if a != b {
                        println!("        projectile {i}: port   {a:?}
                       oracle {b:?}");
                    }
                }
                if let Some(fx) = s.globals.fx.as_ref() {
                    let mut op = mr_sim::particles::Particles::read(&m, fx.particles.types.clone());
                    op.phase = fx.particles.phase;
                    if op.next != fx.particles.next {
                        println!("        particle ring next: port {} oracle {}", fx.particles.next, op.next);
                    }
                    for i in 0..mr_sim::particles::N {
                        if !same_particle(&op.list[i], &fx.particles.list[i]) {
                            println!("        particle {i}: port   {:?}
                     oracle {:?}", fx.particles.list[i], op.list[i]);
                        }
                    }
                    let os = mr_sim::particles::Sparks::read(&m);
                    if os.next != fx.sparks.next {
                        println!("        spark ring next: port {} oracle {}", fx.sparks.next, os.next);
                    }
                    for i in 0..mr_sim::particles::N_SPARKS {
                        if os.list[i] != fx.sparks.list[i] {
                            println!("        spark {i}: port   {:?}
                  oracle {:?}", fx.sparks.list[i], os.list[i]);
                        }
                    }
                }
                hidden_first = Some(j);
            }
        }
        if let Ok(lim) = std::env::var("MR_CAM") {
            if j < lim.parse::<usize>().unwrap() {
                let oc = mr_sim::camera::CameraState::from_bytes(m.slice(0x800C_4F40, mr_sim::camera::CAMERA_SIZE));
                println!("      cam fr {j}: port mode {} pos {:?} pyr ({},{},{}) w {:?} | oracle mode {} pos {:?} pyr ({},{},{}) w {:?}", s.camera.mode, s.camera.pos, s.camera.pitch, s.camera.yaw, s.camera.roll, s.camera.weight, oc.mode, oc.pos, oc.pitch, oc.yaw, oc.roll, oc.weight);
            }
        }
        if std::env::var("MR_WEATHER").is_ok() {
            let ow = mr_sim::objects::ObjectState::read(&m).weather;
            if ow != s.globals.objects.weather {
                let (a, b) = (ow.unwrap(), s.globals.objects.weather.clone().unwrap());
                println!("      weather fr {j}: next {} vs {}", b.next, a.next);
                for i in 0..16 {
                    if a.drops[i] != b.drops[i] {
                        println!("        drop {i}: port {:?}
            oracle {:?}", b.drops[i], a.drops[i]);
                    }
                }
                std::process::exit(0);
            }
        }
        if let Some(lim) = std::env::var("MR_FREEFIELDS").ok().and_then(|x| x.parse::<usize>().ok()) {
            if j < lim {
                let mut all = Vec::new();
                for (slot, _, _, r) in s.kart_records() {
                    let o = cap.read(vb, KART_ARRAY + (RECORD_SIZE * slot) as u32, RECORD_SIZE).unwrap();
                    for &(nm, off, sz) in FIELD_OFFSETS {
                        if r[off..off + sz] != o[off..off + sz] {
                            let v = |b: &[u8]| match sz { 1 => b[0] as i8 as i64, 2 => i16::from_le_bytes([b[0], b[1]]) as i64, _ => i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as i64 };
                            all.push(format!("k{slot}.{nm}={}/{}", v(&r[off..off + sz]), v(&o[off..off + sz])));
                        }
                    }
                }
                let os = m.u32(0x8000_9010);
                let steps = (0..200).find(|&k| mr_sim::session::bios_rand_rewind(os, k) == s.rand_seed()).map(|k| -(k as i32)).or_else(|| (0..200).find(|&k| mr_sim::session::bios_rand_rewind(s.rand_seed(), k) == os).map(|k| k as i32));
                println!("      fr {j}: ptr {:+} seed {:?} {:?}", s.globals.rng.wrapping_sub(m.u32(0x800B_0BCC)) as i32, steps, all);
            }
        }
        for (slot, _, _, r) in s.kart_records() {
            if first[slot].is_some() {
                continue;
            }
            let o = cap.read(vb, KART_ARRAY + (RECORD_SIZE * slot) as u32, RECORD_SIZE).unwrap();
            let bad: Vec<&str> = FIELD_OFFSETS.iter().filter(|&&(nm, off, sz)| MOTION.contains(&nm) && r[off..off + sz] != o[off..off + sz]).map(|e| e.0).collect();
            if !bad.is_empty() {
                first[slot] = Some((j, bad));
            }
        }
        if first.iter().all(|x| x.is_some()) && cam_first.is_some() {
            break;
        }
    }
    let fr = first.iter().enumerate().map(|(i, x)| match x {
        None => format!("k{i} exact"),
        Some((j, b)) => format!("k{i} @{j} {b:?}"),
    });
    let cam = match cam_first {
        None => "camera exact".to_string(),
        Some((j, d)) => format!("camera @{j} {d:?}"),
    };
    println!("   free run over {n} frames: {}; {cam}; random state first differs at {:?}{}", fr.collect::<Vec<_>>().join(", "), rng_first, err.map(|e| format!("; ERROR at {}: {}", e.0, e.1)).unwrap_or_default());
}

/// `--seq`: a free run of kart 0 and the camera against the sequence of the
/// game's kart updates (the VBlanks where kart 0's race time +0x358 changes),
/// fed each update's own buttons (+0x44E) and dt. For captures whose race
/// frame is too light to keep the clock step and the kart update in
/// separate VBlank snapshots (a race frame with one kart: the snapshots land
/// mid-frame, so the (countdown, clock) frame ends of the other modes are
/// not frame ends). Starts from the state at the first update (loaded by
/// the caller from the first (countdown, clock) end).
fn seq_run(s: &mut Session, cap: &Capture) {
    let rt = |vb: u32| cap.i32(vb, KART_ARRAY + 0x358).unwrap();
    let mut start = s.kart().race_time;
    let mut ups: Vec<u32> = cap.vbs.windows(2).filter(|w| rt(w[0]) != rt(w[1]) && rt(w[1]) > start).map(|w| w[1]).collect();
    if start == 0 {
        // From a countdown state: the race time first moves at GO; start
        // from the game's state at its first update.
        let m = cap.mem_at(ups[0], None);
        s.load_mem(&m);
        start = rt(ups[0]);
        ups.remove(0);
        println!("   seq: from the first kart update after GO (VB {}, race time {start})", ups[0]);
    }
    let mut kfirst: Option<(usize, Vec<&str>)> = None;
    let mut cfirst: Option<(usize, Vec<&str>)> = None;
    let mut watched = std::collections::BTreeMap::<i8, usize>::new();
    let mut n = 0;
    for (j, &vb) in ups.iter().enumerate() {
        let m = cap.mem_at(vb, None);
        let buttons = m.u16(KART_ARRAY + 0x44E);
        if let Err(e) = s.tick_dt(buttons, m.i32(0x800B_0AEC)) {
            println!("   seq: ERROR at update {j}: {e}");
            break;
        }
        n += 1;
        let r = s.record().to_vec();
        let o = m.slice(KART_ARRAY, RECORD_SIZE);
        *watched.entry(o[0x4FE] as i8).or_default() += 1;
        if kfirst.is_none() {
            let bad: Vec<&str> = FIELD_OFFSETS.iter().filter(|&&(nm, off, sz)| MOTION.contains(&nm) && nm != "race_pos" && r[off..off + sz] != o[off..off + sz]).map(|e| e.0).collect();
            if !bad.is_empty() {
                kfirst = Some((j, bad));
            }
        }
        if cfirst.is_none() {
            // The camera runs after the kart loop: this VBlank or the next.
            let d0 = camera_diff(s, &m);
            let d = if d0.is_empty() { d0 } else { camera_diff(s, &cap.mem_at(vb + 1, None)) };
            if !d.is_empty() {
                cfirst = Some((j, d));
            }
        }
    }
    println!("   seq free run over {n} kart updates: kart 0 {}; camera {}; game's watch targets (+0x4FE: updates) {watched:?}",
        kfirst.map(|(j, b)| format!("@{j} {b:?}")).unwrap_or_else(|| "exact".into()),
        cfirst.map(|(j, b)| format!("@{j} {b:?}")).unwrap_or_else(|| "exact".into()));
}

/// Teacher forced: every frame from the oracle's previous frame.
fn teacher_forced(s: &mut Session, cap: &Capture, ends: &[u32], n: usize, show_fields: bool) {
    let mut counts: std::collections::BTreeMap<(usize, &str), (usize, usize)> = Default::default();
    let mut rng_bad = 0;
    let mut first_rng = None;
    let mut tf_err = None;
    let mut cam_bad: std::collections::BTreeMap<&str, (usize, usize)> = Default::default();
    for j in 1..n {
        let mprev = cap.mem_at(ends[j - 1], None);
        // The lifts are not in the course captures: keep the session's own
        // (stepped from the menu state, session_at_load) over the reload.
        // Nor is the TER: the breakable groups' states read from it are the
        // menu's; keep the session's own too.
        let keep_lifts = cap.read(ends[j - 1], 0x800D_D708, 4).is_none().then(|| (s.globals.objects.lifts.clone(), s.globals.objects.lift_offsets.clone()));
        let keep_groups = cap.read(ends[j - 1], mprev.u32(0x800B_0FA4), 4).is_none().then(|| (s.globals.objects.group_enabled.clone(), s.globals.objects.hidden_slots.clone()));
        // Nor are the history rings (0x800D4098, the camera's trail): keep
        // the session's own.
        let keep_trails = cap.read(ends[j - 1], 0x800D_4098, 16).is_none().then(|| (s.kart.trail, s.others.iter().map(|o| o.kart.trail).collect::<Vec<_>>()));
        s.load_mem(&mprev);
        s.leader_progress_override = stale_leader(&mprev, 1 + s.others.len());
        // (a sample past the next frame's clock step: its round robin too)
        if clock_plan(Some(ends[j - 1]), ends[j]).is_some_and(|p| !p.0) {
            s.round_robin_stepped(&mprev);
        }
        if let Some((t0, ts)) = keep_trails {
            s.kart.trail = t0;
            for (o, t) in s.others.iter_mut().zip(ts) {
                o.kart.trail = t;
            }
        }
        if let Some((l, o)) = keep_lifts {
            s.globals.objects.lifts = l;
            s.globals.objects.lift_offsets = o;
        }
        if let Some((g, h)) = keep_groups {
            s.globals.objects.group_enabled = g;
            s.globals.objects.hidden_slots = h;
        }
        s.sync_ter();
        if let Some((fj, slot)) = std::env::var("MR_WALLDBG").ok().and_then(|v| { let (a, b) = v.split_once(':')?; Some((a.parse::<usize>().ok()?, b.parse::<usize>().ok()?)) }) {
            if fj == j {
                // Analysis: the walls near kart `slot` before frame j.
                let k = s.kart_records().find(|r| r.0 == slot).map(|r| KartState::from_record(r.3)).unwrap();
                let (kx, ky, kz) = (k.x >> 5, k.y >> 5, k.z >> 5);
                println!("      frame {j} k{slot}: TER ({kx}, {ky}, {kz}) v ({}, {}, {}) hdg {} probe {}..{}", k.vx, k.vy, k.vz, k.heading, k.probe_top, k.probe_bottom);
                let t = &s.world.ter;
                for (wi, wl) in t.walls.iter().enumerate() {
                    let (a, b) = (t.wall_points[wl.p0 as usize], t.wall_points[wl.p1 as usize]);
                    let near = |p: &mr_assets::ter::WallPoint| (p.x - kx).abs() < 1200 && (p.z - kz).abs() < 1200;
                    if near(&a) || near(&b) {
                        println!("        wall {wi} grp {} dis {} p0 {:?} p1 {:?}", mr_sim::breakables::wall_group(t, wi), wl.disabled, a, b);
                    }
                }
                println!("        lifts {:?} offsets {:?} applied {:?}", s.globals.objects.lifts, s.globals.objects.lift_offsets, s.world.ter_applied);
            }
        }
        let m = cap.mem_at(ends[j], None);
        let mut dt = m.i32(0x800B_0AEC);
        if let Some(p) = clock_plan(Some(ends[j - 1]), ends[j]) {
            // These samples hold the next frame's dt (measured before its
            // clock step): the frame's own is the previous sample's.
            s.clock_plan = Some(p);
            s.clock_end_dt = Some(dt);
            dt = mprev.i32(0x800B_0AEC);
        }
        if let Err(e) = s.tick_dt(m.u16(0x800B_0CE8), dt) {
            tf_err = Some((j, e.to_string()));
            break;
        }
        if std::env::var("MR_TFCLOCK").is_ok() {
            println!("      tf {j} vb {}: countdown {} vs {}, clock {} vs {}", ends[j], s.countdown(), m.i32(0x800B_0C70), s.globals.race_clock, m.i32(0x800B_0A94));
        }
        if cap.read(ends[j], 0x800C_4F40, mr_sim::camera::CAMERA_SIZE).is_some() {
            let cd = camera_diff(s, &m);
            if !cd.is_empty() && std::env::var("MR_TFCAM").is_ok() {
                let oc = mr_sim::camera::CameraState::from_bytes(m.slice(0x800C_4F40, mr_sim::camera::CAMERA_SIZE));
                println!("      tf cam {j}: {cd:?} port mode {} pos {:?} | oracle mode {} pos {:?}", s.camera.mode, s.camera.pos, oc.mode, oc.pos);
            }
            for d in cd {
                cam_bad.entry(d).or_insert((0, j)).0 += 1;
            }
        }
        if (s.globals.rng, s.rand_seed()) != (m.u32(0x800B_0BCC), m.u32(0x8000_9010)) {
            rng_bad += 1;
            first_rng.get_or_insert(j);
            if show_fields && rng_bad <= 12 {
                let mut back = m.u32(0x8000_9010);
                let seed_steps = (0..64).find(|&k| { let r = mr_sim::session::bios_rand_rewind(back, k); r == s.rand_seed() }).map(|k| k as i32);
                back = s.rand_seed();
                let seed_ahead = (0..64).find(|&k| mr_sim::session::bios_rand_rewind(back, k) == m.u32(0x8000_9010)).map(|k| -(k as i32));
                println!("      tf frame {j}: pointer {:+}, seed steps port-oracle {:?}", s.globals.rng.wrapping_sub(m.u32(0x800B_0BCC)) as i32, seed_steps.or(seed_ahead));
            }
        }
        for (slot, _, _, r) in s.kart_records() {
            let o = cap.read(ends[j], KART_ARRAY + (RECORD_SIZE * slot) as u32, RECORD_SIZE).unwrap();
            for &(nm, off, sz) in FIELD_OFFSETS {
                if r[off..off + sz] != o[off..off + sz] {
                    if std::env::var("MR_TFFIELDS").ok().map(|f| f.split(',').any(|x| x == nm)).unwrap_or(false) {
                        let v = |b: &[u8]| match sz { 1 => b[0] as i8 as i64, 2 => i16::from_le_bytes([b[0], b[1]]) as i64, _ => i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as i64 };
                        println!("      tf {j} k{slot}.{nm} = {} vs {} (x {} z {})", v(&r[off..off + sz]), v(&o[off..off + sz]), i32::from_le_bytes(o[0..4].try_into().unwrap()) >> 5, i32::from_le_bytes(o[8..12].try_into().unwrap()) >> 5);
                    }
                    let e = counts.entry((slot, nm)).or_insert((0, j));
                    e.0 += 1;
                }
            }
        }
    }
    let motion_bad: Vec<String> = counts.iter().filter(|(k, _)| MOTION.contains(&k.1)).map(|(k, v)| format!("k{} {} x{} (first {})", k.0, k.1, v.0, v.1)).collect();
    let cam: Vec<String> = cam_bad.iter().map(|(k, v)| format!("{k} x{} (first {})", v.0, v.1)).collect();
    println!(
        "   teacher forced over {} frames: motion fields {}; camera {}; random state differs on {rng_bad} frames (first {first_rng:?}){}",
        n - 1,
        if motion_bad.is_empty() { "exact".to_string() } else { motion_bad.join(", ") },
        if cam.is_empty() { "exact".to_string() } else { cam.join(", ") },
        tf_err.map(|e| format!("; ERROR at {}: {}", e.0, e.1)).unwrap_or_default()
    );
    if show_fields {
        for (k, v) in &counts {
            println!("      k{} {:<14} {:>5} frames, first {}", k.0, k.1, v.0, v.1);
        }
    }
}
/// Particle equality without the drawer's on-screen bit (flags 0x200: set at
/// spawn, cleared by the render pass FUN_80010E90 when off screen; nothing
/// in the simulation reads it).
fn same_particle(a: &mr_sim::particles::Particle, b: &mr_sim::particles::Particle) -> bool {
    let mut a = *a;
    let mut b = *b;
    a.flags &= !0x200;
    b.flags &= !0x200;
    a == b
}

fn same_particles(a: &mr_sim::particles::Particles, b: &mr_sim::particles::Particles) -> bool {
    a.next == b.next && a.list.iter().zip(b.list.iter()).all(|(x, y)| same_particle(x, y))
}

/// `--course-load`: where the course load's rand() draws sit in the BIOS
/// stream before the capture's first state (the course menu, ram0). The
/// course load (before the capture) draws, in order: FUN_8007B448 one per
/// turbo token (colour = rand() >> 13), FUN_80084490 two per turntable
/// (phase rand() / 8, spin rand() < 0x4000 ? 5 : -5, x3 on 0x1A / 0x1B) and
/// its per-course switch. Search the seeds before the menu's for the run
/// that gives the token colours and turntable spins the menu state holds.
fn course_load(disc: &mut mr_disc::Disc, exe: &mr_sim::tables::ExeImage, dir: &std::path::Path, names: &[String]) {
    let mut list: Vec<String> = names.to_vec();
    if list.is_empty() {
        let mut v: Vec<String> = std::fs::read_dir(dir).map(|r| r.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_string()).collect()).unwrap_or_default();
        v.sort();
        list = v;
    }
    for name in list {
        let Some(info) = CourseInfo::find(exe, &name) else { continue };
        let Ok(cap) = Capture::open(dir.join(&name)) else { continue };
        let vb0 = cap.vbs[0];
        let m = cap.mem_at(vb0, None);
        let s = Session::with_config(disc, SessionConfig::course(&info)).expect("session");
        let w = &s.world;
        let n_tok = w.pickups.tokens.as_ref().map(|l| l.points.len()).filter(|&n| n != 0 && n <= 0x80).unwrap_or(0);
        let n_turn = w.course.objects.turntables.len();
        if n_tok + n_turn == 0 {
            println!("## {name}: no course-load draws (no tokens, no turntables)");
            continue;
        }
        let hits = course_load_search(w, &info, &m, 4);
        println!("## {name} (course {}): {n_tok} tokens, {n_turn} turntables; course-load start {:?} rand() calls before the menu state (VB {vb0}){}", info.id, hits, if hits.len() == 1 { "" } else { "  <- not unique" });
    }
}

/// The course load's start as rand() calls before the state `m` (a menu or
/// race-load state): the runs of the course-load draws (token colours, then
/// turntable phase / spin) that give `m`'s colours and spins; at most
/// `max` of them, nearest first.
fn course_load_search(w: &mr_sim::world::World, info: &CourseInfo, m: &mr_sim::mem::Mem, max: usize) -> Vec<usize> {
    const BACK: usize = 4_000_000;
    let n_tok = w.pickups.tokens.as_ref().map(|l| l.points.len()).filter(|&n| n != 0 && n <= 0x80).unwrap_or(0);
    let n_turn = w.course.objects.turntables.len();
    if n_tok + n_turn == 0 {
        return Vec::new();
    }
    let ram = mr_sim::items::ItemState::read(m);
    let colours: Vec<u8> = (0..n_tok).map(|i| ram.pickups[mr_sim::items::FIRST_TOKEN + i].colour).collect();
    let spins = mr_sim::objects::ObjectState::read(m).turntable_spin;
    let mut hits = Vec::new();
    let mut back = m.u32(0x8000_9010);
    for j in 0..BACK {
        if j >= n_tok + 2 * n_turn {
            let mut seed = back;
            let mut rand = || mr_sim::session::bios_rand(&mut seed);
            if (0..n_tok).all(|i| (rand() >> 13) as u8 == colours[i]) {
                let sp: Vec<i16> = (0..n_turn)
                    .map(|_| {
                        rand();
                        let v: i16 = if rand() < 0x4000 { 5 } else { -5 };
                        if (0x1A..=0x1B).contains(&info.id) { v * 3 } else { v }
                    })
                    .collect();
                if sp == spins {
                    hits.push(j);
                    if hits.len() >= max {
                        break;
                    }
                }
            }
        }
        back = mr_sim::session::bios_rand_rewind(back, 1);
    }
    hits
}

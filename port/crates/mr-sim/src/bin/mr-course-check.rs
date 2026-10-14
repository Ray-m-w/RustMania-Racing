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

const MOTION: [&str; 14] = ["x", "y", "z", "pitch", "heading", "roll", "vx", "vy", "vz", "fwd_speed", "side_speed", "lap", "buttons", "race_pos"];

struct Ends {
    /// The first VBlank of the race load (countdown 480, mode 1).
    load: u32,
    /// Frame ends (the last VBlank of each (countdown, clock) step).
    ends: Vec<u32>,
}

fn frames(cap: &Capture) -> Option<Ends> {
    let key = |vb: u32| (cap.i32(vb, 0x800B_0C70).unwrap(), cap.i32(vb, 0x800B_0A94).unwrap());
    let load = *cap.vbs.iter().find(|&&v| key(v).0 == 480 && cap.read(v, 0x800B_0D46, 2).unwrap() == [1, 0])?;
    let vbs: Vec<u32> = cap.vbs.iter().copied().filter(|&v| v >= load).collect();
    let ends: Vec<u32> = vbs.windows(2).filter(|w| key(w[0]) != key(w[1])).map(|w| w[0]).collect();
    Some(Ends { load, ends })
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
    let mut s = Session::with_config(disc, cfg).expect("session");
    let m = cap.mem_at(load, None);
    s.water = mr_sim::water::WaterAnim::from_bytes(cap.read(load, 0x800A_FEF0, 0x20).unwrap());
    // Course-load state the session derives from seeds before the race
    // load: the game drew it at its own course load, before the capture.
    let o = mr_sim::objects::ObjectState::read(&m);
    s.globals.objects.turntable_phase = o.turntable_phase;
    s.globals.objects.turntable_spin = o.turntable_spin;
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
    if std::env::var("MR_NOPICK").is_err() {
        s.globals.items.pickups = mr_sim::items::ItemState::read(&m).pickups;
    }
    // The lifts run on between races too (and have moved the TER).
    let po = mr_sim::objects::ObjectState::read(&pre);
    s.globals.objects.lifts = po.lifts;
    s.globals.objects.lift_offsets = po.lift_offsets;
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
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--frames" => nframes = it.next().and_then(|x| x.parse().ok()).unwrap(),
            "--fields" => show_fields = true,
            "--dir" => dir_mode = it.next().cloned(),
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
    if let Some(d) = dir_mode {
        // A capture that starts mid-race (a grid savestate, maybe poked:
        // tools/oracle/capture_poked.py): the session loads the oracle's
        // state at the first frame end (Session::load_mem) and runs from
        // there.
        let name = names.first().expect("--dir DIR COURSE");
        let info = CourseInfo::find(&exe, name).expect("course");
        let cap = Capture::open(root.join(&d)).expect("capture");
        let key = |vb: u32| (cap.i32(vb, 0x800B_0C70).unwrap(), cap.i32(vb, 0x800B_0A94).unwrap());
        let vbs: Vec<u32> = cap.vbs.iter().copied().filter(|&v| cap.read(v, 0x800B_0D46, 2).unwrap() == [1, 0]).collect();
        let ends: Vec<u32> = vbs.windows(2).filter(|w| key(w[0]) != key(w[1])).map(|w| w[0]).collect();
        let n = ends.len().min(nframes);
        println!("## {d} ({name}): {} frames from VB {}", ends.len(), ends[0]);
        let mk = |disc: &mut mr_disc::Disc| {
            let mut cfg = SessionConfig::course(&info);
            cfg.logic_hz = 30;
            let m = cap.mem_at(ends[0], None);
            cfg.faces = Some(m.u32(0x800B_0FA4));
            let mut s = Session::with_config(disc, cfg).expect("session");
            s.load_mem(&m);
            s
        };
        let mut s = mk(&mut disc);
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
    for (j, &vb) in ends.iter().enumerate().take(n) {
        let m = cap.mem_at(vb, None);
        if let Err(e) = s.tick_dt(m.u16(0x800B_0CE8), m.i32(0x800B_0AEC)) {
            err = Some((j, e.to_string()));
            break;
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
        if first.iter().all(|x| x.is_some()) {
            break;
        }
    }
    let fr = first.iter().enumerate().map(|(i, x)| match x {
        None => format!("k{i} exact"),
        Some((j, b)) => format!("k{i} @{j} {b:?}"),
    });
    println!("   free run over {n} frames: {}; random state first differs at {:?}{}", fr.collect::<Vec<_>>().join(", "), rng_first, err.map(|e| format!("; ERROR at {}: {}", e.0, e.1)).unwrap_or_default());
}

/// Teacher forced: every frame from the oracle's previous frame.
fn teacher_forced(s: &mut Session, cap: &Capture, ends: &[u32], n: usize, show_fields: bool) {
    let mut counts: std::collections::BTreeMap<(usize, &str), (usize, usize)> = Default::default();
    let mut rng_bad = 0;
    let mut first_rng = None;
    let mut tf_err = None;
    for j in 1..n {
        let mprev = cap.mem_at(ends[j - 1], None);
        s.load_mem(&mprev);
        let m = cap.mem_at(ends[j], None);
        if let Err(e) = s.tick_dt(m.u16(0x800B_0CE8), m.i32(0x800B_0AEC)) {
            tf_err = Some((j, e.to_string()));
            break;
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
    println!(
        "   teacher forced over {} frames: motion fields {}; random state differs on {rng_bad} frames (first {first_rng:?}){}",
        n - 1,
        if motion_bad.is_empty() { "exact".to_string() } else { motion_bad.join(", ") },
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

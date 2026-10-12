//! Teacher-forcing verification of the chase camera (src/camera.rs) against
//! oracle captures (tools/oracle/capture.py format). Tick k = VBlank 2k: feed
//! the oracle kart + globals of tick k and the oracle camera of tick k-1,
//! compare our camera with the oracle camera at tick k.
//!
//! Needs the local, gitignored inputs: analysis/oracle/{lap1P,c_steer} and
//! disc/SLUS_012.37 (tables). Missing inputs skip the test with a message.
//!
//!     cargo test -p mr-sim --test camera -- --nocapture

#[path = "../src/camera.rs"]
mod camera;

use camera::*;
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
    for (i, e) in t.iter_mut().enumerate() {
        let o = (p as usize & 0x1F_FFFF) + 16 * i;
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
}

impl Report {
    fn new() -> Self {
        Report { fields: Vec::new(), ticks: 0 }
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
        eprintln!("== {} ({} ticks)", title, self.ticks);
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

/// Teacher-force one capture. `push_delay` picks which oracle kart position
/// the history ring entry pushed at tick j holds (0: end of tick j, 1: end of
/// tick j-1); `full` = capture has the globals (ground results) region.
fn run(cap: &Capture, t: &CameraTables, push_delay: u32, full: bool) -> Report {
    let mut rep = Report::new();
    let first_vb = *cap.vb.keys().next().unwrap();
    // Ring at VBlank 0: from ram0 if present, else 12 copies of the grid position.
    let ring0 = match &cap.ram0 {
        Some(r) => ram_trail(r),
        None => {
            let rec = cap.rd(first_vb, KART0, 12).unwrap();
            let p = [0, 1, 2].map(|j| i32::from_le_bytes(rec[4 * j..4 * j + 4].try_into().unwrap()));
            [TrailEntry { pos: p, count: 2 }; TRAIL_LEN]
        }
    };
    // Tick boundaries: the VBlanks where the kart or camera record changed.
    // A tick is normally 2 VBlanks, but the oracle drops frames (dt = 3 at
    // lap1P VBlanks 1373 and 1393), so "tick k = VBlank 2k" is not exact.
    let mut ticks: Vec<u32> = vec![first_vb];
    let mut prev: Option<Vec<u8>> = None;
    for &vb in cap.vb.keys() {
        let mut cur = cap.rd(vb, KART0, KART_SIZE).unwrap().to_vec();
        cur.extend_from_slice(cap.rd(vb, ADDR_CAMERA0, CAMERA_SIZE).unwrap());
        if let Some(p) = &prev {
            if *p != cur {
                ticks.push(vb);
            }
        }
        prev = Some(cur);
    }
    let kart_pos = |vb: u32| -> [i32; 3] {
        let rec = cap.rd(vb, KART0, 12).unwrap();
        [0, 1, 2].map(|i| i32::from_le_bytes(rec[4 * i..4 * i + 4].try_into().unwrap()))
    };
    for n in 1..ticks.len() {
        let (vb, prev_vb) = (ticks[n], ticks[n - 1]);
        let k = n as u32;
        // History ring at tick n: entry i was pushed by tick n-i with the
        // kart position at the start (push_delay 1) or end (0) of that tick
        // and that tick's dt.
        let mut trail = [TrailEntry::default(); TRAIL_LEN];
        for i in 0..TRAIL_LEN {
            let j = n as i64 - i as i64; // tick that pushed entry i
            if j >= 1 {
                let j = j as usize;
                let src = if push_delay == 1 { ticks[j - 1] } else { ticks[j] };
                trail[i] = TrailEntry { pos: kart_pos(src), count: cap.i32(ticks[j], 0x800B_0AEC).unwrap() };
            } else {
                trail[i] = ring0[(i as i64 - n as i64) as usize];
            }
        }
        let rec = cap.rd(vb, KART0, KART_SIZE).unwrap();
        let kart = CameraKart::from_record(rec, Some(trail));
        let globals = if full {
            CameraGlobals::from_reader(&|a, n| {
                cap.rd(vb, a, n).map(|s| s.to_vec()).unwrap_or_else(|| {
                    // Pointer targets outside the capture (only the cut-scene
                    // check uses them; both pointers are null in a race).
                    vec![0; n]
                })
            })
        } else {
            let mut g = CameraGlobals::dock_race();
            g.dt = cap.i32(vb, 0x800B_0AEC).unwrap();
            g.game_state = i16::from_le_bytes(cap.rd(vb, 0x800B_0D46, 2).unwrap().try_into().unwrap());
            g
        };
        let ground = if full {
            CameraGround {
                found: cap.i32(vb, 0x800B_0B0C).unwrap() != 0,
                nearest_y: cap.i32(vb, 0x800B_0B24).unwrap(),
                below_y: cap.i32(vb, 0x800B_0B04).unwrap(),
                alt_found: cap.i32(vb, 0x800B_0C84).unwrap() != 0,
                alt_y: cap.i32(vb, 0x800B_0C9C).unwrap(),
                type3_min_y: cap.i32(vb, 0x800B_0A54).unwrap(),
            }
        } else {
            CameraGround::default()
        };
        let mut cam = CameraState::from_bytes(cap.rd(prev_vb, ADDR_CAMERA0, CAMERA_SIZE).unwrap());
        let want = CameraState::from_bytes(cap.rd(vb, ADDR_CAMERA0, CAMERA_SIZE).unwrap());
        let inp = CameraInput { kart, globals };
        tick(&mut cam, &inp, t, &mut |_, _, _| ground, &mut || panic!("rand on a teacher-forced tick"));

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
        if full {
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
            // Whole record byte-exact?
            rep.add("record_bytes", k, (cam.to_bytes() != want.raw) as i64);
        }
    }
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

#[test]
fn teacher_forced_lap1p() {
    let (Some(t), Some(cap)) = (tables(), Capture::load("lap1P")) else {
        eprintln!("skip: analysis/oracle/lap1P or disc/SLUS_012.37 missing");
        return;
    };
    let r0 = run(&cap, &t, 0, true);
    r0.print("lap1P, history push = kart position at end of the pushing tick");
    let r1 = run(&cap, &t, 1, true);
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
    let r = run(&cap, &t, 1, false);
    r.print("c_steer (no ground globals captured: x/z/yaw/roll/weights only)");
    for f in ["pos.x", "pos.z", "weight.x", "weight.z", "yaw", "roll", "mode"] {
        assert_eq!(r.max(f), 0, "c_steer field {f}");
    }
}

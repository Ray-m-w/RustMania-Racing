//! mr-title: the title screen's karts (course 0x1E, game mode 0x10) against
//! an oracle capture (Session 6, T; docs/spec/frontend.md).
//!
//!   mr-title [CAPTURE_DIR] [--ticks N]
//!
//! CAPTURE_DIR (default analysis/s6/title/karts): a capture from the title
//! savestate (tools/oracle/title_check.py karts) with regions kart records
//! 0..2, globals 0x800B0700.., the camera, the rand() seed. The session is
//! built from the RAM at the first captured VBlank (Session::from_mem) and
//! run free with the oracle's dt; per tick: each kart's position, heading
//! and speed, and the camera, against the capture.

use mr_sim::oracle::{self, Capture};
use mr_sim::session::Session;
use mr_sim::world::{HeapLayout, World};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let opt = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    std::env::set_current_dir(oracle::repo_root()).unwrap();
    let dir = args.get(1).filter(|a| !a.starts_with("--")).cloned().unwrap_or_else(|| "analysis/s6/title/karts".into());
    let cap = Capture::open(&dir).expect("open capture");
    let max_ticks: usize = opt("--ticks").and_then(|v| v.parse().ok()).unwrap_or(usize::MAX);
    let mut disc = mr_disc::Disc::open_default().expect("open the disc");
    let vb0 = cap.vbs[0];
    let m = cap.mem_at(vb0, None);
    let mut w = World::course(&mut disc, 0x1E).expect("frontend world");
    w.heap = HeapLayout { faces: m.u32(mr_sim::oracle::course_addr::FACES_PTR) };
    // kart 0 is an AI kart with a random character on the title: its driver
    // model (the look point uses part 0's pivot), not Kermit's
    let c0 = m.u8(mr_sim::kart::KART_ARRAY + 0x4B2);
    let cpath = mr_sim::ai::model_path(&w.exe, mr_sim::ai::addr::CHARACTER_PATHS, c0, "MDL").expect("character model");
    let cm = mr_assets::mdl::Mdl::parse(&disc.read(&cpath).expect("read character")).expect("parse character");
    w.driver = mr_sim::render::DriverModel { part0_pivot: cm.parts[0].record.pivot };
    let mut s = Session::from_mem(&mut disc, w, &m).expect("session from the title RAM");
    let rd = |vb: u32, a: u32| cap.i32(vb, a).unwrap();
    let mut prev = rd(vb0, 0x800B_0D90);
    let (mut n, mut exact, mut first_bad) = (0usize, 0usize, None);
    let mut max_err = 0i64;
    for &vb in cap.vbs.iter().skip(1) {
        let f = rd(vb, 0x800B_0D90);
        if f == prev {
            continue;
        }
        prev = f;
        let dt = rd(vb, 0x800B_0AEC);
        if let Err(e) = s.tick_dt(0, dt) {
            println!("tick {n} (VB {vb}): out of scope: {e:?}");
            break;
        }
        n += 1;
        // the counter steps at the frame's start, the karts by its end: the
        // tick's result is in the next VBlank's sample
        let vb = vb + 1;
        if !cap.has(vb) {
            break;
        }
        if args.iter().any(|a| a == "--diff") && first_bad.is_none() {
            // every byte of the three records, the random pointer and seed
            for (slot, rec) in std::iter::once(&s.record).chain(s.others.iter().map(|o| &o.record)).enumerate() {
                let a = mr_sim::kart::KART_ARRAY + 0x504 * slot as u32;
                let want = cap.read(vb, a, 0x504).unwrap();
                let offs: Vec<String> = (0..0x504).filter(|&o| want[o] != rec[o]).map(|o| format!("{o:#x}:{:02x}/{:02x}", want[o], rec[o])).collect();
                if !offs.is_empty() {
                    println!("tick {n} VB {vb} kart {slot}: {} bytes differ: {}", offs.len(), offs.iter().take(400).cloned().collect::<Vec<_>>().join(" "));
                }
            }
            if let Some(t) = opt("--fields").and_then(|v| v.parse::<usize>().ok()) {
                if t == n {
                    for (slot, rec) in std::iter::once(&s.record).chain(s.others.iter().map(|o| &o.record)).enumerate() {
                        let a = mr_sim::kart::KART_ARRAY + 0x504 * slot as u32;
                        let want = format!("{:?}", mr_sim::kart::KartState::from_record(cap.read(vb, a, 0x504).unwrap()));
                        let got = format!("{:?}", mr_sim::kart::KartState::from_record(rec));
                        let (w, g): (Vec<&str>, Vec<&str>) = (want.split(", ").collect(), got.split(", ").collect());
                        for (x, y) in w.iter().zip(g.iter()) {
                            if x != y {
                                println!("  kart {slot}: oracle {x} | port {y}");
                            }
                        }
                    }
                }
            }
            let rng = cap.read(vb, 0x800B_0BCC, 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()));
            if rng != Some(s.globals.rng) {
                println!("tick {n} VB {vb}: rng pointer oracle {:08X?} port {:08X}", rng, s.globals.rng);
            }
            let seed = cap.read(vb, 0x8000_9010, 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()));
            if seed != Some(s.rand_seed()) {
                println!("tick {n} VB {vb}: rand seed oracle {:08X?} port {:08X}", seed, s.rand_seed());
            }
        }
        let mut ok = true;
        for (slot, rec) in std::iter::once(&s.record).chain(s.others.iter().map(|o| &o.record)).enumerate() {
            let a = mr_sim::kart::KART_ARRAY + 0x504 * slot as u32;
            for (o, name) in [(0usize, "x"), (4, "y"), (8, "z")] {
                let want = rd(vb, a + o as u32) as i64;
                let got = i32::from_le_bytes(rec[o..o + 4].try_into().unwrap()) as i64;
                if want != got {
                    ok = false;
                    max_err = max_err.max((want - got).abs());
                    if first_bad.is_none() {
                        first_bad = Some((n, vb, slot, name, want, got));
                    }
                }
            }
        }
        let cam = cap.read(vb, 0x800C_4F40 + 0x28, 12).unwrap();
        let want_eye: Vec<i32> = cam.chunks(4).map(|c| i32::from_le_bytes(c.try_into().unwrap())).collect();
        if want_eye != s.camera.pos.to_vec() {
            ok = false;
            if first_bad.is_none() {
                first_bad = Some((n, vb, 9, "camera", want_eye[0] as i64, s.camera.pos[0] as i64));
            }
        }
        if ok {
            exact += 1;
        }
        if n >= max_ticks {
            break;
        }
    }
    println!("{dir}: {n} ticks, {exact} with all 3 karts' positions and the camera eye exact");
    match first_bad {
        Some((t, vb, slot, f, want, got)) => println!("first difference: tick {t} (VB {vb}) kart {slot} {f}: oracle {want}, port {got}; max |pos err| {max_err}"),
        None => println!("no difference"),
    }
}

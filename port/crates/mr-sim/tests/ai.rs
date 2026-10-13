//! The AI drivers (src/ai.rs) against the oracle (Session 3).
//!
//! Needs the owner's disc and the AI captures under analysis/ai/ (local,
//! gitignored: tools/oracle/capture.py ... --physics, capture_noitems.py);
//! each test skips with a message when they are absent.
//!
//! ai.rs is included with #[path] until lib.rs declares `pub mod ai;`.

mod fixed {
    pub use mr_sim::fixed::*;
}
mod globals {
    pub use mr_sim::globals::*;
}
mod kart {
    pub use mr_sim::kart::*;
}
mod tables {
    pub use mr_sim::tables::*;
}
mod world {
    pub use mr_sim::world::*;
}
mod init {
    pub use mr_sim::init::*;
}
#[path = "../src/ai.rs"]
#[allow(dead_code)]
mod ai;
#[path = "../src/bin/mr-ai/harness.rs"]
#[allow(dead_code)]
mod harness;

use mr_sim::kart::{KART_ARRAY, RECORD_SIZE};
use mr_sim::mem::Mem;
use mr_sim::oracle::{self, Capture};

fn capture(name: &str) -> Option<(Capture, Vec<u8>, mr_sim::world::World)> {
    std::env::set_current_dir(oracle::repo_root()).ok()?;
    if mr_disc::Disc::open_default().is_err() {
        eprintln!("skip: owner's disc not found");
        return None;
    }
    let dir = format!("analysis/ai/{name}");
    let Ok(cap) = Capture::open(&dir) else {
        eprintln!("skip: {dir} not captured");
        return None;
    };
    let base = cap.ram0.clone()?;
    let w = oracle::dock_world(&base);
    Some((cap, base, w))
}

/// FUN_800912F8 teacher-forced on every AI kart and tick of a 3-lap race
/// (items on), with the BIOS rand() seed from the capture: buttons, target
/// point and curvature as the game produced them.
#[test]
fn decisions_match_the_oracle() {
    let Some((cap, base, w)) = capture("c_rev") else { return };
    let ticks = harness::ticks(&cap);
    let rep = harness::teacher_force(&cap, &base, &w, &ticks, false);
    print!("{}", rep.text());
    assert!(rep.rand_seeded, "capture lacks the rand() seed region");
    assert!(rep.calls > 20_000);
    for f in ["steer (LEFT/RIGHT)", "accelerate (CROSS)", "brake (CIRCLE)", "turbo (R1)", "target +0x3E8", "curvature +0x4A0", "+0x4CB staleness"] {
        assert_eq!(rep.fields[f].bad, 0, "{f}");
    }
    // Random-driven bits: exact except where other game code drew from
    // rand() before the AI in that frame (4 kart-ticks in this race).
    assert!(rep.fields["all buttons"].bad <= 8, "{}", rep.fields["all buttons"].bad);
}

/// mr-sim's kart update driven by the oracle's AI buttons: every motion
/// field exact except at item events (weapon hits after the kart loop,
/// weapons fired: out of scope).
#[test]
fn kart_update_matches_on_ai_karts() {
    let Some((cap, base, w)) = capture("c_rev") else { return };
    let ticks = harness::ticks(&cap);
    let rep = harness::teacher_force_update(&cap, &base, &w, &ticks, false);
    print!("{}", rep.text());
    let all = &rep.fields["all motion fields"];
    assert!(all.n > 20_000);
    for f in ["x", "y", "z", "heading", "pitch", "roll", "fwd_speed", "side_speed", "lap"] {
        assert_eq!(rep.fields[f].bad, 0, "{f}");
    }
    assert!(all.bad * 500 < all.n, "{} of {}", all.bad, all.n);
}

/// The 48 vehicle templates decoded from FUN_80087EB4 (+ wheel positions
/// from the VEHIC_* models for the loaded vehicles) equal the game's table.
#[test]
fn vehicle_templates_match_ram() {
    let Some((_cap, base, w)) = capture("c_rev") else { return };
    let s = harness::check_vehicles(&base, &w);
    print!("{s}");
    // Vehicle 22 (boss LOG) keeps wheel positions from an earlier load.
    assert!(s.contains("47 of 48 templates byte-identical"));
}

/// The opponents' records from ai::init_opponent agree with the oracle's
/// AI karts on everything that does not change while racing.
#[test]
fn opponent_init_static_fields() {
    let Some((_cap, base, w)) = capture("c_rev") else { return };
    let m = Mem::new(base[..0x20_0000].to_vec(), base[0x20_0000..0x20_0400].to_vec());
    let mut disc = mr_disc::Disc::open_default().unwrap();
    let tpl = ai::vehicle_templates(&w.exe, &w.tables).unwrap();
    let setup = mr_sim::init::RaceSetup { mode: 1, race_type: 0, race_flags: 0, challenge: 0, humans: 1, karts: 6 };
    for slot in 1..6u8 {
        let r = m.slice(KART_ARRAY + RECORD_SIZE as u32 * slot as u32, RECORD_SIZE);
        let (c, v) = (r[0x4B2], r[0x4B1]);
        // The character's default vehicle, as the race loader picks it.
        assert_eq!(ai::character_entry(&w.exe, c), (true, v));
        let mut t = tpl[v as usize];
        let vm = mr_assets::mdl::Mdl::parse(&disc.read(&ai::model_path(&w.exe, ai::addr::VEHICLE_PATHS, v, "MDL").unwrap()).unwrap()).unwrap();
        ai::place_wheels(&mut t, &vm);
        let cm = mr_assets::mdl::Mdl::parse(&disc.read(&ai::model_path(&w.exe, ai::addr::CHARACTER_PATHS, c, "MDL").unwrap()).unwrap()).unwrap();
        let o = ai::Opponent { slot, character: c, vehicle: v, grid: r[0x4CA] as i32 - 1 };
        let mut rng = 0x8001_F94C;
        let mut seed = 1u32;
        let mut rand = || harness::bios_rand(&mut seed);
        let (rec, _) = ai::init_opponent(&w, &t, setup, o, cm.parts[0].record.pivot[1], &mut rng, &mut rand);
        // Static: wheel geometry, probe corners, radius, counts, handling,
        // camera mount, identity bytes.
        let mut stat: Vec<(usize, usize)> = Vec::new();
        for i in 0..rec[0x170] as usize {
            let b = 0x14 + 0x2C * i;
            stat.push((b + 0x18, 6));
            stat.push((b + 0x22, 2));
            stat.push((b + 0x26, 2));
        }
        stat.extend([(0x164, 8), (0x170, 8), (0x178, 10), (0x184, 2), (0x188, 4), (0x412, 2), (0x438, 2), (0x4B1, 2), (0x4D7, 1)]);
        for (o, n) in stat {
            assert_eq!(&rec[o..o + n], &r[o..o + n], "slot {slot} (char {c}, vehicle {v}) +{o:#X}");
        }
        assert_eq!(rec[0x496] & 1, 1);
    }
}

/// Free run: the five AI karts drive three laps of DOCK on their own
/// (items off) without reaching an out-of-scope branch.
#[test]
fn free_run_three_laps() {
    let Some((cap, base, w)) = capture("c_rev") else { return };
    let ticks = harness::ticks(&cap);
    let fr = harness::free_run(&cap, &base, &w, &ticks, None);
    print!("{}", fr.summary());
    assert!(fr.stopped.is_none(), "{:?}", fr.stopped);
    for k in 1..6 {
        assert!(fr.lap_times.get(&k).map(|v| v.len()).unwrap_or(0) >= 3, "kart {k} laps");
    }
}

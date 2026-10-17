//! Phase 6b: what COURSE SECRETS starts (docs/spec/adventure.md "COURSE
//! SECRETS"). On the disc: every mode loads with the game's set-up and
//! runs (BOSS CHALLENGE on the six arenas, BATTLE AS BOSS, DOGFIGHT and
//! DOOZER BATTLE on ARENA/BATTLE, REVERSE RACE, BATTLE RACE on a race and
//! a stunt course, the reversed stunt trial).

use mr_sim::course::CourseInfo;
use mr_sim::session::{Session, SessionConfig};
use mr_sim::tables::ExeImage;

fn disc() -> mr_disc::Disc {
    std::env::set_current_dir(mr_sim::oracle::repo_root()).unwrap();
    mr_disc::Disc::open_default().unwrap()
}

fn run(s: &mut Session, ticks: usize, b: u16) {
    for _ in 0..ticks {
        s.tick(b).unwrap();
    }
}

#[test]
fn boss_challenge_on_the_arenas() {
    let mut d = disc();
    let exe = ExeImage::parse(&d.read("SLUS_012.37").unwrap()).unwrap();
    for (name, bosses) in [("CANYON", vec![0x20]), ("T_SQUARE", vec![0x21]), ("THEATRE", vec![0x22, 0x23, 0x24]), ("CHAMBER", vec![0x25, 0x26]), ("LAGOON", vec![0x27]), ("LAB", vec![0x28])] {
        let info = CourseInfo::find(&exe, name).unwrap();
        let mut s = Session::with_config(&mut d, SessionConfig { boss_challenge: true, ..SessionConfig::course(&info) }).unwrap();
        let g = &s.globals;
        assert_eq!((g.mode, g.karts, g.adv.bosses, g.end_mode), (7, 1, bosses.len() as i16, 1 + bosses.len() as u8), "{name}");
        assert!((0..bosses.len() as i8).contains(&g.adv.boss_cam), "{name}: boss camera");
        let chars: Vec<u8> = s.others.iter().map(|o| o.character).collect();
        assert_eq!(chars, bosses, "{name}");
        run(&mut s, 150 + 300, 0x4000);
        assert_eq!(s.kart.lap, 0, "{name}");
        eprintln!("{name}: clock {} player {} {:?}", s.race_clock(), s.kart.race_time, (s.kart.x, s.kart.z));
    }
}

#[test]
fn arena_secrets() {
    let mut d = disc();
    let exe = ExeImage::parse(&d.read("SLUS_012.37").unwrap()).unwrap();
    // BATTLE AS BOSS (race flag 0x20) on CHAMBER: WALDORF (pick 1).
    let info = CourseInfo::find(&exe, "CHAMBER").unwrap();
    let mut s = Session::with_config(&mut d, SessionConfig { race_flags: 0x20, boss_pick: 1, ..SessionConfig::course(&info) }).unwrap();
    assert_eq!((s.kart.character, s.kart.vehicle, s.globals.mode, s.globals.karts), (0x26, 0x15, 2, 6));
    run(&mut s, 300, 0x4000);
    // DOGFIGHT (0x80) and DOOZER BATTLE (0x100) on ARENA/BATTLE.
    let info = CourseInfo::find(&exe, "BATTLE").unwrap();
    let mut s = Session::with_config(&mut d, SessionConfig { race_flags: 0x80, ..SessionConfig::course(&info) }).unwrap();
    assert!(std::iter::once(s.kart.vehicle).chain(s.others.iter().map(|o| o.vehicle)).all(|v| v == 8), "every kart on the PLANE");
    run(&mut s, 300, 0x4000);
    let mut s = Session::with_config(&mut d, SessionConfig { race_flags: 0x100, ..SessionConfig::course(&info) }).unwrap();
    assert!(std::iter::once(s.kart.character).chain(s.others.iter().map(|o| o.character)).all(|c| c == 24), "every kart a Doozer");
    let v: Vec<u8> = std::iter::once(s.kart.vehicle).chain(s.others.iter().map(|o| o.vehicle)).collect();
    assert_eq!(v, [6, 25, 27, 6, 25, 27]);
    run(&mut s, 300, 0x4000);
}

#[test]
fn race_course_secrets() {
    let mut d = disc();
    let exe = ExeImage::parse(&d.read("SLUS_012.37").unwrap()).unwrap();
    for name in ["DOCK", "ISLAND", "SPEEDWAY", "DISCO"] {
        let info = CourseInfo::find(&exe, name).unwrap();
        // REVERSE RACE (flag 2; DISCO 10): grid + 6, +0x4C0 = 7, +0x4DF = 1
        let flags = if info.id == 0x1F { 10 } else { 2 };
        let mut s = Session::with_config(&mut d, SessionConfig { race_flags: flags, ..SessionConfig::course(&info) }).unwrap();
        assert_eq!((s.globals.mode, s.kart.line_side, s.kart.path_mode), (1, 7, 1), "{name}");
        run(&mut s, 150 + 900, 0x4000);
        // BATTLE RACE (flag 1): game mode 2 on the race course
        let mut s = Session::with_config(&mut d, SessionConfig { race_flags: 1, ..SessionConfig::course(&info) }).unwrap();
        assert_eq!((s.globals.mode, s.globals.race_type), (2, info.race_type), "{name}");
        run(&mut s, 150 + 600, 0x4000);
    }
    // The stunt courses: the reversed trial (flag 4) and BATTLE RACE.
    let info = CourseInfo::find(&exe, "MOUNTAIN").unwrap();
    let mut s = Session::with_config(&mut d, SessionConfig { race_flags: 4, ..SessionConfig::course(&info) }).unwrap();
    assert_eq!((s.globals.mode, s.globals.karts), (3, 1));
    run(&mut s, 150 + 600, 0x4000);
    let mut s = Session::with_config(&mut d, SessionConfig { race_flags: 1, ..SessionConfig::course(&info) }).unwrap();
    assert_eq!((s.globals.mode, s.globals.karts), (2, 6));
    run(&mut s, 150 + 600, 0x4000);
}

/// A BOSS CHALLENGE won (oracle analysis/oracle/secrets/boss_win: LAGOON,
/// THE CHIEF's damage held at 1000 by a poke every VBlank): the boss is
/// knocked out at GO (VB 590, place 2), the player wins the next frame
/// (place 1, out, the clock 10 rounded to 9), mode 10, and the course's
/// flags gain 0x100 (0x59 -> 0x159), as the game's save block shows.
#[test]
fn boss_win_matches_oracle() {
    let mut d = disc();
    let Ok(cap) = mr_sim::oracle::Capture::open(mr_sim::oracle::repo_root().join("analysis/oracle/secrets/boss_win")) else { return };
    let exe = ExeImage::parse(&d.read("SLUS_012.37").unwrap()).unwrap();
    let info = CourseInfo::find(&exe, "LAGOON").unwrap();
    let mut vbs: Vec<u32> = cap.vbs.iter().copied().filter(|&v| (580..=600).contains(&v)).collect();
    vbs.sort();
    let m0 = cap.mem_at(vbs[0], None);
    let mut cfg = SessionConfig { boss_challenge: true, ..SessionConfig::course(&info) };
    cfg.faces = Some(m0.u32(0x800B_0FA4));
    let mut s = Session::with_config(&mut d, cfg).unwrap();
    s.load_mem(&m0);
    assert_eq!((s.globals.mode, s.globals.adv.bosses, s.globals.adv.course_flags), (7, 1, 0x59));
    let key = |v: u32| (cap.i32(v, 0x800B_0C70).unwrap(), cap.i32(v, 0x800B_0A94).unwrap(), cap.i32(v, 0x800B_0D90).unwrap());
    let mut prev = vbs[0];
    for &v in &vbs[1..] {
        if key(v) == key(prev) {
            continue;
        }
        let m = cap.mem_at(v, None);
        // (the capture's poke: the boss's damage back to 1000 each VBlank)
        s.others[0].kart.impact = 1000;
        s.tick_dt(m.u16(0x800B_0CE8), m.i32(0x800B_0AEC)).unwrap();
        prev = v;
    }
    let o = mr_sim::kart::KartState::from_record(cap.read(prev, mr_sim::kart::KART_ARRAY, mr_sim::kart::RECORD_SIZE).unwrap());
    let ob = mr_sim::kart::KartState::from_record(cap.read(prev, mr_sim::kart::KART_ARRAY + 0x504, mr_sim::kart::RECORD_SIZE).unwrap());
    assert_eq!(cap.read(prev, 0x800B_0D46, 2).unwrap(), [10, 0], "the results (mode 10)");
    assert_eq!((s.kart.finished, s.kart.race_pos, s.kart.race_time), (o.finished, o.race_pos, o.race_time));
    assert_eq!((s.others[0].kart.finished, s.others[0].kart.race_pos), (ob.finished, ob.race_pos));
    assert_eq!((s.kart.finished, s.kart.race_pos, s.globals.mode), (1, 1, 10));
    let flags = cap.mem_at(prev, None).u16(0x800E_0226);
    assert_eq!(flags, 0x159);
    assert_eq!(s.globals.adv.course_flags, flags, "the course flags = the game's save block");
}

/// Phase 11: a BOSS CHALLENGE's load queues its first boss's clip base
/// (0x800AC230[0x800B0C7E * 0x14], FUN_8008C4AC in game mode 7): on LAGOON
/// 0xDC, as the oracle queued it (0x80DC = VOICES/WIN.XA channel 12, then
/// the course track; tools/oracle/xa_probe.py LAGOON_menu + the route of
/// routes/boss_lagoon.json, VB 285).
#[test]
fn boss_challenge_clip() {
    let mut d = disc();
    let exe = ExeImage::parse(&d.read("SLUS_012.37").unwrap()).unwrap();
    let info = CourseInfo::find(&exe, "LAGOON").unwrap();
    let boss = mr_sim::secrets::bosses(info.id).0[0].0;
    assert_eq!(exe.u8(0x800A_C230 + 0x14 * boss as u32), 0xDC);
    // (the bosses have no voice flag: FUN_80060580 ignores them)
    assert_eq!(exe.u8(0x800A_C22E + 0x14 * boss as u32) & 1, 0);
}

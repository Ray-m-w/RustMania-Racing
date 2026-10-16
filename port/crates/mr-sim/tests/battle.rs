//! Session 7 (phase 4): 1 PLAYER BATTLE (race type 2, game mode 2) on the
//! arenas (docs/spec/battle.md). On the disc: every arena loads and runs a
//! battle. With the oracle capture analysis/oracle/battle/win (local;
//! tools/oracle/capture_poked.py: LAGOON's grid with the five opponents'
//! damage poked to 1000 and the player's clock to 30000): the knock-outs
//! at GO, the win, the course record and the won bit against the game's RAM.

use mr_sim::course::CourseInfo;
use mr_sim::oracle::Capture;
use mr_sim::save::SaveBlock;
use mr_sim::session::{Session, SessionConfig};
use mr_sim::tables::ExeImage;

fn disc() -> mr_disc::Disc {
    std::env::set_current_dir(mr_sim::oracle::repo_root()).unwrap();
    mr_disc::Disc::open_default().unwrap()
}

#[test]
fn battle_on_every_arena() {
    let mut d = disc();
    let exe = ExeImage::parse(&d.read("SLUS_012.37").unwrap()).unwrap();
    let arenas = CourseInfo::arenas(&exe);
    let names: Vec<&str> = arenas.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["CANYON", "CHAMBER", "LAB", "T_SQUARE", "THEATRE", "LAGOON", "BATTLE", "VOLCANO", "HOUSE", "WHATEVER"]);
    for info in arenas {
        let mut s = Session::with_config(&mut d, SessionConfig::course(&info)).unwrap();
        let karts = if info.cup > 6 { 5 } else { 6 };
        assert_eq!((s.globals.mode, s.globals.race_type, s.globals.karts, s.globals.end_mode), (2, 2, karts, karts as u8), "{}", info.name);
        assert_eq!(s.globals.countdown, 0x78 + 180, "{}: countdown", info.name);
        // the countdown, then 20 s of CROSS
        for _ in 0..150 + 600 {
            s.tick(0x4000).unwrap();
        }
        assert!(s.kart.race_time >= 600 * 10 || s.kart.finished != 0, "{}: the battle clock", info.name);
        assert_eq!(s.kart.lap, 0, "{}: no laps", info.name);
    }
}

/// The win (oracle VB 241 -> 243): five knock-outs at GO (places 6..2,
/// damage 1000 + place, 0x800B0CD8 down to 1), then FUN_8007916C makes the
/// player the winner (+0x4C7 = 1, place 1, the clock 30010 rounded to
/// 30009), the course record 30009 by Kermit / RACER and the won bit, as
/// the game's save block shows them.
#[test]
fn battle_win_matches_oracle() {
    let mut d = disc();
    let Ok(cap) = Capture::open(mr_sim::oracle::repo_root().join("analysis/oracle/battle/win")) else { return };
    let exe = ExeImage::parse(&d.read("SLUS_012.37").unwrap()).unwrap();
    let info = CourseInfo::find(&exe, "LAGOON").unwrap();
    let mut vbs: Vec<u32> = cap.vbs.iter().copied().filter(|&v| (238..=246).contains(&v)).collect();
    vbs.sort();
    let m0 = cap.mem_at(vbs[0], None);
    let mut cfg = SessionConfig::course(&info);
    cfg.faces = Some(m0.u32(0x800B_0FA4));
    let cv = |i: u32| (m0.u8(mr_sim::kart::KART_ARRAY + 0x504 * i + 0x4B2), m0.u8(mr_sim::kart::KART_ARRAY + 0x504 * i + 0x4B1));
    (cfg.character, cfg.vehicle) = cv(0);
    cfg.lineup = Some([cv(1), cv(2), cv(3), cv(4), cv(5)]);
    cfg.lap_record = m0.i32(0x800E_0190 + 0x20 * 4 + 8 * 2);
    let mut s = Session::with_config(&mut d, cfg).unwrap();
    s.load_mem(&m0);
    assert_eq!(s.globals.lap_record, 34062);
    let key = |v: u32| (cap.i32(v, 0x800B_0C70).unwrap(), cap.i32(v, 0x800B_0A94).unwrap(), cap.i32(v, 0x800B_0D90).unwrap());
    let mut prev = vbs[0];
    for &v in &vbs[1..] {
        if key(v) == key(prev) {
            continue;
        }
        let m = cap.mem_at(v, None);
        s.tick_dt(m.u16(0x800B_0CE8), m.i32(0x800B_0AEC)).unwrap();
        prev = v;
    }
    let o = mr_sim::kart::KartState::from_record(cap.read(prev, mr_sim::kart::KART_ARRAY, mr_sim::kart::RECORD_SIZE).unwrap());
    assert_eq!(cap.read(prev, 0x800B_0D46, 2).unwrap(), [10, 0], "the results (mode 10)");
    assert_eq!((s.kart.finished, s.kart.race_pos, s.kart.shown_pos, s.kart.race_time), (o.finished, o.race_pos, o.shown_pos, o.race_time));
    assert_eq!((s.kart.finished, s.kart.race_pos, s.kart.race_time, s.globals.mode, s.globals.end_mode), (1, 1, 30009, 10, 1));
    for (i, k) in s.others.iter().enumerate() {
        let o = mr_sim::kart::KartState::from_record(cap.read(prev, mr_sim::kart::KART_ARRAY + 0x504 * (i as u32 + 1), mr_sim::kart::RECORD_SIZE).unwrap());
        assert_eq!((k.kart.finished, k.kart.race_pos, k.kart.impact), (o.finished, o.race_pos, o.impact), "kart {}", i + 1);
    }
    assert_eq!(s.globals.lap_record, 30009);
    // the save block: the record and its holder, the won bit (0x41 -> 0x43)
    let mut b = SaveBlock::zero();
    let at = 0x20 * 4 + 8 * 2;
    for i in 0..8 {
        b.set_u8(at + i, m0.u8(0x800E_0190 + (at + i) as u32));
    }
    assert!(b.lap_record(4, 2, s.globals.lap_record, s.kart.character, s.kart.vehicle));
    assert!(b.race_won(4, 2));
    let m1 = cap.mem_at(*cap.vbs.last().unwrap(), None);
    for i in 0..8 {
        assert_eq!(b.u8(at + i), m1.u8(0x800E_0190 + (at + i) as u32), "save block byte +{i}");
    }
}

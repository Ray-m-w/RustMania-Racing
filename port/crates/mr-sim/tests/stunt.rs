//! Session 7 (phase 3): the stunt trial (race type 3, game mode 3), the
//! game's only timed solo mode (docs/spec/stunt-trial.md). On the disc:
//! every stunt course runs a trial. With the oracle capture
//! analysis/oracle/stunt/finish (local; tools/oracle/capture_poked.py,
//! MOUNTAIN's grid with the kart poked before the finish gate): the finish
//! frame against the game's RAM, the course record and won bit included.

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
fn stunt_trial_on_every_stunt_course() {
    let mut d = disc();
    let exe = ExeImage::parse(&d.read("SLUS_012.37").unwrap()).unwrap();
    let stunts = CourseInfo::stunts(&exe);
    assert_eq!(stunts.len(), 6, "FUN, REST, DINER, ROOFTOPS, MOUNTAIN, MLABS");
    for info in stunts {
        let mut s = Session::with_config(&mut d, SessionConfig::course(&info)).unwrap();
        assert_eq!((s.globals.mode, s.globals.race_type, s.globals.karts), (3, 3, 1), "{}", info.name);
        assert_eq!(s.globals.countdown, 0x78 + 180, "{}: countdown", info.name);
        // the countdown, then 20 s of CROSS
        for _ in 0..150 + 600 {
            s.tick(0x4000).unwrap();
        }
        assert_eq!(s.camera.mode, 3, "{}: the strip camera", info.name);
        assert_eq!(s.kart.race_time, 600 * 10, "{}: the trial clock", info.name);
        assert!(s.kart.strip > 0, "{}: the kart moved along the strip", info.name);
    }
    // MOUNTAIN's progress-bar marks: 0x800B0C90 in the oracle's RAM
    let info = CourseInfo::find(&exe, "MOUNTAIN").unwrap();
    let s = Session::with_config(&mut d, SessionConfig::course(&info)).unwrap();
    assert_eq!(mr_sim::stunt::banner_marks(&s.world), vec![1004, 44960]);
    assert_eq!(mr_sim::stunt::strip_quads(&s.world), 370);
}

/// The finish frame (oracle VB 276 -> 282): the last gate, the time
/// rounded to 3 (170 -> 171, then the frame's 10: +0x358 = 181), place 1,
/// out (+0x4C7 = 2), the course record 171 by Kermit / RACER and the won
/// bit, as the game's save block shows them.
#[test]
fn stunt_finish_matches_oracle() {
    let mut d = disc();
    let Ok(cap) = Capture::open(mr_sim::oracle::repo_root().join("analysis/oracle/stunt/finish")) else { return };
    let exe = ExeImage::parse(&d.read("SLUS_012.37").unwrap()).unwrap();
    let info = CourseInfo::find(&exe, "MOUNTAIN").unwrap();
    let m0 = cap.mem_at(276, None);
    let m1 = cap.mem_at(282, None);
    let mut cfg = SessionConfig::course(&info);
    cfg.faces = Some(m0.u32(0x800B_0FA4));
    let mut s = Session::with_config(&mut d, cfg).unwrap();
    s.load_mem(&m0);
    assert_eq!((s.kart.gate, s.kart.finished, s.globals.lap_record), (1, 0, 43164));
    // the samples hold the next frame's clock step (mr-course-check
    // `clock_plan`)
    s.clock_plan = Some((false, true));
    s.clock_end_dt = Some(m1.i32(0x800B_0AEC));
    s.tick_dt(m1.u16(0x800B_0CE8), m0.i32(0x800B_0AEC)).unwrap();
    let o = mr_sim::kart::KartState::from_record(cap.read(282, mr_sim::kart::KART_ARRAY, mr_sim::kart::RECORD_SIZE).unwrap());
    assert_eq!((s.kart.gate, s.kart.finished, s.kart.race_time, s.kart.race_pos, s.kart.progress), (o.gate, o.finished, o.race_time, o.race_pos, o.progress));
    assert_eq!((s.kart.gate, s.kart.finished, s.kart.race_time), (2, 2, 181));
    assert_eq!(s.globals.lap_record, 171);
    // the save block: the record and its holder, the won bit (0x103)
    let mut b = SaveBlock::zero();
    let at = 0x20 * 4 + 8 * 3;
    for i in 0..8 {
        b.set_u8(at + i, m0.u8(0x800E_0190 + (at + i) as u32));
    }
    assert!(b.lap_record(4, 3, s.globals.lap_record, s.kart.character, s.kart.vehicle));
    assert!(b.race_won(4, 3));
    for i in 0..8 {
        assert_eq!(b.u8(at + i), m1.u8(0x800E_0190 + (at + i) as u32), "save block byte +{i}");
    }
}

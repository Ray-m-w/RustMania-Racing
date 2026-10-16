//! Session 7 (phase 6): ADVENTURE (game mode 0xB, 0x800B07E6 = 1;
//! docs/spec/adventure.md). On the disc: every course (race, stunt, arena)
//! loads an ADVENTURE with its parked karts and runs it to the time limit.
//! With the oracle captures (local; tools/oracle/routes/adventure_*.json):
//! the DOCK race load (three karts, every named field, the random state),
//! and the poked token secret (analysis/oracle/adventure/token: turbo tokens
//! poked to 1): the run ends, the course's save flags equal the game's RAM.

use mr_sim::course::CourseInfo;
use mr_sim::kart::{KartState, KART_ARRAY, RECORD_SIZE};
use mr_sim::oracle::Capture;
use mr_sim::session::{Session, SessionConfig};
use mr_sim::tables::ExeImage;

fn disc() -> mr_disc::Disc {
    std::env::set_current_dir(mr_sim::oracle::repo_root()).unwrap();
    mr_disc::Disc::open_default().unwrap()
}

fn adventure(info: &CourseInfo) -> SessionConfig {
    SessionConfig { adventure: true, course_flags: 0x101, characters: 0xFF, ..SessionConfig::course(info) }
}

#[test]
fn adventure_on_every_course() {
    let mut d = disc();
    let exe = ExeImage::parse(&d.read("SLUS_012.37").unwrap()).unwrap();
    let courses: Vec<CourseInfo> = CourseInfo::races(&exe).into_iter().chain(CourseInfo::stunts(&exe)).chain(CourseInfo::arenas(&exe)).collect();
    assert_eq!(courses.len(), 18 + 6 + 10);
    for info in courses {
        let mut s = Session::with_config(&mut d, adventure(&info)).unwrap();
        let c = exe.u8(mr_sim::session::COURSE_CHARACTER + 0xC * info.id as u32) as i8;
        let parked = (info.cup < 6) as i16 + (c >= 0 && c > 7) as i16;
        assert_eq!((s.globals.mode, s.globals.challenge, s.globals.karts, s.globals.countdown), (0xB, 1, 1 + parked, 0), "{}", info.name);
        let limit = s.globals.adv.time_limit;
        assert!([54000, 72000, 180000].contains(&limit), "{}: limit {limit}", info.name);
        // 10 s of CROSS, then the clock just short of the limit: time up,
        // the results.
        for _ in 0..300 {
            s.tick(0x4000).unwrap();
        }
        assert!(s.globals.mode == 0xB || s.globals.mode == 10, "{}", info.name);
        if s.globals.mode == 0xB {
            s.globals.race_clock = limit - 20;
            for _ in 0..10 {
                s.tick(0x4000).unwrap();
            }
            assert_eq!((s.globals.mode, s.kart.finished), (10, 2), "{}: time up", info.name);
        }
    }
}

/// The DOCK ADVENTURE's race load (oracle VB 358): Kermit at a random one
/// of grid points 12..17, the Doozer (flags 0x41) on point 18, LINK (DOCK's
/// character, flags 0x21) on 19, and the random state after it.
#[test]
fn adventure_load_matches_oracle() {
    let mut d = disc();
    let Ok(cap) = Capture::open(mr_sim::oracle::repo_root().join("analysis/oracle/adventure/dock")) else { return };
    let exe = ExeImage::parse(&d.read("SLUS_012.37").unwrap()).unwrap();
    let info = CourseInfo::find(&exe, "DOCK").unwrap();
    let load = 358;
    let pre = cap.mem_at(load - 1, None);
    let mut cfg = adventure(&info);
    cfg.rand_seed = pre.u32(0x8000_9010);
    cfg.rng_start = pre.u32(0x800B_0BCC);
    cfg.faces = Some(pre.u32(0x800B_0FA4));
    cfg.course_flags = pre.u16(mr_sim::globals::course_flags_addr(&pre));
    cfg.characters = pre.u32(0x800E_02BC);
    cfg.doozer_vehicle = pre.u8(0x800B_0A14 + pre.u8(0x800B_0A10) as u32);
    let s = Session::with_config(&mut d, cfg).unwrap();
    let m = cap.mem_at(load, None);
    assert_eq!((s.globals.rng, s.rand_seed()), (m.u32(0x800B_0BCC), m.u32(0x8000_9010)));
    for (slot, k) in std::iter::once(&s.kart).chain(s.others.iter().map(|o| &o.kart)).enumerate() {
        let o = KartState::from_record(cap.read(load, KART_ARRAY + (RECORD_SIZE * slot) as u32, RECORD_SIZE).unwrap());
        let dd: Vec<_> = k.diff(&o).into_iter().filter(|d| !d.starts_with("trail") && !d.starts_with("model_slot") && !d.starts_with("char_slot") && !d.starts_with("token_colour")).collect();
        assert!(dd.is_empty(), "kart {slot}: {dd:?}");
    }
    assert_eq!(s.others.iter().map(|o| (o.character, o.kart.flags)).collect::<Vec<_>>(), [(24, 0x41), (11, 0x21)]);
}

/// The token secret: from the capture's first frame end, the oracle's pads;
/// the last token (0x800B0A52 poked to 1) ends the run (0x800B0803 = 4,
/// the player out, the results) and sets the course's secret 0x10, as the
/// game's save block shows it (DOCK flags 0x101 -> 0x111).
#[test]
fn adventure_token_secret_matches_oracle() {
    let mut d = disc();
    let Ok(cap) = Capture::open(mr_sim::oracle::repo_root().join("analysis/oracle/adventure/token")) else { return };
    let exe = ExeImage::parse(&d.read("SLUS_012.37").unwrap()).unwrap();
    let info = CourseInfo::find(&exe, "DOCK").unwrap();
    // frame ends: the last sample of each value of the random pointer
    let rng = |v: u32| cap.i32(v, 0x800B_0BCC).unwrap();
    let ends: Vec<u32> = cap.vbs.windows(2).filter(|w| w[0] >= 1 && w[1] <= 200 && rng(w[0]) != rng(w[1])).map(|w| w[0]).collect();
    let m0 = cap.mem_at(ends[0], None);
    let mut cfg = adventure(&info);
    cfg.faces = Some(m0.u32(0x800B_0FA4));
    let cv = |i: u32| (m0.u8(KART_ARRAY + 0x504 * i + 0x4B2), m0.u8(KART_ARRAY + 0x504 * i + 0x4B1));
    (cfg.character, cfg.vehicle) = cv(0);
    cfg.lineup = Some([cv(1), cv(2), cv(3), cv(4), cv(5)]);
    let mut s = Session::with_config(&mut d, cfg).unwrap();
    s.load_mem(&m0);
    s.round_robin_stepped(&m0);
    for w in ends.windows(2) {
        let m = cap.mem_at(w[1], None);
        s.tick_dt(m.u16(0x800B_0CE8), cap.mem_at(w[0], None).i32(0x800B_0AEC)).unwrap();
        if s.globals.mode == 10 {
            let o = KartState::from_record(m.slice(KART_ARRAY, RECORD_SIZE));
            assert_eq!((s.kart.finished, s.kart.adv_tokens, s.globals.adv.done), (o.finished, o.adv_tokens, m.u8(0x800B_0803)));
            assert_eq!((s.kart.finished, s.globals.adv.done), (1, 4));
            let flags = m.u16(mr_sim::globals::course_flags_addr(&m));
            assert_eq!((s.globals.adv.course_flags, flags), (0x111, 0x111));
            return;
        }
    }
    panic!("no results");
}

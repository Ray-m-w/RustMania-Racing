//! Phase 8: the attract demo (docs/spec/frontend.md "Attract demo"). The
//! oracle run (tools/oracle/routes/attract_demo.json: slot 1 -> the title,
//! idle 1800 VBlanks) picked NEW YORK and loaded six AI karts; the values
//! below are that capture's (analysis/oracle/attract/demo, local).

use mr_sim::course::CourseInfo;
use mr_sim::session::{Session, SessionConfig};
use mr_sim::tables::ExeImage;

fn disc() -> mr_disc::Disc {
    std::env::set_current_dir(mr_sim::oracle::repo_root()).unwrap();
    mr_disc::Disc::open_default().unwrap()
}

#[test]
fn demo_course_draw_matches_oracle() {
    let mut d = disc();
    let w = mr_sim::world::World::course(&mut d, mr_sim::title::TITLE_COURSE).unwrap();
    // the random pointer at the trigger frame, the last three demos 1, 15, 21
    let mut rng = 0x8001_FC92;
    let mut recent = [1, 15, 21];
    let c = mr_sim::title::demo_course(&w, &mut rng, [false; 4], &mut recent);
    assert_eq!(c, 17, "NEW YORK");
    assert_eq!(recent, [17, 1, 15], "RAM 0x800B0A1C after the draw");
    assert_eq!(rng, 0x8001_FC94);
    // a fresh game never repeats one of the last three
    let mut recent = mr_sim::title::DEMO_RECENT_INIT;
    let mut rng = mr_sim::session::RNG_START;
    let mut seen = Vec::new();
    for _ in 0..20 {
        let c = mr_sim::title::demo_course(&w, &mut rng, [false; 4], &mut recent);
        assert!(!seen.iter().rev().take(3).any(|&x| x == c));
        assert!(!matches!(c, 0x0B | 0x0C | 0x16..=0x19 | 0x1A..=0x1C | 0x1F), "no stunt course, no closed world 6 course: {c}");
        seen.push(c);
    }
}

#[test]
fn demo_race_load_and_run() {
    let mut d = disc();
    let exe = ExeImage::parse(&d.read("SLUS_012.37").unwrap()).unwrap();
    let info = CourseInfo::find(&exe, "NEW_YORK").unwrap();
    // the oracle's BIOS seed and random pointer the VBlank before its load
    let cfg = SessionConfig { demo: true, rand_seed: 0x33A9_74BF, rng_start: 0x8001_F94C, ..SessionConfig::course(&info) };
    let mut s = Session::with_config(&mut d, cfg).unwrap();
    let cv: Vec<(u8, u8)> = std::iter::once(&s.kart).chain(s.others.iter().map(|o| &o.kart)).map(|k| (k.character, k.vehicle)).collect();
    assert_eq!(cv, [(7, 6), (0, 0), (4, 4), (23, 18), (11, 10), (10, 9)], "the oracle's six karts");
    assert_eq!(s.kart.flags & 1, 1, "kart 0 is an AI kart");
    assert_eq!((s.globals.demo, s.globals.laps, s.countdown()), (1, 10, 0x3C));
    // the pad is ignored; the trackside cameras; over at 2:00 of race clock
    let mut ticks = 0;
    while !s.demo_over() {
        s.tick(0x4000 | 0x80).unwrap();
        ticks += 1;
        if ticks == 40 {
            assert_eq!(s.camera.mode, 2, "trackside camera");
        }
        assert!(ticks < 4000, "the demo ends at race clock 0x4651");
    }
    println!("demo over after {ticks} ticks, clock {}", s.globals.race_clock);
    assert!(s.globals.race_clock > 0x4650 || s.kart.finished != 0);
    assert_eq!(s.globals.mode, 1, "no results in the demo");
    assert!(s.kart.progress > 0, "kart 0 drove itself");
}

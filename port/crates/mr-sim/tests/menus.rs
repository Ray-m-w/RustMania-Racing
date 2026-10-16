//! Session 7 (front end): what mr-bevy's menus ask of the simulation, on the
//! disc only (no oracle data): the course menu's orbit camera (game state 0,
//! `Session::menu_tick`) on every race course, the frontend's course-select
//! camera (mode 5) on the title, and races with characters / vehicles other
//! than Kermit / RACER (the character select).

use mr_sim::course::CourseInfo;
use mr_sim::session::{Session, SessionConfig};
use mr_sim::tables::ExeImage;

fn disc() -> mr_disc::Disc {
    std::env::set_current_dir(mr_sim::oracle::repo_root()).unwrap();
    mr_disc::Disc::open_default().unwrap()
}

/// The course menu / character select session: the player's kart alone on
/// grid slot 0 (FUN_80072D8C -> FUN_800609F4(kart, c, v, 0, 0)).
fn menu_cfg(info: &CourseInfo, character: u8, vehicle: u8) -> SessionConfig {
    SessionConfig { opponents: 0, grid: 0, character, vehicle, ..SessionConfig::course(info) }
}

#[test]
fn course_menu_orbits_on_every_race_course() {
    let mut d = disc();
    let exe = ExeImage::parse(&d.read("SLUS_012.37").unwrap()).unwrap();
    for info in CourseInfo::races(&exe) {
        let mut s = Session::with_config(&mut d, menu_cfg(&info, 0, 0)).unwrap();
        s.menu_tick(2);
        assert_eq!(s.camera.mode, 1, "{}: orbit camera", info.name);
        let eye0 = s.camera.pos;
        for _ in 0..120 {
            s.menu_tick(2);
        }
        assert_ne!(s.camera.pos, eye0, "{}: the orbit moves", info.name);
        // and the race starts from the menu session's course as usual
        let mut r = Session::with_config(&mut d, SessionConfig::course(&info)).unwrap();
        r.tick(0).unwrap();
    }
}

#[test]
fn course_select_camera_on_the_title() {
    let mut d = disc();
    let mut s = Session::with_config(&mut d, SessionConfig::title(1, mr_sim::session::RNG_START, 30)).unwrap();
    for _ in 0..30 {
        s.tick(0).unwrap();
    }
    for row in [0i8, 3, 6] {
        s.camera_menu = Some((0xC, row));
        for _ in 0..90 {
            s.tick(0).unwrap();
        }
        assert_eq!(s.camera.mode, 5);
        // the eye settles at the world's film strip
        let y = (row as i32 * 10000 - 25000) * 0x20;
        assert!((s.camera.pos[1] - y).abs() < 0x2000, "row {row}: eye y {} vs {y}", s.camera.pos[1]);
        assert_eq!(s.camera.pos[0], 0xA000);
    }
}

#[test]
fn races_with_other_characters_and_vehicles() {
    let mut d = disc();
    let exe = ExeImage::parse(&d.read("SLUS_012.37").unwrap()).unwrap();
    let bog = CourseInfo::find(&exe, "BOG").unwrap();
    // (character, its own vehicle) for the eight characters of a new game,
    // plus Kermit in another vehicle
    for (c, v) in [(1u8, 1u8), (2, 2), (3, 7), (5, 30), (7, 6), (0, 1)] {
        let mut s = Session::with_config(&mut d, SessionConfig { character: c, vehicle: v, ..SessionConfig::course(&bog) }).unwrap();
        assert_eq!((s.record()[0x4B2], s.record()[0x4B1]), (c, v));
        for _ in 0..400 {
            s.tick(0x4000).unwrap();
        }
        assert!(s.kart_records().all(|(_, ch, _, _)| ch != c || s.record()[0x4B2] == c));
        assert_eq!(s.kart_records().filter(|(_, ch, _, _)| *ch == c).count(), 1, "no opponent with the player's character");
    }
}

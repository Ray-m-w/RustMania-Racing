//! Phase 7: two players (split screen; docs/spec/frontend.md "Two players",
//! battle.md "CHICKEN CHASE"). On the disc: a 2 PLAYER RACE loads two
//! human karts and nothing else, both pads drive, player 2's camera follows
//! kart 1; CHICKEN CHASE (game mode 0x14) places the chicken on the ground,
//! a kart on it catches it and it reappears elsewhere; the tenth catch wins.

use mr_sim::course::CourseInfo;
use mr_sim::session::{Session, SessionConfig};
use mr_sim::tables::ExeImage;

fn disc() -> mr_disc::Disc {
    std::env::set_current_dir(mr_sim::oracle::repo_root()).unwrap();
    mr_disc::Disc::open_default().unwrap()
}

fn two(info: &CourseInfo) -> SessionConfig {
    SessionConfig { players: 2, p2_character: 1, p2_vehicle: 0xFF, ..SessionConfig::course(info) }
}

#[test]
fn two_player_race_on_dock() {
    let mut d = disc();
    let exe = ExeImage::parse(&d.read("SLUS_012.37").unwrap()).unwrap();
    let info = CourseInfo::find(&exe, "DOCK").unwrap();
    let mut s = Session::with_config(&mut d, two(&info)).unwrap();
    let g = &s.globals;
    assert_eq!((g.mode, g.humans, g.split_screen, g.karts, s.others.len()), (1, 2, 1, 2, 1));
    let k1 = &s.others[0].kart;
    assert_eq!((k1.player_index, k1.flags & 1, k1.character), (1, 0, 1));
    assert!(s.camera2.is_some());
    // The countdown, then both pads on CROSS (player 2 steering right).
    for _ in 0..240 + 300 {
        s.pad2 = 0x4000 | 0x20;
        s.tick(0x4000).unwrap();
    }
    let (k0, k1) = (&s.kart, &s.others[0].kart);
    assert!(k0.progress > 0 && k1.progress > 0, "both karts race: {} {}", k0.progress, k1.progress);
    assert_ne!((k0.x, k0.z), (k1.x, k1.z));
    // Each camera follows its own kart.
    let c2 = s.camera2.as_ref().unwrap();
    let near = |c: [i32; 3], k: &mr_sim::KartState| ((c[0] - k.x) as i64).abs() + ((c[2] - k.z) as i64).abs() < 0x80000;
    assert!(near(s.camera.pos, &s.kart) && near(c2.pos, &s.others[0].kart), "cameras {:?} {:?}", s.camera.pos, c2.pos);
}

#[test]
fn chicken_chase_catches() {
    let mut d = disc();
    let exe = ExeImage::parse(&d.read("SLUS_012.37").unwrap()).unwrap();
    let info = CourseInfo::find(&exe, "LAGOON").unwrap();
    let mut s = Session::with_config(&mut d, SessionConfig { race_flags: 0x40, ..two(&info) }).unwrap();
    assert_eq!((s.globals.mode, s.globals.karts, s.globals.chicken.target), (0x14, 2, 10));
    assert!(s.globals.chicken.on_ground, "the chicken stands on the ground");
    while s.globals.countdown != 0 {
        s.tick(0).unwrap();
    }
    for n in 1..=10u8 {
        let c = s.globals.chicken.pos;
        // Put player 1 on the chicken (the catch reads the driver position
        // the render pass computes from the kart's).
        s.kart.x = c[0];
        s.kart.y = c[1];
        s.kart.z = c[2];
        s.kart.vx = 0;
        s.kart.vz = 0;
        s.tick(0).unwrap();
        assert_eq!(s.kart.adv_targets, n, "catch {n}");
        assert_ne!(s.globals.chicken.pos, c, "a new chicken");
    }
    // The tenth catch wins: both players out, the results; the voices
    // (FUN_8006EC4C: the other player's clip 2, then the winner's 3)
    assert!(s.kart.finished != 0 && s.others[0].kart.finished != 0);
    use mr_sim::sound::XaReq;
    let (c1, c2) = (s.kart.character, s.others[0].kart.character);
    assert_eq!(s.globals.sound.xa, vec![XaReq::Clip { character: c2, n: 2 }, XaReq::Clip { character: c1, n: 3 }]);
    s.tick(0).unwrap();
    assert_eq!(s.globals.mode, 10);
}

/// Phase 13: COURSE SECRETS' "- 2 PLAYERS" choices (FUN_80074A94 sub-states
/// 8 / 9 / 0xA: 0x800B07CA = 0x800B07F0[row] - 1): REVERSE RACE, BATTLE
/// and BATTLE RACE on a race course, the reversed stunt race, DOGFIGHT
/// (both on the PLANE) and DOOZER BATTLE (both Doozers, the vehicle turn
/// stepped per kart) load split screen with two human karts and race.
#[test]
fn two_player_secret_modes() {
    let mut d = disc();
    let exe = ExeImage::parse(&d.read("SLUS_012.37").unwrap()).unwrap();
    for (course, flags, mode) in [("DOCK", 2u16, 1i16), ("DOCK", 1, 2), ("DOCK", 8, 1), ("MOUNTAIN", 4, 3), ("BATTLE", 0x80, 2), ("BATTLE", 0x100, 2)] {
        let info = CourseInfo::find(&exe, course).unwrap();
        let mut s = Session::with_config(&mut d, SessionConfig { race_flags: flags, ..two(&info) }).unwrap();
        let g = &s.globals;
        assert_eq!((g.mode, g.humans, g.split_screen, g.karts, s.others.len()), (mode, 2, 1, 2, 1), "{course} flags {flags:#x}");
        if flags == 0x80 {
            assert_eq!((s.kart.vehicle, s.others[0].kart.vehicle), (mr_sim::secrets::PLANE, mr_sim::secrets::PLANE));
        }
        if flags == 0x100 {
            assert_eq!((s.kart.character, s.others[0].kart.character), (mr_sim::secrets::DOOZER, mr_sim::secrets::DOOZER));
            assert_ne!(s.kart.vehicle, s.others[0].kart.vehicle, "the turn steps per kart");
        }
        for _ in 0..600 {
            s.pad2 = 0x4000 | 0x20;
            s.tick(0x4000 | 0x80).unwrap();
        }
        assert!(s.camera2.is_some());
    }
}

/// Phase 13: TOURNAMENT - 2 PLAYERS (0x800C1DEF = 2, 0x800C1E4B = 3): a
/// league race is split screen with the two players and the league's third
/// kart (AI, its character / vehicle from the league).
#[test]
fn two_player_league_race() {
    let mut d = disc();
    let exe = ExeImage::parse(&d.read("SLUS_012.37").unwrap()).unwrap();
    let info = CourseInfo::find(&exe, "LONDON").unwrap();
    let cfg = SessionConfig { league: true, opponents: 1, draw_grid: true, lineup: Some([(5, 0xFF), (13, 12), (17, 16), (23, 18), (1, 1)]), ..two(&info) };
    let mut s = Session::with_config(&mut d, cfg).unwrap();
    let g = &s.globals;
    assert_eq!((g.mode, g.humans, g.split_screen, g.karts, s.others.len()), (1, 2, 1, 3, 2));
    assert_eq!((s.others[0].kart.flags & 1, s.others[1].kart.flags & 1), (0, 1), "kart 1 human, kart 2 AI");
    assert_eq!((s.others[1].kart.character, s.others[1].kart.vehicle), (13, 12));
    for _ in 0..900 {
        s.pad2 = 0x4000;
        s.tick(0x4000).unwrap();
    }
    assert!(s.others[1].kart.progress > 0, "the AI kart races");
}

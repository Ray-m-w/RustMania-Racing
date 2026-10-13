//! HUD layout tests. Disc-backed (skip with a message without the disc).
//! The expected primitives were matched one for one against the running
//! game's GP0 packets (tools/oracle/hud_check.py, docs/spec/hud.md).

use mr_assets::hud::{self, HudState, HudTables, Size};
use mr_assets::raw;

fn tables() -> Option<(HudTables, [(u16, u16); 32])> {
    let mut d = match mr_disc::Disc::open_default() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("SKIP: disc not available ({e:?})");
            return None;
        }
    };
    let exe = d.read("SLUS_012.37").unwrap();
    let t = HudTables::from_exe(&exe).unwrap();
    let file = d.read("COURSE_A/DOCK.RAW").unwrap();
    let dec = raw::decode(&file, &raw::LoadEnv::from_exe(&exe, raw::DOCK_COURSE).unwrap()).unwrap();
    Some((t, hud::slot_attrs(&dec.slots)))
}

#[test]
fn timer_format() {
    assert_eq!(hud::timer_text(3410), "0:11.3");
    assert_eq!(hud::timer_text(45300), "2:31.0");
    assert_eq!(hud::timer_text(-5), "0:00.0");
}

#[test]
fn tables_from_exe() {
    let Some((t, _)) = tables() else { return };
    assert_eq!(t.lap, "LAP");
    assert_eq!(t.suffixes, ["ST", "ND", "RD", "TH", "TH", "TH"]);
    assert_eq!(t.colours[4], 8);
    assert_eq!(t.colours[5], 9);
    assert_eq!(t.colours[7], 0xB);
    // '6' in the 24 px font, '0' in the 16 px font [RAM: GP0 packets]
    let g = t.glyphs[(b'6' as usize) + 0x60];
    assert_eq!((g.u, g.v, g.w), (74, 224, 21));
    let g = t.glyphs[(b'0' as usize) + 0x20];
    assert_eq!((g.u, g.v, g.w), (0, 48, 15));
    assert_eq!(t.copy_the_buttons, "COPY THE BUTTONS");
}

type P = (i32, i32, i32, i32, u8, u8, u16, u16, bool, bool);

#[test]
fn race_hud_matches_game_frame() {
    let Some((t, slots)) = tables() else { return };
    // oracle frame 45910: lap 0 of 3, 6th with +0x4C1 set, clock 45300
    let st = HudState { race_time: 45300, position: 6, position_flag: 1, lap: 0, ..HudState::default() };
    let got: Vec<P> = t
        .layout(&st)
        .iter()
        .map(|s| {
            let (tp, cl) = s.attrs(&slots);
            (s.x, s.y, s.w, s.h, s.u, s.v, tp & 0x1FF, cl, s.raw, s.semi)
        })
        .collect();
    let want: Vec<P> = vec![
        (245, 20, 21, 24, 74, 224, 0x008, 0x3EFB, true, true),
        (261, 21, 12, 12, 230, 36, 0x008, 0x3EF8, true, true),
        (272, 21, 13, 12, 192, 12, 0x008, 0x3EF8, true, true),
        (30, 199, 11, 12, 240, 12, 0x068, 0x3EF9, true, false),
        (40, 199, 14, 12, 178, 0, 0x068, 0x3EF9, true, false),
        (53, 199, 12, 12, 232, 24, 0x068, 0x3EF9, true, false),
        (29, 209, 15, 16, 16, 48, 0x068, 0x3EF8, true, false),
        (42, 209, 11, 16, 116, 64, 0x068, 0x3EF8, true, false),
        (51, 209, 15, 16, 48, 48, 0x068, 0x3EF8, true, false),
        (223, 202, 15, 16, 32, 48, 0x068, 0x3EF8, true, false),
        (236, 202, 8, 16, 142, 32, 0x068, 0x3EF8, true, false),
        (242, 202, 15, 16, 48, 48, 0x068, 0x3EF8, true, false),
        (255, 202, 15, 16, 16, 48, 0x068, 0x3EF8, true, false),
        (268, 202, 8, 16, 78, 112, 0x068, 0x3EF8, true, false),
        (274, 202, 15, 16, 0, 48, 0x068, 0x3EF8, true, false),
    ];
    assert_eq!(got, want);
}

#[test]
fn countdown_phases() {
    let Some((t, _)) = tables() else { return };
    let base = HudState { countdown_start: 300, ..HudState::default() };
    // intro: "RACE 3 LAPS", no race HUD, no timer
    let s = t.layout(&HudState { countdown: 300, ..base });
    assert_eq!(s.len(), "RACE3LAPS".len());
    // copy the buttons + minigame (4 icons + count)
    let m = hud::Minigame { button: 2, count: 3, flicker: 0 };
    let s = t.layout(&HudState { countdown: 200, minigame: Some(m), ..base });
    assert_eq!(s.len(), "COPYTHEBUTTONS".len() + 4 + 1);
    assert_eq!(s.iter().filter(|p| p.slot == hud::BUTTON_SLOT && p.rgb == [0x80; 3]).count(), 1);
    // READY flashes: shown when countdown % 30 >= 15 below 0x78
    assert_eq!(t.layout(&HudState { countdown: 0x77, ..base }).len(), 5);
    assert_eq!(t.layout(&HudState { countdown: 0x5A, ..base }).len(), 0);
    // text width/centring of the small font
    let lap = t.text(b"LAP", 0x30, 0xD2, 4, Size::Small, 5, 0x60);
    assert_eq!((lap[0].x, lap[0].y), (30, 199));
}

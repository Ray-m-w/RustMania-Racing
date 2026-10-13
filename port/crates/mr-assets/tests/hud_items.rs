//! Item HUD tests (Session 4, track B). Disc-backed (skip without the disc).
//! The expected primitives are the running game's GP0 packets of one frame
//! (oracle race3 with items on, VBlank 3800 after the grid savestate: the
//! "-1 HIT" popup, both weapon slots, the turbo meter at 252/420, the
//! direction arrow, the minimap with six karts), matched one for one by
//! tools/oracle/hudx_check.py (docs/spec/hud.md "Item HUD").

use mr_assets::hud::{self, HudState, HudTables};
use mr_assets::hud_items::{self, HudPulse, ItemHud, ItemTables, MapKart, Prim};
use mr_assets::raw;

fn setup() -> Option<(HudTables, ItemTables, [(u16, u16); 32])> {
    let mut d = match mr_disc::Disc::open_default() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("SKIP: disc not available ({e:?})");
            return None;
        }
    };
    let exe = d.read("SLUS_012.37").unwrap();
    let ht = HudTables::from_exe(&exe).unwrap();
    let it = ItemTables::from_exe(&exe).unwrap();
    let file = d.read("COURSE_A/DOCK.RAW").unwrap();
    let dec = raw::decode(&file, &raw::LoadEnv::from_exe(&exe, raw::DOCK_COURSE).unwrap()).unwrap();
    Some((ht, it, hud::slot_attrs(&dec.slots)))
}

fn line(p: &Prim, slots: &[(u16, u16); 32]) -> String {
    match *p {
        Prim::Sprt(s) => {
            let (tp, cl) = s.attrs(slots);
            format!("S {} {} {} {} {} {} {:#06x} {:#06x} {} {}", s.x, s.y, s.w, s.h, s.u, s.v, tp, cl, s.raw as u8, s.semi as u8)
        }
        Prim::Ft4(q) => {
            let (tp, cl) = slots[q.slot as usize];
            let tp = (tp & !0x60) | ((q.abr as u16) << 5);
            let v: Vec<String> = q.v.iter().flat_map(|p| [p[0].to_string(), p[1].to_string()]).collect();
            let uv: Vec<String> = q.uv.iter().flat_map(|p| [p[0].to_string(), p[1].to_string()]).collect();
            format!("Q {} {} {:#06x} {:#06x} {}", v.join(" "), uv.join(" "), tp, cl.wrapping_add(q.clut_off), q.semi as u8)
        }
        Prim::F3 { v, rgb } => format!("T {} {} {} {} {} {} {} {} {}", v[0][0], v[0][1], v[1][0], v[1][1], v[2][0], v[2][1], rgb[0], rgb[1], rgb[2]),
        Prim::Line { v, rgb } => format!("L {} {} {} {} {} {} {}", v[0][0], v[0][1], v[1][0], v[1][1], rgb[0], rgb[1], rgb[2]),
    }
}

/// Oracle race3, VBlank 3800 (frame built from the RAM row of that VBlank;
/// frame counter / clock / pulse of the frame before, see mr-hudx).
fn state_3800() -> (HudState, ItemHud) {
    let mut kart = vec![0u8; 0x504];
    for (o, v) in [
        (0xE, 0x74), (0xF, 0x06), (0x444, 0xFF), (0x445, 0xFF), (0x468, 0x2E), (0x469, 0x01), (0x47E, 0x60), (0x48A, 0xFC), (0x48E, 0x33),
        (0x48F, 0x03), (0x496, 0x02), (0x4BF, 0x01), (0x4CA, 0x04), (0x4D0, 0x02), (0x4D1, 0x02), (0x4D2, 0x02), (0x4D3, 0x02), (0x4D4, 0x40),
        (0x4D5, 0x40), (0x4F1, 0xFF), (0x4FC, 0x0F),
    ] {
        kart[o] = v;
    }
    let st = HudState { countdown: 0, countdown_start: 300, mode: 1, laps: 3, race_time: 20415, ..HudState::default() }.with_kart(&kart);
    let karts = vec![
        MapKart { x: 867378, z: 831552, heading: 1652, flags: 2, character: 0 },
        MapKart { x: 939646, z: 534838, heading: 3534, flags: 1, character: 21 },
        MapKart { x: 1064337, z: 906466, heading: 4018, flags: 3, character: 14 },
        MapKart { x: -218181, z: 423576, heading: 2464, flags: 5, character: 23 },
        MapKart { x: 1103851, z: 902756, heading: 3511, flags: 1, character: 3 },
        MapKart { x: 890125, z: 500392, heading: 4084, flags: 1, character: 10 },
    ];
    let it = ItemHud {
        frame: 4564,
        pulse: HudPulse([22, 22]),
        race_time: 20415,
        countdown: 0,
        mode: 1,
        course: 3,
        e38: 4,
        special: (-1, 300, 5),
        cam: [[3595, 559, -1878], [-721, 4027, -180], [1822, 489, 3634]],
        kart,
        karts,
        players: 1,
    };
    (st, it)
}

#[test]
fn race_hud_with_items_matches_game_frame() {
    let Some((ht, it, slots)) = setup() else { return };
    let (st, s) = state_3800();
    let got: Vec<String> = hud_items::race_hud(&ht, &it, &st, &s).iter().map(|p| line(p, &slots)).collect();
    // The game's primitives of that frame (GP0, draw offset removed), in add order.
    let want = [
        // direction arrow (FUN_80032a40)
        "L 270 53 283 63 255 255 255", "L 283 63 269 60 255 255 255", "L 269 60 243 76 255 255 255", "L 243 76 226 66 255 255 255",
        "L 226 66 253 56 255 255 255", "L 253 56 246 54 255 255 255", "L 246 54 270 53 255 255 255",
        // "-1 HIT" (FUN_80049440, +0x4FC = 15, +0x444 = -1)
        "S 209 168 16 24 176 176 0x0068 0x3efb 1 0", "S 221 168 19 24 76 200 0x0068 0x3efb 1 0", "S 248 168 25 24 178 128 0x0068 0x3efb 1 0",
        "S 269 168 14 24 204 128 0x0068 0x3efb 1 0", "S 279 168 23 24 228 152 0x0068 0x3efb 1 0",
        // place "4TH"
        "S 246 20 20 24 140 200 0x0068 0x3efb 1 0", "S 263 21 12 12 230 36 0x0068 0x3ef8 1 0", "S 274 21 13 12 192 12 0x0068 0x3ef8 1 0",
        // weapon slots (FUN_8004cbc8: type 2 animated icons)
        "Q 36 19 67 23 29 39 60 43 128 96 159 96 128 127 159 127 0x0067 0x3f37 0",
        "Q 36 47 67 51 29 67 60 71 128 96 159 96 128 127 159 127 0x0067 0x3f37 0",
        // LAP 2/3
        "S 30 199 11 12 240 12 0x0068 0x3ef9 1 0", "S 40 199 14 12 178 0 0x0068 0x3ef9 1 0", "S 53 199 12 12 232 24 0x0068 0x3ef9 1 0",
        "S 29 209 15 16 32 48 0x0068 0x3ef8 1 0", "S 42 209 11 16 116 64 0x0068 0x3ef8 1 0", "S 51 209 15 16 48 48 0x0068 0x3ef8 1 0",
        // turbo meter (FUN_8004a310): thermometer back (additive), frame, fill 252/420
        "S 452 29 24 35 0 192 0x0027 0x3f3c 0 1", "S 447 29 34 37 24 192 0x0067 0x3f3c 0 0", "S 449 45 30 20 96 14 0x0069 0x3eb3 0 0",
        // minimap (FUN_80048a0c): six kart arrows, then the map
        "T 460 184 440 175 453 170 117 170 4", "T 445 180 463 185 454 190 231 40 8", "T 458 167 466 177 454 178 89 49 40",
        "T 396 194 402 183 411 188 89 65 49", "T 451 170 469 174 462 179 255 178 121", "T 452 178 458 188 446 189 215 215 215",
        "S 384 161 96 64 128 128 0x006c 0x31ba 0 0",
        // timer 1:08.0
        "S 223 202 15 16 16 48 0x0068 0x3ef8 1 0", "S 236 202 8 16 142 32 0x0068 0x3ef8 1 0", "S 242 202 15 16 0 48 0x0068 0x3ef8 1 0",
        "S 255 202 15 16 128 48 0x0068 0x3ef8 1 0", "S 268 202 8 16 78 112 0x0068 0x3ef8 1 0", "S 274 202 15 16 0 48 0x0068 0x3ef8 1 0",
    ];
    assert_eq!(got, want);
}

#[test]
fn pulse_follows_frame_counter() {
    let mut p = HudPulse([0x10, 0x1C]);
    // frame % 120 >= 60: targets swap, each moves by dt
    p.step(60, 2);
    assert_eq!(p.0, [0x12, 0x1A]);
    for f in (62..120).step_by(2) {
        p.step(f, 2);
    }
    assert_eq!(p.0, [0x1C, 0x10]);
    p.step(120, 3);
    assert_eq!(p.0, [0x19, 0x13]);
}

#[test]
fn quad_matches_fun_8002fd04() {
    let Some((_, it, _)) = setup() else { return };
    // unrotated: x offsets * 512/320, cull nothing
    let q = hud_items::quad(&it, 0x30, 0x1F, 11, 11, 0, 0xC0, 0x80, 0x37, 0x37, [0x80; 3], 0x15, 0xB, 0).unwrap();
    assert_eq!(q.v, [[31, 20], [65, 20], [31, 42], [65, 42]]);
    assert!(q.semi);
    // off-screen right: culled
    assert!(hud_items::quad(&it, 560, 0x22, 10, 10, 0, 0, 0, 1, 1, [0x80; 3], 0x15, 0, 0x60).is_none());
}

#[test]
fn rival_line_in_results() {
    let Some((ht, _, _)) = setup() else { return };
    assert_eq!((ht.you_beat.as_str(), ht.beat_you.as_str(), ht.names[1].as_str()), ("YOU BEAT", "BEAT YOU", "GONZO"));
    let st = HudState { finished: true, lap: 3, laps: 3, ..HudState::default() };
    let base = hud::Results { best_lap: 11000, total_time: 33000, place: 1, ..hud::Results::default() };
    let n0 = ht.results(&st, &base).len();
    // "YOU BEAT GONZO": 12 glyphs at y 0x1E - 8
    let won = ht.results(&st, &hud::Results { rival: Some(1), ..base });
    assert_eq!(won.len(), n0 + 12);
    assert!(won[n0..].iter().all(|s| s.y == 0x1E - 8 && s.h == 16));
    // blinking off
    assert_eq!(ht.results(&st, &hud::Results { rival: Some(1), rival_blink: 50, ..base }).len(), n0);
    // lost: "GONZO BEAT YOU" (12 glyphs again)
    let n2 = ht.results(&st, &hud::Results { place: 2, ..base }).len();
    assert_eq!(ht.results(&st, &hud::Results { rival: Some(1), place: 2, ..base }).len(), n2 + 12);
}

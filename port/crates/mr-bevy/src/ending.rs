//! Phase 8: THE END (docs/spec/frontend.md "THE END"), the game's credits
//! and secret screen.
//!
//! The game: OPTIONS row 4 "THE END" (shown once 0x800E02B9, the secret
//! courses' bits, is set) loads course 0x1D (`frontend\the_end`: the
//! chequered disc with Kermit's kart) in game mode 0x15 with no karts
//! (FUN_8008ACE4 / FUN_8008C4AC), the orbit camera (FUN_80070418: mode 1)
//! and the music of a random open course (FUN_80089B98). Sub-state
//! 0x800B0F60 = 0: the credits (the table 0x800AB488: {text, size}, 228
//! lines) scroll up one pixel a frame from y 0xF0, tinted by height, then
//! "PRESS START"; START (the confirm while 0x800B0F60 = 0) opens sub-state
//! 1: the six secret courses (SPEEDWAY, RALLY, GLACIER while DISCO is won,
//! bit 0; VOLCANO, HAUNTED HOUSE, WHATEVER once everything is complete, bit
//! 1; "?" otherwise; the cursor row " - RACE" / " - BATTLE") and BACK TO
//! MAIN MENU [CODE 80074a94, 80051698 case 0x15][SHOT analysis/s8/end].
//! A course loads its course menu (sub-states 0xB / 0xC, whose BACK TO
//! CREDITS comes back here); BACK TO MAIN MENU the title.
//!
//! The port: `--ending` runs this course (courses.rs) with the course-menu
//! machinery (the session's camera only, `menu_tick`; no karts drawn) and
//! this module's 2D layer and input.

use bevy::prelude::*;
use mr_assets::hud::{self as ahud, Sprite};
use mr_assets::hud_items::Prim;

/// Course 0x1D, `frontend\the_end`.
pub const COURSE: u8 = 0x1D;
/// Game mode 0x15.
pub const MODE: i16 = 0x15;
/// The credits table: 8-byte rows {text pointer (0 = blank line), size,
/// -, -, last}.
pub const CREDITS: u32 = 0x800A_B488;
/// The secret courses' names (2 groups x 4 pointers, 3 used each).
pub const NAMES: u32 = 0x800A_B3D4;
/// The courses the six rows load (FUN_80074A94, course 0x1D).
pub const ROW_COURSE: [u8; 6] = [0x00, 0x20, 0x21, 0x22, 0x23, 0x24];

/// This run is THE END.
pub fn on() -> bool {
    crate::courses::id() == COURSE as u32
}

/// The `--ending` argument (courses.rs picks course 0x1D).
pub fn asked() -> bool {
    std::env::args().any(|a| a == "--ending")
}

/// One credits line.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub text: Option<String>,
    pub size: u8,
}

/// The game's tables, read from the executable.
pub struct Tables {
    pub lines: Vec<Line>,
    pub names: [String; 6],
}

fn exe_str(exe: &mr_sim::tables::ExeImage, a: u32) -> String {
    let mut s = String::new();
    let mut p = a;
    while exe.contains(p) && exe.u8(p) != 0 && s.len() < 64 {
        s.push(exe.u8(p) as char);
        p += 1;
    }
    s
}

impl Tables {
    pub fn from_exe(exe: &mr_sim::tables::ExeImage) -> Tables {
        let mut lines = Vec::new();
        let mut a = CREDITS;
        loop {
            let p = exe.i32(a) as u32;
            let text = (p != 0).then(|| exe_str(exe, p));
            lines.push(Line { text, size: exe.u8(a + 4) });
            if exe.u8(a + 7) != 0 || lines.len() > 1000 {
                break;
            }
            a += 8;
        }
        let names = std::array::from_fn(|i| exe_str(exe, exe.i32(NAMES + 4 * ((i / 3) * 4 + i % 3) as u32) as u32));
        Tables { lines, names }
    }

    /// A line's advance: 0xC + 3 (size 2), 0x18 + 3 (size 1), 0x10 + 3.
    fn step(size: u8) -> i32 {
        3 + match size {
            2 => 0xC,
            1 => 0x18,
            _ => 0x10,
        }
    }

    /// 0x800B0622: the credits' height.
    pub fn height(&self) -> i32 {
        self.lines.iter().map(|l| Self::step(l.size)).sum()
    }
}

pub fn tables() -> &'static Tables {
    static T: std::sync::OnceLock<Tables> = std::sync::OnceLock::new();
    T.get_or_init(|| Tables::from_exe(crate::courses::exe()))
}

/// FUN_80051698 case 0x15, the credits for frame counter `c` (0x800B0D90):
/// scrolling while c < 2 (height + 0xF0), y = 0xF0 - c / 2 for the first
/// line; each line FUN_800474E8(text, 0x100, y, 1, size, 0, abr) with the
/// blend colour v = clamp(y * 256 / 0xF0, 0, 255): size 0 (v, 0x60, !v),
/// size 1 (!v, 0x30, v), size 2 (v, 0x90, !v); abr 0x60 in the credits,
/// 0 (semi-transparent) behind the menu. "?" lines are the picture of a
/// complete game (0x800E02B9 = 3: FUN_8002FD04, not ported: the line stays
/// blank) or the text.
pub fn credits_sprites(t: &ahud::HudTables, tb: &Tables, c: i32, menu: bool, complete: bool) -> Vec<Sprite> {
    let mut out = Vec::new();
    if c / ((tb.height() + 0xF0) * 2) != 0 {
        return out;
    }
    let mut y = 0xF0 - c / 2;
    for l in &tb.lines {
        if let Some(s) = &l.text {
            if !(s == "?" && complete) {
                let v = ((y << 8) / 0xF0).clamp(0, 255) as u8;
                let rgb = match l.size {
                    0 => [v, 0x60, !v],
                    1 => [!v, 0x30, v],
                    _ => [v, 0x90, !v],
                };
                if (-0x20..0x100).contains(&y) {
                    out.extend(t.text_rgb(s.as_bytes(), 0x100, y, 1, ahud::Size::from_arg(l.size), 0, if menu { 0 } else { 0x60 }, rgb));
                }
            }
        }
        y += Tables::step(l.size);
    }
    out
}

/// After the scroll (c >= (height + 0x10E) / 30 * 60): "PRESS START" while
/// c % 60 < 45.
pub fn press_start(t: &ahud::HudTables, tb: &Tables, c: i32) -> Vec<Sprite> {
    let scrolling = c / ((tb.height() + 0xF0) * 2) == 0;
    if !scrolling && c >= (tb.height() + 0x10E) / 30 * 60 && c % 60 < 45 {
        t.text(b"PRESS START", 0x100, 0xD2, 0, ahud::Size::from_arg(0), 4, 0x60)
    } else {
        Vec::new()
    }
}

/// Sub-state 1's rows: the six courses ("?" while their group is locked;
/// the cursor's with " - RACE" / " - BATTLE") and BACK TO MAIN MENU.
pub fn menu_items(tb: &Tables, secrets: u8, sel: i32, back_to_main: &str) -> Vec<crate::menus::Item> {
    let mut v = Vec::new();
    for i in 0..6 {
        let open = secrets & if i < 3 { 1 } else { 2 } != 0;
        let text = if !open {
            "?".to_string()
        } else if i as i32 == sel {
            format!("{} - {}", tb.names[i], if i < 3 { "RACE" } else { "BATTLE" })
        } else {
            tb.names[i].clone()
        };
        v.push(crate::menus::Item::new(text));
    }
    v.push(crate::menus::Item::new(back_to_main).tint([0xFF, 0x1F, 0x1F]));
    v
}

#[derive(Resource, Default)]
struct EndRes {
    /// 0x800B0F60.
    sub: u8,
    row: i32,
    prev: u16,
    done: bool,
}

pub struct EndingPlugin;

impl Plugin for EndingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EndRes>().add_systems(Update, run.run_if(on));
    }
}

fn secrets() -> u8 {
    crate::save::with(|s| s.block.secrets())
}

fn run(p: Res<crate::Player>, latch: Res<crate::pacing::PadLatch>, front: Res<crate::menus::Front>, mut ov: ResMut<crate::title::TitleOverlay>, mut res: ResMut<EndRes>, mut sfx: ResMut<crate::sfx::Sfx>, mut exit: MessageWriter<AppExit>) {
    let crate::Driver::Sim(run) = &p.driver else { return };
    let c = run.session.globals.frame;
    let mt = crate::menus::tables();
    let tb = tables();
    let sec = secrets();
    // input (FUN_80073138 / FUN_80074A94 for course 0x1D)
    let b = latch.now;
    let pressed = |m: u16| b & m != 0 && res.prev & m == 0;
    let (up, down, cross, back, start) = (pressed(crate::input::UP), pressed(crate::input::DOWN), pressed(crate::input::CROSS), pressed(crate::input::TRIANGLE), pressed(crate::input::START));
    res.prev = b;
    if !res.done {
        if res.sub == 0 {
            if start {
                sfx.play(crate::title::CONFIRM_SFX);
                res.sub = 1;
            }
        } else if back {
            sfx.play(crate::title::CONFIRM_SFX);
            res.sub = 0;
        } else if up || down {
            let r = (res.row + down as i32 - up as i32).clamp(0, 6);
            if r != res.row {
                sfx.play((0xE, 0x1800, 0x40, 0x40));
                res.row = r;
            }
        } else if cross {
            sfx.play(crate::title::CONFIRM_SFX);
            let r = res.row as usize;
            if r == 6 {
                res.done = true;
                crate::menus::relaunch(None, front.character, front.vehicle, false);
                exit.write(AppExit::Success);
            } else if sec & if r < 3 { 1 } else { 2 } != 0 {
                let id = ROW_COURSE[r];
                if let Some(i) = crate::courses::races().iter().find(|i| i.id == id) {
                    info!("the end: {} -> its course menu", i.name);
                    res.done = true;
                    crate::menus::relaunch(Some(&i.name), front.character, front.vehicle, false);
                    exit.write(AppExit::Success);
                }
            }
        }
    }
    // the 2D layer
    let mut sp = credits_sprites(&mt.hud, tb, c, res.sub == 1, sec == 3);
    if res.sub == 0 {
        sp.extend(press_start(&mt.hud, tb, c));
    } else {
        sp.extend(crate::menus::back_button(mt, 0x100, 0x1E, 0, false));
        sp.extend(crate::menus::select_button(mt, 0x100, 0xD2, 0, false));
        let list = menu_items(tb, sec, res.row, &crate::menus::back_to_main_menu(mt));
        // the menu is drawn before (over) the credits
        let mut m = crate::menus::items(mt, &list, res.row, 0x78, front.vb);
        m.extend(sp);
        sp = m;
    }
    let want = Some(sp.into_iter().map(Prim::Sprt).collect::<Vec<_>>());
    if ov.0 != want {
        ov.0 = want;
    }
}

/// Restart on THE END (BACK TO CREDITS, OPTIONS -> THE END).
pub fn relaunch(character: u8, vehicle: u8) {
    let a: Vec<String> = std::env::args().collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 1;
    while i < a.len() {
        match a[i].as_str() {
            "--course" | "--character" | "--vehicle" => i += 2,
            "--skip-title" | "--menu" | "--league" | "--demo" | "--ending" => i += 1,
            _ => {
                out.push(a[i].clone());
                i += 1;
            }
        }
    }
    out.extend(["--ending".into(), "--skip-title".into(), "--character".into(), character.to_string(), "--vehicle".into(), vehicle.to_string()]);
    match std::env::current_exe() {
        Ok(exe) => {
            if let Err(e) = std::process::Command::new(exe).args(&out).spawn() {
                eprintln!("ending: relaunch failed: {e}");
            }
        }
        Err(e) => eprintln!("ending: relaunch failed: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credits_table() {
        let Ok(mut disc) = mr_disc::Disc::open_default() else { return };
        let exe = mr_sim::tables::ExeImage::parse(&disc.read("SLUS_012.37").unwrap()).unwrap();
        let tb = Tables::from_exe(&exe);
        assert_eq!(tb.lines.len(), 228);
        assert_eq!(tb.lines[0], Line { text: Some("THE END".into()), size: 1 });
        assert_eq!(tb.lines[8].text.as_deref(), Some("MUPPET RACEMANIA"));
        assert_eq!(tb.names[0], "SPEEDWAY");
        assert_eq!(tb.names[3], "VOLCANO");
        assert_eq!(tb.names[4], "HAUNTED HOUSE");
    }

    type Glyph = (i32, i32, u8, u8, i32, i32, u16, bool, [u8; 3]);

    /// The oracle's font glyphs (SPRT 0x64..0x67 on the font CLUT rows) of a
    /// GP0 frame JSON (tools/oracle/menu_walk.py `+gp0`).
    fn oracle_glyphs(json: &str, font_clut: u16) -> Vec<Glyph> {
        let mut out = Vec::new();
        for chunk in json.split("\"words\": [").skip(1) {
            let words: Vec<u32> = chunk.split(']').next().unwrap_or("").split(',').filter_map(|w| u32::from_str_radix(w.trim().trim_matches('"').trim_start_matches("0x"), 16).ok()).collect();
            if words.len() != 4 || !(0x64..=0x67).contains(&(words[0] >> 24)) {
                continue;
            }
            let op = words[0] >> 24;
            let rgb = [(words[0] & 0xFF) as u8, ((words[0] >> 8) & 0xFF) as u8, ((words[0] >> 16) & 0xFF) as u8];
            let (x, y) = ((words[1] & 0xFFFF) as i16 as i32, (words[1] >> 16) as i16 as i32);
            let (u, v, clut) = ((words[2] & 0xFF) as u8, ((words[2] >> 8) & 0xFF) as u8, (words[2] >> 16) as u16);
            let (w, h) = ((words[3] & 0xFFFF) as i32, (words[3] >> 16) as i32);
            if clut & 0xFFC0 != font_clut & 0xFFC0 || w > 32 || h > 32 {
                continue;
            }
            out.push((x, y, u, v, w, h, clut, op & 1 != 0, if op & 1 != 0 { [0x80; 3] } else { rgb }));
        }
        out
    }

    /// The credits against the oracle's GP0 frame (analysis/s8/end/end2,
    /// THE END from the title with 0x800E02B9 poked to 3; local): every
    /// glyph (position, UV, CLUT, blend colour) at the best frame counter.
    #[test]
    fn gp0_credits() {
        let root = mr_sim::oracle::repo_root().join("analysis/s8/end");
        let Ok(text) = std::fs::read_to_string(root.join("end2.json")) else { return };
        let Ok(mut disc) = mr_disc::Disc::open_default() else { return };
        let exe_b = disc.read("SLUS_012.37").unwrap();
        let exe = mr_sim::tables::ExeImage::parse(&exe_b).unwrap();
        let tb = Tables::from_exe(&exe);
        let t = ahud::HudTables::from_exe(&exe_b).unwrap();
        let path = mr_assets::raw::course_raw_path(&exe_b, COURSE as u32).unwrap();
        let env = mr_assets::raw::LoadEnv::from_exe(&exe_b, COURSE as u32).unwrap();
        let dec = mr_assets::raw::decode(&disc.read(&path).unwrap(), &env).unwrap();
        let slots = ahud::slot_attrs(&dec.slots);
        let theirs = oracle_glyphs(&text, slots[ahud::FONT_SLOT as usize].1);
        let mut best = (usize::MAX, 0);
        for c in (800..1000).step_by(2) {
            let ours: Vec<Glyph> = credits_sprites(&t, &tb, c, false, true).iter().filter(|s| s.slot == ahud::FONT_SLOT).map(|s| (s.x, s.y, s.u, s.v, s.w, s.h, s.attrs(&slots).1, s.raw, if s.raw { [0x80; 3] } else { s.rgb })).collect();
            let bad = ours.iter().filter(|o| !theirs.contains(o)).count() + theirs.iter().filter(|o| !ours.contains(o)).count();
            if bad < best.0 {
                best = (bad, c);
            }
        }
        println!("credits: {} oracle glyphs, best frame {} ({} differ)", theirs.len(), best.1, best.0);
        assert!(theirs.len() > 50, "{} oracle glyphs", theirs.len());
        assert_eq!(best.0, 0, "frame {}: {} glyphs differ of {}", best.1, best.0, theirs.len());
    }
}

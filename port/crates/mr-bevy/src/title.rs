//! The title screen (Session 6, track T): the game's first interactive
//! screen. docs/spec/frontend.md has the code and data.
//!
//! What the game does: the title is the frontend course 0x1E
//! (`frontend\options`) run by the race engine in game mode 0x10 (FUN_8008ACE4,
//! sub-state 0x800B0E8A = 0): the OPTIONS scene (the orange disc with the 3D
//! "MUPPET RaceMania" logo, chequered floor, rings of fruit tokens and
//! item-box stars), three AI karts driving round the disc and knocking the
//! logo's letters about, the orbit camera (mode 1) along DAT chunks 0x0B /
//! 0x0A, the title music (course row 30: track 0x18), a fade in from black,
//! then "PRESS START".
//!
//! The port does exactly that with its race machinery: on the title the run's
//! course (courses.rs) is 0x1E, the session is `SessionConfig::title` (mr-sim
//! title.rs: the karts, the knock-out re-creation, the letter bounce, the
//! orbit camera), and the race's scene, karts, effects, animated textures,
//! sounds and music draw it. This module adds the title's own 2D layer and
//! input:
//! - the fade (FUN_8007D174(0x80, 8) at the load, FUN_8007D1C0 per frame:
//!   level +8 per VBlank to 0x100; FUN_8002D1E8 draws a full-screen B - F
//!   quad of (0x80 - level / 2) * 2): a B - F primitive of the effects pass;
//! - "PRESS START" (FUN_800474E8(0x800B0284, 0x100, 0xD2, 0, 0, 4, 0x60)) once
//!   the frame counter 0x800B0D90 passes 599, shown while counter % 60 < 45,
//!   and only once the fade is done (0x800B0F88) [CODE 80051698 @0x8005A3A8];
//! - the race HUD stays off (the overlay replaces it);
//! - START (pressed, fade done; FUN_80073138) plays the confirm sound 0xF
//!   (FUN_8005EECC(0xF, 0x1800, 0x60, 0x60)) and opens the game's main menu
//!   over the same scene (sub-state 1; menus.rs, Session 7).
//! After 1800 idle VBlanks the attract demo (demo.rs, phase 8). The cheat
//! codes (phase 8): face-button presses on the title go to
//! mr_sim::save::Cheats; a completed code changes the save
//! (`SaveBlock::cheat`, autosaved), plays sound 0x24 and shows its name
//! for 0x3C VBlanks (FUN_80051698 case 0: FUN_800474E8(name, 0x100, 0x1E,
//! 0, 1, 5 (RESET 7), 0x60)).

use bevy::prelude::*;
use mr_assets::hud::{self as ahud, HudTables, Sprite};
use mr_assets::hud_items::Prim;

/// Course index of the frontend scene (0x800B0FAC = 0x1E, `frontend\options`).
pub const TITLE_COURSE: u32 = 0x1E;
/// "PRESS START" appears after this many VBlanks (FUN_80051698 @0x8005A3A8:
/// 599 < 0x800B0D90).
pub const PRESS_START_AFTER: i32 = 600;
/// Blink: shown while the counter % 60 < 45.
pub const BLINK_PERIOD: i32 = 60;
pub const BLINK_ON: i32 = 45;
/// Fade speed (FUN_8007D174's second argument) and full level (2 * 0x80).
pub const FADE_SPEED: i32 = 8;
pub const FADE_FULL: i32 = 0x100;
/// The confirm sound (FUN_80073138 -> FUN_8005EECC(0xF, 0x1800, 0x60, 0x60)).
pub const CONFIRM_SFX: (i32, i32, i32, i32) = (0xF, 0x1800, 0x60, 0x60);
/// The game's draw distances on the frontend course (RAM during the title:
/// DAT_800B0CD0 = 5000 (FUN_80029F40: DAT_800B0EE4 < 0 -> 5000),
/// DAT_800B0DE8 = 4616, DAT_800B0B00 = 0x4AF - DAT_800A8840 (60)): the
/// title always uses the game's object selection (main.rs select_objects).
pub const TITLE_DRAW: mr_assets::dat::DrawParams = mr_assets::dat::DrawParams { cd0: 5000, de8: 4616, b00: 1139 };

/// Which screen the app starts on.
#[derive(States, Default, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Screen {
    #[default]
    Title,
    Race,
}

/// Start on the title unless `skip_title` (settings row / `--skip-title`) or
/// a tool flag that expects to start in the race.
pub fn first_screen() -> Screen {
    let a: Vec<String> = std::env::args().collect();
    let tool = ["--oracle-grid", "--route", "--replay", "--replay-capture", "--shot", "--fx-shots", "--latency-log", "--autopilot", "--record-route", "--trace-out", "--ghost"];
    if crate::settings::Settings::global().bool("skip_title", false) || a.iter().any(|x| tool.contains(&x.as_str())) {
        Screen::Race
    } else {
        Screen::Title
    }
}

/// True while the title runs (the course of this run is the frontend's).
pub fn on_title() -> bool {
    crate::courses::id() == TITLE_COURSE
}

/// The title session's rand() seed and random pointer: the oracle run's with
/// `--title-oracle`, else a fresh one each launch (the game's depends on
/// everything before the title).
pub fn session_seed() -> (u32, u32) {
    if std::env::args().any(|a| a == "--title-oracle") {
        return (mr_sim::title::ORACLE_SEED, mr_sim::session::RNG_START);
    }
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1);
    let seed = (n ^ (n >> 29)) as u32 | 1;
    (seed, mr_sim::session::RNG_START + seed % 0x5DB)
}

/// `--title-oracle`: the oracle run's title (BACK TO MAIN MENU from savestate
/// slot 1): its rand() seed (`session_seed`) and its water-animation state
/// (power-on history; mr_sim::title::ORACLE_WATER) give the same characters,
/// grid, token colours and camera.
pub fn prepare(s: &mut mr_sim::session::Session) {
    if on_title() && std::env::args().any(|a| a == "--title-oracle") {
        s.water = mr_sim::water::WaterAnim::from_bytes(&mr_sim::title::ORACLE_WATER);
    }
}

// ---------------------------------------------------------------- fade / text

/// FUN_8007D1C0's fade level (0x800AFED0) on the title's frame counter
/// 0x800B0D90: +8 per VBlank from the load (16 at the first frame, full at
/// counter 30; oracle: 0x800B0F88 = 1 from counter 32).
pub fn fade_level(counter: i32) -> i32 {
    ((counter + 2) * FADE_SPEED).clamp(0, FADE_FULL)
}

/// 0x800B0F88: the fade is done (the frame after the level arrived).
pub fn fade_done(counter: i32) -> bool {
    counter >= FADE_FULL / FADE_SPEED
}

/// FUN_8002D1E8: the B - F quad's colour for a level (0 = none).
pub fn fade_subtract(level: i32) -> u8 {
    let b = (level - (level >> 31)) >> 1;
    ((0x80 - b) * 2).clamp(0, 0xFF) as u8
}

/// The title's 2D layer for one counter value: "PRESS START" when due.
pub fn text_sprites(t: &HudTables, counter: i32, fade_done: bool) -> Vec<Sprite> {
    if counter >= PRESS_START_AFTER && counter % BLINK_PERIOD < BLINK_ON && fade_done {
        t.text(b"PRESS START", 0x100, 0xD2, 0, ahud::Size::from_arg(0), 4, 0x60)
    } else {
        Vec::new()
    }
}

// ---------------------------------------------------------------- plugin

/// The title's 2D primitives for the HUD overlay (courses.rs merges them
/// with the pick-list): Some (maybe empty) while the title shows, so the
/// race HUD stays off.
#[derive(Resource, Default)]
pub struct TitleOverlay(pub Option<Vec<Prim>>);

#[derive(Resource)]
struct TitleRes {
    hud: HudTables,
    prev_buttons: u16,
    shot_wait: u32,
    cheats: mr_sim::save::Cheats,
    /// The code names (0x800B0590 + 4 * (code - 1)).
    cheat_names: Vec<String>,
    /// 0x800B080F (VBlanks left) / 0x800B0811 (the code).
    cheat_shown: i32,
    cheat_code: u8,
    last_frame: i32,
}

/// The code names' pointers (FUN_80051698 @0x8005A50C): MUPPETS AND
/// VEHICLES, STUDIO, ARCHES, FRAGGLE ROCK, END COURSES, RESET.
pub const CHEAT_NAMES: [u32; 6] = [0x800B_0590, 0x800B_0594, 0x800B_0598, 0x800B_059C, 0x800B_05A0, 0x800B_05AC];

pub struct TitlePlugin;

impl Plugin for TitlePlugin {
    fn build(&self, app: &mut App) {
        app.insert_state(first_screen())
            .init_resource::<TitleOverlay>()
            .add_systems(Startup, setup)
            .add_systems(Update, (overlay, letters, shot.after(overlay)).run_if(|| on_title()));
    }
}

fn setup(mut commands: Commands) {
    if !on_title() {
        return;
    }
    let mut disc = mr_disc::Disc::open_default().expect("open the disc");
    let exe = disc.read("SLUS_012.37").expect("read the executable");
    let img = mr_sim::tables::ExeImage::parse(&exe).expect("exe");
    let name = |p: u32| {
        let mut a = img.i32(p) as u32;
        let mut s = String::new();
        while img.contains(a) && img.u8(a) != 0 && s.len() < 40 {
            s.push(img.u8(a) as char);
            a += 1;
        }
        s
    };
    let cheat_names = CHEAT_NAMES.iter().map(|&p| name(p)).collect();
    commands.insert_resource(TitleRes { hud: HudTables::from_exe(&exe).expect("HUD tables"), prev_buttons: 0, shot_wait: 0, cheats: mr_sim::save::Cheats::from_exe(&img), cheat_names, cheat_shown: 0, cheat_code: 0, last_frame: 0 });
}

/// The session's frame counter 0x800B0D90 (the title's clock).
fn counter(p: &crate::Player) -> i32 {
    match &p.driver {
        crate::Driver::Sim(run) => run.session.globals.frame,
        _ => 0,
    }
}

fn overlay(
    p: Res<crate::Player>,
    res: Option<ResMut<TitleRes>>,
    latch: Res<crate::pacing::PadLatch>,
    mut front: ResMut<crate::menus::Front>,
    mut ov: ResMut<TitleOverlay>,
    mut fx: ResMut<crate::psxfx::FxPrims>,
    mut sfx: ResMut<crate::sfx::Sfx>,
) {
    let Some(mut res) = res else { return };
    let c = counter(&p);
    let done = fade_done(c);
    // the fade: a full-screen B - F primitive over everything (HUD view)
    let f = fade_subtract(fade_level(c));
    if f != 0 {
        use crate::psxfx::{Blend, FxPrim, FxVert, VIEW_HUD};
        let k = f as f32 / 255.0;
        let q = [[-4096.0, 4096.0], [4096.0, 4096.0], [-4096.0, -4096.0], [4096.0, -4096.0]];
        fx.list.push(FxPrim {
            v: std::array::from_fn(|i| FxVert { pos: Vec3::new(q[i][0], q[i][1], 40.0), uv: [0.0, 0.0], rgb: [k, k, k] }),
            blend: Blend::Abr(2),
            view: VIEW_HUD,
            bucket: 100,
            always_depth: true,
            ..Default::default()
        });
    }
    // PRESS START (sub-state 0 only; the menus draw their own layer)
    let menu = front.page != crate::menus::Page::None;
    let mut sprites = if menu { Vec::new() } else { text_sprites(&res.hud, c, done) };
    // the cheat codes (sub-state 0): the face buttons, the code's name
    let step = (c - res.last_frame).max(0);
    res.last_frame = c;
    res.cheat_shown = (res.cheat_shown - step).max(0);
    if !menu && !crate::menus::tool_run() && !crate::movie::playing() {
        if let Some(code) = res.cheats.frame(latch.now) {
            crate::save::with(|s| {
                let t = s.tables.clone();
                s.block.cheat(code, &t);
            });
            crate::save::persist();
            sfx.play((0x24, 5000, 0x80, 0x80));
            info!("title: cheat code {code} ({})", res.cheat_names.get(code as usize - 1).cloned().unwrap_or_default());
            res.cheat_code = code;
            res.cheat_shown = 0x3C;
        }
        if res.cheat_shown > 0 {
            if let Some(n) = res.cheat_names.get(res.cheat_code as usize - 1) {
                let colour = if res.cheat_code == 6 { 7 } else { 5 };
                sprites.extend(res.hud.text(n.as_bytes(), 0x100, 0x1E, 0, ahud::Size::from_arg(1), colour, 0x60));
            }
        }
    } else {
        res.cheats.reset();
    }
    let want = Some(sprites.into_iter().map(Prim::Sprt).collect::<Vec<_>>());
    if ov.0 != want {
        ov.0 = want;
    }
    // START: pressed, fade done (FUN_80073138); `--title-press-start N` presses
    // it at counter N (tests of the hand-over without a pad)
    let auto = crate::settings::arg_value("--title-press-start").and_then(|v| v.parse::<i32>().ok()).is_some_and(|v| c >= v && c < v + 8);
    let b = latch.now | if auto { crate::input::START } else { 0 };
    if done && !menu && !crate::movie::playing() && b & crate::input::START != 0 && res.prev_buttons & crate::input::START == 0 {
        sfx.play(CONFIRM_SFX);
        info!("title: START -> main menu (counter {c})");
        front.open_main();
    }
    res.prev_buttons = b;
}

/// FUN_80091B8C's result (mr-sim title.rs `Letters`): the logo letters and
/// their shadows moved to their bounce height (FUN_8002DF88).
fn letters(p: Res<crate::Player>, mut q: Query<(&crate::TrackObject, &mut Transform)>) {
    let crate::Driver::Sim(run) = &p.driver else { return };
    let Some(d) = run.session.world.title.as_deref() else { return };
    let offs: std::collections::HashMap<usize, i32> = d.letters.lock().unwrap().offsets().into_iter().collect();
    for (o, mut t) in q.iter_mut() {
        if let Some(&dy) = offs.get(&o.0) {
            let want = crate::to_bevy(0, dy * 32, 0);
            if t.translation != want {
                t.translation = want;
            }
        }
    }
}

/// `--title-shot PATH --title-shot-at N`: freeze the title at frame counter
/// N (virtual time paused), capture the window, exit (oracle side-by-sides;
/// with `--title-oracle` the title is the oracle run's).
fn shot(p: Res<crate::Player>, res: Option<ResMut<TitleRes>>, mut time: ResMut<Time<Virtual>>, mut c: Commands, mut exit: MessageWriter<AppExit>) {
    let (Some(path), Some(at)) = (crate::settings::arg_value("--title-shot"), crate::settings::arg_value("--title-shot-at").and_then(|v| v.parse::<i32>().ok())) else { return };
    let Some(mut res) = res else { return };
    if counter(&p) < at && res.shot_wait == 0 {
        return;
    }
    time.pause();
    res.shot_wait += 1;
    if res.shot_wait == 8 {
        c.spawn(bevy::render::view::screenshot::Screenshot::primary_window())
            .observe(move |ev: On<bevy::render::view::screenshot::ScreenshotCaptured>| crate::write_png(&ev.image, &path));
    }
    if res.shot_wait == 40 {
        exit.write(AppExit::Success);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fade_in_takes_32_vblanks() {
        // oracle: level 16 on the first title frame, 0x800B0F88 from 32
        assert_eq!(fade_level(0), 16);
        assert_eq!(fade_level(30), FADE_FULL);
        assert!(!fade_done(30) && fade_done(32));
        assert_eq!(fade_subtract(0), 0xFF);
        assert_eq!(fade_subtract(FADE_FULL), 0);
    }

    /// The frontend RAW's texture slot table against the oracle's RAM
    /// 0x800DFF00 during the title (VB 1000 after BACK TO MAIN MENU from
    /// slot 1): (tpage, clut, x, y, mode) of the used slots.
    #[test]
    fn frontend_slots_match_ram() {
        let Ok(mut disc) = mr_disc::Disc::open_default() else { return };
        let exe = disc.read("SLUS_012.37").unwrap();
        let path = mr_assets::raw::course_raw_path(&exe, TITLE_COURSE).unwrap();
        let env = mr_assets::raw::LoadEnv::from_exe(&exe, TITLE_COURSE).unwrap();
        let dec = mr_assets::raw::decode(&disc.read(&path).unwrap(), &env).unwrap();
        let ram: [(usize, u16, u16, i16, i16, u16); 15] = [
            (0, 0x0, 0x3070, 0, 0, 0), (1, 0x81, 0x30B0, 64, 0, 1), (2, 0x3, 0x30F0, 192, 0, 0), (5, 0x4, 0x31B0, 256, 0, 0),
            (7, 0x8C, 0x3E30, 768, 0, 1), (8, 0x8D, 0x3DF0, 832, 0, 1), (9, 0x8E, 0x3DB0, 896, 0, 1), (14, 0x8F, 0x3D70, 960, 0, 1),
            (15, 0xA, 0x3EB0, 640, 0, 0), (16, 0x5, 0x3FF0, 320, 0, 0), (17, 0x6, 0x3FB0, 384, 0, 0), (18, 0x7, 0x3F70, 448, 0, 0),
            (19, 0x8, 0x3F30, 512, 0, 0), (20, 0xB, 0x3E70, 704, 0, 0), (21, 0x9, 0x3EF0, 576, 0, 0),
        ];
        for (i, tp, cl, x, y, mode) in ram {
            let s = dec.slots[i].unwrap_or_else(|| panic!("slot {i} missing"));
            assert_eq!((s.tpage.unwrap_or(0), s.clut, s.x as i16, s.y as i16, s.mode.unwrap_or(0)), (tp, cl, x, y, mode), "slot {i}");
        }
    }

    /// "PRESS START" by the HUD software renderer from the frontend VRAM
    /// (MR_TITLE_DUMP=DIR writes press_start.png, 512 x 240 game pixels, for
    /// tools/oracle/title_check.py).
    #[test]
    fn press_start_render() {
        let Ok(mut disc) = mr_disc::Disc::open_default() else { return };
        let exe = disc.read("SLUS_012.37").unwrap();
        let (v, _, _) = crate::scene::load_vram(&mut disc, &exe, TITLE_COURSE);
        let path = mr_assets::raw::course_raw_path(&exe, TITLE_COURSE).unwrap();
        let env = mr_assets::raw::LoadEnv::from_exe(&exe, TITLE_COURSE).unwrap();
        let dec = mr_assets::raw::decode(&disc.read(&path).unwrap(), &env).unwrap();
        let t = HudTables::from_exe(&exe).unwrap();
        let sp = text_sprites(&t, 600, true);
        assert_eq!(sp.len(), 10, "one sprite per glyph");
        assert!(text_sprites(&t, 598, true).is_empty() && text_sprites(&t, 650, true).is_empty() && text_sprites(&t, 600, false).is_empty());
        let mut canvas = vec![[0u8; 4]; 512 * 240];
        ahud::render(&v, &ahud::slot_attrs(&dec.slots), &sp, &mut canvas, 512, 240);
        if let Ok(dir) = std::env::var("MR_TITLE_DUMP") {
            let flat: Vec<u8> = canvas.iter().flatten().copied().collect();
            std::fs::write(format!("{dir}/press_start.png"), mr_assets::img::png_rgba(512, 240, &flat)).unwrap();
        }
    }

    #[test]
    fn blink_schedule() {
        // shown 45 of every 60 VBlanks from VBlank 600
        let on = |c: i32| c >= PRESS_START_AFTER && c % BLINK_PERIOD < BLINK_ON;
        assert!(!on(598));
        assert!(on(600));
        assert!(on(644));
        assert!(!on(646));
        assert!(on(660));
    }
}

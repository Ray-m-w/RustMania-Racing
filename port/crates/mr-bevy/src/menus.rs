//! The game's own front end for the one-player race flow (Session 7, phase 1;
//! docs/spec/frontend.md "Menus"). Replaces the Session 6 course pick-list.
//!
//! What the game does (FUN_80074A94 menu logic, FUN_80073138 menu input,
//! FUN_80051698 the 2D layer, FUN_800474E8 the font): every menu is text in
//! the HUD font over a running 3D scene, one item per row 0x10 apart centred
//! on y 0x78 (`item_y`), the cursor row (0x800B0E24) drawn medium-size and
//! pulsing white (colour 0, blend colour (sin >> 9) - 0x21), the others small
//! in colour 1, "t BACK" at the top and "x SELECT" at the bottom
//! (FUN_80049C70 / FUN_80049AC0), red / green flat-triangle arrows
//! (FUN_800307D4), navigation sound 0xE and confirm sound 0xF.
//!
//! Screens (the port's `Page`), where they run and what they lead to:
//! - title (course 0x1E, game mode 0x10): PRESS START (title.rs) -> `Main`
//!   (sub-state 1): MEET THE MUPPETS, GAME STATUS, SELECT COURSE,
//!   TOURNAMENT - 1 PLAYER, OPTIONS;
//!   - SELECT COURSE -> `CourseSelect` (game mode 0xC: the film-strip worlds;
//!     the camera is mode 5, mr-sim camera.rs): UP / DOWN world, LEFT /
//!     RIGHT course; CROSS on a race course loads it (the port restarts the
//!     program on that course with `--menu`: the scene is built at start-up);
//!   - OPTIONS -> `Options` (sub-state 3): ADJUST SOUND (`Sound`), SET
//!     CONTROLLERS (`Controllers`), ADJUST SCREEN, MEMORY CARD, and the
//!     port's own PORT SETTINGS (`PortSettings`, settings.txt);
//! - a loaded course (game mode 0): `CourseMenu` (sub-state 0x800B0EA2 = 0:
//!   1 PLAYER RACE - n LAPS, 2 PLAYER RACE, <COURSE> SECRETS, GAME MENU,
//!   BACK TO MAIN MENU), `GameMenu` (ADVENTURE, GAME STATUS, CHANGE COURSE,
//!   TOURNAMENT), `WorldList` / `CourseList` (CHANGE COURSE: the text lists,
//!   another course restarts the program on it), `CharSelect` (0x800B07CB = 1:
//!   UP / DOWN character, LEFT / RIGHT vehicle, the kart on grid slot 0; CROSS
//!   starts the race), all over the course with the orbit camera (mode 1,
//!   `Session::menu_tick`);
//! - in a race: START -> `Pause` (CONTINUE, RETIRE, RESTART, ADJUST SOUND;
//!   RETIRE / RESTART ask YES / NO, `Confirm`); CROSS / START on the results -> the
//!   course menu (in a league: the standings, `Standings`);
//! - TOURNAMENT - 1 PLAYER -> `Leagues` (sub-state 2; league.rs): a league's
//!   courses one after another, the standings after each race.
//!
//! Not ported (shown, greyed with the game's own "unavailable" look, abr 0,
//! and not selectable): MEET THE MUPPETS, GAME STATUS, ADVENTURE, 2 PLAYER
//! RACE, SECRETS, ADJUST SCREEN, MEMORY CARD (the port autosaves), VIBRATION,
//! BATTLE LEAGUE, the course select's MOVIE / MUSIC, stunt and battle
//! courses.
//!
//! Progress: the save block (save.rs; `Progress`): characters, vehicles,
//! courses, leagues, lap records, laps. `unlock_all` (settings, default
//! false) shows everything open without changing the save.

use std::sync::OnceLock;

use bevy::prelude::*;
use mr_assets::hud::{HudTables, Size, Sprite};
use mr_assets::hud_items::Prim;

use crate::input::{CROSS, DOWN, LEFT, RIGHT, START, TRIANGLE, UP};

// ---------------------------------------------------------------- exe tables

const EXE_BASE: u32 = 0x8001_0000;

fn exe_bytes(exe: &[u8], a: u32, n: usize) -> &[u8] {
    let o = (a - EXE_BASE) as usize + 0x800;
    &exe[o..o + n]
}

fn exe_u32(exe: &[u8], a: u32) -> u32 {
    u32::from_le_bytes(exe_bytes(exe, a, 4).try_into().unwrap())
}

fn exe_str(exe: &[u8], a: u32) -> String {
    let o = (a - EXE_BASE) as usize + 0x800;
    let n = exe[o..].iter().position(|&b| b == 0).unwrap_or(0);
    String::from_utf8_lossy(&exe[o..o + n]).into_owned()
}

/// A string through a pointer the code loads (PTR_s_... in the decomp).
fn exe_ptr_str(exe: &[u8], p: u32) -> String {
    exe_str(exe, exe_u32(exe, p))
}

/// Pointers to the menu strings (addresses of the PTR_s_ words).
mod sp {
    pub const MEET_THE_MUPPETS: u32 = 0x800B_03EC;
    pub const SELECT_COURSE: u32 = 0x800B_03F0;
    pub const CHANGE_COURSE: u32 = 0x800B_03F4;
    pub const GAME_STATUS: u32 = 0x800B_03F8;
    pub const OPTIONS: u32 = 0x800B_0404;
    pub const SET_CONTROLLERS: u32 = 0x800B_0408;
    pub const ADJUST_SCREEN: u32 = 0x800B_040C;
    pub const MEMORY_CARD: u32 = 0x800B_0410;
    pub const ONE_PLAYER: u32 = 0x800B_0414;
    pub const TOURNAMENT: u32 = 0x800B_03BC;
    pub const ADVENTURE: u32 = 0x800B_03C0;
    pub const ADJUST_SOUND: u32 = 0x800B_02EC;
    pub const BACK: u32 = 0x800B_0170;
    pub const CANCEL: u32 = 0x800B_017C;
    pub const ACCEPT: u32 = 0x800B_014C;
    pub const SELECT: u32 = 0x800B_0158;
    pub const MUSIC: u32 = 0x800B_0554;
    pub const MOVIE: u32 = 0x800B_01C4;
    pub const RACE_A: u32 = 0x800B_056C;
    pub const RACE_B: u32 = 0x800B_0578;
    pub const STUNT: u32 = 0x800B_0560;
    pub const BATTLE: u32 = 0x800B_043C;
    pub const RACE: u32 = 0x800B_0424;
    pub const BATTLE_RACE: u32 = 0x800B_044C;
    pub const ONE_PLAYER_RACE: u32 = 0x800B_0358;
    pub const TWO_PLAYER_RACE: u32 = 0x800B_035C;
    pub const LAP: u32 = 0x800B_0200;
    pub const LAPS: u32 = 0x800B_020C;
    pub const SECRETS: u32 = 0x800B_0380;
    pub const GAME_MENU: u32 = 0x800B_0384;
    pub const BACK_TO_MAIN_MENU: u32 = 0x800B_05C0;
    pub const BACK_TO_CREDITS: u32 = 0x800B_05C4;
    pub const BEST_LAP: u32 = 0x800B_0354;
    pub const BEST_TIME: u32 = 0x800B_0350;
    pub const ONE_PLAYER_STUNT_TRIAL: u32 = 0x800B_0370;
    pub const TWO_PLAYER_STUNT_RACE: u32 = 0x800B_0374;
    pub const ONE_PLAYER_BATTLE: u32 = 0x800B_0360;
    pub const TWO_PLAYER_BATTLE: u32 = 0x800B_0364;
    pub const NOT_COLLECTED: u32 = 0x800B_05E8;
    pub const NOT_WON: u32 = 0x800B_05F4;
    pub const MUPPET: u32 = 0x800B_01D0;
    pub const VEHICLE: u32 = 0x800B_01DC;
    pub const PAUSED: u32 = 0x800B_02C4;
    pub const CONTINUE: u32 = 0x800B_02D0;
    pub const RETIRE: u32 = 0x800B_02DC;
    pub const RESTART: u32 = 0x800B_02E8;
    pub const YES: u32 = 0x800B_02F8;
    pub const NO: u32 = 0x800B_0300;
    pub const MUSIC_VOLUME: u32 = 0x800B_02C8;
    pub const SOUND_VOLUME: u32 = 0x800B_02CC;
    pub const CONTROLLER: u32 = 0x800B_0288;
    pub const JUMP: u32 = 0x800B_0294;
    pub const BRAKE: u32 = 0x800B_02A0;
    pub const CONFIGURATION: u32 = 0x800B_02A4;
    pub const VIBRATION: u32 = 0x800B_02A8;
    pub const RACE_LEAGUE_A: u32 = 0x800B_01E0;
    pub const RACE_LEAGUE_B: u32 = 0x800B_01E4;
    pub const BATTLE_LEAGUE: u32 = 0x800B_01F0;
    pub const RANDOM_RACE_LEAGUE: u32 = 0x800B_01F4;
    pub const RACE_POINTS: u32 = 0x800B_03D0;
    pub const TOURNAMENT_POINTS: u32 = 0x800B_03D8;
    pub const FINAL_TABLE: u32 = 0x800B_03DC;
    pub const AFTER: u32 = 0x800B_0460;
    pub const RACES: u32 = 0x800B_0430;
    pub const NEXT: u32 = 0x800B_0164;
    pub const START_RACE: u32 = 0x800B_03C8;
    pub const START_BATTLE: u32 = 0x800B_03CC;
    pub const POINTS: u32 = 0x800B_0310;
    pub const QUIT_TOURNAMENT: u32 = 0x800B_02F0;
    pub const P1: u32 = 0x800B_05CC;
    // Phase 6b: COURSE SECRETS (FUN_80051698 sub-states 8 / 9 / 0xA).
    pub const REVERSE_RACE: u32 = 0x800B_0394;
    pub const PLAY_MOVIE: u32 = 0x800B_0398;
    pub const DOGFIGHT: u32 = 0x800B_039C;
    pub const BOSS_CHALLENGE: u32 = 0x800B_03A0;
    pub const DOOZER_BATTLE: u32 = 0x800B_03A4;
    pub const BATTLE_AS_BOSS: u32 = 0x800B_03A8;
    pub const CHICKEN_CHASE: u32 = 0x800B_03AC;
    pub const REVERSE: u32 = 0x800B_03B8;
    pub const TWO_PLAYERS: u32 = 0x800B_0418;
    pub const REVERSE_BATTLE_RACE: u32 = 0x800B_058C;
}

/// Inline strings (the code takes their address).
mod si {
    pub const QUESTION: u32 = 0x800A_FF3C;
    pub const DASH: u32 = 0x800B_070C;
    pub const ON: u32 = 0x800B_02AC;
    pub const OFF: u32 = 0x800B_02B4;
    pub const A: u32 = 0x800B_0704;
    pub const B: u32 = 0x800B_0708;
    /// "t " / "x " (FUN_80049C70 / FUN_80049AC0 prefixes).
    pub const T_PREFIX: u32 = 0x800B_06A0;
    pub const X_PREFIX: u32 = 0x800B_069C;
}

/// Pointer tables.
mod tbl {
    /// World names (char *[9]).
    pub const WORLDS: u32 = 0x800A_B0C0;
    /// Course names, 4 per world (char *[36]).
    pub const COURSE_NAMES: u32 = 0x800A_B364;
    /// Course index per (world, cursor) (i8[36]): the cup table.
    pub const CUPS: u32 = 0x800A_C1E4;
    /// Character names (char *, stride 8).
    pub const NAMES: u32 = 0x800A_B128;
    /// Vehicle names (char *[31]).
    pub const VEHICLES: u32 = 0x800A_B2A4;
    /// The 5 x 5 character grid (i8[25]); the character select's order.
    pub const CHAR_GRID: u32 = 0x800A_C208;
    /// Character rows (0x14 bytes): +0x0E flags (bit 0 races), +0x0F vehicle.
    pub const CHARS: u32 = 0x800A_C220;
    /// Vehicle rows (4 bytes): +3 bit 0 = selectable.
    pub const VEHICLE_ROWS: u32 = 0x800A_C5E4;
    /// i16 sine, 4096 = 360 degrees, amplitude 0x4000; cosine = +1024.
    pub const SIN: u32 = 0x800A_88C0;
    /// Volume levels per menu step 0..15: music CdMix (i16[16]), sound.
    pub const MUSIC_LEVELS: u32 = 0x800A_C118;
    pub const SOUND_LEVELS: u32 = 0x800A_C138;
    /// "1ST".."6TH" (char *, stride 8; the standings' column labels).
    pub const PLACES: u32 = 0x800A_B3F8;
    /// RACE POINTS row headings (char *[8]: race, then battle).
    pub const HEADINGS: u32 = 0x800A_B468;
    /// The bosses' names (char *[3] per world: BATTLE AS BOSS - name).
    pub const BOSS_NAMES: u32 = 0x800A_B0DC;
}

/// Everything the menus read from the executable.
pub struct MenuTables {
    pub hud: HudTables,
    pub items: mr_assets::hud_items::ItemTables,
    sin: Vec<i16>,
    s: std::collections::HashMap<u32, String>,
    pub worlds: Vec<String>,
    pub course_names: Vec<String>,
    pub cups: Vec<i8>,
    pub names: Vec<String>,
    pub vehicles: Vec<String>,
    /// The character select's lists (0x800AC6A4 / 0x800AC6C0 as
    /// FUN_80092B08 / FUN_80092928 fill them with every model loaded).
    pub char_list: Vec<u8>,
    pub vehicle_list: Vec<u8>,
    pub default_vehicle: Vec<u8>,
    pub music_levels: [i16; 16],
    pub sound_levels: [i16; 16],
    pub places: Vec<String>,
    pub headings: Vec<String>,
    pub boss_names: Vec<String>,
    /// GAME STATUS's labels (WON, BOSS, DOOZER, TOKENS, FOOD, STARS,
    /// MOVIE, MUPPET) and each course's character (0x800ACB3C + 0xC * c).
    pub gs_labels: Vec<String>,
    pub course_chars: Vec<i8>,
}

impl MenuTables {
    pub fn from_exe(exe: &[u8]) -> MenuTables {
        let ptrs = [
            sp::MEET_THE_MUPPETS, sp::SELECT_COURSE, sp::CHANGE_COURSE, sp::GAME_STATUS, sp::OPTIONS, sp::SET_CONTROLLERS, sp::ADJUST_SCREEN, sp::MEMORY_CARD,
            sp::ONE_PLAYER, sp::TOURNAMENT, sp::ADVENTURE, sp::ADJUST_SOUND, sp::BACK, sp::CANCEL, sp::ACCEPT, sp::SELECT, sp::MUSIC, sp::MOVIE, sp::RACE_A,
            sp::RACE_B, sp::STUNT, sp::BATTLE, sp::RACE, sp::BATTLE_RACE, sp::ONE_PLAYER_RACE, sp::TWO_PLAYER_RACE, sp::LAP, sp::LAPS, sp::SECRETS,
            sp::GAME_MENU, sp::BACK_TO_MAIN_MENU, sp::BEST_LAP, sp::NOT_COLLECTED, sp::NOT_WON, sp::MUPPET, sp::VEHICLE, sp::PAUSED, sp::CONTINUE,
            sp::RETIRE, sp::RESTART, sp::YES, sp::NO, sp::MUSIC_VOLUME, sp::SOUND_VOLUME, sp::CONTROLLER, sp::JUMP, sp::BRAKE, sp::CONFIGURATION,
            sp::VIBRATION, sp::RACE_LEAGUE_A, sp::RACE_LEAGUE_B, sp::BATTLE_LEAGUE, sp::RANDOM_RACE_LEAGUE, sp::RACE_POINTS, sp::TOURNAMENT_POINTS,
            sp::FINAL_TABLE, sp::AFTER, sp::RACES, sp::NEXT, sp::START_RACE, sp::START_BATTLE, sp::POINTS, sp::QUIT_TOURNAMENT, sp::P1,
            sp::BEST_TIME, sp::ONE_PLAYER_STUNT_TRIAL, sp::TWO_PLAYER_STUNT_RACE, sp::ONE_PLAYER_BATTLE, sp::TWO_PLAYER_BATTLE,
            sp::REVERSE_RACE, sp::PLAY_MOVIE, sp::DOGFIGHT, sp::BOSS_CHALLENGE, sp::DOOZER_BATTLE, sp::BATTLE_AS_BOSS, sp::CHICKEN_CHASE,
            sp::REVERSE, sp::TWO_PLAYERS, sp::REVERSE_BATTLE_RACE, sp::BACK_TO_CREDITS,
        ];
        let mut s: std::collections::HashMap<u32, String> = ptrs.iter().map(|&p| (p, exe_ptr_str(exe, p))).collect();
        for a in [si::QUESTION, si::DASH, si::ON, si::OFF, si::A, si::B, si::T_PREFIX, si::X_PREFIX] {
            s.insert(a, exe_str(exe, a));
        }
        let sin = exe_bytes(exe, tbl::SIN, 5120 * 2).chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
        let grid: Vec<u8> = exe_bytes(exe, tbl::CHAR_GRID, 25).to_vec();
        let char_row = |c: u8, o: u32| exe_bytes(exe, tbl::CHARS + 0x14 * c as u32 + o, 1)[0];
        let default_vehicle: Vec<u8> = (0..25).map(|c| char_row(c, 0x0F)).collect();
        let vehicle_ok = |v: u8| exe_bytes(exe, tbl::VEHICLE_ROWS + 4 * v as u32 + 3, 1)[0] & 1 != 0;
        let char_list = grid.iter().copied().filter(|&c| char_row(c, 0x0E) & 1 != 0).collect();
        let vehicle_list = grid.iter().map(|&c| default_vehicle[c as usize]).filter(|&v| vehicle_ok(v)).collect();
        let lv = |a: u32| -> [i16; 16] { std::array::from_fn(|i| i16::from_le_bytes(exe_bytes(exe, a + 2 * i as u32, 2).try_into().unwrap())) };
        MenuTables {
            hud: HudTables::from_exe(exe).expect("HUD tables"),
            items: mr_assets::hud_items::ItemTables::from_exe(exe).expect("item HUD tables"),
            sin,
            s,
            worlds: (0..9).map(|i| exe_ptr_str(exe, tbl::WORLDS + 4 * i)).collect(),
            course_names: (0..36).map(|i| exe_ptr_str(exe, tbl::COURSE_NAMES + 4 * i)).collect(),
            cups: exe_bytes(exe, tbl::CUPS, 36).iter().map(|&b| b as i8).collect(),
            names: (0..25).map(|i| exe_ptr_str(exe, tbl::NAMES + 8 * i)).collect(),
            vehicles: (0..31).map(|i| exe_ptr_str(exe, tbl::VEHICLES + 4 * i)).collect(),
            char_list,
            vehicle_list,
            default_vehicle,
            music_levels: lv(tbl::MUSIC_LEVELS),
            sound_levels: lv(tbl::SOUND_LEVELS),
            places: (0..6).map(|i| exe_ptr_str(exe, tbl::PLACES + 8 * i)).collect(),
            headings: (0..8).map(|i| exe_ptr_str(exe, tbl::HEADINGS + 4 * i)).collect(),
            boss_names: (0..18).map(|i| exe_ptr_str(exe, tbl::BOSS_NAMES + 4 * i)).collect(),
            gs_labels: [0x800B_0184u32, 0x800B_0190, 0x800B_0194, 0x800B_01A0, 0x800B_01AC, 0x800B_01B8, 0x800B_01C4, 0x800B_01D0].iter().map(|&a| exe_ptr_str(exe, a)).collect(),
            course_chars: (0..40).map(|c| exe_bytes(exe, 0x800A_CB3C + 0xC * c, 1)[0] as i8).collect(),
        }
    }

    fn st(&self, a: u32) -> &str {
        self.s.get(&a).map(|s| s.as_str()).unwrap_or("")
    }

    fn sin(&self, a: i32) -> i32 {
        self.sin[(a & 0xFFF) as usize] as i32
    }

    fn cos(&self, a: i32) -> i32 {
        self.sin[(a & 0xFFF) as usize + 1024] as i32
    }

    /// The selected item's blend colour: (sin[(f & 0x1F) * 0x80] >> 9) -
    /// 0x21 on all three channels (f = 0x800B07D4).
    pub fn pulse(&self, f: i32) -> [u8; 3] {
        let v = self.sin((f & 0x1F) * 0x80);
        let v = if v < 0 { v + 0x1FF } else { v };
        let c = ((v >> 9) - 0x21) as u8;
        [c; 3]
    }

    /// The arrows' half size: ((sin[(f & 0xF) * 0x100] >> 12) + 9) / 2.
    pub fn arrow_size(&self, f: i32) -> i32 {
        let v = self.sin((f & 0xF) * 0x100);
        let v = if v < 0 { v + 0xFFF } else { v };
        ((v >> 12) + 9) / 2
    }

    /// FUN_800307D4(x, y, angle, r, g, b, w, h): a flat triangle pointing
    /// up at angle 0 (0x400 right, 0x800 down, 0xC00 left).
    pub fn arrow(&self, x: i32, y: i32, angle: i32, rgb: [u8; 3], w: i32, h: i32) -> Prim {
        let (s, c) = (self.sin(angle), self.cos(angle));
        let lx = [0, w, -w];
        let ly = [-h, h, h];
        let v = std::array::from_fn(|i| {
            let px = x + (lx[i] * c - ly[i] * s) / 0x2800;
            let t = lx[i] * s + ly[i] * c;
            let py = y + (if t < 0 { t + 0x3FFF } else { t } >> 14);
            [px, py]
        });
        Prim::F3 { v, rgb }
    }

    pub fn music_gain(&self, v: u8) -> f32 {
        self.music_levels[v.min(15) as usize] as f32 / self.music_levels[12].max(1) as f32
    }

    pub fn sound_gain(&self, v: u8) -> f32 {
        self.sound_levels[v.min(15) as usize] as f32 / self.sound_levels[12].max(1) as f32
    }
}

pub fn tables() -> &'static MenuTables {
    static T: OnceLock<MenuTables> = OnceLock::new();
    T.get_or_init(|| {
        let mut disc = mr_disc::Disc::open_default().expect("open the disc");
        MenuTables::from_exe(&disc.read("SLUS_012.37").expect("read the executable"))
    })
}

// ---------------------------------------------------------------- progress

/// The unlock state the menus read: the save block 0x800E0190..
/// (save.rs, mr_sim::save; with `unlock_all` the game's everything code
/// applied to a copy).
#[derive(Clone, Debug)]
pub struct Progress(pub mr_sim::save::SaveBlock);

impl Progress {
    /// A new game (FUN_8008F448; tests).
    #[cfg(test)]
    pub fn new_game() -> Progress {
        let t = crate::save::with(|s| s.tables.clone());
        let mut seed = 1u32;
        let mut rand = || mr_sim::session::bios_rand(&mut seed);
        Progress(mr_sim::save::SaveBlock::new_game(&t, &mut rand))
    }

    pub fn char_open(&self, c: u8) -> bool {
        self.0.char_open(c)
    }

    pub fn vehicle_open(&self, c: u8, v: u8) -> bool {
        self.0.vehicle_open(c, v)
    }

    pub fn course_open(&self, world: usize, ty: usize) -> bool {
        self.0.course_open(world, ty)
    }

    /// 0x800E038C: laps of a 1-player race.
    pub fn laps(&self) -> u8 {
        self.0.laps(0)
    }
}

fn progress() -> Progress {
    Progress(crate::save::view())
}

/// Worlds in the course select / world list: the game's 7 (0x800B07AB),
/// plus world 7 (SPEEDWAY, RALLY, GLACIER) once DISCO is won
/// (0x800E02B9 bit 0: the game's secret screen, course 0x1D, then offers
/// them; that screen is not ported, so the port lists the world).
/// World 8 (the cup-8 arenas VOLCANO, HOUSE, WHATEVER) likewise once
/// everything is complete (0x800E02B9 bit 1; phase 4).
fn worlds_shown(p: &Progress) -> usize {
    if p.0.secrets() & 2 != 0 {
        9
    } else if p.0.secrets() & 1 != 0 {
        8
    } else {
        7
    }
}

/// The courses the port runs (the 18 race courses of the course table and
/// the 6 stunt courses' trial).
fn is_race(course: i8) -> bool {
    course >= 0 && crate::courses::races().iter().any(|c| c.id as i8 == course)
}

// ---------------------------------------------------------------- pages

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Page {
    /// No menu: the title's PRESS START or the race.
    #[default]
    None,
    Main,
    CourseSelect,
    Options,
    Sound,
    Controllers,
    PortSettings,
    CourseMenu,
    GameMenu,
    WorldList,
    CourseList,
    CharSelect,
    Pause,
    /// YES / NO after RETIRE (false) or RESTART (true).
    Confirm(bool),
    /// TOURNAMENT - 1 PLAYER: the leagues (title sub-state 2).
    Leagues,
    /// After a league race (game mode 0x11): 1 RACE POINTS, 2 TOURNAMENT
    /// POINTS (0x800B0A64).
    Standings(u8),
    /// A league's course menu (0x800B0EA2 = 4): START RACE n, POINTS,
    /// QUIT TOURNAMENT.
    LeagueMenu,
    /// Its POINTS (0x800B0EA2 = 3: FUN_8004EEDC's table).
    LeaguePoints,
    /// QUIT TOURNAMENT's YES / NO (0x800B0EA2 = 5).
    LeagueQuit,
    /// Phase 6b: <COURSE> SECRETS (course menu row 2; FUN_80074A94 sub-state
    /// 8 on race courses, 9 in the arenas, 0xA on stunt courses).
    Secrets,
    /// Phase 6b: GAME STATUS (0x800B07CE; main menu row 1, game menu row
    /// 1): FUN_8004A878's page; `col` = 0x800B07FA, `row` = 0x800B07FC;
    /// back to this page.
    GameStatus(GameStatusFrom),
}

/// Where GAME STATUS goes back to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GameStatusFrom {
    Main,
    GameMenu,
}

impl Page {
    /// Pages over a loaded course whose race is not running (the session
    /// runs `menu_tick`).
    pub fn course_menu(self) -> bool {
        matches!(self, Page::CourseMenu | Page::GameMenu | Page::WorldList | Page::CourseList | Page::CharSelect | Page::Standings(_) | Page::LeagueMenu | Page::LeaguePoints | Page::LeagueQuit | Page::Secrets | Page::GameStatus(GameStatusFrom::GameMenu))
    }

    /// Pages that freeze the race (virtual time paused).
    pub fn pauses_race(self) -> bool {
        matches!(self, Page::Pause | Page::Confirm(_)) || (self == Page::Sound && !crate::title::on_title())
    }
}

/// What sim_tick (main.rs) should do with the session.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Request {
    #[default]
    None,
    /// A menu session (the kart alone on grid slot 0) for the chosen
    /// character / vehicle; `keep_camera`: carry the orbit camera over.
    Menu { keep_camera: bool },
    /// A race with the chosen character / vehicle.
    Race,
}

/// The front end's state.
#[derive(Resource)]
pub struct Front {
    pub page: Page,
    pub row: i8,
    pub col: i8,
    /// Cursor rows remembered per page (0x800E1B20 / 0x800E0C00).
    memory: std::collections::HashMap<Page, (i8, i8)>,
    /// Where `Sound` goes back to.
    sound_from: Page,
    prev: u16,
    /// 0x800B07D4: the menu frame counter (VBlanks).
    pub vb: i32,
    vb_frac: f32,
    pub progress: Progress,
    /// The player's character / vehicle (0x800B07A4 / 0x800B07A8 rows).
    pub character: u8,
    pub vehicle: u8,
    /// ADJUST SOUND's values while it is open (0x800B0E30 / 31).
    sound: [u8; 2],
    /// PORT SETTINGS: the file's settings being edited.
    port: Option<crate::settings::Settings>,
    pub request: Request,
    /// The current course's world / type (0x800B0E38 / 0x800B0E32).
    here: Option<(usize, usize)>,
    /// The loaded course (0x800B0FAC) and its lap record.
    pub course: u8,
    pub lap_record: i32,
    /// The league being played (league.rs; `--league`).
    pub league: Option<crate::league::League>,
    /// Phase 6: GAME MENU -> ADVENTURE chose this character select's run
    /// (0x800B07E6; game mode 0xB).
    pub adventure: bool,
    /// The "BEAT x" race after an ADVENTURE's tag: the rival (0x800B07FE).
    pub beat: Option<u8>,
    /// 0x800B0A10: the Doozer's vehicle turn (0..2).
    pub doozer_turn: u8,
    /// Phase 6b: the COURSE SECRETS mode chosen (race flags 0x800B07E2,
    /// BOSS CHALLENGE 0x800B07E8) for the coming race.
    pub secret: Option<Secret>,
    /// 0x800B07F3: BATTLE AS BOSS's boss (LEFT / RIGHT; FUN_8008ACE4
    /// clears it at the course load).
    pub boss_pick: u8,
    /// Phase 7: the coming race's human players (0x800B07CA: 2 = split
    /// screen), player 2's character / vehicle (0x800B0B28[1] /
    /// 0x800B0C18[1]); `picking` 1 while player 2 chooses (the port picks in
    /// turn on one character select; the game shows both at once).
    pub players: u8,
    pub p2_character: u8,
    pub p2_vehicle: u8,
    pub picking: u8,
    /// Player 1's pick while player 2 chooses.
    p1_pick: (u8, u8),
    /// Player 2's pad this frame (SET CONTROLLERS' second column, the
    /// character select in player 2's turn).
    pub pad2: u16,
    prev2: u16,
}

/// One COURSE SECRETS row: its text ("?" while its secret is not found),
/// and what CROSS does.
pub struct SecretRow {
    pub text: String,
    pub open: bool,
    pub action: SecretAction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecretAction {
    /// The character select, then the race in this mode.
    CharSelect(Secret),
    /// The race at once (BATTLE AS BOSS, DOOZER BATTLE: 0x800B0F10 = 2).
    Race(Secret),
    /// Phase 7: a two-player mode (split screen): the character select
    /// for both players, then the race.
    TwoPlayers(Secret),
    /// PLAY MOVIE (phase 9): shown, greyed.
    NotPorted,
}

/// What a COURSE SECRETS row starts (FUN_80074A94 sub-states 8 / 9 / 0xA).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Secret {
    pub race_flags: u16,
    pub boss: bool,
}

impl Front {
    /// The loaded course's race type (FUN_8008AB5C @0x8008ABF8: cups 0..5
    /// slot 1 = 1, slot 2 / cup 8 = 2, slot 3 = 3 (the stunt trial)).
    pub fn race_type(&self) -> i16 {
        match self.here {
            Some((w, 3)) if w < 6 => 3,
            Some((w, 1)) if w < 6 => 1,
            Some((w, 2)) if w < 6 => 2,
            // ARENA/BATTLE (course 0x1B, world 6 slot 1) and cup 8
            Some((6, 1)) | Some((8, _)) => 2,
            _ => 0,
        }
    }

    /// Phase 6b: the COURSE SECRETS page's four rows (FUN_80051698 cases 8 /
    /// 9 / 0xA: the text; FUN_80074A94 sub-states 8 / 9 / 0xA: CROSS). The
    /// course's save flags (0x800E0196 + 8 * slot + 0x20 * world) open
    /// them: every item box (8), every turbo token (0x10), both (0x18), the
    /// ten targets (4), the movie object (0x80), the boss beaten (0x100).
    /// One player: the "n PLAYERS" choice stays at 1 PLAYER.
    pub fn secret_rows(&self, t: &MenuTables) -> [SecretRow; 4] {
        let f = self.here.map(|(w, c)| self.progress.0.flags(w, c)).unwrap_or(0);
        let txt = |a: u32| t.st(a).to_string();
        let one = |a: &str| format!("{} - {}", a, t.st(sp::ONE_PLAYER));
        let row = |open: bool, text: String, action: SecretAction| SecretRow { text: if open { text } else { t.st(si::QUESTION).to_string() }, open, action };
        let sec = |race_flags: u16| Secret { race_flags, boss: false };
        let movie = row(f & 0x80 != 0, txt(sp::PLAY_MOVIE), SecretAction::NotPorted);
        // phase 7: CHICKEN CHASE (race flag 0x40, two players)
        let chicken = row(f & 4 != 0, txt(sp::CHICKEN_CHASE), SecretAction::TwoPlayers(sec(0x40)));
        match self.race_type() {
            2 if self.course == 0x1B => [
                row(f & 8 != 0, one(t.st(sp::DOGFIGHT)), SecretAction::CharSelect(sec(0x80))),
                row(f & 0x10 != 0, one(t.st(sp::DOOZER_BATTLE)), SecretAction::Race(sec(0x100))),
                chicken,
                movie,
            ],
            2 => {
                let w = self.here.map_or(0, |h| h.0);
                let name = t.boss_names.get(w * 3 + self.boss_pick as usize).cloned().unwrap_or_default();
                [
                    row(f & 0x18 == 0x18, txt(sp::BOSS_CHALLENGE), SecretAction::CharSelect(Secret { race_flags: 0, boss: true })),
                    row(f & 0x100 != 0, format!("{} - {}", t.st(sp::BATTLE_AS_BOSS), name), SecretAction::Race(sec(0x20))),
                    chicken,
                    movie,
                ]
            }
            3 => [
                row(f & 8 != 0, one(t.st(sp::REVERSE)), SecretAction::CharSelect(sec(4))),
                // (BATTLE on a stunt course is two-player: 0x800B07CB = 2)
                row(f & 0x10 != 0, txt(sp::BATTLE), SecretAction::TwoPlayers(sec(1))),
                chicken,
                movie,
            ],
            _ => {
                let disco = self.course == 0x1F;
                [
                    row(f & 8 != 0, one(t.st(if disco { sp::REVERSE_BATTLE_RACE } else { sp::REVERSE_RACE })), SecretAction::CharSelect(sec(if disco { 10 } else { 2 }))),
                    row(f & 0x10 != 0, one(t.st(sp::BATTLE)), SecretAction::CharSelect(sec(1))),
                    if disco { chicken } else { row(f & 4 != 0, one(t.st(sp::BATTLE_RACE)), SecretAction::CharSelect(sec(8))) },
                    movie,
                ]
            }
        }
    }

    /// COURSE SECRETS on this course: worlds 0..6 (the cups above 6 have
    /// the secret screen's own course menus: not ported).
    pub fn secrets_offered(&self) -> bool {
        self.here.is_some_and(|(w, _)| w <= 6)
    }

    /// The state on a loaded course `id` (tests; `Default` takes this run's).
    pub fn on_course(mut self, id: u8, lap_record: i32) -> Front {
        self.course = id;
        self.here = tables().cups.iter().position(|&c| c == id as i8).map(|i| (i / 4, i % 4));
        // BEST LAP: the save's record (0x800E0190 + 0x20 * world + 8 * slot)
        self.lap_record = self.here.map(|(w, t)| self.progress.0.record(w, t)).unwrap_or(lap_record);
        self
    }
}

impl Default for Front {
    fn default() -> Self {
        let st = crate::settings::Settings::global();
        let arg = |k: &str| crate::settings::arg_value(k).and_then(|v| v.parse::<u8>().ok());
        let character = arg("--character").unwrap_or(0);
        let vehicle = arg("--vehicle").unwrap_or_else(|| tables().default_vehicle.get(character as usize).copied().unwrap_or(0));
        let info = crate::courses::info();
        if !tool_run() {
            // FUN_8008ACE4: the course load resets laps not yet earned
            crate::save::with(|s| {
                let place = if crate::title::on_title() { None } else { s.tables.place(info.id) };
                s.block.course_loaded(place, info.race_type);
            });
            crate::save::persist();
        }
        let f = Front {
            page: Page::None,
            row: 0,
            col: 0,
            memory: Default::default(),
            sound_from: Page::Options,
            prev: u16::MAX,
            vb: 0,
            vb_frac: 0.0,
            progress: progress(),
            character,
            vehicle,
            sound: [st.i64("music_volume", 12).clamp(0, 15) as u8, st.i64("sound_volume", 12).clamp(0, 15) as u8],
            port: None,
            request: Request::None,
            here: None,
            course: info.id,
            lap_record: info.lap_record,
            league: crate::league::League::load(),
            adventure: false,
            beat: None,
            doozer_turn: 0,
            secret: None,
            boss_pick: 0,
            players: 1,
            p2_character: 1,
            p2_vehicle: tables().default_vehicle.get(1).copied().unwrap_or(0),
            picking: 0,
            p1_pick: (character, vehicle),
            pad2: 0,
            prev2: u16::MAX,
        };
        if crate::title::on_title() { f } else { f.on_course(info.id, info.lap_record) }
    }
}

/// The menu sounds: FUN_8005EECC(0xE, 0x1800, 0x40, 0x40) on a cursor move,
/// (0xF, 0x1800, 0x60, 0x60) on confirm / back.
pub const NAV_SFX: (i32, i32, i32, i32) = (0xE, 0x1800, 0x40, 0x40);
pub const CONFIRM_SFX: (i32, i32, i32, i32) = (0xF, 0x1800, 0x60, 0x60);

impl Front {
    /// Open a page with its remembered (or given) cursor.
    pub fn open(&mut self, page: Page) {
        self.memory.insert(self.page, (self.row, self.col));
        let (r, c) = match page {
            // FUN_80074A94: YES / NO open on NO
            Page::Confirm(_) | Page::LeagueQuit => (1, 0),
            Page::CharSelect => self.char_cursor(),
            Page::CourseSelect | Page::WorldList => self.here.map(|(w, t)| (w as i8, t as i8)).or_else(|| self.memory.get(&page).copied()).unwrap_or((0, 0)),
            Page::CourseList => (self.memory.get(&Page::CourseList).map(|m| m.0).unwrap_or(0), 0),
            Page::Pause => (0, 0),
            Page::Sound => (0, 0),
            _ => self.memory.get(&page).copied().unwrap_or((0, 0)),
        };
        self.page = page;
        self.row = r;
        self.col = c;
        // a page opens with every button counted as held
        self.prev = u16::MAX;
    }

    /// The character select's cursor for the chosen pair.
    fn char_cursor(&self) -> (i8, i8) {
        let t = tables();
        let r = t.char_list.iter().position(|&c| c == self.character).unwrap_or(0) as i8;
        let c = t.vehicle_list.iter().position(|&v| v == self.vehicle).unwrap_or(0) as i8;
        (r, c)
    }

    /// Phase 7: the character select for `players` (2: player 1, then
    /// player 2).
    fn start_select(&mut self, players: u8) {
        self.players = players;
        self.picking = 0;
        self.open(Page::CharSelect);
        self.request = Request::Menu { keep_camera: true };
    }

    /// The title's START (title.rs).
    pub fn open_main(&mut self) {
        self.open(Page::Main);
    }

    /// The results screen's START / RETIRE: the course menu with a fresh
    /// menu session (the save re-read: records, won bits). In a league the
    /// results go to the standings instead (`League::score` first).
    pub fn to_course_menu(&mut self) {
        self.adventure = false;
        self.beat = None;
        self.secret = None;
        self.refresh();
        let page = if self.league.as_ref().is_some_and(|l| l.started) { Page::Standings(1) } else { Page::CourseMenu };
        self.open(page);
        if matches!(page, Page::Standings(_)) {
            self.vb = 0;
        }
        self.request = Request::Menu { keep_camera: false };
    }

    /// Re-read the save (progress, this course's lap record).
    pub fn refresh(&mut self) {
        self.progress = progress();
        if let Some((w, t)) = self.here {
            self.lap_record = self.progress.0.record(w, t);
        }
    }

    fn rows(&self) -> i8 {
        match self.page {
            Page::Main => 5,
            Page::Options => 5,
            Page::Sound | Page::Confirm(_) => 2,
            Page::Controllers => 2,
            Page::PortSettings => PORT_KEYS.len() as i8,
            // the secret screen's courses: sub-states 0xB (world 7) / 0xC (8)
            Page::CourseMenu => match self.here.map(|h| h.0) {
                Some(7) => 7,
                Some(8) => 4,
                _ => 5,
            },
            Page::GameMenu => 4,
            Page::WorldList | Page::CourseSelect => worlds_shown(&self.progress) as i8,
            Page::CourseList => 4,
            Page::CharSelect => tables().char_list.len() as i8,
            Page::Pause => 4,
            Page::Leagues => 4,
            Page::LeagueMenu => 3,
            Page::LeagueQuit => 2,
            Page::Secrets => 4,
            Page::GameStatus(_) => 7,
            Page::LeaguePoints => 1,
            Page::Standings(_) | Page::None => 1,
        }
    }
}

// ---------------------------------------------------------------- drawing

/// y of item `k` of `rows` centred on `centre` (0x78): (rows - 1) * -8 +
/// centre + 0x10 * k.
pub fn item_y(k: i32, rows: i32, centre: i32) -> i32 {
    centre - (rows - 1) * 8 + 0x10 * k
}

/// One menu item as the game draws it: the cursor row medium (size 0) in
/// colour 0 with the pulse, the others small (size 2) in colour 1 with the
/// item's tint (0x800B068C..E); `abr` 0x60 = available, 0 = greyed.
#[derive(Clone, Debug)]
pub struct Item {
    pub text: String,
    pub tint: [u8; 3],
    pub abr: u8,
}

impl Item {
    pub fn new(s: impl Into<String>) -> Item {
        Item { text: s.into(), tint: [0x80; 3], abr: 0x60 }
    }
    pub fn grey(mut self, off: bool) -> Item {
        if off {
            self.abr = 0;
        }
        self
    }
    pub fn tint(mut self, rgb: [u8; 3]) -> Item {
        self.tint = rgb;
        self
    }
}

pub fn items(t: &MenuTables, list: &[Item], sel: i32, centre: i32, f: i32) -> Vec<Sprite> {
    let n = list.len() as i32;
    let mut out = Vec::new();
    for (k, it) in list.iter().enumerate() {
        let y = item_y(k as i32, n, centre);
        // (a tinted item is drawn with colour 0, the white CLUT: oracle GP0
        // of the course menu, world list)
        let (size, colour, rgb) = if k as i32 == sel { (0, 0, t.pulse(f)) } else { (2, if it.tint == [0x80; 3] { 1 } else { 0 }, it.tint) };
        out.extend(t.hud.text_rgb(it.text.as_bytes(), 0x100, y, 0, Size::from_arg(size), colour, it.abr, rgb));
    }
    out
}

/// FUN_80049C70: "t BACK" (or CANCEL) in green (0x10, 0xA4, 0x60).
pub fn back_button(t: &MenuTables, x: i32, y: i32, align: u8, cancel: bool) -> Vec<Sprite> {
    let s = format!("{}{}", t.st(si::T_PREFIX), t.st(if cancel { sp::CANCEL } else { sp::BACK }));
    t.hud.text_rgb(s.as_bytes(), x, y, align, Size::from_arg(0), 0, 0x60, [0x10, 0xA4, 0x60])
}

/// FUN_80049AC0: "x SELECT" (or ACCEPT) in blue (0x68, 0x84, 0xDC).
pub fn select_button(t: &MenuTables, x: i32, y: i32, align: u8, accept: bool) -> Vec<Sprite> {
    let s = format!("{}{}", t.st(si::X_PREFIX), t.st(if accept { sp::ACCEPT } else { sp::SELECT }));
    t.hud.text_rgb(s.as_bytes(), x, y, align, Size::from_arg(0), 0, 0x60, [0x68, 0x84, 0xDC])
}

/// FUN_80049AC0 in game mode 0x11: "x NEXT".
fn next_button(t: &MenuTables) -> Vec<Sprite> {
    let s = format!("{}{}", t.st(si::X_PREFIX), t.st(sp::NEXT));
    t.hud.text_rgb(s.as_bytes(), 0x20, 0xD2, 2, Size::from_arg(0), 0, 0x60, [0x68, 0x84, 0xDC])
}

/// The menu frame's 2D layer (512 x 240 game screen; earlier primitives on
/// top). `here`: the loaded course's name and lap record (course pages).
pub fn page_prims(fr: &Front, t: &MenuTables) -> Vec<Prim> {
    let mut sp_out: Vec<Sprite> = Vec::new();
    let mut tri: Vec<Prim> = Vec::new();
    let f = fr.vb;
    let sel = fr.row as i32;
    let txt = |s: u32| t.st(s).to_string();
    let p = &fr.progress;
    match fr.page {
        Page::None => {}
        Page::Main => {
            // FUN_80051698 course 0x1E, sub-state 1 (@0x8005A034)
            let list = [
                Item::new(txt(sp::MEET_THE_MUPPETS)).grey(true),
                Item::new(txt(sp::GAME_STATUS)),
                Item::new(txt(sp::SELECT_COURSE)),
                Item::new(format!("{} - {}", txt(sp::TOURNAMENT), txt(sp::ONE_PLAYER))),
                Item::new(txt(sp::OPTIONS)),
            ];
            sp_out.extend(items(t, &list, sel, 0x78, f));
            sp_out.extend(select_button(t, 0x100, 0xD2, 0, false));
            sp_out.extend(back_button(t, 0x100, 0x1E, 0, false));
        }
        Page::Leagues => {
            // FUN_80050440 (sub-state 2): "RACE LEAGUE A - n LAPS" etc.,
            // "?" while closed
            let names = [sp::RACE_LEAGUE_A, sp::RACE_LEAGUE_B, sp::BATTLE_LEAGUE, sp::RANDOM_RACE_LEAGUE];
            let list: Vec<Item> = (0..4)
                .map(|l| {
                    if p.0.league_flags(l) & 1 == 0 {
                        Item::new(t.st(si::QUESTION))
                    } else if l == mr_sim::save::LEAGUE_BATTLE {
                        // the arenas (phase 6b): no laps
                        Item::new(txt(names[l]))
                    } else {
                        let n = p.0.league_laps(l);
                        Item::new(format!("{} - {} {}", txt(names[l]), n, txt(if n == 1 { sp::LAP } else { sp::LAPS })))
                    }
                })
                .collect();
            sp_out.extend(items(t, &list, sel, 0x78, f));
            sp_out.extend(select_button(t, 0x100, 0xD2, 0, false));
            sp_out.extend(back_button(t, 0x100, 0x1E, 0, false));
        }
        Page::GameStatus(_) => {
            return game_status(t, p, fr.col, fr.row, fr.vb);
        }
        Page::Standings(page) => {
            if let Some(l) = fr.league.as_ref() {
                sp_out.extend(standings(t, l, page, fr.here, f));
            }
        }
        Page::LeagueMenu | Page::LeaguePoints | Page::LeagueQuit => {
            // FUN_80051698 game mode 0, 0x800B0EA2 = 4 / 3 / 5: the course
            // frame (name, BEST LAP), the league's items
            let here = fr.here.map(|(w, c)| t.course_names[w * 4 + c].clone()).unwrap_or_default();
            sp_out.extend(t.hud.text(here.as_bytes(), 0x1E0, 0x1E, 8, Size::from_arg(0), 4, 0x60));
            sp_out.extend(t.hud.text(best_lap_text(t, fr.lap_record).as_bytes(), 0x1E0, 0xD2, 8, Size::from_arg(0), 4, 0x60));
            let race = fr.league.as_ref().map(|l| l.race).unwrap_or(0);
            match fr.page {
                Page::LeagueMenu => {
                    sp_out.extend(select_button(t, 0x20, 0xD2, 2, false));
                    let list = [
                        // (the BATTLE LEAGUE: START BATTLE n, 0x800C1E4C)
                        Item::new(format!("{} {}", txt(if fr.league.as_ref().is_some_and(|l| l.league as usize == mr_sim::save::LEAGUE_BATTLE) { sp::START_BATTLE } else { sp::START_RACE }), race + 1)),
                        Item::new(txt(sp::POINTS)),
                        Item::new(txt(sp::QUIT_TOURNAMENT)).tint([0xFF, 0x1F, 0x1F]),
                    ];
                    sp_out.extend(items(t, &list, sel, 0x78, f));
                }
                Page::LeagueQuit => {
                    sp_out.extend(select_button(t, 0x20, 0xD2, 2, false));
                    sp_out.extend(back_button(t, 0x20, 0x1E, 2, false));
                    let list = [Item::new(txt(sp::YES)).tint([0x1F, 0xFF, 0x1F]), Item::new(txt(sp::NO)).tint([0xFF, 0x1F, 0x1F])];
                    sp_out.extend(items(t, &list, sel, 0x78, f));
                }
                _ => {
                    // FUN_8004EEDC(0x78): per kart in table order a face, its
                    // points (size 1; colour 5 the leader, 6 the others, 7
                    // none) and "P1" over the player's (blinking), 0x40 apart
                    sp_out.extend(back_button(t, 0x20, 0x1E, 2, false));
                    if let Some(l) = fr.league.as_ref() {
                        let order = l.table();
                        let x0 = 0x100 - 5 * 0x20;
                        let chars: Vec<u8> = order.iter().map(|&k| l.karts[k].0).collect();
                        tri.extend(mr_assets::hud_items::table_faces(&t.items, x0, 0x78, &chars, f));
                        for (i, &k) in order.iter().enumerate() {
                            let x = x0 + 0x40 * i as i32;
                            if k == 0 && f % 0x1E < 0xF {
                                sp_out.extend(t.hud.text(t.st(sp::P1).as_bytes(), x, 0x78 + 0x1B, 1, Size::from_arg(2), 4, 0x60));
                            }
                            let p = l.points[k];
                            let c = if p == 0 { 7 } else if i == 0 { 5 } else { 6 };
                            sp_out.extend(t.hud.text(format!("{p}").as_bytes(), x, 0x78 + 1, 1, Size::from_arg(1), c, 0x60));
                        }
                    }
                }
            }
        }
        Page::Options => {
            // sub-state 3: four items on five rows' positions (the fifth row
            // is the port's PORT SETTINGS)
            let list = [
                Item::new(txt(sp::ADJUST_SOUND)),
                Item::new(txt(sp::SET_CONTROLLERS)),
                Item::new(txt(sp::ADJUST_SCREEN)).grey(true),
                Item::new(txt(sp::MEMORY_CARD)).grey(true),
                Item::new("PORT SETTINGS"),
            ];
            sp_out.extend(items(t, &list, sel, 0x78, f));
            sp_out.extend(select_button(t, 0x100, 0xD2, 0, false));
            sp_out.extend(back_button(t, 0x100, 0x1E, 0, false));
        }
        Page::Sound => {
            // 0x800B07CD: "MUSIC VOLUME - n", "SOUND VOLUME - n"
            let list = [Item::new(format!("{} - {}", txt(sp::MUSIC_VOLUME), fr.sound[0])), Item::new(format!("{} - {}", txt(sp::SOUND_VOLUME), fr.sound[1]))];
            sp_out.extend(items(t, &list, sel, 0x78, f));
            let (x, al) = if fr.sound_from == Page::Pause { (0x20, 2) } else { (0x100, 0) };
            sp_out.extend(select_button(t, x, 0xD2, al, true));
            sp_out.extend(back_button(t, x, 0x1E, al, true));
            if fr.sound_from == Page::Pause {
                sp_out.extend(paused_text(t, f));
            }
        }
        Page::Controllers => {
            // 0x800B07CC, controller 1 (x 0x80)
            let alt = crate::settings::Settings::global().get("controller_config") == Some("B");
            let alt = fr.port.as_ref().map(|s| s.get("controller_config") == Some("B")).unwrap_or(alt);
            let x = 0x80;
            sp_out.extend(t.hud.text(format!("{} 1", txt(sp::CONTROLLER)).as_bytes(), x, 0x4B, 0, Size::from_arg(2), 4, 0x60));
            let sq = format!("s {}", txt(if alt { sp::BRAKE } else { sp::JUMP }));
            sp_out.extend(t.hud.text_rgb(sq.as_bytes(), x, 0x6F, 0, Size::from_arg(0), 0, 0x60, [0xDC, 0x60, 0xBC]));
            let ci = format!("{} o", txt(if alt { sp::JUMP } else { sp::BRAKE }));
            sp_out.extend(t.hud.text_rgb(ci.as_bytes(), x, 0x81, 0, Size::from_arg(0), 0, 0x60, [0xDC, 0x50, 0x28]));
            let rows = [
                (format!("{} - {}", txt(sp::CONFIGURATION), t.st(if alt { si::B } else { si::A })), 0x9D, 0x60),
                (format!("{} - {}", txt(sp::VIBRATION), t.st(si::OFF)), 0xAD, 0),
            ];
            for (k, (s, y, abr)) in rows.iter().enumerate() {
                let (size, colour, rgb) = if k as i32 == sel { (0, 0, t.pulse(f)) } else { (2, 1, [0x80; 3]) };
                sp_out.extend(t.hud.text_rgb(s.as_bytes(), x, *y, 0, Size::from_arg(size), colour, *abr, rgb));
            }
            // phase 7: controller 2 (x 0x180; its pad's LEFT / RIGHT)
            let alt2 = crate::settings::Settings::global().get("controller_config_2") == Some("B");
            let alt2 = fr.port.as_ref().map(|s| s.get("controller_config_2") == Some("B")).unwrap_or(alt2);
            let x = 0x180;
            sp_out.extend(t.hud.text(format!("{} 2", txt(sp::CONTROLLER)).as_bytes(), x, 0x4B, 0, Size::from_arg(2), 4, 0x60));
            let sq = format!("s {}", txt(if alt2 { sp::BRAKE } else { sp::JUMP }));
            sp_out.extend(t.hud.text_rgb(sq.as_bytes(), x, 0x6F, 0, Size::from_arg(0), 0, 0x60, [0xDC, 0x60, 0xBC]));
            let ci = format!("{} o", txt(if alt2 { sp::JUMP } else { sp::BRAKE }));
            sp_out.extend(t.hud.text_rgb(ci.as_bytes(), x, 0x81, 0, Size::from_arg(0), 0, 0x60, [0xDC, 0x50, 0x28]));
            let s2 = format!("{} - {}", txt(sp::CONFIGURATION), t.st(if alt2 { si::B } else { si::A }));
            sp_out.extend(t.hud.text_rgb(s2.as_bytes(), x, 0x9D, 0, Size::from_arg(2), 1, 0x60, [0x80; 3]));
            sp_out.extend(select_button(t, 0x100, 0xD2, 0, true));
            sp_out.extend(back_button(t, 0x100, 0x1E, 0, true));
        }
        Page::PortSettings => {
            let st = fr.port.clone().unwrap_or_else(|| crate::settings::Settings::global().clone());
            let n = PORT_KEYS.len() as i32;
            for (k, key) in PORT_KEYS.iter().enumerate() {
                let v = st.get(key).unwrap_or("").to_ascii_uppercase();
                let s = format!("{} - {}", key.replace('_', " ").to_ascii_uppercase(), v);
                let y = 0x78 - (n - 1) * 0xB / 2 + 0xB * k as i32;
                let (colour, rgb) = if k as i32 == sel { (0, t.pulse(f)) } else { (1, [0x80; 3]) };
                sp_out.extend(t.hud.text_rgb(s.as_bytes(), 0x100, y, 0, Size::from_arg(2), colour, 0x60, rgb));
            }
            sp_out.extend(t.hud.text(b"SAVED TO SETTINGS.TXT - APPLIES ON THE NEXT START", 0x100, 0xC8, 0, Size::from_arg(2), 4, 0x60));
            sp_out.extend(select_button(t, 0x100, 0xDC, 0, true));
            sp_out.extend(back_button(t, 0x100, 0x14, 0, true));
        }
        Page::CourseSelect => {
            // game mode 0xC (FUN_80051698 @0x8005B9C0)
            let (w, c) = (fr.row as usize, fr.col as usize);
            sp_out.extend(t.hud.text(t.worlds[w].as_bytes(), 0x100, 0x3C, 0, Size::from_arg(0), 4, 0x60));
            let open = p.course_open(w, c);
            let width;
            if open {
                sp_out.extend(t.hud.text(type_label(t, w, c).as_bytes(), 0x100, 0x9F, 0, Size::from_arg(2), 1, 0x60));
                let (s, wd) = t.hud.text_w(t.course_names[w * 4 + c].as_bytes(), 0x100, 0xB4, 0, Size::from_arg(1), 5, 0x60);
                sp_out.extend(s);
                width = wd;
            } else {
                let (s, wd) = t.hud.text_w(t.st(si::QUESTION).as_bytes(), 0x100, 0xB4, 0, Size::from_arg(1), 7, 0x60);
                sp_out.extend(s);
                width = wd;
            }
            let playable = open && is_race(t.cups[w * 4 + c]);
            sp_out.extend(t.hud.text_rgb(format!("o {}", txt(sp::MUSIC)).as_bytes(), 0x180, 0xD2, 0, Size::from_arg(0), 0, 0, [0xDC, 0x50, 0x28]));
            sp_out.extend(t.hud.text_rgb(format!("s {}", txt(sp::MOVIE)).as_bytes(), 0x80, 0xD2, 0, Size::from_arg(0), 0, 0, [0xDC, 0x60, 0xBC]));
            sp_out.extend(t.hud.text_rgb(format!("x {}", txt(sp::SELECT)).as_bytes(), 0x100, 0xD2, 0, Size::from_arg(0), 0, if playable { 0x60 } else { 0 }, [0x68, 0x84, 0xDC]));
            let a = t.arrow_size(f);
            if c > 0 {
                tri.push(t.arrow(0xF0 - width / 2, 0xB4, 0xC00, [0xFF, 0, 0], a, a));
            }
            if c < 3 {
                tri.push(t.arrow(width / 2 + 0x110, 0xB4, 0x400, [0xFF, 0, 0], a, a));
            }
            if w > 0 {
                tri.push(t.arrow(0x100, 0x2D, 0, [0xFF, 0, 0], a, a));
            }
            if (w as i8) < fr.rows() - 1 {
                tri.push(t.arrow(0x100, 0x4A, 0x800, [0xFF, 0, 0], a, a));
            }
            sp_out.extend(back_button(t, 0x100, 0x1E, 0, false));
        }
        Page::CourseMenu | Page::GameMenu | Page::WorldList | Page::CourseList | Page::Secrets => {
            // game mode 0 (FUN_80051698 case 0): the course's name and lap
            // record, SELECT bottom left, BACK top left (not on the course
            // menu itself)
            let here = fr.here.map(|(w, c)| t.course_names[w * 4 + c].clone()).unwrap_or_default();
            sp_out.extend(t.hud.text(here.as_bytes(), 0x1E0, 0x1E, 8, Size::from_arg(0), 4, 0x60));
            sp_out.extend(t.hud.text(best_text(t, fr.lap_record, fr.race_type()).as_bytes(), 0x1E0, 0xD2, 8, Size::from_arg(0), 4, 0x60));
            sp_out.extend(select_button(t, 0x20, 0xD2, 2, false));
            if fr.page != Page::CourseMenu {
                sp_out.extend(back_button(t, 0x20, 0x1E, 2, false));
            }
            let list: Vec<Item> = match fr.page {
                // Phase 6b: the secret screen's courses (FUN_80051698 cases
                // 0xB / 0xC): world 7 (SPEEDWAY, RALLY, GLACIER) and the
                // cup-8 arenas; BACK TO CREDITS returns to the secret screen
                // (course 0x1D; not ported: the title here)
                Page::CourseMenu if fr.here.is_some_and(|h| h.0 == 7) => {
                    let laps = p.laps();
                    let lap_s = txt(if laps == 1 { sp::LAP } else { sp::LAPS });
                    let one = |a: u32| format!("{} - {}", txt(a), txt(sp::ONE_PLAYER));
                    vec![
                        Item::new(format!("{} - {} {}", txt(sp::ONE_PLAYER_RACE), laps, lap_s)),
                        Item::new(format!("{} - {} {}", txt(sp::TWO_PLAYER_RACE), p.0.laps(1).max(1), txt(if p.0.laps(1) == 1 { sp::LAP } else { sp::LAPS }))),
                        Item::new(one(sp::REVERSE_RACE)),
                        Item::new(one(sp::BATTLE)),
                        Item::new(one(sp::BATTLE_RACE)),
                        Item::new(txt(sp::CHICKEN_CHASE)),
                        Item::new(txt(sp::BACK_TO_CREDITS)).tint([0xFF, 0x1F, 0x1F]),
                    ]
                }
                Page::CourseMenu if fr.here.is_some_and(|h| h.0 == 8) => vec![
                    Item::new(txt(sp::ONE_PLAYER_BATTLE)),
                    Item::new(txt(sp::TWO_PLAYER_BATTLE)),
                    Item::new(txt(sp::CHICKEN_CHASE)),
                    Item::new(txt(sp::BACK_TO_CREDITS)).tint([0xFF, 0x1F, 0x1F]),
                ],
                Page::CourseMenu if fr.race_type() == 3 => vec![
                    // FUN_80051698 case 0, race type 3: the stunt trial
                    Item::new(txt(sp::ONE_PLAYER_STUNT_TRIAL)),
                    Item::new(txt(sp::TWO_PLAYER_STUNT_RACE)),
                    Item::new(format!("{} {}", here, txt(sp::SECRETS))).grey(!fr.secrets_offered()),
                    Item::new(txt(sp::GAME_MENU)).tint([0x1F, 0xFF, 0x1F]),
                    Item::new(txt(sp::BACK_TO_MAIN_MENU)).tint([0xFF, 0x1F, 0x1F]),
                ],
                Page::CourseMenu if fr.race_type() == 2 => vec![
                    // FUN_80051698 case 0, race type 2: the arena (phase 4)
                    Item::new(txt(sp::ONE_PLAYER_BATTLE)),
                    Item::new(txt(sp::TWO_PLAYER_BATTLE)),
                    Item::new(format!("{} {}", here, txt(sp::SECRETS))).grey(!fr.secrets_offered()),
                    Item::new(txt(sp::GAME_MENU)).tint([0x1F, 0xFF, 0x1F]),
                    Item::new(txt(sp::BACK_TO_MAIN_MENU)).tint([0xFF, 0x1F, 0x1F]),
                ],
                Page::CourseMenu => {
                    let laps = p.laps();
                    let lap_s = txt(if laps == 1 { sp::LAP } else { sp::LAPS });
                    vec![
                        Item::new(format!("{} - {} {}", txt(sp::ONE_PLAYER_RACE), laps, lap_s)),
                        Item::new(format!("{} - {} {}", txt(sp::TWO_PLAYER_RACE), p.0.laps(1).max(1), txt(if p.0.laps(1) == 1 { sp::LAP } else { sp::LAPS }))),
                        Item::new(format!("{} {}", here, txt(sp::SECRETS))).grey(!fr.secrets_offered()),
                        Item::new(txt(sp::GAME_MENU)).tint([0x1F, 0xFF, 0x1F]),
                        Item::new(txt(sp::BACK_TO_MAIN_MENU)).tint([0xFF, 0x1F, 0x1F]),
                    ]
                }
                Page::Secrets => fr.secret_rows(t).into_iter().map(|r| Item::new(r.text).grey(r.open && r.action == SecretAction::NotPorted)).collect(),
                Page::GameMenu => vec![
                    Item::new(txt(sp::ADVENTURE)),
                    Item::new(txt(sp::GAME_STATUS)),
                    Item::new(txt(sp::CHANGE_COURSE)),
                    Item::new(format!("{} - {}", txt(sp::TOURNAMENT), txt(sp::ONE_PLAYER))).grey(true),
                ],
                Page::WorldList => (0..worlds_shown(p))
                    .map(|w| {
                        let it = Item::new(t.worlds[w].clone());
                        if w == 6 { it.tint([0, 0x7F, 0xFF]) } else { it }
                    })
                    .collect(),
                _ => {
                    let w = fr.col as usize; // the chosen world (0x800B0F12)
                    (0..4)
                        .map(|c| {
                            let open = p.course_open(w, c);
                            let mut s = if open { t.course_names[w * 4 + c].clone() } else { t.st(si::QUESTION).to_string() };
                            if open && fr.row as usize == c {
                                s += t.st(si::DASH);
                                s += &type_label(t, w, c);
                            }
                            let course = t.cups[w * 4 + c];
                            let current = course == fr.course as i8;
                            Item::new(s).grey(current || !(open && is_race(course)))
                        })
                        .collect()
                }
            };
            sp_out.extend(items(t, &list, sel, 0x78, f));
        }
        Page::CharSelect => {
            // 0x800B07CB = 1 (FUN_80051698 @0x80051C5C)
            let x = 0x100;
            // phase 7: whose pick (the port picks in turn)
            if fr.players == 2 {
                let who = format!("{} {}", txt(sp::CONTROLLER), fr.picking + 1);
                sp_out.extend(t.hud.text(who.as_bytes(), x, 0x2D, 1, Size::from_arg(2), 5, 0x60));
            }
            sp_out.extend(back_button(t, x, 0x1E, 0, false));
            sp_out.extend(select_button(t, x, 0xD2, 0, false));
            let c = t.char_list.get(fr.row as usize).copied().unwrap_or(0);
            let v = t.vehicle_list.get(fr.col as usize).copied().unwrap_or(0);
            let c_open = p.char_open(c);
            let mut warn: Option<u32> = None;
            let name = if c_open { t.names[c as usize].clone() } else {
                warn = Some(sp::NOT_COLLECTED);
                t.st(si::QUESTION).to_string()
            };
            sp_out.extend(t.hud.text(name.as_bytes(), x, 0x3C, 1, Size::from_arg(2), 4, 0x60));
            let v_open = p.vehicle_open(c, v);
            let vname = if v_open && c_open { t.vehicles[v as usize].clone() } else {
                if c_open && warn.is_none() {
                    warn = Some(sp::NOT_WON);
                }
                t.st(si::QUESTION).to_string()
            };
            let (s, vw) = t.hud.text_w(vname.as_bytes(), x, 0xB4, 4, Size::from_arg(2), 4, 0x60);
            sp_out.extend(s);
            let a = t.arrow_size(f);
            let rgb = |b: bool| if b { [0, 0xFF, 0] } else { [0xFF, 0, 0] };
            let r = fr.row as usize;
            let above = t.char_list[..r].iter().any(|&c| p.char_open(c));
            let below = t.char_list[r + 1..].iter().any(|&c| p.char_open(c));
            if r > 0 {
                tri.push(t.arrow(x, 0x32, 0, rgb(above), a, a));
            }
            if r < 0x18 {
                tri.push(t.arrow(x, 0x50, 0x800, rgb(below), a, a));
            }
            let col = fr.col as usize;
            let (left, right) = if c_open {
                (t.vehicle_list[..col].iter().any(|&v| p.vehicle_open(c, v)), t.vehicle_list.get(col + 1..).is_some_and(|s| s.iter().any(|&v| p.vehicle_open(c, v))))
            } else {
                (false, false)
            };
            if col > 0 {
                tri.push(t.arrow(x - vw / 2 - 0x10, 0xAE, 0xC00, rgb(left), a, a));
            }
            if col < 0x18 {
                tri.push(t.arrow(x + vw / 2 + 0x10, 0xAE, 0x400, rgb(right), a, a));
            }
            if let Some(wn) = warn {
                if f % 0x1E < 0xF {
                    let rgb = if wn == sp::NOT_COLLECTED { [0xFF, 0x7F, 0] } else { [0, 0xFF, 0xFF] };
                    sp_out.extend(t.hud.text_rgb(t.st(wn).as_bytes(), x, 0x78, 1, Size::from_arg(2), 7, 0x60, rgb));
                    let w2 = t.st(if wn == sp::NOT_WON { sp::VEHICLE } else { sp::MUPPET });
                    sp_out.extend(t.hud.text_rgb(w2.as_bytes(), x, 0x78, 4, Size::from_arg(2), 7, 0x60, rgb));
                }
            }
        }
        Page::Pause => {
            // FUN_80051698 case 99 (0x800B0BD0 != 0), page 0x800B0E78 = 0
            sp_out.extend(select_button(t, 0x20, 0xD2, 2, false));
            sp_out.extend(paused_text(t, f));
            let list = [Item::new(txt(sp::CONTINUE)), Item::new(txt(sp::RETIRE)), Item::new(txt(sp::RESTART)), Item::new(txt(sp::ADJUST_SOUND))];
            sp_out.extend(items(t, &list, sel, 0x78, f));
        }
        Page::Confirm(_) => {
            // pages 1 / 0xB: YES green, NO red, BACK shown
            sp_out.extend(back_button(t, 0x20, 0x1E, 2, false));
            sp_out.extend(select_button(t, 0x20, 0xD2, 2, false));
            sp_out.extend(paused_text(t, f));
            let list = [Item::new(txt(sp::YES)).tint([0x1F, 0xFF, 0x1F]), Item::new(txt(sp::NO)).tint([0xFF, 0x1F, 0x1F])];
            sp_out.extend(items(t, &list, sel, 0x78, f));
        }
    }
    let mut v: Vec<Prim> = sp_out.into_iter().map(Prim::Sprt).collect();
    v.extend(tri);
    v
}

/// GAME STATUS (FUN_8004A878; phase 6b), in the game's add order: SELECT
/// COURSE; the cursor's cell blinking (0x800B07D4 % 0x1E < 0xF); the 7 x 4
/// course grid (x 0x65 + 0x12 * slot, y 0x3D + 0xD * world): every secret
/// found (flags 0x1FF) the small star icon, else "?" (closed, colour 6),
/// "_" (won, boxes, tokens, targets, boss: 0x127, colour 5) or "*" (colour
/// 7); the star under the grid and "n/31" (0x800E02B8); the cursor's course
/// at 0x140: its name, its secrets in two columns (WON, BOSS (arenas),
/// DOOZER (worlds 0..5), TOKENS / FOOD, STARS, MOVIE, MUPPET (a course
/// character), "_" found, "*" not, the labels greyed where the course has
/// none), the star once all is found, BEST LAP / BEST TIME and the record,
/// its holder and vehicle; BACK; the three leagues (RACE A, BATTLE, RACE
/// B: "?" closed, "#7*" not won, "#5_" won, the star with 30 / 20 / 30
/// points); the half-black backdrop.
fn game_status(t: &MenuTables, p: &Progress, col: i8, row: i8, vb: i32) -> Vec<Prim> {
    let b = &p.0;
    let mut out: Vec<Sprite> = Vec::new();
    let text = |s: &str, x: i32, y: i32, align: u8, size: u8, colour: u8, abr: u8| t.hud.text(s.as_bytes(), x, y, align, Size::from_arg(size), colour, abr);
    let star = |x: i32, y: i32, abr: i32| mr_assets::hud_items::sprt(x, y, 0x40, 0, 0x18, 0x10, [0x80; 3], 0x15, 0xB, abr);
    let (q, done, mark) = (t.st(si::QUESTION), "_", "*");
    out.extend(text(t.st(sp::SELECT_COURSE), 0x80, 0x30, 0, 2, 4, 0x60));
    let (cw, cc) = (row.clamp(0, 6) as usize, col.clamp(0, 3) as usize);
    if vb.rem_euclid(0x1E) < 0xF {
        let (x, y) = (cc as i32 * 0x12 + 0x65, cw as i32 * 0xD + 0x3D);
        let f = b.flags(cw, cc);
        if f & 0x1FF == 0x1FF {
            out.push(star(x - 0xD, y - 8, 0x60));
        } else {
            out.extend(text(if f & 1 == 0 { q } else if f & 0x127 == 0x127 { done } else { mark }, x, y, 0, 0, 4, 0x60));
        }
    }
    for w in 0..7 {
        for c in 0..4 {
            if (w, c) == (cw, cc) {
                continue;
            }
            let (x, y) = (0x65 + 0x12 * c as i32, 0x3D + 0xD * w as i32);
            let f = b.flags(w, c);
            if f & 0x1FF == 0x1FF {
                out.push(Sprite { w: 0x12, h: 0xC, uw: 0x17, vh: 0xF, ..star(x - 10, y - 6, 0x60) });
            } else {
                let (s, colour) = if f & 1 == 0 { (q, 6) } else if f & 0x127 == 0x127 { (done, 5) } else { (mark, 7) };
                out.extend(text(s, x, y, 0, 2, colour, 0x60));
            }
        }
    }
    let ylast = 0x3D + 0xD * 6;
    out.push(star(0x73, ylast + 0x1A - 8, 0x60));
    out.extend(text(&format!("{}/{}", b.status(), 0x1F), 0x80, ylast + 0x27, 0, 2, 4, 0x60));
    let f = b.flags(cw, cc);
    let open = f & 1 != 0;
    out.extend(text(if open { &t.course_names[cw * 4 + cc] } else { q }, 0x140, 0x30, 0, 2, 4, 0x60));
    let x0 = 0x140;
    let (star_x, star_abr);
    if !open {
        out.extend(text(q, x0, 0x5A, 0, 1, 7, 0x60));
        out.extend(text(done, x0 - 0x10, 0x85, 0, 0, 5, 0));
        star_x = x0 | 2;
        star_abr = 0;
    } else {
        let bit = |n: u32| (f >> n) & 1 != 0;
        let course = t.cups[cc + cw * 4];
        let has_char = course != -1 && t.course_chars.get(course as usize).is_some_and(|&c| c != -1);
        let arena_boss = cw < 6 && cc == 2;
        let doozer = cw < 6;
        let flag_mark = |out: &mut Vec<Sprite>, n: u32, x: i32, y: i32| {
            let (s, colour) = if bit(n) { (done, 5) } else { (mark, 7) };
            out.extend(text(s, x, y, 0, 0, colour, 0x60));
        };
        let xl = x0 - 0x10;
        flag_mark(&mut out, 1, xl, 0x41);
        out.extend(text(&t.gs_labels[0], xl - 0xC, 0x41, 8, 2, 6, 0x60));
        if arena_boss {
            flag_mark(&mut out, 8, xl, 0x52);
        }
        out.extend(text(&t.gs_labels[1], xl - 0xC, 0x52, 8, 2, 6, if arena_boss { 0x60 } else { 0 }));
        if doozer {
            flag_mark(&mut out, 5, xl, 99);
        }
        out.extend(text(&t.gs_labels[2], xl - 0xC, 99, 8, 2, 6, if doozer { 0x60 } else { 0 }));
        flag_mark(&mut out, 2, xl, 0x74);
        out.extend(text(&t.gs_labels[3], xl - 0xC, 0x74, 8, 2, 6, 0x60));
        out.extend(text(done, xl, 0x85, 0, 0, 5, if f & 0x127 == 0x127 { 0x60 } else { 0 }));
        let xr = x0 + 0x10;
        flag_mark(&mut out, 4, xr, 0x41);
        out.extend(text(&t.gs_labels[4], xr + 0xC, 0x41, 2, 2, 6, 0x60));
        flag_mark(&mut out, 3, xr, 0x52);
        out.extend(text(&t.gs_labels[5], xr + 0xC, 0x52, 2, 2, 6, 0x60));
        flag_mark(&mut out, 7, xr, 99);
        out.extend(text(&t.gs_labels[6], xr + 0xC, 99, 2, 2, 6, 0x60));
        if has_char {
            flag_mark(&mut out, 6, xr, 0x74);
        }
        out.extend(text(&t.gs_labels[7], xr + 0xC, 0x74, 2, 2, 6, if has_char { 0x60 } else { 0 }));
        star_x = xr - 0xE;
        star_abr = if f & 0x1FF == 0x1FF { 0x60 } else { 0 };
    }
    out.push(star(star_x, 0x7D, star_abr));
    if !open {
        out.extend(text(q, 0x140, 0x96, 0, 0, 4, 0x60));
        out.extend(text(q, 0x140, 0xA7, 0, 2, 4, 0x60));
        out.extend(text(q, 0x140, 0xB2, 0, 2, 4, 0x60));
    } else {
        let best_time = (cw > 5 || cc > 1) && (cw != 6 || cc == 1);
        out.extend(text(t.st(if best_time { sp::BEST_TIME } else { sp::BEST_LAP }), 0x13E, 0x96, 8, 2, 6, 0x60));
        out.extend(text(&mr_assets::hud::time_text_cs(b.record(cw, cc), false), 0x142, 0x96, 2, 0, 4, 0x60));
        let (hc, hv) = b.holder(cw, cc);
        out.extend(text(t.names.get(hc as usize).map_or("", |s| s.as_str()), 0x180, 0xA7, 0, 2, 4, 0x60));
        out.extend(text(t.vehicles.get(hv as usize).map_or("", |s| s.as_str()), 0x180, 0xB2, 0, 2, 4, 0x60));
    }
    out.extend(back_button(t, 0x100, 0x1E, 0, false));
    for (l, x, label, need) in [(0usize, 0x80, sp::RACE_A, 0x1E), (2, 0x100, sp::BATTLE, 0x14), (1, 0x180, sp::RACE_B, 0x1E)] {
        let lf = b.league_flags(l);
        if lf & 1 == 0 {
            out.extend(text(q, x, 0xD2, 0, 0, 6, 0x60));
            continue;
        }
        out.extend(text(t.st(label), x, 0xC4, 0, 2, 1, 0x60));
        if lf & 2 == 0 {
            out.extend(text("#7*", x, 0xD2, 0, 0, 4, 0x60));
        } else if b.league_best(l) >= need {
            out.push(star(x - 0xD, 0xCA, 0x60));
        } else {
            out.extend(text("#5_", x, 0xD2, 0, 0, 4, 0x60));
        }
    }
    let mut v: Vec<Prim> = out.into_iter().map(Prim::Sprt).collect();
    v.push(Prim::Shade { x: 0, y: 0, w: 0x200, h: 0xF0 });
    v
}

/// The standings screens (game mode 0x11, FUN_80051698 case 0x11 @
/// 0x800597CC). Page 1, RACE POINTS: the title (0x1E0, 0x1E, right) and
/// the course name (0x1E0, 0xD2), per kart in finishing order a column
/// 62 px apart centred on 0x100: "1ST".."6TH" (y 0x38, colour 4, the
/// player's blinking in colour 5), its place / fastest lap / best start /
/// most hits points (y 0x78, 0x87, 0x96, 0xA5; size 2, colour 5, a 0 in
/// colour 7 semi-transparent) and the total (0xB8, colour 6); a heading
/// ". . . POSITION . . ." on the row it explains, moving down a row a
/// second after the first (the values under it in the two middle columns
/// hidden); SELECT. Page 2, TOURNAMENT POINTS: "AFTER n RACES" / FINAL
/// TABLE, the table in columns 68 px apart (place y 0x4C, points "%i%i"
/// y 0xA4 colour 7); SELECT and BACK.
fn standings(t: &MenuTables, l: &crate::league::League, page: u8, here: Option<(usize, usize)>, f: i32) -> Vec<Sprite> {
    let mut out = Vec::new();
    let n = 6i32;
    let blink = |kart: usize| if kart == 0 && (f & 0x1F) >= 16 { 5 } else { 4 };
    if page == 1 {
        out.extend(t.hud.text(t.st(sp::RACE_POINTS).as_bytes(), 0x1E0, 0x1E, 8, Size::from_arg(0), 4, 0x60));
        let course = here.map(|(w, c)| t.course_names[w * 4 + c].clone()).unwrap_or_default();
        out.extend(t.hud.text(course.as_bytes(), 0x1E0, 0xD2, 8, Size::from_arg(0), 4, 0x60));
        // 0x800B0D98: the mode's frame counter (here the menu's VBlanks)
        let row = if f < 0x3C { 0 } else { ((f - 0x3C) % 0xF0) / 0x3C };
        // (the BATTLE LEAGUE's headings: POSITION, BEST CRASH, MOST
        // KNOCKOUTS, MOST DAMAGE)
        let h = row as usize + if l.league as usize == mr_sim::save::LEAGUE_BATTLE { 4 } else { 0 };
        out.extend(t.hud.text(t.headings[h].as_bytes(), 0x100, row * 0xF + 0x78, 0, Size::from_arg(2), 4, 0x60));
        let mut x = 0x100 - (n - 1) * 0x1F;
        for (i, &k) in l.finish.iter().enumerate() {
            let k = k as usize;
            out.extend(t.hud.text(t.places[i].as_bytes(), x, 0x38, 0, Size::from_arg(0), blink(k), 0x60));
            let p = l.last[k];
            let shown = !(2..4).contains(&i);
            for (r, v) in [p.place, p.fastest_lap, p.best_start, p.most_hits].into_iter().enumerate() {
                if r as i32 != row || shown {
                    let (c, abr) = if v == 0 { (7, 0) } else { (5, 0x60) };
                    out.extend(t.hud.text(format!("{v}").as_bytes(), x, 0x78 + 0xF * r as i32, 0, Size::from_arg(2), c, abr));
                }
            }
            out.extend(t.hud.text(format!("{}", p.total).as_bytes(), x, 0xB8, 0, Size::from_arg(0), 6, 0x60));
            x += 0x3E;
        }
        out.extend(next_button(t));
    } else {
        out.extend(t.hud.text(t.st(sp::TOURNAMENT_POINTS).as_bytes(), 0x1E0, 0x1E, 8, Size::from_arg(0), 4, 0x60));
        let done = l.race as i32 + 1;
        let s = if done < 6 {
            format!("{} {} {}", t.st(sp::AFTER), done, t.st(if done == 1 { sp::RACE } else { sp::RACES }))
        } else {
            t.st(sp::FINAL_TABLE).to_string()
        };
        out.extend(t.hud.text(s.as_bytes(), 0x1E0, 0xD2, 8, Size::from_arg(0), 4, 0x60));
        let mut x = 0x100 - (n - 1) * 0x22;
        for (i, k) in l.table().into_iter().enumerate() {
            out.extend(t.hud.text(t.places[i].as_bytes(), x, 0x4C, 0, Size::from_arg(0), blink(k), 0x60));
            let pts = l.points[k];
            out.extend(t.hud.text(format!("{}{}", pts / 10, pts % 10).as_bytes(), x, 0xA4, 0, Size::from_arg(0), 7, 0x60));
            x += 0x44;
        }
        out.extend(next_button(t));
        out.extend(back_button(t, 0x20, 0x1E, 2, false));
    }
    out
}

fn paused_text(t: &MenuTables, f: i32) -> Vec<Sprite> {
    // shown while 0x800B07D0 % 60 < 45
    if f % 60 < 45 {
        t.hud.text(t.st(sp::PAUSED).as_bytes(), 0x1E0, 0x1E, 8, Size::from_arg(0), 4, 0x60)
    } else {
        Vec::new()
    }
}

/// RACE A / RACE B / BATTLE / STUNT (worlds 0..5: the cup slots, slot 2
/// the arena, slot 3 the stunt course; 0x80058E30, fixed in phase 3),
/// RACE / BATTLE / RACE / BATTLE RACE (worlds 6..).
fn type_label(t: &MenuTables, w: usize, c: usize) -> String {
    let a = if w < 6 {
        [sp::RACE_A, sp::RACE_B, sp::BATTLE, sp::STUNT][c]
    } else {
        [sp::RACE, sp::BATTLE, sp::RACE, sp::BATTLE_RACE][c]
    };
    t.st(a).to_string()
}

/// "BEST LAP m:ss.hh" from a lap record (race clock units / 3 =
/// hundredths; "%s %i:%i%i.%i%i").
pub fn best_lap_text(t: &MenuTables, record: i32) -> String {
    best_text(t, record, 0)
}

/// "BEST LAP" / "BEST TIME" (race types 2 and 3, 0x80052D90) and the
/// course's record.
pub fn best_text(t: &MenuTables, record: i32, race_type: i16) -> String {
    let h = record / 3;
    let m = h / 6000;
    let s = h / 100 - m * 60;
    let c = h % 100;
    let label = if (race_type as u16).wrapping_sub(2) < 2 { sp::BEST_TIME } else { sp::BEST_LAP };
    format!("{} {}:{}{}.{}{}", t.st(label), m, s / 10, s % 10, c / 10, c % 10)
}

/// The port's own settings on PORT SETTINGS (settings::KNOWN rows with a
/// list of values).
pub const PORT_KEYS: &[&str] = &[
    "faithful_view",
    "faithful_draw_distance",
    "aspect",
    "msaa",
    "texture_filter",
    "lighting",
    "fx_blend",
    "logic_hz",
    "low_latency",
    "render_latency",
    "fullscreen",
    "vsync",
    "unlock_all",
];

fn cycle(key: &str, cur: &str, dir: i32) -> Option<String> {
    let o = crate::settings::KNOWN.iter().find(|o| o.key == key)?;
    let vals: Vec<&str> = o.allowed.split('|').filter(|s| !s.is_empty()).collect();
    if vals.is_empty() {
        return None;
    }
    let i = vals.iter().position(|v| v.eq_ignore_ascii_case(cur)).unwrap_or(0) as i32;
    let n = vals.len() as i32;
    Some(vals[((i + dir).rem_euclid(n)) as usize].to_string())
}

/// settings.txt beside the executable (or `--settings PATH`), without the
/// command line's values.
fn file_settings() -> crate::settings::Settings {
    let a: Vec<String> = std::env::args().collect();
    let mut keep = vec![a[0].clone()];
    if let Some(i) = a.iter().position(|x| x == "--settings") {
        keep.extend(a.get(i..i + 2).unwrap_or(&[]).iter().cloned());
    }
    crate::settings::Settings::load_from(&keep)
}

fn save_settings(s: &crate::settings::Settings) {
    let path = s.file.clone().or_else(|| std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("settings.txt"))));
    if let Some(p) = path {
        match std::fs::write(&p, s.to_text()) {
            Ok(()) => info!("menus: settings written to {}", p.display()),
            Err(e) => warn!("menus: cannot write {}: {e}", p.display()),
        }
    }
}

// ---------------------------------------------------------------- input

/// What a frame's input asks the app to do besides the page change.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Effects {
    pub sfx: Vec<(i32, i32, i32, i32)>,
    /// Restart the program on this course in its course menu.
    pub relaunch_course: Option<String>,
    /// Restart the program on the title.
    pub relaunch_title: bool,
    pub unpause: bool,
    pub volumes: Option<[u8; 2]>,
    /// The restart continues a league (`--league`).
    pub league: bool,
}

impl Front {
    /// One frame of menu input (FUN_80073138 + FUN_80074A94 for the page).
    pub fn input(&mut self, buttons: u16, t: &MenuTables) -> Effects {
        let mut fx = Effects::default();
        let pressed = |m: u16| buttons & m != 0 && self.prev & m == 0;
        let (up, down, left, right) = (pressed(UP), pressed(DOWN), pressed(LEFT), pressed(RIGHT));
        let cross = pressed(CROSS);
        let back = pressed(TRIANGLE);
        let start = pressed(START);
        self.prev = buttons;
        if self.page == Page::None {
            return fx;
        }
        let rows = self.rows();
        // UP / DOWN: rows clamp, no wrap; the navigation sound when it moves
        // (FUN_80073138)
        if self.page != Page::CharSelect {
            if up && self.row > 0 {
                self.row -= 1;
                fx.sfx.push(NAV_SFX);
            }
            if down && self.row < rows - 1 {
                self.row += 1;
                fx.sfx.push(NAV_SFX);
            }
        }
        let confirm = |fx: &mut Effects| fx.sfx.push(CONFIRM_SFX);
        match self.page {
            Page::None => {}
            Page::Main => {
                if back {
                    confirm(&mut fx);
                    self.page = Page::None;
                } else if cross {
                    match self.row {
                        1 => {
                            // GAME STATUS (phase 6b) on the first cell
                            confirm(&mut fx);
                            self.open(Page::GameStatus(GameStatusFrom::Main));
                            (self.row, self.col) = (0, 0);
                        }
                        2 => {
                            confirm(&mut fx);
                            self.open(Page::CourseSelect);
                        }
                        3 => {
                            confirm(&mut fx);
                            self.open(Page::Leagues);
                        }
                        4 => {
                            confirm(&mut fx);
                            self.open(Page::Options);
                        }
                        _ => {}
                    }
                }
            }
            Page::Options => {
                if back {
                    confirm(&mut fx);
                    self.open(Page::Main);
                } else if cross {
                    match self.row {
                        0 => {
                            confirm(&mut fx);
                            self.sound_from = Page::Options;
                            self.open(Page::Sound);
                        }
                        1 => {
                            confirm(&mut fx);
                            self.port = Some(file_settings());
                            self.open(Page::Controllers);
                        }
                        4 => {
                            confirm(&mut fx);
                            self.port = Some(file_settings());
                            self.open(Page::PortSettings);
                        }
                        _ => {}
                    }
                }
            }
            Page::Sound => {
                // FUN_80074850: LEFT / RIGHT 0..15 with the nav sound at
                // (0x60, 0x60); CROSS keeps, TRIANGLE restores
                let i = self.row as usize;
                let d = right as i32 - left as i32;
                if d != 0 {
                    let v = (self.sound[i] as i32 + d).clamp(0, 15) as u8;
                    if v != self.sound[i] {
                        self.sound[i] = v;
                        fx.sfx.push((0xE, 0x1800, 0x60, 0x60));
                        fx.volumes = Some(self.sound);
                    }
                }
                if cross || back {
                    confirm(&mut fx);
                    let mut st = file_settings();
                    if cross {
                        let _ = st.set("music_volume", &self.sound[0].to_string());
                        let _ = st.set("sound_volume", &self.sound[1].to_string());
                        save_settings(&st);
                    } else {
                        self.sound = [st.i64("music_volume", 12).clamp(0, 15) as u8, st.i64("sound_volume", 12).clamp(0, 15) as u8];
                        fx.volumes = Some(self.sound);
                    }
                    let to = self.sound_from;
                    self.open(to);
                    if to == Page::Pause {
                        self.row = 3;
                    }
                }
            }
            Page::Controllers => {
                // phase 7: controller 2's configuration with pad 2's LEFT /
                // RIGHT (FUN_80073B0C's second column)
                let p2 = self.pad2 & !self.prev2 & (LEFT | RIGHT) != 0;
                self.prev2 = self.pad2;
                if p2 {
                    if let Some(st) = self.port.as_mut() {
                        let alt = st.get("controller_config_2") == Some("B");
                        let _ = st.set("controller_config_2", if alt { "A" } else { "B" });
                        fx.sfx.push(NAV_SFX);
                    }
                }
                if self.row == 0 && (left || right) {
                    if let Some(st) = self.port.as_mut() {
                        let alt = st.get("controller_config") == Some("B");
                        let _ = st.set("controller_config", if alt { "A" } else { "B" });
                        fx.sfx.push(NAV_SFX);
                    }
                }
                if cross || back {
                    confirm(&mut fx);
                    if cross {
                        if let Some(st) = self.port.take() {
                            save_settings(&st);
                        }
                    }
                    self.port = None;
                    self.open(Page::Options);
                    self.row = 1;
                }
            }
            Page::PortSettings => {
                let d = right as i32 - left as i32;
                if d != 0 {
                    if let Some(st) = self.port.as_mut() {
                        let key = PORT_KEYS[self.row as usize];
                        if let Some(v) = cycle(key, st.get(key).unwrap_or(""), d) {
                            let _ = st.set(key, &v);
                            fx.sfx.push(NAV_SFX);
                        }
                    }
                }
                if cross || back {
                    confirm(&mut fx);
                    if cross {
                        if let Some(st) = self.port.take() {
                            save_settings(&st);
                            let mut b = crate::save::with(|s| s.block.clone());
                            if st.bool("unlock_all", false) {
                                crate::save::with(|s| b.unlock_all(&s.tables));
                            }
                            self.progress = Progress(b);
                        }
                    }
                    self.port = None;
                    self.open(Page::Options);
                    self.row = 4;
                }
            }
            Page::Leagues => {
                // FUN_80074A94 sub-state 2: LEFT / RIGHT laps 1..10 of an
                // open, won race league; CROSS starts an open league
                let l = self.row as usize;
                let flags = self.progress.0.league_flags(l);
                let d = right as i32 - left as i32;
                if d != 0 && l != mr_sim::save::LEAGUE_BATTLE && flags & 3 == 3 {
                    let old = self.progress.0.league_laps(l);
                    let n = (old as i32 + d).clamp(1, 10) as u8;
                    if n != old {
                        fx.sfx.push(NAV_SFX);
                        self.progress.0.set_league_laps(l, n);
                        crate::save::with(|s| s.block.set_league_laps(l, n));
                        crate::save::persist();
                    }
                }
                if back {
                    confirm(&mut fx);
                    self.open(Page::Main);
                    self.row = 3;
                } else if cross && flags & 1 != 0 {
                    confirm(&mut fx);
                    let tables = crate::save::with(|s| s.tables.clone());
                    let mut seed = league_seed();
                    let mut rand = || mr_sim::session::bios_rand(&mut seed);
                    let lg = crate::league::League::new(l as u8, &tables, &mut rand);
                    lg.store();
                    fx.relaunch_course = crate::courses::races().iter().find(|i| i.id == lg.courses[0]).map(|i| i.name.clone());
                    fx.league = true;
                    self.league = Some(lg);
                }
            }
            Page::Standings(page) => {
                if back && page == 2 {
                    confirm(&mut fx);
                    self.open(Page::Standings(1));
                } else if cross && page == 1 {
                    confirm(&mut fx);
                    self.open(Page::Standings(2));
                } else if cross {
                    confirm(&mut fx);
                    if let Some(mut lg) = self.league.take() {
                        // 0x8007916C: after a 1-player league race the
                        // progression check (FUN_8008BE68)
                        crate::save::between_courses();
                        lg.race += 1;
                        if lg.race < 6 {
                            lg.store();
                            fx.relaunch_course = crate::courses::races().iter().find(|i| i.id == lg.courses[lg.race as usize]).map(|i| i.name.clone());
                            fx.league = true;
                            self.league = Some(lg);
                        } else {
                            // FUN_8008C4AC @0x8008CE98: the league's result
                            let table = lg.table();
                            let l = lg.league as usize;
                            let fanfare = crate::save::with(|s| s.block.league_end(l, table[0] == 0, lg.points[0]));
                            if fanfare {
                                fx.sfx.push((0x24, 5000, 0x80, 0x80));
                            }
                            crate::save::between_courses();
                            crate::league::League::clear();
                            fx.relaunch_title = true;
                        }
                    }
                }
            }
            Page::LeagueMenu => {
                // FUN_80074A94 0x800B0EA2 = 4 (CROSS only)
                if cross {
                    confirm(&mut fx);
                    match self.row {
                        0 => {
                            self.page = Page::None;
                            self.request = Request::Race;
                        }
                        1 => self.open(Page::LeaguePoints),
                        _ => self.open(Page::LeagueQuit),
                    }
                }
            }
            Page::LeaguePoints => {
                if back || cross {
                    confirm(&mut fx);
                    self.open(Page::LeagueMenu);
                    self.row = 1;
                }
            }
            Page::LeagueQuit => {
                if back || (cross && self.row == 1) {
                    confirm(&mut fx);
                    self.open(Page::LeagueMenu);
                    self.row = 2;
                } else if cross {
                    // YES: 0x800B07E4 = 0, the course menu
                    confirm(&mut fx);
                    self.league = None;
                    crate::league::League::clear();
                    self.open(Page::CourseMenu);
                }
            }
            Page::CourseSelect => {
                let (w, c) = (self.row as usize, self.col as usize);
                if left && self.col > 0 {
                    self.col -= 1;
                    fx.sfx.push(NAV_SFX);
                }
                if right && self.col < 3 {
                    self.col += 1;
                    fx.sfx.push(NAV_SFX);
                }
                if back {
                    confirm(&mut fx);
                    self.open(Page::Main);
                    self.row = 2;
                } else if cross {
                    let course = t.cups[w * 4 + c];
                    if self.progress.course_open(w, c) && is_race(course) {
                        confirm(&mut fx);
                        fx.relaunch_course = crate::courses::races().iter().find(|i| i.id as i8 == course).map(|i| i.name.clone());
                    }
                }
            }
            Page::CourseMenu => {
                // FUN_80074A94 0x800B0EA2 = 0: LEFT / RIGHT on the race rows
                // change the laps (1..10) once the course is won
                // (row 1, 2 PLAYER RACE: its own laps 0x800E038D)
                let d = right as i32 - left as i32;
                if d != 0 && self.row < 2 && self.race_type() < 2 && self.here.is_some_and(|(w, c)| self.progress.0.course_won(w, c)) {
                    let lp = self.row.max(0) as usize;
                    let old = self.progress.0.laps(lp);
                    let n = (old as i32 + d).clamp(1, 10) as u8;
                    if n != old {
                        fx.sfx.push(NAV_SFX);
                        self.progress.0.set_laps(lp, n);
                        crate::save::with(|s| s.block.set_laps(lp, n));
                        crate::save::persist();
                    }
                }
                let world = self.here.map(|h| h.0);
                if cross && world.is_some_and(|w| w >= 7) {
                    // FUN_80074A94 sub-states 0xB / 0xC: world 7's REVERSE
                    // RACE (2), BATTLE (1), BATTLE RACE (8) through the
                    // character select; BACK TO CREDITS
                    let w7 = world == Some(7);
                    let flags = match self.row {
                        2 if w7 => Some(2),
                        3 if w7 => Some(1),
                        4 if w7 => Some(8),
                        _ => None,
                    };
                    // phase 7: world 7's 2 PLAYER RACE (row 1) and CHICKEN
                    // CHASE (row 5); cup 8's 2 PLAYER BATTLE (row 1) and
                    // CHICKEN CHASE (row 2)
                    let two = match self.row {
                        1 => Some(None),
                        5 if w7 => Some(Some(0x40)),
                        2 if !w7 => Some(Some(0x40)),
                        _ => None,
                    };
                    if let Some(f) = two {
                        confirm(&mut fx);
                        self.secret = f.map(|f| Secret { race_flags: f, boss: false });
                        self.start_select(2);
                    } else if self.row == 0 || flags.is_some() {
                        confirm(&mut fx);
                        self.players = 1;
                        self.secret = flags.map(|f| Secret { race_flags: f, boss: false });
                        self.open(Page::CharSelect);
                        self.request = Request::Menu { keep_camera: true };
                    } else if self.row == if w7 { 6 } else { 3 } {
                        confirm(&mut fx);
                        fx.relaunch_title = true;
                    }
                } else if cross {
                    match self.row {
                        0 => {
                            confirm(&mut fx);
                            self.players = 1;
                            self.open(Page::CharSelect);
                            self.request = Request::Menu { keep_camera: true };
                        }
                        // phase 7: 2 PLAYER RACE / BATTLE / STUNT RACE
                        1 => {
                            confirm(&mut fx);
                            self.secret = None;
                            self.start_select(2);
                        }
                        2 if self.secrets_offered() => {
                            confirm(&mut fx);
                            self.open(Page::Secrets);
                        }
                        3 => {
                            confirm(&mut fx);
                            self.open(Page::GameMenu);
                            self.row = 2;
                        }
                        4 => {
                            confirm(&mut fx);
                            fx.relaunch_title = true;
                        }
                        _ => {}
                    }
                }
            }
            Page::GameStatus(from) => {
                // FUN_80073B0C: LEFT / RIGHT the column 0..3 (the sound when
                // it moves), UP / DOWN the world 0..6 (the menu rows);
                // TRIANGLE back
                let d = right as i32 - left as i32;
                let c = (self.col as i32 + d).clamp(0, 3) as i8;
                if c != self.col {
                    self.col = c;
                    fx.sfx.push(NAV_SFX);
                }
                if back {
                    confirm(&mut fx);
                    match from {
                        GameStatusFrom::Main => {
                            self.open(Page::Main);
                            self.row = 1;
                        }
                        GameStatusFrom::GameMenu => {
                            self.open(Page::GameMenu);
                            self.row = 1;
                        }
                    }
                }
            }
            Page::Secrets => {
                let rows = self.secret_rows(t);
                // LEFT / RIGHT on BATTLE AS BOSS: the boss (0x800B07F3,
                // 0..0x800B0A61 - 1)
                let d = right as i32 - left as i32;
                if d != 0 && self.row == 1 && self.race_type() == 2 && self.course != 0x1B && rows[1].open {
                    let n = mr_sim::secrets::bosses(self.course).1 as i32;
                    let b = (self.boss_pick as i32 + d).clamp(0, (n - 1).max(0)) as u8;
                    if b != self.boss_pick {
                        self.boss_pick = b;
                        fx.sfx.push(NAV_SFX);
                    }
                }
                if back {
                    confirm(&mut fx);
                    self.open(Page::CourseMenu);
                    self.row = 2;
                } else if cross {
                    let r = &rows[self.row.clamp(0, 3) as usize];
                    match (r.open, r.action) {
                        (true, SecretAction::TwoPlayers(m)) => {
                            confirm(&mut fx);
                            self.secret = Some(m);
                            self.start_select(2);
                        }
                        (true, SecretAction::CharSelect(m)) => {
                            confirm(&mut fx);
                            self.players = 1;
                            self.secret = Some(m);
                            self.open(Page::CharSelect);
                            self.request = Request::Menu { keep_camera: true };
                        }
                        (true, SecretAction::Race(m)) => {
                            confirm(&mut fx);
                            self.secret = Some(m);
                            self.page = Page::None;
                            self.request = Request::Race;
                        }
                        _ => {}
                    }
                }
            }
            Page::GameMenu => {
                if back {
                    confirm(&mut fx);
                    self.open(Page::CourseMenu);
                    self.row = 3;
                } else if cross && self.row == 1 {
                    // FUN_80074A94 @0x80077A00: GAME STATUS on the course's
                    // cell (0x800B07FA / 0x800B07FC)
                    confirm(&mut fx);
                    let (w, c) = self.here.unwrap_or((0, 0));
                    self.open(Page::GameStatus(GameStatusFrom::GameMenu));
                    (self.row, self.col) = (w.min(6) as i8, c.min(3) as i8);
                } else if cross && self.row == 2 {
                    confirm(&mut fx);
                    self.open(Page::WorldList);
                } else if cross && self.row == 0 {
                    // 0x80077958: ADVENTURE (0x800B07E6 = 1), the character
                    // select (0x800B07CB = 1); its CROSS starts game mode
                    // 0xB (FUN_80073E44 case 6).
                    confirm(&mut fx);
                    self.adventure = true;
                    self.open(Page::CharSelect);
                    self.request = Request::Menu { keep_camera: true };
                }
            }
            Page::WorldList => {
                if back {
                    confirm(&mut fx);
                    self.open(Page::GameMenu);
                    self.row = 2;
                } else if cross {
                    confirm(&mut fx);
                    let w = self.row;
                    self.open(Page::CourseList);
                    self.col = w; // the world (0x800B0F12)
                    self.row = match self.here {
                        Some((hw, ht)) if hw as i8 == w => ht as i8,
                        _ => 0,
                    };
                }
            }
            Page::CourseList => {
                let w = self.col as usize;
                if back {
                    confirm(&mut fx);
                    self.open(Page::WorldList);
                    self.row = w as i8;
                } else if cross {
                    let c = self.row as usize;
                    let course = t.cups[w * 4 + c];
                    if course == self.course as i8 {
                        confirm(&mut fx);
                        self.open(Page::CourseMenu);
                    } else if self.progress.course_open(w, c) && is_race(course) {
                        confirm(&mut fx);
                        fx.relaunch_course = crate::courses::races().iter().find(|i| i.id as i8 == course).map(|i| i.name.clone());
                    }
                }
            }
            Page::CharSelect => {
                let n_c = t.char_list.len() as i8;
                let n_v = t.vehicle_list.len() as i8;
                let mut changed = false;
                if up && self.row > 0 {
                    self.row -= 1;
                    changed = true;
                }
                if down && self.row < n_c - 1 {
                    self.row += 1;
                    changed = true;
                }
                if changed {
                    // FUN_8008B488 + FUN_80072C28(p, 0): the new character
                    // in its own vehicle
                    fx.sfx.push(NAV_SFX);
                    self.character = t.char_list[self.row as usize];
                    self.vehicle = t.default_vehicle[self.character as usize];
                    self.col = t.vehicle_list.iter().position(|&v| v == self.vehicle).unwrap_or(0) as i8;
                    self.request = Request::Menu { keep_camera: true };
                }
                let d = (right && self.col < n_v - 1) as i8 - (left && self.col > 0) as i8;
                if d != 0 {
                    // FUN_8008B644: the vehicle
                    self.col += d;
                    self.vehicle = t.vehicle_list[self.col as usize];
                    fx.sfx.push(NAV_SFX);
                    self.request = Request::Menu { keep_camera: true };
                }
                if back && self.picking == 1 {
                    // phase 7: back to player 1's pick
                    confirm(&mut fx);
                    self.picking = 0;
                    (self.character, self.vehicle) = self.p1_pick;
                    let (r, c) = self.char_cursor();
                    (self.row, self.col) = (r, c);
                    self.request = Request::Menu { keep_camera: true };
                } else if back {
                    confirm(&mut fx);
                    self.picking = 0;
                    if self.league.as_ref().is_some_and(|l| !l.started) {
                        // leaving the league's character select ends it
                        self.league = None;
                        crate::league::League::clear();
                    }
                    self.adventure = false;
                    let from_secrets = self.secret.take().is_some();
                    self.open(if from_secrets && self.secrets_offered() { Page::Secrets } else { Page::CourseMenu });
                    self.request = Request::Menu { keep_camera: true };
                } else if cross && self.progress.char_open(self.character) && self.progress.vehicle_open(self.character, self.vehicle) && self.players == 2 && self.picking == 0 {
                    // phase 7: player 1 has chosen; player 2's turn
                    confirm(&mut fx);
                    self.p1_pick = (self.character, self.vehicle);
                    self.picking = 1;
                    (self.character, self.vehicle) = (self.p2_character, self.p2_vehicle);
                    let (r, c) = self.char_cursor();
                    (self.row, self.col) = (r, c);
                    self.request = Request::Menu { keep_camera: true };
                } else if cross && self.progress.char_open(self.character) && self.progress.vehicle_open(self.character, self.vehicle) && self.picking == 1 {
                    // both have chosen: the race
                    confirm(&mut fx);
                    (self.p2_character, self.p2_vehicle) = (self.character, self.vehicle);
                    (self.character, self.vehicle) = self.p1_pick;
                    self.picking = 0;
                    self.page = Page::None;
                    self.request = Request::Race;
                } else if cross && self.progress.char_open(self.character) && self.progress.vehicle_open(self.character, self.vehicle) {
                    // FUN_80073C58: the character (and its vehicle) collected
                    confirm(&mut fx);
                    if let Some(lg) = self.league.as_mut().filter(|l| !l.started) {
                        // FUN_8008BAF8: the league's opponents
                        let exe = mr_sim::tables::ExeImage::parse(&league_exe()).expect("parse the executable");
                        let mut seed = league_seed();
                        let mut rand = || mr_sim::session::bios_rand(&mut seed);
                        lg.draw_karts(&exe, self.character, self.vehicle, &mut rand);
                        lg.store();
                        // FUN_80073E44: on the league's first course, its
                        // menu (0x800B0EA2 = 4)
                        self.open(Page::LeagueMenu);
                        self.request = Request::Menu { keep_camera: true };
                        return fx;
                    }
                    self.page = Page::None;
                    self.request = Request::Race;
                }
            }
            Page::Pause => {
                if start || (cross && self.row == 0) {
                    confirm(&mut fx);
                    self.page = Page::None;
                    fx.unpause = true;
                } else if cross {
                    confirm(&mut fx);
                    match self.row {
                        1 => self.open(Page::Confirm(false)),
                        2 => self.open(Page::Confirm(true)),
                        _ => {
                            self.sound_from = Page::Pause;
                            self.open(Page::Sound);
                        }
                    }
                }
            }
            Page::Confirm(restart) => {
                if back || (cross && self.row == 1) {
                    confirm(&mut fx);
                    self.open(Page::Pause);
                    self.row = if restart { 2 } else { 1 };
                } else if cross {
                    confirm(&mut fx);
                    fx.unpause = true;
                    if restart {
                        self.page = Page::None;
                        self.request = Request::Race;
                    } else {
                        if self.league.take().is_some() {
                            // RETIRE in a league race quits the league
                            crate::league::League::clear();
                        }
                        self.to_course_menu();
                    }
                }
            }
        }
        fx
    }
}

/// The executable (the league's opponent draw reads its tables).
fn league_exe() -> Vec<u8> {
    let mut disc = mr_disc::Disc::open_default().expect("open the disc");
    disc.read("SLUS_012.37").expect("read the executable")
}

/// A rand() seed for the league draws (the game's comes from everything
/// before; any value gives a valid draw).
fn league_seed() -> u32 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u32).unwrap_or(1) | 1
}

// ---------------------------------------------------------------- plugin

pub struct MenusPlugin;

impl Plugin for MenusPlugin {
    fn build(&self, app: &mut App) {
        let mut front = Front::default();
        if std::env::args().any(|a| a == "--menu") && !crate::title::on_title() {
            front.page = Page::CourseMenu;
            // a league: its character select first, then each race at once
            match front.league.as_ref().map(|l| l.started) {
                Some(false) => front.open(Page::CharSelect),
                Some(true) => front.open(Page::LeagueMenu),
                None => {}
            }
        }
        app.insert_resource(front).add_systems(Startup, apply_saved_volumes).add_systems(Update, (run, overlay.after(run), pause.after(run)));
        if crate::settings::arg_value("--menu-keys").is_some() || crate::settings::arg_value("--menu-shots").is_some() {
            app.add_systems(PreUpdate, script_keys.after(bevy::input::InputSystems).after(crate::pacing::sample)).add_systems(Update, script_shots.after(overlay));
        }
    }
}

/// Checks without a pad: `--menu-keys "VB:buttons,..."` holds the buttons
/// (cross, start, down+cross, ... as tools/oracle routes; empty = none) from
/// menu VBlank VB (0x800B07D4 as `Front::vb` counts it, from the start);
/// `--menu-shots "VB:PATH,..."` captures the window at those VBlanks and
/// exits after the last (analysis/s7/menus side-by-sides).
fn parse_steps(spec: &str) -> Vec<(i32, String)> {
    spec.split(',').filter_map(|s| s.split_once(':')).filter_map(|(v, b)| Some((v.trim().parse().ok()?, b.trim().to_string()))).collect()
}

fn script_keys(front: Res<Front>, mut latch: ResMut<crate::pacing::PadLatch>) {
    let Some(spec) = crate::settings::arg_value("--menu-keys") else { return };
    let names = ["select", "", "", "start", "up", "right", "down", "left", "l2", "r2", "l1", "r1", "triangle", "circle", "cross", "square"];
    let cur = parse_steps(&spec).into_iter().take_while(|(v, _)| *v <= front.vb).last();
    if let Some((_, b)) = cur {
        for n in b.split('+').filter(|x| !x.is_empty()) {
            if let Some(i) = names.iter().position(|x| *x == n) {
                latch.now |= 1 << i;
            }
        }
    }
}

fn script_shots(front: Res<Front>, mut done: Local<usize>, mut wait: Local<u32>, mut c: Commands, mut exit: MessageWriter<AppExit>) {
    let Some(spec) = crate::settings::arg_value("--menu-shots") else { return };
    let shots = parse_steps(&spec);
    if *done >= shots.len() {
        *wait += 1;
        if *wait > 30 {
            exit.write(AppExit::Success);
        }
        return;
    }
    let (vb, path) = shots[*done].clone();
    if front.vb >= vb {
        c.spawn(bevy::render::view::screenshot::Screenshot::primary_window())
            .observe(move |ev: On<bevy::render::view::screenshot::ScreenshotCaptured>| crate::write_png(&ev.image, &path));
        info!("menus: shot at vb {} page {:?} row {}", front.vb, front.page, front.row);
        *done += 1;
    }
}

fn apply_saved_volumes(bus: Option<Res<crate::audio::AudioBus>>, front: Res<Front>) {
    if let Some(bus) = bus {
        crate::audio::apply_game_volumes(&bus, tables().music_gain(front.sound[0]), tables().sound_gain(front.sound[1]));
    }
}

/// Tool runs (routes, replays, oracle grids, shots) never see the menus.
pub fn tool_run() -> bool {
    let a: Vec<String> = std::env::args().collect();
    let tool = ["--oracle-grid", "--route", "--replay", "--replay-capture", "--shot", "--fx-shots", "--latency-log", "--autopilot", "--record-route", "--trace-out", "--ghost"];
    a.iter().any(|x| tool.contains(&x.as_str()))
}

#[allow(clippy::too_many_arguments)]
fn run(
    mut front: ResMut<Front>,
    latch: Res<crate::pacing::PadLatch>,
    real: Res<Time<Real>>,
    mut sfx: ResMut<crate::sfx::Sfx>,
    bus: Option<Res<crate::audio::AudioBus>>,
    mut exit: MessageWriter<AppExit>,
    p: Res<crate::Player>,
) {
    // 0x800B07D4: VBlanks at 60 per second
    front.vb_frac += real.delta_secs() * 60.0;
    let n = front.vb_frac.floor();
    front.vb_frac -= n;
    front.vb = front.vb.wrapping_add(n as i32);
    // START in a race: the pause menu (not on the results, not before GO's
    // race frames exist, never in tool runs)
    let racing = matches!(&p.driver, crate::Driver::Sim(run) if !matches!(run.session.phase(), mr_sim::race::RacePhase::Finished { .. }));
    if front.page == Page::None && !crate::title::on_title() && racing && !tool_run() && !crate::demo::active() {
        // (phase 7: either player's START pauses a split-screen race)
        let b = latch.now | if crate::input::two_players() { latch.now2 & START } else { 0 };
        if b & START != 0 && front.prev & START == 0 {
            front.open(Page::Pause);
            sfx.play(CONFIRM_SFX);
            if let Some(bus) = bus.as_ref() {
                bus.music(crate::audio::MusicCmd::Pause);
            }
            return;
        }
        front.prev = b;
        return;
    }
    if front.page == Page::None {
        front.prev = latch.now;
        return;
    }
    let t = tables();
    front.pad2 = latch.now2;
    let b = if front.page == Page::CharSelect && front.picking == 1 { latch.now | latch.now2 } else { latch.now };
    let fx = front.input(b, t);
    for s in &fx.sfx {
        sfx.play(*s);
    }
    if let Some(v) = fx.volumes {
        if let Some(bus) = bus.as_ref() {
            crate::audio::apply_game_volumes(bus, t.music_gain(v[0]), t.sound_gain(v[1]));
        }
    }
    if fx.unpause {
        if let Some(bus) = bus.as_ref() {
            bus.music(crate::audio::MusicCmd::Resume);
        }
    }
    if fx.relaunch_course.is_some() || fx.relaunch_title {
        // main(): between course sessions the progression check
        // (FUN_8008BE68); outside a league a newly opened course is loaded
        // next (0x800B0800 -> 0x800B0AF4) when the port runs it
        let mut course = fx.relaunch_course.clone();
        if let Some(c) = crate::save::between_courses().filter(|_| !fx.league) {
            if let Some(i) = crate::courses::races().iter().find(|i| i.id == c) {
                info!("save: course {} opened; loading it", i.name);
                course = Some(i.name.clone());
            }
        }
        relaunch(course.as_deref(), front.character, front.vehicle, fx.league);
        exit.write(AppExit::Success);
    }
}

/// The menu layer over the scene (HudOverlay replaces the race HUD while
/// set); off the menus the title's own layer (PRESS START).
fn overlay(front: Res<Front>, title: Res<crate::title::TitleOverlay>, mut ov: ResMut<crate::hud::HudOverlay>, mut sfx: ResMut<crate::sfx::Sfx>, mut dir: ResMut<crate::audio::MusicDirector>) {
    let want = if front.page != Page::None { Some(page_prims(&front, tables())) } else { title.0.clone() };
    if ov.prims != want {
        ov.prims = want;
    }
    // the race's sound driver keeps running on the menus whose session does
    // not tick (sfx.rs), and the course menu plays the course track without
    // the intro clip (audio.rs)
    sfx.idle = front.page.course_menu() || front.page.pauses_race();
    dir.menu = front.page.course_menu();
}

/// The race freezes under the pause menu (virtual time).
fn pause(front: Res<Front>, mut time: ResMut<Time<Virtual>>, mut paused_here: Local<bool>) {
    if front.page.pauses_race() && !time.is_paused() {
        time.pause();
        *paused_here = true;
    } else if !front.page.pauses_race() && *paused_here && time.is_paused() {
        time.unpause();
        *paused_here = false;
    }
}

/// Start this program again on `course` in its course menu (`--menu`), or
/// on the title (None), keeping the other arguments and the character /
/// vehicle chosen. (The scene is built at start-up; the game loads the
/// course behind its LOADING screen.)
pub fn relaunch(course: Option<&str>, character: u8, vehicle: u8, league: bool) {
    let a: Vec<String> = std::env::args().collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 1;
    while i < a.len() {
        match a[i].as_str() {
            "--course" | "--character" | "--vehicle" | "--menu-keys" | "--menu-shots" => i += 2,
            "--skip-title" | "--menu" | "--league" | "--demo" => i += 1,
            _ => {
                out.push(a[i].clone());
                i += 1;
            }
        }
    }
    if let Some(c) = course {
        out.extend(["--course".into(), c.to_string(), "--skip-title".into(), "--menu".into()]);
    }
    out.extend(["--character".into(), character.to_string(), "--vehicle".into(), vehicle.to_string()]);
    if league {
        out.push("--league".into());
    }
    match std::env::current_exe() {
        Ok(exe) => {
            if let Err(e) = std::process::Command::new(exe).args(&out).spawn() {
                eprintln!("menus: relaunch failed: {e}");
            }
        }
        Err(e) => eprintln!("menus: relaunch failed: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exe() -> Option<Vec<u8>> {
        let mut d = mr_disc::Disc::open_default().ok()?;
        d.read("SLUS_012.37").ok()
    }

    #[test]
    fn tables_and_lists() {
        let Some(exe) = exe() else { return };
        let t = MenuTables::from_exe(&exe);
        // the character select's lists equal the oracle's RAM 0x800AC6A4 /
        // 0x800AC6C0 (BOG course menu, analysis/oracle/courses/BOG/ram0.bin)
        assert_eq!(t.char_list, [0, 2, 1, 5, 4, 3, 7, 6, 15, 18, 17, 16, 8, 10, 9, 12, 13, 14, 11, 19, 22, 23, 20, 21, 24]);
        assert_eq!(t.vehicle_list, [0, 2, 1, 30, 4, 7, 6, 5, 14, 17, 16, 15, 3, 9, 8, 28, 12, 13, 10, 27, 11, 18, 29, 26, 25]);
        assert_eq!(t.music_levels[12], 0x6C);
        assert_eq!(t.sound_levels[12], 0x60);
        assert_eq!(best_lap_text(&t, 12186), "BEST LAP 0:40.62");
        // arrows: angle 0x800 points down, 0x400 right
        let Prim::F3 { v, .. } = t.arrow(100, 100, 0x800, [0; 3], 5, 5) else { panic!() };
        assert_eq!(v[0], [100, 104], "cos(0x800) = -16383");
        let Prim::F3 { v, .. } = t.arrow(100, 100, 0x400, [0; 3], 5, 5) else { panic!() };
        assert_eq!(v[0], [108, 100]);
    }

    #[test]
    fn cursor_and_pages() {
        let Some(exe) = exe() else { return };
        let t = MenuTables::from_exe(&exe);
        let mut f = Front { progress: Progress::new_game(), ..Default::default() };
        f.open(Page::Main);
        let press = |f: &mut Front, b: u16| {
            f.input(0, &t);
            f.input(b, &t)
        };
        assert_eq!(f.row, 0);
        press(&mut f, DOWN);
        press(&mut f, DOWN);
        assert_eq!(f.row, 2);
        // MEET THE MUPPETS is not ported: CROSS on row 0 does nothing
        let fx = press(&mut f, CROSS);
        assert_eq!(f.page, Page::CourseSelect, "{fx:?}");
        // locked: the secret courses on the oracle's save
        for _ in 0..6 {
            press(&mut f, DOWN);
        }
        assert_eq!(f.row, 6);
        let fx = press(&mut f, CROSS);
        assert_eq!(fx.relaunch_course, None);
        for _ in 0..3 {
            press(&mut f, UP);
        }
        let fx = press(&mut f, CROSS);
        assert_eq!(fx.relaunch_course.as_deref(), Some("LONDON"), "THE MUPPET CHRISTMAS CAROL / RACE A");
        // the character select: Kermit, RIGHT does not leave Kermit's own
        // vehicle on the oracle's save; DOWN to Miss Piggy
        f.open(Page::CharSelect);
        assert_eq!((f.row, f.col), (0, 0));
        press(&mut f, DOWN);
        assert_eq!((f.character, f.vehicle), (2, 2));
        press(&mut f, RIGHT);
        let fx = press(&mut f, CROSS);
        assert!(!fx.sfx.contains(&CONFIRM_SFX), "MONSTER TRUCK... not won by Miss Piggy");
        press(&mut f, LEFT);
        press(&mut f, CROSS);
        assert_eq!(f.request, Request::Race);
        // pause: RETIRE asks, NO first
        f.open(Page::Pause);
        press(&mut f, DOWN);
        press(&mut f, CROSS);
        assert_eq!((f.page, f.row), (Page::Confirm(false), 1));
        press(&mut f, UP);
        press(&mut f, CROSS);
        assert_eq!(f.page, Page::CourseMenu);
    }

    #[test]
    fn leagues_and_standings() {
        let Some(exe) = exe() else { return };
        let t = MenuTables::from_exe(&exe);
        let mut f = Front { progress: Progress::new_game(), league: None, ..Default::default() };
        let press = |f: &mut Front, b: u16| {
            f.input(0, &t);
            f.input(b, &t)
        };
        f.open(Page::Main);
        f.row = 3;
        press(&mut f, CROSS);
        assert_eq!(f.page, Page::Leagues);
        // RANDOM RACE LEAGUE is closed on a new game ("?")
        f.row = 3;
        let fx = press(&mut f, CROSS);
        assert_eq!(fx.relaunch_course, None);
        // laps stay 3 until the league is won
        f.row = 0;
        press(&mut f, RIGHT);
        assert_eq!(f.progress.0.league_laps(0), 3);
        let fx = press(&mut f, CROSS);
        assert_eq!(fx.relaunch_course.as_deref(), Some("BOG"), "RACE LEAGUE A starts on BOG");
        assert!(fx.league);
        let lg = f.league.clone().unwrap();
        assert_eq!(lg.courses, [8, 9, 16, 1, 3, 15]);
        // its character select draws the opponents and opens the league
        // menu; START RACE 1 starts the race
        f.open(Page::CharSelect);
        press(&mut f, CROSS);
        assert_eq!(f.page, Page::LeagueMenu);
        assert!(f.league.as_ref().is_some_and(|l| l.started && l.karts[1..].iter().all(|k| k.0 != 0)));
        assert!(!page_prims(&f, &t).is_empty());
        f.open(Page::LeaguePoints);
        assert!(!page_prims(&f, &t).is_empty());
        f.open(Page::LeagueMenu);
        press(&mut f, CROSS);
        assert_eq!(f.request, Request::Race);
        // after the last race: RACE POINTS -> TOURNAMENT POINTS -> the result
        let mut lg = lg;
        lg.started = true;
        lg.race = 5;
        lg.points = [30, 20, 10, 5, 2, 1];
        f.league = Some(lg);
        f.open(Page::Standings(1));
        assert!(!page_prims(&f, &t).is_empty());
        press(&mut f, CROSS);
        assert_eq!(f.page, Page::Standings(2));
        let fx = press(&mut f, CROSS);
        assert!(fx.relaunch_title);
        assert!(crate::save::with(|s| s.block.league_flags(0) & 2 != 0), "the player topped the table: league A won");
        assert_eq!(crate::save::with(|s| s.block.league_best(0)), 30);
    }

    /// The main menu and the character select against the oracle's GP0
    /// frames (tools/oracle/menu_walk.py; analysis/s7/menus/oracle/main.json,
    /// charsel.json, local): every font sprite of the menu at the same
    /// place, texture, CLUT and raw / modulated, the modulated ones'
    /// colours equal except the pulse.
    #[test]
    fn gp0_main_menu_and_char_select() {
        let Some(exe) = exe() else { return };
        let root = mr_sim::oracle::repo_root();
        let t = MenuTables::from_exe(&exe);
        let mut disc = mr_disc::Disc::open_default().unwrap();
        // (file, course, page, row, menu frame): the oracle walks of
        // analysis/s7/menus/oracle (menu_walk.py; DOCK = slot 1's course)
        let cases = [
            ("main.json", 0x1Eu8, Page::Main, 0, 0),
            ("charsel.json", 8, Page::CharSelect, 0, 0),
            ("coursemenu.json", 8, Page::CourseMenu, 0, 0),
            ("gamemenu.json", 3, Page::GameMenu, 2, 0),
            ("worldlist.json", 3, Page::WorldList, 4, 0),
            ("courselist.json", 3, Page::CourseList, 0, 0),
            ("pause.json", 3, Page::Pause, 0, 50),
        ];
        for (file, course, page, row, vb) in cases {
            let Ok(text) = std::fs::read_to_string(root.join("analysis/s7/menus/oracle").join(file)) else { continue };
            let course32 = course as u32;
            let path = mr_assets::raw::course_raw_path(&exe, course32).unwrap();
            let env = mr_assets::raw::LoadEnv::from_exe(&exe, course32).unwrap();
            let dec = mr_assets::raw::decode(&disc.read(&path).unwrap(), &env).unwrap();
            let slots = mr_assets::hud::slot_attrs(&dec.slots);
            let rec = mr_sim::course::CourseInfo::read(&mr_sim::tables::ExeImage::parse(&exe).unwrap(), course).map(|i| i.lap_record).unwrap_or(0);
            let mut f = Front { progress: Progress::new_game(), ..Default::default() }.on_course(course, rec);
            f.open(page);
            f.row = row;
            if page == Page::CourseList {
                f.col = 4;
            }
            f.vb = vb;
            let ours: Vec<(i32, i32, u8, u8, i32, i32, u16, bool)> = page_prims(&f, &t)
                .into_iter()
                .filter_map(|p| if let Prim::Sprt(s) = p { Some(s) } else { None })
                .map(|s| (s.x, s.y, s.u, s.v, s.w, s.h, s.attrs(&slots).1, s.raw))
                .collect();
            let theirs = oracle_font_sprites(&text, slots[mr_assets::hud::FONT_SLOT as usize].1);
            let mut missing = Vec::new();
            for o in &ours {
                if !theirs.contains(o) {
                    missing.push(*o);
                }
            }
            assert!(missing.is_empty(), "{file}: {} of {} port sprites not in the oracle frame: {missing:?}", missing.len(), ours.len());
            let ours_n = ours.len();
            let theirs_n = theirs.len();
            let extra: Vec<_> = theirs.iter().filter(|o| !ours.contains(o)).collect();
            assert_eq!(ours_n, theirs_n, "{file}: sprite count; oracle only: {extra:?}");
        }
    }

    /// The league pages against the oracle's GP0 frames
    /// (tools/oracle/save_check.py league: analysis/s7/saves/*.json, local):
    /// the league page, the league menu, its POINTS, RACE POINTS and
    /// TOURNAMENT POINTS after BOG (the league data of league.txt). The
    /// frame counter the pulse / blink / heading follow is searched (0..300).
    #[test]
    fn gp0_league_pages() {
        let Some(exe) = exe() else { return };
        let root = mr_sim::oracle::repo_root().join("analysis/s7/saves");
        let Ok(txt) = std::fs::read_to_string(root.join("league.txt")) else { return };
        let t = MenuTables::from_exe(&exe);
        let mut disc = mr_disc::Disc::open_default().unwrap();
        // the league after the first race (league.txt: ranks, points)
        let mut lg = crate::league::League { league: 0, race: 0, courses: [8, 9, 16, 1, 3, 15], karts: [(0, 0); 6], points: [0; 6], last: Default::default(), finish: [0; 6], started: true };
        for l in txt.lines().filter(|l| l.starts_with("kart ")) {
            let w: Vec<&str> = l.split_whitespace().collect();
            let k: usize = w[1].parse().unwrap();
            let rank: usize = w[3].parse().unwrap();
            let g: Vec<u8> = w[11].split(',').map(|x| x.parse().unwrap()).collect();
            lg.finish[rank] = k as u8;
            lg.last[k] = mr_sim::save::RacePoints { place: g[0], fastest_lap: g[1], best_start: g[2], most_hits: g[3], total: g[4] };
            lg.points[k] = g[4];
        }
        let fresh = crate::league::League { points: [0; 6], ..lg.clone() };
        let cases: [(&str, u8, Page, i8, Option<crate::league::League>); 5] = [
            ("league_page.json", 0x1E, Page::Leagues, 0, None),
            ("league_menu.json", 8, Page::LeagueMenu, 0, Some(fresh.clone())),
            ("league_menu_points.json", 8, Page::LeaguePoints, 0, Some(fresh)),
            ("league_points1.json", 8, Page::Standings(1), 0, Some(lg.clone())),
            ("league_points2.json", 8, Page::Standings(2), 0, Some(lg)),
        ];
        for (file, course, page, row, league) in cases {
            let Ok(text) = std::fs::read_to_string(root.join(file)) else { continue };
            let course32 = course as u32;
            let path = mr_assets::raw::course_raw_path(&exe, course32).unwrap();
            let env = mr_assets::raw::LoadEnv::from_exe(&exe, course32).unwrap();
            let dec = mr_assets::raw::decode(&disc.read(&path).unwrap(), &env).unwrap();
            let slots = mr_assets::hud::slot_attrs(&dec.slots);
            let theirs = oracle_font_sprites(&text, slots[mr_assets::hud::FONT_SLOT as usize].1);
            let rec = mr_sim::course::CourseInfo::read(&mr_sim::tables::ExeImage::parse(&exe).unwrap(), course).map(|i| i.lap_record).unwrap_or(0);
            let mut best: Option<(usize, i32, Vec<(i32, i32, u8, u8, i32, i32, u16, bool)>)> = None;
            for vb in 0..300 {
                let mut f = Front { progress: Progress::new_game(), league: league.clone(), ..Default::default() }.on_course(course, rec);
                f.open(page);
                f.row = row;
                f.vb = vb;
                let ours: Vec<_> = page_prims(&f, &t)
                    .into_iter()
                    .filter_map(|p| if let Prim::Sprt(s) = p { Some(s) } else { None })
                    .map(|s| (s.x, s.y, s.u, s.v, s.w, s.h, s.attrs(&slots).1, s.raw))
                    .collect();
                let bad = ours.iter().filter(|o| !theirs.contains(o)).count() + theirs.iter().filter(|o| !ours.contains(o)).count();
                if best.as_ref().is_none_or(|b| bad < b.0) {
                    best = Some((bad, vb, ours));
                }
            }
            let (bad, vb, ours) = best.unwrap();
            let missing: Vec<_> = ours.iter().filter(|o| !theirs.contains(o)).collect();
            let extra: Vec<_> = theirs.iter().filter(|o| !ours.contains(o)).collect();
            assert_eq!(bad, 0, "{file} (best frame {vb}): port only {missing:?}; oracle only {extra:?}");
        }
    }

    /// (x, y, u, v, w, h, clut, raw) of the oracle frame's SPRTs in the font
    /// page's CLUT row (draw offset removed).
    /// Phase 6b: GAME STATUS (slot 1, DOCK: GAME MENU -> GAME STATUS; the
    /// oracle's GP0 frame and save block, local analysis/oracle/secrets/
    /// game_status): the font sprites equal the game's (the blink frame
    /// searched).
    #[test]
    fn gp0_game_status() {
        let Some(exe) = exe() else { return };
        let root = mr_sim::oracle::repo_root().join("analysis/oracle/secrets/game_status");
        let (Ok(text), Ok(block)) = (std::fs::read_to_string(root.join("game_status.json")), std::fs::read(root.join("block.bin"))) else { return };
        let t = MenuTables::from_exe(&exe);
        let mut disc = mr_disc::Disc::open_default().unwrap();
        let course32 = 3u32;
        let path = mr_assets::raw::course_raw_path(&exe, course32).unwrap();
        let env = mr_assets::raw::LoadEnv::from_exe(&exe, course32).unwrap();
        let dec = mr_assets::raw::decode(&disc.read(&path).unwrap(), &env).unwrap();
        let slots = mr_assets::hud::slot_attrs(&dec.slots);
        let theirs = oracle_font_sprites(&text, slots[mr_assets::hud::FONT_SLOT as usize].1);
        let mut b = [0u8; mr_sim::save::LEN];
        b.copy_from_slice(&block[..mr_sim::save::LEN]);
        let p = Progress(mr_sim::save::SaveBlock(b));
        let mut best: Option<(usize, Vec<(i32, i32, u8, u8, i32, i32, u16, bool)>)> = None;
        for vb in 0..0x1E {
            let ours: Vec<_> = game_status(&t, &p, 0, 4, vb)
                .into_iter()
                .filter_map(|p| if let Prim::Sprt(s) = p { Some(s) } else { None })
                .filter(|s| s.slot == mr_assets::hud::FONT_SLOT)
                .map(|s| (s.x, s.y, s.u, s.v, s.w, s.h, s.attrs(&slots).1, s.raw))
                .collect();
            let bad = ours.iter().filter(|o| !theirs.contains(o)).count() + theirs.iter().filter(|o| !ours.contains(o)).count();
            if best.as_ref().is_none_or(|b| bad < b.0) {
                best = Some((bad, ours));
            }
        }
        let (bad, ours) = best.unwrap();
        let missing: Vec<_> = ours.iter().filter(|o| !theirs.contains(o)).collect();
        let extra: Vec<_> = theirs.iter().filter(|o| !ours.contains(o)).collect();
        assert!(theirs.len() > 100, "{} oracle glyphs", theirs.len());
        assert_eq!(bad, 0, "port only {missing:?}; oracle only {extra:?}");
    }

    fn oracle_font_sprites(json: &str, font_clut: u16) -> Vec<(i32, i32, u8, u8, i32, i32, u16, bool)> {
        // a minimal scan of the decoded GP0 JSON: each prim's words
        let mut out = Vec::new();
        for chunk in json.split("\"words\": [").skip(1) {
            let words: Vec<u32> = chunk
                .split(']')
                .next()
                .unwrap_or("")
                .split(',')
                .filter_map(|w| u32::from_str_radix(w.trim().trim_matches('"').trim_start_matches("0x"), 16).ok())
                .collect();
            if words.len() != 4 || !(0x64..=0x67).contains(&(words[0] >> 24)) {
                continue;
            }
            let op = words[0] >> 24;
            let x = (words[1] & 0xFFFF) as i16 as i32;
            let y = (words[1] >> 16) as i16 as i32;
            let (u, v, clut) = ((words[2] & 0xFF) as u8, ((words[2] >> 8) & 0xFF) as u8, (words[2] >> 16) as u16);
            let (w, h) = ((words[3] & 0xFFFF) as i32, (words[3] >> 16) as i32);
            // glyph cells only (the frame also has a 256 x 5 strip from the
            // font page)
            if clut & 0xFFC0 != font_clut & 0xFFC0 || w > 32 || h > 32 {
                continue;
            }
            out.push((x, y, u, v, w, h, clut, op & 1 != 0));
        }
        out
    }
}

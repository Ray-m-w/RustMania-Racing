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
//!   RETIRE / RESTART ask YES / NO, `Confirm`); START on the results -> the
//!   course menu.
//!
//! Not ported (shown, greyed with the game's own "unavailable" look, abr 0,
//! and not selectable): MEET THE MUPPETS, GAME STATUS, TOURNAMENT,
//! ADVENTURE, 2 PLAYER RACE, SECRETS, ADJUST SCREEN, MEMORY CARD, VIBRATION,
//! the course select's MOVIE / MUSIC, stunt and battle courses.
//!
//! Progress (unlocks) until phase 2's save file: `unlock_all` (settings,
//! default true) = every character, vehicle and course; false = the state of
//! the oracle's save (8 characters, each with its own vehicle, worlds 0..5)
//! [RAM 0x800E0190..].

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
    pub const BEST_LAP: u32 = 0x800B_0354;
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
}

/// Everything the menus read from the executable.
pub struct MenuTables {
    pub hud: HudTables,
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
}

impl MenuTables {
    pub fn from_exe(exe: &[u8]) -> MenuTables {
        let ptrs = [
            sp::MEET_THE_MUPPETS, sp::SELECT_COURSE, sp::CHANGE_COURSE, sp::GAME_STATUS, sp::OPTIONS, sp::SET_CONTROLLERS, sp::ADJUST_SCREEN, sp::MEMORY_CARD,
            sp::ONE_PLAYER, sp::TOURNAMENT, sp::ADVENTURE, sp::ADJUST_SOUND, sp::BACK, sp::CANCEL, sp::ACCEPT, sp::SELECT, sp::MUSIC, sp::MOVIE, sp::RACE_A,
            sp::RACE_B, sp::STUNT, sp::BATTLE, sp::RACE, sp::BATTLE_RACE, sp::ONE_PLAYER_RACE, sp::TWO_PLAYER_RACE, sp::LAP, sp::LAPS, sp::SECRETS,
            sp::GAME_MENU, sp::BACK_TO_MAIN_MENU, sp::BEST_LAP, sp::NOT_COLLECTED, sp::NOT_WON, sp::MUPPET, sp::VEHICLE, sp::PAUSED, sp::CONTINUE,
            sp::RETIRE, sp::RESTART, sp::YES, sp::NO, sp::MUSIC_VOLUME, sp::SOUND_VOLUME, sp::CONTROLLER, sp::JUMP, sp::BRAKE, sp::CONFIGURATION,
            sp::VIBRATION,
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

/// The unlock state the menus read (the save's 0x800E0190.. block; phase 2
/// brings the save file).
#[derive(Clone, Debug)]
pub struct Progress {
    /// 0x800E02BC: characters collected (bit per character).
    pub chars: u32,
    /// 0x800E02C0 + 8 * character: vehicles won per character (2 words).
    pub vehicles: Vec<[u32; 2]>,
    /// 0x800E0194 + 0x20 * world + 8 * type + 2: bit 0 = the course is open.
    pub courses: [[u16; 4]; 9],
    /// 0x800E038C: laps of a 1-player race.
    pub laps: u8,
}

impl Progress {
    /// Every character, vehicle and course.
    pub fn all() -> Progress {
        Progress { chars: 0x1FF_FFFF, vehicles: vec![[u32::MAX, u32::MAX]; 25], courses: [[1; 4]; 9], laps: 3 }
    }

    /// The oracle's save [RAM 0x800E0190.. in every course capture]:
    /// characters 0..7, each with its own vehicle only, worlds 0..5 open,
    /// the secret courses closed (0x60), worlds 7 / 8 0xFF.
    pub fn oracle_save(t: &MenuTables) -> Progress {
        let mut courses = [[1u16; 4]; 9];
        courses[6] = [0x60; 4];
        courses[7] = [0xFF; 4];
        courses[8] = [0xFF; 4];
        let vehicles = (0..25).map(|c| {
            let v = t.default_vehicle[c] as u32;
            let mut m = [0u32; 2];
            m[(v / 32) as usize] = 1 << (v % 32);
            m
        });
        Progress { chars: 0xFF, vehicles: vehicles.collect(), courses, laps: 3 }
    }

    pub fn char_open(&self, c: u8) -> bool {
        c < 32 && self.chars & (1 << c) != 0
    }

    pub fn vehicle_open(&self, c: u8, v: u8) -> bool {
        self.vehicles.get(c as usize).is_some_and(|m| m[(v / 32) as usize & 1] & (1 << (v % 32)) != 0)
    }

    pub fn course_open(&self, world: usize, ty: usize) -> bool {
        self.courses[world][ty] & 1 != 0
    }
}

fn progress() -> Progress {
    if crate::settings::Settings::global().bool("unlock_all", true) {
        Progress::all()
    } else {
        Progress::oracle_save(tables())
    }
}

/// Worlds in the course select / world list: the game's 7 (0x800B07AB),
/// plus world 7 (SPEEDWAY, RALLY, GLACIER; the game needs everything won
/// for it) while everything is unlocked.
fn worlds_shown(p: &Progress) -> usize {
    if p.course_open(7, 0) && p.chars == Progress::all().chars { 8 } else { 7 }
}

/// The race courses the port runs (the 18 of the course table).
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
}

impl Page {
    /// Pages over a loaded course whose race is not running (the session
    /// runs `menu_tick`).
    pub fn course_menu(self) -> bool {
        matches!(self, Page::CourseMenu | Page::GameMenu | Page::WorldList | Page::CourseList | Page::CharSelect)
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
}

impl Front {
    /// The state on a loaded course `id` (tests; `Default` takes this run's).
    pub fn on_course(mut self, id: u8, lap_record: i32) -> Front {
        self.course = id;
        self.lap_record = lap_record;
        self.here = tables().cups.iter().position(|&c| c == id as i8).map(|i| (i / 4, i % 4));
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
            Page::Confirm(_) => (1, 0),
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

    /// The title's START (title.rs).
    pub fn open_main(&mut self) {
        self.open(Page::Main);
    }

    /// The results screen's START / RETIRE: the course menu with a fresh
    /// menu session.
    pub fn to_course_menu(&mut self) {
        self.open(Page::CourseMenu);
        self.request = Request::Menu { keep_camera: false };
    }

    fn rows(&self) -> i8 {
        match self.page {
            Page::Main => 5,
            Page::Options => 5,
            Page::Sound | Page::Confirm(_) => 2,
            Page::Controllers => 2,
            Page::PortSettings => PORT_KEYS.len() as i8,
            Page::CourseMenu => 5,
            Page::GameMenu => 4,
            Page::WorldList | Page::CourseSelect => worlds_shown(&self.progress) as i8,
            Page::CourseList => 4,
            Page::CharSelect => tables().char_list.len() as i8,
            Page::Pause => 4,
            Page::None => 1,
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
                Item::new(txt(sp::GAME_STATUS)).grey(true),
                Item::new(txt(sp::SELECT_COURSE)),
                Item::new(format!("{} - {}", txt(sp::TOURNAMENT), txt(sp::ONE_PLAYER))).grey(true),
                Item::new(txt(sp::OPTIONS)),
            ];
            sp_out.extend(items(t, &list, sel, 0x78, f));
            sp_out.extend(select_button(t, 0x100, 0xD2, 0, false));
            sp_out.extend(back_button(t, 0x100, 0x1E, 0, false));
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
        Page::CourseMenu | Page::GameMenu | Page::WorldList | Page::CourseList => {
            // game mode 0 (FUN_80051698 case 0): the course's name and lap
            // record, SELECT bottom left, BACK top left (not on the course
            // menu itself)
            let here = fr.here.map(|(w, c)| t.course_names[w * 4 + c].clone()).unwrap_or_default();
            sp_out.extend(t.hud.text(here.as_bytes(), 0x1E0, 0x1E, 8, Size::from_arg(0), 4, 0x60));
            sp_out.extend(t.hud.text(best_lap_text(t, fr.lap_record).as_bytes(), 0x1E0, 0xD2, 8, Size::from_arg(0), 4, 0x60));
            sp_out.extend(select_button(t, 0x20, 0xD2, 2, false));
            if fr.page != Page::CourseMenu {
                sp_out.extend(back_button(t, 0x20, 0x1E, 2, false));
            }
            let list: Vec<Item> = match fr.page {
                Page::CourseMenu => {
                    let laps = p.laps;
                    let lap_s = txt(if laps == 1 { sp::LAP } else { sp::LAPS });
                    vec![
                        Item::new(format!("{} - {} {}", txt(sp::ONE_PLAYER_RACE), laps, lap_s)),
                        Item::new(format!("{} - {} {}", txt(sp::TWO_PLAYER_RACE), laps, lap_s)).grey(true),
                        Item::new(format!("{} {}", here, txt(sp::SECRETS))).grey(true),
                        Item::new(txt(sp::GAME_MENU)).tint([0x1F, 0xFF, 0x1F]),
                        Item::new(txt(sp::BACK_TO_MAIN_MENU)).tint([0xFF, 0x1F, 0x1F]),
                    ]
                }
                Page::GameMenu => vec![
                    Item::new(txt(sp::ADVENTURE)).grey(true),
                    Item::new(txt(sp::GAME_STATUS)).grey(true),
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

fn paused_text(t: &MenuTables, f: i32) -> Vec<Sprite> {
    // shown while 0x800B07D0 % 60 < 45
    if f % 60 < 45 {
        t.hud.text(t.st(sp::PAUSED).as_bytes(), 0x1E0, 0x1E, 8, Size::from_arg(0), 4, 0x60)
    } else {
        Vec::new()
    }
}

/// RACE A / RACE B / STUNT / BATTLE (worlds 0..5), RACE / BATTLE / RACE /
/// BATTLE RACE (worlds 6..).
fn type_label(t: &MenuTables, w: usize, c: usize) -> String {
    let a = if w < 6 {
        [sp::RACE_A, sp::RACE_B, sp::STUNT, sp::BATTLE][c]
    } else {
        [sp::RACE, sp::BATTLE, sp::RACE, sp::BATTLE_RACE][c]
    };
    t.st(a).to_string()
}

/// "BEST LAP m:ss.hh" from a lap record (race clock units / 3 =
/// hundredths; "%s %i:%i%i.%i%i").
pub fn best_lap_text(t: &MenuTables, record: i32) -> String {
    let h = record / 3;
    let m = h / 6000;
    let s = h / 100 - m * 60;
    let c = h % 100;
    format!("{} {}:{}{}.{}{}", t.st(sp::BEST_LAP), m, s / 10, s % 10, c / 10, c % 10)
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
                        2 => {
                            confirm(&mut fx);
                            self.open(Page::CourseSelect);
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
                            self.progress = if st.bool("unlock_all", true) { Progress::all() } else { Progress::oracle_save(t) };
                        }
                    }
                    self.port = None;
                    self.open(Page::Options);
                    self.row = 4;
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
                if cross {
                    match self.row {
                        0 => {
                            confirm(&mut fx);
                            self.open(Page::CharSelect);
                            self.request = Request::Menu { keep_camera: true };
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
            Page::GameMenu => {
                if back {
                    confirm(&mut fx);
                    self.open(Page::CourseMenu);
                    self.row = 3;
                } else if cross && self.row == 2 {
                    confirm(&mut fx);
                    self.open(Page::WorldList);
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
                if back {
                    confirm(&mut fx);
                    self.open(Page::CourseMenu);
                    self.request = Request::Menu { keep_camera: true };
                } else if cross && self.progress.char_open(self.character) && self.progress.vehicle_open(self.character, self.vehicle) {
                    // FUN_80073C58: the character (and its vehicle) collected
                    confirm(&mut fx);
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
                        self.to_course_menu();
                    }
                }
            }
        }
        fx
    }
}

// ---------------------------------------------------------------- plugin

pub struct MenusPlugin;

impl Plugin for MenusPlugin {
    fn build(&self, app: &mut App) {
        let mut front = Front::default();
        if std::env::args().any(|a| a == "--menu") && !crate::title::on_title() {
            front.page = Page::CourseMenu;
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
    if front.page == Page::None && !crate::title::on_title() && racing && !tool_run() {
        let b = latch.now;
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
    let fx = front.input(latch.now, t);
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
    if let Some(c) = fx.relaunch_course {
        relaunch(Some(&c), front.character, front.vehicle);
        exit.write(AppExit::Success);
    } else if fx.relaunch_title {
        relaunch(None, front.character, front.vehicle);
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
pub fn relaunch(course: Option<&str>, character: u8, vehicle: u8) {
    let a: Vec<String> = std::env::args().collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 1;
    while i < a.len() {
        match a[i].as_str() {
            "--course" | "--character" | "--vehicle" | "--menu-keys" | "--menu-shots" => i += 2,
            "--skip-title" | "--menu" => i += 1,
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
        let mut f = Front { progress: Progress::oracle_save(&t), ..Default::default() };
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
            let mut f = Front { progress: Progress::oracle_save(&t), ..Default::default() }.on_course(course, rec);
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

    /// (x, y, u, v, w, h, clut, raw) of the oracle frame's SPRTs in the font
    /// page's CLUT row (draw offset removed).
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

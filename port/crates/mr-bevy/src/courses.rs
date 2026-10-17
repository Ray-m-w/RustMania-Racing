//! Course choice (Session 6, track P): which race course this run drives.
//!
//! The course: `--course NAME` (or `course = NAME` in settings.txt; a TER
//! file name such as DOCK, NEW_YORK, or the course index); default DOCK; on
//! the title the frontend course 0x1E. Every course-dependent loader (scene,
//! HUD slots, animated textures, effects, music, the session) reads
//! `courses::info()`. The game's menus (menus.rs, Session 7) pick the
//! course: another course is booted in place (row 12, boot.rs: the boot
//! arguments become `--course NAME --skip-title --menu`, `reset` forgets
//! the choice, the scene is rebuilt). The Session 6 pick-list is gone.

use std::sync::{Mutex, OnceLock};

use mr_sim::course::CourseInfo;

struct Choice {
    info: &'static CourseInfo,
    explicit: bool,
}

/// The course tables, read once.
fn all() -> &'static Vec<CourseInfo> {
    static R: OnceLock<Vec<CourseInfo>> = OnceLock::new();
    R.get_or_init(|| {
        let e = exe();
        // the race courses, (phase 3) the stunt courses' trial and (phase
        // 4) the arenas' 1 PLAYER BATTLE
        CourseInfo::races(e).into_iter().chain(CourseInfo::stunts(e)).chain(CourseInfo::arenas(e)).collect()
    })
}

/// A course row, interned: one copy per course for the process (an
/// in-process course change reuses it; no copy per change).
fn interned(id: u8) -> &'static CourseInfo {
    static M: Mutex<Vec<(u8, &'static CourseInfo)>> = Mutex::new(Vec::new());
    let mut m = M.lock().unwrap();
    if let Some((_, i)) = m.iter().find(|(k, _)| *k == id) {
        return i;
    }
    let i: &'static CourseInfo = Box::leak(Box::new(CourseInfo::read(exe(), id).expect("course row")));
    m.push((id, i));
    i
}

/// The current boot's choice (row 12: reset by an in-process course change,
/// boot.rs, which replaces the boot arguments first).
static CHOICE: Mutex<Option<(&'static CourseInfo, bool)>> = Mutex::new(None);

/// Forget the choice: the next `info()` reads the new boot arguments.
pub fn reset() {
    *CHOICE.lock().unwrap() = None;
}

fn choice() -> Choice {
    let mut c = CHOICE.lock().unwrap();
    if let Some((info, explicit)) = *c {
        return Choice { info, explicit };
    }
    let (info, explicit) = decide();
    *c = Some((info, explicit));
    Choice { info, explicit }
}

fn decide() -> (&'static CourseInfo, bool) {
    let e = exe();
    let races = all();
    // start-up: `--course` or the settings file's `course`; after an
    // in-process course change only the boot arguments count
    let want = if crate::settings::rebooted() {
        crate::settings::arg_value("--course")
    } else {
        crate::settings::Settings::global().get("course").map(|s| s.to_string())
    }
    .filter(|s| !s.is_empty());
    let found = want.as_deref().and_then(|w| CourseInfo::find(e, w)).filter(|c| c.is_race() || c.is_stunt() || c.is_arena());
    if let (Some(w), None) = (&want, &found) {
        eprintln!("course {w}: not a race course of the game's table; DOCK instead (races: {})", races.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(" "));
    }
    let explicit = found.is_some();
    // Phase 8: THE END (ending.rs), course 0x1D
    if crate::ending::asked() {
        return (interned(crate::ending::COURSE), false);
    }
    // The title (title.rs) is the frontend course 0x1E run as the game
    // runs it; the pick-list comes with START.
    if crate::title::first_screen() == crate::title::Screen::Title {
        return (interned(crate::title::TITLE_COURSE as u8), false);
    }
    let id = found.map(|f| f.id).unwrap_or(mr_sim::course::DOCK);
    (interned(id), explicit)
}

/// The course this run drives.
pub fn info() -> &'static CourseInfo {
    choice().info
}

/// Its index (0x800B0FAC).
pub fn id() -> u32 {
    info().id as u32
}

/// True when the course came from `--course` / settings.
pub fn explicit() -> bool {
    choice().explicit
}

/// The courses the port runs: the race courses of the game's table and
/// the stunt courses (their 1 PLAYER STUNT TRIAL).
pub fn races() -> &'static [CourseInfo] {
    all()
}

/// The frame clear colour: the course row's fog colour (0x800ACB34 + 12 *
/// course, +0..2; the GP0 fill at the start of each race frame, checked on
/// DOCK: (0, 0x13, 0x5F)) [INFER for the other courses].
pub fn clear_rgb() -> [u8; 3] {
    info().fog
}

/// The session settings of a play race on this course with the player's
/// character / vehicle (menus.rs): DOCK with Kermit / RACER keeps the
/// oracle's race settings (`dock_default`), the others the game's boot
/// settings for the course (`SessionConfig::course`). The controller
/// configuration B (settings `controller_config`) swaps brake and jump.
pub fn session_config(logic_hz: u8, character: u8, vehicle: u8) -> mr_sim::session::SessionConfig {
    if crate::title::on_title() {
        let (seed, rng) = crate::title::session_seed();
        return mr_sim::session::SessionConfig::title(seed, rng, logic_hz);
    }
    let base = if info().id == mr_sim::course::DOCK && character == 0 && vehicle == 0 {
        mr_sim::session::SessionConfig::dock_default()
    } else {
        mr_sim::session::SessionConfig::course(info())
    };
    let alt_controls = crate::settings::Settings::global().get("controller_config") == Some("B");
    let p2_alt_controls = crate::settings::Settings::global().get("controller_config_2") == Some("B");
    let mut cfg = mr_sim::session::SessionConfig { logic_hz, character, vehicle, alt_controls, p2_alt_controls, ..base };
    // phase 8: THE END (ending.rs): no karts in the game; the session's
    // parked kart is not drawn and only the camera runs
    if crate::ending::on() {
        cfg.opponents = 0;
        return cfg;
    }
    // phase 8: the attract demo (demo.rs): six AI karts, no save
    if crate::demo::active() {
        cfg.demo = true;
        return cfg;
    }
    if !crate::menus::tool_run() {
        // the save (save.rs): the course's lap record and the laps
        // (0x800E038C); a league race: the league's karts, the grid drawn,
        // the league's laps (0x800E0388 + league)
        crate::save::with(|s| {
            if let Some((w, t)) = s.tables.place(info().id) {
                cfg.lap_record = s.block.record(w, t);
            }
            cfg.laps = s.block.laps(0).max(1) as i16;
            // phase 13: the AI catch-up scale 0x800E039A (FUN_800823D4
            // steps it after each 1-player race)
            cfg.difficulty = s.block.difficulty();
            if let Some(l) = crate::league::League::load().filter(|l| l.started) {
                cfg.lineup = Some(l.lineup());
                // row 12: the tie-break race's field
                cfg.opponents = l.opponents();
                // phase 13: TOURNAMENT - 2 PLAYERS: both players (split
                // screen) and the league's third kart; a tie-break the
                // tied players only (a tie with player 2 alone: player 2
                // drives kart 0 with its pad, input.rs)
                let humans: Vec<u8> = if l.tie_break() { l.tie_humans() } else { (0..l.players.max(1)).collect() };
                if let Some(&h0) = humans.first() {
                    (cfg.character, cfg.vehicle) = l.karts[h0 as usize];
                }
                if humans.len() == 2 {
                    cfg.players = 2;
                    (cfg.p2_character, cfg.p2_vehicle) = l.karts[humans[1] as usize];
                }
                cfg.draw_grid = true;
                // 0x800B07E4 (a BATTLE LEAGUE battle ends with FUN_80082450)
                cfg.league = true;
                cfg.laps = s.block.league_laps(l.league as usize).max(1) as i16;
            }
        });
    }
    cfg
}

/// The executable's tables (phase 6: the ADVENTURE / "BEAT x" set-ups).
pub fn exe() -> &'static mr_sim::tables::ExeImage {
    static E: OnceLock<mr_sim::tables::ExeImage> = OnceLock::new();
    E.get_or_init(|| {
        let mut disc = mr_disc::Disc::open_default().expect("open the disc");
        mr_sim::tables::ExeImage::parse(&disc.read("SLUS_012.37").expect("read the executable")).expect("exe")
    })
}

/// The Doozer's vehicles, one per ADVENTURE in turn (0x800B0A14, the turn
/// 0x800B0A10).
pub const DOOZER_VEHICLES: u32 = 0x800B_0A14;

/// ADVENTURE on the loaded course (phase 6, game mode 0xB): the save's
/// course flags and characters; `turn` picks the Doozer's vehicle.
pub fn adventure_config(logic_hz: u8, character: u8, vehicle: u8, turn: u8) -> mr_sim::session::SessionConfig {
    let mut cfg = mr_sim::session::SessionConfig { adventure: true, opponents: 0, lineup: None, ..session_config(logic_hz, character, vehicle) };
    save_bits(&mut cfg);
    cfg.doozer_vehicle = exe().u8(DOOZER_VEHICLES + (turn % 3) as u32);
    cfg
}

/// The "BEAT x" race after an ADVENTURE's tag (race flag 0x10, game mode 1
/// (2 in an arena)): the player against `rival` on its own vehicle (a
/// random selectable one if it has none), the grid drawn, 3 laps
/// (FUN_8008C4AC @0x8008DE8C, 0x8008E49C).
pub fn beat_config(logic_hz: u8, character: u8, vehicle: u8, rival: u8) -> mr_sim::session::SessionConfig {
    let mut cfg = session_config(logic_hz, character, vehicle);
    let (_, mut v) = mr_sim::ai::character_entry(exe(), rival);
    if v == 0xFF {
        let mut seed = cfg.rand_seed;
        loop {
            v = (mr_sim::session::bios_rand(&mut seed) / 0x2AB) as u8;
            if exe().u8(mr_sim::ai::addr::VEHICLES + 4 * v as u32 + 3) & 1 != 0 {
                break;
            }
        }
    }
    cfg.opponents = 1;
    cfg.lineup = Some([(rival, v); 5]);
    cfg.draw_grid = true;
    cfg.race_flags = 0x10;
    cfg.laps = 3;
    cfg.tagged = rival as i8;
    save_bits(&mut cfg);
    cfg
}

/// Phase 6b: a COURSE SECRETS race (docs/spec/adventure.md "COURSE
/// SECRETS"): the race flags 0x800B07E2 (REVERSE RACE 2, BATTLE 1, BATTLE
/// RACE 8, the reversed stunt trial 4, BATTLE AS BOSS 0x20, DOGFIGHT 0x80,
/// DOOZER BATTLE 0x100) or an arena's BOSS CHALLENGE (0x800B07E8, game
/// mode 7); the session places the karts as FUN_8008C4AC does.
pub fn secret_config(logic_hz: u8, character: u8, vehicle: u8, race_flags: u16, boss: bool, boss_pick: u8, doozer_turn: u8) -> mr_sim::session::SessionConfig {
    let mut cfg = session_config(logic_hz, character, vehicle);
    cfg.race_flags = race_flags;
    cfg.boss_challenge = boss;
    cfg.boss_pick = boss_pick;
    cfg.doozer_turn = doozer_turn;
    // (BATTLE on a race / stunt course: a battle's six karts, 5 above cup 6)
    if race_flags & 1 != 0 {
        cfg.opponents = if cfg.cup > 6 { 4 } else { 5 };
    }
    save_bits(&mut cfg);
    cfg
}

/// The save bits the race code reads and writes (course flags,
/// characters collected).
fn save_bits(cfg: &mut mr_sim::session::SessionConfig) {
    if crate::menus::tool_run() {
        return;
    }
    crate::save::with(|s| {
        if let Some((w, t)) = s.tables.place(info().id) {
            cfg.course_flags = s.block.flags(w, t);
        }
        cfg.characters = s.block.chars();
    });
}

/// The course menu / character select session: the player's kart alone on
/// grid slot 0 (FUN_80072D8C -> FUN_800609F4(kart, c, v, 0, 0)), ticked by
/// `Session::menu_tick`.
/// Phase 11: TEST DRIVE on the frontend course (game mode 0xE, from MEET
/// THE MUPPETS): the title's set-up with the player alone in `character`'s
/// `vehicle` (mr_sim SessionConfig::test_drive).
pub fn test_drive_config(logic_hz: u8, character: u8, vehicle: u8) -> mr_sim::session::SessionConfig {
    let (seed, rng) = crate::title::session_seed();
    let alt_controls = crate::settings::Settings::global().get("controller_config") == Some("B");
    mr_sim::session::SessionConfig { test_drive: true, character, vehicle, alt_controls, ..mr_sim::session::SessionConfig::title(seed, rng, logic_hz) }
}

pub fn menu_config(logic_hz: u8, character: u8, vehicle: u8) -> mr_sim::session::SessionConfig {
    mr_sim::session::SessionConfig { opponents: 0, grid: 0, lineup: None, ..session_config(logic_hz, character, vehicle) }
}

//! Course choice (Session 6, track P): which race course this run drives.
//!
//! The course: `--course NAME` (or `course = NAME` in settings.txt; a TER
//! file name such as DOCK, NEW_YORK, or the course index); default DOCK; on
//! the title the frontend course 0x1E. Every course-dependent loader (scene,
//! HUD slots, animated textures, effects, music, the session) reads
//! `courses::info()`. The game's menus (menus.rs, Session 7) pick the
//! course: another course restarts the program on it (`--course NAME
//! --skip-title --menu`; the scene is built at start-up). The Session 6
//! pick-list is gone.

use std::sync::OnceLock;

use mr_sim::course::CourseInfo;

struct Choice {
    info: CourseInfo,
    explicit: bool,
    races: Vec<CourseInfo>,
}

fn choice() -> &'static Choice {
    static C: OnceLock<Choice> = OnceLock::new();
    C.get_or_init(|| {
        let mut disc = mr_disc::Disc::open_default().expect("open the disc (MRM_DISC or game.toml)");
        let exe = mr_sim::tables::ExeImage::parse(&disc.read("SLUS_012.37").expect("read the executable")).expect("exe");
        // the race courses, (phase 3) the stunt courses' trial and (phase
        // 4) the arenas' 1 PLAYER BATTLE
        let races: Vec<CourseInfo> = CourseInfo::races(&exe).into_iter().chain(CourseInfo::stunts(&exe)).chain(CourseInfo::arenas(&exe)).collect();
        let want = crate::settings::Settings::global().get("course").map(|s| s.to_string()).filter(|s| !s.is_empty());
        let found = want.as_deref().and_then(|w| CourseInfo::find(&exe, w)).filter(|c| c.is_race() || c.is_stunt() || c.is_arena());
        if let (Some(w), None) = (&want, &found) {
            eprintln!("course {w}: not a race course of the game's table; DOCK instead (races: {})", races.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(" "));
        }
        let explicit = found.is_some();
        // The title (title.rs) is the frontend course 0x1E run as the game
        // runs it; the pick-list comes with START.
        if crate::title::first_screen() == crate::title::Screen::Title {
            let info = CourseInfo::read(&exe, crate::title::TITLE_COURSE as u8).expect("frontend course row");
            return Choice { info, explicit: false, races };
        }
        let info = found.unwrap_or_else(|| CourseInfo::read(&exe, mr_sim::course::DOCK).expect("DOCK row"));
        Choice { info, explicit, races }
    })
}

/// The course this run drives.
pub fn info() -> &'static CourseInfo {
    &choice().info
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
    &choice().races
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
    let mut cfg = mr_sim::session::SessionConfig { logic_hz, character, vehicle, alt_controls, ..base };
    if !crate::menus::tool_run() {
        // the save (save.rs): the course's lap record and the laps
        // (0x800E038C); a league race: the league's karts, the grid drawn,
        // the league's laps (0x800E0388 + league)
        crate::save::with(|s| {
            if let Some((w, t)) = s.tables.place(info().id) {
                cfg.lap_record = s.block.record(w, t);
            }
            cfg.laps = s.block.laps(0).max(1) as i16;
            if let Some(l) = crate::league::League::load().filter(|l| l.started) {
                cfg.lineup = Some(l.lineup());
                cfg.draw_grid = true;
                cfg.laps = s.block.league_laps(l.league as usize).max(1) as i16;
            }
        });
    }
    cfg
}

/// The course menu / character select session: the player's kart alone on
/// grid slot 0 (FUN_80072D8C -> FUN_800609F4(kart, c, v, 0, 0)), ticked by
/// `Session::menu_tick`.
pub fn menu_config(logic_hz: u8, character: u8, vehicle: u8) -> mr_sim::session::SessionConfig {
    mr_sim::session::SessionConfig { opponents: 0, grid: 0, lineup: None, ..session_config(logic_hz, character, vehicle) }
}

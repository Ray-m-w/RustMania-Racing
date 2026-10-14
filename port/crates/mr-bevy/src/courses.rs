//! Course choice (Session 6, track P): which race course this run drives,
//! and a plain pick-list until the game's own menus exist.
//!
//! - The course: `--course NAME` (or `course = NAME` in settings.txt; a TER
//!   file name such as DOCK, NEW_YORK, or the course index); default DOCK.
//!   Every course-dependent loader (scene, HUD slots, animated textures,
//!   effects, music, the session) reads `courses::info()`.
//! - The pick-list: shown when the race screen starts without an explicit
//!   course, and by START on the results screen. Up / Down / Left / Right
//!   move, CROSS or START races. The same course restarts the race in
//!   place; another course relaunches the program with `--course NAME
//!   --skip-title` (the scene is built at start-up). Drawn with the game's
//!   HUD font (FUN_800474E8 glyphs, hud.rs overlay) over a black screen; the
//!   race (virtual time) is paused while it shows.
//!
//! The list is the game's own course table (mr_sim::course): the 18 race
//! courses (race type 0 / 1) in course-index order.

use std::sync::OnceLock;

use bevy::prelude::*;
use mr_assets::hud::{self as ahud, HudTables};
use mr_assets::hud_items::Prim;
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
        let races = CourseInfo::races(&exe);
        let want = crate::settings::Settings::global().get("course").map(|s| s.to_string()).filter(|s| !s.is_empty());
        let found = want.as_deref().and_then(|w| CourseInfo::find(&exe, w)).filter(|c| c.is_race());
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

/// The race courses of the game's table.
pub fn races() -> &'static [CourseInfo] {
    &choice().races
}

/// The frame clear colour: the course row's fog colour (0x800ACB34 + 12 *
/// course, +0..2; the GP0 fill at the start of each race frame, checked on
/// DOCK: (0, 0x13, 0x5F)) [INFER for the other courses].
pub fn clear_rgb() -> [u8; 3] {
    info().fog
}

/// The session settings of a play race on this course: DOCK keeps the
/// oracle's race settings (`dock_default`), the others the game's boot
/// settings for the course (`SessionConfig::course`).
pub fn session_config(logic_hz: u8) -> mr_sim::session::SessionConfig {
    if crate::title::on_title() {
        let (seed, rng) = crate::title::session_seed();
        return mr_sim::session::SessionConfig::title(seed, rng, logic_hz);
    }
    let base = if info().id == mr_sim::course::DOCK {
        mr_sim::session::SessionConfig::dock_default()
    } else {
        mr_sim::session::SessionConfig::course(info())
    };
    mr_sim::session::SessionConfig { logic_hz, ..base }
}

/// The pick-list's state. `open` while it shows; `restart` asks sim_tick
/// for a fresh race on this course.
#[derive(Resource, Default)]
pub struct PickList {
    pub open: bool,
    pub restart: bool,
    sel: usize,
    prev: u16,
    /// Set once the race screen has been entered (the first entry opens
    /// the list when no course was given).
    started: bool,
}

impl PickList {
    /// Show the list (results screen START).
    pub fn show(&mut self) {
        self.open = true;
        // (from the title, whose course is not a race: DOCK)
        let here = |id: u8| races().iter().position(|c| c.id == id);
        self.sel = here(info().id).or_else(|| here(mr_sim::course::DOCK)).unwrap_or(0);
    }
}

pub struct CoursesPlugin;

impl Plugin for CoursesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PickList>().add_systems(Update, (first_open, navigate, overlay.after(navigate), pause.after(navigate)).run_if(race_visible));
    }
}

/// The race screen shows (the title, track T, switches the race cameras
/// off while it is up).
fn race_visible(q: Query<&Camera, With<crate::hud::HudCamera>>) -> bool {
    q.iter().any(|c| c.is_active)
}

/// Tool runs (routes, replays, oracle grids, shots) never see the list.
fn tool_run() -> bool {
    let a: Vec<String> = std::env::args().collect();
    let tool = ["--oracle-grid", "--route", "--replay", "--replay-capture", "--shot", "--fx-shots", "--latency-log", "--autopilot", "--record-route", "--trace-out", "--ghost", "--no-picklist"];
    a.iter().any(|x| tool.contains(&x.as_str()))
}

fn first_open(mut pick: ResMut<PickList>) {
    if pick.started {
        return;
    }
    pick.started = true;
    // the title opens it with START (title.rs)
    if crate::title::on_title() {
        return;
    }
    // `--show-picklist`: open it in any run (screenshots).
    if (!explicit() && !tool_run()) || std::env::args().any(|a| a == "--show-picklist") {
        pick.show();
    }
}

fn pause(pick: Res<PickList>, mut time: ResMut<Time<Virtual>>) {
    // (`--show-picklist` screenshots keep the race running under it.)
    // (the title keeps running under it, as the game's menus)
    if pick.open && !time.is_paused() && !std::env::args().any(|a| a == "--show-picklist") && !crate::title::on_title() {
        time.pause();
    } else if !pick.open && time.is_paused() && pick.is_changed() {
        time.unpause();
    }
}

fn navigate(mut pick: ResMut<PickList>, latch: Res<crate::pacing::PadLatch>, mut exit: MessageWriter<AppExit>) {
    if !pick.open {
        pick.prev = latch.now;
        return;
    }
    let b = latch.now;
    let pressed = |m: u16| b & m != 0 && pick.prev & m == 0;
    let n = races().len();
    let rows = n.div_ceil(2);
    let mut sel = pick.sel;
    if pressed(crate::input::DOWN) {
        sel = (sel + 1) % n;
    }
    if pressed(crate::input::UP) {
        sel = (sel + n - 1) % n;
    }
    if pressed(crate::input::RIGHT) || pressed(crate::input::LEFT) {
        sel = if sel < rows { (sel + rows).min(n - 1) } else { sel - rows };
    }
    let go = pressed(crate::input::CROSS) || pressed(crate::input::START);
    pick.prev = b;
    pick.sel = sel;
    if go {
        let c = &races()[sel];
        if c.id == info().id {
            pick.open = false;
            pick.restart = true;
        } else {
            relaunch(&c.name);
            exit.write(AppExit::Success);
        }
    }
}

/// Start this program again on `course` (the scene is built at start-up),
/// keeping the other arguments.
fn relaunch(course: &str) {
    let a: Vec<String> = std::env::args().collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 1;
    while i < a.len() {
        if a[i] == "--course" {
            i += 2;
            continue;
        }
        if a[i] == "--skip-title" {
            i += 1;
            continue;
        }
        out.push(a[i].clone());
        i += 1;
    }
    out.push("--course".into());
    out.push(course.into());
    out.push("--skip-title".into());
    match std::env::current_exe() {
        Ok(exe) => {
            if let Err(e) = std::process::Command::new(exe).args(&out).spawn() {
                eprintln!("course: relaunch failed: {e}");
            }
        }
        Err(e) => eprintln!("course: relaunch failed: {e}"),
    }
}

#[derive(Resource)]
struct Font(HudTables);

/// The list as HUD primitives (512 x 240 game screen).
pub fn list_prims(t: &HudTables, sel: usize) -> Vec<Prim> {
    // (Earlier primitives draw on top, as the game's ordering table.)
    let mut v: Vec<Prim> = Vec::new();
    let push = |v: &mut Vec<Prim>, s: Vec<ahud::Sprite>| v.extend(s.into_iter().map(Prim::Sprt));
    push(&mut v, t.text(b"CHOOSE A COURSE", 0x100, 0x18, 0, ahud::Size::from_arg(0), 4, 0x60));
    let n = races().len();
    let rows = n.div_ceil(2);
    for (i, c) in races().iter().enumerate() {
        let (col, row) = (i / rows, i % rows);
        let x = if col == 0 { 0x80 } else { 0x180 };
        let y = 0x3C + 0x11 * row as i32;
        let colour = if i == sel { 1 } else { 4 };
        let label = c.name.replace('_', " ");
        push(&mut v, t.text(label.as_bytes(), x, y, 0, ahud::Size::from_arg(0), colour, 0x60));
    }
    push(&mut v, t.text(b"CROSS TO RACE", 0x100, 0xE0, 0, ahud::Size::from_arg(2), 4, 0x60));
    // Black backdrop under everything: two flat triangles over the
    // (widened) screen.
    let (x0, x1, y0, y1) = (-512, 1024, -16, 256);
    v.push(Prim::F3 { v: [[x0, y0], [x1, y0], [x0, y1]], rgb: [0, 0, 0] });
    v.push(Prim::F3 { v: [[x1, y0], [x1, y1], [x0, y1]], rgb: [0, 0, 0] });
    v
}

fn overlay(mut commands: Commands, pick: Res<PickList>, font: Option<Res<Font>>, title: Res<crate::title::TitleOverlay>, mut ov: ResMut<crate::hud::HudOverlay>) {
    let Some(font) = font else {
        let mut disc = mr_disc::Disc::open_default().expect("open the disc");
        let exe = disc.read("SLUS_012.37").expect("read the executable");
        commands.insert_resource(Font(HudTables::from_exe(&exe).expect("HUD tables")));
        return;
    };
    // closed: the title's layer while it shows (title.rs), else the race HUD
    let want = if pick.open { Some(list_prims(&font.0, pick.sel)) } else { title.0.clone() };
    if ov.prims != want {
        ov.prims = want;
    }
}

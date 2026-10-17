//! Phase 8: the attract demo (docs/spec/frontend.md "Attract demo").
//!
//! The game: after 1800 VBlanks without menu input on the title or the main
//! menu (game mode 0x10; FUN_80074A94: 0x800B0AF0 > 0x707) it picks a
//! course (FUN_80089CA8, mr-sim title.rs `demo_course`), fades out and loads
//! it as a race with the demo flag 0x800B0BD2 (FUN_8008C4AC: six AI karts,
//! kart 0 too, 10 laps, the countdown from 0x3C, the trackside cameras; the
//! HUD only "DEMO" and a blinking "PRESS START"). The demo ends after one
//! minute of race clock (0x800B0A94 > 0x4650) or at kart 0's finish: the
//! title again. START (FUN_8007916C after LAB_80079A58) goes to that course's
//! menu when the course is open, else to the title.
//!
//! The port changes course in place (menus.rs `relaunch`, loading.rs):
//! the title starts the demo with `--course NAME --skip-title --demo
//! --demo-recent A,B,C` (the last three demo courses, 0x800B0A1C), the demo
//! goes back the same way.

use bevy::prelude::*;
use mr_assets::hud::{self as ahud, HudTables};
use mr_assets::hud_items::Prim;

/// This run is the attract demo race.
pub fn active() -> bool {
    crate::settings::args().into_iter().any(|a| a == "--demo") && !crate::title::on_title()
}

/// 0x800B0A1C..0x800B0A1E: the last three demo courses (`--demo-recent`).
pub fn recent() -> [u8; 3] {
    let mut r = mr_sim::title::DEMO_RECENT_INIT;
    if let Some(v) = crate::settings::arg_value("--demo-recent") {
        for (i, x) in v.split(',').take(3).enumerate() {
            if let Ok(n) = x.parse::<u8>() {
                r[i] = n;
            }
        }
    }
    r
}

/// The demo's 2D layer for frame counter `c` (0x800B0D90; FUN_80051698
/// cases 1 / 2 with 0x800B0BD2): "DEMO" at (0x100, 0x1E), size 1, colour 6;
/// "PRESS START" at (0x100, 0xD2) while c % 60 < 45 once the fade-in is
/// done (0x800B0F88).
pub fn text_sprites(t: &HudTables, c: i32) -> Vec<ahud::Sprite> {
    let mut v = t.text(b"DEMO", 0x100, 0x1E, 0, ahud::Size::from_arg(1), 6, 0x60);
    if c % crate::title::BLINK_PERIOD < crate::title::BLINK_ON && crate::title::fade_done(c) {
        v.extend(t.text(b"PRESS START", 0x100, 0xD2, 0, ahud::Size::from_arg(0), 4, 0x60));
    }
    v
}

#[derive(Resource, Default)]
struct DemoRes {
    hud: Option<HudTables>,
    prev: u16,
    /// 0x800B0AF0 on the title: VBlanks since the last menu input.
    idle: i32,
    last_frame: Option<i32>,
    done: bool,
}

pub struct DemoPlugin;

impl Plugin for DemoPlugin {
    fn build(&self, app: &mut App) {
        crate::boot::on_boot(app, |w| w.insert_resource(DemoRes::default()));
        app.add_systems(Update, (title_idle.run_if(crate::title::on_title), demo_run.run_if(active)));
    }
}

/// Never in tool runs, shots, or with `--no-demo`.
fn allowed() -> bool {
    let a: Vec<String> = crate::settings::args();
    !crate::menus::tool_run() && !crate::menus::scripted() && !a.iter().any(|x| x == "--no-demo" || x == "--title-shot")
}

fn title_idle(p: Res<crate::Player>, latch: Res<crate::pacing::PadLatch>, front: Res<crate::menus::Front>, mut res: ResMut<DemoRes>) {
    let crate::Driver::Sim(run) = &p.driver else { return };
    let s = &run.session;
    // FUN_8005C2EC adds dt each frame: the session's frame counter steps
    let f = s.globals.frame;
    let step = res.last_frame.map_or(0, |l| (f - l).max(0));
    res.last_frame = Some(f);
    res.idle += step;
    // FUN_80073138: a menu input (direction, confirm, back) zeroes it
    const KEYS: u16 = crate::input::UP | crate::input::DOWN | crate::input::LEFT | crate::input::RIGHT | crate::input::CROSS | crate::input::TRIANGLE | crate::input::START;
    let b = latch.now | latch.now2;
    if b & KEYS & !res.prev != 0 {
        res.idle = 0;
    }
    res.prev = b;
    use crate::menus::Page;
    let mode_10 = matches!(front.page, Page::None | Page::Main | Page::Options | Page::Leagues);
    if !mode_10 || res.idle < mr_sim::title::DEMO_IDLE || res.done || !allowed() {
        return;
    }
    res.done = true;
    let open6: [bool; 4] = crate::save::with(|st| std::array::from_fn(|t| st.block.course_open(6, t)));
    let mut rng = s.globals.rng;
    let mut recent = recent();
    let c = mr_sim::title::demo_course(&s.world, &mut rng, open6, &mut recent);
    let Some(info) = crate::courses::races().iter().find(|i| i.id == c) else {
        warn!("demo: course {c} not in the port's list");
        return;
    };
    info!("demo: idle {} VBlanks -> attract demo on {} (recent {recent:?})", res.idle, info.name);
    relaunch_demo(&info.name, recent);
}

fn demo_run(p: Res<crate::Player>, latch: Res<crate::pacing::PadLatch>, mut ov: ResMut<crate::title::TitleOverlay>, mut res: ResMut<DemoRes>) {
    let crate::Driver::Sim(run) = &p.driver else { return };
    let s = &run.session;
    if res.hud.is_none() {
        let mut disc = mr_disc::Disc::open_default().expect("open the disc");
        let exe = disc.read("SLUS_012.37").expect("read the executable");
        res.hud = HudTables::from_exe(&exe);
    }
    let c = s.globals.frame;
    let want = res.hud.as_ref().map(|t| text_sprites(t, c).into_iter().map(Prim::Sprt).collect::<Vec<_>>());
    if ov.0 != want {
        ov.0 = want;
    }
    if res.done {
        return;
    }
    let b = latch.now | latch.now2;
    let start = b & crate::input::START != 0 && res.prev & crate::input::START == 0 && !crate::movie::playing() && !crate::loading::busy();
    res.prev = b;
    if start {
        // the course's menu when it is open, else the title
        let course = s.config().course;
        let open = crate::save::with(|st| st.tables.place(course).is_some_and(|(w, t)| st.block.course_open(w, t)));
        info!("demo: START -> {}", if open { "the course menu" } else { "the title" });
        res.done = true;
        let name = open.then(|| crate::courses::info().name.clone());
        crate::menus::relaunch(name.as_deref(), 0, 0, false);
    } else if s.demo_over() {
        info!("demo: over (race clock {}) -> the title", s.globals.race_clock);
        res.done = true;
        crate::menus::relaunch(None, 0, 0, false);
    }
}

/// Change course in place to `course` as the attract demo (loading.rs).
fn relaunch_demo(course: &str, recent: [u8; 3]) {
    // (course_args ends with --character C --vehicle V: the demo has none)
    let mut out = crate::menus::course_args(None, 0, 0, false);
    out.truncate(out.len().saturating_sub(4));
    out.extend(["--course".into(), course.to_string(), "--skip-title".into(), "--demo".into(), "--demo-recent".into(), format!("{},{},{}", recent[0], recent[1], recent[2])]);
    crate::loading::change(out, None);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_text_layer() {
        let Ok(mut disc) = mr_disc::Disc::open_default() else { return };
        let exe = disc.read("SLUS_012.37").unwrap();
        let t = HudTables::from_exe(&exe).unwrap();
        // DEMO always; PRESS START blinking once the fade is done
        assert_eq!(text_sprites(&t, 10).len(), 4);
        assert_eq!(text_sprites(&t, 60).len(), 4 + 10);
        assert_eq!(text_sprites(&t, 110).len(), 4);
    }
}

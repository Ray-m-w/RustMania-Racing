//! Input sampling, logic rate and the display's look-ahead (Session 5,
//! track G; docs/spec/kart-physics-ledger.md "60 Hz mode", port/README.md).
//!
//! Options (settings.rs `KNOWN`, "G 60 Hz"):
//! - `logic_hz` 30 | 60: the simulation's tick rate. 30 = the game (each
//!   tick 2 VBlanks, exact against the original); 60 = the 60 Hz mode
//!   (`SessionConfig::logic_hz`, dt = 1). Live sessions only: replays and
//!   captures play at 30.
//! - `low_latency` true | false: the pad is sampled on every display frame
//!   (a button pressed and released between two ticks still reaches the
//!   next tick), and the display shows the tick being sampled instead of
//!   the last one: every frame it runs the coming tick on a copy of the
//!   session with the buttons held now (`Session::snapshot` / `restore`;
//!   the simulation itself is untouched, the copy is thrown away) and
//!   interpolates from the last real tick toward it. When the real tick
//!   runs with the same buttons it reaches exactly that state, so the
//!   motion stays continuous; a button change shows on the next frame
//!   instead of up to two ticks later. false = the Session 3/4 display
//!   (sampled at the tick, interpolated between the last two ticks).
//!
//! `--latency-log SECONDS`: measure input-to-display latency, print a
//! summary and exit. A synthetic LEFT press (0.3 s, every 0.7..1.2 s, phase
//! random) is mixed into the pad; for each press: the frame its sample was
//! taken, the first frame whose displayed state includes a tick computed with
//! it (interpolation weight > 0), and the first frame that shows such a tick
//! at full weight. Times are measured at the end of the app frame (Last);
//! the render + present chain after it is the same in every mode. The log
//! also times the render world's present call of those two frames
//! (`PresentLog`, after `RenderSystems::Render`): that part depends on
//! `render_latency` (pipelined rendering presents a frame one app frame
//! later); the swapchain queue after the present call (at most 1 frame with
//! `render_latency = low`, 2 otherwise) is not visible to the app.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use bevy::prelude::*;

use crate::karts::{KartFeed, KartFrame};
use crate::{input, Driver, Player, Pose};

/// The logic rate from the settings (30 or 60; default 60 since the owner
/// drove it, 2026-10-14).
pub fn logic_hz() -> u8 {
    match crate::settings::Settings::global().get("logic_hz") {
        Some("30") => 30,
        _ => 60,
    }
}

/// The `low_latency` option (default on).
pub fn low_latency() -> bool {
    crate::settings::Settings::global().bool("low_latency", true)
}

/// The pad as sampled every display frame, and what the ticks took from it.
#[derive(Resource, Default)]
pub struct PadLatch {
    /// This frame's sample.
    pub now: u16,
    /// Phase 7: player 2's pad this frame (split screen), and its presses
    /// since the last tick (low latency).
    pub now2: u16,
    pressed2: u16,
    /// Buttons that went down since the last tick took its buttons.
    pressed: u16,
    /// Counts changes of the sample (press / release events).
    pub gen: u64,
    /// The sample generation the last two ticks used (display endpoints in
    /// the classic mode; the committed endpoint in the low-latency mode).
    pub tick_gen: [u64; 2],
    low_latency: bool,
}

impl PadLatch {
    /// The buttons for the tick that runs now: this frame's sample, plus
    /// (low latency) any button pressed since the previous tick even if it
    /// was released again.
    pub fn take_for_tick(&mut self) -> u16 {
        let b = self.peek();
        self.pressed = 0;
        self.tick_gen = [self.tick_gen[1], self.gen];
        b
    }

    /// Player 2's buttons for the tick that runs now (as `take_for_tick`).
    pub fn take2_for_tick(&mut self) -> u16 {
        let b = if self.low_latency { self.now2 | self.pressed2 } else { self.now2 };
        self.pressed2 = 0;
        b
    }

    /// What `take_for_tick` would return now.
    pub fn peek(&self) -> u16 {
        if self.low_latency {
            self.now | self.pressed
        } else {
            self.now
        }
    }
}

/// The display's look-ahead state (low latency).
#[derive(Resource, Default)]
pub struct Lookahead {
    /// (session ticks, buttons) the shown prediction was made for.
    key: Option<(u64, u16)>,
    /// Sample generation the shown prediction used.
    pub pred_gen: u64,
    /// The last prediction failed (out of scope): the display shows the
    /// committed tick only.
    failed: bool,
}

/// PreUpdate, after Bevy's input systems: sample the pad.
pub fn sample(keys: Res<ButtonInput<KeyCode>>, pads: Query<&Gamepad>, pads2: Query<(Entity, &Gamepad)>, mut latch: ResMut<PadLatch>, mut log: Option<ResMut<LatencyLog>>) {
    // phase 7: player 2's pad always (the menus read it in player 2's
    // turn); split screen: the pads split between the players
    let (a, b) = input::sample_two(&keys, &pads2);
    latch.pressed2 |= b & !latch.now2;
    latch.now2 = b;
    let mut m = if input::two_players() { a } else { input::sample(&keys, &pads) };
    if let Some(l) = log.as_mut() {
        m |= l.inject();
    }
    if m != latch.now {
        latch.gen += 1;
        let went_down = m & !latch.now;
        latch.pressed |= went_down;
        if went_down != 0 {
            if let Some(l) = log.as_mut() {
                let g = latch.gen;
                l.pending.push(Press { gen: g, at: Instant::now(), first: None, first_frame: None });
            }
        }
    }
    latch.now = m;
}

/// The player's pose from a session (the same fields sim_tick reads).
pub fn session_pose(s: &mr_sim::session::Session) -> Pose {
    let r = s.record();
    let i32at = |o: usize| i32::from_le_bytes([r[o], r[o + 1], r[o + 2], r[o + 3]]);
    let mut m = [0i32; 9];
    for (i, v) in m.iter_mut().enumerate() {
        *v = i16::from_le_bytes([r[0x18C + 2 * i], r[0x18D + 2 * i]]) as i32;
    }
    let c = &s.camera;
    let mut cm = [0i32; 9];
    for (i, v) in cm.iter_mut().enumerate() {
        *v = c.matrix[i / 3][i % 3] as i32;
    }
    // phase 7: player 2's camera (split screen)
    let cam2 = s.camera2.as_ref().map(|c| {
        let mut cm = [0i32; 9];
        for (i, v) in cm.iter_mut().enumerate() {
            *v = c.matrix[i / 3][i % 3] as i32;
        }
        (c.pos, cm)
    });
    Pose { x: i32at(0), y: i32at(4), z: i32at(8), heading: i16::from_le_bytes([r[0xE], r[0xF]]) as i32, m, cam: Some((c.pos, cm)), cam2 }
}

fn frames(s: &mr_sim::session::Session) -> Vec<KartFrame> {
    s.kart_records().map(|(slot, _, _, r)| KartFrame::from_record(slot as u8, r)).collect()
}

/// RunFixedMainLoop, after the fixed ticks of this frame: (low latency)
/// the display endpoints become (last real tick, the coming tick run with
/// the buttons held now).
fn predict(mut p: ResMut<Player>, mut feed: ResMut<KartFeed>, latch: Res<PadLatch>, mut la: ResMut<Lookahead>, mut fx_la: Option<ResMut<crate::effects::FxLookahead>>, front: Option<Res<crate::menus::Front>>) {
    if !latch.low_latency {
        return;
    }
    // the course menu / character select (and THE END) run `menu_tick`,
    // not the race
    // (phase 11: and MEET THE MUPPETS)
    if front.is_some_and(|f| f.page.course_menu() || f.page == crate::menus::Page::Gallery) || crate::ending::on() {
        la.key = None;
        if let Some(f) = fx_la.as_mut() {
            f.0 = None;
        }
        return;
    }
    let Driver::Sim(run) = &mut p.driver else { return };
    // The buttons the next real tick will take (sim_tick's rules).
    let b = if run.session.ticks < run.align {
        0
    } else if let Some(r) = &run.route {
        let vb = run.session.vblanks_per_tick() as u64 * run.buttons.len() as u64;
        r.iter().take_while(|(v, _)| *v <= vb).last().map(|(_, m)| *m).unwrap_or(0)
    } else {
        latch.peek()
    };
    let key = (run.session.ticks, b);
    if la.key == Some(key) {
        return;
    }
    la.key = Some(key);
    la.pred_gen = latch.gen;
    let s = &mut run.session;
    let now_pose = session_pose(s);
    let now_frames = frames(s);
    let now_kart = s.record().to_vec();
    let snap = s.snapshot();
    let ok = s.tick(b).is_ok();
    let next = ok.then(|| (session_pose(s), frames(s), s.record().to_vec()));
    // the effects' state at the coming tick (effects.rs interpolates toward it)
    if let Some(f) = fx_la.as_mut() {
        f.0 = ok.then(|| crate::effects::FxSource::of(s));
    }
    s.restore(&snap);
    la.failed = !ok;
    let (pose, fr, kart) = next.unwrap_or_else(|| (now_pose, now_frames.clone(), now_kart.clone()));
    p.prev = now_pose;
    p.pose = pose;
    p.prev_kart = Some(now_kart);
    p.kart = Some(kart);
    feed.prev = now_frames;
    feed.now = fr;
}

struct Press {
    gen: u64,
    at: Instant,
    first: Option<f64>,
    /// The app frame that first showed it.
    first_frame: Option<u64>,
}

/// Present calls of the render world: (app frame number, time), for
/// `--latency-log` (shared with the render app).
#[derive(Resource, Clone, Default)]
pub struct PresentLog(Arc<Mutex<Vec<(u64, Instant)>>>);

/// The app frame number the render world is drawing.
#[derive(Resource, Default)]
struct RenderFrameNo(u64);

fn extract_frame_no(log: bevy::render::Extract<Option<Res<LatencyLog>>>, mut n: ResMut<RenderFrameNo>) {
    if let Some(l) = log.as_ref() {
        n.0 = l.frame;
    }
}

fn after_present(n: Res<RenderFrameNo>, pl: Res<PresentLog>) {
    if let Ok(mut v) = pl.0.lock() {
        v.push((n.0, Instant::now()));
    }
}

/// `--latency-log SECONDS` state.
#[derive(Resource)]
pub struct LatencyLog {
    secs: f64,
    start: Option<Instant>,
    frame: u64,
    rng: u32,
    /// Injected press: (frame it starts, frame it ends).
    window: (u64, u64),
    pending: Vec<Press>,
    first_ms: Vec<f64>,
    full_ms: Vec<f64>,
    frame_ms: Vec<f64>,
    last: Option<Instant>,
    /// Finished presses: (sample time, first frame showing it, frame at full weight).
    resolved: Vec<(Instant, u64, u64)>,
    presents: PresentLog,
}

impl LatencyLog {
    /// The synthetic press for this frame (LEFT, 0.3 s on, 0.7..1.2 s
    /// apart; frame counts at the measured display rate are not assumed:
    /// real time decides).
    fn inject(&mut self) -> u16 {
        let Some(t0) = self.start else { return 0 };
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        let now = ms as u64;
        if now >= self.window.1 {
            self.rng = self.rng.wrapping_mul(1_103_515_245).wrapping_add(12345);
            let gap = 700 + (self.rng >> 16) as u64 % 500;
            self.window = (now + gap, now + gap + 300);
        }
        if now >= self.window.0 && now < self.window.1 {
            input::LEFT
        } else {
            0
        }
    }
}

/// Last: which tick generations the frame shows; time the pending presses.
fn latency_frame(mut log: ResMut<LatencyLog>, latch: Res<PadLatch>, la: Res<Lookahead>, time: Res<Time<Fixed>>, p: Res<Player>, mut exit: MessageWriter<AppExit>) {
    let now = Instant::now();
    log.frame += 1;
    // Skip start-up (shader compiles) and the race load.
    if log.frame < 120 {
        return;
    }
    let started = *log.start.get_or_insert(now);
    if let Some(l) = log.last {
        let d = now.duration_since(l).as_secs_f64() * 1000.0;
        log.frame_ms.push(d);
    }
    log.last = Some(now);
    let a = time.overstep_fraction() as f64;
    // Display endpoints: (A = from, B = to); the frame shows A + a (B - A).
    let (ga, gb) = if latch.low_latency && !la.failed && matches!(p.driver, Driver::Sim(_)) {
        (latch.tick_gen[1], la.pred_gen)
    } else {
        (latch.tick_gen[0], latch.tick_gen[1])
    };
    let mut done = Vec::new();
    let frame_no = log.frame;
    for (i, pr) in log.pending.iter_mut().enumerate() {
        if pr.first.is_none() && (ga >= pr.gen || (a > 0.0 && gb >= pr.gen)) {
            pr.first = Some(now.duration_since(pr.at).as_secs_f64() * 1000.0);
            pr.first_frame = Some(frame_no);
        }
        if ga >= pr.gen {
            done.push((i, now.duration_since(pr.at).as_secs_f64() * 1000.0));
        }
    }
    for (i, full) in done.into_iter().rev() {
        let pr = log.pending.remove(i);
        let f = log.frame;
        log.resolved.push((pr.at, pr.first_frame.unwrap_or(f), f));
        log.first_ms.push(pr.first.unwrap_or(full));
        log.full_ms.push(full);
    }
    if now.duration_since(started).as_secs_f64() >= log.secs {
        let stat = |v: &mut Vec<f64>| {
            v.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let n = v.len().max(1);
            let mean = v.iter().sum::<f64>() / n as f64;
            let q = |p: f64| v.get(((n - 1) as f64 * p) as usize).copied().unwrap_or(f64::NAN);
            format!("mean {mean:.1} p50 {:.1} p95 {:.1} max {:.1}", q(0.5), q(0.95), q(1.0))
        };
        let n = log.first_ms.len();
        let (mut f, mut u, mut fr) = (log.first_ms.clone(), log.full_ms.clone(), log.frame_ms.clone());
        // present calls of those frames (render world)
        let pres: std::collections::HashMap<u64, Instant> = log.presents.0.lock().map(|v| v.iter().copied().collect()).unwrap_or_default();
        let (mut pf, mut pu) = (Vec::new(), Vec::new());
        for &(at, f0, f1) in &log.resolved {
            if let (Some(t0), Some(t1)) = (pres.get(&f0), pres.get(&f1)) {
                pf.push(t0.duration_since(at).as_secs_f64() * 1000.0);
                pu.push(t1.duration_since(at).as_secs_f64() * 1000.0);
            }
        }
        let fps = fr.len() as f64 / (fr.iter().sum::<f64>() / 1000.0).max(1e-9);
        let mut sorted = fr.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let p99 = sorted.get(((sorted.len().max(1) - 1) as f64 * 0.99) as usize).copied().unwrap_or(f64::NAN);
        eprintln!(
            "LATENCY-LOG logic {} Hz, low_latency {}, render_latency {}: {n} presses; sample -> first frame showing it (ms): {}; sample -> shown at full weight (ms): {}; sample -> present call, first frame (ms): {}; sample -> present call, full weight (ms): {}; frame (ms): {}; fps {fps:.1}, frame p99 {p99:.2} ms",
            logic_hz_running(&p),
            latch.low_latency,
            if crate::settings::Settings::global().render_latency_low() { "low" } else { "normal" },
            stat(&mut f),
            stat(&mut u),
            stat(&mut pf),
            stat(&mut pu),
            stat(&mut fr)
        );
        exit.write(AppExit::Success);
    }
}

fn logic_hz_running(p: &Player) -> u8 {
    match &p.driver {
        Driver::Sim(run) => run.session.logic_hz(),
        _ => 30,
    }
}

pub struct PacingPlugin;

impl Plugin for PacingPlugin {
    fn build(&self, app: &mut App) {
        let low = low_latency();
        app.insert_resource(PadLatch { low_latency: low, ..default() })
            .init_resource::<Lookahead>()
            .add_systems(PreUpdate, sample.after(bevy::input::InputSystems))
            .add_systems(RunFixedMainLoop, predict.in_set(RunFixedMainLoopSystems::AfterFixedMainLoop));
        if let Some(secs) = crate::settings::arg_value("--latency-log").and_then(|s| s.parse::<f64>().ok()) {
            let presents = PresentLog::default();
            if let Some(r) = app.get_sub_app_mut(bevy::render::RenderApp) {
                r.insert_resource(presents.clone())
                    .init_resource::<RenderFrameNo>()
                    .add_systems(bevy::render::ExtractSchedule, extract_frame_no)
                    .add_systems(bevy::render::Render, after_present.in_set(bevy::render::RenderSystems::Cleanup));
            }
            // measure at full rate even without focus (Bevy's game default
            // drops an unfocused window to 60 updates per second)
            app.insert_resource(bevy::winit::WinitSettings::continuous());
            app.insert_resource(LatencyLog {
                secs,
                start: None,
                frame: 0,
                rng: std::process::id(),
                window: (0, 0),
                pending: Vec::new(),
                first_ms: Vec::new(),
                full_ms: Vec::new(),
                frame_ms: Vec::new(),
                last: None,
                resolved: Vec::new(),
                presents,
            })
            .add_systems(Last, latency_frame);
        }
    }
}

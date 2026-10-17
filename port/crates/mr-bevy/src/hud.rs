//! Race HUD overlay: the game's own sprites (mr_assets::hud and
//! mr_assets::hud_items) drawn by an orthographic camera over the 3D view in
//! the game's 512x240 screen space, stretched to the window like the PS1
//! display. Unlit textured quads from the decoded VRAM (vramtex), nearest
//! sampling, texel 0 transparent; flat triangles and lines for the minimap
//! markers and the direction arrow; no bevy_text / bevy_ui.
//!
//! Input: `HudInput`, a `HudState` the rest of the app fills each frame. By
//! default `state_from_player` derives it from the player's kart record
//! (replay sidecar or mr-sim) plus `HudGlobals` (countdown, laps, race clock
//! - the race-flow code sets these; without them the HUD shows the race
//! with the kart's own race time +0x358). With a live mr-sim session the
//! item HUD (weapon slots, turbo meter, arrow, banners, minimap; docs/spec/
//! hud.md "Item HUD") comes from the session in `items_from_player`.
//!
//! Drawing: a pool of quad entities, one per primitive, whose meshes are
//! rewritten in place when the layout changes (the item HUD animates every
//! frame). The GPU draws the game's last-added primitive first, so earlier
//! primitives get a nearer z. Semi-transparent primitives are not meshes:
//! they go to the PS1 effects pass (psxfx.rs, `VIEW_HUD`) every frame, which
//! blends them with the GPU's ABR equations on the game's (gamma) values,
//! honours the texel STP bit, and is depth-tested against the opaque HUD
//! primitives (so the game's drawing order holds both ways).

use std::collections::HashMap;

use bevy::camera::visibility::RenderLayers;
use bevy::camera::{OrthographicProjection, ScalingMode};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;
use mr_assets::hud::{self, HudState, HudTables};
use mr_assets::hud_items::{self, HudPulse, ItemHud, ItemTables, MapKart, Prim};

use crate::vramtex::VramTextures;

/// Render layer of the HUD overlay.
pub const HUD_LAYER: usize = 3;
/// The game's screen.
pub const SCREEN_W: f32 = 512.0;
pub const SCREEN_H: f32 = 240.0;

/// Globals of the race flow the HUD reads (DAT_800B0C70 countdown,
/// DAT_800B07F8 its start, DAT_800B0DA6 laps, DAT_800B0A94 race clock,
/// DAT_800B0D46 mode). `race_time: None` = use the kart's +0x358.
#[derive(Resource, Clone, Copy, Debug)]
pub struct HudGlobals {
    pub countdown: i32,
    pub countdown_start: i32,
    pub laps: u8,
    pub mode: u8,
    pub race_time: Option<i32>,
    /// Minigame lit-icon wobble byte (shared random stream); 0 = steady.
    pub flicker: i8,
    /// After the player's finish: the results panel's inputs
    /// (`results_from_session`, or set by hand). None = no panel.
    pub results: Option<hud::Results>,
    /// The stunt trial's HUD inputs (`stunt_from_session`); None in a race.
    pub stunt: Option<hud::StuntHud>,
    /// A battle's HUD inputs (`battle_from_session`); None in a race.
    pub battle: Option<hud::BattleHud>,
    /// ADVENTURE's HUD inputs (`adventure_from_session`; phase 6).
    pub adventure: Option<hud::AdventureHud>,
    /// The "BEAT x" race's rival (race flag 0x10): the intro's name.
    pub rival: Option<u8>,
}

impl Default for HudGlobals {
    fn default() -> Self {
        HudGlobals { countdown: 0, countdown_start: 300, laps: 3, mode: 1, race_time: None, flicker: 0, results: None, stunt: None, battle: None, adventure: None, rival: None }
    }
}

/// The state drawn this frame. Set `manual` to stop `state_from_player`
/// from overwriting it.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct HudInput {
    pub state: Option<HudState>,
    pub manual: bool,
}

/// The item HUD's state this frame (None = minimal HUD only). Filled from a
/// live mr-sim session by `items_from_player` unless `manual`.
#[derive(Resource, Clone, Debug, Default)]
pub struct ItemHudInput {
    pub state: Option<ItemHud>,
    pub manual: bool,
}

/// Phase 7: the split-screen HUD's inputs (both players), filled by
/// `two_from_player` from a two-player session; None otherwise.
#[derive(Resource, Clone, Default)]
pub struct TwoHudInput {
    pub state: Option<TwoHud>,
}

#[derive(Clone)]
pub struct TwoHud {
    pub st: [HudState; 2],
    pub items: [ItemHud; 2],
    /// 0x800B0EBD (CHICKEN CHASE's catches to win).
    pub chicken: u8,
    /// "PRESS START" (mode 10): the results clock it blinks with.
    pub press: Option<i32>,
}

/// A full-screen layer drawn instead of the race HUD while set (the course
/// pick-list, courses.rs).
#[derive(Resource, Clone, Debug, Default)]
pub struct HudOverlay {
    pub prims: Option<Vec<Prim>>,
    /// Phase 11: TEST DRIVE: the item HUD (FUN_8004F688 case 0xE) under
    /// the overlay.
    pub test_drive: bool,
    /// Phase 13: no widescreen anchoring (the two-player character
    /// select's columns keep their 4:3 places, centred).
    pub centred: bool,
}

#[derive(Resource)]
struct HudRes {
    tables: HudTables,
    items: ItemTables,
    slots: [(u16, u16); 32],
    mats: HashMap<MatKey, Handle<StandardMaterial>>,
    pool: Vec<(Entity, Handle<Mesh>)>,
    last: Option<Vec<Prim>>,
    /// The semi-transparent primitives of the last layout (effects pass).
    fx: Vec<crate::psxfx::FxPrim>,
    /// Game pixels the view is wider than 512 (widescreen), split evenly
    /// left and right; the layout was last drawn for `last_extra`.
    extra: f32,
    last_extra: f32,
}

/// (texpage without abr, clut, textured, semi, abr)
type MatKey = (u16, u16, bool, bool, u8);

#[derive(Component)]
pub struct HudCamera;

#[derive(Component)]
struct HudQuad;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        crate::boot::on_boot(app, |w| {
            w.insert_resource(HudGlobals::default());
            w.insert_resource(HudInput::default());
            w.insert_resource(ItemHudInput::default());
            w.insert_resource(HudOverlay::default());
            w.insert_resource(TwoHudInput::default());
        });
        app.add_systems(crate::boot::Boot, setup.after(crate::scene::setup))
            .add_systems(Update, (state_from_player, items_from_player, two_from_player.after(state_from_player).after(items_from_player), fit_hud, draw.after(two_from_player).after(fit_hud)));
    }
}

fn setup(mut commands: Commands) {
    let mut disc = mr_disc::Disc::open_default().expect("open the disc");
    let exe = disc.read("SLUS_012.37").expect("read the executable");
    let tables = HudTables::from_exe(&exe).expect("HUD tables in the exe");
    let items = ItemTables::from_exe(&exe).expect("item HUD tables in the exe");
    // Slot table from the course RAW (the font page 0xF and slot 5 come with it).
    let path = mr_assets::raw::course_raw_path(&exe, crate::courses::id()).expect("course");
    let file = disc.read(&path).expect("read RAW");
    let env = mr_assets::raw::LoadEnv::from_exe(&exe, crate::courses::id()).expect("env");
    let dec = mr_assets::raw::decode(&file, &env).expect("decode RAW");
    let slots = hud::slot_attrs(&dec.slots);
    commands.insert_resource(HudRes { tables, items, slots, mats: HashMap::new(), pool: Vec::new(), last: None, fx: Vec::new(), extra: 0.0, last_extra: 0.0 });
    commands.spawn((
        Camera3d::default(),
        Camera { order: 10, clear_color: ClearColorConfig::None, ..default() },
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::Fixed { width: SCREEN_W, height: SCREEN_H },
            near: -100.0,
            far: 100.0,
            ..OrthographicProjection::default_3d()
        }),
        Tonemapping::None,
        crate::settings::Settings::global().msaa(),
        Transform::from_xyz(0.0, 0.0, 50.0),
        RenderLayers::layer(HUD_LAYER),
        HudCamera,
    ));
}

/// Widescreen: the HUD view is 240 game lines high and as wide as the
/// viewport's aspect allows (a 4:3 view = 512 game pixels, the PS1's
/// non-square pixels); the extra width goes half to each side and `draw`
/// pins every HUD element to its left / centre / right third (as the recomp
/// package's auto_ui_squash does: game.toml [widescreen]).
fn fit_hud(mut res: Option<ResMut<HudRes>>, mut cam: Query<(&Camera, &mut Projection), With<HudCamera>>) {
    let Some(res) = res.as_mut() else { return };
    let Ok((c, mut proj)) = cam.single_mut() else { return };
    let Some(size) = c.logical_viewport_size() else { return };
    if size.y <= 0.0 {
        return;
    }
    let width = (SCREEN_W * (size.x / size.y) / (4.0 / 3.0)).max(SCREEN_W);
    let extra = ((width - SCREEN_W) * 4.0).round() / 4.0;
    if (res.extra - extra).abs() > 0.01 {
        res.extra = extra;
    }
    if let Projection::Orthographic(o) = &mut *proj {
        let want = ScalingMode::Fixed { width: SCREEN_W + extra, height: SCREEN_H };
        if !matches!(o.scaling_mode, ScalingMode::Fixed { width, .. } if (width - (SCREEN_W + extra)).abs() < 0.01) {
            o.scaling_mode = want;
        }
    }
}

/// Screen-space box (x0, y0, x1, y1) of a primitive, game pixels.
fn prim_box(p: &Prim) -> [f32; 4] {
    let pts: Vec<[f32; 2]> = match *p {
        Prim::Sprt(s) => vec![[s.x as f32, s.y as f32], [(s.x + s.w) as f32, (s.y + s.h) as f32]],
        Prim::Ft4(q) => q.v.iter().map(|v| [v[0] as f32, v[1] as f32]).collect(),
        Prim::F3 { v, .. } => v.iter().map(|v| [v[0] as f32, v[1] as f32]).collect(),
        Prim::Line { v, .. } => v.iter().map(|v| [v[0] as f32, v[1] as f32]).collect(),
        Prim::Tile { x, y, w, h, .. } | Prim::Shade { x, y, w, h } => vec![[x as f32, y as f32], [(x + w) as f32, (y + h) as f32]],
    };
    let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for q in pts {
        b = [b[0].min(q[0]), b[1].min(q[1]), b[2].max(q[0]), b[3].max(q[1])];
    }
    b
}

/// Horizontal shift per primitive for a view `extra` game pixels wider than
/// 512: primitives are grouped into elements (boxes closer than 6 pixels
/// join, transitively: a word, the minimap with its markers, a slot with its
/// icon, the results panel), and each element whose box lies in the left
/// 30% of the screen moves left by extra/2, in the right 30% right by
/// extra/2; anything else (place, arrow, timer, banners, countdown, results)
/// stays centred.
pub fn anchor_shifts(prims: &[Prim], extra: f32) -> Vec<f32> {
    let n = prims.len();
    if extra <= 0.0 || n == 0 {
        return vec![0.0; n];
    }
    let boxes: Vec<[f32; 4]> = prims.iter().map(prim_box).collect();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(p: &mut [usize], i: usize) -> usize {
        let mut r = i;
        while p[r] != r {
            r = p[r];
        }
        let mut i = i;
        while p[i] != r {
            let nx = p[i];
            p[i] = r;
            i = nx;
        }
        r
    }
    const GAP: f32 = 6.0;
    for i in 0..n {
        for j in i + 1..n {
            let (a, b) = (boxes[i], boxes[j]);
            if a[0] - GAP <= b[2] && b[0] - GAP <= a[2] && a[1] - GAP <= b[3] && b[1] - GAP <= a[3] {
                let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                if ri != rj {
                    parent[ri] = rj;
                }
            }
        }
    }
    let mut groups: HashMap<usize, [f32; 4]> = HashMap::new();
    for i in 0..n {
        let r = find(&mut parent, i);
        let b = boxes[i];
        let g = groups.entry(r).or_insert(b);
        *g = [g[0].min(b[0]), g[1].min(b[1]), g[2].max(b[2]), g[3].max(b[3])];
    }
    (0..n)
        .map(|i| {
            let g = groups[&find(&mut parent, i)];
            if g[2] <= SCREEN_W * 0.3 {
                -extra / 2.0
            } else if g[0] >= SCREEN_W * 0.7 {
                extra / 2.0
            } else {
                0.0
            }
        })
        .collect()
}

/// Pull a sprite's UVs a quarter texel toward its middle. With MSAA the
/// fragment shader runs at the pixel centre even when only an edge sample
/// is covered, so UVs extrapolate up to half a pixel past the quad's edge
/// and point sampling picks the next texel row of the page (thin lines over
/// the item slots and the meter at 1080p). A quarter texel is below a pixel
/// at any HUD scale above 2x.
fn inset_uvs(uv: &mut [[f32; 2]]) {
    const D: f32 = 0.25 / 256.0;
    for axis in 0..2 {
        let lo = uv.iter().map(|t| t[axis]).fold(f32::MAX, f32::min);
        let hi = uv.iter().map(|t| t[axis]).fold(f32::MIN, f32::max);
        if hi - lo <= 2.0 * D {
            continue;
        }
        for t in uv.iter_mut() {
            if t[axis] <= lo {
                t[axis] += D;
            } else if t[axis] >= hi {
                t[axis] -= D;
            }
        }
    }
}

/// Default HUD state: the player's kart record + HudGlobals.
fn state_from_player(
    p: Res<crate::Player>,
    g: Res<HudGlobals>,
    cap: Option<Res<crate::capture::CaptureReplay>>,
    mut rows: Option<ResMut<crate::effects::CapRows>>,
    mut input: ResMut<HudInput>,
) {
    if input.manual {
        return;
    }
    let Some(rec) = p.kart.as_ref() else { return };
    let mut base = HudState {
        countdown: g.countdown,
        countdown_start: g.countdown_start,
        mode: g.mode,
        laps: g.laps,
        race_time: g.race_time.unwrap_or_else(|| hud::race_clock_from_kart(rec)),
        ..HudState::default()
    };
    // `--replay-capture`: the globals from the captured RAM (countdown,
    // intro, mode, laps, clock), so the replay shows the oracle's HUD phase
    if let (Some(cap), Some(rows)) = (cap.as_ref(), rows.as_mut()) {
        if let Some(m) = rows.mem(cap.vb) {
            base.countdown = m.i32(0x800B_0C70);
            base.countdown_start = m.i16(0x800B_07F8) as i32;
            base.mode = m.u8(0x800B_0D46);
            base.laps = m.i16(0x800B_0DA6) as u8;
            base.race_time = m.i32(0x800B_0A94);
        }
    }
    let mut st = base.with_kart(rec);
    if g.countdown != 0 {
        st.minigame = Some(hud::Minigame {
            button: rec[hud::kart::MINIGAME_BUTTON],
            count: i16::from_le_bytes([rec[hud::kart::MINIGAME_COUNT], rec[hud::kart::MINIGAME_COUNT + 1]]),
            flicker: g.flicker,
        });
    }
    st.results = g.results;
    st.stunt = g.stunt;
    st.battle = g.battle;
    st.adventure = g.adventure;
    st.rival = g.rival;
    if input.state != Some(st) {
        input.state = Some(st);
    }
}

/// The item HUD state from a live mr-sim session: the player's record,
/// every kart (minimap), the camera matrix (arrow), the frame counter and
/// race clock, and the HUD pulse (DAT_800B0E10, stepped here once per sim
/// frame the way FUN_8005C2EC does). Replays have no session: no item HUD.
fn items_from_player(
    p: Res<crate::Player>,
    cap: Option<Res<crate::capture::CaptureReplay>>,
    mut rows: Option<ResMut<crate::effects::CapRows>>,
    mut input: ResMut<ItemHudInput>,
    mut pulse: Local<(HudPulse, i32)>,
) {
    if input.manual {
        return;
    }
    if let (Some(cap), Some(rows)) = (cap.as_ref(), rows.as_mut()) {
        if let Some(m) = rows.mem(cap.vb) {
            input.state = Some(capture_items(&m));
        }
        return;
    }
    let crate::Driver::Sim(run) = &p.driver else {
        input.state = replay_items(&p, &mut *pulse);
        return;
    };
    let s = &run.session;
    let g = &s.globals;
    if g.frame != pulse.1 {
        if g.frame < pulse.1 {
            pulse.0 = HudPulse::default();
        }
        pulse.0.step(g.frame, g.dt.max(1));
        pulse.1 = g.frame;
    }
    let karts = s
        .kart_records()
        .map(|(_, _, _, r)| MapKart::from_record(r))
        .collect();
    let mut cam = [[0i16; 3]; 3];
    for (r, row) in s.camera.matrix.iter().enumerate() {
        for (c, v) in row.iter().enumerate() {
            cam[r][c] = *v as i16;
        }
    }
    input.state = Some(ItemHud {
        frame: g.frame,
        strip: g.frame,
        pulse: pulse.0,
        race_time: g.race_clock,
        countdown: g.countdown,
        mode: g.mode as u8,
        course: crate::courses::id() as u8,
        e38: g.cup as i16,
        special: special_globals(s),
        cam,
        kart: s.record().to_vec(),
        karts,
        players: g.humans.max(1) as usize,
        race_flags: g.race_flags,
        gauge_rgb: g.hud_gauge_rgb,
        chicken: (g.race_flags & 0x40 != 0).then(|| ([g.chicken.pos[0], g.chicken.pos[2]], g.chicken.strip)),
    });
}

/// Phase 7: both players' HUD inputs from a two-player session (player 2:
/// kart 1's record and camera 2's matrix).
fn two_from_player(p: Res<crate::Player>, g: Res<HudGlobals>, items: Res<ItemHudInput>, mut two: ResMut<TwoHudInput>) {
    let crate::Driver::Sim(run) = &p.driver else {
        two.state = None;
        return;
    };
    let s = &run.session;
    let (Some(cam2), Some(it0)) = (s.camera2.as_ref(), items.state.as_ref()) else {
        two.state = None;
        return;
    };
    if s.globals.humans < 2 || s.others.is_empty() {
        two.state = None;
        return;
    }
    let mk = |slot: usize| -> HudState {
        let rec = kart_of(s, slot).1;
        let mut st = HudState { countdown: g.countdown, countdown_start: g.countdown_start, mode: g.mode, laps: g.laps, race_time: s.race_clock(), ..HudState::default() }.with_kart(rec);
        if g.countdown != 0 {
            st.minigame = Some(hud::Minigame {
                button: rec[hud::kart::MINIGAME_BUTTON],
                count: i16::from_le_bytes([rec[hud::kart::MINIGAME_COUNT], rec[hud::kart::MINIGAME_COUNT + 1]]),
                flicker: g.flicker,
            });
        }
        st.results = results_for(s, slot);
        st.stunt = stunt_for(s, slot);
        st.battle = battle_for(s, slot);
        st
    };
    let mut it1 = it0.clone();
    it1.kart = s.others[0].record.clone();
    for (r, row) in cam2.matrix.iter().enumerate() {
        for (c, v) in row.iter().enumerate() {
            it1.cam[r][c] = *v as i16;
        }
    }
    two.state = Some(TwoHud {
        st: [mk(0), mk(1)],
        items: [it0.clone(), it1],
        chicken: s.globals.chicken.target,
        press: (s.globals.mode == 10).then_some(s.globals.results_clock),
    });
}

/// `--replay-capture` of an fx capture: the item HUD from the captured RAM
/// (every kart, the camera, the globals; frame counter and pulse as the
/// frame drew them is approximated by the row itself).
fn capture_items(m: &mr_sim::mem::Mem) -> ItemHud {
    let k0 = 0x800C_30A8u32;
    let n = (m.i16(0x800B_0DFE) + m.i16(0x800B_0C98)).clamp(1, 6) as u32;
    let karts = (0..n)
        .map(|k| {
            let a = k0 + 0x504 * k;
            MapKart::from_record(m.slice(a, 0x504))
        })
        .collect();
    let mut cam = [[0i16; 3]; 3];
    for (k, v) in cam.iter_mut().flatten().enumerate() {
        *v = m.i16(0x800C_4F40 + 2 * k as u32);
    }
    ItemHud {
        frame: m.i32(0x800B_0D90),
        strip: m.i32(0x800B_07D4),
        pulse: HudPulse([m.i16(0x800B_0E10), m.i16(0x800B_0E12)]),
        race_time: m.i32(0x800B_0A94),
        countdown: m.i32(0x800B_0C70),
        mode: m.u8(0x800B_0D46),
        course: m.u8(0x800B_0FAC),
        e38: m.i16(0x800B_0E38),
        special: (m.i8(0x800B_07F4) as i16, m.i16(0x800B_07F6), m.i8(0x800B_0E95)),
        cam,
        kart: m.slice(k0, 0x504).to_vec(),
        karts,
        players: m.i16(0x800B_0E28).max(1) as usize,
        race_flags: m.u16(0x800B_07E2),
        gauge_rgb: None,
        chicken: (m.u16(0x800B_07E2) & 0x40 != 0).then(|| ([m.i32(0x800C_B658), m.i32(0x800C_B660)], m.i32(0x800C_B67C))),
    }
}

/// A replayed oracle trace with its kart sidecar: the item HUD from the
/// player's record alone (frame counter = 2 per tick from the replay start,
/// the trace's camera; the minimap shows the player only).
fn replay_items(p: &crate::Player, pulse: &mut (HudPulse, i32)) -> Option<ItemHud> {
    let rec = p.kart.as_ref()?;
    let frame = (p.tick * 2) as i32;
    if frame != pulse.1 {
        if frame < pulse.1 {
            pulse.0 = HudPulse::default();
        }
        pulse.0.step(frame, 2);
        pulse.1 = frame;
    }
    let mut cam = [[0i16; 3]; 3];
    if let Some((_, m)) = p.pose.cam {
        for (k, v) in m.iter().enumerate() {
            cam[k / 3][k % 3] = *v as i16;
        }
    }
    let r = &rec[..];
    Some(ItemHud {
        frame,
        strip: frame,
        pulse: pulse.0,
        race_time: hud::race_clock_from_kart(r),
        countdown: 0,
        mode: 1,
        course: crate::courses::id() as u8,
        e38: 4,
        special: (-1, 0, 0),
        cam,
        kart: rec.clone(),
        karts: vec![MapKart::from_record(r)],
        players: 1,
        race_flags: 0,
        gauge_rgb: None,
        chicken: None,
    })
}

/// DAT_800B07F4 / DAT_800B07F6 / DAT_800B0E95 (the global special-move
/// announcement) from the session's item state (track A).
fn special_globals(s: &mr_sim::session::Session) -> (i16, i16, i8) {
    let it = s.items();
    (it.special as i16, it.special_clock, it.special_owner as i8)
}

/// The results panel's inputs from a live session once the player has
/// finished (else None): the player's record, the winner's lap times
/// (place 1 = DAT_800D8F94's kart), the frame counter 0x800B0D90 the
/// "PRESS START" prompt blinks with.
/// - `best_lap_record`: the game compares with its course record table
///   (0x800E0190, save data). Here: the player's best lap is the fastest
///   best lap of the race [INFER: in the oracle race the record equalled
///   the player's new best].
/// - `press_start`: from the finish on [INFER: DAT_800B0F88 was set
///   throughout the oracle's results screen].
pub fn results_from_session(s: &mr_sim::session::Session) -> Option<hud::Results> {
    results_for(s, 0)
}

/// Kart `slot`'s state and record (0 = player 1; phase 7: 1 = player 2).
fn kart_of(s: &mr_sim::session::Session, slot: usize) -> (&mr_sim::KartState, &[u8]) {
    if slot == 0 {
        (&s.kart, s.record())
    } else {
        let o = &s.others[slot - 1];
        (&o.kart, &o.record)
    }
}

/// `results_from_session` for human `slot` (phase 7: split screen).
pub fn results_for(s: &mr_sim::session::Session, slot: usize) -> Option<hud::Results> {
    let (k, rec) = kart_of(s, slot);
    if k.finished == 0 {
        return None;
    }
    let laps = s.globals.laps.clamp(1, 99) as u8;
    let rows = s.results();
    let winner = rows.iter().find(|r| r.place == 1).map(|r| r.slot).unwrap_or(0);
    let mut leader: &[u8] = rec;
    for (sl, _, _, r) in s.kart_records() {
        if sl == winner {
            leader = r;
        }
    }
    let mut r = hud::Results::from_records(rec, leader, laps);
    let fastest = rows.iter().map(|k| k.best_lap).filter(|&b| b > 0).min().unwrap_or(0);
    // (no flash in a REVERSE RACE, race flag 2: FUN_8004CEA0)
    r.best_lap_record = r.best_lap > 0 && r.best_lap <= fastest && s.globals.race_flags & 2 == 0;
    r.press_start = true;
    // FUN_80051698 case 10: the prompt blinks with 0x800B0AF0 (VBlanks
    // since the results began; oracle stunt/fx, phase 3)
    r.frame = s.globals.results_clock;
    // the "BEAT x" race (race flag 0x10): "YOU BEAT x" / "x BEAT YOU"
    if s.globals.race_flags & 0x10 != 0 && s.globals.adv.tagged >= 0 {
        r.rival = Some(s.globals.adv.tagged as u8);
        r.rival_blink = s.globals.results_clock;
    }
    // phase 11: the results taunt's faces (one player; audio.rs)
    if s.globals.split_screen == 0 && crate::audio::TAUNT_PLAYING.load(std::sync::atomic::Ordering::Relaxed) {
        let b = s.globals.items.blink;
        r.taunt = Some([b[0] & 1, b[2] & 1]);
    }
    Some(r)
}

/// ADVENTURE's HUD inputs from a live session (game mode 0xB; phase 6):
/// what is left to collect, the time limit, what ended the run.
pub fn adventure_from_session(s: &mr_sim::session::Session) -> Option<hud::AdventureHud> {
    let g = &s.globals;
    if g.challenge == 0 || crate::title::on_title() {
        return None;
    }
    let k = &s.kart;
    Some(hud::AdventureHud {
        course: s.world.info.id,
        course_flags: g.adv.course_flags,
        boxes_left: g.items.boxes as i32 - k.adv_boxes as i32,
        tokens_left: g.items.tokens as i32 - k.adv_tokens as i32,
        targets_left: g.items.challenge[0] as i32 - k.adv_targets as i32,
        token_colour: k.token_colour,
        token_spin: g.items.token_spin[0],
        time_limit: g.adv.time_limit,
        done: g.adv.done,
        stunt: g.race_type == 3,
        // phase 6b: the target marker (0x800B068F while that target is out)
        marker: {
            let m = g.adv.marker.max(0) as usize;
            let n = g.items.challenge[0] as usize;
            let out = n > 0 && g.adv.course_flags & 4 == 0 && g.items.pickups.get(0x30 + m).is_some_and(|p| p.state == 1);
            s.world.pickups.challenge.as_ref().and_then(|l| l.points.get(m)).filter(|_| out && m < n).map(|p| (p[0], p[2], mr_sim::stunt::strip_progress(&s.world, p[0], p[2])))
        },
        bar: (g.race_type == 3 && mr_sim::stunt::strip_quads(&s.world) != 0).then(|| {
            let mk = mr_sim::stunt::banner_marks(&s.world);
            hud::StuntHud {
                progress: k.strip as i32 * 0x80 + k.strip_along as i32,
                quads: mr_sim::stunt::strip_quads(&s.world),
                marks: [mk.first().copied().unwrap_or(0), mk.get(1).copied().unwrap_or(0)],
                n_marks: mk.len() as u8,
                race_flags: g.race_flags,
                character: k.character,
                kart_flags: k.flags,
                ..Default::default()
            }
        }),
    })
}

/// The stunt trial's HUD inputs from a live session (race type 3; else
/// None): the kart's place on the strip, the strip's length and banner
/// marks (mr_sim::stunt), the result's time / place and the course record
/// (the session's, lowered by this run's finish as the game's save block).
/// 0x800B0C20 (the first finisher's time) is the player's own (one kart).
pub fn stunt_from_session(s: &mr_sim::session::Session) -> Option<hud::StuntHud> {
    stunt_for(s, 0)
}

/// `stunt_from_session` for human `slot`; with two players the gap is to
/// the first finisher (0x800B0C20).
pub fn stunt_for(s: &mr_sim::session::Session, slot: usize) -> Option<hud::StuntHud> {
    if s.globals.race_type != 3 || s.globals.challenge != 0 || s.globals.race_flags & 0x40 != 0 {
        return None;
    }
    let k = kart_of(s, slot).0;
    let m = mr_sim::stunt::banner_marks(&s.world);
    Some(hud::StuntHud {
        progress: k.strip as i32 * 0x80 + k.strip_along as i32,
        quads: mr_sim::stunt::strip_quads(&s.world),
        marks: [m.first().copied().unwrap_or(0), m.get(1).copied().unwrap_or(0)],
        n_marks: m.len() as u8,
        race_flags: s.globals.race_flags,
        character: k.character,
        kart_flags: k.flags,
        time: k.race_time,
        flash: k.finished_time,
        place: k.race_pos,
        shown_place: k.shown_pos,
        record: s.globals.lap_record,
        first_time: if s.globals.humans > 1 && s.globals.first_finish >= 0 { s.globals.first_finish } else { k.race_time },
    })
}

/// A battle's HUD inputs from a live session (race type 2; else None):
/// the result's damage dealt / taken, best crash, knock-outs, page counter
/// +0x38C, place, clock and the course record (the session's, lowered by
/// this win as the game's save block).
pub fn battle_from_session(s: &mr_sim::session::Session) -> Option<hud::BattleHud> {
    battle_for(s, 0)
}

/// `battle_from_session` for human `slot` (not in CHICKEN CHASE).
pub fn battle_for(s: &mr_sim::session::Session, slot: usize) -> Option<hud::BattleHud> {
    // (phase 6b: BATTLE on a race course, race flag 1, game mode 2)
    if (s.globals.race_type != 2 && s.globals.race_flags & 1 == 0) || s.globals.challenge != 0 || crate::title::on_title() || s.globals.mode == 0x14 || s.globals.race_flags & 0x40 != 0 {
        return None;
    }
    let k = kart_of(s, slot).0;
    // A BOSS CHALLENGE (game mode 7): the bosses past 0x800B0DFE
    let nb = if s.config().boss_challenge { s.globals.adv.bosses.clamp(0, 3) as usize } else { 0 };
    let first = s.others.len() + 1 - nb;
    let boss_damage: [i16; 3] = std::array::from_fn(|i| if i < nb { s.others[first + i - 1].kart.stun } else { 0 });
    Some(hud::BattleHud {
        dealt: k.hits_dealt,
        taken: k.stun,
        best_crash: k.max_hit,
        knockouts: k.knockouts,
        counter: k.finished_time,
        shown_place: k.shown_pos,
        place: k.race_pos,
        time: k.race_time,
        record: s.globals.lap_record,
        race_flags: s.globals.race_flags,
        course: s.world.info.id,
        boss_count: nb as u8,
        boss_damage,
        race_clock: s.race_clock(),
    })
}

/// Screen (game pixels, y down) -> HUD camera space.
fn sp(x: f32, y: f32, z: f32) -> [f32; 3] {
    [x - SCREEN_W / 2.0, SCREEN_H / 2.0 - y, z]
}

/// One primitive as (material key, positions, uvs, colours); quads as two
/// triangles (v0 v1 v2, v1 v3 v2), lines as a 1-pixel-wide quad.
fn geometry(p: &Prim, slots: &[(u16, u16); 32], z: f32) -> (MatKey, Vec<[f32; 3]>, Vec<[f32; 2]>, [f32; 4]) {
    // The GPU modulates in its own (gamma) space: texel * c / 128. The
    // textures are sRGB, so in linear space the factor is (c / 128)^2.2.
    let col = |rgb: [u8; 3], raw: bool, semi: bool, abr: u8| {
        let k = |c: u8| if raw { 1.0 } else { (c as f32 / 128.0).powf(2.2) };
        // abr 3 (B + F/4): additive at a quarter; abr 0: 50% blend
        let q = if semi && abr == 3 { 0.25 } else { 1.0 };
        [k(rgb[0]) * q, k(rgb[1]) * q, k(rgb[2]) * q, if semi && abr == 0 { 0.5 } else { 1.0 }]
    };
    // flat colours are the game's 8-bit (sRGB) values
    let flat = |rgb: [u8; 3]| {
        let c = Color::srgb_u8(rgb[0], rgb[1], rgb[2]).to_linear();
        [c.red, c.green, c.blue, 1.0]
    };
    let uvf = |u: u8, v: u8| [u as f32 / 256.0, v as f32 / 256.0];
    match *p {
        Prim::Sprt(s) => {
            let (tp, cl) = s.attrs(slots);
            let (x0, y0, x1, y1) = (s.x as f32, s.y as f32, (s.x + s.w) as f32, (s.y + s.h) as f32);
            let (u0, v0) = (s.u as f32 / 256.0, s.v as f32 / 256.0);
            let (u1, v1) = (u0 + s.uw as f32 / 256.0, v0 + s.vh as f32 / 256.0);
            let pos = vec![sp(x0, y0, z), sp(x1, y0, z), sp(x0, y1, z), sp(x1, y0, z), sp(x1, y1, z), sp(x0, y1, z)];
            let uv = vec![[u0, v0], [u1, v0], [u0, v1], [u1, v0], [u1, v1], [u0, v1]];
            ((tp & !0x60, cl, true, s.semi, s.abr), pos, uv, col(s.rgb, s.raw, s.semi, s.abr))
        }
        Prim::Ft4(q) => {
            let (tp, cl) = slots[q.slot as usize & 31];
            let cl = cl.wrapping_add(q.clut_off);
            let v = |i: usize| sp(q.v[i][0] as f32, q.v[i][1] as f32, z);
            // texel centres: the GPU samples u..u+uw-1; a mirrored quad
            // (u1 < u0) runs right to left
            let t = |i: usize| uvf(q.uv[i][0], q.uv[i][1]);
            let pos = vec![v(0), v(1), v(2), v(1), v(3), v(2)];
            let uv = vec![t(0), t(1), t(2), t(1), t(3), t(2)];
            ((tp & !0x60, cl, true, q.semi, q.abr), pos, uv, col(q.rgb, false, q.semi, q.abr))
        }
        Prim::F3 { v, rgb } => {
            let p = |i: usize| sp(v[i][0] as f32, v[i][1] as f32, z);
            ((0, 0, false, false, 0), vec![p(0), p(1), p(2)], vec![[0.0, 0.0]; 3], flat(rgb))
        }
        Prim::Tile { x, y, w, h, rgb } => {
            let (x0, y0, x1, y1) = (x as f32, y as f32, (x + w) as f32, (y + h) as f32);
            let pos = vec![sp(x0, y0, z), sp(x1, y0, z), sp(x0, y1, z), sp(x1, y0, z), sp(x1, y1, z), sp(x0, y1, z)];
            ((0, 0, false, false, 0), pos, vec![[0.0, 0.0]; 6], flat(rgb))
        }
        Prim::Shade { x, y, w, h } => {
            let (x0, y0, x1, y1) = (x as f32, y as f32, (x + w) as f32, (y + h) as f32);
            let pos = vec![sp(x0, y0, z), sp(x1, y0, z), sp(x0, y1, z), sp(x1, y0, z), sp(x1, y1, z), sp(x0, y1, z)];
            ((0, 0, false, true, 0), pos, vec![[0.0, 0.0]; 6], [0.0, 0.0, 0.0, 0.5])
        }
        Prim::Line { v, rgb } => {
            let (a, b) = (Vec2::new(v[0][0] as f32 + 0.5, v[0][1] as f32 + 0.5), Vec2::new(v[1][0] as f32 + 0.5, v[1][1] as f32 + 0.5));
            let d = (b - a).normalize_or_zero();
            let n = Vec2::new(-d.y, d.x) * 0.5;
            let c = [a + n, a - n, b + n, b - n];
            let p = |i: usize| sp(c[i].x, c[i].y, z);
            ((0, 0, false, false, 0), vec![p(0), p(1), p(2), p(1), p(3), p(2)], vec![[0.0, 0.0]; 6], flat(rgb))
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    input: Res<HudInput>,
    items: Res<ItemHudInput>,
    overlay: Res<HudOverlay>,
    two: Res<TwoHudInput>,
    res: Option<ResMut<HudRes>>,
    tex: Option<ResMut<VramTextures>>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut vis: Query<(&mut Visibility, &mut MeshMaterial3d<StandardMaterial>), With<HudQuad>>,
    mut fx: ResMut<crate::psxfx::FxPrims>,
) {
    let (Some(mut res), Some(mut tex)) = (res, tex) else { return };
    let prims: Vec<Prim> = match (overlay.prims.as_ref(), input.state, items.state.as_ref()) {
        (Some(o), _, Some(it)) if overlay.test_drive => {
            let mut v = o.clone();
            v.extend(hud_items::test_drive_hud(&res.tables, &res.items, it));
            v
        }
        (Some(o), _, _) => o.clone(),
        (None, _, _) if two.state.is_some() => {
            let t = two.state.as_ref().unwrap();
            let v = hud_items::two_hud(&res.tables, &res.items, [&t.st[0], &t.st[1]], [&t.items[0], &t.items[1]], t.chicken, t.press);
            if std::env::var("MRM_HUD_DEBUG").is_ok() {
                eprintln!("two_hud mode {} cd {} fin {} {} prims {}", t.st[0].mode, t.st[0].countdown, t.st[0].finished, t.st[1].finished, v.len());
            }
            v
        }
        (None, Some(st), Some(it)) => hud_items::race_hud(&res.tables, &res.items, &st, it),
        (None, Some(st), None) => res.tables.layout(&st).into_iter().map(Prim::Sprt).collect(),
        _ => Vec::new(),
    };
    if res.last.as_ref() == Some(&prims) && res.last_extra == res.extra {
        fx.list.extend(res.fx.iter().cloned());
        return;
    }
    let n = prims.len();
    let shifts = if overlay.centred && overlay.prims.is_some() { vec![0.0; n] } else { anchor_shifts(&prims, res.extra) };
    res.fx.clear();
    for (i, p) in prims.iter().enumerate() {
        let z = (n - i) as f32 * 0.01;
        let (key, mut pos, mut uv, c) = geometry(p, &res.slots, z);
        for q in pos.iter_mut() {
            q[0] += shifts[i];
        }
        if key.2 {
            inset_uvs(&mut uv);
        }
        if key.3 {
            // semi-transparent: the effects pass (quads only: SPRT / FT4);
            // the pooled entity stays hidden
            res.fx.push(fx_prim(p, key, &pos, &uv));
        }
        let mat = match res.mats.get(&key) {
            Some(m) => m.clone(),
            None => {
                let (tp, cl, textured, semi, abr) = key;
                let m = mats.add(StandardMaterial {
                    base_color: Color::WHITE,
                    base_color_texture: textured.then(|| tex.get_nearest(&mut images, tp, cl)),
                    unlit: true,
                    cull_mode: None,
                    alpha_mode: match (semi, abr) {
                        (true, 0) => AlphaMode::Blend,
                        // B + F and B + F/4 additive; B - F approximated by a
                        // 50% blend (render ledger)
                        (true, 1) | (true, 3) => AlphaMode::Add,
                        (true, _) => AlphaMode::Blend,
                        _ => AlphaMode::Mask(0.5),
                    },
                    ..default()
                });
                res.mats.insert(key, m.clone());
                m
            }
        };
        let colours = vec![c; pos.len()];
        let normals = vec![[0.0, 0.0, 1.0]; pos.len()];
        if i >= res.pool.len() {
            let mut m = Mesh::new(bevy::mesh::PrimitiveTopology::TriangleList, bevy::asset::RenderAssetUsages::default());
            m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
            m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
            m.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
            m.insert_attribute(Mesh::ATTRIBUTE_COLOR, colours);
            let h = meshes.add(m);
            let show = if key.3 { Visibility::Hidden } else { Visibility::Inherited };
            let e = commands.spawn((Mesh3d(h.clone()), MeshMaterial3d(mat), Transform::IDENTITY, show, RenderLayers::layer(HUD_LAYER), HudQuad)).id();
            res.pool.push((e, h));
        } else {
            let (e, h) = res.pool[i].clone();
            if let Some(mut m) = meshes.get_mut(&h) {
                m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
                m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
                m.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
                m.insert_attribute(Mesh::ATTRIBUTE_COLOR, colours);
            }
            if let Ok((mut v, mut mm)) = vis.get_mut(e) {
                let show = if key.3 { Visibility::Hidden } else { Visibility::Inherited };
                if *v != show {
                    *v = show;
                }
                if mm.0 != mat {
                    mm.0 = mat;
                }
            }
        }
    }
    for &(e, _) in res.pool.iter().skip(n) {
        if let Ok((mut v, _)) = vis.get_mut(e) {
            if *v != Visibility::Hidden {
                *v = Visibility::Hidden;
            }
        }
    }
    res.last = Some(prims);
    res.last_extra = res.extra;
    fx.list.extend(res.fx.iter().cloned());
}

/// A semi-transparent SPRT / FT4 as an effects-pass quad (HUD camera space):
/// the GPU's modulation texel * rgb / 128 (raw sprites unmodulated), ABR from
/// the primitive. `pos` / `uv` are `geometry`'s two triangles (v0 v1 v2, v1
/// v3 v2).
fn fx_prim(p: &Prim, key: MatKey, pos: &[[f32; 3]], uv: &[[f32; 2]]) -> crate::psxfx::FxPrim {
    use crate::psxfx::{Blend, FxPrim, FxVert, VIEW_HUD};
    let (rgb, raw) = match *p {
        Prim::Sprt(s) => (s.rgb, s.raw),
        Prim::Ft4(q) => (q.rgb, false),
        _ => ([128; 3], false),
    };
    let k = |c: u8| if raw { 1.0 } else { c as f32 / 128.0 };
    let corner = [0usize, 1, 2, 4];
    let v: [FxVert; 4] = std::array::from_fn(|j| {
        let (q, t) = (pos[corner[j]], uv[corner[j]]);
        FxVert { pos: Vec3::from_array(q), uv: [t[0] * 256.0, t[1] * 256.0], rgb: [k(rgb[0]), k(rgb[1]), k(rgb[2])] }
    });
    FxPrim {
        v,
        page: Some((key.0, key.1)),
        blend: Blend::Abr(key.4 & 3),
        center: (v[0].pos + v[1].pos + v[2].pos + v[3].pos) * 0.25,
        view: VIEW_HUD,
        always_depth: true,
        ..Default::default()
    }
}

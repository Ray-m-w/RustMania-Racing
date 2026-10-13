//! Race HUD overlay: the game's own sprites (mr_assets::hud) drawn by an
//! orthographic camera over the 3D view in the game's 512x240 screen space,
//! stretched to the window like the PS1 display. Unlit textured quads from
//! the decoded VRAM (vramtex), nearest sampling, texel 0 transparent; no
//! bevy_text / bevy_ui.
//!
//! Input: `HudInput`, a `HudState` the rest of the app fills each frame. By
//! default `state_from_player` derives it from the player's kart record
//! (replay sidecar or mr-sim) plus `HudGlobals` (countdown, laps, race clock
//! - the race-flow code sets these; without them the HUD shows the race
//! with the kart's own race time +0x358).

use std::collections::HashMap;

use bevy::camera::visibility::RenderLayers;
use bevy::camera::{OrthographicProjection, ScalingMode};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;
use mr_assets::hud::{self, HudState, HudTables, Sprite};

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
}

impl Default for HudGlobals {
    fn default() -> Self {
        HudGlobals { countdown: 0, countdown_start: 300, laps: 3, mode: 1, race_time: None, flicker: 0 }
    }
}

/// The state drawn this frame. Set `manual` to stop `state_from_player`
/// from overwriting it.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct HudInput {
    pub state: Option<HudState>,
    pub manual: bool,
}

#[derive(Resource)]
struct HudRes {
    tables: HudTables,
    slots: [(u16, u16); 32],
    mats: HashMap<(u16, u16, [u8; 3], bool, bool, u8), Handle<StandardMaterial>>,
    meshes: HashMap<(i32, i32, u8, u8, i32, i32), Handle<Mesh>>,
    last: Option<Vec<Sprite>>,
}

#[derive(Component)]
pub struct HudCamera;

#[derive(Component)]
struct HudQuad;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HudGlobals>()
            .init_resource::<HudInput>()
            .add_systems(Startup, setup.after(crate::scene::setup))
            .add_systems(Update, (state_from_player, draw.after(state_from_player)));
    }
}

fn setup(mut commands: Commands) {
    let mut disc = mr_disc::Disc::open_default().expect("open the disc");
    let exe = disc.read("SLUS_012.37").expect("read the executable");
    let tables = HudTables::from_exe(&exe).expect("HUD tables in the exe");
    // Slot table from the course RAW (the font page 0xF and slot 5 come with it).
    let path = mr_assets::raw::course_raw_path(&exe, crate::scene::DOCK).expect("course");
    let file = disc.read(&path).expect("read RAW");
    let env = mr_assets::raw::LoadEnv::from_exe(&exe, crate::scene::DOCK).expect("env");
    let dec = mr_assets::raw::decode(&file, &env).expect("decode RAW");
    let slots = hud::slot_attrs(&dec.slots);
    commands.insert_resource(HudRes { tables, slots, mats: HashMap::new(), meshes: HashMap::new(), last: None });
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
        Transform::from_xyz(0.0, 0.0, 50.0),
        RenderLayers::layer(HUD_LAYER),
        HudCamera,
    ));
}

/// Default HUD state: the player's kart record + HudGlobals.
fn state_from_player(p: Res<crate::Player>, g: Res<HudGlobals>, mut input: ResMut<HudInput>) {
    if input.manual {
        return;
    }
    let Some(rec) = p.kart.as_ref() else { return };
    let base = HudState {
        countdown: g.countdown,
        countdown_start: g.countdown_start,
        mode: g.mode,
        laps: g.laps,
        race_time: g.race_time.unwrap_or_else(|| hud::race_clock_from_kart(rec)),
        ..HudState::default()
    };
    let mut st = base.with_kart(rec);
    if g.countdown != 0 {
        st.minigame = Some(hud::Minigame {
            button: rec[hud::kart::MINIGAME_BUTTON],
            count: i16::from_le_bytes([rec[hud::kart::MINIGAME_COUNT], rec[hud::kart::MINIGAME_COUNT + 1]]),
            flicker: g.flicker,
        });
    }
    if input.state != Some(st) {
        input.state = Some(st);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    input: Res<HudInput>,
    res: Option<ResMut<HudRes>>,
    tex: Option<ResMut<VramTextures>>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    old: Query<Entity, With<HudQuad>>,
) {
    let (Some(mut res), Some(mut tex)) = (res, tex) else { return };
    let sprites = input.state.map(|s| res.tables.layout(&s)).unwrap_or_default();
    if res.last.as_ref() == Some(&sprites) {
        return;
    }
    for e in old.iter() {
        commands.entity(e).despawn();
    }
    let n = sprites.len();
    for (i, s) in sprites.iter().enumerate() {
        let (tp, cl) = s.attrs(&res.slots);
        // texture: the 256x256 texel space of the page through the CLUT
        let page = tex.get(&mut images, tp & !0x60, cl);
        let mkey = (tp & !0x60, cl, s.rgb, s.raw, s.semi, s.abr);
        let mat = res
            .mats
            .entry(mkey)
            .or_insert_with(|| {
                let k = |c: u8| if s.raw { 1.0 } else { c as f32 / 128.0 };
                let a = if s.semi && s.abr == 0 { 0.5 } else { 1.0 };
                mats.add(StandardMaterial {
                    base_color: Color::linear_rgba(k(s.rgb[0]), k(s.rgb[1]), k(s.rgb[2]), a),
                    base_color_texture: Some(page),
                    unlit: true,
                    cull_mode: None,
                    alpha_mode: if s.semi && s.abr == 0 { AlphaMode::Blend } else if s.semi && s.abr == 1 { AlphaMode::Add } else { AlphaMode::Mask(0.5) },
                    ..default()
                })
            })
            .clone();
        let qkey = (s.w, s.h, s.u, s.v, s.uw, s.vh);
        let mesh = res
            .meshes
            .entry(qkey)
            .or_insert_with(|| {
                let (u0, v0) = (s.u as f32 / 256.0, s.v as f32 / 256.0);
                let (u1, v1) = (u0 + s.uw as f32 / 256.0, v0 + s.vh as f32 / 256.0);
                let (w, h) = (s.w as f32, s.h as f32);
                let mut m = Mesh::new(bevy::mesh::PrimitiveTopology::TriangleList, bevy::asset::RenderAssetUsages::default());
                m.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0, 0.0, 0.0], [w, 0.0, 0.0], [0.0, -h, 0.0], [w, -h, 0.0]]);
                m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; 4]);
                m.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[u0, v0], [u1, v0], [u0, v1], [u1, v1]]);
                m.insert_indices(bevy::mesh::Indices::U16(vec![0, 2, 1, 1, 2, 3]));
                meshes.add(m)
            })
            .clone();
        // The GPU draws the last-added primitive first (ordering-table
        // prepend), so earlier sprites sit in front: nearer z.
        let z = (n - i) as f32 * 0.01;
        commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(mat),
            Transform::from_xyz(s.x as f32 - SCREEN_W / 2.0, SCREEN_H / 2.0 - s.y as f32, z),
            RenderLayers::layer(HUD_LAYER),
            HudQuad,
        ));
    }
    res.last = Some(sprites);
}

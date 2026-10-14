//! The title screen (Session 6, track T): the game's first interactive
//! screen, before the race. docs/spec/frontend.md has the code and data.
//!
//! What the game does (course 0x1E = `frontend\options`, game mode 0x10,
//! title sub-state 0x800B0E8A = 0; FUN_8008ACE4 sets them after "BACK TO MAIN
//! MENU" or at boot):
//! - the scene is the frontend "course": FRONTEND/OPTIONS.RAW (VRAM, slot
//!   table) and OPTIONS.DAT (427 layer-0 objects: the orange disc with the
//!   3D "MUPPET RaceMania" logo, the chequered floor, the ring of stars and
//!   fruit), drawn by the race renderer; frame clear black (course table row
//!   30: colour 0, 0, 0);
//! - the camera is mode 1 (FUN_80070418 @0x800705A0: course 0x1E + mode
//!   0x10), an orbit along two DAT point loops: eye = chunk 0x0B
//!   (DAT_800D1C74), look-at = chunk 0x0A (DAT_800D1C70), both evaluated by
//!   FUN_80081BF4 (linear between points, closed loop, parameter 0..0x7FFF);
//!   parameter t = rand() at the mode change, then t -= 64 * dt per tick
//!   (DAT_800B0D58 = 16 << 2 because the path length FUN_80081DC4 returns 0;
//!   look-at at t + 0x400 = DAT_800B0A88); the eye follows its goal with the
//!   common smoothing (weights 32/1024 per VBlank) and the angles turn by
//!   1/8 of the error per VBlank (0x80071D34 tail, as mr_sim::camera);
//! - fade in from black (FUN_8007D174(0x80, 8): level +8 per VBlank,
//!   FUN_8007D1C0; drawn by FUN_8002D1E8 as a B-F full-screen quad of
//!   (0x80 - level) * 2);
//! - "PRESS START" (FUN_800474E8(0x800B0284, 0x100, 0xD2, 0, 0, 4, 0x60))
//!   once the frame counter 0x800B0D90 passes 599 VBlanks, shown while
//!   counter % 60 < 45, and only when the fade is done (0x800B0F88);
//! - START (FUN_80073138 @0x80073688: pressed, fade done) confirms with
//!   sound 0xF (FUN_8005EECC(0xF, 0x1800, 0x60, 0x60));
//! - music: course table row 30 +3 = track 0x18 = TUNES/MUSIC4.XA channel 0
//!   (no intro clip), looping;
//! - after 1800 VBlanks without input the game fades out to the attract
//!   demo race (course 17 in the oracle run): not ported, the port's title
//!   stays.
//! Not drawn yet (docs/spec/frontend.md "open"): the six AI karts the game
//! drives around the disc during the title (full race karts on the frontend
//! course: needs mr-sim to run course 0x1E in mode 0x10).
//!
//! The port: a Bevy state `Screen` (Title -> Race). While on the title the
//! race's cameras are switched off and virtual time is paused (the race's
//! fixed-schedule simulation waits at race load); START fades out and
//! enters the race screen, where track P's course pick-list (courses.rs)
//! opens first when no `--course` was given (it runs only while the race
//! cameras are on). The title has its own VRAM and
//! cameras; its semi-transparent polygons are drawn with Bevy blending
//! (the effects pass is bound to the race's VRAM; render-ledger.md).

use bevy::camera::visibility::RenderLayers;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;
use mr_assets::dat::{Dat, SinTable};
use mr_assets::hud::{self as ahud, HudTables, Sprite};
use mr_sim::camera::{rot_matrix, CameraTables};

use crate::psxmesh::{self, PsxPoly};
use crate::vramtex::VramTextures;

/// Course index of the frontend scene (0x800B0FAC = 0x1E, `frontend\options`).
pub const TITLE_COURSE: u32 = 0x1E;
/// Render layers of the title's 3D scene and its 2D overlay.
pub const TITLE_LAYER: usize = 6;
pub const TITLE_TEXT_LAYER: usize = 7;
/// DAT point chunks of the orbit (DAT_800D1C48[id]).
pub const EYE_CHUNK: i16 = 0x0B;
pub const LOOK_CHUNK: i16 = 0x0A;
/// Orbit speed per VBlank on the frontend course: ((0x40000 - 0) >> 14) << 2
/// (caseD_1 @0x80070B64, path length 0 -> clamped to 0x20000).
pub const ORBIT_STEP: i32 = 64;
/// Look-at parameter offset DAT_800B0A88 = 0x40000 >> 8.
pub const LOOK_AHEAD: i32 = 0x400;
/// "PRESS START" appears after this many VBlanks (FUN_80051698 @0x8005A3A8:
/// 599 < 0x800B0D90).
pub const PRESS_START_AFTER: i32 = 600;
/// Blink: shown while the counter % 60 < 45.
pub const BLINK_PERIOD: i32 = 60;
pub const BLINK_ON: i32 = 45;
/// Fade speed (FUN_8007D174's second argument) and full level (2 * 0x80).
pub const FADE_SPEED: i32 = 8;
pub const FADE_FULL: i32 = 0x100;
/// Course table row 30 +3: the title's music track (MUSIC4.XA channel 0).
pub const TITLE_TRACK: u16 = 0x18;
/// The confirm sound (FUN_80073138 -> FUN_8005EECC(0xF, 0x1800, 0x60, 0x60)).
pub const CONFIRM_SFX: (i32, i32, i32, i32) = (0xF, 0x1800, 0x60, 0x60);

/// Which screen the app shows.
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

// ---------------------------------------------------------------- camera

/// A DAT point loop (chunk body), in DAT units.
#[derive(Clone, Debug, Default)]
pub struct PathLoop(pub Vec<[i32; 3]>);

impl PathLoop {
    pub fn from_dat(dat: &Dat, id: i16) -> Option<PathLoop> {
        dat.chunks.iter().find(|c| c.id == id).and_then(|c| match &c.body {
            mr_assets::dat::ChunkBody::Points(p) if !p.is_empty() => Some(PathLoop(p.clone())),
            _ => None,
        })
    }

    /// FUN_80081BF4(path, 0, 0, t): the closed loop over all points, linear
    /// between neighbours, in world units (points * 32).
    pub fn at(&self, t: i32) -> [i32; 3] {
        let n = self.0.len() as i32;
        let u = (((t & 0x7FFF) * n * 0x40) as i64 >> 15) as i32;
        let i = u >> 6;
        let f = u & 0x3F;
        let j = if i + 1 == n { 0 } else { i + 1 };
        let (p, q) = (self.0[i as usize], self.0[j as usize]);
        std::array::from_fn(|k| p[k] * 32 + (((q[k] - p[k]) * f) >> 1))
    }
}

/// The title camera (the mode-1 fields of the camera record 0x800C4F40).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OrbitCam {
    /// +0x4C orbit parameter (& 0x7FFF).
    pub t: i32,
    /// +0x28 eye, world units.
    pub pos: [i32; 3],
    /// +0x5C follow weights.
    pub weight: [i16; 3],
    /// +0x50 / +0x52 / +0x54.
    pub pitch: i16,
    pub yaw: i16,
    pub roll: i16,
    /// +0x00 matrix (rows right, down, forward; 4.12).
    pub matrix: [[i16; 3]; 3],
    pub started: bool,
}

fn div_pow2(v: i32, n: u32) -> i32 {
    if v < 0 { v.wrapping_add((1 << n) - 1) >> n } else { v >> n }
}

fn wrap_delta(d: i32) -> i32 {
    if d > 2048 { d - 4096 } else if d < -2048 { d + 4096 } else { d }
}

fn approach32(w: i16, target: i16) -> i16 {
    if w > target {
        (w - 32).max(target)
    } else if w < target {
        (w + 32).min(target)
    } else {
        w
    }
}

/// Follow weight of the orbit (per axis, /1024 per VBlank) and the angle
/// divisor (the turn per VBlank = error / ANGLE_DIV). Weight 0x80 fitted to
/// the oracle's camera record (analysis/s6/title/cap1: 895 ticks, eye exact
/// on every axis; the race tail's default is 0x20).
pub const WEIGHT: i16 = 0x80;
pub const ANGLE_DIV: i32 = 8;

impl OrbitCam {
    /// One camera tick of `dt` VBlanks. `t0` is the rand() drawn on the
    /// first tick (the mode change).
    pub fn tick(&mut self, eye: &PathLoop, look: &PathLoop, tb: &CameraTables, dt: i32, t0: i32) {
        let changed = !self.started;
        if changed {
            self.started = true;
            self.t = t0;
        }
        self.t = (self.t - ORBIT_STEP * dt) & 0x7FFF;
        let goal = eye.at(self.t);
        let target = look.at(self.t + LOOK_AHEAD);
        let reps = dt.max(1);
        if changed {
            self.pos = goal;
            self.weight = [WEIGHT; 3];
        } else {
            for _ in 0..reps {
                for i in 0..3 {
                    self.weight[i] = approach32(self.weight[i], WEIGHT);
                    let w = self.weight[i] as i32;
                    self.pos[i] = self.pos[i].wrapping_add(div_pow2(goal[i].wrapping_sub(self.pos[i]).wrapping_mul(w), 10));
                }
            }
        }
        let dx = target[0].wrapping_sub(self.pos[0]) >> 7;
        let dz = target[2].wrapping_sub(self.pos[2]) >> 7;
        let dy = target[1].wrapping_sub(self.pos[1]) >> 7;
        let h = tb.sqrt0(dx.wrapping_mul(dx).wrapping_add(dz.wrapping_mul(dz)));
        let mut pitch_t = tb.ratan2(dy.wrapping_abs(), h);
        if dy < 0 {
            pitch_t = -pitch_t;
        }
        let pitch_t = pitch_t & 0xFFF;
        let yaw_t = tb.ratan2(dx, dz) & 0xFFF;
        if changed {
            self.pitch = pitch_t as i16;
            self.yaw = yaw_t as i16;
            self.roll = 0;
        } else {
            for _ in 0..reps {
                let d = wrap_delta(pitch_t - self.pitch as i32);
                self.pitch = ((self.pitch as i32 + d / ANGLE_DIV) & 0xFFF) as i16;
                let d = wrap_delta(yaw_t - self.yaw as i32);
                self.yaw = ((self.yaw as i32 + d / ANGLE_DIV) & 0xFFF) as i16;
            }
        }
        self.matrix = rot_matrix(tb, self.pitch as i32, (-(self.yaw as i32)) & 0xFFF, self.roll as i32);
    }
}

// ---------------------------------------------------------------- fade / text

/// FUN_8007D1C0: the fade level (0x800AFED0) toward its target.
#[derive(Clone, Copy, Debug, Default)]
pub struct Fade {
    pub level: i32,
    pub target: i32,
    pub speed: i32,
}

impl Fade {
    /// Returns 0x800B0F88: true once the level sits at its target.
    pub fn tick(&mut self, dt: i32) -> bool {
        if self.level < self.target {
            self.level = (self.level + self.speed * dt).min(self.target);
            false
        } else if self.level > self.target {
            self.level = (self.level - self.speed * dt).max(self.target);
            false
        } else {
            self.speed = 0;
            true
        }
    }
    /// FUN_8002D1E8: the B-F quad's colour (0 = none).
    pub fn subtract(&self) -> u8 {
        let b = (self.level - (self.level >> 31)) >> 1;
        ((0x80 - b) * 2).clamp(0, 0xFF) as u8
    }
}

/// The title's 2D layer for one VBlank counter value: "PRESS START" when due.
pub fn text_sprites(t: &HudTables, counter: i32, fade_done: bool) -> Vec<Sprite> {
    if counter >= PRESS_START_AFTER && counter % BLINK_PERIOD < BLINK_ON && fade_done {
        t.text(b"PRESS START", 0x100, 0xD2, 0, ahud::Size::from_arg(0), 4, 0x60)
    } else {
        Vec::new()
    }
}

// ---------------------------------------------------------------- plugin

#[derive(Component)]
pub struct TitleEntity;
/// One DAT object of the frontend scene (index into `Dat::objects`).
#[derive(Component)]
pub struct TitleObject(pub usize);
#[derive(Component)]
pub struct TitleCam;
#[derive(Component)]
pub struct TitleTextCam;
#[derive(Component)]
struct FadeQuad;
#[derive(Component)]
struct TextQuad;

#[derive(Resource)]
pub struct TitleState {
    eye: PathLoop,
    look: PathLoop,
    dat: Dat,
    tables: CameraTables,
    hud: HudTables,
    slots: [(u16, u16); 32],
    vram: VramTextures,
    cam: OrbitCam,
    prev: OrbitCam,
    t0: i32,
    /// 0x800B0D90: VBlanks since the title started.
    pub counter: i32,
    fade: Fade,
    fade_done: bool,
    /// Real time not yet consumed by ticks (seconds).
    acc: f64,
    leaving: bool,
    /// `--title-shot PATH --title-shot-at VB`: frozen at that counter value.
    frozen: bool,
    shot_wait: u32,
    prev_buttons: u16,
    text_key: Option<Vec<Sprite>>,
    sfx: Option<mr_audio::driver::Driver>,
    sfx_left: u32,
}

pub struct TitlePlugin;

impl Plugin for TitlePlugin {
    fn build(&self, app: &mut App) {
        app.insert_state(first_screen())
            .add_systems(OnEnter(Screen::Title), enter)
            .add_systems(OnExit(Screen::Title), leave)
            .add_systems(Update, after_shot.run_if(in_state(Screen::Race)))
            .add_systems(Update, (tick, place_camera.after(tick), select.after(tick), draw_text.after(tick), quiet_race, fit, shot.after(place_camera)).run_if(in_state(Screen::Title)));
    }
}

/// While the title shows, the race waits: no fixed ticks, its cameras off.
pub fn race_running(s: Option<Res<State<Screen>>>) -> bool {
    s.is_none_or(|s| *s.get() == Screen::Race)
}

fn seed() -> i32 {
    if let Some(v) = crate::settings::arg_value("--title-t0").and_then(|v| v.parse::<i32>().ok()) {
        return v & 0x7FFF;
    }
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
    (n >> 7) as i32 & 0x7FFF
}

#[allow(clippy::too_many_arguments)]
fn enter(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut time: ResMut<Time<Virtual>>,
    settings: Res<crate::settings::Settings>,
    bus: Option<Res<crate::audio::AudioBus>>,
) {
    time.pause();
    let mut disc = mr_disc::Disc::open_default().expect("open the disc (MRM_DISC or game.toml)");
    let exe = disc.read("SLUS_012.37").expect("read the executable");
    let (vram, _slots, dslots) = crate::scene::load_vram(&mut disc, &exe, TITLE_COURSE);
    let mut tex = VramTextures::new(vram);
    let path = mr_assets::raw::course_raw_path(&exe, TITLE_COURSE).expect("frontend RAW path");
    let dat_path = path.replace(".RAW", ".DAT").replace(".raw", ".dat");
    let dat = Dat::parse(&disc.read(&dat_path).expect("read OPTIONS.DAT")).expect("parse OPTIONS.DAT");
    let sin = SinTable::from_exe(&exe).expect("sin table");
    let tables = CameraTables::from_exe(&exe).expect("camera tables");
    let eye = PathLoop::from_dat(&dat, EYE_CHUNK).expect("OPTIONS.DAT chunk 0x0B (title camera eye loop)");
    let look = PathLoop::from_dat(&dat, LOOK_CHUNK).expect("OPTIONS.DAT chunk 0x0A (title camera look loop)");

    let world = |p: [i32; 3]| crate::to_bevy(p[0] * 32, p[1] * 32, p[2] * 32);
    let mut cache = psxmesh::MatCache::default();
    let mut semi_mats: std::collections::HashMap<(Option<(u16, u16)>, u8), Handle<StandardMaterial>> = Default::default();
    let layer = RenderLayers::layer(TITLE_LAYER);
    for (oi, o) in dat.objects.iter().enumerate().take(dat.layer0_count) {
        let polys = crate::scene::object_polys(&dat, o, &sin, &dslots);
        if polys.is_empty() {
            continue;
        }
        let parts = psxmesh::build_cached(&polys, &mut tex, &mut images, &mut meshes, &mut mats, &mut cache, world);
        let root = commands.spawn((Transform::default(), Visibility::default(), TitleEntity, TitleObject(oi))).id();
        // Semi-transparent polygons: Bevy blending (ABR 0 = half, 1/3 =
        // additive (3 at a quarter), 2 = subtractive approximated by a
        // multiply); the GPU's exact equations live in the race's effects
        // pass (render-ledger.md "Title").
        let mut semi: Vec<PsxPoly> = polys.iter().filter(|p| p.semi).cloned().collect();
        for p in semi.iter_mut() {
            p.semi = false;
        }
        for abr in 0..4u8 {
            let group: Vec<PsxPoly> = semi.iter().filter(|p| p.abr & 3 == abr).cloned().collect();
            if group.is_empty() {
                continue;
            }
            for (key, mut mesh) in psxmesh::meshes_by_key(&group, world) {
                if abr == 3 {
                    if let Some(bevy::mesh::VertexAttributeValues::Float32x4(c)) = mesh.attribute_mut(Mesh::ATTRIBUTE_COLOR) {
                        for v in c.iter_mut() {
                            v[0] *= 0.25;
                            v[1] *= 0.25;
                            v[2] *= 0.25;
                        }
                    }
                }
                let mat = semi_mats.entry((key.0, abr)).or_insert_with(|| {
                    mats.add(StandardMaterial {
                        base_color: if abr == 0 { Color::srgba(1.0, 1.0, 1.0, 0.5) } else { Color::WHITE },
                        base_color_texture: key.0.map(|(tp, cl)| tex.get(&mut images, tp, cl)),
                        unlit: true,
                        cull_mode: None,
                        double_sided: true,
                        alpha_mode: match abr {
                            0 => AlphaMode::Blend,
                            2 => AlphaMode::Multiply,
                            _ => AlphaMode::Add,
                        },
                        ..default()
                    })
                });
                let m = meshes.add(mesh);
                commands.entity(root).with_children(|c| {
                    c.spawn((Mesh3d(m), MeshMaterial3d(mat.clone()), layer.clone()));
                });
            }
        }
        commands.entity(root).with_children(|c| {
            for (mesh, mat) in parts {
                c.spawn((Mesh3d(mesh), MeshMaterial3d(mat), layer.clone()));
            }
        });
    }

    // Cameras: the scene (same projection as the race camera), then the 2D
    // layer (512 x 240 game pixels) with the fade quad on top.
    commands.spawn((
        Camera3d::default(),
        Camera { order: 30, clear_color: ClearColorConfig::Custom(crate::psxfx::psx_fill([0, 0, 0])), ..default() },
        Projection::Perspective(PerspectiveProjection { fov: 2.0 * (120.0f32 / 160.0).atan(), near: 0.05, far: 2000.0, ..default() }),
        Tonemapping::None,
        settings.msaa(),
        Transform::default(),
        layer.clone(),
        TitleCam,
        TitleEntity,
    ));
    commands.spawn((
        Camera3d::default(),
        Camera { order: 31, clear_color: ClearColorConfig::None, ..default() },
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: bevy::camera::ScalingMode::Fixed { width: crate::hud::SCREEN_W, height: crate::hud::SCREEN_H },
            near: -100.0,
            far: 100.0,
            ..OrthographicProjection::default_3d()
        }),
        Tonemapping::None,
        settings.msaa(),
        Transform::from_xyz(0.0, 0.0, 50.0),
        RenderLayers::layer(TITLE_TEXT_LAYER),
        TitleTextCam,
        TitleEntity,
    ));
    let quad = meshes.add(Mesh::from(Rectangle::new(4096.0, 4096.0)));
    let fade_mat = mats.add(StandardMaterial { base_color: Color::BLACK, unlit: true, alpha_mode: AlphaMode::Multiply, ..default() });
    commands.spawn((Mesh3d(quad), MeshMaterial3d(fade_mat), Transform::from_xyz(0.0, 0.0, 10.0), RenderLayers::layer(TITLE_TEXT_LAYER), FadeQuad, TitleEntity));

    // Font: the frontend RAW's slot table (font page 0xF).
    let file = disc.read(&path).expect("read OPTIONS.RAW");
    let env = mr_assets::raw::LoadEnv::from_exe(&exe, TITLE_COURSE).expect("frontend env");
    let dec = mr_assets::raw::decode(&file, &env).expect("decode OPTIONS.RAW");
    let slots = ahud::slot_attrs(&dec.slots);
    let hud = HudTables::from_exe(&exe).expect("HUD tables");

    // Music: the frontend row's track, looping (no intro clip).
    if let Some(b) = bus.as_ref() {
        let track = mr_audio::xa::exe_course_music(&exe, TITLE_COURSE as u8).unwrap_or(TITLE_TRACK);
        b.music(crate::audio::MusicCmd::Play { track, looping: true, level: crate::audio::MUSIC_LEVEL });
    }
    let sfx = (|| {
        let vh = disc.read("SFX/SFX_6.VH").ok()?;
        let vb = disc.read("SFX/SFX_6.VB").ok()?;
        let c = mr_audio::driver::Consts::from_exe(&exe)?;
        let mut d = mr_audio::driver::Driver::new(mr_audio::vab::Vab::parse(&vh, vb).ok()?, c);
        d.spu.apply_main = false;
        Some(d)
    })();

    let t0 = seed();
    commands.insert_resource(TitleState {
        eye,
        look,
        dat,
        tables,
        hud,
        slots,
        vram: tex,
        cam: OrbitCam::default(),
        prev: OrbitCam::default(),
        t0,
        counter: -2,
        fade: Fade { level: 0, target: FADE_FULL, speed: FADE_SPEED },
        fade_done: false,
        acc: 0.0,
        leaving: false,
        frozen: false,
        shot_wait: 0,
        prev_buttons: 0,
        text_key: None,
        sfx,
        sfx_left: 0,
    });
}

fn leave(mut commands: Commands, q: Query<Entity, With<TitleEntity>>, mut cams: Query<&mut Camera, Without<TitleEntity>>, mut time: ResMut<Time<Virtual>>, mut latch: ResMut<crate::pacing::PadLatch>) {
    for e in q.iter() {
        commands.entity(e).despawn();
    }
    for mut c in cams.iter_mut() {
        c.is_active = true;
    }
    commands.remove_resource::<TitleState>();
    // the START that left the title is not the race's first press
    latch.take_for_tick();
    time.unpause();
}

/// Keep every race camera off while the title shows (the race spawns some
/// later, e.g. the far pass); the pillarbox bars stay.
#[allow(clippy::type_complexity)]
fn quiet_race(mut cams: Query<&mut Camera, (Without<TitleEntity>, Without<crate::BarCamera>)>) {
    for mut c in cams.iter_mut() {
        if c.is_active {
            c.is_active = false;
        }
    }
}

/// The title's logic at the game's rate: one tick per 2 VBlanks (dt = 2),
/// VBlanks at 60 Hz of real time.
fn tick(
    mut st: ResMut<TitleState>,
    real: Res<Time<Real>>,
    latch: Res<crate::pacing::PadLatch>,
    mut next: ResMut<NextState<Screen>>,
    bus: Option<Res<crate::audio::AudioBus>>,
) {
    const DT: i32 = 2;
    let st = &mut *st;
    st.acc += real.delta_secs_f64().min(0.25);
    let step = DT as f64 / 60.0;
    let mut first = !st.cam.started;
    let shot_at = crate::settings::arg_value("--title-shot-at").and_then(|v| v.parse::<i32>().ok());
    while first || st.acc >= step {
        if st.frozen {
            st.acc = 0.0;
            break;
        }
        if !first {
            st.acc -= step;
        }
        first = false;
        st.prev = st.cam;
        // 0x800B0D90 = VBlanks since the title started (0 on its first tick)
        st.counter += DT;
        st.cam.tick(&st.eye, &st.look, &st.tables, DT, st.t0);
        st.fade_done = st.fade.tick(DT);
        if std::env::var("MR_TITLE_DEBUG").is_ok() {
            eprintln!("title tick counter {} t {} pos {:?} fade {}", st.counter, st.cam.t, st.cam.pos, st.fade.level);
        }
        if shot_at.is_some_and(|a| st.counter >= a) {
            st.frozen = true;
            st.prev = st.cam;
            st.acc = 0.0;
        }
        // START: pressed (not held), fade finished (FUN_80073138).
        // `--title-press-start VB`: START held from that counter value on
        // (tests of the hand-over without a pad).
        let auto = crate::settings::arg_value("--title-press-start").and_then(|v| v.parse::<i32>().ok()).is_some_and(|v| st.counter >= v && st.counter < v + 8);
        let b = latch.now | if auto { crate::input::START } else { 0 };
        if !st.leaving && st.fade_done && b & crate::input::START != 0 && st.prev_buttons & crate::input::START == 0 {
            st.leaving = true;
            let (id, pitch, l, r) = CONFIRM_SFX;
            if let Some(d) = st.sfx.as_mut() {
                d.play(id, pitch, l, r);
                st.sfx_left = 60;
            }
            // out: the fade to black (FUN_8007D174(0, 8)), then the race
            st.fade = Fade { level: st.fade.level, target: 0, speed: FADE_SPEED };
            st.fade_done = false;
        }
        st.prev_buttons = b;
        // (and START released: the course pick-list takes edges of its own)
        if st.leaving && st.fade_done && b & crate::input::START == 0 {
            if let Some(b) = bus.as_ref() {
                b.music(crate::audio::MusicCmd::Stop);
            }
            info!("title: START -> race screen (counter {})", st.counter);
            next.set(Screen::Race);
            break;
        }
        // the confirm sound: the driver's VBlank work for this tick
        if st.sfx_left > 0 {
            if let (Some(d), Some(b)) = (st.sfx.as_mut(), bus.as_ref()) {
                let mut buf = Vec::new();
                for _ in 0..DT {
                    d.vblank_render(&mut buf);
                }
                let out: Vec<[i16; 2]> = buf.chunks_exact(2).map(|s| [s[0], s[1]]).collect();
                b.push_sfx(&out);
            }
            st.sfx_left = st.sfx_left.saturating_sub(DT as u32);
        }
    }
}

fn cam_rot(m: [[i16; 3]; 3]) -> Quat {
    let f = |r: usize, c: usize| m[r][c] as f32 / 4096.0;
    Quat::from_mat3(&Mat3::from_cols(
        Vec3::new(f(0, 0), -f(0, 1), -f(0, 2)),
        Vec3::new(-f(1, 0), f(1, 1), f(1, 2)),
        Vec3::new(-f(2, 0), f(2, 1), f(2, 2)),
    ))
    .normalize()
}

/// Display: between the last two ticks (the race camera's interpolation).
fn place_camera(st: Res<TitleState>, mut q: Query<&mut Transform, With<TitleCam>>, mut fade: Query<&MeshMaterial3d<StandardMaterial>, With<FadeQuad>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let a = (st.acc / (2.0 / 60.0)).clamp(0.0, 1.0) as f32;
    let (p0, p1) = (st.prev, st.cam);
    let p0 = if p0.started { p0 } else { p1 };
    for mut tr in q.iter_mut() {
        tr.translation = crate::to_bevy(p0.pos[0], p0.pos[1], p0.pos[2]).lerp(crate::to_bevy(p1.pos[0], p1.pos[1], p1.pos[2]), a);
        tr.rotation = cam_rot(p0.matrix).slerp(cam_rot(p1.matrix), a);
    }
    // B - F with F = (0x80 - level/2) * 2 on the frame: approximated by a
    // multiply with 1 - F/255 (gamma space).
    let k = 1.0 - st.fade.subtract() as f32 / 255.0;
    for h in fade.iter_mut() {
        if let Some(mut m) = mats.get_mut(&h.0) {
            let want = Color::srgb(k, k, k);
            if m.base_color != want {
                m.base_color = want;
            }
        }
    }
}

/// The game's draw distances on the frontend course (RAM during the title:
/// DAT_800B0CD0 = 5000 (FUN_80029F40: DAT_800B0EE4 < 0 -> 5000),
/// DAT_800B0DE8 = 4616, DAT_800B0B00 = 0x4AF - DAT_800A8840 (60)).
pub const TITLE_DRAW: mr_assets::dat::DrawParams = mr_assets::dat::DrawParams { cd0: 5000, de8: 4616, b00: 1139 };

/// FUN_80010000's per-frame object selection (DAT.md; no visibility zones on
/// the frontend course: every mask bit set) at the shown camera.
fn select(st: Res<TitleState>, mut q: Query<(&TitleObject, &mut Visibility)>) {
    let c = st.cam;
    // eye / 32 toward zero (the scratch copy 0x1F800384)
    let eye = c.pos.map(|v| v / 32);
    let drawn: std::collections::HashSet<usize> = st.dat.drawn(eye, c.matrix, &[0xFF; 256], TITLE_DRAW).iter().map(|d| d.object).collect();
    for (o, mut v) in q.iter_mut() {
        let want = if drawn.contains(&o.0) { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
    }
}

/// "PRESS START" as opaque raw sprites from the frontend VRAM's font page.
fn draw_text(
    mut commands: Commands,
    mut st: ResMut<TitleState>,
    old: Query<Entity, With<TextQuad>>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let st = &mut *st;
    let sprites = text_sprites(&st.hud, st.counter, st.fade_done && !st.leaving);
    if st.text_key.as_ref() == Some(&sprites) {
        return;
    }
    for e in old.iter() {
        commands.entity(e).despawn();
    }
    let sp = |x: f32, y: f32| [x - crate::hud::SCREEN_W / 2.0, crate::hud::SCREEN_H / 2.0 - y, 0.0];
    for s in &sprites {
        let (tp, cl) = s.attrs(&st.slots);
        let img = st.vram.get_nearest(&mut images, tp & !0x60, cl);
        let (x0, y0, x1, y1) = (s.x as f32, s.y as f32, (s.x + s.w) as f32, (s.y + s.h) as f32);
        let (u0, v0) = (s.u as f32 / 256.0, s.v as f32 / 256.0);
        let (u1, v1) = (u0 + s.uw as f32 / 256.0, v0 + s.vh as f32 / 256.0);
        let pos = vec![sp(x0, y0), sp(x1, y0), sp(x0, y1), sp(x1, y0), sp(x1, y1), sp(x0, y1)];
        let uv = vec![[u0, v0], [u1, v0], [u0, v1], [u1, v0], [u1, v1], [u0, v1]];
        let mut m = Mesh::new(bevy::mesh::PrimitiveTopology::TriangleList, bevy::asset::RenderAssetUsages::default());
        m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
        m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; 6]);
        m.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
        let mat = mats.add(StandardMaterial { base_color: Color::WHITE, base_color_texture: Some(img), unlit: true, cull_mode: None, alpha_mode: AlphaMode::Mask(0.5), ..default() });
        commands.spawn((Mesh3d(meshes.add(m)), MeshMaterial3d(mat), Transform::IDENTITY, RenderLayers::layer(TITLE_TEXT_LAYER), TextQuad, TitleEntity));
    }
    st.text_key = Some(sprites);
}

/// `--title-shot PATH [--title-shot-at VB]`: a window capture of the frozen
/// title at that counter value (side-by-sides with the oracle), then exit.
fn shot(mut st: ResMut<TitleState>, mut c: Commands, mut exit: MessageWriter<AppExit>) {
    let Some(path) = crate::settings::arg_value("--title-shot") else { return };
    if !st.frozen {
        return;
    }
    st.shot_wait += 1;
    if std::env::var("MR_TITLE_DEBUG").is_ok() {
        eprintln!("title shot wait {}", st.shot_wait);
    }
    if st.shot_wait == 8 {
        c.spawn(bevy::render::view::screenshot::Screenshot::primary_window())
            .observe(move |ev: On<bevy::render::view::screenshot::ScreenshotCaptured>| crate::write_png(&ev.image, &path));
    }
    if st.shot_wait == 40 {
        exit.write(AppExit::Success);
    }
}

/// `--title-after-shot PATH`: a window capture 90 frames after the title
/// handed over to the race screen (checks of the hand-over), then exit.
fn after_shot(mut n: Local<u32>, mut c: Commands, mut exit: MessageWriter<AppExit>) {
    let Some(path) = crate::settings::arg_value("--title-after-shot") else { return };
    if first_screen() != Screen::Title {
        return;
    }
    *n += 1;
    if *n == 90 {
        c.spawn(bevy::render::view::screenshot::Screenshot::primary_window())
            .observe(move |ev: On<bevy::render::view::screenshot::ScreenshotCaptured>| crate::write_png(&ev.image, &path));
    }
    if *n == 120 {
        exit.write(AppExit::Success);
    }
}

/// The title's cameras in the race's view rectangle (4:3 pillarbox or the
/// whole window, settings `aspect`).
fn fit(settings: Res<crate::settings::Settings>, win: Query<&Window, With<bevy::window::PrimaryWindow>>, mut cams: Query<&mut Camera, With<TitleEntity>>) {
    let Ok(w) = win.single() else { return };
    let size = w.physical_size();
    if size.x == 0 || size.y == 0 {
        return;
    }
    let (pos, sz) = crate::view_rect(size, settings.aspect());
    let want = if pos == UVec2::ZERO && sz == size { None } else { Some(bevy::camera::Viewport { physical_position: pos, physical_size: sz, ..default() }) };
    for mut c in cams.iter_mut() {
        let same = match (&c.viewport, &want) {
            (None, None) => true,
            (Some(a), Some(b)) => a.physical_position == b.physical_position && a.physical_size == b.physical_size,
            _ => false,
        };
        if !same {
            c.viewport = want.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fade_in_takes_32_vblanks() {
        let mut f = Fade { level: 0, target: FADE_FULL, speed: FADE_SPEED };
        let mut vb = 0;
        while !f.tick(2) {
            vb += 2;
        }
        assert_eq!(vb, 32);
        assert_eq!(f.subtract(), 0);
    }

    /// The orbit against the oracle's camera record (local capture
    /// analysis/s6/title/cap1, tools: docs/spec/frontend.md; skipped when
    /// absent): the oracle's own dt per tick, eye / angles / matrix exact.
    #[test]
    fn orbit_matches_oracle_capture() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../analysis/s6/title/cap1");
        let (Ok(meta), Ok(frames)) = (std::fs::read_to_string(root.join("meta.json")), std::fs::read(root.join("frames.bin"))) else {
            eprintln!("no title capture: skipped");
            return;
        };
        let Ok(mut disc) = mr_disc::Disc::open_default() else { return };
        let exe = disc.read("SLUS_012.37").unwrap();
        let dat = Dat::parse(&disc.read("FRONTEND/OPTIONS.DAT").unwrap()).unwrap();
        let tb = CameraTables::from_exe(&exe).unwrap();
        let (eye, look) = (PathLoop::from_dat(&dat, EYE_CHUNK).unwrap(), PathLoop::from_dat(&dat, LOOK_CHUNK).unwrap());
        // regions: 6 x 0x40 kart heads, camera 0x70, then 8 words (dt = 6th)
        const ROW: usize = 6 * 0x40 + 0x70 + 8 * 4;
        let first_vb: usize = meta.split("\"first_vb\": ").nth(1).unwrap().split(|c: char| !c.is_ascii_digit()).next().unwrap().parse().unwrap();
        let row = |vb: usize| &frames[(vb - first_vb) * ROW..(vb - first_vb + 1) * ROW];
        let cam = |vb: usize| row(vb)[0x180..0x1F0].to_vec();
        let i16at = |b: &[u8], o: usize| i16::from_le_bytes([b[o], b[o + 1]]);
        let i32at = |b: &[u8], o: usize| i32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
        // the title's first tick at VB 408 (t = 5494 after a 2-VBlank step)
        let mut c = OrbitCam::default();
        c.tick(&eye, &look, &tb, 2, (5494 + 128) & 0x7FFF);
        let mut prev_t = c.t;
        let mut n = 0;
        for vb in 408..2200 {
            let r = cam(vb);
            let t = i32at(&r, 0x4C);
            if vb > 408 && t != prev_t {
                let dt = i32at(row(vb), 0x1F0 + 5 * 4);
                c.tick(&eye, &look, &tb, dt, 0);
                prev_t = t;
                n += 1;
            }
            assert_eq!(c.t, t, "t at VB {vb}");
            assert_eq!(c.pos, [i32at(&r, 0x28), i32at(&r, 0x2C), i32at(&r, 0x30)], "eye at VB {vb}");
            assert_eq!([c.pitch, c.yaw, c.roll], [i16at(&r, 0x50), i16at(&r, 0x52), i16at(&r, 0x54)], "angles at VB {vb}");
            for k in 0..9 {
                assert_eq!(c.matrix[k / 3][k % 3], i16at(&r, 2 * k), "matrix at VB {vb}");
            }
        }
        assert!(n > 800);
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

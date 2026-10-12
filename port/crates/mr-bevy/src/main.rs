//! Muppet RaceMania (Rust + Bevy), milestone 1: one kart on DOCK.
//!
//!   mr-bevy [--replay TRACE] [--ghost TRACE] [--disc CUE]
//!
//! The simulation runs on Bevy's fixed schedule at the game's logic rate
//! (30 Hz: one tick per 2 VBlanks), decoupled from rendering. Bevy only
//! presents: the kart state is integers from mr-sim (or a recorded trace) and
//! is converted to floats here, never the other way.
//!
//! Coordinates: the game's world is x right, y down, z forward (GTE). Bevy is
//! y up, right-handed, -z forward. Mapping (x, y, z) -> (x, -y, -z) * SCALE is
//! a rotation (two axes flipped), so handedness and winding are preserved.

mod ghost;
mod input;
pub mod kartmodel;
mod psxmesh;
mod scene;
mod vramtex;

use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use mr_trace::Trace;

/// Game units per Bevy unit.
pub const UNITS: f32 = 4096.0;

pub fn to_bevy(x: i32, y: i32, z: i32) -> Vec3 {
    Vec3::new(x as f32, -(y as f32), -(z as f32)) / UNITS
}

/// Game heading (4096 = 360 deg, forward = (sin h, cos h) in game x/z) as a
/// Bevy rotation: forward in Bevy is (sin h, 0, -cos h).
pub fn heading_quat(h: i32) -> Quat {
    Quat::from_rotation_y(-(h as f32) * std::f32::consts::TAU / 4096.0)
}

/// The player's kart pose this tick, in game integers.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pose {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub heading: i32,
    /// Body rotation matrix (kart +0x18C), 4.12, row-major: world = M * model.
    pub m: [i32; 9],
    /// The game's camera this tick: eye (kart units) and 4.12 view matrix
    /// (rows right, down, forward), when known (oracle replay or mr-sim).
    pub cam: Option<([i32; 3], [i32; 9])>,
}

impl Pose {
    /// The game's body matrix as a Bevy rotation: R = F M F, F = diag(1,-1,-1).
    pub fn rotation(&self) -> Quat {
        if self.m == [0; 9] {
            return heading_quat(self.heading);
        }
        let f = [1.0f32, -1.0, -1.0];
        let e = |r: usize, c: usize| f[r] * f[c] * self.m[r * 3 + c] as f32 / 4096.0;
        let mat = Mat3::from_cols(
            Vec3::new(e(0, 0), e(1, 0), e(2, 0)),
            Vec3::new(e(0, 1), e(1, 1), e(2, 1)),
            Vec3::new(e(0, 2), e(1, 2), e(2, 2)),
        );
        Quat::from_mat3(&mat).normalize()
    }
}

impl Pose {
    pub fn from_row(t: &Trace, row: &mr_trace::Row) -> Pose {
        let c = |n: &str| t.col(n).map(|i| row.v[i] as i32).unwrap_or(0);
        let mut m = [0; 9];
        for (i, v) in m.iter_mut().enumerate() {
            *v = c(&format!("m{i}"));
        }
        let cam = t.col("cam_x").map(|_| {
            let mut cm = [0; 9];
            for (i, v) in cm.iter_mut().enumerate() {
                *v = c(&format!("c{i}"));
            }
            ([c("cam_x"), c("cam_y"), c("cam_z")], cm)
        }).filter(|(_, cm)| *cm != [0; 9]);
        Pose { x: c("x"), y: c("y"), z: c("z"), heading: c("heading"), m, cam }
    }
}

/// Where the player kart's state comes from.
pub enum Driver {
    /// Play a recorded trace back (the oracle's own run) - the hybrid view.
    /// `karts`: the trace's sidecar (TRACE.kart.bin, 0x504 bytes per row),
    /// the full kart record per tick, when present.
    Replay { trace: Trace, at: usize, karts: Option<Vec<u8>> },
    /// mr-sim drives the kart from live input (wired in when mr-sim lands).
    Sim,
}

#[derive(Resource)]
pub struct Player {
    pub driver: Driver,
    pub pose: Pose,
    pub prev: Pose,
    pub tick: i64,
    pub buttons: u16,
    /// The player's kart record this tick and last tick (0x504 bytes): from
    /// the replay sidecar, else synthesized from `pose` (kartmodel::synth_record).
    pub kart: Option<Vec<u8>>,
    pub prev_kart: Option<Vec<u8>>,
}

/// One model part of the player's vehicle (model 0) or driver (model 1),
/// placed every frame by `place_parts`.
#[derive(Component)]
pub struct KartPart {
    pub model: usize,
    pub part: usize,
}

/// A polygon of the driver's neck (world-space vertices rewritten per frame).
#[derive(Component)]
pub struct NeckPoly;

#[derive(Resource)]
pub struct KartRig {
    pub model: kartmodel::KartModel,
    /// Per neck part, per polygon: its mesh (vertices in GPU order).
    pub neck_meshes: Vec<Vec<Handle<Mesh>>>,
}

#[derive(Component)]
pub struct PlayerKart;

#[derive(Component)]
pub struct ChaseCamera;

#[derive(Component)]
pub struct Sky;

/// A DAT scenery object (index into Dat::objects).
#[derive(Component)]
pub struct TrackObject(pub usize);

/// The game's per-frame scenery selection (DAT.md "Per-frame object
/// selection"): visibility zone/node from the camera eye, then the layer-0
/// near/ground cull and the layer-1 far stand-ins, exactly as FUN_80010000 /
/// FUN_8001031C choose them.
#[derive(Resource)]
pub struct TrackVis {
    pub dat: mr_assets::dat::Dat,
    pub zone: usize,
}

fn select_objects(
    p: Res<Player>,
    mut vis: ResMut<TrackVis>,
    mut q: Query<(&TrackObject, &mut Visibility)>,
    mut cams: Query<(Entity, &mut Camera, &Projection), With<ChaseCamera>>,
    mut commands: Commands,
    mut far_done: Local<bool>,
) {
    // Painter's order of the game's ordering table: far pass (sky + DAT
    // layer 1) first, then everything else over it (scene::setup_far_pass).
    if !*far_done {
        if let Ok((e, mut cam, proj)) = cams.single_mut() {
            scene::setup_far_pass(&mut commands, e, &mut cam, proj);
            *far_done = true;
        }
    }
    let Some((eye_w, cm)) = p.pose.cam else { return };
    if !p.is_changed() {
        return;
    }
    // DAT units = world >> 5 toward zero (the game's scratch copy of eye / 32).
    let eye = [eye_w[0] / 32, eye_w[1] / 32, eye_w[2] / 32];
    let cam = [
        [cm[0] as i16, cm[1] as i16, cm[2] as i16],
        [cm[3] as i16, cm[4] as i16, cm[5] as i16],
        [cm[6] as i16, cm[7] as i16, cm[8] as i16],
    ];
    let prev = vis.zone;
    let zone = vis.dat.vis_zone(eye, prev);
    vis.zone = zone;
    let mask = match vis.dat.vis_node(zone, eye) {
        Some(node) => vis.dat.vis[zone].mask_at(node),
        None => vec![0xFF; 256],
    };
    let drawn: std::collections::HashSet<usize> = vis
        .dat
        .drawn(eye, cam, &mask, mr_assets::dat::DrawParams::DOCK_1P)
        .iter()
        .map(|d| d.object)
        .collect();
    if p.tick % 100 == 0 && std::env::args().any(|a| a == "--debug-vis") {
        let n = q.iter().count();
        eprintln!("tick {} eye {:?} zone {} node {:?} drawn {} of {} entities", p.tick, eye, zone, vis.dat.vis_node(zone, eye), drawn.len(), n);
    }
    for (o, mut v) in q.iter_mut() {
        let want = if drawn.contains(&o.0) { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
    }
}

/// The sky dome is centred on the eye (the game draws it with zero translation).
fn follow_sky(cam: Query<&Transform, With<ChaseCamera>>, mut sky: Query<&mut Transform, (With<Sky>, Without<ChaseCamera>)>) {
    let Ok(c) = cam.single() else { return };
    for mut s in sky.iter_mut() {
        s.translation = c.translation;
    }
}

pub struct Args {
    pub replay: Option<String>,
    pub ghost: Option<String>,
}

/// --shot PATH --shot-at TICK: save one window capture at that sim tick, then exit.
#[derive(Resource)]
struct ShotPlan {
    path: Option<String>,
    at: i64,
    state: u8,
}

fn parse_args() -> Args {
    let a: Vec<String> = std::env::args().collect();
    let opt = |k: &str| a.iter().position(|x| x == k).and_then(|i| a.get(i + 1)).cloned();
    if let Some(d) = opt("--disc") {
        std::env::set_var("MRM_DISC", d);
    }
    Args { replay: opt("--replay"), ghost: opt("--ghost") }
}

fn shot_plan() -> ShotPlan {
    let a: Vec<String> = std::env::args().collect();
    let opt = |k: &str| a.iter().position(|x| x == k).and_then(|i| a.get(i + 1)).cloned();
    ShotPlan { path: opt("--shot"), at: opt("--shot-at").and_then(|v| v.parse().ok()).unwrap_or(30), state: 0 }
}

fn main() {
    let args = parse_args();
    let driver = match &args.replay {
        Some(p) => {
            let side = p.replace(".trace.jsonl", ".kart.bin");
            let karts = std::fs::read(&side).ok().filter(|b| b.len() % kartmodel::KART_LEN == 0);
            if karts.is_none() {
                eprintln!("no kart sidecar {side}: parts placed from the trace pose only");
            }
            Driver::Replay { trace: Trace::read(p).expect("read --replay trace"), at: 0, karts }
        }
        None => Driver::Sim,
    };
    let ghost = args.ghost.as_ref().map(|p| Trace::read(p).expect("read --ghost trace"));
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Muppet RaceMania (Rust)".into(),
                resolution: (1280u32, 960u32).into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(Time::<Fixed>::from_hz(30.0))
        .insert_resource(ClearColor(Color::srgb(0.25, 0.2, 0.45)))
        .insert_resource(Player { driver, pose: Pose::default(), prev: Pose::default(), tick: 0, buttons: 0, kart: None, prev_kart: None })
        .insert_resource(ghost::Ghost::new(ghost))
        .add_systems(Startup, (scene::setup, setup_camera))
        .add_systems(FixedUpdate, (sim_tick, ghost::tick.after(sim_tick)))
        .insert_resource(shot_plan())
        .add_systems(Update, (place_player, place_parts, ghost::place, follow_camera, title, follow_sky.after(follow_camera), select_objects, shot.after(follow_sky).after(select_objects)))
        .run();
}

fn setup_camera(mut c: Commands) {
    // No tonemapping: PS1 colours go to the screen as the game wrote them.
    // The game's projection: H = 160 over a 240-line screen (SetGeomScreen
    // at 0x8004311C) = 2*atan(120/160) = 73.74 deg vertical [camera.md].
    c.spawn((
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            fov: 2.0 * (120.0f32 / 160.0).atan(),
            near: 0.05,
            far: 2000.0,
            ..default()
        }),
        Tonemapping::None,
        Transform::from_xyz(0.0, 3.0, 8.0),
        ChaseCamera,
    ));
    c.spawn((DirectionalLight { illuminance: 8000.0, ..default() },
             Transform::from_xyz(1.0, 3.0, 2.0).looking_at(Vec3::ZERO, Vec3::Y)));
    c.insert_resource(GlobalAmbientLight { brightness: 1500.0, ..default() });
}

fn sim_tick(mut p: ResMut<Player>, keys: Res<ButtonInput<KeyCode>>, pads: Query<&Gamepad>) {
    let buttons = input::sample(&keys, &pads);
    p.prev = p.pose;
    let tick = p.tick;
    let mut pose = p.pose;
    let mut kart = p.kart.clone();
    match &mut p.driver {
        Driver::Replay { trace, at, karts } => {
            if *at < trace.rows.len() {
                pose = Pose::from_row(trace, &trace.rows[*at]);
                kart = karts.as_ref().and_then(|b| b.get(*at * kartmodel::KART_LEN..(*at + 1) * kartmodel::KART_LEN)).map(|r| r.to_vec());
                *at += 1;
            }
        }
        Driver::Sim => {}
    }
    p.prev_kart = p.kart.take();
    p.kart = kart;
    p.pose = pose;
    p.buttons = buttons;
    p.tick = tick + 1;
}

/// Interpolate between the last two ticks for display only.
fn place_player(p: Res<Player>, time: Res<Time<Fixed>>, mut q: Query<&mut Transform, With<PlayerKart>>) {
    let a = time.overstep_fraction();
    for mut t in q.iter_mut() {
        let p0 = to_bevy(p.prev.x, p.prev.y, p.prev.z);
        let p1 = to_bevy(p.pose.x, p.pose.y, p.pose.z);
        t.translation = p0.lerp(p1, a);
        t.rotation = p.prev.rotation().slerp(p.pose.rotation(), a);
    }
}

/// Place every vehicle/driver part from the kart record (kartmodel), with
/// display interpolation between the last two ticks like `place_player`.
fn place_parts(
    p: Res<Player>,
    rig: Option<Res<KartRig>>,
    time: Res<Time<Fixed>>,
    mut q: Query<(&KartPart, &mut Transform, &mut Visibility)>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Some(rig) = rig else { return };
    let rec = |k: &Option<Vec<u8>>, pose: &Pose| match k {
        Some(r) => r.clone(),
        None => {
            let id = if pose.m == [0; 9] { [4096, 0, 0, 0, 4096, 0, 0, 0, 4096] } else { pose.m };
            kartmodel::synth_record(pose.x, pose.y, pose.z, id, &rig.model.vehicle_pivots)
        }
    };
    let k1 = rec(&p.kart, &p.pose);
    let k0 = if p.prev_kart.is_some() || p.kart.is_none() { rec(&p.prev_kart, &p.prev) } else { k1.clone() };
    let x0 = rig.model.part_transforms(&k0);
    let x1 = rig.model.part_transforms(&k1);
    let a = time.overstep_fraction();
    for (kp, mut t, mut vis) in q.iter_mut() {
        let find = |xs: &[kartmodel::PartXform]| xs.iter().find(|x| x.model == kp.model && x.part == kp.part).copied();
        let (Some(b0), Some(b1)) = (find(&x0), find(&x1)) else { continue };
        let (t0, t1) = (kartmodel::to_transform(&b0), kartmodel::to_transform(&b1));
        t.translation = t0.translation.lerp(t1.translation, a);
        t.rotation = t0.rotation.slerp(t1.rotation, a);
        let want = if b1.visible { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
    }
    let (n0, n1) = (rig.model.neck_vertices(&x0), rig.model.neck_vertices(&x1));
    for (ni, polys) in rig.neck_meshes.iter().enumerate() {
        for (pi, h) in polys.iter().enumerate() {
            let Some(mut mesh) = meshes.get_mut(h) else { continue };
            let pos: Vec<[f32; 3]> = n0[ni][pi]
                .iter()
                .zip(&n1[ni][pi])
                .map(|(v0, v1)| {
                    to_bevy(v0[0] * 32, v0[1] * 32, v0[2] * 32).lerp(to_bevy(v1[0] * 32, v1[1] * 32, v1[2] * 32), a).to_array()
                })
                .collect();
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
        }
    }
}

/// The game's camera (docs/spec/camera.md): eye + view matrix from the
/// replayed trace or mr-sim, rotation = cols (m00,-m01,-m02), (-m10,m11,m12),
/// (-m20,m21,m22). Without one, a simple chase stand-in.
fn follow_camera(p: Res<Player>, time: Res<Time<Fixed>>, mut q: Query<&mut Transform, (With<ChaseCamera>, Without<PlayerKart>)>, k: Query<&Transform, With<PlayerKart>>) {
    if let (Some((e0, m0)), Some((e1, m1))) = (p.prev.cam.or(p.pose.cam), p.pose.cam) {
        let a = time.overstep_fraction();
        let rot = |m: [i32; 9]| {
            let f = |i: usize| m[i] as f32 / 4096.0;
            Quat::from_mat3(&Mat3::from_cols(
                Vec3::new(f(0), -f(1), -f(2)),
                Vec3::new(-f(3), f(4), f(5)),
                Vec3::new(-f(6), f(7), f(8)),
            ))
            .normalize()
        };
        for mut cam in q.iter_mut() {
            cam.translation = to_bevy(e0[0], e0[1], e0[2]).lerp(to_bevy(e1[0], e1[1], e1[2]), a);
            cam.rotation = rot(m0).slerp(rot(m1), a);
        }
        return;
    }
    let Ok(kart) = k.single() else { return };
    for mut cam in q.iter_mut() {
        // kart forward is +Z in Bevy space (model -z_game flipped)
        let back = kart.rotation * Vec3::new(0.0, 0.0, -1.0);
        let target = kart.translation + back * 6.0 + Vec3::Y * 2.5;
        cam.translation = target;
        cam.look_at(kart.translation + Vec3::Y * 0.8, Vec3::Y);
    }
}

fn title(p: Res<Player>, g: Res<ghost::Ghost>, mut w: Query<&mut Window>) {
    let Ok(mut win) = w.single_mut() else { return };
    let mut s = format!("Muppet RaceMania (Rust) | tick {} | buttons {:04X}", p.tick, p.buttons);
    if let Some(d) = g.drift(&p.pose) {
        s += &format!(" | ghost drift {:.0} units", d);
    }
    if win.title != s {
        win.title = s;
    }
}

fn shot(mut plan: ResMut<ShotPlan>, p: Res<Player>, mut c: Commands, mut exit: MessageWriter<AppExit>) {
    let Some(path) = plan.path.clone() else { return };
    match plan.state {
        0 if p.tick >= plan.at => {
            c.spawn(Screenshot::primary_window()).observe(move |ev: On<ScreenshotCaptured>| {
                write_png(&ev.image, &path);
            });
            plan.state = 1;
        }
        // give the capture a few frames to land on disk
        n if n >= 1 && n < 30 => plan.state += 1,
        30 => {
            exit.write(AppExit::Success);
        }
        _ => {}
    }
}

/// Encode a captured window image with mr-assets' dependency-free PNG writer.
fn write_png(img: &Image, path: &str) {
    use bevy::render::render_resource::TextureFormat;
    let Some(data) = img.data.as_ref() else { return };
    let (w, h) = (img.width(), img.height());
    let mut rgba = data.clone();
    if matches!(img.texture_descriptor.format, TextureFormat::Bgra8Unorm | TextureFormat::Bgra8UnormSrgb) {
        for px in rgba.chunks_exact_mut(4) {
            px.swap(0, 2);
        }
    }
    for px in rgba.chunks_exact_mut(4) {
        px[3] = 255;
    }
    let _ = std::fs::write(path, mr_assets::img::png_rgba(w, h, &rgba));
}

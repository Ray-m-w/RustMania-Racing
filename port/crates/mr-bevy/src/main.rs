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
        Pose { x: c("x"), y: c("y"), z: c("z"), heading: c("heading"), m }
    }
}

/// Where the player kart's state comes from.
pub enum Driver {
    /// Play a recorded trace back (the oracle's own run) - the hybrid view.
    Replay { trace: Trace, at: usize },
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
}

#[derive(Component)]
pub struct PlayerKart;

#[derive(Component)]
pub struct ChaseCamera;

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
        Some(p) => Driver::Replay { trace: Trace::read(p).expect("read --replay trace"), at: 0 },
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
        .insert_resource(Player { driver, pose: Pose::default(), prev: Pose::default(), tick: 0, buttons: 0 })
        .insert_resource(ghost::Ghost::new(ghost))
        .add_systems(Startup, (scene::setup, setup_camera))
        .add_systems(FixedUpdate, (sim_tick, ghost::tick.after(sim_tick)))
        .insert_resource(shot_plan())
        .add_systems(Update, (place_player, ghost::place, follow_camera, title, shot.after(follow_camera)))
        .run();
}

fn setup_camera(mut c: Commands) {
    // No tonemapping: PS1 colours go to the screen as the game wrote them.
    c.spawn((Camera3d::default(), Tonemapping::None, Transform::from_xyz(0.0, 3.0, 8.0), ChaseCamera));
    c.spawn((DirectionalLight { illuminance: 8000.0, ..default() },
             Transform::from_xyz(1.0, 3.0, 2.0).looking_at(Vec3::ZERO, Vec3::Y)));
    c.insert_resource(GlobalAmbientLight { brightness: 1500.0, ..default() });
}

fn sim_tick(mut p: ResMut<Player>, keys: Res<ButtonInput<KeyCode>>, pads: Query<&Gamepad>) {
    let buttons = input::sample(&keys, &pads);
    p.prev = p.pose;
    let tick = p.tick;
    let mut pose = p.pose;
    match &mut p.driver {
        Driver::Replay { trace, at } => {
            if *at < trace.rows.len() {
                pose = Pose::from_row(trace, &trace.rows[*at]);
                *at += 1;
            }
        }
        Driver::Sim => {}
    }
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

/// Placeholder chase camera until mr-sim's port of FUN_80070418 lands.
fn follow_camera(p: Res<Player>, mut q: Query<&mut Transform, (With<ChaseCamera>, Without<PlayerKart>)>, k: Query<&Transform, With<PlayerKart>>) {
    let Ok(kart) = k.single() else { return };
    let _ = &p;
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

//! Row 12: the game's fades and LOADING screen between course sessions.
//!
//! A course change (menus, the demo, THE END; boot.rs rebuilds the course
//! in place) runs the game's sequence [CODE 80074a94, 800786b4, 8008ace4,
//! 8005df74, 8008a834, 80046dfc, 80046794, 8007d1c0, 8002d1e8]:
//!
//! 1. the menu fades the scene out: FUN_8007D174(0, 8), the level
//!    0x800AFED0 steps toward 0 by 8 per VBlank (8 * dt a frame);
//! 2. main(): a movie asked for (MOVIE / PLAY MOVIE, or the course's first
//!    load since power-on) plays;
//! 3. FUN_8008ACE4: FUN_8005DF74 loads a random picture GFX\LOAD_nn.RAW
//!    (nn = rand() / 0x5D2, 0..21, never the last one, 0x800B0814): its
//!    picture chunk (type 0x25, 320 x 256, to VRAM (0x280, 0x100)), drawn as the screen's backdrop (FUN_80046DFC:
//!    copied from x 0 (0x800B0624 = 0), y -8 (0x800B0628, FUN_80047218(0,
//!    -8)), tiled across the 320-pixel screen; FUN_80046794: the rows below
//!    filled with its last pixel's colour); it fades in by 16 a VBlank
//!    (FUN_8005DEFC(1) a loop pass, 0x800B0AEC = 2) and stays 3 more passes;
//! 4. the track loads (here: boot.rs, while the full picture shows);
//! 5. FUN_8008A834 fades the picture out (8 a VBlank, 0x800B0AEC = 1) and 3
//!    more passes, then the course session starts with its own fade-in
//!    (FUN_8007D174(0x80, 8): the title's in title.rs; the other courses'
//!    here, 8 a VBlank).
//!
//! Each fade is FUN_8002D1E8's full-screen B - F quad of (0x80 - level / 2)
//! * 2 (title.rs `fade_subtract`): over the scene an effects-pass primitive,
//! over the picture the same subtraction on its 15-bit pixels.
//!
//! The picture is drawn by its own camera (order 39, under a movie's 40) as
//! the largest centred 4:3 rectangle, like the movies; it survives the
//! course change (`KeepOnBoot`). The new session waits (virtual time paused)
//! until the picture is gone; menus and music wait too.

use std::sync::atomic::{AtomicI32, AtomicU8, Ordering};
use std::sync::Mutex;

use bevy::asset::RenderAssetUsages;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::boot::KeepOnBoot;

const W: usize = 320;
const H: usize = 240;
const LAYER: usize = 8;
/// The fade level when nothing fades (2 * 0x80).
pub const FULL: i32 = 0x100;
/// The pictures GFX/LOAD_00.RAW .. LOAD_21.RAW (rand() / 0x5D2).
pub const PICTURES: u32 = 22;
/// Passes the game keeps a finished fade (the loops' `sVar1 < 3`).
const HOLD: u32 = 3;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Phase {
    #[default]
    Idle = 0,
    /// The old scene fades to black.
    SceneOut,
    /// A movie plays (movie.rs) before the load.
    Movie,
    /// The LOADING picture fades in.
    PicIn,
    /// The picture is full: the course is rebuilt (boot.rs).
    Load,
    /// The picture fades out over the new course (waiting).
    PicOut,
    /// The new course's (or game mode's) scene fades in.
    SceneIn,
    /// A game mode change in the course (the menus' FUN_8007D174(0, 8)
    /// before 0x800B0F10 takes effect: race start, back to the course
    /// menu): the scene fades out ...
    ModeOut,
    /// ... and the new session may start (main.rs), then `SceneIn`.
    ModeReady,
}

static PHASE: AtomicU8 = AtomicU8::new(0);
static PENDING: Mutex<Option<(Vec<String>, Option<u8>)>> = Mutex::new(None);
/// A level the next `step` starts from (-1: none).
static START_LEVEL: AtomicI32 = AtomicI32::new(-1);

fn phase() -> Phase {
    match PHASE.load(Ordering::Relaxed) {
        1 => Phase::SceneOut,
        2 => Phase::Movie,
        3 => Phase::PicIn,
        4 => Phase::Load,
        5 => Phase::PicOut,
        6 => Phase::SceneIn,
        7 => Phase::ModeOut,
        8 => Phase::ModeReady,
        _ => Phase::Idle,
    }
}

fn set_phase(p: Phase) {
    PHASE.store(p as u8, Ordering::Relaxed);
}

/// Change course: boot `args` behind the fades and the LOADING picture,
/// after `movie` when given (else the new course's first-load movie, if
/// due).
pub fn change(args: Vec<String>, movie: Option<u8>) {
    if phase() != Phase::Idle && phase() != Phase::SceneIn {
        return;
    }
    *PENDING.lock().unwrap() = Some((args, movie));
    set_phase(Phase::SceneOut);
}

/// A game mode change (main.rs, the menus' session requests): false while
/// the scene fades out (started by the first call), true once it is black.
pub fn mode_change_ready() -> bool {
    match phase() {
        Phase::ModeReady => true,
        Phase::Idle | Phase::SceneIn => {
            START_LEVEL.store(FULL, Ordering::Relaxed);
            set_phase(Phase::ModeOut);
            false
        }
        _ => false,
    }
}

/// The new game mode's session started: its scene fades in.
pub fn mode_changed() {
    START_LEVEL.store(0, Ordering::Relaxed);
    set_phase(Phase::SceneIn);
}

/// A course change runs: menu input waits (the game's menus wait for
/// 0x800B0F88).
pub fn busy() -> bool {
    phase() != Phase::Idle
}

/// Between the fade-out and the new scene: the music and the new session
/// wait.
pub fn holding() -> bool {
    !matches!(phase(), Phase::Idle | Phase::SceneIn | Phase::ModeOut | Phase::ModeReady)
}

#[derive(Resource)]
struct Loading {
    level: i32,
    hold: u32,
    vb_frac: f32,
    args: Option<Vec<String>>,
    movie: Option<u8>,
    /// 0x800B0814: the last picture.
    last: i32,
    rng: u32,
    /// The picture's 15-bit frame (320 x 240) and its shown level.
    frame: Vec<u16>,
    shown: i32,
    image: Handle<Image>,
    paused_time: bool,
    title_next: bool,
}

#[derive(Component)]
struct LoadCamera;
/// Row 12: the MEMORY CARD page's picture (under the HUD, over the scene).
#[derive(Component)]
struct CardCamera;
#[derive(Component)]
struct CardQuad;

/// The shown MEMORY CARD picture (n, its image).
#[derive(Resource, Default)]
struct CardPic {
    shown: Option<u8>,
    image: Handle<Image>,
}
#[derive(Component)]
struct LoadQuad;

pub struct LoadingPlugin;

impl Plugin for LoadingPlugin {
    fn build(&self, app: &mut App) {
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(1) | 1;
        app.insert_resource(Loading { level: FULL, hold: 0, vb_frac: 0.0, args: None, movie: None, last: -1, rng: seed, frame: Vec::new(), shown: -1, image: Handle::default(), paused_time: false, title_next: false })
            .init_resource::<CardPic>()
            .add_systems(Startup, setup)
            .add_systems(Update, card_backdrop)
            .add_systems(First, load)
            .add_systems(Update, step);
        if crate::settings::arg_value("--boot-soak").is_some() {
            app.add_systems(Update, soak);
        }
    }
}

fn setup(mut c: Commands, mut res: ResMut<Loading>, mut images: ResMut<Assets<Image>>, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>, settings: Res<crate::settings::Settings>) {
    let img = Image::new(
        Extent3d { width: W as u32, height: H as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        [0, 0, 0, 255].repeat(W * H),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    res.image = images.add(img);
    c.spawn((
        Camera3d::default(),
        Camera { order: 39, clear_color: ClearColorConfig::Custom(Color::BLACK), is_active: false, ..default() },
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: bevy::camera::ScalingMode::Fixed { width: 1.0, height: 1.0 },
            near: -10.0,
            far: 10.0,
            ..OrthographicProjection::default_3d()
        }),
        Tonemapping::None,
        settings.msaa(),
        Transform::IDENTITY,
        bevy::camera::visibility::RenderLayers::layer(LAYER),
        LoadCamera,
        KeepOnBoot,
    ));
    let mat = mats.add(StandardMaterial { base_color_texture: Some(res.image.clone()), unlit: true, ..default() });
    c.spawn((
        Mesh3d(meshes.add(Mesh::from(Rectangle::new(1.0, 1.0)))),
        MeshMaterial3d(mat),
        Transform::IDENTITY,
        Visibility::Hidden,
        bevy::camera::visibility::RenderLayers::layer(LAYER),
        LoadQuad,
        KeepOnBoot,
    ));
    // row 12: the MEMORY CARD page's picture (camera order 9: over the 3D
    // view, under the HUD's 10)
    let img = Image::new(
        Extent3d { width: 512, height: H as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        [0, 0, 0, 255].repeat(512 * H),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    let card_img = images.add(img);
    c.insert_resource(CardPic { shown: None, image: card_img.clone() });
    c.spawn((
        Camera3d::default(),
        Camera { order: 9, clear_color: ClearColorConfig::Custom(Color::BLACK), is_active: false, ..default() },
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: bevy::camera::ScalingMode::Fixed { width: 1.0, height: 1.0 },
            near: -10.0,
            far: 10.0,
            ..OrthographicProjection::default_3d()
        }),
        Tonemapping::None,
        settings.msaa(),
        Transform::IDENTITY,
        bevy::camera::visibility::RenderLayers::layer(CARD_LAYER),
        CardCamera,
        KeepOnBoot,
    ));
    let mat = mats.add(StandardMaterial { base_color_texture: Some(card_img), unlit: true, ..default() });
    c.spawn((
        Mesh3d(meshes.add(Mesh::from(Rectangle::new(1.0, 1.0)))),
        MeshMaterial3d(mat),
        Transform::IDENTITY,
        Visibility::Hidden,
        bevy::camera::visibility::RenderLayers::layer(CARD_LAYER),
        CardQuad,
        KeepOnBoot,
    ));
}

const CARD_LAYER: usize = 7;

/// Row 12: the MEMORY CARD page's picture: FRONTEND/MEM_0n.RAW (course 10's
/// RAW, FUN_8004627C: n = a random byte >> 5, + 1, not the last one), drawn
/// as the loading pictures are (x 0, from line 8; 512 wide) on the view
/// rectangle (oracle: MEM_03 exact but for the text).
#[allow(clippy::type_complexity)]
fn card_backdrop(
    front: Res<crate::menus::Front>,
    settings: Res<crate::settings::Settings>,
    mut pic: ResMut<CardPic>,
    mut images: ResMut<Assets<Image>>,
    win: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut cam: Query<(&mut Camera, &mut Projection), With<CardCamera>>,
    mut quad: Query<(&mut Transform, &mut Visibility), (With<CardQuad>, Without<CardCamera>)>,
) {
    let want = (front.page == crate::menus::Page::MemoryCard).then_some(front.card_pic).flatten();
    if want != pic.shown {
        if let Some(n) = want {
            let file = mr_disc::Disc::open_default().ok().and_then(|mut d| d.read(&format!("FRONTEND/MEM_0{n}.RAW")).ok());
            let frame = file.as_deref().and_then(|f| screen_frame(f, 512, 8)).unwrap_or_else(|| vec![0; 512 * H]);
            if let Some(mut img) = images.get_mut(&pic.image) {
                img.data = Some(faded_rgba(&frame, FULL));
            }
        }
        pic.shown = want;
    }
    let on = want.is_some();
    let Ok((mut c, mut proj)) = cam.single_mut() else { return };
    if c.is_active != on {
        c.is_active = on;
    }
    let Ok((mut t, mut v)) = quad.single_mut() else { return };
    let wv = if on { Visibility::Inherited } else { Visibility::Hidden };
    if *v != wv {
        *v = wv;
    }
    if !on {
        return;
    }
    let Ok(w) = win.single() else { return };
    let size = w.physical_size();
    if size.x == 0 || size.y == 0 {
        return;
    }
    let (pos, sz) = crate::view_rect(size, settings.aspect());
    let vp = Some(bevy::camera::Viewport { physical_position: pos, physical_size: sz, ..default() });
    if c.viewport.as_ref().map(|v| (v.physical_position, v.physical_size)) != Some((pos, sz)) {
        c.viewport = vp;
    }
    if let Projection::Orthographic(o) = &mut *proj {
        let (ww, wh) = (sz.x as f32, sz.y as f32);
        if !matches!(o.scaling_mode, bevy::camera::ScalingMode::Fixed { width, height } if width == ww && height == wh) {
            o.scaling_mode = bevy::camera::ScalingMode::Fixed { width: ww, height: wh };
        }
        let want = Transform::from_scale(Vec3::new(ww, wh, 1.0));
        if *t != want {
            *t = want;
        }
    }
}

/// GFX/LOAD_nn.RAW's picture as the screen shows it (FUN_80046DFC /
/// FUN_80046794 with x 0, y -8, a 320-pixel screen), 15-bit.
pub fn picture_frame(file: &[u8]) -> Option<Vec<u16>> {
    screen_frame(file, W, 8)
}

/// A RAW's picture chunk (type 0x20 / 0x25): width, height, 15-bit pixels.
pub fn raw_picture(file: &[u8]) -> Option<(usize, usize, Vec<u16>)> {
    let raw = mr_assets::raw::parse(file).ok()?;
    let c = raw.chunks.iter().find(|c| c.kind == 0x20 || c.kind == 0x25)?;
    let d = &c.data;
    let rd32 = |o: usize| d.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let (w, h) = (rd32(4)? as usize, rd32(8)? as usize);
    if w == 0 || h == 0 {
        return None;
    }
    let px = (0..w * h).map(|i| d.get(0x10 + 2 * i..0x12 + 2 * i).map(|b| u16::from_le_bytes([b[0], b[1]])).unwrap_or(0)).collect();
    Some((w, h, px))
}

/// The picture as FUN_80046DFC / FUN_80046794 put it on a `sw`-pixel
/// screen from x 0 and y -`skip`: tiled across, the rows below it filled
/// with its last pixel's colour; 240 lines.
pub fn screen_frame(file: &[u8], sw: usize, skip: usize) -> Option<Vec<u16>> {
    let (w, h, px) = raw_picture(file)?;
    let last = px[w * h - 1];
    let mut out = vec![0u16; sw * H];
    for y in 0..H {
        for x in 0..sw {
            out[y * sw + x] = if y + skip < h { px[(y + skip) * w + x % w] } else { last };
        }
    }
    Some(out)
}

/// The frame under FUN_8002D1E8's B - F quad at `level`, as RGBA8 (15-bit
/// channels shown as c << 3, the display's 8-bit DAC values).
pub fn faded_rgba(frame: &[u16], level: i32) -> Vec<u8> {
    let sub = (crate::title::fade_subtract(level) >> 3) as i32;
    let mut out = Vec::with_capacity(frame.len() * 4);
    for &p in frame {
        for sh in [0, 5, 10] {
            let c = ((p >> sh) & 0x1F) as i32;
            out.push(((c - sub).max(0) << 3) as u8);
        }
        out.push(255);
    }
    out
}

fn next_rand(r: &mut u32) -> u32 {
    *r ^= *r << 13;
    *r ^= *r >> 17;
    *r ^= *r << 5;
    *r
}

/// The B - F quad over the scene (VIEW_HUD: the whole view).
fn scene_quad(fx: &mut crate::psxfx::FxPrims, level: i32) {
    let f = crate::title::fade_subtract(level);
    if f == 0 {
        return;
    }
    use crate::psxfx::{Blend, FxPrim, FxVert, VIEW_HUD};
    let k = f as f32 / 255.0;
    let q = [[-4096.0, 4096.0], [4096.0, 4096.0], [-4096.0, -4096.0], [4096.0, -4096.0]];
    fx.list.push(FxPrim {
        v: std::array::from_fn(|i| FxVert { pos: Vec3::new(q[i][0], q[i][1], 40.0), uv: [0.0, 0.0], rgb: [k, k, k] }),
        blend: Blend::Abr(2),
        view: VIEW_HUD,
        bucket: 101,
        always_depth: true,
        ..Default::default()
    });
}

#[allow(clippy::too_many_arguments)]
fn step(
    mut res: ResMut<Loading>,
    real: Res<Time<Real>>,
    mut time: ResMut<Time<Virtual>>,
    mut fx: ResMut<crate::psxfx::FxPrims>,
    mut images: ResMut<Assets<Image>>,
    win: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut cam: Query<(&mut Camera, &mut Projection), With<LoadCamera>>,
    mut quad: Query<(&mut Transform, &mut Visibility), (With<LoadQuad>, Without<LoadCamera>)>,
    mut c: Commands,
) {
    let ph = phase();
    if ph == Phase::Idle {
        return;
    }
    let l = START_LEVEL.swap(-1, Ordering::Relaxed);
    if l >= 0 {
        res.level = l;
        res.hold = 0;
    }
    // VBlanks this frame (at most 4: the load's long frame does not count)
    res.vb_frac += real.delta_secs() * 60.0;
    let n = (res.vb_frac.floor() as i32).clamp(0, 4);
    res.vb_frac = (res.vb_frac - res.vb_frac.floor()).min(1.0);
    match ph {
        Phase::Idle | Phase::Load => {}
        Phase::ModeOut => {
            res.level = (res.level - 8 * n).max(0);
            scene_quad(&mut fx, res.level);
            if res.level == 0 {
                res.hold += 1;
                if res.hold > HOLD {
                    set_phase(Phase::ModeReady);
                }
            }
        }
        Phase::ModeReady => scene_quad(&mut fx, 0),
        Phase::SceneOut => {
            if res.args.is_none() {
                if let Some((a, m)) = PENDING.lock().unwrap().take() {
                    res.args = Some(a);
                    res.movie = m;
                    res.level = FULL;
                    res.hold = 0;
                }
            }
            res.level = (res.level - 8 * n).max(0);
            scene_quad(&mut fx, res.level);
            if res.level == 0 {
                res.hold += 1;
                if res.hold > HOLD {
                    // main(): the movie asked for, else the new course's
                    // first-load movie
                    let m = res.movie.take().or_else(|| target_course(res.args.as_deref().unwrap_or(&[])).and_then(|c| crate::movie::course_load_movie(c)));
                    if let Some(m) = m {
                        crate::movie::request(m);
                        set_phase(Phase::Movie);
                    } else {
                        start_picture(&mut res, &mut images);
                    }
                }
            }
        }
        Phase::Movie => {
            scene_quad(&mut fx, 0);
            if !crate::movie::playing() {
                start_picture(&mut res, &mut images);
            }
        }
        Phase::PicIn => {
            scene_quad(&mut fx, 0);
            if res.level >= FULL {
                res.hold += 1;
                // `--loading-shot PATH` (checks): the full picture's window
                if res.hold == 2 {
                    if let Some(path) = crate::settings::arg_value("--loading-shot") {
                        c.spawn(bevy::render::view::screenshot::Screenshot::primary_window())
                            .observe(move |ev: On<bevy::render::view::screenshot::ScreenshotCaptured>| crate::write_png(&ev.image, &path));
                    }
                }
                if res.hold > HOLD {
                    set_phase(Phase::Load);
                }
            }
            res.level = (res.level + 16 * n).min(FULL);
        }
        Phase::PicOut => {
            scene_quad(&mut fx, 0);
            if !time.is_paused() {
                time.pause();
                res.paused_time = true;
            }
            if res.level <= 0 {
                res.hold += 1;
                if res.hold > HOLD {
                    if res.paused_time {
                        time.unpause();
                        res.paused_time = false;
                    }
                    res.level = if res.title_next { FULL } else { 0 };
                    set_phase(if res.title_next { Phase::Idle } else { Phase::SceneIn });
                }
            }
            res.level = (res.level - 8 * n).max(0);
        }
        Phase::SceneIn => {
            res.level = (res.level + 8 * n).min(FULL);
            scene_quad(&mut fx, res.level);
            if res.level >= FULL {
                set_phase(Phase::Idle);
            }
        }
    }
    // the picture
    let pic = matches!(phase(), Phase::PicIn | Phase::Load | Phase::PicOut);
    if pic && res.shown != res.level && !res.frame.is_empty() {
        let rgba = faded_rgba(&res.frame, res.level);
        if let Some(mut img) = images.get_mut(&res.image) {
            img.data = Some(rgba);
        }
        res.shown = res.level;
    }
    if let Ok((mut c, mut proj)) = cam.single_mut() {
        if c.is_active != pic {
            c.is_active = pic;
        }
        fit(&win, &mut proj, &mut quad);
    }
    if let Ok((_, mut v)) = quad.single_mut() {
        let want = if pic { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
    }
}

/// FUN_8005DF74: a random picture, not the last one.
fn start_picture(res: &mut Loading, images: &mut Assets<Image>) {
    let mut k;
    loop {
        k = (next_rand(&mut res.rng) % PICTURES) as i32;
        if k != res.last {
            break;
        }
    }
    res.last = k;
    let file = mr_disc::Disc::open_default().ok().and_then(|mut d| d.read(&format!("GFX/LOAD_{k:02}.RAW")).ok());
    res.frame = file.as_deref().and_then(picture_frame).unwrap_or_else(|| vec![0; W * H]);
    res.level = 0;
    res.hold = 0;
    res.shown = -1;
    if let Some(mut img) = images.get_mut(&res.image) {
        img.data = Some(faded_rgba(&res.frame, 0));
    }
    set_phase(Phase::PicIn);
}

/// The course a boot's arguments load (None: the title or THE END).
fn target_course(args: &[String]) -> Option<&'static mr_sim::course::CourseInfo> {
    if args.iter().any(|a| a == "--ending") {
        return None;
    }
    let name = args.iter().position(|a| a == "--course").and_then(|i| args.get(i + 1))?;
    crate::courses::races().iter().find(|c| c.name.eq_ignore_ascii_case(name))
}

/// The rebuild, at the start of the frame after the full picture showed.
fn load(world: &mut World) {
    if phase() != Phase::Load {
        return;
    }
    let args = world.resource_mut::<Loading>().args.take();
    if let Some(args) = args {
        crate::boot::reboot(world, args);
    }
    let mut res = world.resource_mut::<Loading>();
    res.level = FULL;
    res.hold = 0;
    res.title_next = crate::title::on_title();
    set_phase(Phase::PicOut);
}

/// `--boot-soak N` (tests): N course changes in a row (the race courses in
/// turn, the title between them), each 1.5 s after the last one settled;
/// `--boot-stats` lines (boot.rs) report the entities and memory; then exit.
fn soak(world: &mut World, mut since: Local<f64>) {
    let now = world.resource::<Time<Real>>().elapsed_secs_f64();
    if phase() != Phase::Idle {
        *since = now;
        return;
    }
    if now - *since < 1.5 {
        return;
    }
    *since = now;
    let n: u32 = crate::settings::arg_value("--boot-soak").and_then(|v| v.parse().ok()).unwrap_or(20);
    let g = crate::boot::generation();
    eprintln!("soak: boot {g} {} settled: {}", crate::courses::info().name, crate::boot::census(world));
    if g >= n {
        world.write_message(AppExit::Success);
        return;
    }
    let races = crate::courses::races();
    let course = (g % 2 == 0).then(|| races[(g as usize / 2 * 5) % races.len()].name.clone());
    let mut a = crate::menus::course_args(course.as_deref(), 0, 0, false);
    if !a.iter().any(|x| x == "--boot-stats") {
        a.push("--boot-stats".into());
    }
    change(a, None);
}

/// The picture: the largest centred 4:3 rectangle, black around.
fn fit(win: &Query<&Window, With<bevy::window::PrimaryWindow>>, proj: &mut Projection, quad: &mut Query<(&mut Transform, &mut Visibility), (With<LoadQuad>, Without<LoadCamera>)>) {
    let Ok(w) = win.single() else { return };
    let s = w.physical_size();
    if s.x == 0 || s.y == 0 {
        return;
    }
    let (ww, wh) = (s.x as f32, s.y as f32);
    if let Projection::Orthographic(o) = proj {
        if !matches!(o.scaling_mode, bevy::camera::ScalingMode::Fixed { width, height } if width == ww && height == wh) {
            o.scaling_mode = bevy::camera::ScalingMode::Fixed { width: ww, height: wh };
        }
    }
    let (qw, qh) = if ww * 3.0 >= wh * 4.0 { (wh * 4.0 / 3.0, wh) } else { (ww, ww * 3.0 / 4.0) };
    if let Ok((mut t, _)) = quad.single_mut() {
        let want = Transform::from_scale(Vec3::new(qw.round(), qh.round(), 1.0));
        if *t != want {
            *t = want;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pictures_decode() {
        let Ok(mut d) = mr_disc::Disc::open_default() else { return };
        for k in 0..PICTURES {
            let f = d.read(&format!("GFX/LOAD_{k:02}.RAW")).unwrap();
            let fr = picture_frame(&f).unwrap_or_else(|| panic!("LOAD_{k:02}"));
            assert_eq!(fr.len(), W * H);
        }
        // MRM_LOAD_DUMP=DIR: the 22 frames as raw RGBA (oracle checks),
        // and the memory card page's 8 pictures (512 wide, from y 0)
        if let Ok(dir) = std::env::var("MRM_LOAD_DUMP") {
            for k in 1..=8 {
                let f = d.read(&format!("FRONTEND/MEM_0{k}.RAW")).unwrap();
                std::fs::write(format!("{dir}/mem_0{k}.rgba"), faded_rgba(&screen_frame(&f, 512, 0).unwrap(), FULL)).unwrap();
            }
            for k in 0..PICTURES {
                let f = d.read(&format!("GFX/LOAD_{k:02}.RAW")).unwrap();
                std::fs::write(format!("{dir}/load_{k:02}.rgba"), faded_rgba(&picture_frame(&f).unwrap(), FULL)).unwrap();
            }
        }
        // the fade: level 0 = black, full = the picture
        let fr = vec![0x7FFFu16; 4];
        assert_eq!(faded_rgba(&fr, 0)[0], 0);
        assert_eq!(faded_rgba(&fr, FULL)[0], 0xF8);
    }
}

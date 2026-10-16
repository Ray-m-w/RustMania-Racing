//! Phase 9: the game's movies (docs/spec/fmv.md; decoding in mr-video).
//!
//! When they play (the game's code):
//! - power-on: HI_15 (0x1C) then TT_15 (0x1D) (main: 0x800B0804 = 0x1C);
//! - a course's first load since power-on, when it is open: its movie
//!   (course row + 9) [main @0x80078848: the cup / slot's save flags & 1 and
//!   the row's 0x800 bit, set then]; the attract demo's courses too;
//! - the course select's SQUARE (MOVIE) on a course whose movie object was
//!   found (save flags & 0x80) [FUN_80074A94, sub-state 0xC], and COURSE
//!   SECRETS -> PLAY MOVIE (the same bit) [FUN_80074A94 cases 8 / 9 / 0xA].
//!
//! The port: the movie covers the window (black around a 4:3 picture),
//! the scene behind it is frozen (virtual time paused) and the music paused;
//! a worker thread streams the STR from the disc (16 sectors a read),
//! decodes frames a few ahead and feeds the XA audio to the audio bus
//! (`push_movie`, mixed as CD input at the music volume); frames show at
//! their last sector's time (150 sectors a second). The player's rules
//! (mr_video::movies::Player): START / CROSS skips (boot movies after frame
//! 75), the confirm sound, the fades.
//!
//! "Power-on" spans the port's relaunches (a course change restarts the
//! program): `MRM_MOVIES_SEEN` (environment, inherited by the relaunched
//! process) holds the courses whose movie was played; its absence = a
//! fresh power-on (the boot movies). Setting `skip_intro_movies` skips the
//! boot and course-load movies (the recomp package's skip-fmv, which skips
//! every movie); MOVIE / PLAY MOVIE always play.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use std::sync::{Arc, Mutex};

use bevy::asset::RenderAssetUsages;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use mr_video::movies::{self, Movie, Player};

/// True while a movie covers the window (menus, title and music hold off).
static PLAYING: AtomicBool = AtomicBool::new(false);
/// Movies asked for by the menus (MOVIE, PLAY MOVIE).
static REQUESTS: Mutex<VecDeque<u8>> = Mutex::new(VecDeque::new());

pub fn playing() -> bool {
    PLAYING.load(Ordering::Relaxed)
}

/// Play movie `m` now (the menus).
pub fn request(m: u8) {
    REQUESTS.lock().unwrap().push_back(m);
    PLAYING.store(true, Ordering::Relaxed);
}

const ENV: &str = "MRM_MOVIES_SEEN";
const W: usize = 320;
const H: usize = 240;
const LAYER: usize = 9;
/// Movie audio queued ahead of the device (frames at 44.1 kHz).
const AUDIO_AHEAD: usize = 44_100 / 3;
/// Decoded frames queued ahead.
const FRAMES_AHEAD: usize = 4;

/// The boot and course-load movies of this process (decided once, before
/// the app runs; also records the course as seen for later relaunches).
fn startup_movies() -> Vec<u8> {
    // test runs: `--movie M` plays movie M first (0..30, or 0x..)
    if let Some(m) = crate::settings::arg_value("--movie").and_then(|v| v.strip_prefix("0x").map_or_else(|| v.parse().ok(), |h| u8::from_str_radix(h, 16).ok())) {
        return vec![m];
    }
    if crate::menus::tool_run() {
        return Vec::new();
    }
    let skip = crate::settings::Settings::global().bool("skip_intro_movies", false);
    let seen = std::env::var(ENV).ok();
    let mut mask = seen.as_deref().and_then(|s| u64::from_str_radix(s, 16).ok()).unwrap_or(0);
    let mut out = Vec::new();
    if seen.is_none() {
        // power-on (main: 0x800B0804 = 0x1C -> 0x1C, 0x1D)
        // (scripted title / menu runs keep their timing: no logos)
        let script = ["--menu-keys", "--menu-shots", "--title-shot", "--title-press-start", "--title-oracle"];
        let scripted = std::env::args().any(|x| script.contains(&x.as_str()));
        if crate::title::first_screen() == crate::title::Screen::Title && !skip && !scripted {
            out.extend(movies::BOOT);
        }
    } else {
        // main @0x80078848: the course being loaded, open, its row's 0x800
        // bit clear -> its movie; the bit is set
        let c = crate::courses::info();
        if c.cup >= 0 && c.slot >= 0 && mask & (1 << c.id) == 0 {
            let open = crate::save::with(|st| st.block.course_open(c.cup as usize, c.slot as usize));
            if open {
                mask |= 1 << c.id;
                if c.row9 >= 0 && !skip {
                    out.push(c.row9 as u8);
                }
            }
        }
    }
    // SAFETY-free: set before the app starts its own threads' work; read
    // only by the processes this one spawns.
    std::env::set_var(ENV, format!("{mask:x}"));
    out
}

struct Decoded {
    number: u32,
    ready_at: f64,
    rgba: Vec<u8>,
}

struct Cur {
    rx: Mutex<Receiver<Decoded>>,
    stop: Arc<AtomicBool>,
    start: f64,
    player: Player,
    next: Option<Decoded>,
    shown: Option<Decoded>,
    finished_stream: bool,
}

#[derive(Resource)]
struct MovieRes {
    queue: VecDeque<u8>,
    table: Vec<Movie>,
    cur: Option<Cur>,
    image: Handle<Image>,
    paused_time: bool,
    paused_music: bool,
    /// `--movie-shot PATH --movie-shot-at N`: capture the window when frame
    /// N shows (then exit after 30 updates).
    shot: Option<(String, u32, u32)>,
}

#[derive(Component)]
struct MovieCamera;
#[derive(Component)]
struct MovieQuad;

pub struct MoviePlugin;

impl Plugin for MoviePlugin {
    fn build(&self, app: &mut App) {
        let queue: VecDeque<u8> = startup_movies().into();
        PLAYING.store(!queue.is_empty(), Ordering::Relaxed);
        let table = mr_disc::Disc::open_default().ok().and_then(|mut d| d.read("FMV/FMV.BIN").ok()).map(|b| movies::table(&b)).unwrap_or_default();
        app.insert_resource(MovieRes { queue, table, cur: None, image: Handle::default(), paused_time: false, paused_music: false, shot: None })
            .add_systems(Startup, setup)
            .add_systems(Update, run);
    }
}

fn setup(
    mut c: Commands,
    mut res: ResMut<MovieRes>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    settings: Res<crate::settings::Settings>,
    mut time: ResMut<Time<Virtual>>,
) {
    let img = Image::new(
        Extent3d { width: W as u32, height: H as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        [0, 0, 0, 255].repeat(W * H),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    res.image = images.add(img);
    let on = !res.queue.is_empty();
    c.spawn((
        Camera3d::default(),
        Camera { order: 40, clear_color: ClearColorConfig::Custom(Color::BLACK), is_active: on, ..default() },
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
        MovieCamera,
    ));
    let mat = mats.add(StandardMaterial { base_color_texture: Some(res.image.clone()), unlit: true, ..default() });
    c.spawn((
        Mesh3d(meshes.add(Mesh::from(Rectangle::new(1.0, 1.0)))),
        MeshMaterial3d(mat),
        Transform::IDENTITY,
        if on { Visibility::Inherited } else { Visibility::Hidden },
        bevy::camera::visibility::RenderLayers::layer(LAYER),
        MovieQuad,
    ));
    if on && !time.is_paused() {
        // the scene waits behind the boot / course movie
        time.pause();
        res.paused_time = true;
    }
}

/// The worker: stream, decode ahead, feed the audio.
fn worker(mut disc: mr_disc::Disc, path: String, yo: usize, bus: Option<crate::audio::AudioBus>, tx: SyncSender<Decoded>, stop: Arc<AtomicBool>) {
    let Some(mut r) = mr_video::StrReader::open(&disc, &path) else {
        warn!("movie: {path} not on the disc");
        return;
    };
    let m = mr_video::mdec::Mdec::default();
    let mut rgb = Vec::new();
    while !stop.load(Ordering::Relaxed) {
        let item = match r.next(&mut disc) {
            Ok(Some(i)) => i,
            Ok(None) => return,
            Err(e) => {
                warn!("movie: {path}: {e}");
                return;
            }
        };
        match item {
            mr_video::Item::Frame(f) => {
                if f.decode(&m, &mut rgb).is_err() {
                    continue;
                }
                let (fw, fh) = (f.width as usize, f.height as usize);
                let mut rgba = [0, 0, 0, 255].repeat(W * H);
                for y in 0..fh.min(H - yo.min(H)) {
                    for x in 0..fw.min(W) {
                        let (s, d) = ((y * fw + x) * 3, ((y + yo) * W + x) * 4);
                        rgba[d..d + 3].copy_from_slice(&rgb[s..s + 3]);
                    }
                }
                if tx.send(Decoded { number: f.number, ready_at: f.ready_at(), rgba }).is_err() {
                    return;
                }
            }
            mr_video::Item::Audio { pcm, .. } => {
                let Some(bus) = bus.as_ref() else { continue };
                while bus.enabled() && bus.movie_queued() > AUDIO_AHEAD && !stop.load(Ordering::Relaxed) {
                    std::thread::sleep(std::time::Duration::from_millis(4));
                }
                bus.push_movie(&pcm);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn run(
    mut res: ResMut<MovieRes>,
    real: Res<Time<Real>>,
    mut time: ResMut<Time<Virtual>>,
    latch: Res<crate::pacing::PadLatch>,
    bus: Option<Res<crate::audio::AudioBus>>,
    mut sfx: ResMut<crate::sfx::Sfx>,
    mut images: ResMut<Assets<Image>>,
    win: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut cam: Query<(&mut Camera, &mut Projection), With<MovieCamera>>,
    mut quad: Query<(&mut Transform, &mut Visibility), With<MovieQuad>>,
    mut c: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    if let Some((_, _, n)) = res.shot.as_mut() {
        *n += 1;
        if *n > 30 {
            exit.write(AppExit::Success);
        }
    }
    {
        let mut rq = REQUESTS.lock().unwrap();
        while let Some(m) = rq.pop_front() {
            res.queue.push_back(m);
        }
    }
    let now = real.elapsed_secs_f64();
    if res.cur.is_none() {
        let Some(m) = res.queue.pop_front() else {
            if playing() {
                finish_all(&mut res, &mut time, bus.as_deref());
                show(&mut cam, &mut quad, false);
            }
            return;
        };
        PLAYING.store(true, Ordering::Relaxed);
        if !time.is_paused() {
            time.pause();
            res.paused_time = true;
        }
        if let Some(b) = bus.as_deref() {
            if !res.paused_music {
                b.music(crate::audio::MusicCmd::Pause);
                res.paused_music = true;
            }
            b.clear_movie();
            b.set_movie_mix(0);
        }
        let Some(mv) = res.table.get(m as usize).cloned() else {
            warn!("movie {m:#x}: not in FMV.BIN's table");
            return;
        };
        info!("movie {m:#x}: {} ({} frames)", mv.path, mv.end);
        let (tx, rx) = mpsc::sync_channel(FRAMES_AHEAD);
        let stop = Arc::new(AtomicBool::new(false));
        let disc = mr_disc::Disc::open_default();
        match disc {
            Ok(disc) => {
                let (st, b, path, yo) = (stop.clone(), bus.as_deref().cloned(), mv.path.clone(), movies::y_offset(m) as usize);
                let _ = std::thread::Builder::new().name("mr-movie".into()).spawn(move || worker(disc, path, yo, b, tx, st));
            }
            Err(e) => warn!("movie: cannot open the disc ({e})"),
        }
        res.cur = Some(Cur { rx: Mutex::new(rx), stop, start: now, player: Player::new(m, mv.end), next: None, shown: None, finished_stream: false });
        show(&mut cam, &mut quad, true);
    }
    fit(&win, &mut cam, &mut quad);
    let held = latch.now & (crate::input::START | crate::input::CROSS) != 0;
    let image = res.image.clone();
    let cur = res.cur.as_mut().unwrap();
    let t = now - cur.start;
    // the newest frame whose time has come
    let mut fresh = None;
    loop {
        if cur.next.is_none() {
            match cur.rx.get_mut().unwrap().try_recv() {
                Ok(d) => cur.next = Some(d),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    cur.finished_stream = true;
                    break;
                }
            }
        }
        if cur.next.as_ref().is_some_and(|d| d.ready_at <= t) {
            fresh = cur.next.take();
        } else {
            break;
        }
    }
    if let Some(mut d) = fresh {
        let (look, sound) = cur.player.frame(d.number, held);
        if sound {
            sfx.play(crate::title::CONFIRM_SFX);
        }
        if let Some(b) = bus.as_deref() {
            b.set_movie_mix(look.mix);
        }
        if look.level < 8 {
            for px in d.rgba.chunks_exact_mut(4) {
                movies::darken(&mut px[..3], look.level);
            }
        }
        if let Some(mut img) = images.get_mut(&image) {
            img.data = Some(d.rgba.clone());
        }
        let number = d.number;
        cur.shown = Some(d);
        if res.shot.is_none() {
            if let (Some(path), Some(at)) = (crate::settings::arg_value("--movie-shot"), crate::settings::arg_value("--movie-shot-at").and_then(|v| v.parse::<u32>().ok())) {
                if number >= at {
                    info!("movie: shot at frame {number} -> {path}");
                    let p2 = path.clone();
                    c.spawn(bevy::render::view::screenshot::Screenshot::primary_window())
                        .observe(move |ev: On<bevy::render::view::screenshot::ScreenshotCaptured>| crate::write_png(&ev.image, &p2));
                    res.shot = Some((path, at, 0));
                }
            }
        }
    }
    let cur = res.cur.as_mut().unwrap();
    let over = cur.player.done || (cur.finished_stream && cur.next.is_none());
    if over {
        let c = res.cur.take().unwrap();
        c.stop.store(true, Ordering::Relaxed);
        drop(c.rx);
        if let Some(b) = bus.as_deref() {
            b.clear_movie();
            b.set_movie_mix(0);
        }
        if let Some(mut img) = images.get_mut(&image) {
            img.data = Some([0, 0, 0, 255].repeat(W * H));
        }
        if res.queue.is_empty() {
            finish_all(&mut res, &mut time, bus.as_deref());
            show(&mut cam, &mut quad, false);
        }
    }
}

/// The last movie ended: the scene and the music go on.
fn finish_all(res: &mut MovieRes, time: &mut Time<Virtual>, bus: Option<&crate::audio::AudioBus>) {
    PLAYING.store(false, Ordering::Relaxed);
    if res.paused_time {
        time.unpause();
        res.paused_time = false;
    }
    if res.paused_music {
        if let Some(b) = bus {
            b.music(crate::audio::MusicCmd::Resume);
        }
        res.paused_music = false;
    }
}

fn show(cam: &mut Query<(&mut Camera, &mut Projection), With<MovieCamera>>, quad: &mut Query<(&mut Transform, &mut Visibility), With<MovieQuad>>, on: bool) {
    if let Ok((mut c, _)) = cam.single_mut() {
        if c.is_active != on {
            c.is_active = on;
        }
    }
    if let Ok((_, mut v)) = quad.single_mut() {
        let want = if on { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
    }
}

/// The picture: the largest centred 4:3 rectangle (the 320 x 240 screen as
/// a TV shows it), black around.
fn fit(win: &Query<&Window, With<bevy::window::PrimaryWindow>>, cam: &mut Query<(&mut Camera, &mut Projection), With<MovieCamera>>, quad: &mut Query<(&mut Transform, &mut Visibility), With<MovieQuad>>) {
    let Ok(w) = win.single() else { return };
    let s = w.physical_size();
    if s.x == 0 || s.y == 0 {
        return;
    }
    let (ww, wh) = (s.x as f32, s.y as f32);
    if let Ok((_, mut proj)) = cam.single_mut() {
        if let Projection::Orthographic(o) = &mut *proj {
            if !matches!(o.scaling_mode, bevy::camera::ScalingMode::Fixed { width, height } if width == ww && height == wh) {
                o.scaling_mode = bevy::camera::ScalingMode::Fixed { width: ww, height: wh };
            }
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

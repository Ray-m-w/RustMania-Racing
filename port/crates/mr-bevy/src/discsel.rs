//! Which disc image the port reads (phase 10), checked once against the
//! Redump record, and the "no disc" screen.
//!
//! Order, first match wins: `--disc PATH`; the `disc` key of settings.txt
//! (data folder, paths.rs; a relative path is taken from the data folder);
//! `$MRM_DISC`; the `disc = "..."` line of a game.toml found upward from the
//! current directory (the development repo). The choice is passed on as
//! `$MRM_DISC`, which every `mr_disc::Disc::open_default` reads. mr-disc reads a .cue (single FILE, MODE2/2352)
//! or its track .bin; not a .chd (extract it with chdman first).
//!
//! With no readable disc the window shows where to set it and the program
//! exits (`no_disc_screen`). The track file's SHA-1 is compared once with
//! Redump disc 16664 track 1 (BINLIST.toml); the result is kept in
//! `disc-check.txt` in the data folder (keyed by path, size and time), a
//! mismatch is a warning (stderr, the window title), never a refusal.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use bevy::asset::RenderAssetUsages;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

/// Redump disc 16664 (Muppet RaceMania (USA)), track 1: SHA-1 (BINLIST.toml).
pub const REDUMP_TRACK1_SHA1: &str = "1b0fc0b6fa0958ecac5f087aab4b444b2067a387";

static WARNING: OnceLock<String> = OnceLock::new();

/// A disc-check warning for the window title, once known.
pub fn warning() -> Option<&'static str> {
    WARNING.get().map(|s| s.as_str())
}

/// Pick the disc and open it once. Err = the reason no disc could be read.
pub fn resolve(settings: &crate::settings::Settings) -> Result<PathBuf, String> {
    let from_flag = crate::settings::args().into_iter().any(|a| a == "--disc");
    if !from_flag {
        if let Some(d) = settings.get("disc").map(str::trim).filter(|d| !d.is_empty()) {
            let p = PathBuf::from(d);
            let p = if p.is_relative() { crate::paths::data_dir().map(|dd| dd.join(&p)).unwrap_or(p) } else { p };
            std::env::set_var("MRM_DISC", &p);
        }
    }
    match mr_disc::Disc::open_default() {
        Ok(d) => Ok(d.bin_path().to_path_buf()),
        Err(e) => Err(match std::env::var("MRM_DISC") {
            Ok(p) => format!("cannot read the disc image {p}: {e}"),
            Err(_) => "no disc is set".to_string(),
        }),
    }
}

/// Compare the track file with the Redump hash on a worker thread (once per
/// path / size / time; cached in disc-check.txt).
pub fn check_in_background(bin: PathBuf) {
    let _ = std::thread::Builder::new().name("mr-disc-check".into()).spawn(move || {
        if let Some(w) = check(&bin, crate::paths::data_file("disc-check.txt").as_deref()) {
            eprintln!("WARNING: {w}");
            let _ = WARNING.set(w);
        }
    });
}

/// Returns a warning when the track does not match (or cannot be hashed).
fn check(bin: &Path, cache: Option<&Path>) -> Option<String> {
    let meta = std::fs::metadata(bin).ok()?;
    let mtime = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
    let key = format!("track = {}\nsize = {}\nmodified = {mtime}\n", bin.display(), meta.len());
    let cached = cache.and_then(|c| std::fs::read_to_string(c).ok()).filter(|t| t.starts_with(&key));
    let sha = match cached.as_deref().and_then(|t| t.lines().find_map(|l| l.strip_prefix("sha1 = ")).map(str::to_string)) {
        Some(s) => s,
        None => {
            let s = match mr_disc::sha1::file_hex(bin) {
                Ok(s) => s,
                Err(e) => return Some(format!("disc check: cannot read {}: {e}", bin.display())),
            };
            let ok = s == REDUMP_TRACK1_SHA1;
            if let Some(c) = cache {
                let text = format!(
                    "{key}sha1 = {s}\nredump_sha1 = {REDUMP_TRACK1_SHA1}\nresult = {}\n# Muppet RaceMania (USA), Redump disc 16664 track 1. Delete this file to check again.\n",
                    if ok { "match" } else { "MISMATCH" }
                );
                let _ = std::fs::write(c, text);
            }
            eprintln!("disc check: {} sha1 {s}: {}", bin.display(), if ok { "matches Redump disc 16664 track 1" } else { "MISMATCH" });
            s
        }
    };
    (sha != REDUMP_TRACK1_SHA1).then(|| {
        format!("the disc image {} is not the verified Redump dump of Muppet RaceMania (USA) (track 1 SHA-1 {sha}); the port may misbehave", bin.display())
    })
}

// ------------------------------------------------------------ no disc screen

const IW: usize = 480;
const IH: usize = 270;
const COLS: usize = (IW - 16) / 6;

/// The message (upper case: the built-in font has no lower case).
pub fn message(reason: &str) -> Vec<String> {
    let dir = crate::paths::data_dir().map(|d| d.display().to_string()).unwrap_or_else(|| "(the game's folder)".into());
    let mut out = Vec::new();
    let mut put = |s: &str| {
        let s = s.to_ascii_uppercase();
        let mut line = String::new();
        for ch in s.chars() {
            if line.len() == COLS {
                out.push(std::mem::take(&mut line));
                line.push_str("  ");
            }
            line.push(ch);
        }
        out.push(line);
    };
    put("MUPPET RACEMANIA NEEDS YOUR OWN DISC IMAGE");
    put("");
    put("NO DISC IMAGE WAS FOUND. OPEN THIS FILE IN A TEXT EDITOR:");
    put(&format!("  {}", Path::new(&dir).join("settings.txt").display()));
    put("AND SET THE LINE STARTING WITH DISC TO YOUR .CUE (OR .BIN):");
    put("  DISC = D:\\GAMES\\MUPPET RACEMANIA (USA).CUE");
    put("THEN START THE GAME AGAIN. A .CHD MUST BE EXTRACTED FIRST");
    put("(CHDMAN EXTRACTCD). SEE README-PORT.TXT.");
    put("");
    put(&format!("REASON: {reason}"));
    put("");
    put("PRESS ANY KEY OR CLOSE THE WINDOW TO EXIT.");
    out
}

/// Draw the lines into an RGBA image (5x7 font, 6x9 cells).
pub fn render(lines: &[String]) -> Vec<u8> {
    let mut px = [16u8, 16, 40, 255].repeat(IW * IH);
    for (row, l) in lines.iter().enumerate() {
        let y0 = 8 + row * 9;
        if y0 + 7 > IH {
            break;
        }
        let col = if row == 0 { [255, 220, 80] } else { [230, 230, 230] };
        for (i, ch) in l.chars().enumerate().take(COLS) {
            let g = glyph(ch);
            let x0 = 8 + i * 6;
            for (cx, bits) in g.iter().enumerate() {
                for cy in 0..7 {
                    if bits >> cy & 1 != 0 {
                        let o = ((y0 + cy) * IW + x0 + cx) * 4;
                        px[o..o + 3].copy_from_slice(&col);
                    }
                }
            }
        }
    }
    px
}

/// Show the "no disc" screen until a key, a pad button or the window's close
/// (`--shot PATH` writes the picture and exits), then exit the process.
pub fn no_disc_screen(reason: &str) -> ! {
    eprintln!("Muppet RaceMania: {reason}. Set `disc = <your .cue>` in settings.txt in the data folder.");
    // A settings.txt to edit (every key with its help, `disc` among them).
    if let Some(p) = crate::paths::data_file("settings.txt").filter(|p| !p.exists()) {
        let _ = std::fs::write(&p, crate::settings::Settings::default().to_text());
    }
    let pixels = render(&message(reason));
    if let Some(shot) = crate::settings::arg_value("--shot") {
        let _ = std::fs::write(&shot, mr_assets::img::png_rgba(IW as u32, IH as u32, &pixels));
        std::process::exit(2);
    }
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Muppet RaceMania (Rust): no disc".into(),
                resolution: ((IW * 2) as u32, (IH * 2) as u32).into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(NoDisc(pixels))
        .add_systems(Startup, setup)
        .add_systems(Update, any_key)
        .run();
    std::process::exit(2);
}

#[derive(Resource)]
struct NoDisc(Vec<u8>);

fn setup(mut c: Commands, nd: Res<NoDisc>, mut images: ResMut<Assets<Image>>, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let mut img = Image::new(
        Extent3d { width: IW as u32, height: IH as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        nd.0.clone(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    img.sampler = ImageSampler::nearest();
    c.spawn((
        Camera3d::default(),
        Camera { clear_color: ClearColorConfig::Custom(Color::BLACK), ..default() },
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: bevy::camera::ScalingMode::Fixed { width: 1.0, height: 1.0 },
            near: -10.0,
            far: 10.0,
            ..OrthographicProjection::default_3d()
        }),
        Tonemapping::None,
        Transform::IDENTITY,
    ));
    let mat = mats.add(StandardMaterial { base_color_texture: Some(images.add(img)), unlit: true, ..default() });
    c.spawn((Mesh3d(meshes.add(Mesh::from(Rectangle::new(1.0, 1.0)))), MeshMaterial3d(mat), Transform::IDENTITY));
}

fn any_key(keys: Res<ButtonInput<KeyCode>>, pads: Query<&Gamepad>, mut exit: MessageWriter<AppExit>) {
    if keys.get_just_pressed().next().is_some() || pads.iter().any(|g| g.get_just_pressed().next().is_some()) {
        exit.write(AppExit::Success);
    }
}

/// 5x7 font, column bytes (bit 0 = top row); ASCII 0x20..0x5F.
fn glyph(c: char) -> [u8; 5] {
    const F: [[u8; 5]; 64] = [
        [0x00, 0x00, 0x00, 0x00, 0x00], [0x00, 0x00, 0x5F, 0x00, 0x00], [0x00, 0x07, 0x00, 0x07, 0x00], [0x14, 0x7F, 0x14, 0x7F, 0x14],
        [0x24, 0x2A, 0x7F, 0x2A, 0x12], [0x23, 0x13, 0x08, 0x64, 0x62], [0x36, 0x49, 0x56, 0x20, 0x50], [0x00, 0x05, 0x03, 0x00, 0x00],
        [0x00, 0x1C, 0x22, 0x41, 0x00], [0x00, 0x41, 0x22, 0x1C, 0x00], [0x2A, 0x1C, 0x7F, 0x1C, 0x2A], [0x08, 0x08, 0x3E, 0x08, 0x08],
        [0x00, 0x50, 0x30, 0x00, 0x00], [0x08, 0x08, 0x08, 0x08, 0x08], [0x00, 0x60, 0x60, 0x00, 0x00], [0x20, 0x10, 0x08, 0x04, 0x02],
        [0x3E, 0x51, 0x49, 0x45, 0x3E], [0x00, 0x42, 0x7F, 0x40, 0x00], [0x42, 0x61, 0x51, 0x49, 0x46], [0x21, 0x41, 0x45, 0x4B, 0x31],
        [0x18, 0x14, 0x12, 0x7F, 0x10], [0x27, 0x45, 0x45, 0x45, 0x39], [0x3C, 0x4A, 0x49, 0x49, 0x30], [0x01, 0x71, 0x09, 0x05, 0x03],
        [0x36, 0x49, 0x49, 0x49, 0x36], [0x06, 0x49, 0x49, 0x29, 0x1E], [0x00, 0x36, 0x36, 0x00, 0x00], [0x00, 0x56, 0x36, 0x00, 0x00],
        [0x08, 0x14, 0x22, 0x41, 0x00], [0x14, 0x14, 0x14, 0x14, 0x14], [0x00, 0x41, 0x22, 0x14, 0x08], [0x02, 0x01, 0x51, 0x09, 0x06],
        [0x32, 0x49, 0x79, 0x41, 0x3E], [0x7E, 0x11, 0x11, 0x11, 0x7E], [0x7F, 0x49, 0x49, 0x49, 0x36], [0x3E, 0x41, 0x41, 0x41, 0x22],
        [0x7F, 0x41, 0x41, 0x22, 0x1C], [0x7F, 0x49, 0x49, 0x49, 0x41], [0x7F, 0x09, 0x09, 0x09, 0x01], [0x3E, 0x41, 0x49, 0x49, 0x7A],
        [0x7F, 0x08, 0x08, 0x08, 0x7F], [0x00, 0x41, 0x7F, 0x41, 0x00], [0x20, 0x40, 0x41, 0x3F, 0x01], [0x7F, 0x08, 0x14, 0x22, 0x41],
        [0x7F, 0x40, 0x40, 0x40, 0x40], [0x7F, 0x02, 0x0C, 0x02, 0x7F], [0x7F, 0x04, 0x08, 0x10, 0x7F], [0x3E, 0x41, 0x41, 0x41, 0x3E],
        [0x7F, 0x09, 0x09, 0x09, 0x06], [0x3E, 0x41, 0x51, 0x21, 0x5E], [0x7F, 0x09, 0x19, 0x29, 0x46], [0x46, 0x49, 0x49, 0x49, 0x31],
        [0x01, 0x01, 0x7F, 0x01, 0x01], [0x3F, 0x40, 0x40, 0x40, 0x3F], [0x1F, 0x20, 0x40, 0x20, 0x1F], [0x3F, 0x40, 0x38, 0x40, 0x3F],
        [0x63, 0x14, 0x08, 0x14, 0x63], [0x07, 0x08, 0x70, 0x08, 0x07], [0x61, 0x51, 0x49, 0x45, 0x43], [0x00, 0x7F, 0x41, 0x41, 0x00],
        [0x02, 0x04, 0x08, 0x10, 0x20], [0x00, 0x41, 0x41, 0x7F, 0x00], [0x04, 0x02, 0x01, 0x02, 0x04], [0x40, 0x40, 0x40, 0x40, 0x40],
    ];
    let c = c.to_ascii_uppercase() as u32;
    if (0x20..0x60).contains(&c) {
        F[(c - 0x20) as usize]
    } else {
        F[('?' as u32 - 0x20) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_caches_and_warns() {
        let tmp = std::env::temp_dir().join(format!("mrm-disccheck-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let (bin, cache) = (tmp.join("t.bin"), tmp.join("disc-check.txt"));
        std::fs::write(&bin, b"abc").unwrap();
        let w = check(&bin, Some(&cache)).expect("not the Redump track");
        assert!(w.contains("a9993e364706816aba3e25717850c26c9cd0d89d"));
        let text = std::fs::read_to_string(&cache).unwrap();
        assert!(text.contains("result = MISMATCH"));
        // cached: same answer without rehashing (a forged cache line is used)
        std::fs::write(&cache, text.replace("sha1 = a9993e364706816aba3e25717850c26c9cd0d89d", &format!("sha1 = {REDUMP_TRACK1_SHA1}"))).unwrap();
        assert!(check(&bin, Some(&cache)).is_none());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn message_fits() {
        let m = message("no disc is set");
        assert!(m.iter().all(|l| l.len() <= COLS));
        assert!(m.len() * 9 + 8 <= IH);
        assert_eq!(render(&m).len(), IW * IH * 4);
    }
}

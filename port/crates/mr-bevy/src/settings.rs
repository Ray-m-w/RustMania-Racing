//! Every user option in one place: the `Settings` resource (Session 5).
//!
//! Sources, later ones win:
//! 1. the defaults in `KNOWN`;
//! 2. `settings.txt` beside the executable (or `--settings PATH`): plain text,
//!    one `key = value` per line, `#` starts a comment, blank lines ignored;
//! 3. the command line: `--set key=value` (any key, repeatable) or
//!    `--key-with-dashes [value]` for a known key (a bool key alone = true).
//!
//! Values are kept as strings; typed getters parse them (an unparsable value
//! warns once on stderr and falls back to the default). `KNOWN` lists every
//! option with its default and allowed values, so a future settings menu can
//! iterate it and write the file back with `Settings::to_text`.
//!
//! Outside ECS systems (e.g. code that runs before the App exists),
//! `Settings::global()` returns the same values (loaded once, from the
//! process's arguments).

use std::collections::BTreeMap;
use std::sync::OnceLock;

use bevy::prelude::*;

/// One known option: key, default, allowed values ("" = free-form), help.
pub struct Opt {
    pub key: &'static str,
    pub default: &'static str,
    pub allowed: &'static str,
    pub help: &'static str,
}

/// Every option. Tracks append rows here (one line each).
pub const KNOWN: &[Opt] = &[
    // --- E view ---
    Opt { key: "faithful_view", default: "false", allowed: "true|false", help: "preset: every view option at the game's look (faithful draw distance, 4:3, nearest, faithful lighting; window 1280x960 unless resolution is set): for side-by-sides with the oracle" },
    Opt { key: "faithful_draw_distance", default: "false", allowed: "true|false", help: "the game's per-frame object selection (visibility stream, cull radius, layer-1 far stand-ins); false = the whole course every frame" },
    Opt { key: "aspect", default: "native", allowed: "native|4:3", help: "native = the window's aspect (horizontal FOV widened, vertical FOV kept, HUD pinned to its edges); 4:3 = the game's view, pillarboxed" },
    Opt { key: "msaa", default: "4", allowed: "off|1|2|4|8", help: "multisample anti-aliasing samples (off = 1)" },
    Opt { key: "texture_filter", default: "smooth", allowed: "nearest|smooth", help: "nearest = PS1 point sampling; smooth = bilinear + mipmaps (cut-out edges bled, no fringes)" },
    Opt { key: "lighting", default: "enhanced", allowed: "faithful|enhanced", help: "enhanced (default, owner decision 2026-10-13) = distance haze toward the sky's horizon colour over the game's vertex colours; faithful = the game's baked vertex colours only" },
    Opt { key: "resolution", default: "1600x900", allowed: "", help: "window size WxH in logical pixels" },
    Opt { key: "fullscreen", default: "false", allowed: "true|false", help: "borderless fullscreen on the current monitor" },
    Opt { key: "vsync", default: "true", allowed: "true|false", help: "wait for the display's refresh (false = uncapped, for measuring)" },
    // --- F effects --- (append below)
    Opt { key: "fx_blend", default: "psx", allowed: "psx|linear", help: "psx = effects, shadows, semi-transparent scenery (water) and HUD primitives blended as the PS1 GPU does (B/2+F/2, B+F, B-F, B+F/4 on the gamma-encoded frame); linear = the same equations in linear light (softer additive glow, dimmer dim sparks)" },
    Opt { key: "fx_depth_test", default: "true", allowed: "true|false", help: "true = sprites, particles, projectiles and shadows are hidden by walls (the ground under a sprite never hides it, as the game's ordering table draws the ground first); false = always drawn over the scene (debug)" },
    // --- G 60 Hz --- (append below)
    Opt { key: "logic_hz", default: "60", allowed: "30|60", help: "simulation rate: 30 = the game's logic (2 VBlanks per tick, exact against the original); 60 = the 60 Hz mode (1 VBlank per tick, within the feel tolerance of 30; kart-physics-ledger.md \"60 Hz mode\")" },
    Opt { key: "render_latency", default: "low", allowed: "low|normal", help: "low = no pipelined rendering (the frame is rendered right after its update) and a swapchain of at most 1 queued frame; normal = Bevy's defaults (render one frame behind on its own thread, 2 queued frames)" },
    Opt { key: "low_latency", default: "true", allowed: "true|false", help: "pad sampled every display frame and the coming tick shown as it is sampled (look-ahead on a throw-away copy; the simulation is unchanged); false = the Session 4 display (interpolated one tick behind)" },
    // --- P courses ---
    Opt { key: "course", default: "", allowed: "", help: "race course with skip_title / --skip-title: a TER file name of the game's 18 race courses (DOCK, BOG, NEW_YORK, ...; courses.rs) or its index; empty = DOCK" },
    // --- T title --- (append below)
    Opt { key: "skip_title", default: "false", allowed: "true|false", help: "start in the race instead of the title screen (tools and tests; --oracle-grid, --route, --replay, --shot and the other tool flags always do)" },
    // --- Session 7 front end (menus.rs) ---
    Opt { key: "music_volume", default: "12", allowed: "", help: "ADJUST SOUND's music volume, the game's steps 0..15 (12 = the game's default level)" },
    Opt { key: "sound_volume", default: "12", allowed: "", help: "ADJUST SOUND's sound effects volume, 0..15 (12 = the game's default)" },
    Opt { key: "controller_config", default: "A", allowed: "A|B", help: "SET CONTROLLERS: A = square jumps, circle brakes; B = the game's alternate layout (square brakes, circle jumps)" },
    Opt { key: "unlock_all", default: "false", allowed: "true|false", help: "show every character, vehicle, course and league open (the game's everything code on a copy of the save; save.bin itself is not changed); false = the progress in save.bin" },
];

#[derive(Resource, Clone, Debug)]
pub struct Settings {
    /// key -> value, for every known key (defaults filled) and any unknown
    /// key a file or `--set` gave (kept, warned about).
    pub values: BTreeMap<String, String>,
    /// Where the file was read from (None = no file).
    pub file: Option<std::path::PathBuf>,
    /// Keys a file or the command line set (not defaults).
    pub explicit: std::collections::BTreeSet<String>,
}

static GLOBAL: OnceLock<Settings> = OnceLock::new();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Aspect {
    Native,
    FourThree,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TexFilter {
    Nearest,
    Smooth,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lighting {
    Faithful,
    Enhanced,
}

impl Default for Settings {
    fn default() -> Self {
        let values = KNOWN.iter().map(|o| (o.key.to_string(), o.default.to_string())).collect();
        Settings { values, file: None, explicit: Default::default() }
    }
}

/// Parse settings text: `key = value` lines, `#` comments. Returns the pairs
/// in order and the lines it could not read (1-based line number, text).
pub fn parse_text(text: &str) -> (Vec<(String, String)>, Vec<(usize, String)>) {
    let mut out = Vec::new();
    let mut bad = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let l = line.split('#').next().unwrap_or("").trim();
        if l.is_empty() {
            continue;
        }
        match l.split_once('=') {
            Some((k, v)) if !k.trim().is_empty() => out.push((norm_key(k), v.trim().trim_matches('"').to_string())),
            _ => bad.push((n + 1, line.to_string())),
        }
    }
    (out, bad)
}

fn norm_key(k: &str) -> String {
    k.trim().to_ascii_lowercase().replace('-', "_")
}

impl Settings {
    /// Defaults <- file <- command line (the process's arguments).
    pub fn load() -> Settings {
        let args: Vec<String> = std::env::args().collect();
        Self::load_from(&args)
    }

    /// The process-wide settings (loaded on first use).
    pub fn global() -> &'static Settings {
        GLOBAL.get_or_init(Settings::load)
    }

    pub fn load_from(args: &[String]) -> Settings {
        let mut s = Settings::default();
        let explicit = args.iter().position(|a| a == "--settings").and_then(|i| args.get(i + 1)).map(std::path::PathBuf::from);
        let path = explicit.clone().or_else(|| std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("settings.txt"))));
        if let Some(p) = path {
            match std::fs::read_to_string(&p) {
                Ok(text) => {
                    let (pairs, bad) = parse_text(&text);
                    for (n, l) in bad {
                        eprintln!("settings: {}:{n}: not `key = value`: {l}", p.display());
                    }
                    for (k, v) in pairs {
                        if let Err(e) = s.set(&k, &v) {
                            eprintln!("settings: {}: {e}", p.display());
                        }
                    }
                    s.file = Some(p);
                }
                Err(e) if explicit.is_some() => eprintln!("settings: cannot read {}: {e}", p.display()),
                Err(_) => {}
            }
        }
        s.apply_args(args);
        s
    }

    /// `--set key=value` and `--key-with-dashes [value]` for known keys.
    pub fn apply_args(&mut self, args: &[String]) {
        let mut i = 1;
        while i < args.len() {
            let a = &args[i];
            if a == "--set" {
                if let Some((k, v)) = args.get(i + 1).and_then(|kv| kv.split_once('=')) {
                    if let Err(e) = self.set(k, v) {
                        eprintln!("settings: --set: {e}");
                    }
                }
                i += 2;
                continue;
            }
            if let Some(name) = a.strip_prefix("--") {
                let key = norm_key(name);
                if let Some(o) = KNOWN.iter().find(|o| o.key == key) {
                    let next = args.get(i + 1).filter(|n| !n.starts_with("--"));
                    let is_bool = o.allowed == "true|false";
                    let v = match next {
                        Some(n) if !is_bool || matches!(n.as_str(), "true" | "false" | "on" | "off" | "1" | "0") => {
                            i += 1;
                            n.clone()
                        }
                        _ if is_bool => "true".into(),
                        _ => {
                            eprintln!("settings: --{name} needs a value ({})", o.allowed);
                            i += 1;
                            continue;
                        }
                    };
                    if let Err(e) = self.set(&key, &v) {
                        eprintln!("settings: --{name}: {e}");
                    }
                }
            }
            i += 1;
        }
    }

    /// Set one option (validated against `KNOWN.allowed` when listed).
    pub fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
        let key = norm_key(key);
        let value = value.trim().to_string();
        match KNOWN.iter().find(|o| o.key == key) {
            Some(o) if !o.allowed.is_empty() => {
                let ok = if o.allowed == "true|false" {
                    parse_bool(&value).is_some()
                } else {
                    o.allowed.split('|').any(|a| a.eq_ignore_ascii_case(&value))
                };
                if !ok {
                    return Err(format!("{key} = {value}: expected {}", o.allowed));
                }
            }
            Some(_) => {}
            None => eprintln!("settings: unknown key {key} (kept)"),
        }
        self.explicit.insert(key.clone());
        self.values.insert(key, value);
        Ok(())
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(|s| s.as_str())
    }

    /// A bool option ("true/false/on/off/yes/no/1/0"); `default` when unset.
    pub fn bool(&self, key: &str, default: bool) -> bool {
        self.get(key).and_then(parse_bool).unwrap_or(default)
    }

    pub fn f32(&self, key: &str, default: f32) -> f32 {
        self.get(key).and_then(|v| v.parse().ok()).unwrap_or(default)
    }

    pub fn i64(&self, key: &str, default: i64) -> i64 {
        self.get(key).and_then(|v| v.parse().ok()).unwrap_or(default)
    }

    // --- typed getters, E view ---
    /// The `faithful_view` preset.
    pub fn faithful_view(&self) -> bool {
        self.bool("faithful_view", false)
    }

    pub fn faithful_draw_distance(&self) -> bool {
        self.faithful_view() || self.bool("faithful_draw_distance", false)
    }

    pub fn aspect(&self) -> Aspect {
        if self.faithful_view() {
            return Aspect::FourThree;
        }
        match self.get("aspect") {
            Some("4:3") => Aspect::FourThree,
            _ => Aspect::Native,
        }
    }

    /// MSAA sample count: 1 (off), 2, 4 or 8.
    /// `render_latency = low` (default).
    pub fn render_latency_low(&self) -> bool {
        self.get("render_latency") != Some("normal")
    }

    pub fn msaa_samples(&self) -> u32 {
        match self.get("msaa").unwrap_or("4") {
            "off" | "1" | "0" => 1,
            "2" => 2,
            "8" => 8,
            _ => 4,
        }
    }

    pub fn msaa(&self) -> Msaa {
        match self.msaa_samples() {
            1 => Msaa::Off,
            2 => Msaa::Sample2,
            8 => Msaa::Sample8,
            _ => Msaa::Sample4,
        }
    }

    pub fn texture_filter(&self) -> TexFilter {
        if self.faithful_view() {
            return TexFilter::Nearest;
        }
        match self.get("texture_filter") {
            Some("nearest") => TexFilter::Nearest,
            _ => TexFilter::Smooth,
        }
    }

    pub fn lighting(&self) -> Lighting {
        if self.faithful_view() {
            return Lighting::Faithful;
        }
        match self.get("lighting") {
            Some("faithful") => Lighting::Faithful,
            _ => Lighting::Enhanced,
        }
    }

    /// Window size (logical pixels) from `resolution` = "WxH".
    pub fn resolution(&self) -> (u32, u32) {
        if self.faithful_view() && !self.explicit.contains("resolution") {
            return (1280, 960);
        }
        self.get("resolution")
            .and_then(|r| r.split_once(['x', 'X']))
            .and_then(|(w, h)| Some((w.trim().parse().ok()?, h.trim().parse().ok()?)))
            .filter(|&(w, h): &(u32, u32)| w >= 64 && h >= 64)
            .unwrap_or((1600, 900))
    }

    pub fn fullscreen(&self) -> bool {
        self.bool("fullscreen", false)
    }

    /// The file form of these settings (known keys with their help as
    /// comments, then any unknown keys).
    pub fn to_text(&self) -> String {
        let mut s = String::from("# Muppet RaceMania (Rust) settings: key = value; command-line --set key=value overrides.\n");
        for o in KNOWN {
            s += &format!("\n# {} ({}; default {})\n{} = {}\n", o.help, o.allowed, o.default, o.key, self.get(o.key).unwrap_or(o.default));
        }
        for (k, v) in &self.values {
            if !KNOWN.iter().any(|o| o.key == k) {
                s += &format!("{k} = {v}\n");
            }
        }
        s
    }
}

/// The value after a command-line flag (`--flag VALUE`), for one-off tool flags
/// that are not settings (paths, debug switches).
pub fn arg_value(flag: &str) -> Option<String> {
    let a: Vec<String> = std::env::args().collect();
    a.iter().position(|x| x == flag).and_then(|i| a.get(i + 1)).cloned()
}

fn parse_bool(v: &str) -> Option<bool> {
    match v.to_ascii_lowercase().as_str() {
        "true" | "on" | "yes" | "1" => Some(true),
        "false" | "off" | "no" | "0" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        std::iter::once("mr-bevy").chain(v.iter().copied()).map(String::from).collect()
    }

    #[test]
    fn defaults_file_and_cli() {
        let s = Settings::default();
        assert!(!s.faithful_draw_distance());
        assert_eq!(s.msaa_samples(), 4);
        assert_eq!(s.texture_filter(), TexFilter::Smooth);
        assert_eq!(s.aspect(), Aspect::Native);
        assert_eq!(s.lighting(), Lighting::Enhanced);

        let (pairs, bad) = parse_text("# c\nmsaa = 8\n  Texture-Filter=nearest # trailing\nbroken line\n");
        assert_eq!(pairs, vec![("msaa".into(), "8".into()), ("texture_filter".into(), "nearest".into())]);
        assert_eq!(bad.len(), 1);

        let mut s = Settings::default();
        s.apply_args(&args(&["--faithful-draw-distance", "--msaa", "off", "--set", "aspect=4:3", "--set", "logic_hz=60", "--shot", "x.png"]));
        assert!(s.faithful_draw_distance());
        assert_eq!(s.msaa_samples(), 1);
        assert_eq!(s.aspect(), Aspect::FourThree);
        assert_eq!(s.get("logic_hz"), Some("60"));
        assert!(s.set("msaa", "3").is_err());
        assert_eq!(s.msaa_samples(), 1);
        let mut t = Settings::default();
        let (pairs, _) = parse_text(&s.to_text());
        for (k, v) in pairs {
            t.set(&k, &v).unwrap();
        }
        assert_eq!(t.values, s.values);
        let mut f = Settings::default();
        f.apply_args(&args(&["--faithful-view"]));
        assert!(f.faithful_draw_distance());
        assert_eq!((f.aspect(), f.texture_filter(), f.resolution()), (Aspect::FourThree, TexFilter::Nearest, (1280, 960)));
        assert_eq!(f.lighting(), Lighting::Faithful);
    }
}

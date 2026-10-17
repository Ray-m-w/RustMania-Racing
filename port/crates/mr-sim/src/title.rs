//! The title screen's karts (Session 6, track T; docs/spec/frontend.md).
//!
//! The game's title is the frontend course 0x1E run in game mode 0x10
//! (FUN_8008ACE4). The race loader FUN_8008C4AC sets it up with three karts,
//! all AI (0x800B0DFE = 3; FUN_800609F4(kart, c, v, grid, 1) each, the
//! character drawn by rand() / 0x2AB among the loaded racers not already
//! taken, its vehicle from the character table or drawn), the first grid
//! slot *rng % 3 [CODE 8008c4ac @0x8008DB90..]; no countdown (0x800B0C70 =
//! 0), pickups on (0x800B0D80 | 1), race flag 8 (0x800B07E2 = 8 at the end
//! of the loader) [CODE, RAM]. The karts drive with the battle AI
//! (FUN_80090FAC: race type 2) along OPTIONS.TER's one path segment. Race
//! flag 8 knocks a kart out once its damage +0x440 reaches 0x7D on this
//! course (FUN_80082708 @0x80082770); on course 0x1E the knocked-out kart
//! is at once re-created on a random grid slot with a new random character
//! (FUN_8008B258) and vehicle (FUN_8008B1BC) [CODE 80082708 @0x800828C4]:
//! `respawn`.
//!
//! Check: tools/oracle/title_check.py karts + `mr-title` (from the oracle's
//! title state, free-running with the oracle's dt).

use crate::ai::{self, Opponent, TEMPLATE_SIZE};
use crate::globals::RaceGlobals;
use crate::kart::KartState;
use crate::world::World;

/// 0x800B0FAC of the title (`frontend\options`).
pub const TITLE_COURSE: u8 = 0x1E;
/// 0x800B0D46 on the title.
pub const TITLE_MODE: i16 = 0x10;
/// 0x800B0DFE on the title (FUN_8008C4AC @0x8008D6F0).
pub const TITLE_KARTS: usize = 3;
/// 0x800B07E2 on the title (FUN_8008C4AC's last statement).
pub const TITLE_RACE_FLAGS: u16 = 8;

/// The water animation state 0x800AFEF0..0x800AFF0B at the oracle's title
/// load (BACK TO MAIN MENU from savestate slot 1, VB 407): the game never
/// resets it, so it carries the power-on history (water.rs). With the
/// oracle's rand() seed it makes a new title session that run, tick for
/// tick (mr-title --fresh-vs).
pub const ORACLE_WATER: [u8; 0x1C] = [
    0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x80, 0x05, 0xC0, 0x06, 0x80, 0x0C, 0xB4, 0xFE, 0xD4, 0xFF, 0xA8, 0x00, 0x18, 0xFE, 0x04, 0x00, 0xD4, 0xFF, 0x3C, 0x00, 0xB4, 0xFE,
];
/// FUN_8007A9DC's two blink toggles and timers 0x800B07B0..0x800B07B3 at
/// the same load: also never reset by the loader (power-on history); with
/// zeros both timers fire in the title's first frame and draw one random
/// byte more than the game (17 in the oracle's frame 1, phase 14).
pub const ORACLE_BLINK: [u8; 4] = [0x00, 0x01, 0x00, 0x0F];
/// The projectile table 0x800DDC50 (4 x 0x48 bytes) at the same load: the
/// loader leaves it as the last race left it (all inactive), and
/// FUN_80068C6C's launch keeps the fields it does not write (+0x33, the
/// grounded flag the random hop tests): the title's first projectiles act on
/// that history (phase 14: without it the oracle's run is left at tick 444).
pub const ORACLE_PROJECTILES: [u8; 0x120] = [
    0xEB, 0x79, 0x02, 0x00, 0xC4, 0x0A, 0x01, 0x00, 0x9C, 0xCF, 0x08, 0x00, 0x54, 0xFC, 0xFF, 0xFF,
    0x08, 0x00, 0x00, 0x00, 0x6F, 0xF0, 0xFF, 0xFF, 0x9E, 0x2B, 0x01, 0x00, 0x20, 0x00, 0x03, 0x00,
    0xA0, 0xF5, 0x0A, 0x00, 0x33, 0x40, 0x03, 0x00, 0x32, 0x18, 0x0C, 0x00, 0x05, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x01, 0x00, 0x80, 0x02, 0x36, 0x00, 0x78, 0x00, 0xD0, 0x0D, 0x40, 0x00, 0x2E, 0x00,
    0x01, 0x00, 0x00, 0x00, 0x28, 0xA4, 0x18, 0x80, 0xA1, 0xB6, 0x00, 0x00, 0x95, 0xB9, 0x00, 0x00,
    0x30, 0xFF, 0x09, 0x00, 0x12, 0xF0, 0xFF, 0xFF, 0xCD, 0xFA, 0xFF, 0xFF, 0x7E, 0x01, 0x00, 0x00,
    0x76, 0xDA, 0x00, 0x00, 0x20, 0x20, 0x01, 0x00, 0x60, 0x27, 0x0A, 0x00, 0x39, 0xF5, 0x01, 0x00,
    0x58, 0xE1, 0x09, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x80, 0x02, 0x14, 0x00,
    0x78, 0x00, 0x40, 0x07, 0xC0, 0xFF, 0x2E, 0x00, 0x07, 0x00, 0x00, 0x00, 0x28, 0xC1, 0x18, 0x80,
    0xA0, 0xAA, 0xFD, 0xFF, 0x34, 0x62, 0x00, 0x00, 0x3D, 0xBE, 0x09, 0x00, 0xC2, 0xFF, 0xFF, 0xFF,
    0x00, 0x00, 0x00, 0x00, 0x54, 0x00, 0x00, 0x00, 0x34, 0x62, 0x00, 0x00, 0x20, 0x0F, 0xFE, 0xFF,
    0x80, 0xDF, 0x09, 0x00, 0xE0, 0x94, 0xFF, 0xFF, 0xAB, 0xBD, 0x09, 0x00, 0x01, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x01, 0x01, 0x78, 0x00, 0x92, 0x00, 0x2C, 0x01, 0xD0, 0x01, 0x40, 0x00, 0x1E, 0x00,
    0x06, 0x00, 0x00, 0x00, 0x38, 0xEE, 0x18, 0x80, 0x12, 0xB2, 0x02, 0x00, 0xA0, 0x2A, 0x01, 0x00,
    0x1A, 0xB4, 0x09, 0x00, 0x05, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0xA0, 0x2A, 0x01, 0x00, 0xA0, 0xCF, 0xFE, 0xFF, 0xA0, 0xEC, 0x09, 0x00, 0x20, 0x85, 0x00, 0x00,
    0x72, 0xB3, 0x09, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x01, 0x78, 0x00, 0x2E, 0x01,
    0x2C, 0x01, 0xC0, 0x05, 0xC0, 0xFF, 0x1E, 0x00, 0x05, 0x00, 0x00, 0x00, 0x48, 0xC5, 0x18, 0x80
];
/// 0x800C305B at the oracle's title load (SessionConfig::round_robin_start:
/// the 3-kart round robin of FUN_8009004C).
pub const ORACLE_ROUND_ROBIN: u8 = 0;
/// 0x800B07E0 (FUN_80068C6C's next projectile slot) at that load.
pub const ORACLE_NEXT_PROJECTILE: u8 = 1;
/// The power-on history of the oracle's title load put into a new session:
/// the blink timers, the projectile table and its next slot (the water: ORACLE_WATER; the
/// round robin: ORACLE_ROUND_ROBIN in the config).
pub fn oracle_history(g: &mut crate::globals::RaceGlobals) {
    g.items.blink = ORACLE_BLINK;
    g.items.next_projectile = ORACLE_NEXT_PROJECTILE;
    for (i, p) in g.items.projectiles.iter_mut().enumerate() {
        *p = crate::items::Projectile::read(&ORACLE_PROJECTILES[0x48 * i..0x48 * (i + 1)]);
    }
}
/// The BIOS rand() seed at that load (VB 369..405, analysis/s6/title/seed).
pub const ORACLE_SEED: u32 = 0xF831_0C8E;

/// Phase 8: the attract demo starts after this many idle VBlanks on the
/// title / main menu (FUN_80074A94: game mode 0x10 and 0x800B0AF0 > 0x707;
/// FUN_80073138 zeroes the counter on each menu input, FUN_8005C2EC adds dt).
pub const DEMO_IDLE: i32 = 0x708;
/// 0x800B0A1C..0x800B0A1E: the last three demo courses (the executable's
/// initial bytes: none).
pub const DEMO_RECENT_INIT: [u8; 3] = [0xFF; 3];
/// The world / course table (9 worlds x 4 slots, -1 = none).
pub const WORLD_COURSES: u32 = 0x800A_C1E4;

/// FUN_80089CA8: the attract demo's course. Two random bytes per try (the
/// pointer 0x800B0BCC, no wrap): world = b0 % 7, slot = b1 & 3; worlds 0..5
/// skip slot 3 (the stunt courses), world 6 needs its slot open (save flag
/// 1; `world6_open`); the course (0x800AC1E4[slot + 4 world]) must not be
/// one of the last three demos, which it then joins [CODE 80089ca8]
/// (oracle: 17 from 0x8001FC92 after 1, 15, 21; analysis/oracle/attract).
pub fn demo_course(w: &World, rng: &mut u32, world6_open: [bool; 4], recent: &mut [u8; 3]) -> u8 {
    loop {
        let (b0, b1) = (w.rng_byte(*rng), w.rng_byte(rng.wrapping_add(1)));
        *rng = rng.wrapping_add(2);
        let (world, slot) = ((b0 % 7) as u32, (b1 & 3) as usize);
        if (world < 6 && slot == 3) || (world == 6 && !world6_open[slot]) {
            continue;
        }
        let c = w.exe.u8(WORLD_COURSES + slot as u32 + world * 4);
        if c != 0xFF && !recent.contains(&c) {
            *recent = [c, recent[0], recent[1]];
            return c;
        }
    }
}

/// THE END (course 0x1D): the course whose music plays (FUN_80089B98, the
/// track of FUN_8008C4AC @0x8008D2xx for course 0x1D and game mode 0xE).
/// Two random bytes per try: world = b0 % 6, slot = b1 & 3, the slot open
/// (`open(world, slot)`), its course's track (0x800ACB37 + 0xC * course)
/// not -1 nor one of the last three (0x800B0A18..1A), which it joins
/// [CODE 80089b98].
pub fn credits_track(w: &World, rng: &mut u32, open: &dyn Fn(usize, usize) -> bool, recent: &mut [i8; 3]) -> i8 {
    loop {
        let (b0, b1) = (w.rng_byte(*rng), w.rng_byte(rng.wrapping_add(1)));
        *rng = rng.wrapping_add(2);
        let (world, slot) = ((b0 % 6) as u32, (b1 & 3) as usize);
        if !open(world as usize, slot) {
            continue;
        }
        let c = w.exe.u8(WORLD_COURSES + slot as u32 + world * 4) as i8;
        if c == -1 {
            continue;
        }
        let t = w.exe.u8(0x800A_CB37 + 0xC * c as u32) as i8;
        if t != -1 && !recent.contains(&t) {
            *recent = [t, recent[0], recent[1]];
            return t;
        }
    }
}

/// 0x800C3028[c] != -1: the characters whose models the boot loaded
/// (0..24 on the oracle, title and race alike) [RAM].
pub fn character_loaded(c: u8) -> bool {
    c < 25
}

/// 0x800D1560[v] != -1: the vehicles loaded (0..18 and 25..30) [RAM].
pub fn vehicle_loaded(v: u8) -> bool {
    v <= 18 || (25..=30).contains(&v)
}

/// Every vehicle's template (wheels placed from its model) and every
/// character's part-0 pivot, for re-creating karts.
pub struct TitleData {
    pub templates: Vec<Option<[u8; TEMPLATE_SIZE]>>,
    pub vehicle_models: Vec<Option<String>>,
    pub characters: Vec<Option<([i32; 3], String)>>,
    /// Every kart's character +0x4B2 (FUN_8008B258 avoids them all).
    pub chars: std::sync::Mutex<Vec<u8>>,
    /// A kart re-created during this tick's kart loop: the session swaps
    /// its record and models in after the kart's update.
    pub pending: std::sync::Mutex<Option<Respawned>>,
    /// The logo letters' bounce (FUN_80091B8C; `Letters`).
    pub letters: std::sync::Mutex<Letters>,
}

impl Clone for TitleData {
    fn clone(&self) -> Self {
        TitleData {
            templates: self.templates.clone(),
            vehicle_models: self.vehicle_models.clone(),
            characters: self.characters.clone(),
            chars: std::sync::Mutex::new(self.chars.lock().unwrap().clone()),
            pending: std::sync::Mutex::new(self.pending.lock().unwrap().clone()),
            letters: std::sync::Mutex::new(self.letters.lock().unwrap().clone()),
        }
    }
}

impl TitleData {
    pub fn load(disc: &mut mr_disc::Disc, w: &World) -> Result<TitleData, String> {
        let tpl = ai::vehicle_templates(&w.exe, &w.tables).map_err(|e| e.0)?;
        let mdl = |disc: &mut mr_disc::Disc, p: &str| -> Option<mr_assets::mdl::Mdl> { mr_assets::mdl::Mdl::parse(&disc.read(p).ok()?).ok() };
        let mut templates = Vec::new();
        let mut vehicle_models = Vec::new();
        for v in 0..ai::N_VEHICLES as u8 {
            let path = ai::model_path(&w.exe, ai::addr::VEHICLE_PATHS, v, "MDL").filter(|_| vehicle_loaded(v));
            let t = path.as_ref().and_then(|p| mdl(disc, p)).map(|m| {
                let mut t = tpl[v as usize];
                ai::place_wheels(&mut t, &m);
                t
            });
            templates.push(t);
            vehicle_models.push(path);
        }
        let mut characters = Vec::new();
        for c in 0..25u8 {
            let path = ai::model_path(&w.exe, ai::addr::CHARACTER_PATHS, c, "MDL");
            characters.push(path.and_then(|p| mdl(disc, &p).map(|m| (m.parts[0].record.pivot, p))));
        }
        let dat = mr_assets::dat::Dat::parse(&disc.read(&w.info.file("DAT")).map_err(|e| format!("{e:?}"))?).map_err(|e| format!("{e:?}"))?;
        let letters = Letters::load(&dat);
        Ok(TitleData { templates, vehicle_models, characters, chars: Default::default(), pending: Default::default(), letters: std::sync::Mutex::new(letters) })
    }
}

/// A kart re-created by `respawn`.
#[derive(Clone)]
pub struct Respawned {
    pub record: Vec<u8>,
    pub kart: KartState,
    pub character: u8,
    pub vehicle: u8,
    pub pivot: [i32; 3],
    pub vehicle_model: String,
    pub character_model: String,
}

/// From the kart update's lap timing (progress.rs `knocked_out`): on the
/// title, re-create the kart in place; the session picks up the record.
pub fn respawn_in_update(k: &mut KartState, g: &mut RaceGlobals, w: &World, rand: &mut dyn FnMut() -> i32) {
    let Some(d) = w.title.as_deref() else { return };
    let slot = k.player_index as usize;
    let chars = d.chars.lock().unwrap().clone();
    if slot >= chars.len() {
        return;
    }
    let r = respawn(slot, &chars, g, w, d, rand);
    if std::env::var("MR_TITLE_DEBUG").is_ok() {
        eprintln!("title respawn: kart {slot} -> character {} vehicle {} at {:?}", r.character, r.vehicle, [r.kart.x, r.kart.y, r.kart.z]);
    }
    *k = r.kart.clone();
    d.chars.lock().unwrap()[slot] = r.character;
    *d.pending.lock().unwrap() = Some(r);
}

/// FUN_80082708's course-0x1E tail for kart `slot` just knocked out:
/// FUN_8008B258 (a loaded racing character no kart has now), its vehicle
/// (the table's, else FUN_8008B1BC), grid slot *rng % karts,
/// FUN_800609F4(kart, c, v, grid, 1), then one random byte and the
/// FUN_800870E8 puff at the new kart. `chars`: every kart's +0x4B2.
pub fn respawn(slot: usize, chars: &[u8], g: &mut RaceGlobals, w: &World, d: &TitleData, rand: &mut dyn FnMut() -> i32) -> Respawned {
    let (c, v) = ai::pick_opponent(&w.exe, chars, &character_loaded, &|v| d.templates.get(v as usize).is_some_and(|t| t.is_some()), rand);
    let b = w.rng_byte(g.rng);
    g.rng = g.rng.wrapping_add(1);
    let grid = b as i32 % chars.len() as i32;
    let (pivot, cpath) = d.characters[c as usize].clone().expect("loaded character");
    let setup = crate::init::RaceSetup { mode: TITLE_MODE, race_type: 2, race_flags: g.race_flags, challenge: 0, humans: g.humans as i16, karts: chars.len() as i16 };
    let t = d.templates[v as usize].expect("loaded vehicle");
    let (record, kart) = ai::init_opponent(w, &t, setup, Opponent { slot: slot as u8, character: c, vehicle: v, grid }, pivot[1], &mut g.rng, rand);
    let b = crate::update::rng_next(g, w) as i32;
    crate::items::particle(g, [kart.drv_x, kart.ref_y, kart.drv_z], [kart.vx, kart.vy, kart.vz], 0, b << 4, 0, 0x11, [0x80; 3], 0x180, 0x180, 0x20);
    Respawned { record, kart, character: c, vehicle: v, pivot, vehicle_model: d.vehicle_models[v as usize].clone().unwrap_or_default(), character_model: cpath }
}

/// The logo's bouncing letters (FUN_80091AA8 at the load, FUN_80091B8C each
/// title frame after the camera): 20 pairs of special slots 0x68 + 2i /
/// 0x69 + 2i (letter and its shadow), each at its object's centre x, z
/// (0x800DDD90 + 12i, world units) and a height 0x800DDD94 + 12i (0 at
/// rest, negative = up) with a speed 0x800DD6C0 + 2i. A resting letter
/// (height 0, speed < 0x40) jumps with -0x600 when a kart is within 0x4000
/// (x) / 0x8000 (z) world units, else with -0x400 for a flying projectile;
/// per VBlank the speed grows by 0x30 (at most 0x6000) and moves the
/// height; landing (height > 0) puts it back at 0 and halves the speed,
/// reversed. Objects follow at (x, height, z) >> 5 (FUN_8002DF88).
/// [CODE 80091aa8, 80091b8c] [RAM: positions = the objects' centres]
#[derive(Clone, Debug, Default)]
pub struct Letters {
    /// Per letter: (x, z) world units, height, speed.
    pub xz: Vec<[i32; 2]>,
    pub height: Vec<i32>,
    pub speed: Vec<i16>,
    /// The DAT objects of the two special slots (None: empty slot).
    pub objects: Vec<[Option<usize>; 2]>,
}

impl Letters {
    pub const N: usize = 20;
    pub const FIRST_SLOT: usize = 0x68;

    pub fn load(dat: &mr_assets::dat::Dat) -> Letters {
        let mut l = Letters::default();
        for i in 0..Self::N {
            let o = |s: usize| dat.special.get(s).copied().flatten();
            let pair = [o(Self::FIRST_SLOT + 2 * i), o(Self::FIRST_SLOT + 2 * i + 1)];
            let xz = pair[0].map(|k| [dat.objects[k].centre[0] << 5, dat.objects[k].centre[2] << 5]).unwrap_or([0, 0]);
            l.xz.push(xz);
            l.height.push(0);
            l.speed.push(0);
            l.objects.push(pair);
        }
        l
    }

    /// FUN_80091B8C, mode 0x10 branch. `karts`: every kart's (x, z) world
    /// units; `projectiles`: the flying ones' (x, z).
    pub fn frame(&mut self, karts: &[[i32; 2]], projectiles: &[[i32; 2]], dt: i32) {
        let near = |a: [i32; 2], b: [i32; 2]| a[0].wrapping_sub(b[0]).unsigned_abs() < 0x4000 && a[1].wrapping_sub(b[1]).unsigned_abs() < 0x8000;
        for i in 0..self.xz.len() {
            if self.height[i] == 0 && self.speed[i] < 0x40 {
                if karts.iter().any(|&k| near(k, self.xz[i])) {
                    self.speed[i] = -0x600;
                } else if projectiles.iter().any(|&p| near(p, self.xz[i])) {
                    self.speed[i] = 0xFC00u16 as i16;
                }
            }
            for _ in 0..dt.max(1) {
                let v = self.speed[i].wrapping_add(0x30);
                self.speed[i] = if v > 0x6000 { 0x6000 } else { v };
                let y = self.height[i].wrapping_add(self.speed[i] as i32);
                self.height[i] = y;
                if y > 0 {
                    self.height[i] = 0;
                    self.speed[i] = (-(self.speed[i] as i32) / 2) as i16;
                }
            }
        }
    }

    /// The objects' y offsets now (DAT units, `height >> 5`), per object.
    pub fn offsets(&self) -> Vec<(usize, i32)> {
        let mut out = Vec::new();
        for (i, pair) in self.objects.iter().enumerate() {
            for o in pair.iter().flatten() {
                out.push((*o, self.height[i] >> 5));
            }
        }
        out
    }
}

/// Row 12: the course select's scenery (game mode 0xC on course 0x1E).
///
/// FUN_80091B8C, mode 0xC branch, each frame (`dt` VBlanks): angles a24 /
/// a26 / a28 (0x800B0A24..28) += dt * (0x23, 0x19, 0x2D); the column
/// position a2c (0x800B0A2C) eases toward cursor column * 0x10000 by 1/8
/// (C division); per world w (y = w * 10000 - 25000, DAT units) whose row
/// is within 0x4000 of the eye's y (eye >> 5): the world's props (special
/// slots 0xA0 + 2w, 0xA1 + 2w) at scale 0x1000 turned about y by (frame &
/// 0x3FF) << 2, else scale 0; per course column c: its scale (0x800AD0D4 +
/// 8w + 2c, 0x200 at power-on) eases dt times by 1/16 toward 0xE00 on the
/// cursor, 0x300 elsewhere (used as 0 when the row is far); an open
/// course's pair of slots (1 + 2(4w + c), + 1) is scaled, turned (0x100,
/// a26, sin(a28) >> 7) and placed at ((0xA000 - a2c) >> 5 + c * 0x800, y,
/// -1988); a closed one hides that pair and shows the next "?" slot (0x39 +
/// k) there, turned (0x100, sin(a26) >> 6, sin(a28) >> 7); the unused "?"
/// slots get scale 0. [CODE 80091b8c]
///
/// FUN_80035004 / FUN_800351E8 (grid type 2, course 0x1E, drawn in game
/// mode 0xC only): the backdrop, a 10 x 10 vertex grid in view space (x =
/// -1080 + 240c, y = -864 + 192r, z = 1000 - (v >> 8), v = (sin(p0 + 0x179
/// r) * sin(p1 + 500 c)) >> 14 (raw table), phase p += (0x23, 0x2A) a
/// frame), texture slot 1 (u, v = 28c, 28r; 27 texels a cell), colour (s >>
/// 2, s >> 1, s) with s = 0xA0 - |v >> 7|, under everything (ordering table
/// 1400) [CODE 80034ce0, 80035004, 800351e8]. On screen: sx = 256 + 192 x /
/// z, sy = 120 + 160 y / z (the course select's view matrix has row 0 =
/// 1.2 [GPU: the oracle's 100 vertices within 1 pixel]).
#[derive(Clone, Debug)]
pub struct CourseSelect {
    pub a: [u16; 3],
    pub a2c: i32,
    pub scale: [[i16; 4]; 7],
    /// The backdrop grid's phase (0x800AFEE4 / 0x800AFEE6).
    pub grid: (u16, u16),
}

/// One special slot set by the course select this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlotSet {
    pub slot: u16,
    /// FUN_8002D72C's scale (same on the three axes; 0 = hidden).
    pub scale: i32,
    /// FUN_8002D9E4's angles (added to the load pose's).
    pub rot: [i32; 3],
    /// FUN_8002DF88's origin (DAT units), when placed.
    pub pos: Option<[i32; 3]>,
}

/// One backdrop grid vertex: view-space position and the shade.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridVertex {
    pub pos: [i32; 3],
    pub shade: u8,
}

impl CourseSelect {
    pub const SCALES: u32 = 0x800A_D0D4;
    pub const COLS: usize = 10;
    pub const ROWS: usize = 10;

    pub fn new(exe: &crate::tables::ExeImage) -> CourseSelect {
        let mut scale = [[0x200i16; 4]; 7];
        for (w, row) in scale.iter_mut().enumerate() {
            for (c, s) in row.iter_mut().enumerate() {
                *s = exe.i16(Self::SCALES + 8 * w as u32 + 2 * c as u32);
            }
        }
        CourseSelect { a: [0; 3], a2c: 0, scale, grid: (0, 0) }
    }

    /// One frame: `cursor` (world, column), `eye_y` (the camera eye's y,
    /// world units), `frame` (0x800B0D90), `open(w, c)` (save flags & 1).
    pub fn frame(&mut self, t: &crate::tables::Tables, cursor: (i8, i8), dt: i32, eye_y: i32, frame: i32, open: &dyn Fn(usize, usize) -> bool) -> Vec<SlotSet> {
        let toward = |v: i32, s: u32| if v < 0 { (v + (1 << s) - 1) >> s } else { v >> s };
        let d = cursor.1 as i32 * 0x10000 - self.a2c;
        for (k, step) in [0x23i32, 0x19, 0x2D].iter().enumerate() {
            self.a[k] = ((self.a[k] as i32 + dt * step) & 0xFFF) as u16;
        }
        self.a2c += toward(d, 3);
        self.grid = ((self.grid.0 + 0x23) & 0xFFF, (self.grid.1 + 0x2A) & 0xFFF);
        let sinr = |a: u16| t.sin_raw(a as i32 & 0xFFF);
        let eye = eye_y >> 5;
        let mut out = Vec::new();
        let mut q = 0u16;
        let mut slot = 1u16;
        let mut y = -25000;
        for w in 0..7usize {
            let near = (y - eye).abs() < 0x4001;
            for k in 0..2 {
                let s = 0xA0 + 2 * w as u16 + k;
                let (scale, rot) = if near { (0x1000, [0, (frame & 0x3FF) << 2, 0]) } else { (0, [0; 3]) };
                out.push(SlotSet { slot: s, scale, rot, pos: None });
            }
            let wob = toward(sinr(self.a[2]), 7);
            let mut x = (0xA000 - self.a2c) >> 5;
            for c in 0..4usize {
                let target = if w as i8 == cursor.0 && c as i8 == cursor.1 { 0xE00 } else { 0x300 };
                for _ in 0..dt.max(1) {
                    let v = self.scale[w][c] as i32;
                    self.scale[w][c] = (v + toward(target - v, 4)) as i16;
                }
                let s = if near { self.scale[w][c] as i32 } else { 0 };
                let pos = Some([x, y, -1988]);
                if !open(w, c) {
                    out.push(SlotSet { slot, scale: 0, rot: [0; 3], pos: None });
                    out.push(SlotSet { slot: slot + 1, scale: 0, rot: [0; 3], pos: None });
                    out.push(SlotSet { slot: 0x39 + q, scale: s, rot: [0x100, toward(sinr(self.a[1]), 6), wob], pos });
                    q += 1;
                } else {
                    for o in [slot, slot + 1] {
                        out.push(SlotSet { slot: o, scale: s, rot: [0x100, self.a[1] as i32, wob], pos });
                    }
                }
                slot += 2;
                x += 0x800;
            }
            y += 10000;
        }
        while q < 0x16 {
            out.push(SlotSet { slot: 0x39 + q, scale: 0, rot: [0; 3], pos: None });
            q += 1;
        }
        out
    }

    /// The backdrop grid now (FUN_80035004 type 2), row-major.
    pub fn backdrop(&self, t: &crate::tables::Tables) -> Vec<GridVertex> {
        let toward = |v: i32, s: u32| if v < 0 { (v + (1 << s) - 1) >> s } else { v >> s };
        let mut out = Vec::with_capacity(Self::COLS * Self::ROWS);
        for r in 0..Self::ROWS as i32 {
            let u = (self.grid.0 as i32 + 0x179 * r) & 0xFFF;
            for c in 0..Self::COLS as i32 {
                let w = (self.grid.1 as i32 + 500 * c) & 0xFFF;
                let v = toward(t.sin_raw(u) * t.sin_raw(w), 14) as i16 as i32;
                out.push(GridVertex {
                    pos: [(240 - 10 * 240) / 2 + 240 * c, (192 - 10 * 192) / 2 + 192 * r, 1000 - toward(v, 8)],
                    shade: (0xA0 - toward(v, 7).abs()) as u8,
                });
            }
        }
        out
    }
}

#[cfg(test)]
mod course_select_tests {
    use super::*;

    #[test]
    fn backdrop_matches_oracle_frame() {
        let Ok(mut d) = mr_disc::Disc::open_default() else { return };
        let exe = crate::tables::ExeImage::parse(&d.read("SLUS_012.37").unwrap()).unwrap();
        let t = crate::tables::Tables::from_exe(&exe);
        let mut cs = CourseSelect::new(&exe);
        // the oracle's course select frame (analysis/s7/menus/oracle
        // worlds.json): phase (3904, 588); vertex (0, 0) at (51, -17) with
        // shade 131, (9, 9) at (451, 250) with 35 (GP0)
        cs.grid = (3904, 588);
        let g = cs.backdrop(&t);
        let scr = |v: &GridVertex| (256 + 192 * v.pos[0] / v.pos[2], 120 + 160 * v.pos[1] / v.pos[2]);
        assert_eq!(g[0].shade, 131);
        assert_eq!(g[99].shade, 35);
        let (x, y) = scr(&g[0]);
        assert!((x - 51).abs() <= 1 && (y + 17).abs() <= 1, "{x} {y}");
        let (x, y) = scr(&g[99]);
        assert!((x - 451).abs() <= 1 && (y - 250).abs() <= 1, "{x} {y}");
        // the cursor's slot grows toward 0xE00, the others toward 0x300
        let sets = cs.frame(&t, (0, 0), 2, -25000 * 32, 0, &|_, _| true);
        let s1 = sets.iter().find(|s| s.slot == 1).unwrap();
        assert!(s1.scale > 0x200 && s1.pos.is_some());
        let s3 = sets.iter().find(|s| s.slot == 3).unwrap();
        assert!(s3.scale > 0x200 && s3.scale < s1.scale);
    }
}

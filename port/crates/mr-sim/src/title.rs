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
}

impl Clone for TitleData {
    fn clone(&self) -> Self {
        TitleData {
            templates: self.templates.clone(),
            vehicle_models: self.vehicle_models.clone(),
            characters: self.characters.clone(),
            chars: std::sync::Mutex::new(self.chars.lock().unwrap().clone()),
            pending: std::sync::Mutex::new(self.pending.lock().unwrap().clone()),
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
        Ok(TitleData { templates, vehicle_models, characters, chars: Default::default(), pending: Default::default() })
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

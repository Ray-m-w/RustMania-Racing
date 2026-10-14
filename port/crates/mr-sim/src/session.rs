//! A standalone one-player DOCK race: everything the oracle used to supply
//! is produced here from the disc and the ported game code. No captured RAM
//! is read.
//!
//! ```ignore
//! let mut disc = mr_disc::Disc::open_default()?;
//! let mut s = mr_sim::session::Session::new(&mut disc)?;
//! loop {
//!     s.tick(pad_word)?;           // one game tick (dt = 2 VBlanks)
//!     let k = s.kart();            // KartState: position, angles, ...
//!     let m = s.kart().body;       // body matrix, 4.12
//!     let v = s.camera_view();     // camera.rs view (eye, matrix, ...)
//! }
//! ```
//!
//! The session starts at race load (countdown 480: the intro fly-by, then the
//! "Copy the buttons" start game below 300, GO at 0) and runs to the finish
//! and beyond. Race state for a HUD: `phase()` (race::RacePhase),
//! `countdown()`, `race_clock()`, `position()`, `laps_done()`, `results()`
//! (race::KartResult per kart), and the kart record (`kart()`: lap times
//! +0x35C.., best lap, shown position +0x4CA and its flash +0x4FF, the start
//! boost +0x446 and copy button +0x4DA).
//!
//! Per tick, in the game's order:
//! 0. FUN_8005C2EC: frame counter, countdown / GO / race clock (race.rs);
//!    FUN_8007916C's results switch (all humans finished -> mode 10);
//! 1. FUN_8003C5FC water animation (normals, random draws) — water.rs;
//! 2. FUN_800418D8 contact pass over `others`, then the look-at target
//!    (FUN_80041A20 / FUN_80041CC4) — collide.rs, render.rs;
//! 3. FUN_8009004C path position for the round-robin kart;
//! 4. the kart loop, slot order: AI karts (and the finished player) get
//!    FUN_800912F8's buttons (ai.rs, items off), the human the pad;
//!    FUN_80064B5C for every kart, FUN_80086B7C for the human;
//! 5. FUN_80086DF0, FUN_800837F8 (race progress, positions), FUN_80025D8C
//!    (body, vehicle, driver matrices, look point, wheels, lights);
//! 6. FUN_8007D304's random-pointer step (wraps in 0x8001F94C..0x8001FF28);
//! 7. FUN_80070418 camera — camera.rs (intro mode 10, chase mode 0,
//!    trackside mode 2 after the finish).
//! Not produced (see the ledger): track tokens that fill the turbo meter
//! (FUN_8007BBF0), items.

use mr_assets::mdl::Mdl;

use crate::camera::{self, CameraGlobals, CameraGround, CameraInput, CameraKart, CameraState, CameraTables, CameraView};
use crate::globals::RaceGlobals;
use crate::init::{init_kart, KartInit, RaceSetup};
use crate::kart::KartState;
use crate::water::WaterAnim;
use crate::world::{TickInputs, World, WorldError};

/// Settings of the race a session runs (front-end choices and saved
/// records). `dock_default` has the oracle savestate's values.
#[derive(Clone, Copy, Debug)]
pub struct SessionConfig {
    /// Course index (0x800B0FAC; course::CourseInfo): 3 = DOCK.
    pub course: u8,
    /// Grid position (0-based): 5 = the slot the player had in the oracle
    /// race (6th of 6).
    pub grid: i32,
    /// Character (+0x4B2): 0 = Kermit.
    pub character: u8,
    /// 0x800B0DA6: laps.
    pub laps: i16,
    /// 0x800E0392: alternate button layout (brake SQUARE, jump CIRCLE).
    pub alt_controls: bool,
    /// 0x800B0FAC event number of the race (3 on DOCK).
    pub event: i32,
    /// 0x800B0E38 (difficulty / cup byte; the camera reads it).
    pub cup: i8,
    /// 0x800E0190.. course lap record (the lap-end code compares with it).
    pub lap_record: i32,
    /// BIOS rand() seed at race load (0x80009010; boot history decides it
    /// in the game): opponents, wheel angles, AI decisions, camera.
    pub rand_seed: u32,
    /// AI opponents (0x800B0DFE - 1): 5 in a 1-player race, 0 for a solo
    /// session (analysis).
    pub opponents: u8,
    /// 0x800E039A: AI catch-up scale /16 (8 in the oracle's race).
    pub difficulty: u8,
    /// The random byte pointer 0x800B0BCC at race load.
    pub rng_start: u32,
    /// 0x800C3058[karts] at race load: the first kart the path / progress
    /// round robin serves (1 in the oracle's race load).
    pub round_robin_start: u8,
    /// Analysis: a fixed line-up of opponents, (character, vehicle) for
    /// slots 1.., instead of the race loader's random draws; the player then
    /// starts on `grid` (no grid draw). E.g. `ORACLE_LINEUP`.
    pub lineup: Option<[(u8, u8); 5]>,
    /// Items, weapons and specials on (the game's race). Off reproduces the
    /// Session 3 items-off runs: the AI's item buttons are masked
    /// (`ai::items_off`) and nobody collects pickups.
    pub items: bool,
    /// Session 5: logic rate, 30 (the game: dt = 2 VBlanks per tick) or 60
    /// (dt = 1, the 60 Hz mode; docs/spec/kart-physics-ledger.md "60 Hz
    /// mode"). `tick` advances 1/30 s or 1/60 s of game time.
    pub logic_hz: u8,
}

/// The opponents of the oracle's slot-2 DOCK race (analysis/ai/*): Clifford
/// on the TRIKE, Bobo W_BUGGY, Sal FLAME, Beaker M_BUGGY, Chef HOTDOG.
pub const ORACLE_LINEUP: [(u8, u8); 5] = [(21, 26), (14, 13), (23, 18), (3, 7), (10, 9)];

impl SessionConfig {
    /// The settings of the oracle's DOCK race, read once from that
    /// savestate's RAM and kept here as race settings [RAM]: laps
    /// (0x800B0DA6) 3, event (0x800B0FAC) 3, cup (0x800B0E38) 4, DOCK lap
    /// record (0x800E0190 + 0x20 * 4) 12186 (= 0:40.62 at 300 units/s).
    pub fn dock_default() -> Self {
        SessionConfig {
            course: crate::course::DOCK,
            grid: 5,
            character: 0,
            laps: 3,
            alt_controls: false,
            event: 3,
            cup: 4,
            lap_record: 12186,
            // The oracle's state just before its race load from the course
            // menu (analysis/oracle/start_probe): opponents Floyd, Rizzo,
            // Gonzo, Johnny, Clifford; the player 6th on the grid.
            rand_seed: 0x832B_2260,
            opponents: 5,
            difficulty: 8,
            rng_start: 0x8001_FC4E,
            round_robin_start: 1,
            lineup: None,
            items: true,
            logic_hz: 30,
        }
    }

    /// A 1-player race on course `info` from the menus with the game's
    /// boot-time settings (FUN_8008F448: laps 3, difficulty 8, the default
    /// lap record) and the race loader's set-up for the course
    /// (FUN_8008AB5C cup / slot, FUN_8008C4AC karts: 6, or 5 above cup 6).
    /// The rand() seed and the random pointer are DOCK's oracle values
    /// (any values give a valid race).
    pub fn course(info: &crate::course::CourseInfo) -> Self {
        SessionConfig {
            course: info.id,
            event: info.id as i32,
            cup: info.cup,
            laps: 3,
            lap_record: info.lap_record,
            opponents: (info.karts - 1).max(0) as u8,
            ..Self::dock_default()
        }
    }

    /// `dock_default` without opponents: the player alone (analysis; the
    /// oracle's grid slot 5).
    pub fn dock_solo() -> Self {
        SessionConfig { opponents: 0, rand_seed: 1, rng_start: RNG_START, ..Self::dock_default() }
    }
}

/// The random pointer's reset value at race load (FUN_8008ACE4 @0x8008AD94).
pub const RNG_START: u32 = 0x8001_F94C;
/// FUN_8007D304: after its per-frame step the pointer wraps back by 0x5DB
/// once it passes this address.
pub const RNG_WRAP: u32 = 0x8001_FF28;

pub struct Session {
    pub world: World,
    pub kart: KartState,
    /// The full 0x504-byte record (fields mr-sim does not model keep the
    /// values the race-load code gave them).
    pub record: Vec<u8>,
    pub globals: RaceGlobals,
    pub water: WaterAnim,
    pub camera: CameraState,
    pub camera_tables: CameraTables,
    pub camera_globals: CameraGlobals,
    pub ticks: u64,
    /// Analysis only: race progress the catch-up code treats as the
    /// leader's (None: the player leads, as in a race with no AI).
    pub leader_progress_override: Option<i32>,
    /// Analysis only: the random pointer to use for the next tick, set
    /// after the water animation (the game's pointer is shared with the AI).
    pub rng_override: Option<u32>,
    /// Analysis only: the player's race position after the position pass
    /// (e.g. 6, the oracle race's, so the catch-up runs as it did there).
    pub race_pos_override: Option<u8>,
    /// Analysis / attract: the player's buttons come from the AI driver
    /// (FUN_800912F8 on the player's kart, items off) instead of the pad.
    pub autopilot: bool,
    /// Karts 1.. (the AI opponents), slot order; each with its full record.
    pub others: Vec<OtherKart>,
    /// The AI fields of the player's record (+0x3E8.., +0x4A0): used once
    /// the game hands the finished player's kart to the AI.
    pub player_ai: crate::ai::AiState,
    /// 0x800E02BA: the best start boost of the race (start game).
    pub best_start: u8,
    /// 0x800C3058[n]: the kart FUN_8009004C / FUN_800837F8 serve this
    /// frame (round robin over the karts).
    pub pick: usize,
    /// The first race frame after the race load runs with the countdown
    /// held at 480 (oracle: kart updated once, countdown unchanged).
    pub load_frame: bool,
    /// Analysis only: for the next tick, step the random pointer by this
    /// many bytes in place of FUN_8007F03C (kart particles); None runs it.
    pub rng_skip_items: Option<u32>,
    /// The first tick after the race load: the camera's change to the intro
    /// mode takes its side bit from the next rand() without drawing it
    /// (oracle start_probe: drawing it puts every later AI decision one
    /// rand() off; the game enters the intro during the load) [INFER].
    pub camera_peek: bool,
    /// The BIOS rand() seed 0x80009010 (rand_seed() reads it).
    pub rand_state: u32,
    cfg: SessionConfig,
}

/// Everything a session changes as it runs (all but the world and the
/// exe's tables): `Session::snapshot` / `restore`. Used to look a tick
/// ahead without keeping it (mr-bevy's display prediction) and to start a
/// 60 Hz session from a 30 Hz one (examples/feel60.rs).
#[derive(Clone)]
pub struct Snapshot {
    kart: KartState,
    record: Vec<u8>,
    globals: RaceGlobals,
    water: WaterAnim,
    camera: CameraState,
    camera_globals: CameraGlobals,
    ticks: u64,
    leader_progress_override: Option<i32>,
    rng_override: Option<u32>,
    race_pos_override: Option<u8>,
    autopilot: bool,
    others: Vec<OtherKart>,
    player_ai: crate::ai::AiState,
    best_start: u8,
    pick: usize,
    load_frame: bool,
    rng_skip_items: Option<u32>,
    camera_peek: bool,
    rand_state: u32,
    cfg: SessionConfig,
}

impl Snapshot {
    /// The player's kart in the snapshot.
    pub fn kart(&self) -> &KartState {
        &self.kart
    }

    /// Session ticks at the snapshot.
    pub fn ticks(&self) -> u64 {
        self.ticks
    }
}

/// A kart other than the player's (slot 1..): an AI opponent.
#[derive(Clone)]
pub struct OtherKart {
    pub kart: KartState,
    /// The full 0x504-byte record after the last tick's render pass.
    pub record: Vec<u8>,
    /// AI fields of the record (+0x3E8.., +0x4A0, +0x4A2).
    pub ai: crate::ai::AiState,
    /// +0x4B2 character and +0x4B1 vehicle (exe table indices).
    pub character: u8,
    pub vehicle: u8,
    /// The character model's part 0 pivot (driver placement).
    pub driver: crate::render::DriverModel,
    /// Disc paths of the vehicle and character models ("VEHIC_A/TRIKE.MDL").
    pub vehicle_model: String,
    pub character_model: String,
}

#[derive(Debug)]
pub enum SessionError {
    World(WorldError),
    Disc(String),
    /// The game would take a path outside the port (items, race end, ...);
    /// the session state is left as it was before the tick.
    OutOfScope(String),
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionError::World(e) => write!(f, "world: {e}"),
            SessionError::Disc(e) => write!(f, "disc: {e}"),
            SessionError::OutOfScope(e) => write!(f, "out of scope: {e}"),
        }
    }
}

impl std::error::Error for SessionError {}

/// What the camera needs from the session: the TER ground query and path,
/// BIOS rand() and the random byte pointer 0x800B0BCC.
struct SessionCameraEnv<'a> {
    peek_rand: bool,
    world: &'a World,
    rand_state: &'a mut u32,
    rng: &'a mut u32,
}

impl camera::CameraEnv for SessionCameraEnv<'_> {
    fn ground(&mut self, x: i32, y: i32, z: i32) -> CameraGround {
        let q = self.world.ter.ground(x, y, z);
        CameraGround {
            found: q.nearest.is_some(),
            nearest_y: q.nearest_y,
            below_y: q.below.map(|s| s.y).unwrap_or(q.nearest_y),
            // 0x800B0AE0 falls back to the nearest surface (0x8003E34C).
            below_found: q.below.is_some() || q.nearest.is_some(),
            alt_found: q.alt.is_some(),
            alt_y: q.alt.map(|s| s.y).unwrap_or(0),
            type3_min_y: q.water_y,
        }
    }

    fn rand(&mut self) -> i32 {
        if self.peek_rand {
            // See `Session::camera_peek`.
            let mut s = *self.rand_state;
            return bios_rand(&mut s);
        }
        bios_rand(self.rand_state)
    }

    fn walk(&mut self, seg: i16, pt: i16, n: i16, backward: bool) -> (i16, i16) {
        let seed = &mut *self.rand_state;
        let mut rand = || bios_rand(seed);
        let wk = if backward {
            crate::ai::walk_backward(self.world, seg, pt, n, &mut rand)
        } else {
            crate::ai::walk_forward(self.world, seg, pt, n, false, &mut rand)
        };
        (wk.seg, wk.pt)
    }

    fn path_point(&mut self, seg: i16, pt: i16) -> [i32; 3] {
        let ter = &self.world.ter;
        let first = ter.path_segments[seg as u16 as usize].first;
        let p = &ter.path_points[first.wrapping_add(pt) as u16 as usize];
        [p.x, p.y, p.z]
    }

    fn rng_byte(&mut self) -> u8 {
        let b = self.world.rng_byte(*self.rng);
        *self.rng = self.rng.wrapping_add(1);
        b
    }
}

/// BIOS rand() (A0:2F, SCPH-1001 @0xBFC02200): seed = seed * 0x41C64E6D +
/// 0x3039; result (seed >> 16) & 0x7FFF.
pub fn bios_rand(seed: &mut u32) -> i32 {
    *seed = seed.wrapping_mul(0x41C6_4E6D).wrapping_add(0x3039);
    ((*seed >> 16) & 0x7FFF) as i32
}

/// The seed `n` BIOS rand() calls before `seed` (the LCG run backwards:
/// the multiplier 0x41C64E6D is odd, so it has an inverse mod 2^32).
pub fn bios_rand_rewind(seed: u32, n: u32) -> u32 {
    let a: u32 = 0x41C6_4E6D;
    let mut inv: u32 = a;
    for _ in 0..5 {
        inv = inv.wrapping_mul(2u32.wrapping_sub(a.wrapping_mul(inv)));
    }
    let mut s = seed;
    for _ in 0..n {
        s = s.wrapping_sub(0x3039).wrapping_mul(inv);
    }
    s
}

/// FUN_800912F8 for kart `slot` (`f` = the AI's button choice). 60 Hz mode:
/// the AI decides on the first tick of a pair, with dt = 2 (its timers and
/// +0x4CB count the pair's VBlanks), and the buttons hold for the second
/// tick, so its decisions and random draws keep the game's rate. 30 Hz: `f`.
fn ai_tick(g: &mut RaceGlobals, slot: usize, f: impl FnOnce(&mut RaceGlobals) -> u16) -> u16 {
    if !g.hz60.on {
        return f(g);
    }
    let i = slot.min(7);
    if g.hz60.beat() {
        let dt = g.dt;
        g.dt = 2;
        let b = f(g);
        g.dt = dt;
        g.hz60.ai_hold[i] = b;
        b
    } else {
        g.hz60.ai_hold[i]
    }
}

impl Session {
    /// DOCK, Kermit in the RACER against five AI opponents.
    pub fn new(disc: &mut mr_disc::Disc) -> Result<Session, SessionError> {
        Self::with_config(disc, SessionConfig::dock_default())
    }

    /// The player alone on DOCK (no opponents; analysis).
    pub fn solo(disc: &mut mr_disc::Disc) -> Result<Session, SessionError> {
        Self::with_config(disc, SessionConfig::dock_solo())
    }

    pub fn with_config(disc: &mut mr_disc::Disc, cfg: SessionConfig) -> Result<Session, SessionError> {
        let world = World::course(disc, cfg.course).map_err(SessionError::World)?;
        let read_mdl = |disc: &mut mr_disc::Disc, p: &str| -> Result<Mdl, SessionError> {
            let b = disc.read(p).map_err(|e| SessionError::Disc(format!("{p}: {e:?}")))?;
            Mdl::parse(&b).map_err(|e| SessionError::Disc(format!("{p}: {e:?}")))
        };
        let racer = read_mdl(disc, "VEHIC_A/RACER.MDL")?;
        if cfg.character != 0 {
            return Err(SessionError::OutOfScope("only Kermit (character 0) is wired up".into()));
        }
        let kermit = read_mdl(disc, "CHARS_A/KERMIT.MDL")?;
        let exe = disc.read("SLUS_012.37").map_err(|e| SessionError::Disc(format!("{e:?}")))?;
        let camera_tables = CameraTables::from_exe(&exe).ok_or_else(|| SessionError::Disc("camera tables".into()))?;
        let template = crate::vehicle::racer_template(&world.tables, &racer);
        let n = 1 + cfg.opponents as usize;
        let race_type = world.info.race_type;
        if world.info.mode != 1 {
            return Err(SessionError::OutOfScope(format!("course {} is not a race course (mode {})", world.info.name, world.info.mode)));
        }
        let setup = RaceSetup { mode: 1, race_type, race_flags: 0, challenge: 0, humans: 1, karts: n as i16 };
        let mut rand_state = cfg.rand_seed;
        let mut globals = RaceGlobals {
            dt: if cfg.logic_hz == 60 { 1 } else { 2 },
            mode: 1,
            race_clock: 0,
            // FUN_8008C4AC @0x8008DA4C: 300 + 180 VBlanks to GO.
            countdown: crate::race::COUNTDOWN_START,
            countdown_len: 300,
            race_flags: 0,
            challenge: 0,
            race_type,
            race_over: 0,
            game_state: cfg.event,
            split_screen: 0,
            humans: 1,
            laps: cfg.laps,
            frame: 0,
            rng: cfg.rng_start,
            alt_controls: [cfg.alt_controls as u8, 0],
            sound_event: 0,
            // 0x800B0CD8 is 1 during a race (finishing order counter).
            end_mode: 1,
            lap_threshold: 0,
            lap_record: cfg.lap_record,
            karts: n as i16,
            field: [(0, 0); 8],
            difficulty: cfg.difficulty,
            cup: cfg.cup,
            start_best_mask: 0,
            sound: Default::default(),
            items: Default::default(),
            stack: Default::default(),
            fx: Some(Box::new(crate::items::Fx { particles: crate::particles::Particles::from_exe(&exe), sparks: Default::default(), flats: Default::default() })),
            // DOCK's race setup: no rain (0x800B0B9A = 0, 0x800B07E2 = 0).
            kart_fx_rain: false,
            hz60: crate::globals::Hz60 { on: cfg.logic_hz == 60, ..Default::default() },
        };
        // FUN_8008C4AC: the first human's grid slot (*rng++ % karts), the
        // other karts on the following slots; with no opponents the
        // configured slot.
        let grid0 = if n > 1 && cfg.lineup.is_none() { crate::ai::grid_start(&mut globals, &world, n as i16) } else { cfg.grid };
        let (mut record, mut kart) = {
            let mut rand = || bios_rand(&mut rand_state);
            init_kart(
                &KartInit { world: &world, template: &template, setup, slot: 0, character: cfg.character, vehicle: 0, grid: grid0, flags: 0, character_pivot_y: kermit.parts[0].record.pivot[1] },
                &mut globals.rng,
                &mut rand,
            )
        };
        // The opponents: each slot's character / vehicle draw, then its
        // FUN_800609F4 (both use rand(), in this order).
        let mut others = Vec::new();
        if n > 1 {
            let tpl = crate::ai::vehicle_templates(&world.exe, &world.tables).map_err(|e| SessionError::Disc(e.0))?;
            let mut taken = vec![cfg.character];
            for s in 1..n {
                let (c, v) = match cfg.lineup {
                    Some(l) => l[(s - 1) % 5],
                    None => {
                        let mut rand = || bios_rand(&mut rand_state);
                        crate::ai::pick_opponent(&world.exe, &taken, &|_| true, &|_| true, &mut rand)
                    }
                };
                taken.push(c);
                let vpath = crate::ai::model_path(&world.exe, crate::ai::addr::VEHICLE_PATHS, v, "MDL").ok_or_else(|| SessionError::Disc(format!("vehicle {v}: no model")))?;
                let cpath = crate::ai::model_path(&world.exe, crate::ai::addr::CHARACTER_PATHS, c, "MDL").ok_or_else(|| SessionError::Disc(format!("character {c}: no model")))?;
                let mut t = tpl[v as usize];
                crate::ai::place_wheels(&mut t, &read_mdl(disc, &vpath)?);
                let cm = read_mdl(disc, &cpath)?;
                let o = crate::ai::Opponent { slot: s as u8, character: c, vehicle: v, grid: (grid0 + s as i32) % n as i32 };
                let (rec, k) = {
                    let mut rand = || bios_rand(&mut rand_state);
                    crate::ai::init_opponent(&world, &t, setup, o, cm.parts[0].record.pivot[1], &mut globals.rng, &mut rand)
                };
                others.push(OtherKart {
                    ai: crate::ai::AiState::from_record(&rec),
                    kart: k,
                    record: rec,
                    character: c,
                    vehicle: v,
                    driver: crate::render::DriverModel { part0_pivot: cm.parts[0].record.pivot },
                    vehicle_model: vpath,
                    character_model: cpath,
                });
            }
            // FUN_8009192C(&kart0, karts): every kart's special-move roll.
            let mut ks: Vec<KartState> = std::iter::once(kart.clone()).chain(others.iter().map(|o| o.kart.clone())).collect();
            crate::ai::race_load_roll(&mut ks, &mut globals, &world, n as i16);
            let mut it = ks.into_iter();
            kart = it.next().unwrap();
            for (o, k) in others.iter_mut().zip(it) {
                o.kart = k;
            }
        }
        let player_ai = crate::ai::AiState::from_record(&record);
        kart.to_record(&mut record);
        let water = WaterAnim::from_exe(&world.exe);
        // Camera record before its first tick: any mode other than 0 makes
        // the first tick a mode change, which places the camera directly.
        let mut cam_bytes = [0u8; camera::CAMERA_SIZE];
        cam_bytes[0x64..0x66].copy_from_slice(&(-1i16).to_le_bytes());
        cam_bytes[0x66..0x68].copy_from_slice(&(-1i16).to_le_bytes());
        let camera = CameraState::from_bytes(&cam_bytes);
        let camera_globals = CameraGlobals {
            dt: if cfg.logic_hz == 60 { 1 } else { 2 },
            countdown: 0,
            countdown_len: 300,
            laps: cfg.laps,
            finish_gate_a: 0,
            finish_gate_b: 0,
            game_state: 1,
            event: cfg.event,
            alt_rule: 0,
            overhead: cfg.cup,
            num_karts: n as i16,
            cutscene_sync: false,
            scene_center: [0; 3],
        };
        let mut s = Session {
            world,
            kart,
            record,
            globals,
            water,
            camera,
            camera_tables,
            camera_globals,
            ticks: 0,
            leader_progress_override: None,
            rng_override: None,
            race_pos_override: None,
            autopilot: false,
            others,
            player_ai,
            best_start: 0,
            pick: cfg.round_robin_start as usize % n,
            load_frame: true,
            rng_skip_items: None,
            camera_peek: true,
            rand_state,
            cfg,
        };
        if cfg.items {
            // FUN_8007B448 ran at the course load, before the race load the
            // seed above stands for (oracle start_probe: the race load's
            // 32 rand() calls follow 1-per-frame menu draws, no 92-call burst
            // for the tokens). Their colours come from the 92 seeds that lead
            // up to it, which leaves the race-load stream as it was.
            let n = s.world.pickups.tokens.as_ref().map(|l| l.points.len()).unwrap_or(0) as u32;
            let mut seed = bios_rand_rewind(cfg.rand_seed, n);
            let mut rand = || bios_rand(&mut seed);
            crate::items::pickups::init(&mut s.globals.items, &s.world.pickups, false, &mut rand);
        }
        // The records get a render pass for the starting poses (for drawing
        // before the first tick); the karts themselves keep the load state
        // (look point 0 etc.: the game's first render pass is after the first
        // frame's kart loop, and the contact pass reads the look point).
        s.sync_record();
        {
            let mut k = s.kart.clone();
            k.body = crate::body::body_matrix(&s.world.tables, &k);
            crate::render::after_kart_loop(&mut k, &mut s.record, &s.world, &s.world.driver);
        }
        for o in s.others.iter_mut() {
            o.kart.to_record(&mut o.record);
            let mut k = o.kart.clone();
            k.body = crate::body::body_matrix(&s.world.tables, &k);
            crate::render::after_kart_loop(&mut k, &mut o.record, &s.world, &o.driver);
        }
        Ok(s)
    }

    fn sync_record(&mut self) {
        self.kart.to_record(&mut self.record);
    }

    /// One game tick (dt = 2) with the PS1 pad word `buttons` (active high:
    /// 0x4000 CROSS accelerates, 0x2000 CIRCLE brakes, 0x8000 SQUARE jumps,
    /// 0x80 / 0x20 left / right; see update::btn).
    ///
    /// In the 60 Hz mode (`SessionConfig::logic_hz` 60) a tick is one VBlank
    /// (dt = 1): call it 60 times a second.
    pub fn tick(&mut self, buttons: u16) -> Result<(), SessionError> {
        let dt = if self.globals.hz60.on { 1 } else { 2 };
        self.tick_dt(buttons, dt)
    }

    /// The whole changing state (see `Snapshot`).
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            kart: self.kart.clone(),
            record: self.record.clone(),
            globals: self.globals.clone(),
            water: self.water,
            camera: self.camera.clone(),
            camera_globals: self.camera_globals.clone(),
            ticks: self.ticks,
            leader_progress_override: self.leader_progress_override,
            rng_override: self.rng_override,
            race_pos_override: self.race_pos_override,
            autopilot: self.autopilot,
            others: self.others.clone(),
            player_ai: self.player_ai,
            best_start: self.best_start,
            pick: self.pick,
            load_frame: self.load_frame,
            rng_skip_items: self.rng_skip_items,
            camera_peek: self.camera_peek,
            rand_state: self.rand_state,
            cfg: self.cfg,
        }
    }

    /// Put the session back in a state `snapshot` took (of this session or
    /// another on the same course with the same opponents' models).
    pub fn restore(&mut self, s: &Snapshot) {
        self.kart = s.kart.clone();
        self.record.clone_from(&s.record);
        self.globals.clone_from(&s.globals);
        self.water = s.water;
        self.camera.clone_from(&s.camera);
        self.camera_globals.clone_from(&s.camera_globals);
        self.ticks = s.ticks;
        self.leader_progress_override = s.leader_progress_override;
        self.rng_override = s.rng_override;
        self.race_pos_override = s.race_pos_override;
        self.autopilot = s.autopilot;
        self.others.clone_from(&s.others);
        self.player_ai = s.player_ai;
        self.best_start = s.best_start;
        self.pick = s.pick;
        self.load_frame = s.load_frame;
        self.rng_skip_items = s.rng_skip_items;
        self.camera_peek = s.camera_peek;
        self.rand_state = s.rand_state;
        self.cfg = s.cfg;
    }

    /// The logic rate: 30 (the game) or 60 (the 60 Hz mode).
    pub fn logic_hz(&self) -> u8 {
        if self.globals.hz60.on { 60 } else { 30 }
    }

    /// Switch the logic rate (30 or 60). Takes effect at a 30 Hz step
    /// boundary (after the second tick of a 60 Hz pair); returns whether it
    /// switched now. A race switched mid-way is no longer the faithful race.
    pub fn set_logic_hz(&mut self, hz: u8) -> bool {
        if self.globals.hz60.phase != 0 {
            return false;
        }
        self.globals.hz60.on = hz == 60;
        self.cfg.logic_hz = if hz == 60 { 60 } else { 30 };
        true
    }

    /// VBlanks of game time per `tick`: 2 (30 Hz) or 1 (60 Hz mode). Route
    /// files count VBlanks: tick k reads VBlank k * this.
    pub fn vblanks_per_tick(&self) -> i32 {
        if self.globals.hz60.on { 1 } else { 2 }
    }

    /// Seconds of game time per `tick`.
    pub fn tick_seconds(&self) -> f64 {
        if self.globals.hz60.on { 1.0 / 60.0 } else { 1.0 / 30.0 }
    }

    /// A tick of `dt` VBlanks (the game uses 3 when a frame runs long; live
    /// play uses `tick`, dt = 2).
    pub fn tick_dt(&mut self, buttons: u16, dt: i32) -> Result<(), SessionError> {
        let before = (self.kart.clone(), self.globals.clone(), self.water, self.camera.clone(), self.rand_state, self.record.clone(), self.others.clone(), self.best_start, self.pick, self.load_frame, self.player_ai);
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.tick_inner(buttons, dt)));
        match r {
            Ok(()) => {
                self.ticks += 1;
                Ok(())
            }
            Err(e) => {
                (self.kart, self.globals, self.water, self.camera, self.rand_state, self.record, self.others, self.best_start, self.pick, self.load_frame, self.player_ai) = before;
                let msg = e
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_default();
                Err(SessionError::OutOfScope(msg))
            }
        }
    }

    fn tick_inner(&mut self, buttons: u16, dt: i32) {
        self.globals.items.events.clear();
        // All karts as one slice in slot order (player = 0) for the frame.
        let mut ks: Vec<KartState> = Vec::with_capacity(1 + self.others.len());
        ks.push(std::mem::take(&mut self.kart));
        for o in self.others.iter_mut() {
            ks.push(std::mem::take(&mut o.kart));
        }
        self.frame(&mut ks, buttons, dt);
        if self.globals.hz60.on {
            self.globals.hz60.phase ^= 1;
        }
        let mut it = ks.into_iter();
        self.kart = it.next().unwrap();
        for (o, k) in self.others.iter_mut().zip(it) {
            o.kart = k;
        }
    }

    fn frame(&mut self, ks: &mut [KartState], buttons: u16, dt: i32) {
        let w = &self.world;
        let g = &mut self.globals;
        let n = ks.len();
        crate::sound::begin_frame(g, ks, dt, self.camera.pos);
        // 0. FUN_8005C2EC (main loop, before the race frame): frame counter,
        //    countdown (GO and the start boosts), race clock, one rand().
        if self.load_frame {
            // The load frame: FUN_8005C2EC's rand() is drawn (oracle: six
            // rand() calls in that frame), the countdown is not stepped.
            // 60 Hz mode: the load frame is the first pair (one draw).
            if g.hz60.pair_end() {
                self.load_frame = false;
            }
            if g.hz60.beat() {
                bios_rand(&mut self.rand_state);
            }
        } else {
            let mut karts: Vec<&mut KartState> = ks.iter_mut().collect();
            let seed = &mut self.rand_state;
            let mut rand = || bios_rand(seed);
            crate::race::frame_clock(g, &mut karts, &mut self.best_start, &mut rand);
        }
        // The frame's own dt (0x800B0AEC) is measured after FUN_8005C2EC,
        // which therefore steps the countdown / clock by the previous
        // frame's dt (oracle: countdown -2 in a frame whose kart update ran
        // with dt 3).
        g.dt = dt;
        // FUN_8007916C's start: once every human has finished (+0x4C7) the
        // race goes to the results (mode 10): that frame runs no race
        // simulation (0x800792xx, FUN_800606D0, then LAB_80079A58).
        if g.mode == 1 && g.race_over == 0 {
            let humans_done = ks.iter().filter(|x| (x.player_index as i32) < g.humans as i32 && x.finished != 0).count() as i32;
            if humans_done > 0 {
                if humans_done < g.humans as i32 {
                    crate::unreachable_in_scope!("one of two human players finished (split screen)");
                }
                g.mode = 10;
                return;
            }
        }
        // 1. Water animation.
        let rng_world = |a: u32| w.rng_byte(a);
        let (wn, an) = self.water.step(&w.tables, g.dt, &mut g.rng, &rng_world);
        crate::items::token_spins(g, w, self.camera.roll);
        if let Some(r) = self.rng_override.take() {
            g.rng = r;
        }
        // 2. Contact pass over every pair (FUN_800418D8), then the human's
        //    look-at target (FUN_80041A20 / FUN_80041CC4).
        let rules = crate::collide::ContactRules { karts: n as i32, tagged: -1 };
        crate::collide::frame_contacts_all(ks, g, w, &rules);
        for k in ks.iter_mut() {
            crate::render::aim_at_course(k, g, w);
        }
        for k in ks.iter_mut() {
            crate::render::aim_camera_override(k, g, self.camera.mode, self.camera.pos);
        }
        // 3. Path position (FUN_8009004C): this frame's kart of the round
        //    robin 0x800C3058[n]. None while the countdown runs (oracle
        //    start_probe: every AI kart's +0x4CB grows from race load to GO;
        //    the counter itself steps every frame).
        //    60 Hz mode: once per pair (the round robin keeps the game's
        //    rate; +0x4CB counts VBlanks either way).
        if g.countdown == 0 && g.hz60.beat() {
            crate::progress::path_position(&mut ks[self.pick % n], w);
        }
        // 4. The kart loop, slot order. The leader's progress feeds the
        //    catch-up.
        g.karts = n as i16;
        let leader = self.leader_progress_override.unwrap_or_else(|| ks.iter().find(|x| x.race_pos == 1).map(|x| x.progress).unwrap_or(ks[0].progress));
        let inp = TickInputs { leader_progress: leader, water_normal: wn, alt_normal: an, aim: None, cameras: [self.camera.pos, [0; 3]] };
        for s in 0..n {
            g.field = [(0, 0); 8];
            for (j, x) in ks.iter().enumerate().take(8) {
                g.field[j] = (x.finished, x.lap);
            }
            if ks[s].flags & 1 != 0 {
                // An AI kart, or the player's after the finish: FUN_800912F8
                // picks the buttons, the update runs with player -1. Items
                // are out of scope: their buttons are dropped.
                let ctx = crate::ai::AiContext { karts: n as i16, kart0_flags: ks[0].flags, kart0_race_pos: ks[0].race_pos, difficulty: g.difficulty, cup: g.cup, driver_pos: &[] };
                let ai = if s == 0 { &mut self.player_ai } else { &mut self.others[s - 1].ai };
                let seed = &mut self.rand_state;
                let mut rand = || bios_rand(seed);
                let b = ai_tick(g, s, |g| {
                    let b = crate::ai::ai_buttons(&mut ks[s], ai, g, &ctx, w, &mut rand);
                    if !self.cfg.items { crate::ai::items_off(b) } else { b }
                });
                crate::update::kart_update_at(ks, s, g, w, &inp, b, -1, &mut rand);
            } else {
                let seed = &mut self.rand_state;
                let mut rand = || bios_rand(seed);
                let b = if s == 0 && self.autopilot {
                    let ctx = crate::ai::AiContext { karts: n as i16, kart0_flags: ks[0].flags, kart0_race_pos: ks[0].race_pos, difficulty: g.difficulty, cup: g.cup, driver_pos: &[] };
                    let items = self.cfg.items;
                    ai_tick(g, 0, |g| {
                        let b = crate::ai::ai_buttons(&mut ks[0], &mut self.player_ai, g, &ctx, w, &mut rand);
                        if items { b } else { crate::ai::items_off(b) }
                    })
                } else {
                    buttons
                };
                crate::update::kart_update_at(ks, s, g, w, &inp, b, s as i16, &mut rand);
            }
            if s < g.humans as usize {
                crate::progress::track_direction(&mut ks[s], g, w);
            }
        }
        // 5. After the kart loop: wheel / driver part matrices (FUN_80086DF0),
        //    race progress and positions (FUN_800837F8), body, vehicle and
        //    driver matrices, driver position, look point, wheel and light
        //    positions (FUN_80025D8C).
        for (s, k) in ks.iter_mut().enumerate() {
            let (rec, ai, drv) = if s == 0 {
                (&mut self.record, &self.player_ai, &w.driver)
            } else {
                let o = &mut self.others[s - 1];
                (&mut o.record, &o.ai, &o.driver)
            };
            k.to_record(rec);
            ai.to_record(rec);
            crate::render::after_kart_loop(k, rec, w, drv);
        }
        {
            let mut karts: Vec<&mut KartState> = ks.iter_mut().collect();
            crate::race::race_progress(g, w, &mut karts, self.pick % n);
        }
        if let Some(p) = self.race_pos_override {
            ks[0].race_pos = p;
            ks[0].shown_pos = p;
            ks[0].pos_flash = 0;
        }
        if g.hz60.pair_end() {
            self.pick = (self.pick + 1) % n;
        }
        // 5b. After the kart loop (FUN_8007916C @0x80079A8C..): effects
        //     FUN_8006C388, FUN_8007F03C (kart particles, items/kart_fx.rs),
        //     FUN_8007A9DC, pickups FUN_8007BBF0,
        //     projectiles FUN_80069648 (items.rs).
        {
            let cams = crate::items::Cameras { pos: [self.camera.pos_div32, [0; 3]], n: g.humans.clamp(0, 2) as usize };
            let seed = &mut self.rand_state;
            let mut rand = || bios_rand(seed);
            let skip = self.rng_skip_items.take();
            let mut recs: Vec<&mut Vec<u8>> = std::iter::once(&mut self.record).chain(self.others.iter_mut().map(|o| &mut o.record)).collect();
            crate::items::after_kart_loop(ks, &mut recs, g, w, &cams, self.cfg.items, skip, &mut rand);
        }
        crate::sound::character_voices(ks, g, w);
        for (s, k) in ks.iter().enumerate() {
            let rec = if s == 0 { &mut self.record } else { &mut self.others[s - 1].record };
            k.to_record(rec);
        }
        let k = &ks[0];
        // 6. Random pointer step (60 Hz mode: once per pair).
        if g.hz60.beat() {
            let r = g.rng.wrapping_add(1);
            g.rng = if RNG_WRAP < r { g.rng.wrapping_sub(0x5DB) } else { r };
        }
        // 7. Camera: intro (countdown >= 300), chase, and the trackside
        //    cameras once the player has finished (camera.rs).
        let ck = CameraKart::from_record(&self.record, Some(k.trail));
        let inp = CameraInput {
            kart: ck,
            globals: CameraGlobals {
                dt: g.dt,
                num_karts: n as i16,
                countdown: g.countdown,
                countdown_len: g.countdown_len as i16,
                laps: g.laps,
                game_state: g.mode,
                ..self.camera_globals.clone()
            },
        };
        let mut env = SessionCameraEnv { peek_rand: std::mem::take(&mut self.camera_peek), world: w, rand_state: &mut self.rand_state, rng: &mut g.rng };
        camera::tick(&mut self.camera, &inp, &self.camera_tables, &mut env);
        // 8. After the camera (FUN_8007916C @0x80079B30..): each human's
        //    splash (FUN_800330D4 when +0x4DB, or in water / a special move
        //    under the ground at random: one random byte), the particle and
        //    spark lists (FUN_8008727C, FUN_800327D0). The splash rings and
        //    the skid quads FUN_80033440 go to the flat list 0x800C84B8,
        //    moved by FUN_80033B98 (particles.rs `Flats`, drawn by mr-bevy).
        for (s, k) in ks.iter().enumerate().take(g.humans.max(0) as usize) {
            let mut splash = k.splash != 0;
            if g.hz60.beat() && k.splash == 0 && (k.float_state != 0 || (k.below_ground != 0 && k.special != 0)) {
                splash = w.rng_byte(g.rng) < 0x20;
                g.rng = g.rng.wrapping_add(1);
            }
            let rec = if s == 0 { &self.record } else { &self.others[s - 1].record };
            if let Some(fx) = g.fx.as_mut() {
                if splash && g.hz60.beat() {
                    let size = crate::fixed::div_pow2(k.spark_offset as i32 * k.engine as i32, 12);
                    fx.flats.splash([k.x, k.water_y, k.z], size);
                }
                fx.flats.skids(s, rec, g.dt, &w.tables);
            }
        }
        if let Some(fx) = g.fx.as_mut() {
            fx.particles.update(g.dt, &w.tables.sin[..4096]);
            fx.sparks.update(g.dt);
            fx.flats.update(g.dt);
        }
    }

    /// Idle (no buttons) until the player's race time (+0x358) reaches
    /// `t`. `idle_until_race_time(1430)` reaches the oracle's DOCK savestate
    /// (slot 2): the countdown's 240 ticks, GO, then 142 more (the kart's
    /// race time runs one tick ahead of the race clock 0x800B0A94).
    pub fn idle_until_race_time(&mut self, t: i32) -> Result<(), SessionError> {
        while self.kart.race_time < t {
            self.tick(0)?;
        }
        Ok(())
    }

    /// Every kart's full 0x504-byte record after the last tick's render
    /// pass, slot order: (slot, character +0x4B2, vehicle +0x4B1, record).
    /// Slot 0 is the player.
    pub fn kart_records(&self) -> impl Iterator<Item = (usize, u8, u8, &[u8])> {
        std::iter::once((0usize, self.kart.character, self.kart.vehicle, &self.record[..]))
            .chain(self.others.iter().enumerate().map(|(i, o)| (i + 1, o.character, o.vehicle, &o.record[..])))
    }

    /// Where the race is for the player (intro, start game, racing,
    /// finished); see race::RacePhase for the field meanings.
    pub fn phase(&self) -> crate::race::RacePhase {
        crate::race::phase(&self.globals, &self.kart, &self.world)
    }

    /// One results line per kart (slot 0 = the player), sorted by place.
    pub fn results(&self) -> Vec<crate::race::KartResult> {
        let mut r: Vec<_> = std::iter::once(crate::race::result(0, &self.kart))
            .chain(self.others.iter().enumerate().map(|(i, o)| crate::race::result(i + 1, &o.kart)))
            .collect();
        r.sort_by_key(|x| x.place);
        r
    }

    /// The player's race position (+0x4BE, 1 = leading; the HUD shows
    /// +0x4CA, `kart().shown_pos`, which follows with a flash).
    pub fn position(&self) -> u8 {
        self.kart.race_pos
    }

    /// Laps the player has completed (+0x4BF).
    pub fn laps_done(&self) -> u8 {
        self.kart.lap
    }

    /// The race clock (0x800B0A94; 300 = 1 s; 0 until GO).
    pub fn race_clock(&self) -> i32 {
        self.globals.race_clock
    }

    /// The countdown (0x800B0C70): VBlanks to GO, 0 once racing.
    pub fn countdown(&self) -> i32 {
        self.globals.countdown
    }

    /// Analysis only: overwrite record ranges (and the matching kart
    /// fields) with `src`, e.g. effects of game code outside the port.
    pub fn inject(&mut self, src: &[u8], ranges: &[(usize, usize)]) {
        self.kart.to_record(&mut self.record);
        for &(o, n) in ranges {
            self.record[o..o + n].copy_from_slice(&src[o..o + n]);
        }
        let trail = self.kart.trail;
        self.kart = KartState::from_record(&self.record);
        self.kart.trail = trail;
    }

    /// The full kart record (0x504 bytes) as the game would hold it after
    /// the last tick: what part placement reads.
    pub fn record(&self) -> &[u8] {
        &self.record
    }

    pub fn kart(&self) -> &KartState {
        &self.kart
    }

    /// The camera's view for rendering (camera.rs; letterbox 0 in a race).
    pub fn camera_view(&self) -> CameraView {
        self.camera.view(0)
    }

    /// The BIOS rand() seed now (0x80009010; analysis).
    pub fn rand_seed(&self) -> u32 {
        self.rand_state
    }

    pub fn config(&self) -> &SessionConfig {
        &self.cfg
    }

    /// The race's item state: pickups, projectiles, this tick's events
    /// (crate::items; docs/notes/SESSION-4-coord.md "A items sim").
    pub fn items(&self) -> &crate::items::ItemState {
        &self.globals.items
    }

    /// The particle list (0x800E0C20; track B's `mr_sim::particles`).
    pub fn particles(&self) -> Option<&crate::particles::Particles> {
        self.globals.fx.as_ref().map(|f| &f.particles)
    }

    /// The spark streaks (0x800B4540).
    pub fn sparks(&self) -> Option<&crate::particles::Sparks> {
        self.globals.fx.as_ref().map(|f| &f.sparks)
    }

    /// The flat quads (0x800C84B8: splash rings, skid quads).
    pub fn flats(&self) -> Option<&crate::particles::Flats> {
        self.globals.fx.as_ref().map(|f| &f.flats)
    }

    /// Analysis: a session holding the race the game has in memory `m`
    /// (an oracle capture's frame): every kart, its AI fields, the race
    /// globals and item state, the camera, the water animation, the BIOS
    /// rand() seed, the round robin. Opponent models come from the disc.
    pub fn from_mem(disc: &mut mr_disc::Disc, world: World, m: &crate::mem::Mem) -> Result<Session, SessionError> {
        let exe = disc.read("SLUS_012.37").map_err(|e| SessionError::Disc(format!("{e:?}")))?;
        let camera_tables = CameraTables::from_exe(&exe).ok_or_else(|| SessionError::Disc("camera tables".into()))?;
        let n = (m.i16(0x800B_0DFE) + m.i16(0x800B_0C98)) as usize;
        let mut others = Vec::new();
        for slot in 1..n {
            let a = crate::kart::KART_ARRAY + 0x504 * slot as u32;
            let rec = m.slice(a, 0x504).to_vec();
            let (c, v) = (rec[0x4B2], rec[0x4B1]);
            let cpath = crate::ai::model_path(&world.exe, crate::ai::addr::CHARACTER_PATHS, c, "MDL").ok_or_else(|| SessionError::Disc(format!("character {c}: no model")))?;
            let vpath = crate::ai::model_path(&world.exe, crate::ai::addr::VEHICLE_PATHS, v, "MDL").ok_or_else(|| SessionError::Disc(format!("vehicle {v}: no model")))?;
            let b = disc.read(&cpath).map_err(|e| SessionError::Disc(format!("{cpath}: {e:?}")))?;
            let cm = Mdl::parse(&b).map_err(|e| SessionError::Disc(format!("{cpath}: {e:?}")))?;
            others.push(OtherKart {
                kart: crate::oracle::load_kart(m, slot as u32),
                ai: crate::ai::AiState::from_record(&rec),
                record: rec,
                character: c,
                vehicle: v,
                driver: crate::render::DriverModel { part0_pivot: cm.parts[0].record.pivot },
                vehicle_model: vpath,
                character_model: cpath,
            });
        }
        let record = m.slice(crate::kart::KART_ARRAY, 0x504).to_vec();
        let mut cfg = SessionConfig::course(&world.info);
        if world.info.id == crate::course::DOCK {
            cfg = SessionConfig::dock_default();
        }
        cfg.opponents = (n - 1) as u8;
        let mut s = Session {
            kart: crate::oracle::load_kart(m, 0),
            player_ai: crate::ai::AiState::from_record(&record),
            record,
            globals: RaceGlobals::read(m),
            water: WaterAnim::from_bytes(m.slice(crate::water::STATE_ADDR, 0x1C)),
            camera: CameraState::from_bytes(m.slice(0x800C_4F40, camera::CAMERA_SIZE)),
            camera_tables,
            camera_globals: CameraGlobals::from_reader(&|a, l| m.slice(a, l).to_vec()),
            world,
            ticks: 0,
            leader_progress_override: None,
            rng_override: None,
            race_pos_override: None,
            autopilot: false,
            others,
            best_start: m.u8(0x800E_02BA),
            pick: m.u8(0x800C_3058 + n as u32) as usize % n,
            load_frame: false,
            rng_skip_items: None,
            camera_peek: false,
            rand_state: m.u32(0x8000_9010),
            cfg,
        };
        s.globals.fx = Some(Box::new(crate::items::Fx { particles: crate::particles::Particles::from_exe(&exe), sparks: Default::default(), flats: Default::default() }));
        s.load_mem(m);
        Ok(s)
    }

    /// Analysis: overwrite the race state with memory `m` (see `from_mem`;
    /// the opponents must be the same karts).
    pub fn load_mem(&mut self, m: &crate::mem::Mem) {
        let n = 1 + self.others.len();
        self.kart = crate::oracle::load_kart(m, 0);
        self.record = m.slice(crate::kart::KART_ARRAY, 0x504).to_vec();
        self.player_ai = crate::ai::AiState::from_record(&self.record);
        for (i, o) in self.others.iter_mut().enumerate() {
            let a = crate::kart::KART_ARRAY + 0x504 * (i + 1) as u32;
            o.record = m.slice(a, 0x504).to_vec();
            o.kart = crate::oracle::load_kart(m, (i + 1) as u32);
            o.ai = crate::ai::AiState::from_record(&o.record);
        }
        let stack = self.globals.stack;
        let types = self.globals.fx.as_ref().map(|f| f.particles.types.clone()).unwrap_or_default();
        self.globals = RaceGlobals::read(m);
        self.globals.stack = stack;
        self.globals.fx = Some(Box::new(crate::items::Fx { particles: crate::particles::Particles::read(m, types), sparks: crate::particles::Sparks::read(m), flats: crate::particles::Flats::read(m) }));
        self.water = WaterAnim::from_bytes(m.slice(crate::water::STATE_ADDR, 0x1C));
        self.camera = CameraState::from_bytes(m.slice(0x800C_4F40, camera::CAMERA_SIZE));
        self.camera_globals = CameraGlobals::from_reader(&|a, l| m.slice(a, l).to_vec());
        self.best_start = m.u8(0x800E_02BA);
        // The counter 0x800C3058[n] steps before the race frame reads it.
        self.pick = (m.u8(0x800C_3058 + n as u32) as usize + 1) % n;
        self.load_frame = false;
        self.rand_state = m.u32(0x8000_9010);
    }

    /// Analysis: write the race state into memory `m` (the fields the port
    /// models), for comparison with the game's next frame.
    pub fn store_mem(&self, m: &mut crate::mem::Mem) {
        let mut rec = self.record.clone();
        self.kart.to_record(&mut rec);
        self.player_ai.to_record(&mut rec);
        m.write(crate::kart::KART_ARRAY, &rec);
        for (i, o) in self.others.iter().enumerate() {
            let mut rec = o.record.clone();
            o.kart.to_record(&mut rec);
            o.ai.to_record(&mut rec);
            m.write(crate::kart::KART_ARRAY + 0x504 * (i + 1) as u32, &rec);
        }
        self.globals.write(m);
        self.globals.items.write(m);
        m.put_u32(0x8000_9010, self.rand_state);
    }
}

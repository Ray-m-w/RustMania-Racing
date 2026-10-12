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
//! Per tick, in the race frame's order (FUN_8007916C):
//! 1. FUN_8003C5FC water animation (normals, random draws) — water.rs;
//! 2. FUN_800418D8 contact pass: no other karts, only its per-frame resets;
//! 3. FUN_8009004C path position (one kart: kart 0 every frame);
//! 4. FUN_80064B5C + FUN_80086B7C — update.rs / progress.rs;
//! 5. FUN_80025D8C body matrix — body.rs;
//! 6. FUN_8007D304's random-pointer step (wraps in 0x8001F94C..0x8001FF28);
//! 7. FUN_80070418 camera — camera.rs;
//! 8. FUN_8005C2EC frame counter and race clock.
//! Not produced (render side or other subsystems; see the ledger): the
//! look-at target +0x2C0 (FUN_80041A20/80041CC4) and look point +0x324
//! (FUN_80025D8C), the race-position code (FUN_800837F8), track tokens that
//! fill the turbo meter (FUN_8007BBF0), AI karts, items.

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
    /// PsyQ rand() seed for the few cosmetic draws (wheel angles, camera).
    pub rand_seed: u32,
}

impl SessionConfig {
    /// The settings of the oracle's DOCK race, read once from that
    /// savestate's RAM and kept here as race settings [RAM]: laps
    /// (0x800B0DA6) 3, event (0x800B0FAC) 3, cup (0x800B0E38) 4, DOCK lap
    /// record (0x800E0190 + 0x20 * 4) 12186 (= 0:40.62 at 300 units/s).
    pub fn dock_default() -> Self {
        SessionConfig {
            grid: 5,
            character: 0,
            laps: 3,
            alt_controls: false,
            event: 3,
            cup: 4,
            lap_record: 12186,
            rand_seed: 1,
        }
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
    rand_state: u32,
    cfg: SessionConfig,
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

/// BIOS rand() (A0:2F, SCPH-1001 @0xBFC02200): seed = seed * 0x41C64E6D +
/// 0x3039; result (seed >> 16) & 0x7FFF.
pub fn bios_rand(seed: &mut u32) -> i32 {
    *seed = seed.wrapping_mul(0x41C6_4E6D).wrapping_add(0x3039);
    ((*seed >> 16) & 0x7FFF) as i32
}

impl Session {
    /// DOCK, Kermit in the RACER, one player, no AI karts.
    pub fn new(disc: &mut mr_disc::Disc) -> Result<Session, SessionError> {
        Self::with_config(disc, SessionConfig::dock_default())
    }

    pub fn with_config(disc: &mut mr_disc::Disc, cfg: SessionConfig) -> Result<Session, SessionError> {
        let world = World::dock(disc).map_err(SessionError::World)?;
        let mdl = disc.read("VEHIC_A/RACER.MDL").map_err(|e| SessionError::Disc(format!("{e:?}")))?;
        let racer = Mdl::parse(&mdl).map_err(|e| SessionError::Disc(format!("RACER.MDL: {e:?}")))?;
        if cfg.character != 0 {
            return Err(SessionError::OutOfScope("only Kermit (character 0) is wired up".into()));
        }
        let kmdl = disc.read("CHARS_A/KERMIT.MDL").map_err(|e| SessionError::Disc(format!("{e:?}")))?;
        let kermit = Mdl::parse(&kmdl).map_err(|e| SessionError::Disc(format!("KERMIT.MDL: {e:?}")))?;
        let exe = disc.read("SLUS_012.37").map_err(|e| SessionError::Disc(format!("{e:?}")))?;
        let camera_tables = CameraTables::from_exe(&exe).ok_or_else(|| SessionError::Disc("camera tables".into()))?;
        let template = crate::vehicle::racer_template(&world.tables, &racer);
        let setup = RaceSetup { mode: 1, race_type: 0, race_flags: 0, challenge: 0, humans: 1, karts: 1 };
        let mut rng = RNG_START;
        let mut rand_state = cfg.rand_seed;
        let (record, kart) = {
            let mut rand = || bios_rand(&mut rand_state);
            init_kart(
                &KartInit { world: &world, template: &template, setup, slot: 0, character: cfg.character, vehicle: 0, grid: cfg.grid, flags: 0, character_pivot_y: kermit.parts[0].record.pivot[1] },
                &mut rng,
                &mut rand,
            )
        };
        let globals = RaceGlobals {
            dt: 2,
            mode: 1,
            race_clock: 0,
            countdown: 0,
            countdown_len: 300,
            race_flags: 0,
            challenge: 0,
            race_type: 0,
            race_over: 0,
            game_state: cfg.event,
            split_screen: 0,
            humans: 1,
            laps: cfg.laps,
            frame: 0,
            rng,
            alt_controls: [cfg.alt_controls as u8, 0],
            sound_event: 0,
            // 0x800B0CD8 is 1 during a race (finishing order counter).
            end_mode: 1,
            lap_threshold: 0,
            lap_record: cfg.lap_record,
        };
        let water = WaterAnim::from_exe(&world.exe);
        // Camera record before its first tick: any mode other than 0 makes
        // the first tick a mode change, which places the camera directly.
        let mut cam_bytes = [0u8; camera::CAMERA_SIZE];
        cam_bytes[0x64..0x66].copy_from_slice(&(-1i16).to_le_bytes());
        cam_bytes[0x66..0x68].copy_from_slice(&(-1i16).to_le_bytes());
        let camera = CameraState::from_bytes(&cam_bytes);
        let camera_globals = CameraGlobals {
            dt: 2,
            finish_timer: 0,
            finish_orbit_at: 300,
            game_state: 1,
            event: cfg.event,
            alt_rule: 0,
            overhead: cfg.cup,
            num_karts: 1,
            cutscene_sync: false,
            scene_center: [0; 3],
        };
        let mut s = Session { world, kart, record, globals, water, camera, camera_tables, camera_globals, ticks: 0, leader_progress_override: None, rng_override: None, rand_state, cfg };
        s.kart.body = crate::body::body_matrix(&s.world.tables, &s.kart);
        s.sync_record();
        // The race frame's render pass for the starting pose.
        {
            let (k, rec, w) = (&mut s.kart, &mut s.record, &s.world);
            crate::render::after_kart_loop(k, rec, w, &w.driver);
        }
        Ok(s)
    }

    fn sync_record(&mut self) {
        self.kart.to_record(&mut self.record);
    }

    /// One game tick (dt = 2) with the PS1 pad word `buttons` (active high:
    /// 0x4000 CROSS accelerates, 0x2000 CIRCLE brakes, 0x8000 SQUARE jumps,
    /// 0x80 / 0x20 left / right; see update::btn).
    pub fn tick(&mut self, buttons: u16) -> Result<(), SessionError> {
        self.tick_dt(buttons, 2)
    }

    /// A tick of `dt` VBlanks (the game uses 3 when a frame runs long; live
    /// play uses `tick`, dt = 2).
    pub fn tick_dt(&mut self, buttons: u16, dt: i32) -> Result<(), SessionError> {
        let before = (self.kart.clone(), self.globals.clone(), self.water, self.camera.clone(), self.rand_state, self.record.clone());
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.tick_inner(buttons, dt)));
        match r {
            Ok(()) => {
                self.ticks += 1;
                Ok(())
            }
            Err(e) => {
                (self.kart, self.globals, self.water, self.camera, self.rand_state, self.record) = before;
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
        let w = &self.world;
        let g = &mut self.globals;
        let k = &mut self.kart;
        g.dt = dt;
        // 1. Water animation.
        let rng_world = |a: u32| w.rng_byte(a);
        let (wn, an) = self.water.step(&w.tables, g.dt, &mut g.rng, &rng_world);
        if let Some(r) = self.rng_override.take() {
            g.rng = r;
        }
        // 2. Contact pass resets (no other karts), then the look-at target
        //    (FUN_80041A20 / FUN_80041CC4).
        k.aim_dist = 0x7FFF_FFFF;
        k.nearest_kart = -1;
        k.close_kart = 0;
        crate::render::aim_at_course(k, g, w);
        crate::render::aim_camera_override(k, g, self.camera.mode);
        // 3. Path position.
        crate::progress::path_position(k, w);
        // 4. Kart update. With no AI the player leads the race.
        let leader = self.leader_progress_override.unwrap_or(k.progress);
        let inp = TickInputs { leader_progress: leader, water_normal: wn, alt_normal: an, aim: None };
        crate::tick(k, g, w, &inp, buttons);
        // 5. After the kart loop: wheel / driver part matrices (FUN_80086DF0),
        //    body, vehicle and driver matrices, driver position, look point,
        //    wheel and light positions (FUN_80025D8C).
        k.to_record(&mut self.record);
        crate::render::after_kart_loop(k, &mut self.record, w, &w.driver);
        // 6. Random pointer step.
        let n = g.rng.wrapping_add(1);
        g.rng = if RNG_WRAP < n { g.rng.wrapping_sub(0x5DB) } else { n };
        // 7. Camera.
        k.to_record(&mut self.record);
        let ck = CameraKart::from_record(&self.record, Some(k.trail));
        let inp = CameraInput { kart: ck, globals: CameraGlobals { dt: g.dt, ..self.camera_globals.clone() } };
        let ter = &w.ter;
        let mut ground = |x: i32, y: i32, z: i32| -> CameraGround {
            let q = ter.ground(x, y, z);
            CameraGround {
                found: q.nearest.is_some(),
                nearest_y: q.nearest_y,
                below_y: q.below.map(|s| s.y).unwrap_or(q.nearest_y),
                alt_found: q.alt.is_some(),
                alt_y: q.alt.map(|s| s.y).unwrap_or(0),
                type3_min_y: q.water_y,
            }
        };
        let seed = &mut self.rand_state;
        let mut rand = || bios_rand(seed);
        camera::tick(&mut self.camera, &inp, &self.camera_tables, &mut ground, &mut rand);
        // 8. Frame counter and race clock (FUN_8005C2EC).
        g.frame = g.frame.wrapping_add(g.dt);
        if g.countdown == 0 {
            g.race_clock = g.race_clock.wrapping_add(g.dt * 5);
        }
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

    pub fn config(&self) -> &SessionConfig {
        &self.cfg
    }
}

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
//! 4. FUN_80064B5C + FUN_80086B7C — update.rs / progress.rs (the player's
//!    kart; the AI port adds the others);
//! 5. FUN_80086DF0, FUN_800837F8 (race progress, positions), FUN_80025D8C
//!    (body, vehicle, driver matrices, look point, wheels, lights);
//! 6. FUN_8007D304's random-pointer step (wraps in 0x8001F94C..0x8001FF28);
//! 7. FUN_80070418 camera — camera.rs (held during the intro and after the
//!    finish: those camera modes are not ported).
//! Not produced (see the ledger): track tokens that fill the turbo meter
//! (FUN_8007BBF0), items, AI karts (ai.rs, another agent).

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
    /// Analysis only: the player's race position after the position pass
    /// (e.g. 6, the oracle race's, so the catch-up runs as it did there).
    pub race_pos_override: Option<u8>,
    /// Karts 1.. (AI). Empty until the AI port adds them; every race-wide
    /// pass (GO, contacts, positions, results) already includes them.
    pub others: Vec<OtherKart>,
    /// 0x800E02BA: the best start boost of the race (start game).
    pub best_start: u8,
    /// 0x800C3058[n]: the kart FUN_8009004C / FUN_800837F8 serve this
    /// frame (round robin over the karts).
    pub pick: usize,
    /// The first race frame after the race load runs with the countdown
    /// held at 480 (oracle: kart updated once, countdown unchanged).
    load_frame: bool,
    rand_state: u32,
    cfg: SessionConfig,
}

/// A kart other than the player's (slot 1..). Who drives it is the AI
/// port's business (ai.rs); until it does, a kart here is parked.
#[derive(Clone)]
pub struct OtherKart {
    pub kart: KartState,
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
            // FUN_8008C4AC @0x8008DA4C: 300 + 180 VBlanks to GO.
            countdown: crate::race::COUNTDOWN_START,
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
            karts: 1,
            field: [(0, 0); 8],
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
        let mut s = Session { world, kart, record, globals, water, camera, camera_tables, camera_globals, ticks: 0, leader_progress_override: None, rng_override: None, race_pos_override: None, others: Vec::new(), best_start: 0, pick: 0, load_frame: true, rand_state, cfg };
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
        let before = (self.kart.clone(), self.globals.clone(), self.water, self.camera.clone(), self.rand_state, self.record.clone(), self.others.clone(), self.best_start, self.pick, self.load_frame);
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.tick_inner(buttons, dt)));
        match r {
            Ok(()) => {
                self.ticks += 1;
                Ok(())
            }
            Err(e) => {
                (self.kart, self.globals, self.water, self.camera, self.rand_state, self.record, self.others, self.best_start, self.pick, self.load_frame) = before;
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
        let n_karts = 1 + self.others.len();
        // 0. FUN_8005C2EC (main loop, before the race frame): frame counter,
        //    countdown (GO and the start boosts), race clock, one rand().
        if self.load_frame {
            self.load_frame = false;
        } else {
            let mut karts: Vec<&mut KartState> =
                std::iter::once(&mut self.kart).chain(self.others.iter_mut().map(|o| &mut o.kart)).collect();
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
            let humans_done = std::iter::once(&self.kart)
                .chain(self.others.iter().map(|o| &o.kart))
                .filter(|x| (x.player_index as i32) < g.humans as i32 && x.finished != 0)
                .count() as i32;
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
        if let Some(r) = self.rng_override.take() {
            g.rng = r;
        }
        // 2. Contact pass (FUN_800418D8), then the look-at target
        //    (FUN_80041A20 / FUN_80041CC4).
        {
            let rules = crate::collide::ContactRules { karts: n_karts as i32, tagged: -1 };
            let mut others: Vec<KartState> = self.others.iter().map(|o| o.kart.clone()).collect();
            crate::collide::frame_contacts(&mut self.kart, &mut others, g, w, &rules);
            for (o, ok) in self.others.iter_mut().zip(others) {
                o.kart = ok;
            }
        }
        let k = &mut self.kart;
        crate::render::aim_at_course(k, g, w);
        crate::render::aim_camera_override(k, g, self.camera.mode);
        // 3. Path position (FUN_8009004C): this frame's kart of the round
        //    robin 0x800C3058[n].
        if self.pick == 0 {
            crate::progress::path_position(k, w);
        } else {
            crate::progress::path_position(&mut self.others[self.pick - 1].kart, w);
        }
        // 4. Kart update. The leader's progress feeds the catch-up.
        g.karts = n_karts as i16;
        g.field = [(0, 0); 8];
        for (j, x) in std::iter::once(&*k).chain(self.others.iter().map(|o| &o.kart)).enumerate().take(8) {
            g.field[j] = (x.finished, x.lap);
        }
        let leader = self.leader_progress_override.unwrap_or_else(|| {
            std::iter::once(&*k)
                .chain(self.others.iter().map(|o| &o.kart))
                .find(|x| x.race_pos == 1)
                .map(|x| x.progress)
                .unwrap_or(k.progress)
        });
        let inp = TickInputs { leader_progress: leader, water_normal: wn, alt_normal: an, aim: None };
        // After the finish the game hands the kart to the AI (flags bit 0):
        // FUN_800912F8 would choose its buttons. Until the AI port drives
        // it, it gets none and rolls to a stop (ledger, race flow).
        let buttons = if k.flags & 1 != 0 { Self::finished_kart_buttons(k) } else { buttons };
        crate::tick(k, g, w, &inp, buttons);
        // 5. After the kart loop: wheel / driver part matrices (FUN_80086DF0),
        //    race progress and positions (FUN_800837F8), body, vehicle and
        //    driver matrices, driver position, look point, wheel and light
        //    positions (FUN_80025D8C).
        k.to_record(&mut self.record);
        crate::render::after_kart_loop(k, &mut self.record, w, &w.driver);
        {
            let mut karts: Vec<&mut KartState> =
                std::iter::once(&mut self.kart).chain(self.others.iter_mut().map(|o| &mut o.kart)).collect();
            crate::race::race_progress(g, w, &mut karts, self.pick);
        }
        if let Some(p) = self.race_pos_override {
            self.kart.race_pos = p;
            self.kart.shown_pos = p;
            self.kart.pos_flash = 0;
        }
        self.pick = (self.pick + 1) % n_karts;
        let k = &mut self.kart;
        k.to_record(&mut self.record);
        // 6. Random pointer step.
        let n = g.rng.wrapping_add(1);
        g.rng = if RNG_WRAP < n { g.rng.wrapping_sub(0x5DB) } else { n };
        // 7. Camera. During the intro (countdown >= 300) the game flies the
        //    camera round the course, and after the finish it switches to
        //    the finished-kart / results cameras (camera.rs: not ported;
        //    ledger): in both the camera is held where it is.
        if g.countdown >= g.countdown_len as i32 || k.finished != 0 || g.mode != 1 {
            return;
        }
        let ck = CameraKart::from_record(&self.record, Some(k.trail));
        let inp = CameraInput {
            kart: ck,
            globals: CameraGlobals {
                dt: g.dt,
                num_karts: n_karts as i16,
                // 0x800B0C70 / 0x800B07F8: the camera eases differently
                // during the start game.
                finish_timer: g.countdown,
                finish_orbit_at: g.countdown_len as i16,
                game_state: g.mode,
                ..self.camera_globals.clone()
            },
        };
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

    /// The buttons of the player's kart after the finish, when the game has
    /// handed it to the AI. The AI port (ai.rs, FUN_800912F8) plugs in
    /// here; until then: none.
    fn finished_kart_buttons(_k: &KartState) -> u16 {
        0
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

    pub fn config(&self) -> &SessionConfig {
        &self.cfg
    }
}

//! Game globals the kart update reads or writes (0x800B0xxx unless noted).

use crate::mem::Mem;

/// Race-wide state shared by every kart update in a frame.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RaceGlobals {
    /// 0x800B0AEC: VBlanks per tick (2 in a race). Every per-tick rate is
    /// applied as `dt` sub-steps or multiplied by it.
    pub dt: i32,
    /// 0x800B0D46: game mode, 1 = race; 0x12 forces the buttons to 0.
    pub mode: i16,
    /// 0x800B0A94: race clock (+5 per VBlank pair after the start).
    pub race_clock: i32,
    /// 0x800B0C70: non-zero before the start (countdown).
    pub countdown: i32,
    /// 0x800B07F8: countdown length (the boost-start window test).
    pub countdown_len: i32,
    /// 0x800B07E2: race option bits.
    pub race_flags: u16,
    /// 0x800B07E6: challenge mode (0 in a normal race).
    pub challenge: i16,
    /// 0x800B0B9A: race type (3 = time trial ghost).
    pub race_type: i16,
    /// 0x800B0BD2: race over / paused flag.
    pub race_over: u8,
    /// 0x800B0FAC: top-level game state (0x1E = attract demo).
    pub game_state: i32,
    /// 0x800B0CA6: two-player split screen.
    pub split_screen: u8,
    /// 0x800B0E28: number of human players.
    pub humans: i16,
    /// 0x800B0DA6: laps in the race.
    pub laps: i16,
    /// 0x800B0D90: frame counter (water bob phase).
    pub frame: i32,
    /// 0x800B0BCC: the "random" byte pointer. The game draws pseudo-random
    /// bytes by reading memory at this address and advancing it; the bytes
    /// are the executable's own text (ExeImage).
    pub rng: u32,
    /// 0x800E0392 + player: 1 = alternate controls (brake on SQUARE, jump on
    /// CIRCLE); otherwise brake CIRCLE, jump SQUARE. Accelerate is CROSS.
    pub alt_controls: [u8; 2],
    /// 0x800B0EC4: UI sound request written by the update.
    pub sound_event: u8,
    /// 0x800B0CD8: race-end display mode (< 2 in a normal race).
    pub end_mode: u8,
    /// 0x800B0DD0: lap-time threshold for a lap announcement (0 = none).
    pub lap_threshold: i32,
    /// The course lap record compared at each lap end:
    /// 0x800E0190 + 8 * (s8)0x800B0E32 + 0x20 * (s8)0x800B0E38.
    pub lap_record: i32,
    /// 0x800B0DFE: karts in the race (humans + AI).
    pub karts: i16,
    /// Every kart's (finished +0x4C7, lap +0x4BF), slot order, as the kart
    /// loop sees them (an early finish counts the karts still racing on a
    /// later lap). The race driver refreshes it before each kart update.
    pub field: [(u8, u8); 8],
    /// 0x800E039A: AI catch-up scale, /16 (8 in the oracle's races).
    pub difficulty: u8,
    /// 0x800B0E38: cup / course slot (4 for DOCK in the oracle's race); the
    /// AI catch-up scaling applies below 7, the camera reads it too.
    pub cup: i8,
    /// 0x800B0E8C: bit per kart slot with the best start boost at GO.
    pub start_best_mask: u8,
    /// Sound calls of the frame and the listener (crate::sound, track D).
    pub sound: crate::sound::SoundState,
    /// Item state: pickups, projectiles, the special in progress
    /// (crate::items; 0x800E1B70, 0x800DDC50, 0x800B07E0/07F4/07F6, ...).
    pub items: crate::items::ItemState,
    /// FUN_80064B5C's stack slots a special move reads without writing
    /// (see `StackSlots`): what the previous kart update left there.
    pub stack: StackSlots,
    /// Particles and sparks (crate::particles); None in analysis runs that
    /// do not track them.
    pub fx: Option<Box<crate::items::Fx>>,
}

/// Two locals of FUN_80064B5C that its special-move path (+0x4D6) reads
/// but does not set: the steering rate 0x58(sp) (steering lag) and the
/// sideways speed 0x48(sp) (body-sway input). The kart updates of a frame
/// run at the same stack depth, so they hold the last value an ordinary
/// update stored. Not game memory the capture holds; the harness carries
/// them over.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StackSlots {
    pub steer_rate: i32,
    pub side0: i32,
}

pub mod addr {
    pub const DT: u32 = 0x800B_0AEC;
    pub const MODE: u32 = 0x800B_0D46;
    pub const RACE_CLOCK: u32 = 0x800B_0A94;
    pub const COUNTDOWN: u32 = 0x800B_0C70;
    pub const COUNTDOWN_LEN: u32 = 0x800B_07F8;
    pub const RACE_FLAGS: u32 = 0x800B_07E2;
    pub const CHALLENGE: u32 = 0x800B_07E6;
    pub const RACE_TYPE: u32 = 0x800B_0B9A;
    pub const RACE_OVER: u32 = 0x800B_0BD2;
    pub const GAME_STATE: u32 = 0x800B_0FAC;
    pub const SPLIT: u32 = 0x800B_0CA6;
    pub const HUMANS: u32 = 0x800B_0E28;
    pub const LAPS: u32 = 0x800B_0DA6;
    pub const FRAME: u32 = 0x800B_0D90;
    pub const RNG: u32 = 0x800B_0BCC;
    pub const ALT_CONTROLS: u32 = 0x800E_0392;
    pub const SOUND_EVENT: u32 = 0x800B_0EC4;
    /// Per-player button words fed to the kart update.
    pub const BUTTONS: u32 = 0x800B_0CE8;
}

impl RaceGlobals {
    pub fn read(m: &Mem) -> RaceGlobals {
        use addr::*;
        RaceGlobals {
            dt: m.i32(DT),
            mode: m.i16(MODE),
            race_clock: m.i32(RACE_CLOCK),
            countdown: m.i32(COUNTDOWN),
            countdown_len: m.i32(COUNTDOWN_LEN),
            race_flags: m.u16(RACE_FLAGS),
            challenge: m.i16(CHALLENGE),
            race_type: m.i16(RACE_TYPE),
            race_over: m.u8(RACE_OVER),
            game_state: m.i32(GAME_STATE),
            split_screen: m.u8(SPLIT),
            humans: m.i16(HUMANS),
            laps: m.i16(LAPS),
            frame: m.i32(FRAME),
            rng: m.u32(RNG),
            alt_controls: [m.u8(ALT_CONTROLS), m.u8(ALT_CONTROLS + 1)],
            sound_event: m.u8(SOUND_EVENT),
            end_mode: m.u8(0x800B_0CD8),
            lap_threshold: m.i32(0x800B_0DD0),
            karts: m.i16(0x800B_0DFE),
            field: std::array::from_fn(|j| {
                let b = crate::kart::KART_ARRAY + j as u32 * crate::kart::RECORD_SIZE as u32;
                (m.u8(b + 0x4C7), m.u8(b + 0x4BF))
            }),
            difficulty: m.u8(0x800E_039A),
            cup: m.i8(0x800B_0E38),
            start_best_mask: m.u8(0x800B_0E8C),
            sound: Default::default(),
            items: crate::items::ItemState::read(m),
            stack: StackSlots::default(),
            fx: None,
            lap_record: m.i32(0x800E_0190u32.wrapping_add((8 * m.i8(0x800B_0E32) as i32 + 0x20 * m.i8(0x800B_0E38) as i32) as u32)),
        }
    }

    /// Write back the globals the kart update modifies.
    pub fn write(&self, m: &mut Mem) {
        m.put_u32(addr::RNG, self.rng);
        m.put_u8(addr::SOUND_EVENT, self.sound_event);
    }
}

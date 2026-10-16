//! The kart record (0x504 bytes at 0x800C30A8 + 0x504 * slot) as named fields.
//!
//! Only the fields the kart update (FUN_80064B5C, FUN_80086B7C and callees)
//! reads or writes are modelled. `to_record` writes exactly these fields into
//! a copy of a record and leaves every other byte alone. Offsets are from the
//! record base; meanings are documented in docs/spec/kart-state.md, where each
//! is tagged [CODE addr] / [RAM] / [INFER].

macro_rules! kart_fields {
    ($( $(#[$m:meta])* $name:ident : $ty:ident @ $off:expr ),* $(,)?) => {
        /// Named view of one kart record. See the module docs.
        #[derive(Clone, Debug, Default, PartialEq, Eq)]
        pub struct KartState {
            $( $(#[$m])* pub $name: $ty, )*
            /// Wheel blocks of 0x2C bytes at +0x14 (+0x170 says how many are
            /// used: 4 on the RACER; 3 to 6 on other vehicles, e.g. the
            /// TRIKE 3, the M_BUGGY 5, the W_BUGGY 6; AI karts on DOCK).
            pub wheels: [Wheel; MAX_WHEELS],
            /// +0x35C: lap times, 4 bytes each, indexed by lap (+0x4BF);
            /// ten (the menus allow 1..10 laps once a course is won), up to
            /// the best lap at +0x384.
            pub lap_time: [i32; 10],
            /// +0x3A0..+0x3E8: 18 words, the body-sway spring state stepped
            /// by FUN_80064738 (positions, velocities, targets per axis).
            pub sway: [i32; 18],
            /// +0x414..+0x438: 18 halfwords, the interpolated body shape
            /// (current / from / to triples), lerped by +0x4A6 / +0x4A7.
            pub shape: [i16; 18],
            /// +0x4D0..+0x4D5: weapon slots (L1 slot: current/next type at
            /// +0x4D0/+0x4D1, L2 slot at +0x4D2/+0x4D3) and their charge
            /// timers (+0x4D4/+0x4D5, 0x40 = ready to fire).
            pub weapons: [u8; 6],
            /// The position-history ring the record points to (+0x2DC),
            /// 12 entries, newest first. Not part of the 0x504 bytes: the
            /// harness loads and stores it through `history_ptr`.
            pub trail: [crate::camera::TrailEntry; crate::camera::TRAIL_LEN],
            /// +0x18C: body matrix (4.12, rows), rebuilt after the kart loop
            /// by FUN_80025D8C (body.rs); the camera reads it.
            pub body: [[i16; 3]; 3],
            /// +0x1AC: vehicle matrix (body x lean), FUN_80025D8C; weapons
            /// launch along it (FUN_80068C6C -> FUN_8007D58C).
            pub vmat: [[i16; 3]; 3],
            /// 60 Hz mode only (not game memory): the world velocity the
            /// last kart-frame -> world conversion wrote (vx, vz) and the
            /// 1/4096 remainders its truncation dropped; the next
            /// conversion back to the kart frame uses them while +0x2F4 /
            /// +0x2FC still hold those values (kart-physics-ledger.md
            /// "60 Hz mode", the velocity frame conversion). None at 30 Hz.
            pub hz60_vel: Option<[i32; 4]>,
            /// 60 Hz mode only: +0x2D2 (steer) at the start of the pair,
            /// what the power turn's yaw reads on both ticks of it.
            pub hz60_steer: i16,
        }

        /// Byte offsets of every scalar field, for reports.
        pub const FIELD_OFFSETS: &[(&str, usize, usize)] = &[
            $( (stringify!($name), $off, std::mem::size_of::<$ty>()), )*
        ];

        impl KartState {
            /// Read the owned fields from a 0x504-byte record.
            pub fn from_record(r: &[u8]) -> KartState {
                assert!(r.len() >= RECORD_SIZE);
                let mut k = KartState {
                    $( $name: <$ty>::from_le_bytes(r[$off..$off + std::mem::size_of::<$ty>()].try_into().unwrap()), )*
                    ..Default::default()
                };
                for (i, w) in k.wheels.iter_mut().enumerate() {
                    *w = Wheel::read(r, 0x14 + 0x2C * i);
                }
                for i in 0..10 {
                    k.lap_time[i] = i32::from_le_bytes(r[0x35C + 4 * i..0x360 + 4 * i].try_into().unwrap());
                }
                for i in 0..18 {
                    k.sway[i] = i32::from_le_bytes(r[0x3A0 + 4 * i..0x3A4 + 4 * i].try_into().unwrap());
                    k.shape[i] = i16::from_le_bytes(r[0x414 + 2 * i..0x416 + 2 * i].try_into().unwrap());
                }
                k.weapons.copy_from_slice(&r[0x4D0..0x4D6]);
                for i in 0..9 {
                    k.body[i / 3][i % 3] = i16::from_le_bytes([r[0x18C + 2 * i], r[0x18D + 2 * i]]);
                    k.vmat[i / 3][i % 3] = i16::from_le_bytes([r[0x1AC + 2 * i], r[0x1AD + 2 * i]]);
                }
                k
            }

            /// Write the owned fields into `r` (a copy of the original record).
            pub fn to_record(&self, r: &mut [u8]) {
                assert!(r.len() >= RECORD_SIZE);
                $( r[$off..$off + std::mem::size_of::<$ty>()].copy_from_slice(&self.$name.to_le_bytes()); )*
                for (i, w) in self.wheels.iter().enumerate() {
                    w.write(r, 0x14 + 0x2C * i);
                }
                for i in 0..10 {
                    r[0x35C + 4 * i..0x360 + 4 * i].copy_from_slice(&self.lap_time[i].to_le_bytes());
                }
                for i in 0..18 {
                    r[0x3A0 + 4 * i..0x3A4 + 4 * i].copy_from_slice(&self.sway[i].to_le_bytes());
                    r[0x414 + 2 * i..0x416 + 2 * i].copy_from_slice(&self.shape[i].to_le_bytes());
                }
                r[0x4D0..0x4D6].copy_from_slice(&self.weapons);
                for i in 0..9 {
                    r[0x18C + 2 * i..0x18E + 2 * i].copy_from_slice(&self.body[i / 3][i % 3].to_le_bytes());
                    r[0x1AC + 2 * i..0x1AE + 2 * i].copy_from_slice(&self.vmat[i / 3][i % 3].to_le_bytes());
                }
            }
        }
    };
}

pub const RECORD_SIZE: usize = 0x504;
/// Wheel blocks a record can hold (+0x14 .. +0x11C).
pub const MAX_WHEELS: usize = 6;
/// Kart array in RAM [CODE 0x8007916C].
pub const KART_ARRAY: u32 = 0x800C_30A8;

/// One wheel block (+0x14 + 0x2C * i). Only the fields the update touches.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Wheel {
    /// +0x1E: rolling angle, 12-bit [CODE 0x80067CA0].
    pub spin: i16,
    /// +0x20: rolling rate per sub-step [CODE 0x80067BCC].
    pub spin_rate: i16,
    /// +0x22: wheel radius (divides forward speed) [CODE 0x80067B7C].
    pub radius: i16,
    /// +0x26: flags: bit0 driven, 0x38 locked/spinning, 0xC0 off [CODE 0x80067A28].
    pub flags: u16,
}

impl Wheel {
    fn read(r: &[u8], b: usize) -> Wheel {
        let h = |o: usize| i16::from_le_bytes([r[b + o], r[b + o + 1]]);
        Wheel { spin: h(0x1E), spin_rate: h(0x20), radius: h(0x22), flags: h(0x26) as u16 }
    }
    fn write(&self, r: &mut [u8], b: usize) {
        r[b + 0x1E..b + 0x20].copy_from_slice(&self.spin.to_le_bytes());
        r[b + 0x20..b + 0x22].copy_from_slice(&self.spin_rate.to_le_bytes());
    }
}

kart_fields! {
    /// Position, world units (TER units << 5); y grows downward.
    x: i32 @ 0x000, y: i32 @ 0x004, z: i32 @ 0x008,
    /// Body angles, 12-bit. heading: forward = (sin h, cos h) in (x, z).
    pitch: i16 @ 0x00C, heading: i16 @ 0x00E, roll: i16 @ 0x010,
    /// Corner offsets of the four ground probes (FUN_8003E91C).
    body_front: i16 @ 0x164, body_rear: i16 @ 0x166, body_left: i16 @ 0x168, body_right: i16 @ 0x16A,
    wheel_count: u8 @ 0x170,
    /// Non-zero: skid / engine effects audible.
    engine_audible: u8 @ 0x171,
    /// Engine-sound class (one bit set per class).
    vehicle_class: u8 @ 0x173,
    /// bit1: floats (hover / boat handling).
    chassis_flags: u8 @ 0x174,
    /// Wall bounce strength (FUN_8003D240 via 0x800B0C30).
    wall_bounce: u8 @ 0x175,
    spark_offset: i16 @ 0x176,
    /// Handling numbers rebuilt every tick from the vehicle stats.
    steer_centre: i16 @ 0x178, accel: i16 @ 0x17A, top_speed: i16 @ 0x17C,
    steer_rate: i16 @ 0x17E, stat_d: i16 @ 0x180,
    ride_height: i16 @ 0x184,
    /// Template halfwords the effect of special 16 scales (FUN_8006C388).
    f188: i16 @ 0x188, f18a: i16 @ 0x18A,
    /// Written by code outside the kart update (render side); read by the
    /// body-sway target at 0x80068128.
    aim_x: i32 @ 0x2C0, aim_y: i32 @ 0x2C4, aim_z: i32 @ 0x2C8,
    /// Squared distance (>> 10 units) to the current aim target; reset to
    /// 0x7FFFFFFF each frame by FUN_800418D8.
    aim_dist: i32 @ 0x2CC,
    /// Smoothed ground normal (eased toward the floor face normal).
    normal_x: i16 @ 0x2D0, normal_y: i16 @ 0x2D2, normal_z: i16 @ 0x2D4,
    /// Normal used for the body orientation.
    tilt_nx: i16 @ 0x2D6, tilt_ny: i16 @ 0x2D8, tilt_nz: i16 @ 0x2DA,
    /// RAM pointer to this kart's position history ring (0 = none).
    history_ptr: u32 @ 0x2DC,
    /// RAM pointers to the floor face and water face found by the ground query.
    floor_face_ptr: u32 @ 0x2E0, water_face_ptr: u32 @ 0x2E4,
    prev_x: i32 @ 0x2E8, prev_y: i32 @ 0x2EC, prev_z: i32 @ 0x2F0,
    /// World velocity per sub-step.
    vx: i32 @ 0x2F4, vy: i32 @ 0x2F8, vz: i32 @ 0x2FC,
    /// Effect parameters (+0x300..+0x318): the homing target of status bits
    /// 0x20 / 0x80, the spin of 0x40, and the effects' own values
    /// (FUN_8006B828 sets them, FUN_8006C388 steps them).
    target_x: i32 @ 0x300, target_yaw: i32 @ 0x304, target_z: i32 @ 0x308,
    eff_30c: i32 @ 0x30C, eff_310: i32 @ 0x310, eff_314: i32 @ 0x314, eff_318: i32 @ 0x318,
    /// Ground height under the kart and the support height (ground or water).
    ground_y: i32 @ 0x31C, support_y: i32 @ 0x320,
    /// Written outside the kart update; read by the body-sway target.
    look_x: i32 @ 0x324, look_y: i32 @ 0x328, look_z: i32 @ 0x32C,
    /// Driver position (FUN_80025D8C): x, y (`ref_y`), z. Pickups and
    /// projectile hits test it.
    drv_x: i32 @ 0x330,
    /// +0x334 / +0x33C / +0x344: the driver position before the last
    /// render pass (FUN_80025D8C copies +0x330.. here first); the jets
    /// (FUN_80063C24) read them.
    drv_prev_x: i32 @ 0x334, drv_prev_y: i32 @ 0x33C, drv_prev_z: i32 @ 0x344,
    ref_y: i32 @ 0x338,
    drv_z: i32 @ 0x340,
    /// Suspension bounce: offset, velocity, impulse.
    bounce: i32 @ 0x348, bounce_vel: i32 @ 0x34C, bounce_kick: i32 @ 0x350,
    /// Status bits (homing, frozen, ...); 0 for the player on DOCK.
    status: u32 @ 0x354,
    race_time: i32 @ 0x358,
    /// First lap time (+0x384), set at the first lap end.
    best_lap: i32 @ 0x384,
    progress: i32 @ 0x388,
    /// Nearest course point (DAT chunk 5) for the race progress (FUN_800837F8).
    course_point: i16 @ 0x488,
    finished_time: i32 @ 0x38C,
    /// Velocity in the kart frame: forward and sideways.
    fwd_speed: i32 @ 0x390, side_speed: i32 @ 0x394,
    /// Water surface height (low byte kept between ticks).
    water_y: i32 @ 0x398,
    /// +0x39C: the mirror plane's coordinate (FUN_80062E38).
    mirror_at: i32 @ 0x39C,
    /// Ground speed: |position delta| / dt.
    /// FUN_8009004C: track width at the nearest path point (bytes * -8 / << 3)
    /// and the signed lateral offset from it.
    path_width_l: i32 @ 0x3F4, path_width_r: i32 @ 0x3F8, path_lateral: i32 @ 0x3FC,
    speed_est: i32 @ 0x400,
    /// AI steering target (+0x3E8.., TER units; FUN_80090D00) and the special
    /// move's path point (world units; FUN_800913F4).
    ai_tx: i32 @ 0x3E8, ai_ty: i32 @ 0x3EC, ai_tz: i32 @ 0x3F0,
    /// Special move (+0x4D6): the chase point that follows the path
    /// (+0x404..) and the path position of its target (+0x49C / +0x49E).
    sp_x: i32 @ 0x404, sp_y: i32 @ 0x408, sp_z: i32 @ 0x40C,
    sp_seg: i16 @ 0x49C, sp_pt: i16 @ 0x49E,
    /// Kart-kart contact bits: other karts' `own_bit` this kart overlaps.
    hit_mask: u16 @ 0x410,
    /// This kart's bit (1 << slot).
    own_bit: u16 @ 0x412,
    hits_dealt: i16 @ 0x43C,
    hit_dir: i16 @ 0x444,
    probe_top: i16 @ 0x438, probe_bottom: i16 @ 0x43A,
    stun: i16 @ 0x43E,
    impact: i16 @ 0x440,
    /// Engine/scale factor eased toward 0x1000.
    engine: i16 @ 0x442,
    boost_count: i16 @ 0x446,
    /// Heading change over the tick / dt.
    yaw_rate: i16 @ 0x448,
    prev_vy: i16 @ 0x44A,
    blink_timer: i16 @ 0x44C,
    buttons: u16 @ 0x44E,
    lean_a: i16 @ 0x450, lean_b: i16 @ 0x452,
    sway_a: i16 @ 0x454, sway_b: i16 @ 0x456, sway_c: i16 @ 0x458, sway_d: i16 @ 0x45A, sway_e: i16 @ 0x45C,
    /// External push applied to the velocity next tick, cleared after use.
    push_x: i16 @ 0x45E, gravity_mod: i16 @ 0x460, push_z: i16 @ 0x462,
    sink: i16 @ 0x464, sink_target: i16 @ 0x466,
    homing_timer: i16 @ 0x468,
    slope_roll: i16 @ 0x470,
    /// Steering amount (signed, +-8 * steer_rate).
    steer: i16 @ 0x472,
    slope_turn: i16 @ 0x474,
    conveyor_dir: i16 @ 0x476,
    max_hit: i16 @ 0x478,
    respawn_timer: i16 @ 0x47A,
    steer_lag: i16 @ 0x47C,
    boost_glow: i16 @ 0x47E,
    /// Throttle/brake pitch wobble.
    lurch: i16 @ 0x480,
    spin_rate: i16 @ 0x482,
    spin_timer: i16 @ 0x484,
    drift: i16 @ 0x486,
    /// +0x490: the stunt course's strip quad under the kart (FUN_8006F64C;
    /// FUN_8006F410 at the race load).
    strip: i16 @ 0x490,
    contacts_from_above: i16 @ 0x492,
    contacts_below: i16 @ 0x494,
    start_timer: i16 @ 0x48A,
    wobble: i16 @ 0x48C,
    /// Direction of the track ahead (FUN_80086B7C).
    track_dir: i16 @ 0x48E,
    flags: u16 @ 0x496,
    path_seg: i16 @ 0x498, path_pt: i16 @ 0x49A,
    /// Model slot numbers of the vehicle / driver (-1: none).
    model_slot: i8 @ 0x4A4, char_slot: i8 @ 0x4A5,
    shape_t: u8 @ 0x4A6, shape_n: u8 @ 0x4A7,
    /// +0x4A8: the stunt camera's fraction along the strip quad (0..0x80,
    /// 0x800B0AE8 of FUN_8006FB2C); +0x4A9: across it (0x40 at the load).
    strip_along: u8 @ 0x4A8, strip_across: u8 @ 0x4A9,
    throttle_hold: u8 @ 0x4AA, coast_hold: u8 @ 0x4AB, brake_hold: u8 @ 0x4AF,
    /// ADVENTURE (phase 6, FUN_8007BBF0): targets, item boxes and turbo
    /// tokens this kart collected.
    adv_targets: u8 @ 0x4AC, adv_boxes: u8 @ 0x4AD, adv_tokens: u8 @ 0x4AE,
    blink_count: u8 @ 0x4B0,
    vehicle: u8 @ 0x4B1,
    character: u8 @ 0x4B2,
    wall_flag: u8 @ 0x4B3,
    pickup_timer: u8 @ 0x4B5,
    /// +0x4B6: the stunt pipe's clock (FUN_800628FC, 0..180).
    pipe_clock: u8 @ 0x4B6,
    rumble: u8 @ 0x4B7,
    start_hold: u8 @ 0x4B8,
    accel_spin: u8 @ 0x4B9,
    ceiling_hit: u8 @ 0x4BA,
    surf_b5: u8 @ 0x4BB, surf_b6: u8 @ 0x4BC, surf_b7: u8 @ 0x4BD,
    race_pos: u8 @ 0x4BE,
    lap: u8 @ 0x4BF,
    line_side: u8 @ 0x4C0, line_flag: u8 @ 0x4C1,
    /// Catch-up (rubber band) 0..180.
    catchup: u8 @ 0x4C3,
    boosting: u8 @ 0x4C4,
    below_ground: u8 @ 0x4C5,
    /// +0x4C2: the mirror plane's kind (FUN_80062E38: 0 z, 1 floor, 2 x).
    mirror_kind: u8 @ 0x4C2,
    /// +0x4C6: stunt-course gates passed (FUN_80081DD8).
    gate: u8 @ 0x4C6,
    finished: u8 @ 0x4C7,
    /// 0 on the ground, 1 just above water / ground, 2 in the water.
    float_state: u8 @ 0x4C8,
    reversing: u8 @ 0x4C9,
    /// Race position the HUD shows (follows +0x4BE when a change flashes).
    shown_pos: u8 @ 0x4CA,
    path_flag_4cb: u8 @ 0x4CB,
    /// Shoulder-button latches: L1, L2, R1, R2.
    latch_l1: u8 @ 0x4CC, latch_l2: u8 @ 0x4CD, latch_r1: u8 @ 0x4CE, latch_r2: u8 @ 0x4CF,
    special: u8 @ 0x4D6,
    player_index: i8 @ 0x4D7,
    copy_hold: u8 @ 0x4D8,
    wall_hit: u8 @ 0x4D9,
    copy_button: i8 @ 0x4DA,
    splash: u8 @ 0x4DB,
    nearest_kart: i8 @ 0x4DC,
    event_bits: u8 @ 0x4DD,
    engine_rev: u8 @ 0x4DE,
    path_mode: u8 @ 0x4DF,
    /// Ticks in contact with the ground (saturates at 0x40); 0 = airborne.
    ground_contact: u8 @ 0x4E0,
    sliding: u8 @ 0x4E1,
    jump_state: i8 @ 0x4E2,
    object_e3: i8 @ 0x4E3, object_e4: i8 @ 0x4E4, obj_e5: u8 @ 0x4E5,
    pickup: i8 @ 0x4E6, item_e7: i8 @ 0x4E7, carried: i8 @ 0x4E8, carried_drop: u8 @ 0x4E9,
    rumble_a: i8 @ 0x4EA, rumble_b: i8 @ 0x4EB,
    skid: u8 @ 0x4ED, item_ee: i8 @ 0x4EE, pickup_state: u8 @ 0x4EF,
    blink: u8 @ 0x4F0,
    effect: i8 @ 0x4F1,
    camera: i8 @ 0x4F2,
    surface: i8 @ 0x4F3,
    /// Colour of the last turbo token collected (FUN_8007BBF0).
    token_colour: u8 @ 0x4F4,
    conveyor: u8 @ 0x4F6, conveyor_prev: u8 @ 0x4F7, object_f8: i8 @ 0x4F8,
    pause_bits: u8 @ 0x4F9,
    knockouts: u8 @ 0x4FB,
    boost_flash: u8 @ 0x4FA,
    flash_timer: i8 @ 0x4FC,
    /// TRIANGLE latch and the kart TRIANGLE locks the camera onto.
    item_fire: u8 @ 0x4FD, look_target: u8 @ 0x4FE,
    path_side: u8 @ 0x500,
    close_kart: u8 @ 0x501,
    on_ground: u8 @ 0x502,
    /// Position-change flash, VBlanks (+0x4FF).
    pos_flash: u8 @ 0x4FF,
    warn_timer: i8 @ 0x503,
}

impl KartState {
    /// One mismatch line per differing owned scalar field.
    pub fn diff(&self, other: &KartState) -> Vec<String> {
        let mut a = vec![0u8; RECORD_SIZE];
        let mut b = vec![0u8; RECORD_SIZE];
        self.to_record(&mut a);
        other.to_record(&mut b);
        diff_records(&a, &b)
    }
}

/// Compare two records on the owned bytes only.
pub fn diff_records(a: &[u8], b: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    for &(name, off, n) in FIELD_OFFSETS {
        if a[off..off + n] != b[off..off + n] {
            out.push(format!("{name}@{off:03X}: {} vs {}", val(&a[off..off + n]), val(&b[off..off + n])));
        }
    }
    let arr = |out: &mut Vec<String>, name: &str, base: usize, n: usize, w: usize| {
        for i in 0..n {
            let o = base + w * i;
            if a[o..o + w] != b[o..o + w] {
                out.push(format!("{name}[{i}]@{o:03X}: {} vs {}", val(&a[o..o + w]), val(&b[o..o + w])));
            }
        }
    };
    for k in 0..4 {
        arr(&mut out, &format!("wheel{k}.spin"), 0x14 + 0x2C * k + 0x1E, 2, 2);
    }
    arr(&mut out, "lap_time", 0x35C, 10, 4);
    arr(&mut out, "sway", 0x3A0, 18, 4);
    arr(&mut out, "shape", 0x414, 18, 2);
    arr(&mut out, "weapons", 0x4D0, 6, 1);
    out
}

fn val(b: &[u8]) -> String {
    match b.len() {
        1 => format!("{}", b[0] as i8),
        2 => format!("{}", i16::from_le_bytes([b[0], b[1]])),
        _ => format!("{}", i32::from_le_bytes([b[0], b[1], b[2], b[3]])),
    }
}

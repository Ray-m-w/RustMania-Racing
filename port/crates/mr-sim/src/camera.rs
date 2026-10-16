//! Player chase camera: FUN_80070418(camera = 0x800C4F40, kart = 0x800C30A8),
//! called once per race tick from the race frame (FUN_8007916C @0x8007A1B8)
//! after every kart has been updated, plus the view setup the renderer does
//! with the result (FUN_8002CEBC, called from FUN_80078F38 before the world
//! is drawn). Spec: docs/spec/camera.md.
//!
//! Scope: the modes of a one-player race: 10 (intro, countdown >= 300),
//! 0 (chase) and 2 (trackside cameras after the finish). The other camera
//! modes (pre-race orbit, finish orbit, replay, cut-scene sync, boss arenas,
//! overhead, split screen) are recognised by the same mode selection as the
//! game and panic through `unreachable_in_scope!` so a test notices if a
//! route reaches them.
//!
//! Fixed point exactly as the game: angles 4096 = 360 degrees, matrices 4.12,
//! MIPS `div` truncation, 32-bit wrapping products, arithmetic shifts.
//! Constant tables (sine/cosine, ratan2, square root) are read from the
//! executable at run time ([`CameraTables::from_exe`]); nothing is pasted in.

#![allow(dead_code)]

/// Panics on a branch that the solo DOCK race never takes. Kept as a macro
/// so the reason string is greppable.
macro_rules! unreachable_in_scope {
    ($why:expr) => {
        panic!("camera: unreachable in scope: {}", $why)
    };
}

// ---------------------------------------------------------------------------
// Addresses (all inside the PS-X EXE text/data, t_addr 0x80010000).
// ---------------------------------------------------------------------------

/// PS-X EXE load address and header size: file offset = addr - T_ADDR + 0x800.
pub const EXE_T_ADDR: u32 = 0x8001_0000;
pub const EXE_HEADER: usize = 0x800;
/// Sine table, i16, 1.14 fixed point (16384 = 1.0), read by FUN_80024AB4 and
/// FUN_80070418 as `table[angle]`. 5120 entries are used: the cosine pointer
/// (DAT_800AFF20) points 1024 entries in, and is indexed up to 4095.
pub const ADDR_SIN_TABLE: u32 = 0x800A_88C0;
pub const SIN_TABLE_LEN: usize = 5120;
/// Pointer to the cosine table (= ADDR_SIN_TABLE + 0x800 in this exe).
pub const ADDR_COS_PTR: u32 = 0x800A_FF20;
/// PsyQ ratan2 table (ratan2 @0x80095010), i16, 1025 entries.
pub const ADDR_RATAN_TABLE: u32 = 0x800A_D608;
pub const RATAN_TABLE_LEN: usize = 1025;
/// PsyQ SquareRoot0 table (@0x80093D70), i16, indexed (mantissa - 64), 192 entries.
pub const ADDR_SQRT_TABLE: u32 = 0x800A_D2E8;
pub const SQRT_TABLE_LEN: usize = 192;

/// Camera record base (player 0) and stride (player 1 at +0x70).
pub const ADDR_CAMERA0: u32 = 0x800C_4F40;
pub const CAMERA_SIZE: usize = 0x70;
/// Entries in the game's kart position history ring (kart+0x2DC points at
/// it): 12 x 16 bytes. The oracle's RAM and every comparison use these.
pub const GAME_TRAIL_LEN: usize = 12;
/// Entries in the sim's ring: longer than the game's so the chase camera's
/// 16-VBlank walk back (mode 0) still finds its point at dt = 1 (60 Hz logic:
/// 12 entries cover only 12 VBlanks there). At dt >= 2 the walk ends within
/// the first 9 entries, so entries 12.. are never read and dt = 2 / 3 results
/// are unchanged.
pub const TRAIL_LEN: usize = 24;

/// GTE projection plane distance H, set once by SetGeomScreen(160) in
/// FUN_800430F8 @0x8004311C (InitGeom's default 1000 is overwritten there).
pub const GTE_H: i32 = 160;
/// GTE screen offset (OFX, OFY) for a one-player race: SetGeomOffset(256, 120)
/// in FUN_80029F40 @0x80029FF8. Two-player halves use (256, 180) / (256, 60).
pub const GTE_OFFSET_1P: (i32, i32) = (256, 120);
/// Frame buffer drawn into: 512 x 240.
pub const SCREEN_W: i32 = 512;
pub const SCREEN_H: i32 = 240;

// ---------------------------------------------------------------------------
// Tables
// ---------------------------------------------------------------------------

/// The executable's constant tables the camera needs.
#[derive(Clone, Debug)]
pub struct CameraTables {
    /// 1.14 sine, 5120 entries; `cos[a] = sin[a + cos_offset]`.
    pub sin: Vec<i16>,
    /// Offset of the cosine table in entries (read from DAT_800AFF20).
    pub cos_offset: usize,
    pub ratan: Vec<i16>,
    pub sqrt: Vec<i16>,
}

fn rd_u32(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}

fn rd_i16s(b: &[u8], o: usize, n: usize) -> Option<Vec<i16>> {
    let s = b.get(o..o + 2 * n)?;
    Some(s.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect())
}

impl CameraTables {
    /// From the PS-X EXE file bytes (SLUS_012.37 on the disc).
    pub fn from_exe(exe: &[u8]) -> Option<Self> {
        if exe.get(0..8)? != b"PS-X EXE" {
            return None;
        }
        Self::from_mapper(exe, |a| (a.checked_sub(EXE_T_ADDR)? as usize).checked_add(EXE_HEADER))
    }

    /// From a 2 MB main-RAM image (e.g. an oracle ram0.bin).
    pub fn from_ram(ram: &[u8]) -> Option<Self> {
        Self::from_mapper(ram, |a| Some((a & 0x1F_FFFF) as usize))
    }

    fn from_mapper(b: &[u8], off: impl Fn(u32) -> Option<usize>) -> Option<Self> {
        let sin = rd_i16s(b, off(ADDR_SIN_TABLE)?, SIN_TABLE_LEN)?;
        let cos_ptr = rd_u32(b, off(ADDR_COS_PTR)?)?;
        let cos_offset = (cos_ptr.checked_sub(ADDR_SIN_TABLE)? / 2) as usize;
        if cos_offset + 4096 > SIN_TABLE_LEN {
            return None;
        }
        let ratan = rd_i16s(b, off(ADDR_RATAN_TABLE)?, RATAN_TABLE_LEN)?;
        let sqrt = rd_i16s(b, off(ADDR_SQRT_TABLE)?, SQRT_TABLE_LEN)?;
        Some(CameraTables { sin, cos_offset, ratan, sqrt })
    }

    /// Raw 1.14 sine for a 12-bit angle (0..4095).
    #[inline]
    fn sin14(&self, a: i32) -> i32 {
        self.sin[(a & 0xFFF) as usize] as i32
    }
    #[inline]
    fn cos14(&self, a: i32) -> i32 {
        self.sin[(a & 0xFFF) as usize + self.cos_offset] as i32
    }
    /// The game always uses the table as `(s16 entry) >> 2` = 4.12.
    #[inline]
    pub fn sin12(&self, a: i32) -> i32 {
        self.sin14(a) >> 2
    }
    #[inline]
    pub fn cos12(&self, a: i32) -> i32 {
        self.cos14(a) >> 2
    }

    /// PsyQ `SquareRoot0` (@0x80093D70): GTE leading-zero count + table.
    /// LZCR counts leading sign bits, so a negative input behaves like the
    /// hardware does (the camera only passes non-negative sums).
    pub fn sqrt0(&self, a: i32) -> i32 {
        if a < 0 {
            // A sum past 2^31 (phase 6: ADVENTURE's first frame, the camera
            // still at the session's origin with the kart far away): the
            // game's table read lands before the table (no defined value);
            // the port takes the unsigned root, a far distance [INFER].
            return (a as u32 as f64).sqrt() as i32;
        }
        let lz = if a >= 0 { (a as u32).leading_zeros() } else { (!(a as u32)).leading_zeros() } as i32;
        if lz == 32 {
            return 0;
        }
        let even = lz & !1;
        let shift_out = (31 - even) >> 1;
        let m = if even - 24 >= 0 { a.wrapping_shl((even - 24) as u32) } else { a >> (24 - even) };
        let idx = (m - 64) as usize;
        let t = self.sqrt[idx] as i32;
        ((t.wrapping_shl(shift_out as u32)) as u32 >> 12) as i32
    }

    /// PsyQ `ratan2(y, x)` (@0x80095010): 4096 = 360 degrees, result in
    /// -2048..=2048.
    pub fn ratan2(&self, y: i32, x: i32) -> i32 {
        let (mut y, mut x) = (y, x);
        let neg_x = x < 0;
        if neg_x {
            x = x.wrapping_neg();
        }
        let neg_y = y < 0;
        if neg_y {
            y = y.wrapping_neg();
        }
        if x == 0 && y == 0 {
            return 0;
        }
        let mut v = if y < x {
            let t = if y & 0x7FE0_0000 != 0 { mips_div(y, x >> 10) } else { mips_div(y << 10, x) };
            self.ratan[t as usize] as i32
        } else {
            let t = if x & 0x7FE0_0000 != 0 { mips_div(x, y >> 10) } else { mips_div(x << 10, y) };
            1024 - self.ratan[t as usize] as i32
        };
        if neg_x {
            v = 2048 - v;
        }
        if neg_y {
            v = -v;
        }
        v
    }
}

/// MIPS `div` quotient (truncates toward zero; the game guards /0 with a
/// break, which we treat as a bug).
#[inline]
fn mips_div(a: i32, b: i32) -> i32 {
    assert!(b != 0, "camera: division by zero");
    a.wrapping_div(b)
}

/// `(v < 0 ? v + (2^n - 1) : v) >> n`: the compiler's signed division by 2^n.
#[inline]
fn div_pow2(v: i32, n: u32) -> i32 {
    if v < 0 {
        v.wrapping_add((1 << n) - 1) >> n
    } else {
        v >> n
    }
}

/// Iterations of the camera's per-VBlank loops: they are do-while loops on
/// the 16-bit dt (`do { .. } while ((s16)--t > 0)`), so they run at least once.
#[inline]
fn reps(dt: i32) -> i32 {
    let d = dt as i16 as i32;
    if d >= 1 {
        d
    } else {
        1
    }
}

/// Wrap an angle difference into -2048..=2048 the way the camera does
/// (`d > 2048 -> d - 4096; d < -2048 -> d + 4096`).
#[inline]
fn wrap_delta(d: i32) -> i32 {
    if d > 2048 {
        d - 4096
    } else if d < -2048 {
        d + 4096
    } else {
        d
    }
}

/// Step a smoothing weight toward its target by 32 without overshooting
/// (the three identical blocks at 0x80071E0C..0x80071F14 / 0x80072280).
#[inline]
fn approach32(w: i16, target: i16) -> i16 {
    if w > target {
        let n = w.wrapping_sub(32);
        if n < target {
            target
        } else {
            n
        }
    } else if w < target {
        let n = w.wrapping_add(32);
        if target < n {
            target
        } else {
            n
        }
    } else {
        w
    }
}

// ---------------------------------------------------------------------------
// Camera record
// ---------------------------------------------------------------------------

/// The 0x70-byte camera record at 0x800C4F40. Offsets in the field docs.
/// Bytes the camera never touches are kept verbatim in `raw`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CameraState {
    /// +0x00: 3x3 i16 4.12 camera rotation, written by FUN_80024AB4. Rows are
    /// the camera axes in world space: right, down, forward. The renderer
    /// copies this whole 32-byte MATRIX to the view matrix 0x800C3008.
    pub matrix: [[i16; 3]; 3],
    /// +0x14: MATRIX translation (never written by the camera; zero in a race).
    pub matrix_t: [i32; 3],
    /// +0x20: forward vector, 4.12 (equals matrix row 2 up to table rounding).
    pub forward: [i16; 3],
    /// +0x28: eye position, world units (same space as kart+0x00).
    pub pos: [i32; 3],
    /// +0x34: eye >> 5 taken before the height pass (y is the unclamped goal).
    pub pos_div32: [i32; 3],
    /// +0x48: roll reference for the finish orbit (mode 10 only).
    pub roll_ref: i32,
    /// +0x4C: orbit angle (modes 1 and 11 only).
    pub orbit_angle: i32,
    /// +0x50 pitch, +0x52 yaw, +0x54 roll; 12-bit angles kept in 0..4095.
    pub pitch: i16,
    /// +0x58: the stunt course menu's sweep phase (mode 9).
    pub sweep: i16,
    pub yaw: i16,
    pub roll: i16,
    /// +0x5C/+0x5E/+0x60: per-axis follow weights (out of 1024 per VBlank).
    pub weight: [i16; 3],
    /// +0x64: camera mode (0 = in-race chase).
    pub mode: i16,
    /// +0x66 / +0x68: scripted sub-state and its countdown (VBlanks).
    pub sub_state: i16,
    pub sub_timer: i16,
    /// +0x6B / +0x6C: random direction bits drawn on a mode change.
    pub rand_a: u8,
    pub rand_b: u8,
    /// +0x6E: 1 on the tick the mode changed.
    pub mode_changed: u8,
    /// The whole record as read; untouched bytes are written back from here.
    pub raw: [u8; CAMERA_SIZE],
}

fn g_i16(b: &[u8], o: usize) -> i16 {
    i16::from_le_bytes([b[o], b[o + 1]])
}
fn g_i32(b: &[u8], o: usize) -> i32 {
    i32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn p_i16(b: &mut [u8], o: usize, v: i16) {
    b[o..o + 2].copy_from_slice(&v.to_le_bytes());
}
fn p_i32(b: &mut [u8], o: usize, v: i32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}

impl CameraState {
    pub fn from_bytes(b: &[u8]) -> Self {
        assert!(b.len() >= CAMERA_SIZE);
        let mut raw = [0u8; CAMERA_SIZE];
        raw.copy_from_slice(&b[..CAMERA_SIZE]);
        let mut matrix = [[0i16; 3]; 3];
        for r in 0..3 {
            for c in 0..3 {
                matrix[r][c] = g_i16(b, 2 * (3 * r + c));
            }
        }
        CameraState {
            matrix,
            matrix_t: [g_i32(b, 0x14), g_i32(b, 0x18), g_i32(b, 0x1C)],
            forward: [g_i16(b, 0x20), g_i16(b, 0x22), g_i16(b, 0x24)],
            pos: [g_i32(b, 0x28), g_i32(b, 0x2C), g_i32(b, 0x30)],
            pos_div32: [g_i32(b, 0x34), g_i32(b, 0x38), g_i32(b, 0x3C)],
            roll_ref: g_i32(b, 0x48),
            orbit_angle: g_i32(b, 0x4C),
            pitch: g_i16(b, 0x50),
            sweep: g_i16(b, 0x58),
            yaw: g_i16(b, 0x52),
            roll: g_i16(b, 0x54),
            weight: [g_i16(b, 0x5C), g_i16(b, 0x5E), g_i16(b, 0x60)],
            mode: g_i16(b, 0x64),
            sub_state: g_i16(b, 0x66),
            sub_timer: g_i16(b, 0x68),
            rand_a: b[0x6B],
            rand_b: b[0x6C],
            mode_changed: b[0x6E],
            raw,
        }
    }

    pub fn to_bytes(&self) -> [u8; CAMERA_SIZE] {
        let mut b = self.raw;
        for r in 0..3 {
            for c in 0..3 {
                p_i16(&mut b, 2 * (3 * r + c), self.matrix[r][c]);
            }
        }
        for i in 0..3 {
            p_i32(&mut b, 0x14 + 4 * i, self.matrix_t[i]);
            p_i16(&mut b, 0x20 + 2 * i, self.forward[i]);
            p_i32(&mut b, 0x28 + 4 * i, self.pos[i]);
            p_i32(&mut b, 0x34 + 4 * i, self.pos_div32[i]);
            p_i16(&mut b, 0x5C + 2 * i, self.weight[i]);
        }
        p_i32(&mut b, 0x48, self.roll_ref);
        p_i32(&mut b, 0x4C, self.orbit_angle);
        p_i16(&mut b, 0x50, self.pitch);
        p_i16(&mut b, 0x52, self.yaw);
        p_i16(&mut b, 0x54, self.roll);
        p_i16(&mut b, 0x58, self.sweep);
        p_i16(&mut b, 0x64, self.mode);
        p_i16(&mut b, 0x66, self.sub_state);
        p_i16(&mut b, 0x68, self.sub_timer);
        b[0x6B] = self.rand_a;
        b[0x6C] = self.rand_b;
        b[0x6E] = self.mode_changed;
        b
    }
}

// ---------------------------------------------------------------------------
// Inputs
// ---------------------------------------------------------------------------

/// One entry of the kart's position history (16 bytes at *(kart+0x2DC)).
/// The kart update (FUN_80064B5C @0x8006529C) shifts the 12 entries down by
/// one and writes `{kart position, dt}` into entry 0 every tick while
/// 0x800B0C70 == 0; see [`push_trail`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TrailEntry {
    pub pos: [i32; 3],
    /// VBlanks this entry covers (= dt when it was pushed).
    pub count: i32,
}

/// The kart update's history push (FUN_80064B5C @0x8006529C..0x80065324).
pub fn push_trail(trail: &mut [TrailEntry; TRAIL_LEN], kart_pos: [i32; 3], dt: i32) {
    for i in (1..TRAIL_LEN).rev() {
        trail[i] = trail[i - 1];
    }
    trail[0] = TrailEntry { pos: kart_pos, count: dt };
}

/// The kart fields the camera reads (offsets into the 0x504-byte record).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CameraKart {
    /// +0x000 position.
    pub pos: [i32; 3],
    /// +0x182 / +0x184 / +0x186 / +0x188: camera mount (i16). The look-at
    /// point is pos + M * (-m0, m1 - m3, -m2) >> 7 and the eye is raised by
    /// (m1 - m3) << 5. Kermit on DOCK: (0, -145, 5, 145).
    pub mount: [i16; 4],
    /// +0x18C body rotation matrix, 4.12, row-major.
    pub matrix: [[i16; 3]; 3],
    /// *(kart+0x2DC): position history, or None when the pointer is null.
    pub trail: Option<[TrailEntry; TRAIL_LEN]>,
    /// +0x394 i32 (yaw rate) and +0x470 i16: roll lean = (0x470 + (0x394 >> 3)) / 2.
    pub yaw_rate: i32,
    pub lean: i16,
    /// +0x4C7 u8: nonzero switches the 1P race camera out of chase (finish).
    pub finished: u8,
    /// +0x4D6 i8: nonzero selects the "airborne" roll rule (roll = -3 * yaw error).
    pub roll_mode: i8,
    /// +0x4FE i8: -1 normally; otherwise the camera watches another object.
    pub watch_target: i8,
    /// +0x4BF u8: laps completed (the finish rule compares it with 0x800B0DA6).
    pub lap: u8,
    /// +0x38C i32 and +0x440 i16: the finish rule's "still driving" tests
    /// (+0x38C < 181 keeps the chase camera; +0x440 >= 1000 the orbit).
    pub finish_wait: i32,
    pub finish_score: i16,
    /// +0x498 / +0x49A: TER path segment / point (FUN_8009004C).
    pub path_seg: i16,
    pub path_pt: i16,
    /// +0x4DF: nonzero = the kart runs the course backward (path walks use
    /// FUN_80090670 instead of FUN_80090424).
    pub path_backward: u8,
    /// +0x490: the stunt course's strip quad (mode 3).
    pub strip: i16,
}

impl CameraKart {
    /// Read from a kart record. The trail is not inside the record (it is a
    /// pointer at +0x2DC); pass it separately.
    pub fn from_record(rec: &[u8], trail: Option<[TrailEntry; TRAIL_LEN]>) -> Self {
        let mut matrix = [[0i16; 3]; 3];
        for r in 0..3 {
            for c in 0..3 {
                matrix[r][c] = g_i16(rec, 0x18C + 2 * (3 * r + c));
            }
        }
        CameraKart {
            pos: [g_i32(rec, 0), g_i32(rec, 4), g_i32(rec, 8)],
            mount: [g_i16(rec, 0x182), g_i16(rec, 0x184), g_i16(rec, 0x186), g_i16(rec, 0x188)],
            matrix,
            trail,
            yaw_rate: g_i32(rec, 0x394),
            lean: g_i16(rec, 0x470),
            finished: rec[0x4C7],
            roll_mode: rec[0x4D6] as i8,
            watch_target: rec[0x4FE] as i8,
            lap: rec[0x4BF],
            finish_wait: g_i32(rec, 0x38C),
            finish_score: g_i16(rec, 0x440),
            path_seg: g_i16(rec, 0x498),
            path_pt: g_i16(rec, 0x49A),
            path_backward: rec[0x4DF],
            strip: g_i16(rec, 0x490),
        }
    }
}

/// Race globals the camera reads (addresses in the field docs).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CameraGlobals {
    /// 0x800B0AEC: VBlanks per tick (2 in a race).
    pub dt: i32,
    /// 0x800B0C70: the race-start countdown (480 at race load, 0 from GO).
    pub countdown: i32,
    /// 0x800B07F8 (i16): 300; while countdown >= this the intro camera
    /// (mode 10) runs, below it the start game (chase camera).
    pub countdown_len: i16,
    /// 0x800B0DA6 (i16): laps of the race.
    pub laps: i16,
    /// 0x800B07E6 (i16) and 0x800B0803 (i8): the finish rule's gate
    /// (both 0 in a 1P DOCK race).
    pub finish_gate_a: i16,
    pub finish_gate_b: i8,
    /// 0x800B0D46 (i16): game state (1 = race running).
    pub game_state: i16,
    /// 0x800B0FAC: event type (3 on DOCK; 0x1D/0x1E/30 are special events).
    pub event: i32,
    /// 0x800B0BD2 (u8): nonzero = the attract demo (trackside cameras).
    pub alt_rule: u8,
    /// 0x800B0E38 (i8): > 6 forces the overhead camera (mode 12).
    pub overhead: i8,
    /// 0x800B0DFE (i16): number of karts in the race.
    pub num_karts: i16,
    /// *0x800D1C84 == *0x800D1C88 with both pointers non-null (cut-scene sync).
    pub cutscene_sync: bool,
    /// (0x800B0B6C + 0x800B0B74) * 16, (0x800B0B78 + 0x800B0B7C) * 16,
    /// (0x800B0B80 + 0x800B0B88) * 16: the fallback eye goal.
    pub scene_center: [i32; 3],
    /// 0x800B0E24 (i8): player 1's menu cursor row; the frontend's course
    /// select (event 0x1E, game state 0xC: camera mode 5) looks at that
    /// world's film strip.
    pub menu_row: i8,
}

impl CameraGlobals {
    /// Values for a one-player DOCK race (all read from oracle lap1P; constant
    /// over the lap).
    pub fn dock_race() -> Self {
        CameraGlobals {
            dt: 2,
            countdown: 0,
            countdown_len: 300,
            laps: 3,
            finish_gate_a: 0,
            finish_gate_b: 0,
            game_state: 1,
            event: 3,
            alt_rule: 0,
            overhead: 4,
            num_karts: 6,
            cutscene_sync: false,
            menu_row: 0,
            scene_center: [0; 3],
        }
    }

    /// Read from a full RAM image or any byte source addressed by `rd(addr, len)`.
    pub fn from_reader(rd: &dyn Fn(u32, usize) -> Vec<u8>) -> Self {
        let i32_at = |a: u32| g_i32(&rd(a, 4), 0);
        let i16_at = |a: u32| g_i16(&rd(a, 2), 0);
        let p1 = i32_at(0x800D_1C84) as u32;
        let p2 = i32_at(0x800D_1C88) as u32;
        let cutscene_sync = p1 != 0 && p2 != 0 && i16_at(p1) == i16_at(p2);
        CameraGlobals {
            dt: i32_at(0x800B_0AEC),
            countdown: i32_at(0x800B_0C70),
            countdown_len: i16_at(0x800B_07F8),
            laps: i16_at(0x800B_0DA6),
            finish_gate_a: i16_at(0x800B_07E6),
            finish_gate_b: rd(0x800B_0803, 1)[0] as i8,
            game_state: i16_at(0x800B_0D46),
            event: i32_at(0x800B_0FAC),
            alt_rule: rd(0x800B_0BD2, 1)[0],
            overhead: rd(0x800B_0E38, 1)[0] as i8,
            num_karts: i16_at(0x800B_0DFE),
            cutscene_sync,
            menu_row: 0,
            scene_center: [
                i32_at(0x800B_0B6C).wrapping_add(i32_at(0x800B_0B74)).wrapping_mul(16),
                i32_at(0x800B_0B78).wrapping_add(i32_at(0x800B_0B7C)).wrapping_mul(16),
                i32_at(0x800B_0B80).wrapping_add(i32_at(0x800B_0B88)).wrapping_mul(16),
            ],
        }
    }
}

/// What the ground query FUN_8003DDB0(x, y, z) leaves in its globals, asked
/// at the camera's new (x, goal y, z). The terrain owner implements the query;
/// the camera only needs these results.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CameraGround {
    /// 0x800B0B0C != 0: a surface was found under/over (x, z).
    pub found: bool,
    /// 0x800B0B24: height of the surface closest to y.
    pub nearest_y: i32,
    /// 0x800B0B04: height of the nearest surface at or below y
    /// (falls back to nearest_y).
    pub below_y: i32,
    /// 0x800B0AE0 != 0: a surface at or below y (or, failing that, the
    /// nearest one) was found. Read by the trackside camera (mode 2).
    pub below_found: bool,
    /// 0x800B0C84 != 0: a surface with flag 0x40 (byte 14) was found.
    pub alt_found: bool,
    /// 0x800B0C9C: its height (closest to y).
    pub alt_y: i32,
    /// 0x800B0A54: minimum height of type-3 surfaces (byte 15 == 3), or the
    /// level default (0xA5AC0 on DOCK) when none.
    pub type3_min_y: i32,
}

/// What the camera asks of the rest of the game while it runs: the ground
/// query, BIOS rand(), the TER path walk and the random byte stream.
pub trait CameraEnv {
    /// FUN_8003DDB0(x, y, z) results.
    fn ground(&mut self, x: i32, y: i32, z: i32) -> CameraGround;
    /// PsyQ rand() (0..=0x7FFF).
    fn rand(&mut self) -> i32;
    /// FUN_80090424(seg, pt, n) (forward) or FUN_80090670 (backward) with
    /// 0x800B0A20 clear: the (0x800B0B20, 0x800B0F3C) it leaves.
    fn walk(&mut self, seg: i16, pt: i16, n: i16, backward: bool) -> (i16, i16);
    /// The TER path point (x, y, z), in TER units (world >> 5):
    /// *(0x800B0AC4 + 16 * (*(i16*)(0x800B0C50 + 36 * seg) + pt)).
    fn path_point(&mut self, seg: i16, pt: i16) -> [i32; 3];
    /// `*(*0x800B0BCC)++`: the next byte of the game's random byte stream.
    fn rng_byte(&mut self) -> u8;
    /// The orbit camera's paths (mode 1): 0 none, 1 the eye path (DAT
    /// chunk 0x0B, 0x800D1C74) only, 2 also the look path (0x0A,
    /// 0x800D1C70) (FUN_80070418 sp+0x60).
    fn orbit_paths(&mut self) -> u8 {
        0
    }
    /// FUN_80081BF4(chunk list, 0, 0, phase, out, 1): the point at `phase`
    /// (0..0x7FFF) around DAT chunk `chunk`'s closed path (world units).
    fn orbit_point(&mut self, _chunk: i16, _phase: i32) -> [i32; 3] {
        [0; 3]
    }
    /// FUN_8006FB2C(x >> 5, z >> 5, quad): the stunt strip's edge points
    /// across the kart and the fraction along the quad (mr_sim::stunt).
    fn strip_project(&mut self, _x5: i32, _z5: i32, _quad: i16) -> Option<([i32; 3], [i32; 3], i16)> {
        None
    }
    /// FUN_800702C4(phase): the strip's edge points at `phase` (0..0x7FFF
    /// of its length).
    fn strip_point(&mut self, _phase: i32) -> Option<([i32; 3], [i32; 3])> {
        None
    }
    /// The stunt camera's distance constants (exe 0x800B078A, 0x800B078C).
    fn stunt_dist(&mut self) -> (i32, i32) {
        (0x800, 0x800)
    }
    /// Mode 3 writes the kart's +0x4A8 (along the quad) and +0x4A9
    /// (across the strip).
    fn strip_kart(&mut self, _along: u8, _across: u8) {}
}

/// Everything one camera tick reads.
#[derive(Clone, Debug)]
pub struct CameraInput {
    pub kart: CameraKart,
    pub globals: CameraGlobals,
    /// What the kart's +0x4FE watch target (TRIANGLE, [`look_at_search`])
    /// is, resolved by the caller with [`resolve_watch`]. Read in mode 0
    /// while the kart has not finished.
    pub watched: Watched,
    /// Game mode 7 (a BOSS CHALLENGE): the boss kart 0x800B0DFE +
    /// 0x800B0DCC the intro camera (mode 10) shows; None otherwise.
    pub intro_kart: Option<CameraKart>,
}

/// The object a kart's watch target +0x4FE names (FUN_80070418
/// @0x8007091C..0x800709A4): the chase camera looks at it in place of the
/// kart's own look-at point.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Watched {
    /// -1: nothing (the kart's own look-at point).
    #[default]
    None,
    /// 0..9: kart record 0x800C30A8 + 0x504 * n: its position, camera mount
    /// and body matrix (the look-at point is computed from them exactly as
    /// for the camera's own kart).
    Kart { pos: [i32; 3], mount: [i16; 4], matrix: [[i16; 3]; 3] },
    /// 0x1E..: look point n - 0x1E of DAT chunk 7 (*0x800D1C64 + 4 + 12 * i),
    /// DAT units; the camera looks at it << 5. (10..0x1D, the challenge
    /// objects 0x800D1C78, and 0x7F, the mode 0x14 point 0x800CB658, are out
    /// of scope: [`resolve_watch`] stops on them.)
    Point([i32; 3]),
    /// 0x7F (game mode 0x14, CHICKEN CHASE): the chicken 0x800CB658, world
    /// units (the session passes it; `resolve_watch` has no chicken).
    World([i32; 3]),
}

/// Resolve a watch target (+0x4FE) for the camera: `kart(n)` returns kart
/// record n (as the race frame left it), `look_points` the course's DAT
/// chunk 7.
pub fn resolve_watch<'a>(target: i8, kart: impl Fn(usize) -> Option<&'a [u8]>, look_points: &[[i32; 3]], challenge: &[[i32; 3]]) -> Watched {
    match target {
        -1 => Watched::None,
        0x7F => unreachable_in_scope!("watch target 0x7F: the session resolves the chicken (Watched::World)"),
        t if t < 10 => {
            let rec = kart(t as usize).unwrap_or_else(|| unreachable_in_scope!(format!("watch target kart {t} not in the race")));
            let k = CameraKart::from_record(rec, None);
            Watched::Kart { pos: k.pos, mount: k.mount, matrix: k.matrix }
        }
        t if t < 0x1E => Watched::Point(*challenge.get(t as usize - 10).unwrap_or_else(|| unreachable_in_scope!(format!("watch target {t}: no such challenge object")))),
        t => Watched::Point(*look_points.get(t as usize - 0x1E).unwrap_or_else(|| unreachable_in_scope!(format!("watch target {t} past the look points")))),
    }
}

/// FUN_80081170(x, y, z, list, 1, 0, 0) on DAT chunk 7 (the look points):
/// the index of the nearest point over the whole list, compared at >> 5 of
/// the arguments and of the points (squared distance as an unsigned 32-bit
/// sum, first strict minimum; -1 when the list is empty). The function also
/// leaves sqrt(best) << 5 in 0x800C3068 and 0 in 0x800B0EB8; nothing reads
/// either before it is written again (FUN_800418D8 resets the nearest-kart
/// table 0x800C3068 at the start of every frame's contact pass, FUN_8007BBF0
/// reads it only after its own FUN_80081170 call), so the port drops them.
pub fn nearest_point_all(pts: &[[i32; 3]], x: i32, y: i32, z: i32) -> Option<usize> {
    let (x, y, z) = (x >> 5, y >> 5, z >> 5);
    let mut best = u32::MAX;
    let mut idx = None;
    for (i, p) in pts.iter().enumerate() {
        let dx = (p[0] >> 5).wrapping_sub(x);
        let dy = (p[1] >> 5).wrapping_sub(y);
        let dz = (p[2] >> 5).wrapping_sub(z);
        let d = dx.wrapping_mul(dx).wrapping_add(dy.wrapping_mul(dy)).wrapping_add(dz.wrapping_mul(dz)) as u32;
        if d < best {
            best = d;
            idx = Some(i);
        }
    }
    idx
}

/// What ADVENTURE's TRIANGLE look-at needs besides the kart: the challenge
/// objects (DAT chunk 0x0C, TER units), the nearest kart's position (+0x4DC
/// set) and whether the Doozer is in the race (more than one kart and kart
/// 1's +0x496 & 0x40).
#[derive(Clone, Copy, Debug, Default)]
pub struct LookAdventure<'a> {
    pub points: &'a [[i32; 3]],
    pub nearest_kart: Option<[i32; 3]>,
    pub doozer: bool,
}

/// FUN_800728C0(kart): TRIANGLE's camera lock (called from the kart update
/// FUN_80064B5C @0x800651F0 on the press). Sets the watch target +0x4FE:
/// - mode 0x14: 0x7F (the fixed point 0x800CB658);
/// - else the nearest kart +0x4DC (FUN_80040018's contact pass); in mode 1
///   with a kart near, and always in modes 2 and 7, that is all;
/// - challenge mode (0x800B07E6 != 0): the nearest live challenge object
///   (0x800D1C78, target 10 + i) [not ported: challenge mode];
/// - otherwise, when the course has a look-point list (DAT chunk 7,
///   0x800D1C64) of fewer than 96 points, the look point nearest to the
///   driver position (+0x330 / +0x338 / +0x340 >> 5, FUN_80081170 mode 1):
///   target 0x1E + i. With no list, or 96 or more points, it stays -1.
pub fn look_at_search(k: &mut crate::kart::KartState, g: &crate::globals::RaceGlobals, look_points: &[[i32; 3]], adv: &LookAdventure) {
    // 0x800728D0
    if g.mode == 0x14 {
        k.look_target = 0x7F;
        return;
    }
    // 0x800728E8: +0x4FE = +0x4DC.
    k.look_target = k.nearest_kart as u8;
    if (g.mode == 1 && k.nearest_kart != -1) || g.mode == 2 || g.mode == 7 {
        return;
    }
    // 0x8007292C..0x800729B4: the distance to the nearest kart (t0), only
    // compared by the challenge-object loop.
    // 0x800729B8: challenge objects.
    if g.challenge != 0 {
        // ADVENTURE (phase 6): the nearest of the challenge objects still
        // out (state 1: the targets and the movie object; the Doozer goals
        // while the Doozer is in the race), compared at the kart's >> 10
        // and the object's >> 5, against the nearest kart's distance
        // (only when that kart is the target already).
        let mut best = adv.nearest_kart.map_or(0, |o| {
            let d = [(k.x - o[0]) >> 10, (k.y - o[1]) >> 10, (k.z - o[2]) >> 10];
            d[0].wrapping_mul(d[0]).wrapping_add(d[1].wrapping_mul(d[1])).wrapping_add(d[2].wrapping_mul(d[2]))
        });
        let (ten, goals) = (g.items.challenge[0] as usize, g.items.challenge[1] as usize);
        for i in 0..ten + goals + 1 {
            let take = if i < ten || ten + goals <= i { g.items.pickups[crate::items::FIRST_CHALLENGE + i].state == 1 } else { adv.doozer };
            let Some(p) = take.then(|| adv.points.get(i).copied()).flatten() else { continue };
            let d = [(k.drv_x >> 10) - (p[0] >> 5), (k.ref_y >> 10) - (p[1] >> 5), (k.drv_z >> 10) - (p[2] >> 5)];
            let d2 = d[0].wrapping_mul(d[0]).wrapping_add(d[1].wrapping_mul(d[1])).wrapping_add(d[2].wrapping_mul(d[2]));
            if k.look_target as i8 == -1 || d2 < best {
                k.look_target = (i as u8).wrapping_add(10);
                best = d2;
            }
        }
        if k.look_target as i8 != -1 {
            return;
        }
    }
    // 0x80072B94: the look points (no chunk 7: the pointer is null; an
    // empty list finds nothing either).
    if look_points.len() >= 0x60 {
        return;
    }
    if let Some(i) = nearest_point_all(look_points, k.drv_x >> 5, k.ref_y >> 5, k.drv_z >> 5) {
        k.look_target = (i as u8).wrapping_add(0x1E);
    }
}

// ---------------------------------------------------------------------------
// Tick
// ---------------------------------------------------------------------------

/// Mode selection (0x80070418..0x80070820).
fn select_mode(cam: &mut CameraState, inp: &CameraInput) -> i16 {
    let g = &inp.globals;
    let k = &inp.kart;
    // Scripted sub-state countdown (0x800704E4..0x80070550); stopped while
    // the race-start countdown runs.
    if g.countdown == 0 {
        let t = (cam.sub_timer as i32).wrapping_sub(g.dt) as i16;
        cam.sub_timer = t;
        if t < 0 {
            cam.sub_timer = 0;
            match cam.sub_state {
                4 => {
                    cam.sub_state = 5;
                    cam.sub_timer = 600;
                }
                5 => {
                    cam.sub_state = 6;
                    cam.sub_timer = 600;
                }
                6 => cam.sub_state = -1,
                _ => {}
            }
        }
    }
    // 0x80070554..0x800707A4: the special events pick their mode by game
    // state; the title (event 0x1E, state 0x10) orbits (mode 1).
    if g.event == 0x1E && g.game_state == 0x10 {
        return 1;
    }
    // 0x800705B8: the frontend's course select (game state 0xC) is mode 5
    // (Session 7, menus).
    if g.event == 0x1E && g.game_state == 0xC {
        return 5;
    }
    // LAB_80070554 (phase 8): THE END (course 0x1D, game mode 0x15) orbits.
    if g.event == 0x1D && g.game_state == 0x15 {
        return 1;
    }
    if g.event == 0x1D || g.event == 0x1E {
        unreachable_in_scope!("special event camera (0x800B0FAC = 0x1D/0x1E)");
    }
    // The finish rule (0x80070660 / 0x800706E0): the trackside camera once
    // the kart has done its laps.
    let finish_rule = || -> i16 {
        if k.lap as i32 != g.laps as i32 && (g.finish_gate_a == 0 || g.finish_gate_b != 0) {
            if k.finish_wait < 181 {
                0
            } else if k.finish_score < 1000 {
                2
            } else {
                // 0x80070740: knocked out (race flag 8 / battle): orbit.
                1
            }
        } else {
            2
        }
    };
    let mut mode: i16 = match g.game_state {
        // 0x80070614: the course menu and the character select (game
        // state 0; Session 7) and state 17 orbit (mode 1).
        0 | 17 => 1,
        18 => unreachable_in_scope!("camera mode 11 (game state 18)"),
        1 | 2 => {
            if g.alt_rule != 0 {
                // Game modes 1 / 2 (phase 8): the attract demo (0x800B0BD2) shows
                // the trackside cameras (mode 2) while the scripted
                // sub-state is below 5 (6: mode 1 + case 3, not reached).
                match cam.sub_state {
                    s if s < 5 => 2,
                    6 => unreachable_in_scope!("attract demo camera, sub-state 6"),
                    _ => 0,
                }
            } else if k.finished != 0 {
                finish_rule()
            } else {
                0
            }
        }
        10 => finish_rule(),
        _ => 0,
    };
    // 0x80070750: a stunt course's strip (0x800D1C84 / 0x800D1C88 with
    // equal counts): mode 3 (9 in game mode 0, the course menu).
    if g.cutscene_sync {
        mode = if g.game_state == 0 { 9 } else { 3 };
    }
    // 0x800707A8: the intro camera while the countdown is at or above 300.
    if mode == 0 && g.countdown >= g.countdown_len as i32 {
        mode = 10;
    }
    // 0x800707B8: cups above 6 (SPEEDWAY, RALLY, GLACIER, the cup-8
    // arenas) use the overhead camera, mode 12.
    if g.overhead > 6 && mode != 1 {
        mode = 12;
    }
    mode
}

/// One camera tick: FUN_80070418(cam, kart). Modes ported: 0 (in-race
/// chase), 10 (intro behind the kart while the countdown is >= 300) and 2
/// (trackside cameras after the finish).
///
/// `env` supplies the terrain query FUN_8003DDB0 (mode 0: once, at the
/// eye's new x/z and goal height; mode 2: at a new trackside point),
/// rand() (drawn on a change to mode 0 or 10), the TER path walk and the
/// random byte stream (mode 2).
pub fn tick(cam: &mut CameraState, inp: &CameraInput, t: &CameraTables, env: &mut dyn CameraEnv) {
    let g = &inp.globals;
    let k = &inp.kart;
    let dt = g.dt;

    cam.mode_changed = 0;
    let old_mode = cam.mode;
    cam.mode = 0;
    let mode = select_mode(cam, inp);
    cam.mode = mode;
    let changed = mode != old_mode;
    if changed {
        cam.mode_changed = 1;
    }
    // 0x8007098C (phase 6b): the intro camera of a BOSS CHALLENGE (game
    // mode 7, 0x800B0DCC != -1) follows that boss's kart instead.
    let k = match (&inp.intro_kart, mode) {
        (Some(b), 10) => b,
        _ => k,
    };

    // Defaults before the per-mode block (0x80070824..0x80070844).
    let mut goal = g.scene_center; // eye goal (sp+0x18)
    let mut w_target: [i16; 3] = [32, 32, 32];
    let mut yaw_div: i32 = 16;
    let mut pitch_div: i32 = 16;

    // Look-at point (sp+0x70): kart position plus the mount offset rotated
    // by the body matrix (FUN_8007D58C), 0x800709A8..0x80070A04.
    if g.num_karts == 0 {
        unreachable_in_scope!("camera with no karts");
    }
    let lift = k.mount[1].wrapping_sub(k.mount[3]); // (s16)(+0x184 - +0x188)
    let rotate_by = |matrix: &[[i16; 3]; 3], local: [i32; 3]| -> [i32; 3] {
        let mut o = [0i32; 3];
        for r in 0..3 {
            let mut s: i32 = 0;
            for c in 0..3 {
                s = s.wrapping_add(local[c].wrapping_mul(matrix[r][c] as i32));
            }
            o[r] = s >> 7;
        }
        o
    };
    let rotate = |local: [i32; 3]| rotate_by(&k.matrix, local);
    let look_of = |pos: &[i32; 3], mount: &[i16; 4], matrix: &[[i16; 3]; 3]| -> [i32; 3] {
        let l = mount[1].wrapping_sub(mount[3]);
        let m = rotate_by(matrix, [-(mount[0] as i32), l as i32, -(mount[2] as i32)]);
        [0, 1, 2].map(|i| pos[i].wrapping_add(m[i]))
    };
    // 0x80070904: in the chase camera a watch target (+0x4FE, TRIANGLE)
    // replaces the look-at point: another kart's own look-at point (its
    // mount and matrix, 0x80070960), or a point << 5 (0x80070984). The eye
    // still follows the camera's own kart.
    let look = if mode == 0 && k.finished == 0 && k.watch_target != -1 {
        match &inp.watched {
            Watched::Kart { pos, mount, matrix } => look_of(pos, mount, matrix),
            Watched::Point(p) => [p[0] << 5, p[1] << 5, p[2] << 5],
            Watched::World(p) => *p,
            Watched::None => panic!("camera: watch target {} not resolved by the caller (camera::resolve_watch)", k.watch_target),
        }
    } else {
        look_of(&k.pos, &k.mount, &k.matrix)
    };
    // What the aim looks at: sp+0x70 (`look`) for modes 0, 2, 10; the
    // mode's own point for the orbit.
    let mut aim_at = look;
    let mut orbit_paths = 0u8;

    let mut follow: i32 = 22400; // sp+0x50: eye height above ground (mode 0)
    match mode {
        0 => {
            // 0x80071954..0x80071C24.
            if changed {
                cam.rand_a = (div_pow2(env.rand(), 14)) as u8;
            }
            if let Some(trail) = &k.trail {
                let dx = k.pos[0].wrapping_sub(cam.pos[0]) >> 5;
                let dz = k.pos[2].wrapping_sub(cam.pos[2]) >> 5;
                let d = t.sqrt0(dx.wrapping_mul(dx).wrapping_add(dz.wrapping_mul(dz)));
                if d < 1025 && g.countdown == 0 {
                    follow = div_pow2((2048 - d).wrapping_mul(follow), 10);
                }
                // Walk the history 16 VBlanks back (0x80071A28..0x80071A8C).
                let mut seg = 0usize;
                let mut sub: i32;
                let mut total: i32 = 0;
                'walk: loop {
                    sub = 0;
                    if seg + 1 >= TRAIL_LEN {
                        unreachable_in_scope!("trail shorter than 16 VBlanks");
                    }
                    let cnt = trail[seg].count;
                    while sub < cnt {
                        total += 1;
                        sub += 1;
                        if total == 16 {
                            break 'walk;
                        }
                    }
                    seg += 1;
                }
                let a = &trail[seg];
                let b = &trail[seg + 1];
                for i in 0..3 {
                    goal[i] = a.pos[i].wrapping_add(mips_div(b.pos[i].wrapping_sub(a.pos[i]).wrapping_mul(sub), a.count));
                }
                goal[1] = goal[1].wrapping_add((lift as i32) << 5);
                w_target = [128, 96, 128];
                pitch_div = 8;
                yaw_div = 8;
                if d < 1024 {
                    w_target[0] = 0;
                    w_target[2] = 0;
                } else if d < 2048 {
                    let w = (((d - 1024) << 7) >> 10) as i16;
                    w_target[0] = w;
                    w_target[2] = w;
                }
            }
        }
        10 => {
            // Intro (0x80071C28..0x80071D30): behind the kart along its body
            // axis, pulling in from 32768 to 16384 units as the countdown
            // runs from 480 to 360, banked to the side rand_a picked.
            if changed {
                cam.rand_a = (div_pow2(env.rand(), 14)) as u8;
            }
            let s0 = (g.countdown - 60 - g.countdown_len as i32).max(0);
            let s1 = (s0 << 9) / 120;
            let back = rotate([0, 0, -(s1 + 512)]);
            let r = if cam.rand_a != 0 { s1 } else { -s1 };
            cam.roll_ref = r / 2;
            w_target = [64, 64, 64];
            pitch_div = 8;
            yaw_div = 8;
            goal = [0, 1, 2].map(|i| look[i].wrapping_add(back[i]));
        }
        2 => {
            // Trackside cameras (0x800710D4..0x80071570).
            let (seg, pt) = env.walk(k.path_seg, k.path_pt, 1, k.path_backward != 0);
            if cam.sub_state == -1 {
                cam.sub_timer = 0;
            }
            let diff = |a: i32, b: i32, sh: u32| a.wrapping_sub(b) >> sh;
            let sq = |v: [i32; 3]| v[0].wrapping_mul(v[0]).wrapping_add(v[1].wrapping_mul(v[1])).wrapping_add(v[2].wrapping_mul(v[2]));
            let kd = [0, 1, 2].map(|i| diff(k.pos[i], cam.pos[i], 10));
            let keep = seg == -1 || cam.sub_timer != 0 || (cam.sub_state != -1 && sq(kd) < 16384);
            goal = cam.pos;
            if !keep {
                // About 4096 TER units ahead of the kart: 4096 / (point
                // spacing) points, rounded (0x800711C4..0x800712E4).
                let a = env.path_point(seg, pt);
                let b = env.path_point(k.path_seg, k.path_pt);
                let d = t.sqrt0(sq([0, 1, 2].map(|i| a[i].wrapping_sub(b[i]))));
                let n = if d == 0 {
                    1
                } else {
                    let (q, r) = (4096 / d, 4096 % d);
                    if d / 2 < r {
                        q + 1
                    } else if q == 0 {
                        1
                    } else {
                        q
                    }
                };
                let (seg2, pt2) = env.walk(k.path_seg, k.path_pt, n as i16, k.path_backward != 0);
                let cand = env.path_point(seg2, pt2).map(|v| v << 5);
                let cd = [0, 1, 2].map(|i| diff(cand[i], cam.pos[i], 5));
                if cam.sub_state == -1 || !(sq(cd) < 0x0100_0000) {
                    // Cut to the new point, 8192 above the ground below it
                    // (0x80071448..0x80071500).
                    cam.mode_changed = 1;
                    goal = cand;
                    let gr = env.ground(goal[0], goal[1], goal[2]);
                    if gr.below_found {
                        let y = gr.below_y.wrapping_sub(8192);
                        goal[1] = y;
                        if gr.alt_found && gr.alt_y < gr.below_y {
                            let a2 = gr.alt_y.wrapping_add(8192);
                            if y < a2 {
                                goal[1] = gr.below_y.wrapping_add(gr.alt_y) / 2;
                            } else if env.rng_byte() < 128 {
                                goal[1] = a2;
                            }
                        }
                    }
                    cam.sub_timer = 120;
                }
            }
            if cam.sub_state == -1 {
                cam.sub_timer = 120;
                cam.sub_state = 0;
            }
            // A cut turns the camera at once; otherwise it pans by 1/8.
            if goal != cam.pos {
                pitch_div = 1;
                yaw_div = 1;
            } else {
                pitch_div = 8;
                yaw_div = 8;
            }
        }
        1 => {
            // Orbit (0x800708B0..0x80070B64): the eye runs round DAT chunk
            // 0x0B's path, looking at chunk 0x0A's a quarter-ish ahead
            // (behind) of it; direction and offset side from rand() on the
            // change. FUN_80081DC4 (path length) returns 0, so the speeds
            // are 0x40000 >> 14 = 16 a VBlank and 0x400 apart.
            let paths = env.orbit_paths();
            if changed {
                cam.orbit_angle = env.rand();
                let a = if g.game_state == 0x10 {
                    cam.rand_b = 0;
                    1
                } else {
                    cam.rand_b = (env.rand() >> 14) as u8;
                    (env.rand() >> 14) as u8
                };
                cam.rand_a = a;
            }
            let (spd, ahead) = if paths == 0 { (8, 0x200) } else if g.event == 0x1E { (16 << 2, 0x400) } else { (16, 0x400) };
            if paths != 0 {
                let a = if cam.rand_a == 0 { cam.orbit_angle.wrapping_add(spd * dt) } else { cam.orbit_angle.wrapping_sub(spd * dt) };
                cam.orbit_angle = a & 0x7FFF;
                goal = env.orbit_point(0x0B, cam.orbit_angle);
                aim_at = if paths == 2 {
                    let b = if cam.rand_b == 0 { cam.orbit_angle.wrapping_add(ahead) } else { cam.orbit_angle.wrapping_sub(ahead) };
                    env.orbit_point(0x0A, b & 0x7FFF)
                } else {
                    // Only the eye path: the game looks at a stale stack
                    // value here [INFER]; the port looks at the kart.
                    k.pos
                };
            } else {
                aim_at = [0; 3];
                goal = [0, -160000, -160000];
            }
            w_target = [128, 128, 128];
            pitch_div = 8;
            yaw_div = 8;
            orbit_paths = paths;
        }
        5 => {
            // Course select (0x80070E4C): eye at (0xA000, y, -0x28000)
            // looking along +z at (0xA000, y, 0x20000), y = (row * 10000 -
            // 25000) * 32 for the world's film strip; the common tail with
            // its default weights (0x20); sp+0x48 = 8 [INFER: the angle
            // divisors].
            let y = (g.menu_row as i32 * 10000 - 25000) * 0x20;
            goal = [0xA000, y, -0x28000];
            aim_at = [0xA000, y, 0x20000];
            pitch_div = 8;
            yaw_div = 8;
        }
        12 => {
            // Overhead (0x80070D94): 0x1400 (minus up to 0xA00 while the
            // countdown is above 300) TER units straight above the kart,
            // looking at its position; weights 0x40 during the countdown,
            // else 0x100. The angles are fixed below.
            let mut h: i32 = 0x1400;
            if g.countdown >= g.countdown_len as i32 {
                h = 0x1400 - ((g.countdown - g.countdown_len as i32).wrapping_mul(0xA00)) / 0xB4;
            }
            goal = [k.pos[0], k.pos[1].wrapping_sub(h << 5), k.pos[2]];
            let w: i16 = if g.countdown > 0 { 0x40 } else { 0x100 };
            w_target = [w, w, w];
        }
        3 => {
            // The stunt trial (0x80071574): beside the kart, on the
            // strip's first edge across from it, pulled toward the kart
            // to 0x800B078A less than its distance (at most 0x1C00) and
            // raised 0x800B078C over it; both scaled by s (0x1000 in the
            // trial; drawing in over the countdown's first 180 VBlanks,
            // backing off over 180 after the finish).
            let s = if g.countdown < g.countdown_len as i32 {
                if k.finished == 0 {
                    0x1000
                } else if k.finish_wait > 0xB3 {
                    0x800
                } else {
                    0x1000 - (k.finish_wait << 11) / 0xB4
                }
            } else {
                0x1000 - (g.countdown - g.countdown_len as i32).wrapping_mul(0x800) / 0xB4
            };
            if let Some((a, b, along)) = env.strip_project(k.pos[0] >> 5, k.pos[2] >> 5, k.strip) {
                let (c_in, c_up) = env.stunt_dist();
                goal = [a[0], k.pos[1].wrapping_sub(div_pow2(c_up.wrapping_mul(s), 7)), a[2]];
                let d = |p: &[i32; 3]| {
                    let (dx, dz) = (p[0].wrapping_sub(k.pos[0]) >> 5, p[2].wrapping_sub(k.pos[2]) >> 5);
                    t.sqrt0(dx.wrapping_mul(dx).wrapping_add(dz.wrapping_mul(dz)))
                };
                let (d1, d2) = (d(&a), d(&b));
                let across = if d1 + d2 != 0 { (d1 << 7) / (d1 + d2) } else { 0 };
                env.strip_kart(along as u8, across as u8);
                let (ex, ez) = (b[0].wrapping_sub(a[0]) >> 5, b[2].wrapping_sub(a[2]) >> 5);
                let l = t.sqrt0(ex.wrapping_mul(ex).wrapping_add(ez.wrapping_mul(ez)));
                if l != 0 {
                    let mut e = d1.min(0x1C00) - div_pow2(c_in.wrapping_mul(s), 12);
                    let mut ox = e.wrapping_mul((ex << 12) / l);
                    if e < 0 {
                        e = 0;
                        ox = 0;
                    }
                    goal[0] = goal[0].wrapping_add(ox >> 7);
                    goal[2] = goal[2].wrapping_add(e.wrapping_mul((ez << 12) / l) >> 7);
                }
            }
            // 0x80071804: weights (s6, s7, s4) = (128, 64, 128); fp =
            // sp+0x48 = 16.
            w_target = [128, 64, 128];
        }
        9 => {
            // A stunt course's menu (0x80071844): the eye sweeps along the
            // strip's first edge (a sine of a phase drawn on the change,
            // running either way), 0x8000 up, looking at the second edge
            // 0x8000 down; sp+0x48 = 0x20.
            if changed {
                cam.sweep = (env.rand() >> 3) as i16;
                cam.rand_a = (env.rand() >> 14) as u8;
            }
            cam.sweep = if cam.rand_a == 0 { cam.sweep.wrapping_sub(dt as i16) } else { cam.sweep.wrapping_add(dt as i16) };
            cam.sweep &= 0xFFF;
            let ph = (div_pow2(t.sin14(cam.sweep as i32), 2) + 0x4000) as i16 as i32;
            if let Some((a, b)) = env.strip_point(ph) {
                goal = [a[0], a[1].wrapping_sub(0x8000), a[2]];
                aim_at = [b[0], b[1].wrapping_add(0x8000), b[2]];
            }
            w_target = [64, 64, 64];
            pitch_div = 32;
            yaw_div = 32;
        }
        m => unreachable_in_scope!(format!("camera mode {m}")),
    }

    // Common tail (0x80071D34). Modes 0, 2, 10 look at sp+0x70.
    let old_y = cam.pos[1];
    if mode == 2 || changed {
        cam.pos = goal;
        cam.weight = w_target;
    } else {
        for _ in 0..reps(dt) {
            cam.weight[0] = approach32(cam.weight[0], w_target[0]);
            if mode != 0 {
                cam.weight[1] = approach32(cam.weight[1], w_target[1]);
            }
            cam.weight[2] = approach32(cam.weight[2], w_target[2]);
            let step = |p: i32, q: i32, w: i16| p.wrapping_add(div_pow2(q.wrapping_sub(p).wrapping_mul(w as i32), 10));
            cam.pos[0] = step(cam.pos[0], goal[0], cam.weight[0]);
            if mode == 0 {
                cam.pos[1] = goal[1];
            } else {
                cam.pos[1] = step(cam.pos[1], goal[1], cam.weight[1]);
            }
            cam.pos[2] = step(cam.pos[2], goal[2], cam.weight[2]);
        }
    }
    cam.pos_div32 = [cam.pos[0] >> 5, cam.pos[1] >> 5, cam.pos[2] >> 5];

    if mode == 0 {
        // Height pass, mode 0 only (0x80071FFC..0x80072310).
        let gr = env.ground(cam.pos[0], cam.pos[1], cam.pos[2]);
        let y = ground_clamp(cam.pos[1], follow, lift, &gr);
        if changed {
            cam.weight[1] = w_target[1];
            cam.pos[1] = y;
        } else {
            let mut s = old_y;
            for _ in 0..reps(dt) {
                cam.weight[1] = approach32(cam.weight[1], w_target[1]);
                s = s.wrapping_add(div_pow2(y.wrapping_sub(s).wrapping_mul(cam.weight[1] as i32), 10));
            }
            cam.pos[1] = s;
        }
    }

    // Aim (0x80072314..0x80072394).
    let dx = aim_at[0].wrapping_sub(cam.pos[0]) >> 7;
    let dz = aim_at[2].wrapping_sub(cam.pos[2]) >> 7;
    let dy = aim_at[1].wrapping_sub(cam.pos[1]) >> 7;
    let h = t.sqrt0(dx.wrapping_mul(dx).wrapping_add(dz.wrapping_mul(dz)));
    let mut pitch_t = t.ratan2(dy.wrapping_abs(), h);
    if dy < 0 {
        pitch_t = -pitch_t;
    }
    let pitch_t = pitch_t & 0xFFF;
    let yaw_t = t.ratan2(dx, dz) & 0xFFF;

    // 0x80072394: event 30 fixes the angles except in modes 1 and 7 (the
    // title's orbit turns as below: oracle title capture, exact).
    // (0x80072394 sets pitch 0 (0x400 above cup 6), yaw 0, roll 0; the
    // roll step 0x80072708 is skipped on event 30.)
    let fixed = g.event == 30 && mode != 1 && mode != 7;
    if fixed {
        cam.pitch = if g.overhead > 6 { 0x400 } else { 0 };
        cam.yaw = 0;
        cam.roll = 0;
    } else if g.overhead > 6 {
        // 0x800723E8: cups above 6: straight down, no yaw, no roll.
        cam.pitch = 0x400;
        cam.yaw = 0;
        cam.roll = 0;
    } else if changed {
        cam.pitch = pitch_t as i16;
        cam.yaw = yaw_t as i16;
    } else {
        for _ in 0..reps(dt) {
            let d = wrap_delta(pitch_t - cam.pitch as i32);
            cam.pitch = ((cam.pitch as i32 + mips_div(d, pitch_div)) & 0xFFF) as i16;
            let d = wrap_delta(yaw_t - cam.yaw as i32);
            cam.yaw = ((cam.yaw as i32 + mips_div(d, yaw_div)) & 0xFFF) as i16;
        }
    }

    // Roll goal (0x8007252C..0x80072708): modes 0 and 10 bank with the
    // kart, mode 2 levels out.
    let roll_t: i32 = if mode == 1 {
        // 0x800725A0: bank against the remaining yaw error (no paths, or
        // events 0x1D / 0x1E: level at once).
        if orbit_paths == 0 || g.event == 0x1D || g.event == 0x1E {
            cam.roll = 0;
            0
        } else {
            let e = yaw_t.wrapping_sub(cam.yaw as u16 as i32);
            let e16 = e as i16 as i32;
            let e = if e16 > 0x800 { e - 0x1000 } else if e16 < -0x800 { e + 0x1000 } else { e };
            if cam.rand_a == cam.rand_b {
                (-((e << 16) >> 14)) & 0xFFF
            } else {
                (e & 0x3FF) << 2
            }
        }
    } else if !(mode == 0 || mode == 10) || k.watch_target != -1 {
        // 0x80072640: only modes 0 and 10 bank (and the orbit above).
        0
    } else if k.roll_mode != 0 {
        // Airborne/spinning: bank against the remaining yaw error.
        let e = yaw_t.wrapping_sub(cam.yaw as u16 as i32);
        let e16 = e as i16 as i32;
        let e = if e16 > 2048 { e - 4096 } else if e16 < -2048 { e + 4096 } else { e };
        (-(e as i16 as i32) * 3) & 0xFFF
    } else {
        let s = (k.lean as i32).wrapping_add(k.yaw_rate >> 3);
        if mode == 10 {
            cam.roll_ref.wrapping_sub(s) & 0xFFF
        } else {
            let half = s / 2; // (s + sign) >> 1
            let h16 = half as i16 as i32;
            if h16 > 512 {
                512
            } else if h16 < -512 {
                (-512) & 0xFFF
            } else {
                half & 0xFFF
            }
        }
    };
    if fixed {
    } else if changed {
        cam.roll = roll_t as i16;
    } else {
        for _ in 0..reps(dt) {
            let d = wrap_delta(roll_t as i16 as i32 - cam.roll as i32);
            cam.roll = ((cam.roll as i32 + div_pow2(d, 4)) & 0xFFF) as i16;
        }
    }

    // Forward vector (0x800727A8..0x8007287C) and matrix (FUN_80024AB4).
    let np = (-(cam.pitch as i32)) & 0xFFF;
    cam.forward[1] = (-t.sin12(np)) as i16;
    let cp = t.cos12(np);
    cam.forward[0] = (div_pow2(cp.wrapping_mul(t.sin12(cam.yaw as i32)), 12)) as i16;
    cam.forward[2] = (div_pow2(cp.wrapping_mul(t.cos12(cam.yaw as i32)), 12)) as i16;
    cam.matrix = rot_matrix(t, cam.pitch as i32, (-(cam.yaw as i32)) & 0xFFF, cam.roll as i32);
}

/// Eye height rules after the ground query (0x80072020..0x80072248).
/// Heights grow downward; `follow` is the wanted clearance above the surface.
fn ground_clamp(y0: i32, follow: i32, lift: i16, gr: &CameraGround) -> i32 {
    let avg = |a: i32, b: i32| a.wrapping_add(b) / 2;
    if !gr.found {
        return y0.wrapping_sub(follow.wrapping_add((lift as i32) << 5));
    }
    let mut y = y0;
    if gr.alt_found && y.wrapping_sub(gr.alt_y).wrapping_abs() < y.wrapping_sub(gr.nearest_y).wrapping_abs() {
        // Closer to the flagged surface (0x80072088).
        let a2 = gr.alt_y.wrapping_add(follow);
        if !(y < a2) {
            return y;
        }
        y = a2;
        if gr.alt_y < gr.type3_min_y && a2 > gr.type3_min_y.wrapping_sub(follow) {
            return avg(gr.alt_y, gr.type3_min_y);
        }
        if gr.below_y.wrapping_sub(follow) < y {
            return avg(gr.alt_y, gr.below_y);
        }
        return y;
    }
    // 0x800720F8: keep `follow` above the nearest surface.
    let lim = gr.nearest_y.wrapping_sub(follow);
    if lim < y {
        y = lim;
    }
    let cap = gr.type3_min_y.wrapping_sub(follow);
    let skip_cap = gr.alt_found && gr.alt_y < gr.nearest_y && gr.type3_min_y < gr.alt_y;
    if !skip_cap && cap < y {
        // 0x80072170: stay `follow` above the type-3 surface.
        y = cap;
        if !gr.alt_found {
            return y;
        }
        if gr.alt_y < gr.type3_min_y && cap < gr.alt_y.wrapping_add(follow) {
            return avg(gr.alt_y, gr.type3_min_y);
        }
        return y;
    }
    // 0x800721C4
    if gr.alt_found && gr.alt_y < y0 && y < gr.alt_y.wrapping_add(follow) {
        return avg(gr.alt_y, gr.nearest_y);
    }
    y
}

/// FUN_80024AB4(x, y, z, out): rotation matrix from three 12-bit angles,
/// M = A(x, y) * Rz(z), stored transposed. All products /4096 truncating.
/// The camera passes (pitch, -yaw, roll).
pub fn rot_matrix(t: &CameraTables, ax: i32, ay: i32, az: i32) -> [[i16; 3]; 3] {
    let d = |v: i32| div_pow2(v, 12);
    let (sx, cx) = (t.sin12(ax), t.cos12(ax));
    let (sy, cy) = (t.sin12(ay), t.cos12(ay));
    let (sz, cz) = (t.sin12(az), t.cos12(az));
    // A = [[cy, sy*sx, -sy*cx], [0, cx, sx], [sy, -cy*sx, cy*cx]]
    let a01 = d(sy * sx) as i16 as i32;
    let a02 = d(-sy * cx);
    let a11 = cx;
    let a12 = sx;
    let a21 = d(-cy * sx) as i16 as i32;
    let a22 = d(cy * cx);
    // Columns 0 and 1 rotated by roll.
    let m00 = d(cy * cz - a01 * sz);
    let m01 = d(cy * sz + a01 * cz);
    let m10 = d(-(a11 * sz));
    let m11 = d(a11 * cz);
    let m20 = d(sy * cz - a21 * sz);
    let m21 = d(sy * sz + a21 * cz);
    let m = [[m00, m01, a02], [m10, m11, a12], [m20, m21, a22]];
    let mut out = [[0i16; 3]; 3];
    for r in 0..3 {
        for c in 0..3 {
            out[r][c] = m[c][r] as i16;
        }
    }
    out
}

// ---------------------------------------------------------------------------
// View
// ---------------------------------------------------------------------------

/// The GTE view the renderer builds from the camera (FUN_8002CEBC).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CameraView {
    /// Camera matrix as stored at cam+0x00 (rows: right, down, forward;
    /// orthonormal 4.12).
    pub camera_matrix: [[i16; 3]; 3],
    /// The GTE rotation actually loaded (0x800C3008 / scratch 0x1F800394):
    /// row 0 * 8/5 (512-wide frame buffer aspect), row 1 * (240 - letterbox)/240.
    pub gte_rotation: [[i16; 3]; 3],
    /// Eye position in world units (scratch 0x1F800374).
    pub eye: [i32; 3],
    /// eye / 32, rounded toward zero (scratch 0x1F800384).
    pub eye_div32: [i32; 3],
    /// Projection plane distance H.
    pub h: i32,
    /// Screen offset (OFX, OFY).
    pub offset: (i32, i32),
}

impl CameraState {
    /// The view for a one-player race. `letterbox` is the i16 at 0x800B07D8
    /// (0 in a race).
    pub fn view(&self, letterbox: i16) -> CameraView {
        let mut r = self.matrix;
        for c in 0..3 {
            // Row 0: (m << 9) * 0x66666667 >> 32 >> 7, toward zero = m * 1.6.
            let v = (self.matrix[0][c] as i32) << 9;
            let hi = ((v as i64 * 0x6666_6667i64) >> 32) as i32;
            r[0][c] = ((hi >> 7) - (v >> 31)) as i16;
            // Row 1: m * (240 - letterbox) / 240 via 0x88888889.
            let a = (self.matrix[1][c] as i32).wrapping_mul(240 - letterbox as i32);
            let hi = ((a as i64 * (0x8888_8889u32 as i32 as i64)) >> 32) as i32;
            r[1][c] = ((hi.wrapping_add(a) >> 7) - (a >> 31)) as i16;
        }
        CameraView {
            camera_matrix: self.matrix,
            gte_rotation: r,
            eye: self.pos,
            eye_div32: [div_pow2(self.pos[0], 5), div_pow2(self.pos[1], 5), div_pow2(self.pos[2], 5)],
            h: GTE_H,
            offset: GTE_OFFSET_1P,
        }
    }
}

impl CameraView {
    /// The per-object GTE translation the world renderer loads
    /// (FUN_8002ADA8 @0x8002AE84): TR = (R * (P - eye)) / 2^17, toward zero.
    /// Model vertices are then in 1/32 world units: screen = R * v / 4096 + TR.
    pub fn translation_for(&self, p: [i32; 3]) -> [i32; 3] {
        let d = [p[0].wrapping_sub(self.eye[0]), p[1].wrapping_sub(self.eye[1]), p[2].wrapping_sub(self.eye[2])];
        let mut tr = [0i32; 3];
        for r in 0..3 {
            let s = d[0]
                .wrapping_mul(self.gte_rotation[r][0] as i32)
                .wrapping_add(d[1].wrapping_mul(self.gte_rotation[r][1] as i32))
                .wrapping_add(d[2].wrapping_mul(self.gte_rotation[r][2] as i32));
            tr[r] = div_pow2(s, 17);
        }
        tr
    }

    /// Project a camera-space point (GTE units) like RTPS without the
    /// saturation and UNR-divide details: sx = OFX + H*X/Z, sy = OFY + H*Y/Z.
    pub fn project(&self, v: [i32; 3]) -> Option<(i32, i32)> {
        if v[2] <= 0 {
            return None;
        }
        let z = v[2] as i64;
        Some((
            self.offset.0 + ((self.h as i64 * v[0] as i64) / z) as i32,
            self.offset.1 + ((self.h as i64 * v[1] as i64) / z) as i32,
        ))
    }
}

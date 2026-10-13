//! Constant data the kart update reads from the game's executable.
//!
//! Every table is read at run time from SLUS_012.37 on the owner's disc (PS-X
//! EXE: file offset = addr - t_addr + 0x800, t_addr 0x80010000). Nothing here
//! embeds game bytes. Each accessor names the address it reproduces; the
//! tests in tests/tables.rs check them against the oracle's RAM.

use std::fmt;

#[derive(Debug)]
pub struct ExeError(pub String);

impl fmt::Display for ExeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "exe: {}", self.0)
    }
}

impl std::error::Error for ExeError {}

/// The PS-X EXE text image, addressable by RAM address.
#[derive(Clone)]
pub struct ExeImage {
    pub t_addr: u32,
    pub text: Vec<u8>,
}

impl ExeImage {
    pub fn parse(exe: &[u8]) -> Result<Self, ExeError> {
        if exe.len() < 0x800 || &exe[0..8] != b"PS-X EXE" {
            return Err(ExeError("not a PS-X EXE".into()));
        }
        let t_addr = u32::from_le_bytes([exe[0x18], exe[0x19], exe[0x1A], exe[0x1B]]);
        let t_size = u32::from_le_bytes([exe[0x1C], exe[0x1D], exe[0x1E], exe[0x1F]]) as usize;
        if exe.len() < 0x800 + t_size {
            return Err(ExeError(format!("short text: {} < {}", exe.len(), 0x800 + t_size)));
        }
        Ok(ExeImage { t_addr, text: exe[0x800..0x800 + t_size].to_vec() })
    }

    pub fn contains(&self, addr: u32) -> bool {
        addr >= self.t_addr && ((addr - self.t_addr) as usize) < self.text.len()
    }

    pub fn u8(&self, addr: u32) -> u8 {
        assert!(self.contains(addr), "address {addr:08X} outside SLUS_012.37 text");
        self.text[(addr - self.t_addr) as usize]
    }

    pub fn i16(&self, addr: u32) -> i16 {
        i16::from_le_bytes([self.u8(addr), self.u8(addr + 1)])
    }

    pub fn i16s(&self, addr: u32, n: usize) -> Vec<i16> {
        (0..n).map(|i| self.i16(addr + 2 * i as u32)).collect()
    }

    pub fn bytes(&self, addr: u32, n: usize) -> Vec<u8> {
        (0..n).map(|i| self.u8(addr + i as u32)).collect()
    }
}

/// Sine table, 5120 entries, 1.0 = 0x4000 [exe 0x800A88C0]. The game reads
/// sine at 0x800A88C0 + 2a and cosine through PTR_DAT_800AFF20 = 0x800A90C0
/// (entry a + 1024), always as `(short)entry >> 2` (4.12).
pub const SIN_ADDR: u32 = 0x800A_88C0;
/// ratan2's arctangent table, 1025 entries [exe 0x800AD608] (ratan2 @0x80095010).
pub const ATAN_ADDR: u32 = 0x800A_D608;
/// SquareRoot0's table, indexed (normalised - 0x40) [exe 0x800AD2E8] (@0x80093D70).
pub const SQRT_ADDR: u32 = 0x800A_D2E8;
/// Surface properties, 8 bytes per surface type [exe 0x800ACCF0]; the kart
/// update reads bytes 0..3 (handling scales /128) and 5..7 (copied to the kart
/// at +0x4BB, +0x4BC, +0x4BD by FUN_8003E91C).
pub const SURFACE_ADDR: u32 = 0x800A_CCF0;
/// Wall-height step table, halfwords, read as [dt + 5] [exe 0x800A88AA]
/// (FUN_8003D240).
pub const WALL_STEP_ADDR: u32 = 0x800A_88AA;
/// Vehicle base stats, 8 bytes per vehicle: bytes 1..4 become the kart's
/// stats at 0x800D9136 + 0x174 * vehicle [exe 0x800ACDB0] (FUN_80087EB4).
pub const VEHICLE_ADDR: u32 = 0x800A_CDB0;
/// Weapon types drawn for a recharging weapon slot (random index >> 3)
/// at 0x80065A0C [exe 0x800AC178].
pub const WEAPON_PICK_ADDR: u32 = 0x800A_C178;

pub const N_SURFACES: usize = 32;
pub const N_VEHICLES: usize = 48;

#[derive(Clone)]
pub struct Tables {
    pub sin: Vec<i16>,
    pub atan: Vec<i16>,
    pub sqrt: Vec<i16>,
    /// The 192 halfwords before the square-root table: SquareRoot0 of a
    /// negative argument (a wrapped squared distance) reads them.
    pub sqrt_neg: Vec<i16>,
    pub surface: Vec<[u8; 8]>,
    pub wall_step: Vec<i16>,
    pub vehicle: Vec<[u8; 8]>,
    pub weapon_pick: Vec<u8>,
}

impl Tables {
    pub fn from_exe(exe: &ExeImage) -> Self {
        let rows = |addr: u32, n: usize| -> Vec<[u8; 8]> {
            (0..n)
                .map(|i| {
                    let mut r = [0u8; 8];
                    for (j, b) in r.iter_mut().enumerate() {
                        *b = exe.u8(addr + 8 * i as u32 + j as u32);
                    }
                    r
                })
                .collect()
        };
        Tables {
            sin: exe.i16s(SIN_ADDR, 5120),
            atan: exe.i16s(ATAN_ADDR, 1025),
            sqrt: exe.i16s(SQRT_ADDR, 192),
            sqrt_neg: exe.i16s(SQRT_ADDR - 2 * 192, 192),
            surface: rows(SURFACE_ADDR, N_SURFACES),
            wall_step: exe.i16s(WALL_STEP_ADDR, 16),
            vehicle: rows(VEHICLE_ADDR, N_VEHICLES),
            weapon_pick: exe.bytes(WEAPON_PICK_ADDR, 32),
        }
    }

    /// `(short)sin_table[a] >> 2`, a already in 0..0x1000 (4.12).
    #[inline]
    pub fn sin(&self, a: i32) -> i32 {
        (self.sin[a as usize] as i32) >> 2
    }

    /// `(short)cos_table[a] >> 2` (cos table = sin table + 1024 entries).
    #[inline]
    pub fn cos(&self, a: i32) -> i32 {
        (self.sin[a as usize + 1024] as i32) >> 2
    }

    /// Raw sine entry (1.0 = 0x4000), for the few places the game uses it
    /// without the >> 2.
    #[inline]
    pub fn sin_raw(&self, a: i32) -> i32 {
        self.sin[a as usize] as i32
    }

    #[inline]
    pub fn cos_raw(&self, a: i32) -> i32 {
        self.sin[a as usize + 1024] as i32
    }

    /// Surface property byte `b` of surface type `t` (0x800ACCF0 + 8t + b).
    #[inline]
    pub fn surface(&self, t: i32, b: usize) -> i32 {
        self.surface[t as usize][b] as i32
    }
}

/// ratan2 (PSYQ libgte, 0x80095010): 12-bit angle of (y, x), atan(y/x)
/// measured from +x... exactly the integer algorithm of the library: fold to
/// the first octant, index the 1025-entry table with (min << 10) / max (or
/// min / (max >> 10) when max >= 2^21), unfold.
pub fn ratan2(t: &Tables, y: i32, x: i32) -> i32 {
    let mut x = x;
    let mut y = y;
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
    let v;
    if y < x {
        // 0x80095048..: angle below 45 deg, table value used directly
        let idx = if (y as u32) & 0x7FE0_0000 != 0 {
            crate::fixed::div(y, x >> 10)
        } else {
            crate::fixed::div(y.wrapping_shl(10), x)
        };
        v = t.atan[idx as usize] as i32;
    } else {
        let idx = if (x as u32) & 0x7FE0_0000 != 0 {
            crate::fixed::div(x, y >> 10)
        } else {
            crate::fixed::div(x.wrapping_shl(10), y)
        };
        v = 0x400 - t.atan[idx as usize] as i32;
    }
    let mut v = v;
    if neg_x {
        v = 0x800 - v;
    }
    if neg_y {
        v = -v;
    }
    v
}

/// SquareRoot0 (PSYQ libgte, 0x80093D70): integer sqrt via the GTE leading
/// zero counter and a 192-entry table.
/// GTE op: LZCS/LZCR (cop2 r30 -> r31): LZCR = number of leading bits equal
/// to bit 31 of LZCS (psx-spx "GTE Registers: LZCS/LZCR").
pub fn square_root0(t: &Tables, a: i32) -> i32 {
    let lz = if a < 0 { (!a).leading_zeros() } else { a.leading_zeros() } as i32;
    if lz == 32 {
        return 0;
    }
    let e = lz & !1;
    let n = if e - 24 < 0 { a >> (24 - e) } else { a.wrapping_shl((e - 24) as u32) };
    let sh = (31 - e) >> 1;
    let i = n - 0x40;
    let v = if i < 0 { t.sqrt_neg[(i + 192) as usize] } else { t.sqrt[i as usize] };
    let r = (v as i32).wrapping_shl(sh as u32);
    ((r as u32) >> 12) as i32
}

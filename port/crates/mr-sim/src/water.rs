//! FUN_8003C5FC: the water animation, run at the start of every race frame.
//! Two "wave" normals (0x800B0CA0 for water faces, 0x800B0EA8 for the
//! alternative normal FUN_8003E91C uses) are driven by angles that chase
//! targets; a target that is reached is replaced by a pseudo-random byte
//! << 2 drawn from the shared random pointer 0x800B0BCC. Its other outputs
//! (texture scroll angles 0x800B0CB0.., 0x800B0DC0.., 0x800B0B90.., the
//! table 0x800A889C) are render side and not ported.
//!
//! State lives in the executable's data section (0x800AFEF0..0x800AFF0B);
//! a session starts from the values the exe file holds.

use crate::fixed::{div_pow2, s16};
use crate::tables::{ExeImage, Tables};

pub const STATE_ADDR: u32 = 0x800A_FEF0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WaterAnim {
    /// 0x800AFEF0..0x800AFEFA: free-running phases (render outputs only).
    pub phase: [u16; 6],
    /// 0x800AFEFC/0x800AFEFE and 0x800AFF00/0x800AFF02: the alternative
    /// normal's two angles and their targets.
    pub alt: [(i16, i16); 2],
    /// 0x800AFF04/0x800AFF06 and 0x800AFF08/0x800AFF0A: the water normal's.
    pub water: [(i16, i16); 2],
}

impl WaterAnim {
    pub fn from_exe(exe: &ExeImage) -> WaterAnim {
        let h = |o: u32| exe.i16(STATE_ADDR + o);
        WaterAnim {
            phase: [h(0) as u16, h(2) as u16, h(4) as u16, h(6) as u16, h(8) as u16, h(10) as u16],
            alt: [(h(0xC), h(0xE)), (h(0x10), h(0x12))],
            water: [(h(0x14), h(0x16)), (h(0x18), h(0x1A))],
        }
    }

    pub fn from_bytes(b: &[u8]) -> WaterAnim {
        let h = |o: usize| i16::from_le_bytes([b[o], b[o + 1]]);
        WaterAnim {
            phase: [h(0) as u16, h(2) as u16, h(4) as u16, h(6) as u16, h(8) as u16, h(10) as u16],
            alt: [(h(0xC), h(0xE)), (h(0x10), h(0x12))],
            water: [(h(0x14), h(0x16)), (h(0x18), h(0x1A))],
        }
    }

    /// One race frame. Returns (water normal 0x800B0CA0, alt normal
    /// 0x800B0EA8). `rng` is 0x800B0BCC; `byte` reads the random stream.
    pub fn step(&mut self, t: &Tables, dt: i32, rng: &mut u32, byte: &dyn Fn(u32) -> u8) -> ([i16; 3], [i16; 3]) {
        let ph = &mut self.phase;
        ph[0] = ((ph[0] as i32 + dt * 0x20) & 0xFFF) as u16;
        ph[1] = ((ph[1] as i32 + s16(dt) * 0x40) & 0xFFF) as u16;
        ph[3] = ((ph[3] as i32 + dt * 0x26) & 0xFFF) as u16;
        ph[4] = ((ph[4] as i32 + s16(dt) * 0x43) & 0xFFF) as u16;
        ph[5] = ((ph[5] as i32 + dt * 0x22) & 0xFFF) as u16;
        let mut draw = |slot: &mut (i16, i16)| {
            if slot.0 == slot.1 {
                slot.1 = (byte(*rng) as i8 as i16) << 2;
                *rng = rng.wrapping_add(1);
            }
        };
        // 0x8003C734: the water normal's angles chase their targets.
        chase(&mut self.water[0], dt * 0x10);
        chase(&mut self.water[1], dt * 0x10);
        draw(&mut self.water[0]);
        draw(&mut self.water[1]);
        let wn = normal(t, self.water[0].0, self.water[1].0, 6);
        // 0x8003C940: the alternative normal's.
        chase(&mut self.alt[0], dt * 0x20);
        chase(&mut self.alt[1], dt * 0x20);
        draw(&mut self.alt[0]);
        draw(&mut self.alt[1]);
        let an = normal(t, self.alt[0].0, self.alt[1].0, 5);
        (wn, an)
    }
}

/// Move the angle toward its target by `step`, stopping on it.
fn chase(s: &mut (i16, i16), step: i32) {
    let (a, tg) = (s.0 as i32, s.1 as i32);
    if tg < a {
        let n = a - step;
        s.0 = if s16(n) < tg { tg as i16 } else { n as i16 };
    } else if a < tg {
        let n = a + step;
        s.0 = if tg < s16(n) { tg as i16 } else { n as i16 };
    }
}

/// The normal (x, y, z) from the two angles: tilt u = sin(a) >> shift,
/// direction v = (sin(b) >> 4) with the table index kept even.
fn normal(t: &Tables, a: i16, b: i16, shift: u32) -> [i16; 3] {
    let bias = if shift == 6 { 0x3F } else { 0x1F };
    let ra = t.sin_raw(a as i32 & 0xFFF);
    let u = ((if ra < 0 { ra + bias } else { ra }) >> shift) & 0xFFF;
    let rb = t.sin_raw(b as i32 & 0xFFF);
    let v = ((if rb < 0 { rb + 0x1F } else { rb }) >> 4) & 0x1FFE;
    let vi = v / 2;
    let x = (-t.sin_raw(vi)) >> 2;
    let y = ((-(t.cos_raw(vi).wrapping_mul(t.cos_raw(u)))) as u32 >> 16) as i32;
    let z = ((t.cos_raw(vi).wrapping_mul(t.sin_raw(u))) as u32 >> 16) as i32;
    let _ = div_pow2;
    [x as i16, y as i16, z as i16]
}

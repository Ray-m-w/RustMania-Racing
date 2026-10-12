//! The kart's body matrix +0x18C, rebuilt after the kart loop by
//! FUN_80025D8C -> FUN_80024594(angles, out, kart) with angles (pitch,
//! heading + 0x800, roll) and the scale factors +0x442 (engine) and +0x464
//! (sink). Render side, but the camera reads it (camera.rs) and the oracle
//! trace records it (m0..m8).

use crate::fixed::div_pow2;
use crate::kart::KartState;
use crate::tables::Tables;

/// FUN_80024594: rows are the body axes, 4.12, scaled by engine/0x1000 and,
/// vertically, by (0x1000 + sink)/0x1000 (at least 1/4).
pub fn body_matrix(t: &Tables, k: &KartState) -> [[i16; 3]; 3] {
    let a0 = k.pitch as i32 & 0xFFF;
    let a1 = (k.heading as i32 + 0x800) & 0xFFF;
    let a2 = k.roll as i32 & 0xFFF;
    let (s0, c0) = (div_pow2(t.sin_raw(a0), 2), div_pow2(t.cos_raw(a0), 2));
    let (s1, c1) = (div_pow2(t.sin_raw(a1), 2), div_pow2(t.cos_raw(a1), 2));
    let (s2, c2) = (div_pow2(t.sin_raw(a2), 2), div_pow2(t.cos_raw(a2), 2));
    let e = k.engine as i32;
    let mut sink = (k.sink as u16 as i32 + 0x1000) as i16 as i32;
    if sink < 0x400 {
        sink = 0x400;
    }
    let d = |x: i32| div_pow2(x, 12);
    let ds = |x: i32| d(x) as i16 as i32;
    let a = ds(e.wrapping_mul(0x1000)); // = engine
    let q = ds(sink.wrapping_mul(e));
    let p = ds(a.wrapping_mul(c1));
    let sx = ds(-(a.wrapping_mul(s1)));
    let r = ds(a.wrapping_mul(s1));
    let tt = ds(a.wrapping_mul(c1));
    let m00 = d(p.wrapping_mul(c2));
    let ps2 = ds(p.wrapping_mul(s2));
    let m01 = d(-(q.wrapping_mul(s2)));
    let m02 = d(r.wrapping_mul(c2));
    let rs2 = ds(r.wrapping_mul(s2));
    let qc2 = ds(q.wrapping_mul(c2));
    let m10 = d(ps2.wrapping_mul(c0).wrapping_sub(sx.wrapping_mul(s0)));
    let m11 = d(qc2.wrapping_mul(c0));
    let m12 = d(rs2.wrapping_mul(c0).wrapping_sub(tt.wrapping_mul(s0)));
    let m20 = d(ps2.wrapping_mul(s0).wrapping_add(sx.wrapping_mul(c0)));
    let m21 = d(qc2.wrapping_mul(s0));
    let m22 = d(rs2.wrapping_mul(s0).wrapping_add(tt.wrapping_mul(c0)));
    [
        [m00 as i16, m01 as i16, m02 as i16],
        [m10 as i16, m11 as i16, m12 as i16],
        [m20 as i16, m21 as i16, m22 as i16],
    ]
}

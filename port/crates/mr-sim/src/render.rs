//! Render-side kart fields the race frame rebuilds every tick, which the
//! game's part placement (mr-bevy kartmodel.rs, docs/spec/formats/MDL.md)
//! reads, and which feed back into the next tick only through the body-sway
//! springs (look point, aim target):
//!
//! - FUN_80041A20 (before the kart loop): the look-at target +0x2C0..+0x2CC
//!   from the course's look points (DAT chunk 7) via FUN_80081170;
//!   FUN_80041CC4 (finish / camera override, not taken in a race).
//! - FUN_80086DF0 (after the loop): wheel part matrices +0x20C + 0x12 * i
//!   (spin and steer) and the driver's head / body matrices +0x29C, +0x2AE.
//! - FUN_80025D8C (after the loop): body matrix +0x18C (body.rs), bounce
//!   offset +0x46A..+0x46E, vehicle matrix +0x1AC (body x lean), driver matrix
//!   +0x1CC (x sway), driver position +0x330/+0x338/+0x340 (+ previous at
//!   +0x334/+0x33C/+0x344), look point +0x324..+0x32C, wheel world positions
//!   (+0x14 + 0x2C * i, + previous), light points (+0x11C + 0x18 * j).
//!   Not ported: +0x1EC (driver x the character model's own matrix; nothing
//!   in the kart code or the part placement reads it).
//!
//! GTE: MulMatrix0 (PsyQ @0x800940B0) = three MVMVA (sf = 1, rotation matrix
//! = first operand, vector = a column of the second, no translation, lm = 0):
//! each element = sat16((sum of three products) >> 12), arithmetic shift,
//! saturated to -0x8000..0x7FFF (psx-spx "GTE Coordinate Calculation
//! Commands: MVMVA").

use crate::fixed::div_pow2;
use crate::kart::KartState;
use crate::tables::Tables;
use crate::world::World;

pub type Mat = [[i16; 3]; 3];

/// MulMatrix0(a, b) = a * b on the GTE.
pub fn mul_matrix0(a: &Mat, b: &Mat) -> Mat {
    let mut m = [[0i16; 3]; 3];
    for c in 0..3 {
        for r in 0..3 {
            let s: i64 = (0..3).map(|k| a[r][k] as i64 * b[k][c] as i64).sum();
            m[r][c] = (s >> 12).clamp(-0x8000, 0x7FFF) as i16;
        }
    }
    m
}

/// FUN_8007D58C(x, y, z, M): M * (-x, y, -z) >> 7 (the model-to-world point
/// transform with the game's axis flip).
pub fn mvec(x: i32, y: i32, z: i32, m: &Mat) -> [i32; 3] {
    let (x, z) = (-x, -z);
    let mut o = [0; 3];
    for r in 0..3 {
        o[r] = x
            .wrapping_mul(m[r][0] as i32)
            .wrapping_add(y.wrapping_mul(m[r][1] as i32))
            .wrapping_add(z.wrapping_mul(m[r][2] as i32))
            >> 7;
    }
    o
}

fn sr(t: &Tables, a: i32) -> i32 {
    t.sin_raw(a as i16 as i32 & 0xFFF)
}
fn cr(t: &Tables, a: i32) -> i32 {
    t.cos_raw(a as i16 as i32 & 0xFFF)
}

/// FUN_800252A4(a, b): rotation about x by b then about z by a (lean / sway).
pub fn rot_lean(t: &Tables, a: i32, b: i32) -> Mat {
    let (sa, ca, sb, cb) = (sr(t, a), cr(t, a), sr(t, b), cr(t, b));
    let d = |v: i32| div_pow2(v, 12) as i16;
    [
        [(ca >> 2) as i16, -((sa >> 2) as i16), 0],
        [d((sa >> 2) * (cb >> 2)), d((ca >> 2) * (cb >> 2)), -((sb >> 2) as i16)],
        [d((sa >> 2) * (sb >> 2)), d((ca >> 2) * (sb >> 2)), (cb >> 2) as i16],
    ]
}

/// FUN_800251AC(spin, steer): a steered, rolling wheel.
pub fn rot_wheel(t: &Tables, a: i32, b: i32) -> Mat {
    let (sa, ca, sb, cb) = (sr(t, a), cr(t, a), sr(t, b), cr(t, b));
    let d = |v: i32| div_pow2(v, 12) as i16;
    [
        [(cb >> 2) as i16, d((sa >> 2) * (sb >> 2)), d((ca >> 2) * (sb >> 2))],
        [0, (ca >> 2) as i16, -((sa >> 2) as i16)],
        [-((sb >> 2) as i16), d((sa >> 2) * (cb >> 2)), d((ca >> 2) * (cb >> 2))],
    ]
}

/// FUN_80025408 (about y), FUN_8002546C (about z), FUN_800253A4 (about x).
pub fn rot_axis(t: &Tables, a: i32, axis: usize) -> Mat {
    let c = (cr(t, a) >> 2) as i16;
    let s = (sr(t, a) >> 2) as i16;
    match axis {
        1 => [[c, 0, s], [0, 0x1000, 0], [-s, 0, c]],
        2 => [[c, s, 0], [-s, c, 0], [0, 0, 0x1000]],
        _ => [[0x1000, 0, 0], [0, c, -s], [0, s, c]],
    }
}

/// FUN_80024AB4(x, y, z): the same rotation the camera uses (camera.rs
/// rot_matrix), M = A(x, y) * Rz(z) stored transposed.
pub fn rot_xyz(t: &Tables, ax: i32, ay: i32, az: i32) -> Mat {
    let d = |v: i32| div_pow2(v, 12);
    let (sx, cx) = (t.sin(ax & 0xFFF), t.cos(ax & 0xFFF));
    let (sy, cy) = (t.sin(ay & 0xFFF), t.cos(ay & 0xFFF));
    let (sz, cz) = (t.sin(az & 0xFFF), t.cos(az & 0xFFF));
    let a01 = d(sy * sx) as i16 as i32;
    let a02 = d(-sy * cx);
    let a21 = d(-cy * sx) as i16 as i32;
    let a22 = d(cy * cx);
    let m = [
        [d(cy * cz - a01 * sz), d(cy * sz + a01 * cz), a02],
        [d(-(cx * sz)), d(cx * cz), sx],
        [d(sy * cz - a21 * sz), d(sy * sz + a21 * cz), a22],
    ];
    let mut o = [[0i16; 3]; 3];
    for r in 0..3 {
        for c in 0..3 {
            o[r][c] = m[c][r] as i16;
        }
    }
    o
}

fn put_m(r: &mut [u8], o: usize, m: &Mat) {
    for i in 0..9 {
        r[o + 2 * i..o + 2 * i + 2].copy_from_slice(&m[i / 3][i % 3].to_le_bytes());
    }
}
fn g16(r: &[u8], o: usize) -> i32 {
    i16::from_le_bytes([r[o], r[o + 1]]) as i32
}
fn g32(r: &[u8], o: usize) -> i32 {
    i32::from_le_bytes(r[o..o + 4].try_into().unwrap())
}
fn p16(r: &mut [u8], o: usize, v: i32) {
    r[o..o + 2].copy_from_slice(&(v as i16).to_le_bytes());
}
fn p32(r: &mut [u8], o: usize, v: i32) {
    r[o..o + 4].copy_from_slice(&v.to_le_bytes());
}

/// What the render passes need from the models.
#[derive(Clone, Copy, Debug, Default)]
pub struct DriverModel {
    /// Pivot of the character model's part 0 (FUN_80060944(model, 0) returns
    /// (-x, y, -z) of it).
    pub part0_pivot: [i32; 3],
}

/// FUN_80041A20 for one human kart in a race (no other look sources).
pub fn aim_at_course(k: &mut KartState, g: &crate::RaceGlobals, w: &World) {
    if k.special != 0 {
        return;
    }
    if k.status & 0x20000 != 0 {
        // 0x80041A90: an effect's own target (+0x300..+0x308).
        k.aim_dist = 0;
        k.aim_x = k.target_x >> 5;
        k.aim_z = k.target_z >> 5;
        k.aim_y = k.target_yaw >> 5;
        return;
    }
    if g.race_clock == 0 || (k.player_index as i32) >= g.humans as i32 || k.blink != 1 {
        return;
    }
    let pts = &w.course.look_points;
    if !pts.is_empty() {
        // FUN_80081170(x >> 5, y >> 5, z >> 5, list, 1, 0, 0): the nearest
        // point, compared at >> 10 world units, first minimum.
        let (x, y, z) = ((k.x >> 5) >> 5, (k.y >> 5) >> 5, (k.z >> 5) >> 5);
        let mut best = u32::MAX;
        let mut idx = None;
        for (i, p) in pts.iter().enumerate() {
            let dx = (p[0] >> 5) - x;
            let dy = (p[1] >> 5) - y;
            let dz = (p[2] >> 5) - z;
            let d = dx.wrapping_mul(dx).wrapping_add(dy.wrapping_mul(dy)).wrapping_add(dz.wrapping_mul(dz)) as u32;
            if d < best {
                best = d;
                idx = Some(i);
            }
        }
        if let Some(i) = idx {
            let p = pts[i];
            let (kx, ky, kz) = (k.x >> 10, k.y >> 10, k.z >> 10);
            let dx = (p[0] >> 5) - kx;
            let dy = (p[1] >> 5) - ky;
            let dz = (p[2] >> 5) - kz;
            let d = dx.wrapping_mul(dx).wrapping_add(dy.wrapping_mul(dy)).wrapping_add(dz.wrapping_mul(dz));
            if d < k.aim_dist {
                k.aim_dist = d;
                k.aim_x = p[0];
                k.aim_y = p[1];
                k.aim_z = p[2];
            }
        }
    }
    if g.race_flags & 0x40 != 0 {
        crate::unreachable_in_scope!("race flag 0x40 aim at the boss");
    }
}

/// FUN_80041CC4 for one kart: while its camera (+0x4F2; one camera in a
/// one-player race) is active (`cam_mode` = camera +0x64 != -1), a kart in
/// status 0x10000, a finished one (not in status 0x20000), or any kart
/// during the intro (countdown >= 300) looks at the camera: aim = the eye
/// position (camera +0x28) >> 5, aim distance 0.
pub fn aim_camera_override(k: &mut KartState, g: &crate::RaceGlobals, cam_mode: i16, cam_pos: [i32; 3]) {
    if cam_mode == -1 {
        return;
    }
    let take = k.status & 0x10000 != 0
        || (k.special == 0 && ((k.finished != 0 && k.status & 0x20000 == 0) || g.countdown >= g.countdown_len));
    if take {
        k.aim_dist = 0;
        k.aim_x = cam_pos[0] >> 5;
        k.aim_y = cam_pos[1] >> 5;
        k.aim_z = cam_pos[2] >> 5;
    }
}

/// FUN_80086DF0 + FUN_80025D8C for kart 0 (after the kart loop). Writes the
/// render fields into `rec` (the kart's 0x504-byte record, already holding
/// the template fields) and the look point / driver y into `k`.
pub fn after_kart_loop(k: &mut KartState, rec: &mut [u8], w: &World, drv: &DriverModel) {
    let t = &w.tables;
    // ---- FUN_80086DF0: wheel and driver part matrices ----
    if rec[0x4A4] as i8 != -1 && k.wheel_count != 0 {
        let steer = k.steer_lag as i32;
        for i in 0..k.wheel_count as usize {
            let f = k.wheels[i].flags;
            let o = 0x20C + 0x12 * i;
            let m = if f & 0x38 == 0 {
                if f & 0x40 == 0 {
                    let spin = if f & 0x80 == 0 { k.wheels[i].spin as i32 } else { 0 };
                    let st = if f & 2 != 0 {
                        ((steer as u16 & 0x7FF) as i32) << 1
                    } else if f & 4 != 0 {
                        (((-steer) as u16 & 0x7FF) as i32) << 1
                    } else {
                        0
                    };
                    rot_wheel(t, spin, st)
                } else {
                    let a = (((-steer) as u16 & 0x7FF) as i32) << 1;
                    k.wheels[i].spin = a as i16;
                    rot_axis(t, a, 1)
                }
            } else if f & 8 == 0 {
                if f & 0x10 == 0 {
                    rot_axis(t, k.wheels[i].spin as i32, 2)
                } else {
                    rot_axis(t, k.wheels[i].spin as i32, 1)
                }
            } else {
                rot_axis(t, k.wheels[i].spin as i32, 0)
            };
            put_m(rec, o, &m);
        }
    }
    if rec[0x4A5] as i8 != -1 {
        let mut b = k.sway_d as u16 as i32;
        if k.character == 0x0E || k.character == 0x20 {
            let v = b as i16 as i32;
            b = (v - (v >> 31)) >> 1;
        }
        let m = rot_xyz(t, k.sway_c as i32 & 0xFFF, b & 0xFFF, k.sway_e as i32 & 0xFFF);
        put_m(rec, 0x29C, &m);
        let ident = w.exe.bytes(crate::init::addr::IDENTITY, 18);
        rec[0x2AE..0x2AE + 18].copy_from_slice(&ident);
    }

    // ---- FUN_80025D8C ----
    let mut m0 = crate::body::body_matrix(t, k);
    if k.status & 0x0040_0000 != 0 {
        // 0x80025E34: tilted about x by +0x30C (effect 7).
        let a = k.eff_30c as i16 as i32;
        let s = (t.sin_raw(a) as i16 >> 2) as i16;
        let c = (t.cos_raw(a) as i16 >> 2) as i16;
        let r = [[0x1000, 0, 0], [0, c, -s], [0, s, c]];
        m0 = mul_matrix0(&m0, &r);
    }
    k.body = m0;
    put_m(rec, 0x18C, &m0);
    let sf = 0x80 - k.stat_d as i32;
    let off = mvec(0, div_pow2(k.bounce.wrapping_mul(sf), 15), 0, &m0);
    for i in 0..3 {
        p16(rec, 0x46A + 2 * i, off[i]);
    }
    let sf16 = (0x80 - (k.stat_d as u16 as i32)) as i16 as i32;
    let a = div_pow2((k.lean_a as i32).wrapping_mul(sf16), 7) & 0xFFF;
    let b = div_pow2((k.lean_b as i32).wrapping_mul(sf16), 7) & 0xFFF;
    let m2 = mul_matrix0(&m0, &rot_lean(t, b, a));
    put_m(rec, 0x1AC, &m2);
    k.vmat = m2;
    let m3 = mul_matrix0(&m2, &rot_lean(t, k.sway_b as i32 & 0xFFF, k.sway_a as i32 & 0xFFF));
    put_m(rec, 0x1CC, &m3);
    // Driver position (and the previous one).
    let (px, py, pz) = (g32(rec, 0x330), g32(rec, 0x338), g32(rec, 0x340));
    p32(rec, 0x334, px);
    p32(rec, 0x33C, py);
    p32(rec, 0x344, pz);
    let d = mvec(g16(rec, 0x182), g16(rec, 0x184), g16(rec, 0x186), &m2);
    let (o0, o1, o2) = (g16(rec, 0x46A), g16(rec, 0x46C), g16(rec, 0x46E));
    let dx = k.x.wrapping_add(o0).wrapping_add(d[0]);
    let dy = k.y.wrapping_add(o1).wrapping_add(d[1]);
    let dz = k.z.wrapping_add(o2).wrapping_add(d[2]);
    p32(rec, 0x330, dx);
    p32(rec, 0x338, dy);
    p32(rec, 0x340, dz);
    k.drv_x = dx;
    k.ref_y = dy;
    k.drv_z = dz;
    // Look point: the driver model's part 0 pivot through the driver matrix.
    let pv = drv.part0_pivot;
    let l = mvec(-pv[0], pv[1], -pv[2], &m3);
    k.look_x = dx.wrapping_add(l[0]);
    k.look_y = dy.wrapping_add(l[1]);
    k.look_z = dz.wrapping_add(l[2]);
    p32(rec, 0x324, k.look_x);
    p32(rec, 0x328, k.look_y);
    p32(rec, 0x32C, k.look_z);
    // Wheel world positions (+0x14 + 0x2C * i).
    let n1 = rec[0x171] as usize;
    let n2 = rec[0x170] as usize;
    for i in 0..n2.max(n1) {
        let b = 0x14 + 0x2C * i;
        let (cx, cy, cz) = (g32(rec, b), g32(rec, b + 8), g32(rec, b + 0x10));
        p32(rec, b + 4, cx);
        p32(rec, b + 0xC, cy);
        p32(rec, b + 0x14, cz);
        if i < n1 {
            let d = mvec(g16(rec, b + 0x18), 0, g16(rec, b + 0x1C), &m0);
            p32(rec, b, k.x.wrapping_add(d[0]));
            p32(rec, b + 8, k.y.wrapping_add(d[1]));
            p32(rec, b + 0x10, k.z.wrapping_add(d[2]));
        } else {
            let d = mvec(g16(rec, b + 0x18), g16(rec, b + 0x1A), g16(rec, b + 0x1C), &m0);
            p32(rec, b, k.x.wrapping_add(o0).wrapping_add(d[0]));
            p32(rec, b + 8, k.y.wrapping_add(o1).wrapping_add(d[1]));
            p32(rec, b + 0x10, k.z.wrapping_add(o2).wrapping_add(d[2]));
        }
    }
    // Light points (+0x11C + 0x18 * j, count +0x172).
    for j in 0..rec[0x172] as usize {
        let b = 0x11C + 0x18 * j;
        let d = mvec(g16(rec, b + 0xC), g16(rec, b + 0xE), g16(rec, b + 0x10), &m2);
        p32(rec, b, k.x.wrapping_add(o0).wrapping_add(d[0]));
        p32(rec, b + 4, k.y.wrapping_add(o1).wrapping_add(d[1]));
        p32(rec, b + 8, k.z.wrapping_add(o2).wrapping_add(d[2]));
    }
    // Wheel angles forced by flag 0x40 above.
    for i in 0..k.wheel_count as usize {
        let b = 0x14 + 0x2C * i + 0x1E;
        rec[b..b + 2].copy_from_slice(&k.wheels[i].spin.to_le_bytes());
    }
}

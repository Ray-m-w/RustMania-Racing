//! Per-kart drawing that the game does from the kart record alone (no state
//! of its own), docs/spec/effects.md "Kart effects":
//! - FUN_8007D660, the far-kart impostor: beyond the kart LOD distance
//!   (kart +0x4B4 >= 0x10, FUN_8002976C: (|d| >> 10)^2 / 0x1200 capped at
//!   0x20) the game draws no model but seven ball sprites: four wheels
//!   (black; red for vehicle 4), the body in the vehicle's colour (table
//!   0x800AC5E4) and the driver's two colours (table 0x800AC224), shaded by
//!   the kart's depth-cue factors +0x414..+0x41E;
//! - the function cases of FUN_80037B80 (the status effect +0x4F1): 0
//!   FUN_80036A34 (a 3 x 3 grid of four quads around the kart, both faces),
//!   0xB FUN_800365C0 (an additive box of six quads at +0x330), 0xC
//!   FUN_800376A4 (two additive triangles from the kart to +0x300), 0xE
//!   FUN_80037018 and 0x15 FUN_80037364 (one quad at the look point +0x324);
//!   all drawn by FUN_800313BC from its 0x4C-byte quad list.
//! [CODE 8007d660, 8002976c, 80037b80, 80036a34, 800365c0, 800376a4,
//! 80037018, 80037364, 800313bc, 8007d58c]

use bevy::prelude::*;

use crate::effects::{Billboard, FxTables};
use crate::psxfx::{Blend, FxPrim, FxVert};

fn i16at(r: &[u8], o: usize) -> i32 {
    i16::from_le_bytes([r[o], r[o + 1]]) as i32
}
fn i32at(r: &[u8], o: usize) -> i32 {
    i32::from_le_bytes([r[o], r[o + 1], r[o + 2], r[o + 3]])
}
/// C `a / b` (toward zero) is Rust's `/`; `(a + (2^k - 1 if a < 0)) >> k`:
fn sra_z(a: i32, k: u32) -> i32 {
    (if a < 0 { a + ((1 << k) - 1) } else { a }) >> k
}

/// Exe tables the kart drawing reads.
#[derive(Clone, Debug)]
pub struct KartFxTables {
    /// 0x800AC5E4: vehicle colour (r, g, b, _) by +0x4B1.
    pub vehicle: Vec<[u8; 4]>,
    /// 0x800AC224 (stride 0x14): two driver colours by +0x4B2.
    pub character: Vec<[u8; 6]>,
    /// 0x800B07B4..07B7: the impostor ball sprite (u, v, size, clut).
    pub ball: [u8; 4],
    /// 0x80020F3C: FUN_800365C0's six quads (corner indices).
    pub box6: [u8; 24],
    /// 0x80020F54: FUN_80036A34's four quads.
    pub grid4: [u8; 16],
    /// 0x800AC168: FUN_800861F0's four corners (x, z), times size / 16.
    pub decal_corners: [[i16; 2]; 4],
    /// The sine table 0x800A88C0 (1.0 = 16384; cos = entry + 1024).
    pub sin: Vec<i16>,
}

fn exe_off(a: u32) -> usize {
    (a - 0x8001_0000) as usize + 0x800
}

impl KartFxTables {
    pub fn load(exe: &[u8]) -> KartFxTables {
        let at = |a: u32, n: usize| exe[exe_off(a)..exe_off(a) + n].to_vec();
        let v = at(0x800A_C5E4, 4 * 32);
        let c = at(0x800A_C224, 0x14 * 0x29);
        KartFxTables {
            vehicle: v.chunks_exact(4).map(|x| x.try_into().unwrap()).collect(),
            character: c.chunks_exact(0x14).map(|x| x[..6].try_into().unwrap()).collect(),
            ball: at(0x800B_07B4, 4).try_into().unwrap(),
            box6: at(0x8002_0F3C, 24).try_into().unwrap(),
            grid4: at(0x8002_0F54, 16).try_into().unwrap(),
            decal_corners: {
                let c = at(0x800A_C168, 16);
                std::array::from_fn(|i| [i16::from_le_bytes([c[4 * i], c[4 * i + 1]]), i16::from_le_bytes([c[4 * i + 2], c[4 * i + 3]])])
            },
            sin: mr_assets::dat::SinTable::from_exe(exe).map(|t| t.0).unwrap_or_default(),
        }
    }
}

/// The kart LOD byte +0x4B4 as FUN_8002976C computes it from the eye.
pub fn lod(r: &[u8], eye: [i32; 3]) -> u8 {
    let d: Vec<i64> = (0..3).map(|k| ((i32at(r, 4 * k) - eye[k]) >> 10) as i64).collect();
    let s = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
    if s < 0x24001 {
        (s / 0x1200) as u8
    } else {
        0x20
    }
}

/// M * v >> 7 with the 3x3 i16 matrix at record offset `o` (row major).
fn mat(r: &[u8], o: usize, v: [i32; 3]) -> [i32; 3] {
    let m = |k: usize| i16at(r, o + 2 * k);
    [
        (v[0] * m(0) + v[1] * m(1) + v[2] * m(2)) >> 7,
        (v[0] * m(3) + v[1] * m(4) + v[2] * m(5)) >> 7,
        (v[0] * m(6) + v[1] * m(7) + v[2] * m(8)) >> 7,
    ]
}

/// FUN_8007D58C(x, y, z, M): M * (-x, y, -z) >> 7.
fn d58c(r: &[u8], o: usize, x: i32, y: i32, z: i32) -> [i32; 3] {
    mat(r, o, [-x, y, -z])
}

fn ball(t: &FxTables, kt: &KartFxTables, p: [i32; 3], hw: i32, hh: i32, v: u8, vh: i32, clut: u16, rgb: [u8; 3]) -> Billboard {
    let (tp, cl) = t.slot_attr(0x15, clut, 0x60);
    // FUN_80030474 takes view units (world >> 5): keep the truncation
    Billboard {
        pos: p.map(|c| (c >> 5) << 5),
        hw,
        hh,
        angle: 0,
        anchor: 0,
        flat: false,
        u: kt.ball[0],
        v,
        uw: kt.ball[2] as i32 - 1,
        vh,
        tpage: tp,
        clut: cl,
        rgb,
        semi: false,
        abr: 3,
        bias: 0,
    }
}

fn shade(c: u8, f: i32) -> u8 {
    sra_z(c as i32 * f, 12).min(0xFF) as u8
}

/// FUN_8007D660: the impostor of a kart at LOD >= 0x10 (`cam_yaw` = camera
/// +0x52).
pub fn impostor(t: &FxTables, kt: &KartFxTables, r: &[u8], cam_yaw: i32, out: &mut Vec<Billboard>) {
    if i32at(r, 0x354) as u32 & 0x0800_0000 != 0 {
        return;
    }
    let veh = r[0x4B1] as usize;
    let wheel_rgb = if veh == 4 { [0xC6, 0x20, 0x18] } else { [0, 0, 0] };
    let pos = [i32at(r, 0), i32at(r, 4), i32at(r, 8)];
    let scale = i16at(r, 0x442);
    let nw = r[0x171] as usize;
    for i in 0..nw {
        let o = 0x2C * i;
        let a = i16at(r, 0x36 + o) * scale;
        let a = if a < 0 { a + 0xFFF } else { a };
        let d = mat(r, 0x18C, [-i16at(r, 0x2C + o), i16at(r, 0x2E + o), -i16at(r, 0x30 + o)]);
        let w = ((a << 4) >> 16) as i16 as i32;
        let h = sra_z(w * 10, 3);
        out.push(ball(t, kt, [pos[0] + d[0], pos[1] + d[1], pos[2] + d[2]], w, h, 0x40, 0xF, 7, wheel_rgb));
    }
    // body
    let mut dy = (((r[0xE] as i32 | (r[0xF] as i32) << 8) - (cam_yaw & 0xFFFF)) as i16) as i32;
    let mut s12 = dy as i16;
    if dy > 0x800 {
        s12 = s12.wrapping_sub(0x1000);
    } else if dy < -0x800 {
        s12 = s12.wrapping_add(0x1000);
    }
    let s12 = (s12 as i32).abs();
    dy = (0x400 - s12).abs();
    let f = dy as i16 as i32;
    let sum_x = i16at(r, 0x164) + i16at(r, 0x166);
    let sum_z = i16at(r, 0x168) + i16at(r, 0x16A);
    let mut len = ((i16at(r, 0x166) - i16at(r, 0x164)).abs()) as i16 as i32;
    let mut wid = ((i16at(r, 0x16A) - i16at(r, 0x168)).abs()) as i16 as i32;
    if nw != 0 {
        len = sra_z(len * 3, 2) as i16 as i32;
        wid = sra_z(wid * 7, 3) as i16 as i32;
    }
    let a8 = { let v = len * f; if v < 0 { v + 0x3FF } else { v } };
    let a15 = { let v = wid * (0x400 - f); if v < 0 { v + 0x3FF } else { v } };
    let vc = kt.vehicle.get(veh).copied().unwrap_or([0x80, 0x80, 0x80, 0]);
    let rgb = [shade(vc[0], i16at(r, 0x414)), shade(vc[1], i16at(r, 0x418)), shade(vc[2], i16at(r, 0x41C))];
    let i6 = i16at(r, 0x184).abs() - (r[0x16E] as i32 | (r[0x16F] as i32) << 8);
    let i13 = i6 as i16 as i32;
    let mut h12 = i13;
    let lx = -(sum_x / 2) as i16 as i32;
    let ly = -(i16at(r, 0x16E) + i13 / 2);
    let lz = -(sum_z / 2) as i16 as i32;
    let d = mat(r, 0x1AC, [lx, ly, lz]);
    if veh == 7 {
        h12 = (i13 << 1) as i16 as i32;
    }
    let sc2 = scale * 2;
    let w = {
        let v = ((((a8 >> 10) as i16 as i32) + (a15 >> 10)) / 2) as i16 as i32 * sc2;
        (if v < 0 { v + 0x1FFF } else { v }) >> 13
    };
    let h = (h12 * sc2) / 0x3000;
    let p = [pos[0] + d[0] + i16at(r, 0x46A), pos[1] + d[1] + i16at(r, 0x46C), pos[2] + d[2] + i16at(r, 0x46E)];
    out.push(ball(t, kt, p, w, h, kt.ball[1], kt.ball[2] as i32 - 1, kt.ball[3] as u16, rgb));
    // driver: head (colour 1 at +0x330) and body (colour 2 at +0x324)
    let cc = kt.character.get(r[0x4B2] as usize).copied().unwrap_or([0x80; 6]);
    let rgb1 = [shade(cc[0], i16at(r, 0x416)), shade(cc[1], i16at(r, 0x41A)), shade(cc[2], i16at(r, 0x41E))];
    let s = i16at(r, 0x188).abs();
    let a = -s / 2;
    let d = [(a * i16at(r, 0x1CE)) >> 7, (a * i16at(r, 0x1D4)) >> 7, (a * i16at(r, 0x1DA)) >> 7];
    let hh = sra_z(s * scale * 4, 15);
    let hw = (s * scale * 5) / 0xA000;
    let p = [i32at(r, 0x330) + d[0], i32at(r, 0x338) + d[1], i32at(r, 0x340) + d[2]];
    let b6 = kt.ball[2];
    out.push(ball(t, kt, p, hw, hh, kt.ball[1] | (b6 >> 2), (b6 >> 1) as i32 - 1, kt.ball[3] as u16, rgb1));
    let rgb2 = [shade(cc[3], i16at(r, 0x416)), shade(cc[4], i16at(r, 0x41A)), shade(cc[5], i16at(r, 0x41E))];
    let s = i16at(r, 0x18A).abs();
    let c = r[0x4B2];
    let s2 = if c == 3 || c == 0x28 { s >> 1 } else { s };
    let a = -s / 2;
    let d = [(a * i16at(r, 0x1EE)) >> 7, (a * i16at(r, 0x1F4)) >> 7, (a * i16at(r, 0x1FA)) >> 7];
    let hw = sra_z(s2 * scale * 4, 15);
    let hh = sra_z(s * scale * 5, 15);
    let p = [i32at(r, 0x324) + d[0], i32at(r, 0x328) + d[1], i32at(r, 0x32C) + d[2]];
    out.push(ball(t, kt, p, hw, hh, 0x40, 0xF, 7, rgb2));
}

/// One entry of FUN_800313BC's quad list (0x4C bytes at 0x800C2668):
/// corners in view units (world >> 5), slot order v0 (+0), v1 (+0xC), v2
/// (+0x18), v3 (+0x24); the GPU gets v0, v1, v3, v2 (a triangle: v0, v1, v2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Quad3 {
    pub v: [[i32; 3]; 4],
    pub uv: [[u8; 2]; 4],
    pub rgb: [[u8; 3]; 4],
    /// Texture slot (+0x44), CLUT add (+0x46), tpage add (+0x48: 0x60 opaque).
    pub slot: usize,
    pub clut: u16,
    pub abr: u16,
    /// +0x4B: four corners (else a triangle of v0..v2).
    pub quad: bool,
    /// +0x4A: back faces culled (RotAverageNclip).
    pub cull: bool,
    /// FUN_800313BC's centre (view units) and OT bias.
    pub center: [i32; 3],
    pub bias: i16,
}

/// FUN_800313BC's cull: the centre within 0x2000 view units of the eye on
/// every axis.
fn near_eye(c: [i32; 3], eye: [i32; 3]) -> bool {
    (0..3).all(|k| ((c[k] - (eye[k] >> 5)) as u32).wrapping_add(0x2000) < 0x4001)
}

/// The function cases of FUN_80037B80 for one kart record.
pub fn status_quads(kt: &KartFxTables, r: &[u8], eye: [i32; 3], out: &mut Vec<Quad3>) {
    let pos = [i32at(r, 0), i32at(r, 4), i32at(r, 8)];
    let v5 = |p: [i32; 3]| p.map(|c| c >> 5);
    let add = |a: [i32; 3], d: [i32; 3]| [(a[0] + d[0]) >> 5, (a[1] + d[1]) >> 5, (a[2] + d[2]) >> 5];
    let t300 = |lim: i32| {
        let t = i16at(r, 0x468);
        if t > lim - 1 { (300 - t).min(lim) } else { t }
    };
    let mut push = |q: Quad3| {
        if near_eye(q.center, eye) {
            out.push(q);
        }
    };
    match r[0x4F1] {
        0 => {
            // FUN_80036A34: 3 x 3 grid in the kart's plane, 4 quads, both faces
            let t = t300(0x10);
            let x0 = sra_z((i16at(r, 0x164) - 0x80) * t, 4);
            let x1 = sra_z((i16at(r, 0x166) + 0x80) * t, 4);
            let z0 = sra_z((i16at(r, 0x168) + 0x80) * t, 4);
            let z1 = sra_z((i16at(r, 0x16A) - 0x80) * t, 4);
            let xs = [x0, 0, x1, x0, 0, x1, x0, 0, x1];
            let zs = [z0, z0, z0, 0, 0, 0, z1, z1, z1];
            let p: Vec<[i32; 3]> = (0..9).map(|i| add(pos, d58c(r, 0x18C, xs[i], 0, zs[i]))).collect();
            let (mut u, mut v) = ([0x40u8, 0x7F, 0x7F, 0x40], [0xC0u8, 0xC0, 0xFF, 0xFF]);
            for q in 0..4 {
                let ix = &kt.grid4[4 * q..4 * q + 4];
                let front = Quad3 {
                    v: std::array::from_fn(|k| p[ix[k] as usize]),
                    uv: std::array::from_fn(|k| [u[k], v[k]]),
                    rgb: [[0x80; 3]; 4],
                    slot: 0x14,
                    clut: 0xD,
                    abr: 0x60,
                    quad: true,
                    cull: true,
                    center: v5(pos),
                    bias: 0x20,
                };
                // the second list (0x800C2798): the same quad reversed, OT bias -0x20
                let back = Quad3 {
                    v: std::array::from_fn(|k| front.v[3 - k]),
                    uv: std::array::from_fn(|k| front.uv[3 - k]),
                    bias: -0x20,
                    ..front.clone()
                };
                push(front);
                push(back);
                u = [u[3], u[0], u[1], u[2]];
                v = [v[3], v[0], v[1], v[2]];
            }
        }
        0xB => {
            // FUN_800365C0: additive box at +0x330, six quads
            let t = i16at(r, 0x468).min(0x20);
            let w = (((r[0x188] as u32 | (r[0x189] as u32) << 8) + (r[0x18A] as u32 | (r[0x18B] as u32) << 8)) as u16) as i16 as i32;
            let n = sra_z((-w / 2) * t, 5);
            let pp = sra_z((w / 2) * t, 5);
            let top = sra_z(-(w + 0x40) * t, 5);
            let xs = [n, pp, pp, n, n, pp, pp, n];
            let ys = [top, top, top, top, 0, 0, 0, 0];
            let zs = [pp, pp, n, n, pp, pp, n, n];
            let base = [i32at(r, 0x330), i32at(r, 0x338), i32at(r, 0x340)];
            let p: Vec<[i32; 3]> = (0..8).map(|i| add(base, d58c(r, 0x1CC, xs[i], ys[i], zs[i]))).collect();
            let b = (0x2Cu8.wrapping_sub(r[0x468])).min(0x20);
            let c = b.wrapping_shl(2);
            for q in 0..6 {
                let ix = &kt.box6[4 * q..4 * q + 4];
                push(Quad3 {
                    v: std::array::from_fn(|k| p[ix[k] as usize]),
                    uv: [[0, 0xC0], [0x1F, 0xC0], [0x1F, 0xDF], [0, 0xDF]],
                    rgb: [[c; 3]; 4],
                    slot: 0x15,
                    clut: 0xC,
                    abr: 0x20,
                    quad: true,
                    cull: true,
                    center: v5(base),
                    bias: -0x30,
                });
            }
        }
        0xC => {
            // FUN_800376A4: two additive triangles from the kart to +0x300
            let tgt = v5([i32at(r, 0x300), i32at(r, 0x304), i32at(r, 0x308)]);
            let c = i32at(r, 0x310);
            let c4 = sra_z(c, 2) as u8;
            let c = c as u8;
            let h = -(i16at(r, 0x16E));
            let len = ((r[0x166] as u32 | (r[0x167] as u32) << 8).wrapping_sub(r[0x164] as u32 | (r[0x165] as u32) << 8) as u16) as i16 as i32;
            let wid = ((r[0x168] as u32 | (r[0x169] as u32) << 8).wrapping_sub(r[0x16A] as u32 | (r[0x16B] as u32) << 8) as u16) as i16 as i32;
            let tri = |a: [i32; 3], b: [i32; 3]| Quad3 {
                v: [add(pos, a), add(pos, b), tgt, tgt],
                uv: [[0x6F, 0xAF], [0x79, 0xAF], [0x74, 0xB9], [0x74, 0xB9]],
                rgb: [[c4; 3], [c4; 3], [c; 3], [c; 3]],
                slot: 0x14,
                clut: 9,
                abr: 0x20,
                quad: false,
                cull: false,
                center: v5(pos),
                bias: 0,
            };
            push(tri(d58c(r, 0x1AC, sra_z(-len, 2), h, 0), d58c(r, 0x1AC, sra_z(len, 2), h, 0)));
            push(tri(d58c(r, 0x1AC, 0, h, sra_z(-wid, 2)), d58c(r, 0x1AC, 0, h, sra_z(wid, 2))));
        }
        0xE | 0x15 => {
            // FUN_80037018 / FUN_80037364: one quad at the look point +0x324
            let t = t300(0x20);
            let (a, y0, y1, uv, slot, clut) = if r[0x4F1] == 0xE {
                let a = (sra_z(t * 0x100, 6)) as i16 as i32;
                let i3 = t * 0x180;
                let i3 = if i3 < 0 { i3 + 0x3F } else { i3 };
                let i4 = (i3 << 10) >> 16;
                (a, sra_z(i4 * -5, 2), sra_z(i4 * 3, 2), [[0u8, 0x80u8], [0x1F, 0x80], [0x1F, 0xAF], [0, 0xAF]], 0x14, 8)
            } else {
                let a = sra_z(t * 0x1900, 10) as i16 as i32;
                let i3 = t * 0x980;
                let i3 = if i3 < 0 { i3 + 0x3FF } else { i3 };
                (a, ((i3 << 6) >> 16) * -2, 0, [[0x40, 0xAD], [0x71, 0xAD], [0x71, 0xBF], [0x40, 0xBF]], 0x15, 9)
            };
            let base = [i32at(r, 0x324), i32at(r, 0x328), i32at(r, 0x32C)];
            let p = [
                add(base, d58c(r, 0x1EC, a, y0, 0xC0)),
                add(base, d58c(r, 0x1EC, -a, y0, 0xC0)),
                add(base, d58c(r, 0x1EC, -a, y1, 0xC0)),
                add(base, d58c(r, 0x1EC, a, y1, 0xC0)),
            ];
            push(Quad3 { v: p, uv, rgb: [[0x80; 3]; 4], slot, clut, abr: 0x60, quad: true, cull: false, center: v5(base), bias: 0 });
        }
        _ => {}
    }
}

/// FUN_800861F0: one entry of the ground-decal part of the shadow list
/// (0x800C8B38 entries 0xC6..0xE5 = 0x800CB058, count DAT_800B0DCD; both
/// cleared every frame by FUN_800306D0) drawn by FUN_8002ADA8 like the kart
/// shadows. `normal`: the face normal at the face pointer `p4` (+8..+C of a
/// TER face; pointer 0 = flat). Flags 0: shadow tile (0xA0, 0) 0x20, slot
/// 0x14, B - F; 1: slot 0x15 (0x80, 0xC0) 0x10, B - F; 2: (0x68, 0xA8)
/// 0x18, slot 0x14, B + F. [CODE 800861f0, 8003ccbc, 800252a4, 8002ada8,
/// 800306d0]
#[allow(clippy::too_many_arguments)]
pub fn decal(kt: &KartFxTables, pos: [i32; 3], normal: Option<[i16; 3]>, size: i32, scale: u8, intensity: u8, flags: u8) -> crate::kartshadow::ShadowRec {
    let sin = |a: i32| kt.sin.get((a & 0xFFF) as usize).map(|v| (*v as i32) >> 2).unwrap_or(0);
    let cos = |a: i32| kt.sin.get((a & 0xFFF) as usize + 1024).map(|v| (*v as i32) >> 2).unwrap_or(4096);
    // FUN_8003CCBC: angles of the normal (0, 0 without a face)
    let (a, b) = match normal {
        Some(n) => {
            let (nx, ny, nz) = (n[0] as i32, n[1] as i32, n[2] as i32);
            let a = (-mr_assets::dat::ratan2(nz, -ny)) & 0xFFF;
            let t = -ny * cos(a) - nz * sin(a);
            let t = (if t < 0 { t + 0xFFF } else { t }) >> 12;
            (a, mr_assets::dat::ratan2(nx, t) & 0xFFF)
        }
        None => (0, 0),
    };
    // FUN_800252A4(b, a): the rows used are m0, m2 = 0, m3, m5, m6, m8
    let mz = |x: i32, y: i32| {
        let p = x * y;
        (if p < 0 { p + 0xFFF } else { p }) >> 12
    };
    let m = [cos(b), -sin(b), 0, mz(sin(b), cos(a)), mz(cos(b), cos(a)), -sin(a), mz(sin(b), sin(a)), mz(cos(b), sin(a)), cos(a)];
    let m = m.map(|v| v as i16 as i32);
    let offs = kt.decal_corners.map(|c| {
        let q = |v: i16| {
            let p = v as i32 * size;
            (if p < 0 { p + 0xF } else { p }) >> 4
        };
        let (x, z) = (q(c[0]), q(c[1]));
        [((x * m[0] + z * m[2]) >> 12) as i16, ((x * m[3] + z * m[5]) >> 12) as i16, ((x * m[6] + z * m[8]) >> 12) as i16]
    });
    let (uv0, wh, slot, tpage_add) = match flags {
        2 => ([0x68, 0xA8], [0x18, 0x18], 0x14, 0x20),
        1 => ([0x80, 0xC0], [0x10, 0x10], 0x15, 0x40),
        _ => ([0xA0, 0x00], [0x20, 0x20], 0x14, 0x40),
    };
    crate::kartshadow::ShadowRec { origin: pos, offs, uv0, size: wh, slot, scale, count: 1, intensity, tpage_add }
}

/// The FUN_800861F0 calls of FUN_8006C388 (the character specials' per
/// effect animation, switch on +0x4F1 - 1) for one kart record, from the
/// fields that frame's code left in the record. `normal` resolves a TER
/// face pointer (+0x2E0 / +0x30C / +0x318) to its normal.
pub fn special_decals(kt: &KartFxTables, r: &[u8], normal: &dyn Fn(u32) -> Option<[i16; 3]>, out: &mut Vec<crate::kartshadow::ShadowRec>) {
    let eff = r[0x4F1] as i8;
    if eff < 0 {
        return;
    }
    let u32at = |o: usize| i32at(r, o) as u32;
    let clock = i16at(r, 0x468);
    let ground = [i32at(r, 0), i32at(r, 0x31C), i32at(r, 8)];
    match eff {
        // effects 4, 0x13, 0x14, 0x17: the kart's shadow, grown (it floats);
        // +0x300 = the size clock the same code just stored
        4 | 0x13 | 0x14 | 0x17 => {
            let size = match eff {
                4 => 4,
                0x17 => 6,
                _ => 3,
            };
            out.push(decal(kt, ground, normal(u32at(0x2E0)), size, (i32at(r, 0x300) << 3) as u8, 0x80, 0));
        }
        // effect 7: a slot 0x15 tile under the kart, size from +0x176
        7 => {
            let t = if clock > 0xF { 2 * (300 - clock).min(8) } else { clock };
            out.push(decal(kt, ground, normal(u32at(0x2E0)), (i16at(r, 0x176) / 0x30) as i16 as i32, (t << 3) as u8, 0xFF, 1));
        }
        // effect 0xC: the beam's glow at +0x300, intensity +0x310 (t * 8)
        0xC => {
            let p = [i32at(r, 0x300), i32at(r, 0x304), i32at(r, 0x308)];
            out.push(decal(kt, p, normal(u32at(0x30C)), 5, 0x80, i32at(r, 0x310) as u8, 2));
        }
        // effect 0xF: a shadow at +0x300 while +0x318 (a face) is set
        0xF => {
            if u32at(0x318) != 0 {
                let p = [i32at(r, 0x300), i32at(r, 0x304), i32at(r, 0x308)];
                out.push(decal(kt, p, normal(u32at(0x318)), 3, i32at(r, 0x30C) as u8, 0x80, 0));
            }
        }
        _ => {}
    }
}

/// A list quad as an effects-pass primitive (`None` when back-facing and
/// culled). `eye`/`rot`: the chase camera (Bevy).
pub fn quad_prim(t: &FxTables, q: &Quad3, eye: Vec3, rot: Quat) -> Option<FxPrim> {
    let w = |p: [i32; 3]| crate::to_bevy(p[0] << 5, p[1] << 5, p[2] << 5);
    // GPU order v0, v1, v3, v2 (triangles: v0, v1, v2, v2)
    let ord = if q.quad { [0usize, 1, 3, 2] } else { [0, 1, 2, 2] };
    let pos: [Vec3; 4] = std::array::from_fn(|k| w(q.v[ord[k]]));
    if q.cull {
        // NCLIP > 0 on the PS1 (screen y down) = clockwise on screen; in a
        // y-up view projection that is a negative 2D cross product
        let inv = rot.inverse();
        let s = |p: Vec3| {
            let v = inv * (p - eye);
            Vec2::new(v.x / -v.z, v.y / -v.z)
        };
        let (a, b, c) = (s(pos[0]), s(pos[1]), s(pos[2]));
        if (b - a).perp_dot(c - a) >= 0.0 {
            return None;
        }
    }
    let (tp, cl) = t.slot_attr(q.slot, q.clut, q.abr);
    let center = w(q.center);
    Some(FxPrim {
        v: std::array::from_fn(|k| FxVert {
            pos: pos[k],
            uv: [q.uv[ord[k]][0] as f32, q.uv[ord[k]][1] as f32],
            rgb: q.rgb[ord[k]].map(|c| c as f32 / 128.0),
        }),
        page: Some((tp & !0x60, cl)),
        blend: if q.abr & 0x60 == 0x60 { Blend::Opaque } else { Blend::Abr(((q.abr >> 5) & 3) as u8) },
        center,
        ground_eps: -1.0,
        ground_radius: 0.0,
        // FUN_800313BC sorts at (OTZ + bias) * 4, OTZ = view z / 4 [INFER]
        depth_pull: -(q.bias as f32) * 4.0 * 32.0 / crate::UNITS,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec() -> Vec<u8> {
        let mut r = vec![0u8; 0x504];
        // identity matrices (4.12 >> 7 scale: 0x80 = 1.0 here) at the four bases
        for o in [0x18C, 0x1AC, 0x1CC, 0x1EC] {
            for k in 0..3 {
                r[o + 8 * k..o + 8 * k + 2].copy_from_slice(&0x80i16.to_le_bytes());
            }
        }
        r
    }

    #[test]
    fn lod_matches_fun_8002976c() {
        let mut r = rec();
        r[0..4].copy_from_slice(&(0x10000i32 * 100).to_le_bytes());
        // |dx| >> 10 = 6400 -> 6400^2 >= 0x24001 -> 0x20
        assert_eq!(lod(&r, [0, 0, 0]), 0x20);
        r[0..4].copy_from_slice(&(200i32 << 10).to_le_bytes());
        assert_eq!(lod(&r, [0, 0, 0]), (200 * 200 / 0x1200) as u8);
    }

    #[test]
    fn decal_flat_corners_and_glow_texture() {
        let kt = KartFxTables {
            vehicle: vec![],
            character: vec![],
            ball: [0; 4],
            box6: [0; 24],
            grid4: [0; 16],
            decal_corners: [[-16, -16], [16, -16], [16, 16], [-16, 16]],
            sin: (0..5120).map(|i| ((i as f64) * std::f64::consts::TAU / 4096.0).sin().mul_add(16384.0, 0.0).round() as i16).collect(),
        };
        // no face: corners = table * size / 16 in the ground plane
        let d = decal(&kt, [100, 200, 300], None, 5, 0x80, 0x40, 2);
        assert_eq!(d.offs[0], [-5, 0, -5]);
        assert_eq!(d.offs[2], [5, 0, 5]);
        assert_eq!((d.uv0, d.size, d.slot, d.tpage_add, d.intensity, d.scale), ([0x68, 0xA8], [0x18, 0x18], 0x14, 0x20, 0x40, 0x80));
        // CLUT 9 as in the oracle's GP0: (0x68 >> 6) | ((0xA8 & 0xC0) >> 4)
        assert_eq!(d.clut_add(), 9);
        // a flat floor normal (0, -4096, 0) gives the same corners
        let f = decal(&kt, [0, 0, 0], Some([0, -4096, 0]), 5, 0x80, 0x40, 2);
        assert_eq!(f.offs, d.offs);
    }

    #[test]
    fn effect_0xe_quad_is_symmetric_about_the_look_point() {
        let mut r = rec();
        r[0x4F1] = 0xE;
        r[0x468..0x46A].copy_from_slice(&16i16.to_le_bytes());
        let kt = KartFxTables { vehicle: vec![], character: vec![], ball: [0xE2, 0x40, 0x10, 7], box6: [0; 24], grid4: [0; 16], decal_corners: [[0; 2]; 4], sin: vec![] };
        let mut out = Vec::new();
        status_quads(&kt, &r, [0, 0, 0], &mut out);
        assert_eq!(out.len(), 1);
        let q = &out[0];
        // a = 16 * 4 = 64 (model units, identity >> 7 * 0x80): corners at x = -64 / +64, >> 5
        assert_eq!(q.v[0][0], -64 >> 5);
        assert_eq!(q.v[1][0], 64 >> 5);
        assert_eq!(q.v[0][2], -0xC0 >> 5);
    }
}

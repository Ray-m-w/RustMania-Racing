//! FUN_8007F03C: the kart particles of one race frame (after the kart loop,
//! after FUN_8006C388; docs/spec/items.md "Kart particles").
//!
//! For every kart near a human camera (16-bit test, 10000 TER units on each
//! axis) the function draws random bytes from the pointer 0x800B0BCC and
//! spawns sprite particles (FUN_800870E8, [`super::particle`]) and spark
//! streaks (FUN_800326B4, [`super::spark`]): ground sparks, wheel dust or
//! the splash on surface 0xB, wake / dust trails by surface, rain hits
//! (weather flag), engine smoke from the light points, the turbo / special
//! smoke puff, the pickup and item sparkles. Every draw is kept: the random
//! pointer is the AI's random source too.
//!
//! It reads the kart records (0x800C30A8 + 0x504 * slot) as the game does,
//! so the caller passes each kart's current record bytes. Offsets below are
//! record offsets; the code addresses s1 = record + 0x340 and s4 = record.
//! The first loop of the function (track objects, 0x800B0C58 of them) is
//! not ported: DOCK has none (0x800B0C58 = 0 in every oracle race) [CODE].

use super::{particle, spark, Cameras};
use crate::globals::RaceGlobals;
use crate::render::{mul_matrix0, Mat};
use crate::world::World;

/// Colours of the surface-0xB splash, 3 bytes x 3 (exe 0x800AC1D8).
const SPLASH_RGB: u32 = 0x800A_C1D8;
/// Light-point colours of kind 2 points, 4 bytes x 8 (exe 0x800AC198).
const LIGHT_RGB: u32 = 0x800A_C198;
/// Dust colour on surfaces 4 / 0xF (exe 0x800ACD15..17).
const DUST_RGB: u32 = 0x800A_CD15;

struct Rec<'a>(&'a [u8]);

impl Rec<'_> {
    fn i32(&self, o: usize) -> i32 {
        i32::from_le_bytes([self.0[o], self.0[o + 1], self.0[o + 2], self.0[o + 3]])
    }
    fn u32(&self, o: usize) -> u32 {
        self.i32(o) as u32
    }
    fn i16(&self, o: usize) -> i32 {
        i16::from_le_bytes([self.0[o], self.0[o + 1]]) as i32
    }
    fn u16(&self, o: usize) -> i32 {
        u16::from_le_bytes([self.0[o], self.0[o + 1]]) as i32
    }
    fn u8(&self, o: usize) -> i32 {
        self.0[o] as i32
    }
    fn i8(&self, o: usize) -> i32 {
        self.0[o] as i8 as i32
    }
    fn mat(&self, o: usize) -> Mat {
        let mut m = [[0i16; 3]; 3];
        for (i, v) in m.iter_mut().flatten().enumerate() {
            *v = self.i16(o + 2 * i) as i16;
        }
        m
    }
}

/// A random byte read as a signed char.
fn sb(v: u8) -> i32 {
    v as i8 as i32
}

/// The random pointer: `peek(k)` reads 0x800B0BCC[k] without moving it.
struct Rng<'a> {
    g: &'a mut RaceGlobals,
    w: &'a World,
}

impl Rng<'_> {
    fn peek(&self, k: u32) -> u8 {
        self.w.rng_byte(self.g.rng.wrapping_add(k))
    }
    fn skip(&mut self, n: u32) {
        self.g.rng = self.g.rng.wrapping_add(n);
    }
    fn next(&mut self) -> u8 {
        let b = self.peek(0);
        self.skip(1);
        b
    }
}

/// Row `r` of `m` applied to (a, b) on the columns `c0`, `c1`, >> 7 (the
/// 0x800DFD88 offset vector).
fn rot2(m: &Mat, r: usize, a: i32, b: i32) -> i32 {
    a.wrapping_mul(m[r][0] as i32).wrapping_add(b.wrapping_mul(m[r][2] as i32)) >> 7
}

/// The stale colour registers of the game's call (s8 / fp, s7 and the
/// stack byte sp+0x88): a light point of an unknown kind spawns with
/// whatever the previous spawn of this call left in them.
#[derive(Clone, Copy, Default)]
struct Colour {
    r: i32,
    g: i32,
    b: i32,
}

/// FUN_8007F03C (the kart part, 0x8007F468..): `recs` = the records of the
/// karts in slot order.
pub fn frame(recs: &[&[u8]], g: &mut RaceGlobals, w: &World, cams: &Cameras) {
    // 0x800B0D80 & 1: race karts present (always in a race).
    let rain = g.kart_fx_rain;
    let dt = g.dt;
    let mut col = Colour::default();
    for rec in recs {
        let r = Rec(rec);
        let mut rng = Rng { g: &mut *g, w };
        kart(&r, &mut rng, cams, rain, dt, &mut col);
    }
}

fn kart(r: &Rec, rng: &mut Rng, cams: &Cameras, rain: bool, dt: i32, col: &mut Colour) {
    let (x, y, z) = (r.i32(0x0), r.i32(0x4), r.i32(0x8));
    if !cams.near(x >> 5, y >> 5, z >> 5, 10000) {
        return;
    }
    let on_ground = r.i8(0x4E0) != 0;
    let special = r.i8(0x4D6) != 0;
    let status = r.u32(0x354);
    let wheels = r.u8(0x171);
    let engine = r.i16(0x442);
    // 0x8007F5xx: ground sparks (scrape): bounce * (0x80 - stat) >> 15
    // against the spark threshold +0x16E, or no wheels touching.
    {
        let t = r.i32(0x348).wrapping_mul(0x80 - r.i16(0x180)) / 0x8000;
        if on_ground && r.u8(0x174) & 2 == 0 && (r.i16(0x16E) <= t || (wheels == 0 && status & 0x20_0000 == 0)) {
            let b = [rng.next(), rng.next(), rng.next()];
            spark(rng.g, [x, y, z], [sb(b[0]) << 1, sb(b[1]) << 1, sb(b[2]) << 1], 0x18, 0x10, [0xFF, 0x7F, 0]);
        }
    }
    // 0x8007F6xx..0x8007FDF8: wheel dust / the surface-0xB splash.
    let mut dust = false;
    let mut to_fdf8 = true;
    'wheel: {
        if status & 0x1000 != 0 {
            break 'wheel;
        }
        let splash = on_ground
            && r.u8(0x4F3) == 0xB
            && (r.i32(0x400) >= 0x101 || {
                let b = rng.next();
                b <= 0x3F
            });
        if splash {
            let b: Vec<u8> = (0..6).map(|k| rng.peek(k)).collect();
            let i = (b[0] as u32 / 0x56) * 3;
            let rgb = [rng.w.rng_byte(SPLASH_RGB + i) as i32, rng.w.rng_byte(SPLASH_RGB + i + 1) as i32, rng.w.rng_byte(SPLASH_RGB + i + 2) as i32];
            let pos = [r.i32(0x330) + sb(b[1]) * 0x20, r.i32(0x338), r.i32(0x340) + sb(b[2]) * 0x20];
            let vel = [sb(b[3]) << 2, b[4] as i32 * -2 - 0x200, sb(b[5]) << 2];
            rng.skip(6);
            particle(rng.g, pos, vel, 0x30, 0, 0, 0x15, rgb, 0x60, 0x60, 0x20);
            break 'wheel;
        }
        if special {
            // 0x8007FE38 directly.
            to_fdf8 = false;
            dust = true;
            break 'wheel;
        }
        if !(r.i8(0x4E7) == -1 && wheels != 0) {
            break 'wheel;
        }
        let rgb = [r.u8(0x4BB), r.u8(0x4BC), r.u8(0x4BD)];
        let size = |sel: usize| ((r.i16(0x36 + 0x2C * sel).wrapping_mul(engine) / 0x1000) as i16) as i32;
        let m = r.mat(0x18C);
        let fwd = r.i32(0x390).wrapping_abs();
        let side = r.i32(0x394).wrapping_abs();
        if (fwd < side || r.i8(0x4C9) != 0) && on_ground {
            // Sideways: dust from a random wheel.
            let sel = (rng.peek(0) as i32 % wheels) as usize;
            let a = -r.i16(0x2C + 0x2C * sel);
            let c = -r.i16(0x30 + 0x2C * sel);
            let pos = [x + rot2(&m, 0, a, c), y + rot2(&m, 1, a, c), z + rot2(&m, 2, a, c)];
            let vel = [sb(rng.peek(1)) / 2, 0, sb(rng.peek(2)) / 2];
            let spin = sb(rng.peek(3)) / 2;
            rng.skip(4);
            let s = size(sel);
            particle(rng.g, pos, vel, -12, 0, spin, 2, rgb, s, s, 0x20);
        } else {
            if r.i16(0x486) < 0x41 || !on_ground {
                break 'wheel;
            }
            // Drifting: a random wheel, only a driven one (+0x3A & 1);
            // otherwise the rest of this kart is skipped (0x8008110C).
            let sel = (rng.next() as i32 % wheels) as usize;
            if r.u16(0x3A + 0x2C * sel) & 1 == 0 {
                return;
            }
            let a = -r.i16(0x2C + 0x2C * sel);
            let c = -r.i16(0x30 + 0x2C * sel);
            let e = |row: usize| (m[row][1] as i32).wrapping_mul(-0x100).wrapping_add((m[row][2] as i32).wrapping_mul(0x300));
            let pos = [x + rot2(&m, 0, a, c), y + rot2(&m, 1, a, c), z + rot2(&m, 2, a, c)];
            let vel = [(e(0) >> 12) + r.i32(0x2F4) / 2, (e(1) >> 12) + r.i32(0x2F8) / 2, (e(2) >> 12) + r.i32(0x2FC) / 2];
            let spin = sb(rng.peek(0)) / 2;
            rng.skip(1);
            let s = size(sel);
            particle(rng.g, pos, vel, 0, 0, spin, 2, rgb, s, s, 0x20);
        }
    }
    if to_fdf8 {
        // 0x8007FDF8
        dust = special || (r.i32(0x31C).wrapping_sub(y) < 0x1000 && r.i32(0x400) > 0x200);
    }
    // 0x8007FE38..: the trail under the kart: water spray while floating
    // (+0x4C8 / +0x4C5), else dust by surface (1, 2: grey; 4, 0xF: brown).
    let mut kind = -1;
    let (mut vx, mut vz) = (0, 0);
    let spray = r.i8(0x4C8) != 0 || (dust && r.i8(0x4C5) != 0);
    if spray {
        kind = 0xF;
        *col = Colour { r: 0, g: 0x80, b: 0x80 };
        vx = sb(rng.peek(0));
        vz = sb(rng.peek(1));
        rng.skip(2);
    } else if dust {
        let s = r.u8(0x4F3);
        if ((s - 1) as u32) < 2 {
            kind = 0xB;
            *col = Colour { r: 0x80, g: 0x80, b: 0x80 };
        } else if s == 4 || s == 0xF {
            kind = 0x14;
            let c = |k| rng.w.rng_byte(DUST_RGB + k) as i32;
            *col = Colour { r: c(0), g: c(1), b: c(2) };
        }
        vx = sb(rng.peek(0)) / 2;
        vz = sb(rng.peek(1)) / 2;
        rng.skip(2);
    }
    if kind != -1 {
        let (px, py, pz, mut size);
        if wheels == 0 || special {
            px = x + sb(rng.peek(0)) * 0x20;
            pz = r.i32(0x8) + sb(rng.peek(1)) * 0x20;
            if kind == 0xF {
                py = r.i32(0x398);
                rng.skip(2);
            } else {
                py = r.i32(0x31C) + rng.peek(2) as i32 * -8;
                rng.skip(3);
            }
            size = ((rng.next() >> 3) | 0x40) as i32;
        } else {
            let sel = (rng.peek(0) as i32 % wheels) as usize;
            let wb = 0x14 + 0x2C * sel;
            if kind == 0xF {
                py = r.i32(0x398);
                px = r.i32(wb) + sb(rng.peek(1)) * 0x10;
                pz = r.i32(wb + 0x10) + sb(rng.peek(2)) * 0x10;
            } else {
                px = r.i32(wb) + sb(rng.peek(1)) * 4;
                pz = r.i32(wb + 0x10) + sb(rng.peek(2)) * 4;
                py = if special { r.i32(0x31C) } else { r.i32(wb + 8) };
            }
            rng.skip(3);
            size = r.u16(wb + 0x22);
        }
        if !special {
            size = ((size as i16 as i32).wrapping_mul(engine) / 0x1000) as u16 as i32;
        }
        let (gravity, angle, spin) = if kind == 0xF {
            (0, 0, 0)
        } else {
            let a = (rng.peek(0) as i32) << 4;
            let s = sb(rng.peek(1)) / 2;
            rng.skip(2);
            (-3, a, s)
        };
        let s = size as i16 as i32;
        particle(rng.g, [px, py, pz], [vx, 0, vz], gravity, angle, spin, kind, [col.r, col.g, col.b], s, s, 0x20);
    }
    // 0x8008007x..: rain hits on the kart (weather 0x800B0B9A == 2 or
    // 0x800B07E2 & 9) once it is moving (+0x440 > 800): dt tries.
    if rain && r.i16(0x440) > 800 {
        let m = r.mat(0x18C);
        let mut n = dt;
        loop {
            let b0 = rng.peek(0);
            if b0 < 0x40 {
                let b: Vec<u8> = (0..7).map(|k| rng.peek(k)).collect();
                let fx = (b[1] as i32).wrapping_mul(r.i16(0x166) - r.i16(0x164)) / 256;
                let fz = (b[2] as i32).wrapping_mul(r.i16(0x168) - r.i16(0x16A)) / 256;
                let a = -(r.i16(0x164) + fx);
                let c = -(r.i16(0x16A) + fz);
                let d = [rot2(&m, 0, a, c), rot2(&m, 1, a, c), rot2(&m, 2, a, c)];
                let half = r.i16(0x176) / 2;
                let v = half.wrapping_mul(sb(b[3])) / 256;
                let s = (((half + v).wrapping_mul(engine) / 0x1000) as i16) as i32;
                let spin = sb(b[5]) / 4;
                let kind = if b[6] > 0x1F { 3 } else { 0x11 };
                rng.skip(7);
                particle(rng.g, [x + d[0], y + d[1], r.i32(0x8) + d[2]], [0; 3], -16, (b[4] as i32) << 4, spin, kind, [0x80; 3], s, s, 0x20);
            } else {
                rng.skip(1);
            }
            n -= 1;
            if n <= 0 {
                break;
            }
        }
    }
    // 0x800803xx..: exhaust / light points while running normally.
    if status & 0x1001 == 0 && !special && r.i8(0x4E7) == -1 && engine > 0x20 {
        let lights = r.u8(0x172);
        if lights == 0 {
            // Engine smoke from each wheel part flagged 0x40.
            let n = r.u8(0x170);
            if n != 0 && r.i8(0x4A5) != -1 {
                let a = r.mat(0x1AC);
                for i in 0..n as usize {
                    if r.u16(0x3A + 0x2C * i) & 0x40 == 0 {
                        continue;
                    }
                    if r.u8(0x4AA) == 0 {
                        let b = rng.next();
                        if b >= 0x20 {
                            continue;
                        }
                    }
                    let mm = mul_matrix0(&a, &r.mat(0x20C + 0x12 * i));
                    let d = [mm[0][2] as i32 >> 2, mm[1][2] as i32 >> 2, mm[2][2] as i32 >> 2];
                    let b: Vec<u8> = (0..5).map(|k| rng.peek(k)).collect();
                    rng.skip(5);
                    let wb = 0x14 + 0x2C * i;
                    particle(
                        rng.g,
                        [r.i32(wb) + d[0] * 4, r.i32(wb + 8) + d[1] * 4, r.i32(wb + 0x10) + d[2] * 4],
                        [r.i32(0x2F4) / 2 + d[0] / 2 + sb(b[0]), d[1] / 2 + sb(b[1]), r.i32(0x2FC) / 2 + d[2] / 2 + sb(b[2])],
                        0,
                        (b[3] as i32) << 4,
                        sb(b[4]) / 2,
                        10,
                        [0xC0, 0x80, 0x40],
                        0x30,
                        0x30,
                        0xC,
                    );
                }
            }
        } else {
            // A random light point (one in 8 while cruising, else one in
            // 64 tries hits a point index below the count).
            let calm = r.i8(0x4C4) == 0 && r.i8(0x4ED) == 0 && (r.u8(0x4AA) == 0 || r.u8(0x4AA) > 0x1D);
            let b0 = rng.peek(0) as i32;
            let j = if calm { b0 >> 3 } else { b0 >> 6 };
            if j >= lights {
                rng.skip(1);
            } else {
                let p = 0x11C + 0x18 * j as usize;
                let mut c = r.i8(p + 0x12);
                if c == 3 {
                    c = 1;
                }
                let mut kind = 2;
                let mut sp: i32 = 0;
                match c {
                    1 => {
                        kind = 4;
                        col.b = 0x80;
                        col.r = (rng.peek(1) >> 2) as i32;
                        col.g = (rng.peek(2) >> 2) as i32;
                        rng.skip(3);
                    }
                    0 | 2 | 4 => {
                        let v;
                        if c == 0 {
                            kind = 8;
                            *col = Colour { r: 0x80, g: 0x80, b: 0x80 };
                            v = rng.peek(1);
                            rng.skip(2);
                        } else if c == 2 {
                            let i = ((rng.peek(1) >> 5) as u32) * 4;
                            let t = |k| rng.w.rng_byte(LIGHT_RGB + i + k) as i32;
                            *col = Colour { r: t(0), g: t(1), b: t(2) };
                            v = rng.peek(2);
                            kind = 5;
                            rng.skip(3);
                        } else {
                            kind = 10;
                            *col = Colour { r: 0xC0, g: 0x80, b: 0x40 };
                            v = rng.peek(1);
                            rng.skip(2);
                        }
                        sp = sb(v) / 2;
                    }
                    _ => {
                        // 0x80080750 with the stale colours.
                        rng.skip(1);
                    }
                }
                let f = r.u8(p + 0x13);
                let ax = if f & 2 != 0 { -0x100 } else { (f & 1) << 8 };
                let ay = if f & 8 != 0 { -0x100 } else { (f & 4) << 6 };
                let az = if f & 0x20 != 0 { -0x100 } else { (f & 0x10) << 4 };
                let cnt = (ax != 0) as i32 + (ay != 0) as i32 + (az != 0) as i32;
                let scale = match cnt {
                    1 => 0x1000,
                    2 => 0xB4F,
                    3 => 0x93C,
                    _ => 0,
                };
                let ax = -(ax * scale / 0x1000);
                let ay = ay * scale / 0x1000;
                let az = -(az * scale / 0x1000);
                let m = r.mat(0x1AC);
                let row = |k: usize| ax * m[k][0] as i32 + ay * m[k][1] as i32 + az * m[k][2] as i32;
                let (e0, e1, e2) = (row(0), row(1), row(2));
                let sz = (engine / 128) * ((rng.peek(0) as i32 >> 1) + 0x100) / 256;
                let sz = sz as i16 as i32;
                let angle = if sp & 0xFFFF == 0 {
                    rng.skip(1);
                    0
                } else {
                    let a = (rng.peek(1) as i32) << 4;
                    rng.skip(2);
                    a
                };
                let life = if kind == 10 || kind == 8 { 0xC } else { 0x18 };
                particle(
                    rng.g,
                    [r.i32(p), r.i32(p + 4), r.i32(p + 8)],
                    [(e0 >> 12) + r.i32(0x2F4) / 2, e1 >> 12, (e2 >> 12) + r.i32(0x2FC) / 2],
                    -8,
                    angle,
                    sp as i16 as i32,
                    kind,
                    [col.r, col.g, col.b],
                    sz,
                    sz,
                    life,
                );
            }
        }
    }
    // 0x80080Cxx: the turbo / special / skid puff behind the kart.
    if r.i8(0x4C4) != 0 || special || r.i8(0x4ED) != 0 {
        let m = r.mat(0x1AC);
        let d = |k: usize| ((m[k][1] as i32) * -0x60 + (m[k][2] as i32) * 0x80) >> 7;
        let s = if special { 0x80 } else { ((engine / 32) as i16) as i32 };
        let b: Vec<u8> = (0..7).map(|k| rng.peek(k)).collect();
        rng.skip(7);
        particle(
            rng.g,
            [x + d(0) + sb(b[0]) * 0x10, y + d(1) + sb(b[1]) * 0x10, r.i32(0x8) + d(2) + sb(b[2]) * 0x10],
            [r.i32(0x2F4) / 4 + sb(b[3]), r.i32(0x2F8) / 4 + sb(b[4]), r.i32(0x2FC) / 4 + sb(b[5])],
            0,
            0,
            sb(b[6]) / 2,
            0x11,
            [0x80; 3],
            s,
            s,
            0x20,
        );
    }
    // 0x80080E90: sparkles around the driver: a pickup being collected
    // (+0x4E6, timer +0x4B5 outside 60..180 and not 0xF0), the item +0x4E7
    // early (+0x4B6 < 60), +0x4EE while the engine runs.
    let e7 = r.i8(0x4E7) != -1;
    if (r.i8(0x4E6) != -1 && r.u8(0x4B5) != 0xF0 && (r.u8(0x4B5) - 0x3C) as u32 > 0x78)
        || (e7 && r.u8(0x4B6) < 0x3C)
        || (r.i8(0x4EE) != -1 && engine > 0)
    {
        let b: Vec<u8> = (0..7).map(|k| rng.peek(k)).collect();
        rng.skip(7);
        particle(
            rng.g,
            [r.i32(0x330) + sb(b[0]) * 0x20, r.i32(0x338) + sb(b[1]) * 0x10, r.i32(0x340) + sb(b[2]) * 0x20],
            [sb(b[3]) << 1, sb(b[4]), sb(b[5]) << 1],
            0,
            0,
            sb(b[6]) / 2,
            1,
            [0x80; 3],
            0x60,
            0x60,
            0x10,
        );
    }
    if e7 && r.u8(0x4B6) > 0x3B {
        let b: Vec<u8> = (0..7).map(|k| rng.peek(k)).collect();
        rng.skip(7);
        particle(
            rng.g,
            [r.i32(0x330) + sb(b[0]) * 0x20, r.i32(0x338) + sb(b[1]) * 0x10, r.i32(0x340) + sb(b[2]) * 0x20],
            [sb(b[3]), sb(b[4]), sb(b[5])],
            0,
            0,
            sb(b[6]) / 2,
            0x11,
            [0x80; 3],
            0x80,
            0x80,
            0x20,
        );
    }
}

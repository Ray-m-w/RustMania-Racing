//! CHICKEN CHASE (game mode 0x14, race flag 0x40; phase 7,
//! docs/spec/battle.md "CHICKEN CHASE"): the chicken 0x800CB658.. and the
//! catches.
//!
//! - [`spawn`] = FUN_8006E564 (race load, and after every catch): a place
//!   at random (a random path point of a random path segment, or near a
//!   random kart on a course without a path), away from every kart, on the
//!   ground; a random heading.
//! - [`frame`] = FUN_8006EC4C (race frame, after the projectiles, race flag
//!   0x40): the chicken hops (gravity 0x60 a VBlank; at random a new hop on
//!   landing), turns toward the nearest kart; a kart within 0x100 ((d >>
//!   10)^2 in driver-position units) catches it: +0x4AC + 1, the first to
//!   0x800B0EBD wins and both humans are out; the chicken bursts
//!   ([`burst`] = FUN_8006EA2C) and appears elsewhere.
//! The chicken never moves sideways: it hops in place [CODE 8006ec4c].

use crate::globals::RaceGlobals;
use crate::kart::KartState;
use crate::world::World;

/// The chicken record 0x800CB658..0x800CB680 and the catches to win
/// 0x800B0EBD.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Chicken {
    /// 0x800CB658 / 5C / 60: position (world units).
    pub pos: [i32; 3],
    /// 0x800CB664: vertical speed (y down: negative = rising).
    pub vy: i32,
    /// 0x800CB668: the flag-0x40 surface it lands on first (with `has_floor`).
    pub floor: i32,
    /// 0x800CB66C: the ground height under it.
    pub ground: i32,
    /// 0x800CB670 != 0: a surface below was found (it hops).
    pub on_ground: bool,
    /// 0x800CB674 != 0: a flag-0x40 surface above the ground.
    pub has_floor: bool,
    /// 0x800CB678: heading (12 bits).
    pub heading: i16,
    /// 0x800CB67C: its place along the stunt strip (race type 3 only).
    pub strip: i32,
    /// 0x800B0EBD: catches to win (10; 5 on a stunt course).
    pub target: u8,
}

pub const ADDR: u32 = 0x800C_B658;
pub const TARGET: u32 = 0x800B_0EBD;

impl Chicken {
    pub fn read(m: &crate::mem::Mem) -> Chicken {
        Chicken {
            pos: [m.i32(ADDR), m.i32(ADDR + 4), m.i32(ADDR + 8)],
            vy: m.i32(ADDR + 0xC),
            floor: m.i32(ADDR + 0x10),
            ground: m.i32(ADDR + 0x14),
            on_ground: m.u32(ADDR + 0x18) != 0,
            has_floor: m.u32(ADDR + 0x1C) != 0,
            heading: m.i16(ADDR + 0x20),
            strip: m.i32(ADDR + 0x24),
            target: m.u8(TARGET),
        }
    }
}

fn byte(g: &mut RaceGlobals, w: &World) -> u8 {
    let b = w.rng_byte(g.rng);
    g.rng = g.rng.wrapping_add(1);
    b
}

/// FUN_8003DDB0 at `p`: (0x800B0AE4 found, 0x800B0AE0 != 0, 0x800B0B04,
/// 0x800B0C84 != 0, 0x800B0C9C).
fn ground(w: &World, p: [i32; 3]) -> (bool, bool, i32, bool, i32) {
    let q = w.ter.ground(p[0], p[1], p[2]);
    (q.found, q.below.is_some() || q.nearest.is_some(), q.below.map(|s| s.y).unwrap_or(q.nearest_y), q.alt.is_some(), q.alt.map(|s| s.y).unwrap_or(0))
}

/// FUN_8006E564: place the chicken. `karts`: every kart's (x, y, z) (+0,
/// +4, +8), slot order (0x800B0DFE of them).
pub fn spawn(c: &mut Chicken, g: &mut RaceGlobals, w: &World, karts: &[[i32; 3]], rand: &mut dyn FnMut() -> i32) {
    let mut near: i16 = 0x41;
    c.vy = 0;
    let segs = &w.ter.path_segments;
    loop {
        if near != 0 {
            near -= 1;
        }
        if segs.is_empty() {
            let k = (byte(g, w) >> 7) as usize;
            let base = karts.get(k).copied().unwrap_or_default();
            for (i, b) in base.iter().enumerate() {
                c.pos[i] = b.wrapping_add((rand() - 0x4000).wrapping_mul(0x20));
            }
        } else {
            let step = (0x7FFF / segs.len() as i32 + 1) as i16 as i32;
            let mut si;
            loop {
                si = (rand() / step) as i16 as usize;
                if segs.get(si).map_or(0x80, |s| s.flags) != 0x80 {
                    break;
                }
            }
            let s = &segs[si];
            let pstep = (0x7FFF / s.count as i32 + 1) as i16 as i32;
            let pt = (rand() / pstep) as i16;
            let p = &w.ter.path_points[s.first.wrapping_add(pt) as u16 as usize];
            c.pos = [p.x << 5, p.y << 5, p.z << 5];
        }
        let half = near as i32 * 0x1000;
        let size = ((near as i32) << 13) as u32;
        let close = karts.iter().any(|k| {
            (c.pos[0].wrapping_sub(k[0]).wrapping_add(half) as u32) < size
                && (c.pos[2].wrapping_sub(k[2]).wrapping_add(half) as u32) < size
                && (c.pos[1].wrapping_sub(k[1]).wrapping_add(half) as u32) < size
        });
        if close {
            continue;
        }
        let (found, below, below_y, _, _) = ground(w, c.pos);
        if !found {
            continue;
        }
        c.on_ground = below;
        if below {
            c.pos[1] = below_y - 0x2000;
            c.ground = below_y;
            let (_, _, _, alt, alt_y) = ground(w, c.pos);
            if !alt || c.ground <= alt_y {
                c.has_floor = false;
            } else {
                c.floor = alt_y;
                c.has_floor = true;
            }
        }
        c.heading = ((byte(g, w) as u16) << 4) as i16;
        if g.race_type == 3 {
            c.strip = crate::stunt::strip_progress(w, c.pos[0] >> 5, c.pos[2] >> 5);
        }
        return;
    }
}

/// FUN_8006EA2C: the burst of feathers at the chicken (one puff, kind
/// 0x11, and five feathers, kind 9: 2 + 5 * 9 random bytes).
pub fn burst(c: &Chicken, g: &mut RaceGlobals, w: &World) {
    let b0 = byte(g, w) as i32;
    let spin = if byte(g, w) < 0x80 { 0x40 } else { -0x40 };
    crate::items::particle(g, c.pos, [0; 3], 0, b0 << 4, spin, 0x11, [0x80; 3], 0x180, 0x180, 0x20);
    for _ in 0..5 {
        let b: [u8; 9] = std::array::from_fn(|_| byte(g, w));
        let size = (b[0] >> 2 | 0x40) as i32;
        let pos = [c.pos[0] + (b[1] as i8 as i32) * 0x20, c.pos[1] + (b[2] as i8 as i32) * 0x20, c.pos[2] + (b[3] as i8 as i32) * 0x20];
        let vel = [(b[4] as i8 as i32) << 2, (b[5] as i8 as i32) << 2, (b[6] as i8 as i32) << 2];
        let s = b[8] as i8 as i32;
        crate::items::particle(g, pos, vel, 0, (b[7] as i32) << 4, (s - (s >> 31)) >> 1, 9, [0x80; 3], size, size, 0x20);
    }
}

/// FUN_8006EC4C: one race frame of the chicken (race flag 0x40).
pub fn frame(ks: &mut [KartState], g: &mut RaceGlobals, w: &World, rand: &mut dyn FnMut() -> i32) {
    let mut c = g.chicken;
    let dt = g.dt as u16 as i32;
    if c.on_ground {
        let mut n = dt;
        loop {
            n -= 1;
            c.vy = c.vy.wrapping_add(0x60);
            c.pos[1] = c.pos[1].wrapping_add(c.vy);
            if (n << 16) <= 0 {
                break;
            }
        }
        if c.has_floor && c.pos[1] - 0x2000 <= c.floor {
            c.vy = 0;
            c.pos[1] = c.floor + 0x2000;
        }
        if c.ground <= c.pos[1] + 0x2000 {
            c.pos[1] = c.ground - 0x2000;
            if w.rng_byte(g.rng) < 0x20 {
                let b1 = w.rng_byte(g.rng.wrapping_add(1)) as i32;
                c.vy = ((b1 >> 3) + 0x20) * -0x20;
                g.rng = g.rng.wrapping_add(2);
                crate::sound::at(g, w, 0xB, 0x1600, 0xA0, c.pos, -1);
            } else {
                c.vy = 0;
                g.rng = g.rng.wrapping_add(1);
            }
        }
        // FUN_800861F0: its shadow (render).
    }
    // The nearest kart by driver position (+0x330 / +0x338 / +0x340).
    let n = (g.karts.max(0) as usize).min(ks.len());
    let mut best: Option<(usize, i32)> = None;
    for (i, k) in ks.iter().enumerate().take(n) {
        let dx = k.drv_x.wrapping_sub(c.pos[0]) >> 10;
        let dy = k.ref_y.wrapping_sub(c.pos[1]) >> 10;
        let dz = k.drv_z.wrapping_sub(c.pos[2]) >> 10;
        let d = dx.wrapping_mul(dx).wrapping_add(dy.wrapping_mul(dy)).wrapping_add(dz.wrapping_mul(dz));
        if best.is_none_or(|b| d < b.1) {
            best = Some((i, d));
        }
    }
    let Some((i, d)) = best else {
        g.chicken = c;
        return;
    };
    let a = crate::tables::ratan2(&w.tables, ks[i].x.wrapping_sub(c.pos[0]) >> 10, ks[i].z.wrapping_sub(c.pos[2]) >> 10);
    let mut n = dt;
    loop {
        let mut e = (a & 0xFFF) - c.heading as i32;
        if e > 0x800 {
            e -= 0x1000;
        } else if e < -0x800 {
            e += 0x1000;
        }
        if e < 0 {
            e += 7;
        }
        n -= 1;
        c.heading = (c.heading.wrapping_add((e >> 3) as i16)) & 0xFFF;
        if (n << 16) <= 0 {
            break;
        }
    }
    g.chicken = c;
    if d >= 0x100 {
        return;
    }
    let target = c.target as i32;
    if ks[i].finished == 0 && g.countdown == 0 && (ks[i].adv_targets as i32) < target {
        let k = &mut ks[i];
        k.adv_targets = k.adv_targets.wrapping_add(1);
        k.flash_timer = 0xD;
        k.blink_count = 0;
        k.hit_dir = 1;
        if k.adv_targets as i32 == target {
            // The winner and the other player (by player index) are out;
            // FUN_80060580 voices: the other's clip 2, then the winner's 3.
            k.finished = 1;
            k.pause_bits = 0x80;
            k.flags |= 1;
            let winner = k.character;
            let other = ((k.player_index as i32 + 1) & 1) as usize;
            if let Some(o) = ks.get_mut(other) {
                o.finished = 1;
                o.flags |= 1;
                o.pause_bits = 0x80;
                crate::sound::voice_clip(g, o.character, 2);
            }
            crate::sound::voice_clip(g, winner, 3);
        } else {
            let b = byte(g, w);
            ks[i].event_bits |= if b < 0x55 {
                8
            } else if b < 0xAA {
                4
            } else {
                2
            };
        }
    }
    crate::update::rumble(&mut ks[i], g, 200);
    let p = g.chicken.pos;
    crate::sound::at(g, w, 0x14, 0x1600, 0x80, p, -1);
    crate::sound::at(g, w, 0xB, 0x1600, 0xA0, p, -1);
    let karts: Vec<[i32; 3]> = ks.iter().take((g.karts.max(0) as usize).min(ks.len())).map(|k| [k.x, k.y, k.z]).collect();
    let mut c = g.chicken;
    burst(&c, g, w);
    spawn(&mut c, g, w, &karts, rand);
    g.chicken = c;
    burst(&c, g, w);
}

/// FUN_80069648 in mode 0x14: a projectile hit costs the victim a chicken.
pub fn projectile_hit(v: &mut KartState) {
    if v.adv_targets != 0 {
        v.adv_targets -= 1;
        v.flash_timer = 0xD;
        v.blink_count = 0;
        v.hit_dir = -1;
    }
}

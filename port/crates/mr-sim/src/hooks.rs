//! GRAVE's hooks (DAT chunk 0x02 at 0x800D1C50; 0x800B0E00 groups of 32
//! path points, three hooks per group): FUN_800616A0 (course load),
//! FUN_800617CC (every race frame, after the render pass of the karts) and
//! FUN_80061BC4 (kart update, before the ground contact).
//!
//! A hook runs along its group's closed path (phase 0..0x7FFF, + 0x20 per
//! VBlank; FUN_80081BF4 interpolates the points). An airborne kart within
//! 16 units (>> 10) of a free hook is caught (+0x4E8 = hook, the hook's
//! +0x27 = the kart's slot): each update eases it 1/16 of the way to the
//! hook and sets its velocity from the move. The hook lets go when its
//! phase wraps; a jump or a wall hit drops the kart (+0x4E9 = 1).

use crate::fixed::{div, div_pow2};
use crate::globals::RaceGlobals;
use crate::kart::KartState;
use crate::world::World;

/// One hook (0x800D15A0 + 0x2C * i).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Hook {
    /// +0x00: position on the path (world units).
    pub pos: [i32; 3],
    /// +0x10: the trailing point (the carried kart's driver position, or
    /// the swing; the render's rope / orientation).
    pub trail: [i32; 3],
    /// +0x24: phase (u16, 0..0x7FFF).
    pub phase: u16,
    /// +0x26: the phase wrapped this frame (the hook lets go).
    pub wrapped: u8,
    /// +0x27: the carried kart's slot, -1 = free.
    pub carrier: i8,
}

/// FUN_800616A0: three hooks per group at phases 0, 0x2AAB, 0x5556.
pub fn load(w: &World) -> Vec<Hook> {
    let n = w.course.objects.hooks.len();
    (0..3 * n).map(|i| Hook { phase: ((i % 3) as i32 * 0x2AAB) as u16, carrier: -1, ..Default::default() }).collect()
}

/// FUN_80081BF4(list, first, last, phase, out, 0) over one group's 32
/// points: point (phase & 0x7FFF) * 31 / 0x8000 and the next (wrapping to
/// the first), 1/64 steps.
pub fn path_point(pts: &[[i32; 3]; 32], phase: u16) -> [i32; 3] {
    let n = 31i32;
    let v = ((phase as i32 & 0x7FFF) * n * 0x40) >> 15;
    let i = (v >> 6) as usize;
    let f = v & 0x3F;
    let j = if i + 1 == 32 { 0 } else { i + 1 };
    let (a, b) = (pts[i], pts[j]);
    [0, 1, 2].map(|c| a[c] * 0x20 + ((b[c] - a[c]).wrapping_mul(f) >> 1))
}

/// FUN_800617CC (simulation part): advance every hook, place it on its
/// path, move its trailing point. `drivers` = each kart's driver position
/// (+0x330 / +0x338 / +0x340), slot order. The ground query for the
/// hook's shadow and the orientation (FUN_8003CCBC) are render side.
pub fn frame(g: &mut RaceGlobals, w: &World, drivers: &[[i32; 3]]) {
    let dt = g.dt;
    for (i, h) in g.objects.hooks.iter_mut().enumerate() {
        let p = h.phase as u32 + (dt * 0x20) as u32;
        h.phase = p as u16;
        if (p >> 15) & 1 == 0 {
            h.wrapped = 0;
        } else {
            h.wrapped = 1;
            h.phase = h.phase.wrapping_sub(0x8000);
        }
        h.pos = path_point(&w.course.objects.hooks[i / 3], h.phase);
        if h.wrapped != 0 {
            h.trail = h.pos;
        } else if h.carrier == -1 {
            let mut d = [0, 1, 2].map(|c| h.pos[c].wrapping_sub(h.trail[c]));
            for _ in 0..(dt as u16).max(1) {
                for c in 0..3 {
                    d[c] = div_pow2(d[c].wrapping_mul(7), 3);
                    h.trail[c] = h.pos[c].wrapping_add(d[c]);
                }
            }
        } else {
            h.trail = drivers.get(h.carrier as u8 as usize).copied().unwrap_or(h.pos);
        }
    }
}

/// The end of FUN_800617CC's hook loop (render side): FUN_8003CCBC's
/// angles of the rope from the trailing point (0x8000 below it) to the
/// hook, stored at +0x28 (about x) / +0x2A (about z); FUN_800859EC turns
/// slot 0x30 + i by them.
pub fn angles(t: &crate::tables::Tables, h: &Hook) -> [u16; 2] {
    let x = h.pos[0].wrapping_sub(h.trail[0]) >> 5;
    let y = h.pos[1].wrapping_sub(h.trail[1].wrapping_add(0x8000)) >> 5;
    let z = h.pos[2].wrapping_sub(h.trail[2]) >> 5;
    let ax = crate::tables::ratan2(t, z, y.wrapping_neg()).wrapping_neg() & 0xFFF;
    let r = y.wrapping_neg().wrapping_mul(t.cos(ax)).wrapping_sub(z.wrapping_mul(t.sin(ax)));
    let az = crate::tables::ratan2(t, x, crate::fixed::div_pow2(r, 12)) & 0xFFF;
    [ax as u16, az as u16]
}

/// The slot origin FUN_800859EC gives hook i (FUN_8002DF88, DAT units):
/// the hook >> 5, 0x400 higher.
pub fn slot_origin(h: &Hook) -> [i32; 3] {
    [h.pos[0] >> 5, (h.pos[1] >> 5) - 0x400, h.pos[2] >> 5]
}

/// FUN_80061BC4(kart): catch, carry and release.
pub fn kart(k: &mut KartState, g: &mut RaceGlobals) {
    if k.effect != -1 || g.objects.hooks.is_empty() || k.carried_drop != 0 {
        return;
    }
    if k.carried == -1 {
        if k.ground_contact != 0 {
            return;
        }
        let (x, y, z) = (k.x >> 10, k.y >> 10, k.z >> 10);
        // The game scans all 24 records; past the course's hooks they are
        // never free (+0x27 = 0 in BSS).
        let hit = g.objects.hooks.iter().position(|h| {
            let dx = (h.pos[0] >> 10) - x;
            let dy = (h.pos[1] >> 10) - y;
            let dz = (h.pos[2] >> 10) - z;
            h.carrier == -1 && dx.wrapping_mul(dx).wrapping_add(dy.wrapping_mul(dy)).wrapping_add(dz.wrapping_mul(dz)) < 0x100
        });
        let Some(i) = hit else { return };
        k.carried = i as i8;
        g.objects.hooks[i].carrier = k.player_index;
        k.event_bits |= 8;
    } else {
        let h = &mut g.objects.hooks[k.carried as u8 as usize];
        if h.wrapped != 0 {
            h.carrier = -1;
            k.carried = -1;
            return;
        }
    }
    let h = g.objects.hooks[k.carried as u8 as usize];
    k.x = k.x.wrapping_add(div_pow2(h.pos[0].wrapping_sub(k.x), 4));
    k.y = k.y.wrapping_add(div_pow2(h.pos[1].wrapping_sub(k.y), 4));
    k.z = k.z.wrapping_add(div_pow2(h.pos[2].wrapping_sub(k.z), 4));
    let dt = g.dt;
    k.vx = div(k.x.wrapping_sub(k.prev_x), dt);
    k.vy = div(k.y.wrapping_sub(k.prev_y), dt);
    k.vz = div(k.z.wrapping_sub(k.prev_z), dt);
}

/// A jump or a wall hit drops a carried kart (0x80066314, 0x8003EB0C):
/// +0x4E9 = 1, the hook freed, +0x4E8 = -1.
pub fn drop(k: &mut KartState, g: &mut RaceGlobals) {
    if k.carried == -1 {
        return;
    }
    k.carried_drop = 1;
    if let Some(h) = g.objects.hooks.get_mut(k.carried as u8 as usize) {
        h.carrier = -1;
    }
    k.carried = -1;
}

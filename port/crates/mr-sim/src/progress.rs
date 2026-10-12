//! Course interaction around the ground contact: object lists
//! (FUN_800628FC, FUN_80061FC0, FUN_80061BC4, FUN_800632BC, FUN_80063C24,
//! FUN_80062E38), lap timing (FUN_80082708) and the track direction ahead
//! (FUN_80086B7C with FUN_80090424).

use crate::fixed::{div_pow2, s16, substeps, wrap_angle};
use crate::globals::RaceGlobals;
use crate::kart::KartState;
use crate::tables::ratan2;
use crate::world::{TickInputs, World};

/// 0x80067898..0x80067950: object checks wrapped around the ground contact.
pub fn contact_phase(k: &mut KartState, g: &mut RaceGlobals, w: &World, inp: &TickInputs) {
    // FUN_800628FC: list at 0x800D1C98 (null on DOCK).
    if k.effect == -1 && w.course.has_other_objects {
        unreachable_in_scope!("FUN_800628FC object list");
    }
    if k.item_e7 != -1 {
        unreachable_in_scope!("item +0x4E7 active: ground contact skipped");
    }
    // FUN_80061FC0: proximity triggers (list 0x800D1C9C, count 0x800B0DC6).
    if k.effect == -1 {
        if k.pickup == -1 {
            let (x, y, z) = (k.x >> 10, k.y >> 10, k.z >> 10);
            for tr in &w.course.triggers {
                let dx = (tr.pos[0] >> 5) - x;
                let dy = (tr.pos[1] >> 5) - y;
                let dz = (tr.pos[2] >> 5) - z;
                if dx.wrapping_mul(dx).wrapping_add(dy.wrapping_mul(dy)).wrapping_add(dz.wrapping_mul(dz)) < 0x100 {
                    unreachable_in_scope!("DOCK trigger object touched (FUN_80061FC0 pickup)");
                }
            }
        } else if !w.course.triggers.is_empty() {
            unreachable_in_scope!("pickup state +0x4E6 active");
        }
    }
    if !(k.pickup == -1 || k.wall_flag != 0) {
        unreachable_in_scope!("pickup state +0x4E6: ground contact skipped");
    }
    // FUN_80061BC4: flag 0x800B0E00 (0 on DOCK).
    if k.effect == -1 && w.course.has_other_objects {
        unreachable_in_scope!("FUN_80061BC4");
    }
    if k.carried == -1 {
        // FUN_800632BC: +0x4E4 = -1; list 0x800D1C8C is null on DOCK.
        k.object_e4 = -1;
    } else {
        unreachable_in_scope!("carrying an object");
    }
    crate::ground::kart_ground(k, g, w, inp);
    // FUN_80063C24: +0x4E3 = -1; count byte 0x800B0C58 is 0 on DOCK.
    k.object_e3 = -1;
    if k.flags & 0x40 != 0 && k.item_ee != -1 {
        unreachable_in_scope!("FUN_800627E0");
    }
    // FUN_80062E38: +0x4F8 = -1; lists 0x800D1C68 / 0x800D1CA4 null on DOCK.
    k.object_f8 = -1;
}

/// FUN_8003D208: 1 when (px, pz) is on the non-negative side of the line
/// (ax, az) -> (bx, bz).
fn side(px: i32, pz: i32, ax: i32, az: i32, bx: i32, bz: i32) -> i32 {
    let v = (px - ax).wrapping_mul(bz - az).wrapping_add((pz - az).wrapping_mul(ax - bx));
    (!v as u32 >> 31) as i32
}

/// FUN_80082708: lap time accumulation and the finish-line side bits.
pub fn lap_timing(k: &mut KartState, g: &mut RaceGlobals, w: &World) {
    if g.challenge != 0 || g.countdown != 0 || g.mode == 20 {
        return;
    }
    if g.mode == 2 || g.mode == 7 {
        unreachable_in_scope!("lap timing for modes 2/7");
    }
    if g.race_flags & 8 != 0 {
        unreachable_in_scope!("lap timing with race flag 8");
    }
    if g.race_type == 3 {
        unreachable_in_scope!("lap timing, time trial");
    }
    if (k.lap as i32) >= g.laps as i32 || g.mode == 10 || g.mode == 17 {
        return;
    }
    const LIMIT: i32 = 0x2BF20;
    if k.race_time >= LIMIT {
        return;
    }
    let lap = k.lap as usize;
    k.lap_time[lap] = k.lap_time[lap].wrapping_add(g.dt * 5);
    k.race_time = 0;
    for i in 0..=lap {
        k.race_time = k.race_time.wrapping_add(k.lap_time[i]);
    }
    if LIMIT < k.race_time {
        unreachable_in_scope!("race time limit reached");
    }
    let Some(f) = &w.course.finish else { return };
    let q = |i: usize| f.words[i] >> 4;
    let (px, pz) = (k.x >> 9, k.z >> 9);
    let s = side(px, pz, q(7), q(9), q(4), q(6))
        | side(px, pz, q(1), q(3), q(4), q(6)) << 1
        | side(px, pz, q(7), q(9), q(10), q(12)) << 2;
    if g.race_flags & 2 != 0 {
        unreachable_in_scope!("finish line with race flag 2");
    }
    if s == 7 && k.line_side == 6 {
        if k.line_flag == 0 {
            lap_completed(k, g);
        }
        k.line_flag = 0;
    } else if s == 6 && k.line_side == 7 {
        // 0x80083458: crossed backwards: remember it so the next forward
        // crossing does not count a lap.
        if k.line_flag == 1 {
            unreachable_in_scope!("finish line crossed backwards twice (0x80083468)");
        }
        k.line_flag = 1;
    }
    k.line_side = s as u8;
}

/// 0x80082DA0: a lap was completed.
fn lap_completed(k: &mut KartState, g: &mut RaceGlobals) {
    let lap = k.lap as usize;
    let t = k.lap_time[lap];
    let q = t / 3;
    // Round the lap time to a multiple of 3 (nearest).
    k.lap_time[lap] = 3 * q;
    if t - 3 * q >= 2 {
        // 0x80082E04 (not reached on the lap route): rounds up.
        k.lap_time[lap] += 3;
    }
    if (k.player_index as i32) < g.humans as i32 && k.flags & 1 == 0 && g.race_flags & 2 == 0 && k.lap_time[lap] < g.lap_record {
        unreachable_in_scope!("lap record beaten (0x80082EB4)");
    }
    if k.best_lap != 0 {
        unreachable_in_scope!("second lap end: best-lap compare (0x80082F28)");
    }
    k.best_lap = k.lap_time[lap];
    if g.lap_threshold != 0 && k.best_lap < g.lap_threshold {
        unreachable_in_scope!("lap under the announcement threshold (0x80082F90)");
    }
    k.lap = k.lap.wrapping_add(1);
    let lap = k.lap as i32;
    let laps = g.laps as i32;
    if lap == laps {
        unreachable_in_scope!("race finished (0x80083008)");
    }
    if g.end_mode >= 2 {
        unreachable_in_scope!("lap end with end mode >= 2");
    }
    if lap == laps - 1 {
        unreachable_in_scope!("final lap banner (0x800833C8)");
    }
    if lap < laps {
        k.flash_timer = (lap - 1) as i8;
        k.blink_count = 0;
    }
}

/// FUN_80090424 for the player (path flag 0x800B0A20 = 1, so a branch
/// always takes the first link and rand() is never called): walk `n` path
/// points ahead of (seg, pt). Returns the segment and point reached (the
/// globals 0x800B0B20 / 0x800B0F3C) or None for segment -1.
pub fn path_ahead(w: &World, seg: i16, pt: i16, n: i32) -> Option<(i16, i16)> {
    if seg == -1 {
        return None;
    }
    let ter = &w.ter;
    let mut seg = seg as u16;
    let mut pt = pt as i32;
    let mut out_pt = pt as i16;
    let mut n = n;
    while n << 16 > 0 {
        let cur = pt as i16 as i32;
        pt += 1;
        let s = &ter.path_segments[seg as i16 as usize];
        let h0 = ter.path_points[(s.first as i32 + cur) as usize].heading;
        let mut next = seg;
        if s.count as i32 <= s16(pt) {
            next = 0xFFFF;
            if s.n_next > 0 {
                next = ter.links[(s.link_start as i32) as usize] as u16;
            }
            pt = 1;
            if next == 0xFFFF {
                pt = s.count as u16 as i32 - 1;
                next = seg;
            }
        }
        let s2 = &ter.path_segments[next as i16 as usize];
        let h1 = ter.path_points[(s2.first as i32 + s16(pt)) as usize].heading;
        let _turn = wrap_angle(s16(h1 as i32 - h0 as i32));
        n -= 1;
        seg = next;
        out_pt = pt as i16;
    }
    Some((seg as i16, out_pt))
}

/// FUN_80086B7C: ease +0x48E toward the direction of a path point a few
/// points ahead (further at higher speed).
pub fn track_direction(k: &mut KartState, g: &mut RaceGlobals, w: &World) {
    if g.race_type == 3 {
        return;
    }
    if g.mode != 1 {
        if g.mode == 2 || g.mode == 7 || g.mode == 0x14 {
            unreachable_in_scope!("track direction for modes 2/7/0x14");
        }
        return;
    }
    let mut n: i32 = 3;
    if k.warn_timer == 0 {
        let v = s16(k.speed_est >> 5).clamp(0x18, 0x37);
        n = v >> 3;
    }
    if k.path_mode != 0 {
        unreachable_in_scope!("FUN_80090670 path mode");
    }
    let Some((seg, pt)) = path_ahead(w, k.path_seg, k.path_pt, n) else { return };
    let ter = &w.ter;
    let s = &ter.path_segments[seg as usize];
    let p = &ter.path_points[s16(s.first as u16 as i32 + pt as i32) as usize];
    let px = s16(((p.x as u32) >> 4) as i32);
    let pz = s16(((p.z as u32) >> 4) as i32);
    let a = ratan2(&w.tables, px - (k.x >> 9), pz - (k.z >> 9));
    for _ in 0..substeps(g.dt) {
        let d = s16((a & 0xFFF) - (k.track_dir as u16 as i32));
        let d = wrap_angle(d);
        k.track_dir = (((k.track_dir as i32) + div_pow2(s16(d), 3)) & 0xFFF) as i16;
    }
}

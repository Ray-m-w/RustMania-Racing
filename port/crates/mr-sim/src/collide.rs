//! Kart-kart contact: FUN_800418D8 (start of the race frame, before any
//! kart update) and the pair test FUN_80040018, with the spin-out helper
//! FUN_8007B26C.
//!
//! The game tests every pair (j, i), j < i, in order. For the player (slot
//! 0) that is (0, 1), (0, 2), ... (0, n-1); each other kart is still in the
//! state it ended the previous frame with when it meets kart 0, so the
//! player's side of every contact is reproduced exactly from the other
//! karts' previous-frame states (the harness takes them from the oracle; in
//! a solo session `others` is empty).

use crate::fixed::{div, div_pow2, mul, s16, wrap_angle};
use crate::globals::RaceGlobals;
use crate::kart::KartState;
use crate::tables::{ratan2, square_root0};
use crate::update::rumble;
use crate::world::World;

/// Race-frame globals the contact code reads.
#[derive(Clone, Copy, Debug)]
pub struct ContactRules {
    /// 0x800B0DFE: karts in the race (humans + AI).
    pub karts: i32,
    /// 0x800B07FE: challenge "tagged" kart (-1 in a race).
    pub tagged: i8,
}

/// FUN_800418D8 for kart 0 against `others` (slots 1..).
pub fn frame_contacts(k: &mut KartState, others: &mut [KartState], g: &mut RaceGlobals, w: &World, rules: &ContactRules) {
    // Per-frame resets (all karts) and the nearest-kart table 0x800C3068.
    let mut nearest = vec![0x7FFF_FFFFu32; others.len() + 1];
    k.aim_dist = 0x7FFF_FFFF;
    k.nearest_kart = -1;
    k.close_kart = 0;
    for o in others.iter_mut() {
        o.aim_dist = 0x7FFF_FFFF;
        o.nearest_kart = -1;
        o.close_kart = 0;
    }
    for o in others.iter_mut() {
        pair(k, o, g, w, rules, &mut nearest);
    }
}

/// FUN_800418D8 over every kart: the per-frame resets, then every pair
/// (j, i), j < i, in order, sharing one nearest-kart table (0x800C3068,
/// indexed by +0x4D7).
pub fn frame_contacts_all(karts: &mut [KartState], g: &mut RaceGlobals, w: &World, rules: &ContactRules) {
    let n = karts.len();
    let mut nearest = vec![0x7FFF_FFFFu32; n.max(8)];
    for k in karts.iter_mut() {
        k.aim_dist = 0x7FFF_FFFF;
        k.nearest_kart = -1;
        k.close_kart = 0;
    }
    for j in 0..n {
        let (head, tail) = karts.split_at_mut(j + 1);
        for o in tail.iter_mut() {
            pair(&mut head[j], o, g, w, rules, &mut nearest);
        }
    }
}

fn eng(v: i16, e: i16) -> i32 {
    div_pow2(mul(v as i32, e as i32), 12)
}

/// FUN_8007B26C(kart, turns): start a spin-out.
pub fn spin_out(k: &mut KartState, g: &mut RaceGlobals, w: &World, turns: i32) {
    let full = (turns << 16) >> 4;
    let mut a = -full;
    if k.spin_rate < 0 {
        a = full;
    } else if k.spin_rate < 1 {
        let b = crate::update::rng_next(g, w);
        if b > 0x7F {
            a = full;
        }
    }
    let human = g.mode == 1 && (k.player_index as i32) < g.humans as i32 && k.flags & 1 == 0;
    if human {
        a += wrap_angle(s16(k.track_dir as u16 as i32 - k.heading as u16 as i32));
    }
    let mut a = s16(a);
    k.spin_rate = 0x55;
    if a < 0 {
        k.spin_rate = -0x55;
        a = -a;
    }
    k.spin_timer = (a / 0x55) as i16;
}

/// FUN_80040018(a, b).
fn pair(a: &mut KartState, b: &mut KartState, g: &mut RaceGlobals, w: &World, r: &ContactRules, nearest: &mut [u32]) {
    let t = &w.tables;
    if a.special != 0 || b.special != 0 || a.item_e7 != -1 || b.item_e7 != -1 || a.item_ee != -1 || b.item_ee != -1 {
        return;
    }
    // 0x800400A0: proximity bookkeeping (aim target, nearest kart).
    let dx = (a.look_x.wrapping_sub(b.look_x)) >> 10;
    let dy = (a.look_y.wrapping_sub(b.look_y)) >> 10;
    let dz = (a.look_z.wrapping_sub(b.look_z)) >> 10;
    let d2 = mul(dx, dx).wrapping_add(mul(dy, dy)).wrapping_add(mul(dz, dz));
    if g.race_clock != 0 {
        if d2 < a.aim_dist {
            a.aim_dist = d2;
            a.aim_x = b.look_x >> 5;
            a.aim_y = b.look_y >> 5;
            a.aim_z = b.look_z >> 5;
        }
        if d2 < b.aim_dist {
            b.aim_dist = d2;
            b.aim_x = a.look_x >> 5;
            b.aim_y = a.look_y >> 5;
            b.aim_z = a.look_z >> 5;
        }
    }
    let racing = (g.race_type != 2 && g.race_flags & 9 == 0)
        || (a.finished == 0 && b.finished == 0 && ((a.player_index as i32) < r.karts || (b.player_index as i32) < r.karts));
    if racing {
        let (ia, ib) = (a.player_index as usize, b.player_index as usize);
        if (d2 as u32) < nearest[ia] {
            nearest[ia] = d2 as u32;
            a.nearest_kart = b.player_index;
            if d2 < 0x4000 {
                a.close_kart = 1;
            }
        }
        if (d2 as u32) < nearest[ib] {
            nearest[ib] = d2 as u32;
            b.nearest_kart = a.player_index;
            if d2 < 0x4000 {
                b.close_kart = 1;
            }
        }
    }
    if (a.pickup != -1 && a.wall_flag == 0) || (b.pickup != -1 && b.wall_flag == 0) {
        return;
    }
    // 0x80040200: vertical overlap of the two probe spans (scaled by +0x442).
    let ha = eng(a.probe_bottom - a.probe_top, a.engine);
    let hb = eng(b.probe_bottom - b.probe_top, b.engine);
    if a.y + eng(a.probe_bottom, a.engine) < b.y + eng(b.probe_top, b.engine) {
        return;
    }
    if b.y + eng(b.probe_bottom, b.engine) < a.y + eng(a.probe_top, a.engine) {
        return;
    }
    let stacked = a.prev_y < b.prev_y - hb || b.prev_y < a.prev_y - ha;
    // 0x800402E0: horizontal overlap of the two radii (+0x176).
    let ra = eng(a.spark_offset, a.engine);
    let rb = eng(b.spark_offset, b.engine);
    let ddx = (a.x - b.x) >> 8;
    let ddz = (a.z - b.z) >> 8;
    let rr = (ra + rb) >> 3;
    if mul(rr, rr) <= mul(ddx, ddx).wrapping_add(mul(ddz, ddz)) {
        if b.own_bit & a.hit_mask != 0 {
            a.hit_mask -= b.own_bit;
        }
        if a.own_bit & b.hit_mask != 0 {
            b.hit_mask -= a.own_bit;
        }
        return;
    }
    let half_e = |e: i16| -> i32 {
        let e = e as i32;
        (e - (e >> 31)) >> 1
    };
    if stacked {
        // 0x80040378: one kart lands on the other.
        rumble(a, g, 200);
        rumble(b, g, 200);
        // The kick is added in every sub-step of the next update (two at
        // 30 Hz): the 60 Hz mode's one sub-step takes both.
        let kick = if g.hz60.on { 2000 } else { 1000 };
        a.bounce_kick -= kick;
        b.bounce_kick -= kick;
        let (va, vb) = (a.vy, b.vy);
        a.vy = vb;
        b.vy = va;
        if a.y < b.y {
            a.vy = if a.ceiling_hit == 0 { a.vy - (half_e(b.engine) + 0x800) / 2 } else { s16(va) };
            b.vy = if b.ground_contact == 0 { b.vy + div_pow2((half_e(a.engine) + 0x800) * 0x600, 12) } else { s16(vb) };
        } else {
            b.vy = if b.ceiling_hit == 0 { va - (half_e(a.engine) + 0x800) / 2 } else { s16(vb) };
            a.vy = if a.ground_contact == 0 { a.vy + div_pow2((half_e(b.engine) + 0x800) * 0x600, 12) } else { s16(va) };
        }
        a.hit_mask |= b.own_bit;
        b.hit_mask |= a.own_bit;
        let a_above = a.y < b.y;
        {
            let (upper, lower): (&mut KartState, &mut KartState) = if a_above { (a, b) } else { (b, a) };
            let r0 = w.rng_byte(g.rng) as i8;
            upper.event_bits = if r0 < 0 { upper.event_bits | 4 } else { upper.event_bits | 2 };
            g.rng = g.rng.wrapping_add(1);
            let r1 = w.rng_byte(g.rng) as i8;
            lower.event_bits = if r1 < 0 { lower.event_bits | 0x20 } else { lower.event_bits | 0x10 };
            g.rng = g.rng.wrapping_add(1);
            lower.sink_target = -0x1800;
            spin_out(lower, g, w, 1);
            if upper.finished == 0 && lower.finished == 0 && ((upper.player_index as i32) < r.karts || (lower.player_index as i32) < r.karts) {
                if g.mode == 2 || g.mode == 7 || g.race_flags & 8 != 0 {
                    // 0x80040A1C: landing on a kart deals 100 damage.
                    let old = lower.impact;
                    upper.hits_dealt = upper.hits_dealt.wrapping_add(100);
                    lower.impact = lower.impact.wrapping_add(100);
                    lower.stun = lower.stun.wrapping_add(100);
                    upper.hit_dir = 100;
                    upper.flash_timer = 10;
                    upper.blink_count = 0;
                    lower.hit_dir = -100;
                    lower.flash_timer = 10;
                    lower.blink_count = 0;
                    if old < 1000 && 999 < lower.impact {
                        upper.flash_timer = 0x10;
                        upper.blink_count = 0;
                        upper.knockouts = upper.knockouts.wrapping_add(1);
                    }
                }
                if g.mode == 1 {
                    upper.contacts_from_above = upper.contacts_from_above.wrapping_add(1);
                    lower.contacts_below = lower.contacts_below.wrapping_add(1);
                    if matches!(g.race_flags, 0 | 2 | 0x10) {
                        upper.flash_timer = 0xF;
                        upper.blink_count = 0;
                        upper.hit_dir = 1;
                        lower.flash_timer = 0xF;
                        lower.blink_count = 0;
                        lower.hit_dir = -1;
                    }
                }
            }
        }
        a.obj_e5 = 1;
        b.obj_e5 = 1;
        crate::sound::kart_land(a, b, g, w);
        if g.challenge != 0 || r.tagged != -1 {
            crate::unreachable_in_scope!("challenge tag on kart contact");
        }
    }
    if b.own_bit & a.hit_mask != 0 || a.own_bit & b.hit_mask != 0 {
        // 0x80040774: still overlapping: push apart.
        let px = (b.x - a.x) >> 10;
        let pz = (b.z - a.z) >> 10;
        let d = (square_root0(t, mul(px, px).wrapping_add(mul(pz, pz))) + 1) * 4;
        let fx = div(px << 11, d);
        let fz = div(pz << 11, d);
        a.push_x = a.push_x.wrapping_add((-fx / 2) as i16);
        a.push_z = a.push_z.wrapping_add((-fz / 2) as i16);
        b.push_x = b.push_x.wrapping_add((fx / 2) as i16);
        b.push_z = b.push_z.wrapping_add((fz / 2) as i16);
        return;
    }
    // 0x80040840: first touch side by side: exchange the velocity
    // components along the line between the karts.
    let ang = ratan2(t, (b.prev_z - a.prev_z) >> 5, (a.prev_x - b.prev_x) >> 5) & 0xFFF;
    let u = (-((ang - 0x400) & 0xFFF)) & 0xFFF;
    let push_a = div_pow2(mul(a.vz, t.cos_raw(u)).wrapping_sub(mul(a.vx, t.sin_raw(u))), 14);
    let push_b = div_pow2(mul(b.vz, t.cos_raw(u)).wrapping_sub(mul(b.vx, t.sin_raw(u))), 14);
    let na = (-ang) & 0xFFF;
    let (c13, s17) = (t.cos(na), t.sin(na));
    let mut a_al = div_pow2(mul(a.vx, c13).wrapping_add(mul(a.vz, s17)), 12);
    let a_ac = div_pow2(mul(-a.vx, s17).wrapping_add(mul(a.vz, c13)), 12);
    let mut b_al = div_pow2(mul(b.vx, c13).wrapping_add(mul(b.vz, s17)), 12);
    let b_ac = div_pow2(mul(-b.vx, s17).wrapping_add(mul(b.vz, c13)), 12);
    if a_al < b_al || b_al < a_al {
        std::mem::swap(&mut a_al, &mut b_al);
    }
    let (c19, s18) = (t.cos(ang), t.sin(ang));
    a.vx = div_pow2(mul(a_al, c19).wrapping_add(mul(a_ac, s18)), 12);
    a.vz = div_pow2(mul(-a_al, s18).wrapping_add(mul(a_ac, c19)), 12);
    b.vx = div_pow2(mul(b_al, c19).wrapping_add(mul(b_ac, s18)), 12);
    b.vz = div_pow2(mul(-b_al, s18).wrapping_add(mul(b_ac, c19)), 12);
    a.hit_mask |= b.own_bit;
    b.hit_mask |= a.own_bit;
    let sa = wrap_angle(s16(((ang - 0x400) & 0xFFF) - a.heading as u16 as i32));
    let sb = wrap_angle(s16(((ang + 0x400) & 0xFFF) - b.heading as u16 as i32));
    let (mut score_a, mut score_b) = (0u8, 0u8);
    if push_b.abs() < push_a.abs() {
        score_a = 1;
    } else if push_a.abs() < push_b.abs() {
        score_b = 1;
    }
    if sa.abs() < sb.abs() {
        score_a += 1;
    } else if sb.abs() < sa.abs() {
        score_b += 1;
    }
    if b.speed_est < a.speed_est {
        score_a += 1;
    } else if a.speed_est < b.speed_est {
        score_b += 1;
    }
    // The "loser" (piVar23) spins harder; ties go by a random byte.
    let loser_is_b = if score_b < score_a {
        true
    } else if score_b == score_a {
        (crate::update::rng_next(g, w) as i8) < 0
    } else {
        false
    };
    let spin_a = side_spin(sa);
    let spin_b = side_spin(sb);
    let free = |k: &KartState| -> bool {
        k.spin_rate == 0
            && k.spin_timer == 0
            && (g.mode != 1 || g.humans as i32 <= k.player_index as i32 || k.flags & 1 != 0)
    };
    let a_locked = if free(a) {
        a.spin_rate = div_pow2(spin_a, 4) as i16;
        a.spin_timer = 8;
        false
    } else {
        true
    };
    let b_locked = if free(b) {
        b.spin_rate = div_pow2(spin_b, 4) as i16;
        b.spin_timer = 8;
        false
    } else {
        true
    };
    let imp = ((push_a - push_b).abs() >> 4).min(0xFF);
    rumble(a, g, imp);
    rumble(b, g, imp);
    crate::sound::kart_bump(a, b, g, w, imp);
    {
        let (loser, winner, locked): (&mut KartState, &mut KartState, bool) =
            if loser_is_b { (b, a, b_locked) } else { (a, b, a_locked) };
        if !locked {
            let kk = ((push_a - push_b).abs() >> 9).max(1);
            let st = s16(loser.spin_timer as i32 * kk);
            loser.spin_timer = st as i16;
            if 0x78 < st {
                loser.spin_timer = 0x78;
            }
            let k2 = (kk >> 1).max(1);
            loser.spin_rate = (loser.spin_rate as i32 * k2) as i16;
        }
        if (g.mode == 2 || g.mode == 7 || g.race_flags & 8 != 0)
            && winner.finished == 0
            && loser.finished == 0
            && ((winner.player_index as i32) < r.karts || (loser.player_index as i32) < r.karts)
        {
            // 0x800410A8: damage (race flag 8 here; battle modes 2 / 7 the
            // same code): |push difference| >> 5, at least 1.
            let d = ((push_a - push_b).abs() >> 5).max(1);
            let old = loser.impact;
            winner.hits_dealt = winner.hits_dealt.wrapping_add(d as i16);
            if (winner.max_hit as i32) < d {
                winner.max_hit = d as i16;
            }
            loser.stun = loser.stun.wrapping_add(d as i16);
            loser.impact = loser.impact.wrapping_add(d as i16);
            winner.flash_timer = 10;
            winner.blink_count = 0;
            winner.hit_dir = d as i16;
            loser.flash_timer = 10;
            loser.hit_dir = -d as i16;
            loser.blink_count = 0;
            if 0x95 < d {
                winner.event_bits |= 1;
                let b = w.rng_byte(g.rng) as i8;
                g.rng = g.rng.wrapping_add(1);
                loser.event_bits |= if b < 0 { 0x20 } else { 0x10 };
            }
            if old < 1000 && 999 < loser.impact {
                winner.flash_timer = 0x10;
                winner.blink_count = 0;
                winner.knockouts = winner.knockouts.wrapping_add(1);
            }
        }
        if w.rng_byte(g.rng) < 0x10 {
            let mut bits = winner.event_bits | 2;
            if (w.rng_byte(g.rng.wrapping_add(1)) as i8) < 0 {
                bits = winner.event_bits | 4;
            }
            winner.event_bits = bits;
            g.rng = g.rng.wrapping_add(2);
        } else {
            g.rng = g.rng.wrapping_add(1);
        }
        if w.rng_byte(g.rng) < 0x10 {
            let mut bits = loser.event_bits | 0x10;
            if (w.rng_byte(g.rng.wrapping_add(1)) as i8) < 0 {
                bits = loser.event_bits | 0x20;
            }
            loser.event_bits = bits;
            g.rng = g.rng.wrapping_add(2);
        } else {
            g.rng = g.rng.wrapping_add(1);
        }
    }
    if (a.player_index as i32) < g.humans as i32 || (b.player_index as i32) < g.humans as i32 {
        // 0x800415A8: contact sparks (FUN_800326B4 x3, three random bytes
        // each) midway between the two karts' contact points: each kart's
        // +0x176 radius along its heading + the side angle, at its ride
        // height.
        let ua = (a.heading as i32 + sa as i16 as i32) & 0xFFF;
        let ub = (b.heading as i32 + sb as i16 as i32) & 0xFFF;
        let (ra, rb) = (a.spark_offset as i32, b.spark_offset as i32);
        let px = (a.x.wrapping_add(ra * t.sin_raw(ua) >> 9).wrapping_add(b.x).wrapping_add(rb * t.sin_raw(ub) >> 9)) / 2;
        let py = (a.y.wrapping_add((a.ride_height as i32) << 5).wrapping_add(b.y).wrapping_add((b.ride_height as i32) << 5)) / 2;
        let pz = (a.z.wrapping_add(ra * t.cos_raw(ua) >> 9).wrapping_add(b.z).wrapping_add(rb * t.cos_raw(ub) >> 9)) / 2;
        for _ in 0..3 {
            let v = [0, 1, 2].map(|_| (crate::update::rng_next(g, w) as i8 as i32) << 1);
            crate::items::spark(g, [px, py, pz], v, 0x18, 0x10, [0xFF, 0x7F, 0]);
        }
    }
    if g.challenge != 0 || r.tagged != -1 {
        crate::unreachable_in_scope!("challenge tag on kart contact");
    }
}

/// 0x80040FE0: spin impulse from the angle between a kart's heading and
/// the contact direction (0 when hit square on or dead from the side).
fn side_spin(s: i32) -> i32 {
    if s >= 1 {
        if s < 0x400 {
            (0x200 - s).abs() - 0x200
        } else if s > 0x400 {
            0x200 - (0x600 - s).abs()
        } else {
            0
        }
    } else if s < 0 {
        if -0x400 < s {
            0x200 - (-0x200 - s).abs()
        } else if s < -0x400 {
            (-0x600 - s).abs() - 0x200
        } else {
            0
        }
    } else {
        0
    }
}

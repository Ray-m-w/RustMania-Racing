//! FUN_8003E91C: the kart against the track. Wall test along the move
//! (FUN_8003DA18 -> FUN_8003D240 height test + bounce, FUN_8003D760 re-test),
//! the ground query (FUN_8003DDB0, ported in mr_assets::ter::Ter::ground),
//! floor / ceiling / water resolution, ground-contact counter, slope push,
//! body pitch and roll from the surface normal, slope steering
//! (FUN_8003CD7C), suspension input and body lean (FUN_80064384).
//!
//! Not ported (render side, no effect on the kart record): the shadow
//! polygon (FUN_8003E59C + FUN_8003E3A0) and wall-spark particles
//! (FUN_800326B4; their random-byte draws are kept).

use mr_assets::ter::Ter;

use crate::fixed::{div, div_pow2, mul, s16, s8, substeps, wrap_angle};
use crate::globals::RaceGlobals;
use crate::kart::KartState;
use crate::tables::{ratan2, Tables};
use crate::update::{rng_next, rumble};
use crate::world::{TickInputs, World};

/// Result of the wall test (the globals FUN_8003D240 leaves behind).
#[derive(Clone, Copy, Debug, Default)]
pub struct WallHit {
    /// 0x800B0BFC / 0x800B0C0C: position after the bounce.
    pub x: i32,
    pub z: i32,
    /// 0x800B0CF0 / 0x800B0CFC: velocity after the bounce.
    pub vx: i32,
    pub vz: i32,
    /// 0x800B0C40: impact speed along the wall normal (>= 0x100).
    pub impact: i32,
    /// 0x800B0AC8: heading relative to the wall.
    pub rel_angle: i32,
    pub wall: usize,
}

/// What FUN_8003DA18 reads from globals the caller set (0x800B0F68,
/// 0x800B0F62, 0x800B0DA4, 0x800B0CF0/CFC, 0x800B0C30).
#[derive(Clone, Copy, Debug)]
pub struct WallQuery {
    pub heading: i32,
    pub probe_top: i32,
    pub probe_bottom: i32,
    pub vx: i32,
    pub vz: i32,
    pub bounce: i32,
    pub dt: i32,
}

/// FUN_8003D240: does the move (p0 -> p1, world units) hit the wall edge
/// a -> b at this height, and if so the bounce. `a` is the wall's second
/// point (offset +2 of the wall record), `b` its first.
fn wall_hit(t: &Tables, ter: &Ter, q: &WallQuery, p0: [i32; 3], p1: [i32; 3], wi: usize) -> Option<WallHit> {
    let wl = ter.walls[wi];
    let a = ter.wall_points[wl.p1 as usize];
    let b = ter.wall_points[wl.p0 as usize];
    let u = ratan2(t, b.x - a.x, b.z - a.z) & 0xFFF;
    let nu = (-u) & 0xFFF;
    let (cw, sw) = (t.cos(nu), t.sin(nu));
    let (dx, dz) = (p1[0].wrapping_sub(p0[0]), p1[2].wrapping_sub(p0[2]));
    let along = div_pow2(mul(dx, cw).wrapping_add(mul(dz, sw)), 12);
    let across = div_pow2(mul(dz, cw).wrapping_sub(mul(dx, sw)), 12);
    if a.y_top != a.y_bottom || b.y_top != b.y_bottom {
        // 0x8003D2F8: wall height at the crossing point.
        let rx = (p1[0] >> 5) - a.x;
        let len = div_pow2(mul(a.x - b.x, sw).wrapping_add(mul(b.z - a.z, cw)), 12);
        let rz = (p1[2] >> 5) - a.z;
        let ra = div_pow2(mul(rx, cw).wrapping_add(mul(rz, sw)), 12);
        let mut s = div_pow2(mul(-rx, sw).wrapping_add(mul(rz, cw)), 12);
        if along != 0 {
            s += div(mul(ra, across), along);
        }
        let step = t.wall_step[(q.dt + 5) as usize] as i32;
        let top = a.y_top * 0x20 + s * div((b.y_top - a.y_top) * 0x20, len) + step + 1;
        if p1[1] + q.probe_bottom < top {
            return None;
        }
        let bot = a.y_bottom * 0x20 + s * div((b.y_bottom - a.y_bottom) * 0x20, len) - step - 1;
        if bot < p1[1] + q.probe_top {
            return None;
        }
    }
    // 0x8003D4E4: reflect the velocity off the wall.
    let mut va = div_pow2(mul(q.vx, cw).wrapping_add(mul(q.vz, sw)), 12);
    let vc = div_pow2(mul(q.vz, cw).wrapping_sub(mul(q.vx, sw)), 12);
    if va < 0x100 {
        va = 0x100;
    }
    let refl = div_pow2(mul(-va, q.bounce), 4);
    let impact = s16(va).abs();
    let rel = wrap_angle(s16(q.heading - u));
    let (cu, su) = (t.cos(u), t.sin(u));
    let nvx = div_pow2(mul(refl, cu).wrapping_add(mul(vc, su)), 12);
    let nvz = div_pow2(mul(vc, cu).wrapping_sub(mul(refl, su)), 12);
    Some(WallHit {
        x: p0[0] + nvx * q.dt,
        z: p0[2] + nvz * q.dt,
        vx: nvx,
        vz: nvz,
        impact,
        rel_angle: rel,
        wall: wi,
    })
}

/// FUN_8003DA18: first wall the move p0 -> p1 hits (FUN_8003D240), then
/// FUN_8003D760: if the bounced move would cross a wall again, stay at p0.
pub fn wall_test(t: &Tables, ter: &Ter, q: &WallQuery, p0: [i32; 3], p1: [i32; 3]) -> Option<WallHit> {
    for wi in ter.wall_candidates(p0[0], p0[2], p1[0], p1[2]) {
        if let Some(mut h) = wall_hit(t, ter, q, p0, p1, wi) {
            if ter.wall_cross(p0[0], p0[2], h.x, h.z).is_some() {
                h.x = p0[0];
                h.z = p0[2];
            }
            return Some(h);
        }
    }
    None
}

/// FUN_8003E91C for the solo player.
pub fn kart_ground(k: &mut KartState, g: &mut RaceGlobals, w: &World, inp: &TickInputs) {
    let t = &w.tables;
    let ter = &w.ter;
    let dt = g.dt;
    let old_contact = k.ground_contact;
    let old_ceiling = k.ceiling_hit;
    k.wall_hit = 0;

    // 0x8003E94C: walls along the move prev -> now (not in a special move).
    let q = WallQuery {
        heading: k.heading as i32,
        probe_top: k.probe_top as i32,
        probe_bottom: k.probe_bottom as i32,
        vx: k.vx,
        vz: k.vz,
        bounce: k.wall_bounce as i32,
        dt,
    };
    let hit = if k.special == 0 { wall_test(t, ter, &q, [k.prev_x, k.prev_y, k.prev_z], [k.x, k.y, k.z]) } else { None };
    if let Some(h) = hit {
        let mut strength = s16(h.impact) >> 4;
        if strength > 0xFF {
            strength = 0xFF;
        }
        if s16(h.impact) > 0x800 {
            let b = w.rng_byte(g.rng);
            let mut next = g.rng.wrapping_add(1);
            if b < 0x20 {
                let mut bits = k.event_bits | 0x10;
                if (w.rng_byte(g.rng.wrapping_add(1)) as i8) < 0 {
                    bits = k.event_bits | 0x20;
                }
                next = g.rng.wrapping_add(2);
                k.event_bits = bits;
            }
            g.rng = next;
        }
        rumble(k, g, strength);
        crate::sound::wall_hit(k, g, w, strength);
        if (k.player_index as i32) < g.humans as i32 {
            // Wall sparks (FUN_800326B4 x3) at the kart's side facing the
            // wall.
            let u = ((k.heading as i32).wrapping_sub(h.rel_angle).wrapping_add(0x400)) & 0xFFF;
            let r = k.spark_offset as i32;
            let pos = [k.x.wrapping_add(r.wrapping_mul(t.sin_raw(u)) >> 9), k.ref_y, k.z.wrapping_add(r.wrapping_mul(t.cos_raw(u)) >> 9)];
            for _ in 0..3 {
                let v = [0, 1, 2].map(|_| (rng_next(g, w) as i8 as i32) << 1);
                crate::items::spark(g, pos, v, 0x18, 0x10, [0xFF, 0x7F, 0]);
            }
        }
        // 0x8003EB0C: a wall hit drops a kart off GRAVE's hook.
        crate::hooks::drop(k, g);
        k.wall_hit = 1;
        k.x = h.x;
        k.z = h.z;
        k.vx = h.vx;
        k.vz = h.vz;
    }

    // 0x8003EC18: ground query at the probe centre.
    let old_y = k.y;
    let qy = (old_y.wrapping_mul(2) + k.probe_top as i32 + k.probe_bottom as i32) / 2;
    let gq = ter.ground(k.x, qy, k.z);
    let Some(nearest) = gq.nearest else {
        // 0x8003EC50: off the terrain: undo the move.
        k.on_ground = 0;
        k.vx = 0;
        k.vy = 0;
        k.vz = 0;
        k.x = k.prev_x;
        k.y = k.prev_y;
        k.z = k.prev_z;
        return;
    };
    let top = k.probe_top as i32;
    let bot = k.probe_bottom as i32;
    k.ceiling_hit = 0;
    let mut face: Option<usize> = None;
    let mut ceiling = false;
    if let Some(alt) = gq.alt {
        if k.y + bot > alt.y {
            ceiling = true;
            if k.y + top < alt.y {
                k.y = alt.y - top;
                k.ceiling_hit = 1;
            }
            face = gq.below.map(|s| s.face);
            k.ground_y = gq.below.map(|s| s.y).unwrap_or(gq.nearest_y);
        }
    }
    if !ceiling || k.y + top < gq.nearest_y {
        if gq.nearest_y <= k.y + bot {
            k.y = gq.nearest_y - bot;
        }
        face = Some(nearest.face);
        k.ground_y = gq.nearest_y;
    }
    if k.ground_y <= k.y + bot {
        k.y = k.ground_y - bot;
    }
    let face = face.expect("floor face");
    k.floor_face_ptr = w.face_ptr(Some(face));
    k.water_face_ptr = w.face_ptr(gq.water.map(|s| s.face));
    if k.special != 0 {
        // 0x8003ED60: a special move keeps its height (the floor only
        // informs the normals below).
        k.y = old_y;
    }
    let fc = &ter.faces[face];

    // 0x8003ED78: ease the smoothed normal toward the floor's.
    if g.countdown == 0 && 299 < g.race_clock {
        let step = (k.speed_est.wrapping_mul(dt)) / 2;
        k.normal_x = ease(k.normal_x, fc.normal[0], step);
        k.normal_y = ease(k.normal_y, fc.normal[1], step);
        k.normal_z = ease(k.normal_z, fc.normal[2], step);
    } else {
        k.normal_x = fc.normal[0];
        k.normal_y = fc.normal[1];
        k.normal_z = fc.normal[2];
    }
    let mut n130 = [k.normal_x as i32, k.normal_y as i32, k.normal_z as i32];
    k.surface = fc.kind as i8;
    if k.status & 0x1000 != 0 && k.effect != 0x17 && fc.kind != 0x0C {
        k.surface = 2;
    }
    let water_n: [i32; 3] = if gq.water.is_none() {
        [0, -0x1000, 0]
    } else {
        [inp.water_normal[0] as i32, inp.water_normal[1] as i32, inp.water_normal[2] as i32]
    };
    let water_y = gq.water_y;
    k.water_y = water_y;
    if k.ground_y - (k.y + bot) < 0x200 {
        let n = k.ground_contact as i32 + dt;
        k.ground_contact = n as u8;
        if s8(n) > 0x40 {
            k.ground_contact = 0x40;
        }
    } else {
        k.ground_contact = 0;
    }

    // 0x8003F014: conveyor faces (kind 0x0C quads).
    k.conveyor_prev = k.conveyor;
    let fw = (fc.flags as u16) | ((fc.kind as u16) << 8);
    if k.ground_contact == 0 || fw & 0xFF80 != 0x0C80 {
        k.conveyor = 0;
    } else {
        k.conveyor = 1;
        let v = |i: usize| ter.vertices[fc.v[i] as usize];
        let ex0 = (v(0).x + v(1).x) / 2;
        let ez0 = (v(0).z + v(1).z) / 2;
        let ex1 = (v(2).x + v(3).x) / 2;
        let ez1 = (v(2).z + v(3).z) / 2;
        k.conveyor_dir = (ratan2(t, ex0 - ex1, ez0 - ez1) & 0xFFF) as i16;
    }

    // 0x8003F0E8: floating state against the water surface.
    let saved_float = k.float_state;
    let rh = (k.ride_height as i32).abs();
    k.float_state = if water_y < k.y + rh * -0x20 {
        2
    } else if water_y < k.y {
        1
    } else {
        0
    };
    if k.status & 0x0020_0000 != 0 {
        k.float_state = 0;
    }
    if k.float_state == 0 {
        k.splash = 0;
    } else {
        if let Some(alt) = gq.alt {
            if alt.y < k.y && water_y < alt.y {
                k.float_state = 0;
            }
        }
        k.splash = if k.float_state == 0 || saved_float != 0 { 0 } else { 1 };
    }
    k.below_ground = (k.water_y < k.ground_y) as u8;

    // 0x8003F214: which normal the body follows; splash sound.
    let mut sound_check = false;
    if (k.object_e3 == -1 || k.ground_contact != 0) && k.special == 0 && k.carried == -1 && k.status & 0x1000_0000 == 0 {
        if k.chassis_flags & 2 != 0 && k.float_state == 1 && k.ground_contact == 0 {
            n130 = match gq.water {
                None => [0, -0x1000, 0],
                Some(s) => {
                    let f = &ter.faces[s.face];
                    [f.normal[0] as i32, f.normal[1] as i32, f.normal[2] as i32]
                }
            };
            sound_check = true;
        } else if k.float_state != 0 {
            if k.ground_contact == 0 {
                n130 = water_n;
            }
            sound_check = true;
        }
    } else {
        n130 = [inp.alt_normal[0] as i32, inp.alt_normal[1] as i32, inp.alt_normal[2] as i32];
        sound_check = true;
    }
    if sound_check && k.float_state != 0 {
        crate::sound::splash(k, g, w);
    }
    let alt_blocks = matches!(gq.alt, Some(a) if k.y > a.y && a.y > k.water_y);
    k.support_y = if k.water_y < k.ground_y && !alt_blocks { k.water_y } else { k.ground_y };

    // 0x8003F420: surface property bytes.
    let s = k.surface as i32;
    k.surf_b5 = t.surface(s, 5) as u8;
    k.surf_b6 = t.surface(s, 6) as u8;
    k.surf_b7 = t.surface(s, 7) as u8;

    // (0x8003F4A4..0x8003F7AC: shadow polygon, render side, not ported.)

    let found = gq.found;
    if k.special != 0 {
        // 0x8003F7A4: a special move: no sink, no bounce, the body follows
        // the normal chosen above; the sink offset eases to 0.
        k.sink_target = 0;
        k.on_ground = found as u8;
        k.tilt_nx = n130[0] as i16;
        k.tilt_ny = n130[1] as i16;
        k.tilt_nz = n130[2] as i16;
        k.bounce = 0;
        k.bounce_vel = 0;
        for _ in 0..substeps(dt) {
            let d = k.sink_target as i32 - k.sink as i32;
            k.sink = (k.sink as i32 + div_pow2(d, 3)) as i16;
        }
        return tilt_and_lean(k, g, w, n130);
    }
    if k.carried != -1 {
        // 0x8003F7AC: on a hook: straight to the tilt (LAB_8003FC1C).
        return tilt_and_lean(k, g, w, n130);
    }
    // 0x8003F7B8: sink offset.
    if k.status & 1 == 0 {
        if k.effect == 2 {
            k.sink_target = -0xC00;
        } else if k.ground_contact == 0 || old_contact != 0 {
            let st = k.sink_target as i32;
            if st >= 1 {
                let n = st - dt * 0x200;
                k.sink_target = n as i16;
                if s16(n) < 0 {
                    k.sink_target = 0;
                }
            } else if st < 0 {
                let n = st + dt * 0x200;
                k.sink_target = n as i16;
                if s16(n) > 0 {
                    k.sink_target = 0;
                }
            }
        } else {
            k.sink_target = 0;
        }
        for _ in 0..substeps(dt) {
            let d = k.sink_target as i32 - k.sink as i32;
            k.sink = (k.sink as i32 + div_pow2(d, 3)) as i16;
        }
    }
    // 0x8003F890: ceiling: vertical speed from the position change.
    if k.ceiling_hit != 0 {
        if old_ceiling == 0 && k.prev_y < k.y {
            k.vy = 0;
        }
        if k.ceiling_hit != 0 {
            k.vy = div(k.y - k.prev_y, dt);
        }
    }
    // 0x8003F8F8: landing rumble.
    if k.ground_contact != 0 && old_contact == 0 {
        let v = ((k.vy >> 5).abs()).min(0xFF);
        rumble(k, g, v);
        crate::sound::landing(k, g, w, v);
    }
    // 0x8003F9B0: vertical speed on the ground; slope push.
    let mut slope_push = true;
    if k.status & 0x400 == 0 && k.float_state != 2 {
        if k.ground_contact != 0 {
            if old_contact == 0 && k.y < k.prev_y {
                k.vy = 0;
            } else {
                k.vy = div(k.y - k.prev_y, dt);
            }
        } else {
            if old_contact != 0 {
                k.vy = div(k.y - k.prev_y, dt);
            }
            slope_push = false;
        }
    }
    if slope_push && g.countdown == 0 && k.status & 0x0010_0000 == 0 {
        let kk = if k.float_state == 2 && k.ground_contact == 0 { 0x30 } else { 0x60 };
        let mut px = div_pow2(s16(n130[0]) * kk, 12);
        let mut pz = div_pow2(n130[2] * kk, 12);
        if k.conveyor != 0 {
            px = div_pow2(px, 2);
            pz = div_pow2(pz, 2);
        }
        k.push_x = k.push_x.wrapping_add(px as i16);
        k.push_z = k.push_z.wrapping_add(pz as i16);
    }
    if k.on_ground == 0 && found && k.vy < 0 {
        k.vy = 0;
    }
    k.on_ground = found as u8;
    tilt_and_lean(k, g, w, n130)
}

/// 0x8003FC1C to the end of FUN_8003E91C: pitch and roll toward the
/// normal, slope steering, suspension input, body lean.
fn tilt_and_lean(k: &mut KartState, g: &mut RaceGlobals, w: &World, n130: [i32; 3]) {
    let t = &w.tables;
    let dt = g.dt;
    let (nx, ny, nz);
    if k.ground_contact == 0 && k.float_state == 0 && k.object_e3 == -1 && k.carried == -1 && k.special == 0 && k.status & 0x400 == 0 {
        nx = k.tilt_nx as i32;
        ny = k.tilt_ny as i32;
        nz = k.tilt_nz as i32;
    } else {
        nx = s16(n130[0]);
        ny = s16(n130[1]);
        nz = s16(n130[2]);
        k.tilt_nx = nx as i16;
        k.tilt_ny = ny as i16;
        k.tilt_nz = nz as i16;
    }
    let ab0 = (-ratan2(t, nz, -ny)) & 0xFFF;
    let c = mul(-ny, t.cos(ab0)).wrapping_sub(mul(nz, t.sin(ab0)));
    let ab4 = ratan2(t, nx, div_pow2(c, 12)) & 0xFFF;
    let kk = if k.ground_contact != 0 && (k.ground_contact as i8) < 0x10 { 4 } else { 8 };
    for _ in 0..substeps(dt) {
        let d = wrap_angle(s16(ab0 - k.pitch as u16 as i32));
        k.pitch = (((k.pitch as i32) + s16(d) / kk) & 0xFFF) as i16;
        let d = wrap_angle(s16(ab4 - k.roll as u16 as i32));
        k.roll = (((k.roll as i32) + s16(d) / kk) & 0xFFF) as i16;
    }
    slope_steer(k, t);
    if k.special == 0 && k.carried == -1 {
        k.bounce_vel = k.bounce_vel.wrapping_add((k.prev_vy as i32 - k.vy).wrapping_mul(4));
        if k.chassis_flags & 2 != 0 {
            // 60 Hz mode: the same byte on both ticks of a pair, drawn once.
            let b = g.replay_draws(|g| rng_next(g, w)) as i8 as i32;
            k.bounce_kick = k.bounce_kick.wrapping_add(b * 2);
        }
    }
    k.obj_e5 = 0;
    body_lean(k, dt);
}

/// Move `cur` toward `target` by `step`, clamping at the target.
fn ease(cur: i16, target: i16, step: i32) -> i16 {
    let c = cur as i32;
    let tg = target as i32;
    if tg < c {
        let n = c - step;
        if s16(n) < tg {
            target
        } else {
            n as i16
        }
    } else if c < tg {
        let n = c + step;
        if tg < s16(n) {
            target
        } else {
            n as i16
        }
    } else {
        cur
    }
}

/// FUN_8003CD7C: slope roll angle (+0x470) and the yaw it induces (+0x474).
pub fn slope_steer(k: &mut KartState, t: &Tables) {
    let roll = k.roll as i32;
    let pitch = k.pitch as i32;
    let a = div_pow2(t.sin(roll) * -0x1000, 12);
    let u = (-(k.heading as i32)) & 0xFFF;
    let b = div_pow2(t.cos(roll) * 0x1000, 12);
    let c = div_pow2(b * t.cos(pitch), 12);
    let d = div_pow2(b * t.sin(pitch), 12);
    let e = div_pow2(a * t.cos(u) + d * t.sin(u), 12);
    let r = ratan2(t, e, c);
    k.slope_turn = ((-r).wrapping_mul(k.fwd_speed) >> 0x12) as i16;
    k.slope_roll = r as i16;
    if k.special != 0 {
        k.sway[0] = 0;
        k.sway[6] = 0;
    }
}

/// FUN_80064384: body lean follows the sway springs.
pub fn body_lean(k: &mut KartState, dt: i32) {
    if k.status & 1 != 0 {
        return;
    }
    for _ in 0..substeps(dt) {
        let a = div_pow2(k.sway[0], 6) - k.lean_a as i32;
        let b0 = k.sway[6];
        k.lean_a = (k.lean_a as i32 + div_pow2(a, 2)) as i16;
        let b = div_pow2(b0, 6) - k.lean_b as i32;
        k.lean_b = (k.lean_b as i32 + div_pow2(b, 2)) as i16;
    }
}

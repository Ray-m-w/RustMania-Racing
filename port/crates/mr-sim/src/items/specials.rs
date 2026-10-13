//! Specials: the character special (R2 with a full meter, FUN_8006B828), the
//! special move (R1 with a full meter, kart +0x4D6, FUN_800913F4) and the
//! effect driver FUN_8006C388 (effect +0x4F1 on the karts a special hit,
//! timer +0x468, global special 0x800B07F4 and its clock 0x800B07F6).

use super::Cameras;
use crate::globals::RaceGlobals;
use crate::kart::KartState;
use crate::world::World;


use crate::fixed::{div, div_pow2, mul, s16, substeps, wrap_angle};
use crate::tables::{ratan2, square_root0};

fn path_point(w: &World, seg: i16, pt: i16) -> [i32; 3] {
    let s = &w.ter.path_segments[seg as u16 as usize];
    let p = &w.ter.path_points[s.first.wrapping_add(pt) as u16 as usize];
    [p.x, p.y, p.z]
}

/// R1 with a full meter on the path (0x80065B5C): the special move. The
/// kart leaves the physics and flies along the TER path (forward, or
/// backward with race flag 2) until the meter is spent, then returns to the
/// path (state 3) and lands.
pub fn special_move_start(k: &mut KartState, g: &mut RaceGlobals, w: &World, slot: usize, rand: &mut dyn FnMut() -> i32) {
    k.special = if g.race_flags & 2 != 0 { 2 } else { 1 };
    k.event_bits |= 2;
    k.sp_x = k.x;
    k.sp_y = k.y;
    k.sp_z = k.z;
    let wk = if k.special == 1 {
        crate::ai::walk_forward(w, k.path_seg, k.path_pt, 2, false, rand)
    } else {
        crate::ai::walk_backward(w, k.path_seg, k.path_pt, 2, rand)
    };
    k.sp_seg = wk.seg;
    k.sp_pt = wk.pt;
    let p = path_point(w, k.sp_seg, k.sp_pt);
    k.ai_tx = p[0] << 5;
    k.ai_ty = p[1] << 5;
    k.ai_tz = p[2] << 5;
    crate::ai::reroll(k, g, w, g.karts);
    g.items.events.push(super::ItemEvent::SpecialMove { kart: slot as u8 });
}

/// The special-move branch of the kart update (0x800653D0..0x80065500):
/// the kart's scale (+0x442) follows the meter (shrinks to nothing, grows
/// back over the last 0x20), spin stopped, the meter drains by dt (state 3
/// when empty), then FUN_800913F4.
pub fn special_move_tick(k: &mut KartState, g: &mut RaceGlobals, w: &World, rand: &mut dyn FnMut() -> i32) {
    let m = k.start_timer as i32;
    k.engine = if m < 0x21 {
        (m * -0x80 + 0x1000) as i16
    } else if m < 0x184 {
        0
    } else {
        ((m - 0x184) * 0x80) as i16
    };
    k.spin_rate = 0;
    k.spin_timer = 0;
    crate::update::rumble(k, g, 200);
    if k.special < 3 {
        let n = (k.start_timer as u16 as i32).wrapping_sub(g.dt);
        k.start_timer = n as i16;
        if (n << 16) < 1 {
            k.start_timer = 0;
            k.special = 3;
        }
    }
    follow_path(k, g, w, rand);
}

/// FUN_800913F4: the special move along the path. A chase point (+0x404..)
/// runs toward the target path point (+0x3E8.., world units) at 0xA0 per
/// VBlank, stepping one point on when within 0xA0; the kart eases toward
/// it by 1/16; state 3 converges on the last point (1/4) and ends within
/// 0x14. Velocity and heading follow the movement.
pub fn follow_path(k: &mut KartState, g: &mut RaceGlobals, w: &World, rand: &mut dyn FnMut() -> i32) {
    let t = &w.tables;
    if k.sp_seg == -1 {
        return;
    }
    let dt = g.dt;
    for _ in 0..substeps(dt) {
        let dx = k.ai_tx.wrapping_sub(k.sp_x) >> 5;
        let dy = k.ai_ty.wrapping_sub(k.sp_y) >> 5;
        let dz = k.ai_tz.wrapping_sub(k.sp_z) >> 5;
        let d = square_root0(t, mul(dx, dx).wrapping_add(mul(dy, dy)).wrapping_add(mul(dz, dz)));
        if d < 0xA0 {
            let walked = match k.special {
                1 => Some(crate::ai::walk_forward(w, k.sp_seg, k.sp_pt, 1, false, rand)),
                2 => Some(crate::ai::walk_backward(w, k.sp_seg, k.sp_pt, 1, rand)),
                3 if d < 0x14 => {
                    k.special = 0;
                    return;
                }
                _ => None,
            };
            if (k.special as i8) < 3 {
                let wk = walked.expect("special state 1 / 2 walked");
                let p = path_point(w, wk.seg, wk.pt);
                k.ai_tx = p[0] << 5;
                k.ai_ty = p[1] << 5;
                k.sp_seg = wk.seg;
                k.sp_pt = wk.pt;
                k.ai_tz = p[2] << 5;
            }
        }
        if k.special == 3 {
            k.sp_x = k.sp_x.wrapping_add(div_pow2(k.ai_tx.wrapping_sub(k.sp_x), 2));
            k.sp_y = k.sp_y.wrapping_add(div_pow2(k.ai_ty.wrapping_sub(k.sp_y), 2));
            k.sp_z = k.sp_z.wrapping_add(div_pow2(k.ai_tz.wrapping_sub(k.sp_z), 2));
        } else {
            let ex = k.ai_tx.wrapping_sub(k.sp_x) >> 4;
            let ey = k.ai_ty.wrapping_sub(k.sp_y) >> 4;
            let ez = k.ai_tz.wrapping_sub(k.sp_z) >> 4;
            let n = square_root0(t, mul(ex, ex).wrapping_add(mul(ey, ey)).wrapping_add(mul(ez, ez))) + 1;
            k.sp_x = k.sp_x.wrapping_add(mul(div(ex << 12, n), 0xA0) >> 7);
            k.sp_y = k.sp_y.wrapping_add(mul(div(ey << 12, n), 0xA0) >> 7);
            k.sp_z = k.sp_z.wrapping_add(mul(div(ez << 12, n), 0xA0) >> 7);
        }
        k.x = k.x.wrapping_add(div_pow2(k.sp_x.wrapping_sub(k.x), 4));
        k.y = k.y.wrapping_add(div_pow2(k.sp_y.wrapping_sub(k.y), 4));
        k.z = k.z.wrapping_add(div_pow2(k.sp_z.wrapping_sub(k.z), 4));
    }
    let mx = k.x.wrapping_sub(k.prev_x);
    let mz = k.z.wrapping_sub(k.prev_z);
    k.vx = div(mx, dt);
    k.vy = div(k.y.wrapping_sub(k.prev_y), dt);
    k.vz = div(mz, dt);
    let a = ratan2(t, mx >> 5, mz >> 5);
    for _ in 0..substeps(dt) {
        let d = wrap_angle((a & 0xFFF) - k.heading as i32);
        k.heading = ((k.heading as i32 + div_pow2(d, 2)) & 0xFFF) as i16;
    }
}

/// Character table byte +0x11 (exe 0x800AC231 + 0x14 * character): the
/// character's special type (0..24; -1 none). It doubles as the effect
/// number (+0x4F1) the special puts on its victims.
pub const SPECIAL_TYPE: u32 = 0x800A_C231;

pub fn special_kind(w: &World, character: u8) -> i8 {
    w.exe.u8(SPECIAL_TYPE + 0x14 * character as u32) as i8
}

/// Exe tables FUN_80092878 / FUN_8006B828 read: the 5 x 5 character
/// grid of the select screen (0x800AC208), each character's default
/// vehicle (character table +0x0F, 0x800AC22F + 0x14 * c) and the "hit
/// with a special" collection bit per grid cell (0x800AC6C0).
pub const CHAR_GRID: u32 = 0x800A_C208;
pub const CHAR_VEHICLE: u32 = 0x800A_C22F;
pub const HIT_BIT: u32 = 0x800A_C6C0;
/// Vehicle table flags byte (+3 of 4-byte entries at 0x800AC5E4).
pub const VEHICLE_FLAGS: u32 = 0x800A_C5E7;
/// Shape targets a special can pick, 3 halfwords per entry, indexed by a
/// random byte / 0x34 [exe 0x800AC0F8].
pub const SHAPE_PICKS: u32 = 0x800A_C0F8;

/// FUN_80092878(vehicle): the grid cell whose character drives `vehicle`
/// by default (-1 none).
fn grid_cell_of_vehicle(w: &World, vehicle: u8) -> i32 {
    for i in 0..5u32 {
        for j in 0..5u32 {
            let c = w.exe.u8(CHAR_GRID + i * 5 + j) as i8 as i32;
            if w.exe.u8((CHAR_VEHICLE as i32 + c * 0x14) as u32) == vehicle {
                return (j + i * 5) as i32;
            }
        }
    }
    -1
}

/// Two random bytes pick body-shape targets from SHAPE_PICKS for the two
/// shape channel sets (0x8006BD68; FUN_8006C388 effects 1 and 18): an
/// entry with a -1 leaves its set alone. The caller sets the morph time.
pub fn pick_shape(k: &mut KartState, g: &mut RaceGlobals, w: &World) {
    let b0 = w.rng_byte(g.rng) as u32;
    let b1 = w.rng_byte(g.rng.wrapping_add(1)) as u32;
    g.rng = g.rng.wrapping_add(2);
    let e = |b: u32, o: u32| w.exe.i16(SHAPE_PICKS + (b / 0x34) * 6 + o);
    let (a0, a1, a2) = (e(b0, 0), e(b0, 2), e(b0, 4));
    let (c0, c1, c2) = (e(b1, 0), e(b1, 2), e(b1, 4));
    if a0 != -1 && a1 != -1 && a2 != -1 {
        k.shape[12] = a0;
        k.shape[14] = a1;
        k.shape[16] = a2;
        k.shape[6] = k.shape[0];
        k.shape[8] = k.shape[2];
        k.shape[10] = k.shape[4];
    }
    if c0 != -1 && c1 != -1 && c2 != -1 {
        k.shape[13] = c0;
        k.shape[15] = c1;
        k.shape[17] = c2;
        k.shape[7] = k.shape[1];
        k.shape[9] = k.shape[3];
        k.shape[11] = k.shape[5];
    }
}

/// Morph targets for both channel sets (x, y, z each) over `n` VBlanks,
/// from the current shape.
fn morph(k: &mut KartState, a: [i16; 3], b: [i16; 3], n: u8) {
    k.shape_t = 0;
    k.shape_n = n;
    k.shape[12] = a[0];
    k.shape[14] = a[1];
    k.shape[16] = a[2];
    k.shape[13] = b[0];
    k.shape[15] = b[1];
    k.shape[17] = b[2];
    for j in 0..6 {
        k.shape[6 + j] = k.shape[j];
    }
}

/// FUN_8006B828(kart): the character special (R2 with a full meter). Its
/// type (character table +0x11) becomes the race-wide special 0x800B07F4
/// (clock 0x800B07F6 from 0) and the effect +0x4F1 of every other kart it
/// can reach, with the type's status bits (+0x354) and parameters;
/// FUN_8006C388 runs the effects for 300 VBlanks.
pub fn character_special(k: &mut KartState, others: &mut crate::update::Others, g: &mut RaceGlobals, w: &World) {
    let kind = special_kind(w, k.character);
    g.items.special = kind;
    k.event_bits |= 1;
    g.items.special_clock = 0;
    g.items.special_owner = k.player_index as u8;
    let voice = match kind {
        0x10 => 0x55,
        0x12 => 0x54,
        0x16 => 0x56,
        2 => 0x12,
        _ => -1,
    };
    if voice != -1 {
        crate::sound::play(g, voice, 0x1500, 0xA0, 0xA0);
    }
    let slot = others.slot();
    g.items.events.push(super::ItemEvent::Special { kart: slot as u8, kind: kind as u8 });
    let mut count: i32 = 0;
    let mut unlocked: i32 = 0;
    let n = others.len();
    for j in 0..n {
        if j == slot {
            k.homing_timer = 0;
            continue;
        }
        let v = others.get_mut(j).expect("kart in range");
        v.homing_timer = 0;
        let karts = g.karts as i32;
        if (karts <= v.player_index as i32 && karts <= k.player_index as i32)
            || v.special != 0
            || v.stun > 999
            || (v.pickup != -1 && v.wall_flag == 0)
            || v.item_e7 != -1
            || v.carried != -1
            || v.engine != 0x1000
        {
            continue;
        }
        v.wall_flag = 0;
        if k.finished == 0 && v.finished == 0 {
            if (k.player_index as i32) < g.humans as i32 && k.flags & 1 == 0 && g.race_over == 0 {
                // A collection bit per (special user, hit character's
                // default vehicle) in the save data 0x800E02C0.
                let cell = grid_cell_of_vehicle(w, v.vehicle);
                if cell != -1 {
                    // 0x800AC6C0[cell]: filled at run time by the vehicle
                    // model loader FUN_80092928 with the vehicle itself
                    // (racing vehicles, flag 0x800AC5E7 + 4v bit 0; every
                    // racer's model is loaded in a DOCK race).
                    let vv = v.vehicle as u32;
                    let bit = if w.exe.u8(VEHICLE_FLAGS + 4 * vv) & 1 != 0 { vv as i32 } else { -1 };
                    if bit != -1 {
                        let word = (k.character as usize) * 2 + div_pow2(bit, 5) as usize;
                        let mask = 1u32 << (bit & 31);
                        if g.items.hit_bits[word] & mask == 0 {
                            g.items.hit_bits[word] |= mask;
                            unlocked += 1;
                        }
                    }
                }
            }
            if g.mode == 1 {
                count += 1;
                k.contacts_from_above = k.contacts_from_above.wrapping_add(1);
                v.contacts_below = v.contacts_below.wrapping_add(1);
                if matches!(g.race_flags, 0 | 2 | 0x10) {
                    k.flash_timer = 0xF;
                    k.blink_count = 0;
                    k.hit_dir = 1;
                    v.flash_timer = 0xF;
                    v.blink_count = 0;
                    v.hit_dir = -1;
                }
            }
        }
        v.effect = kind;
        apply_type(v, kind, g, w);
    }
    if unlocked == 0 {
        if count != 0 {
            k.flash_timer = 0xF;
            k.blink_count = 0;
            k.hit_dir = count as i16;
        }
    } else {
        crate::sound::play(g, 0x24, 5000, 0x80, 0x80);
        k.flash_timer = 0xE;
        k.blink_count = 0;
        k.hit_dir = unlocked as i16;
    }
}

/// The per-type set-up of FUN_8006B828's switch (0x8006BBA8..0x8006C2C4):
/// status bits +0x354 and the effect's parameters.
fn apply_type(v: &mut KartState, kind: i8, g: &mut RaceGlobals, w: &World) {
    let t = &w.tables;
    let back = v.yaw_rate < 0;
    match kind {
        0 => v.status = 0x1100_0400,
        1 => {
            v.status = 0x244C;
            v.target_x = if back { -0x20 } else { 0x20 };
            v.target_yaw = 0;
            pick_shape(v, g, w);
            v.shape_t = 0;
            v.shape_n = 0x1E;
            v.target_z = 0;
            v.eff_30c = 0xC00;
        }
        2 => v.status = 0x0420_2000,
        3 => {
            v.status = 0x800;
            v.target_x = 0;
            v.target_yaw = 0;
            v.target_z = 0;
        }
        4 | 0x13 | 0x14 | 0x17 => v.status = 0x0020_200C,
        5 | 9 | 0xD | 0x18 => v.status = 0x2000,
        6 => {
            v.status = 0x40;
            morph(v, [0x1000, 0x1000, 0x1000], [0x800, 0x1800, 0x800], 0x3C);
            v.target_x = if back { -0x80 } else { 0x80 };
            v.target_yaw = 0;
        }
        7 => {
            v.status = 0x0070_200C;
            v.eff_30c = 0;
        }
        8 => v.status = 0x0800_2000,
        0xA => {
            v.status = 0x40;
            v.target_x = if back { -0x80 } else { 0x80 };
            v.target_yaw = 0;
        }
        0xB => {
            v.status = 0x0020_100D;
            morph(v, [0x400, 0x800, 0x2000], [0x400, 0x800, 0x2000], 0x3C);
        }
        0xC => {
            v.status = 0x2020;
            v.target_x = v.x;
            v.target_yaw = v.ground_y;
            v.target_z = v.z;
            v.eff_30c = v.floor_face_ptr as i32;
        }
        0xE => {
            v.status = 0x1001E;
            morph(v, [0x400; 3], [0x400; 3], 0x3C);
        }
        0xF => {
            v.status = 0x0006_200C;
            let r0 = w.rng_byte(g.rng) as i8 as i32;
            let r1 = w.rng_byte(g.rng.wrapping_add(1)) as i8 as i32;
            g.rng = g.rng.wrapping_add(2);
            let mut a = r0 * 4 + 0x400;
            if r1 < 0 {
                a = -a;
            }
            let u = (v.heading as u16 as i32 + a) & 0xFFF;
            v.target_yaw = v.y;
            let r = v.spark_offset as i32;
            v.target_x = v.look_x.wrapping_add(t.sin_raw(u).wrapping_mul(r).wrapping_mul(3) >> 10);
            v.target_z = v.look_z.wrapping_add(t.cos_raw(u).wrapping_mul(r).wrapping_mul(3) >> 10);
            let gq = w.ter.ground(v.target_x, v.ground_y, v.target_z);
            if gq.nearest.is_some() {
                v.target_yaw = gq.nearest_y;
            }
            v.eff_30c = 0;
            let b0 = w.rng_byte(g.rng) as i8 as i32;
            let b1 = w.rng_byte(g.rng.wrapping_add(1)) as i8 as i32;
            g.rng = g.rng.wrapping_add(2);
            v.eff_310 = (b0 << 1) & 0xFFF;
            v.eff_314 = div_pow2(b1, 2);
            v.eff_318 = w.face_ptr(gq.nearest.map(|s| s.face)) as i32;
        }
        0x10 => v.status = 0x1000_0600,
        0x11 => {
            morph(v, [0x400; 3], [0x400; 3], 0x3C);
            v.status = 0x18C;
            v.target_x = (w.rng_byte(g.rng) >> 4) as i32;
            g.rng = g.rng.wrapping_add(1);
        }
        0x12 => {
            v.status = 0x1801C;
            pick_shape(v, g, w);
            v.shape_t = 0;
            v.shape_n = 0x1E;
        }
        0x15 => v.status = 0x0201_001C,
        0x16 => {
            morph(v, [0x800; 3], [0x2000; 3], 0x3C);
            v.status = 0x1001C;
            v.target_x = (w.rng_byte(g.rng) >> 4) as i32;
            g.rng = g.rng.wrapping_add(1);
        }
        _ => {}
    }
}

fn rng_skip(g: &mut RaceGlobals, n: u32) {
    g.rng = g.rng.wrapping_add(n);
}

/// FUN_8006C388: the effects of the character special in progress
/// (0x800B07F4 != -1): the special's clock 0x800B07F6 and each victim's
/// effect clock +0x468 run to 300 VBlanks (twice as fast while a human
/// victim with status 0x200 burns turbo meter), then the effect ends (body
/// shape back, status cleared, a spin from the yaw rate: toward the track
/// for a human); while it runs each effect type animates its victim
/// (0x8006C8D4.. by type - 1; particles are track B's, their random draws
/// are kept); every victim's pad rumbles at random.
pub fn effects_frame(karts: &mut [KartState], g: &mut RaceGlobals, w: &World, cams: &Cameras, rand: &mut dyn FnMut() -> i32) {
    if g.items.special == -1 {
        return;
    }
    let dt = g.dt;
    let n = karts.len();
    let old_clock = g.items.special_clock as i32;
    let mut clock = s16(old_clock + dt);
    if clock > 299 {
        clock = 300;
    }
    g.items.special_clock = clock as i16;
    let mut beep = clock / 60 < 5 && old_clock / 60 < clock / 60;
    let mut s2: i32 = 0;
    let mut old_t = [0i32; 8];
    for j in 0..n {
        let k = &mut karts[j];
        old_t[j] = k.homing_timer as i32;
        let tt = s16(k.homing_timer as i32 + dt);
        k.homing_timer = tt as i16;
        if k.effect != -1 && k.flags & 1 == 0 && k.buttons & 0x200 != 0 && k.start_timer != 0 {
            // A human victim holding R2 shakes it off twice as fast,
            // burning turbo meter.
            k.homing_timer = s16(tt + dt) as i16;
            crate::update::rumble(k, g, 200);
            let m = s16(k.start_timer as u16 as i32 - dt);
            k.start_timer = m as i16;
            if m < 1 {
                k.start_timer = 0;
            }
        }
        let t = k.homing_timer as i32;
        if t / 60 < 5 && old_t[j] / 60 < t / 60 {
            beep = true;
        }
        if k.effect != -1 {
            if clock < 300 && t < 300 {
                s2 = 1;
            } else {
                end_effect(k, g, w);
                s2 = if k.impact > 999 { 0x400 } else { 0x1000 };
            }
        }
    }
    if s2 == 0 && clock == 300 {
        g.items.special = -1;
    }
    if g.items.special != -1 {
        if g.items.special == 9 {
            let nn = s16((clock - 0x1E) / 0x4B);
            if nn > 0 && (old_clock - 0x1E) / 0x4B < nn {
                crate::sound::play(g, 0x57, 0x1400, 0xA0, 0xA0);
            }
        }
        let mut j = 0;
        while j < n {
            let k = &karts[j];
            let near = cams.near(k.x >> 5, k.y >> 5, k.z >> 5, 10000);
            let mut stop = false;
            effect_step(&mut karts[j], j, old_t[j], near, g, w, rand, &mut stop);
            let k = &mut karts[j];
            if k.effect != -1 {
                let r = crate::update::rng_next(g, w) as i8 as i32;
                crate::update::rumble(k, g, s16(div_pow2(r, 1) + 0x80));
            }
            if stop {
                // Effect 5's particle loop reuses the kart loop's counter
                // (register s6 ends at 7): the remaining karts wait a frame.
                break;
            }
            j += 1;
        }
    }
    if beep {
        crate::sound::play(g, 0x23, 0x157C, 0x80, 0x80);
    }
}

/// 0x8006C1xx: an effect runs out (or the special's clock reached 300).
fn end_effect(k: &mut KartState, g: &mut RaceGlobals, w: &World) {
    let v = (k.speed_est >> 5).min(0x40);
    k.yaw_rate = div_pow2(k.yaw_rate as i32 * v, 6) as i16;
    if g.mode == 1 && (k.player_index as i32) < g.humans as i32 && k.flags & 1 == 0 {
        k.spin_rate = k.yaw_rate.wrapping_neg();
        crate::collide::spin_out(k, g, w, 0);
    } else {
        k.spin_timer = 8;
        k.spin_rate = k.yaw_rate;
    }
    let s = if k.impact > 999 { 0x400 } else { 0x1000 };
    k.shape[12] = s;
    k.shape[14] = s;
    k.shape[16] = s;
    k.shape[6] = k.shape[0];
    k.shape[8] = k.shape[2];
    k.shape[10] = k.shape[4];
    k.shape[13] = 0x1000;
    k.shape[15] = 0x1000;
    k.shape[17] = 0x1000;
    k.shape_t = 0;
    k.shape_n = 0x3C;
    k.effect = -1;
    k.status = 0;
    k.shape[7] = k.shape[1];
    k.shape[9] = k.shape[3];
    k.shape[11] = k.shape[5];
}

/// One victim's effect animation (FUN_8006C388 @0x8006C8D4.., table
/// 0x80022FF8 by effect - 1). `old_t`: its effect clock before this frame.
#[allow(clippy::too_many_arguments)]
fn effect_step(k: &mut KartState, j: usize, old_t: i32, near: bool, g: &mut RaceGlobals, w: &World, rand: &mut dyn FnMut() -> i32, stop: &mut bool) {
    let t = &w.tables;
    let dt = g.dt;
    let timer = k.homing_timer as i32;
    let every = |d: i32| div_pow2(old_t, d as u32) < div_pow2(timer, d as u32);
    match k.effect {
        1 => {
            if every(4) {
                pick_shape(k, g, w);
                k.shape_t = 0;
                k.shape_n = 0x1E;
            }
            if near {
                // Smoke puff: a size byte, rand() for its height, 8 bytes.
                rng_skip(g, 1);
                let _ = rand();
                rng_skip(g, 8);
            }
            for _ in 0..substeps(dt) {
                let d = div_pow2(k.eff_30c.wrapping_sub(k.engine as i32), 6);
                let v = k.target_z.wrapping_add(d);
                let e = (k.engine as u16 as i32).wrapping_add(v);
                k.engine = e as i16;
                k.target_z = v;
                if (e << 16) < 0 {
                    k.engine = 0;
                    k.target_z = 0;
                }
            }
        }
        3 => {
            k.target_x = k.target_x.wrapping_add(dt);
            if k.target_yaw < k.target_x {
                k.target_x = 0;
                let b = crate::update::rng_next(g, w);
                k.target_z = (k.target_z + 1) & 1;
                k.target_yaw = (b >> 4) as i32 + 8;
                if k.target_z == 0 {
                    k.status = 0;
                    morph(k, [0x1000; 3], [0x1000; 3], 0xF);
                } else {
                    morph(k, [0x4000; 3], [0x4000; 3], 7);
                    k.status |= 0x600D;
                    crate::sound::at(g, w, 0x53, 0x1600, 0xA0, [k.x, k.y, k.z], -1);
                }
            }
        }
        4 | 0x13 | 0x14 | 0x17 => {
            let mut tt = timer;
            if tt > 0xF {
                let t2 = s16(300 - tt);
                if t2 < 0x11 {
                    k.status = 0x200C;
                    tt = t2;
                } else {
                    tt = 0x10;
                }
            }
            k.engine = ((0x10 - tt) * 0x100) as i16;
            k.target_x = tt;
            // FUN_800861F0 glow (render).
            if k.effect == 4 && near && crate::update::rng_next(g, w) < 0x20 {
                rng_skip(g, 10);
            }
        }
        5 => {
            if crate::update::rng_next(g, w) < 0x20 {
                let b = if (crate::update::rng_next(g, w) as i8) < 0 { 0x55 } else { -0x55 };
                if (k.spin_rate as i32).abs() < 0x55 {
                    k.spin_rate = b;
                    k.spin_timer = ((crate::update::rng_next(g, w) >> 3) as i16) + 0x10;
                    if near {
                        // Seven particles (3 bytes, then 2 more for the
                        // first two and 3 each).
                        rng_skip(g, 3 + 2 * 2 + 7 * 3);
                        *stop = true;
                    }
                    crate::sound::at(g, w, 0x18, 0x1538, 0x70, [k.x, k.y, k.z], -1);
                }
            }
        }
        7 => {
            let mut a = timer;
            if a >= 32 {
                let mut t2 = 300 - timer;
                if s16(t2) >= 17 {
                    t2 = 16;
                }
                a = ((t2 as u32 & 0xFFFF) << 1) as i32;
            }
            k.engine = ((32 - s16(a)) << 7) as i16;
            let mut b = timer;
            if timer >= 16 {
                let u = 300 - timer;
                let t2 = if s16(u) < 9 {
                    k.status = 0x0040_200C;
                    u
                } else {
                    8
                };
                b = ((t2 as u32 & 0xFFFF) << 1) as i32;
            }
            let b = s16(b);
            k.eff_30c = b << 5;
            if timer >= 151 {
                k.eff_30c = 0xFFF - (b << 5);
            }
            // FUN_800861F0 (render).
            if near && crate::update::rng_next(g, w) < 0x40 {
                let r1 = crate::update::rng_next(g, w) as i8;
                if r1 < 0 {
                    rng_skip(g, 8);
                } else {
                    rng_skip(g, 2 + 7);
                }
            }
        }
        8 => {
            if k.model_slot as u8 != 0xFF && k.char_slot as u8 != 0xFF {
                let (pa, pb) = g.items.kart_parts[j];
                let nn = (pa as i32 + pb as i32) * 4;
                let s = if timer < nn {
                    timer
                } else {
                    let r = 300 - timer;
                    if nn < s16(r) {
                        nn
                    } else {
                        r
                    }
                };
                let d = div_pow2(s16(nn - s), 2);
                let pa = pa as i32;
                k.target_x = if pa < d { pa } else { d };
                k.target_yaw = if pa < d { d - pa } else { 0 };
                if (d << 16) != 0 && near {
                    rng_skip(g, 9);
                }
            } else {
                k.target_x = -1;
                k.target_yaw = -1;
            }
        }
        9 => {
            if every(4) {
                let to = if div_pow2(timer, 4) & 1 != 0 { ([0x400, 0x400, 0x2000], [0x2000, 0x400, 0x400]) } else { ([0x2000, 0x400, 0x400], [0x400, 0x400, 0x2000]) };
                morph(k, to.0, to.1, 0xF);
            }
        }
        10 => {
            if near {
                if crate::update::rng_next(g, w) < 0x40 {
                    rng_skip(g, 6);
                }
                if crate::update::rng_next(g, w) < 0x10 {
                    rng_skip(g, 9);
                }
            }
        }
        12 => {
            let mut tt = timer;
            if tt > 0xF {
                let t2 = s16(300 - tt);
                tt = if t2 > 0x10 { 0x10 } else { t2 };
            }
            k.eff_310 = (tt << 16) >> 13;
            // FUN_800861F0 (render).
        }
        13 | 0x18 => {
            let mut tt = timer;
            if tt > 0x1F {
                let t2 = s16(300 - tt);
                tt = if t2 > 0x20 { 0x20 } else { t2 };
            }
            k.engine = ((0x40 - (tt * 3) / 2) * 0x40) as i16;
            if k.effect == 0x18 && crate::update::rng_next(g, w) < 0x20 {
                let b: i16 = if (crate::update::rng_next(g, w) as i8) < 0 { 0x55 } else { -0x55 };
                if (k.spin_rate as i32).abs() < 0x55 {
                    k.spin_rate = b;
                    k.spin_timer = ((crate::update::rng_next(g, w) >> 4) as i16) + 8;
                }
            }
        }
        15 => {
            if old_t / 0x3D < timer / 0x3D {
                let r0 = w.rng_byte(g.rng) as i8 as i32;
                let r1 = w.rng_byte(g.rng.wrapping_add(1)) as i8 as i32;
                rng_skip(g, 2);
                let mut a = r0 * 4 + 0x400;
                if r1 < 0 {
                    a = -a;
                }
                let u = (k.heading as u16 as i32 + a) & 0xFFF;
                k.target_yaw = k.look_y;
                let r = k.spark_offset as i32;
                k.target_x = k.look_x.wrapping_add(t.sin_raw(u).wrapping_mul(r).wrapping_mul(3) >> 10);
                k.target_z = k.look_z.wrapping_add(t.cos_raw(u).wrapping_mul(r).wrapping_mul(3) >> 10);
                let gq = w.ter.ground(k.target_x, k.ground_y, k.target_z);
                if gq.nearest.is_some() {
                    k.target_yaw = gq.nearest_y;
                }
                k.eff_30c = 0;
                let b0 = w.rng_byte(g.rng) as i8 as i32;
                let b1 = w.rng_byte(g.rng.wrapping_add(1)) as i8 as i32;
                let b2 = w.rng_byte(g.rng.wrapping_add(2));
                rng_skip(g, 3);
                k.eff_310 = (b0 << 1) & 0xFFF;
                k.eff_314 = div_pow2(b1, 2);
                k.eff_318 = w.face_ptr(gq.nearest.map(|s| s.face)) as i32;
                if b2 < 0x40 {
                    k.event_bits |= 8;
                }
            }
            let mut m = s16(timer % 0x3D);
            if m >= 8 {
                m = 0x3D - m;
                if m >= 9 {
                    m = 8;
                }
            }
            k.eff_30c = m << 4;
            k.eff_310 = (k.eff_310.wrapping_add(k.eff_314)) & 0xFFF;
            // FUN_800861F0 (render) when +0x318 != 0.
        }
        16 => {
            if timer < 0x20 {
                k.target_x = timer;
            } else {
                k.target_x = 300 - timer;
                if 0x20 < 300 - timer {
                    k.target_x = 0x20;
                }
            }
            let (a, b) = (k.f188 as i32, k.f18a as i32);
            k.target_yaw = div_pow2((a + b) * k.target_x * 6, 7);
        }
        17 | 22 => {
            let v = k.target_x - dt;
            k.target_x = v;
            if v < 1 {
                if k.effect == 0x16 {
                    rng_skip(g, 1);
                }
                // FUN_8007D58C + one particle (4 bytes), then the next delay.
                rng_skip(g, 4);
                let b = crate::update::rng_next(g, w);
                k.target_x = (b >> 4) as i32 + 0xF;
            }
            if k.effect == 0x11 && crate::update::rng_next(g, w) < 8 {
                crate::sound::at(g, w, 0x58, 0x1400, 0xA0, [k.x, k.y, k.z], -1);
            }
        }
        18 => {
            if every(4) {
                pick_shape(k, g, w);
                k.shape_t = 0;
                k.shape_n = 0x1E;
            }
        }
        _ => {}
    }
}

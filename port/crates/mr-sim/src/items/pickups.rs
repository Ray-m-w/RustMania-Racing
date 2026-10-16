//! Pickup objects: item boxes (a weapon slot) and turbo tokens (+12 on the
//! turbo meter). FUN_8007B448 places them at race load; FUN_8007BBF0 runs
//! after the kart loop every frame: collection by each kart, then every
//! object's bounce, shrink / respawn / grow state machine and particle burst.

use super::{Cameras, ItemEvent, Pickup, PickupKind, FIRST_CHALLENGE, FIRST_TOKEN, PICKUP_SLOTS};
use crate::fixed::{div_pow2, s16};
use crate::globals::RaceGlobals;
use crate::kart::KartState;
use crate::update::{rng_next, rumble};
use crate::world::World;

/// Weapon types an item box can give, 32 entries indexed rand() >> 10
/// [exe 0x800AC178] (`Tables::weapon_pick`, shared with the slot recharge).
pub const WEAPON_PICK: u32 = 0x800A_C178;
/// Full turbo meter (+0x48A).
pub const METER_FULL: i16 = 0x1A4;
/// Meter added per token [CODE 0x8007C6E8].
pub const TOKEN_METER: i16 = 0xC;

/// A pickup list from the course .DAT: count, then x, y, z (TER units).
#[derive(Clone, Debug, Default)]
pub struct PickupList {
    pub points: Vec<[i32; 3]>,
}

/// The course's pickup lists (DAT chunks registered at 0x800D1C48 + 4 * id by
/// FUN_8002E3DC): item boxes id 0x13 (0x800D1C94), turbo tokens id 0x12
/// (0x800D1C90), challenge objects id 0x0C (0x800D1C78).
#[derive(Clone, Debug, Default)]
pub struct CoursePickups {
    pub boxes: Option<PickupList>,
    pub tokens: Option<PickupList>,
    pub challenge: Option<PickupList>,
}

pub mod chunk {
    pub const BOXES: i16 = 0x13;
    pub const TOKENS: i16 = 0x12;
    pub const CHALLENGE: i16 = 0x0C;
}

impl CoursePickups {
    pub fn from_dat(dat: &mr_assets::dat::Dat, bytes: &[u8]) -> CoursePickups {
        let le32 = |o: usize| i32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
        let list = |id: i16| {
            dat.chunks.iter().find(|c| c.id == id).map(|c| PickupList {
                points: (0..c.count as u16 as usize).map(|i| {
                    let o = c.offset + 4 + 12 * i;
                    [le32(o), le32(o + 4), le32(o + 8)]
                }).collect(),
            })
        };
        CoursePickups { boxes: list(chunk::BOXES), tokens: list(chunk::TOKENS), challenge: list(chunk::CHALLENGE) }
    }
}

/// FUN_8007B448 (race load, from the course loader FUN_8008ACE4): place the
/// item boxes, challenge objects and tokens; each token's colour is
/// rand() >> 13. Challenge-mode objects are placed but, in a race, hidden by
/// the race loader (FUN_8008C4AC @0x8008EBxx: state 2).
pub fn init(items: &mut super::ItemState, c: &CoursePickups, challenge_mode: bool, rand: &mut dyn FnMut() -> i32) {
    for p in items.pickups.iter_mut() {
        p.state = 0;
        p.burst = 0;
        p.vy = 0;
    }
    items.boxes = 0;
    items.challenge = [0; 3];
    items.tokens = 0;
    let place = |p: &mut Pickup, pt: [i32; 3]| {
        p.state = 1;
        p.x = pt[0];
        p.y = pt[1] << 5;
        p.z = pt[2];
        p.floor_y = p.y;
    };
    if let Some(l) = &c.boxes {
        let n = l.points.len();
        if n != 0 && n < 0x31 {
            items.boxes = n as u8;
            for (i, &pt) in l.points.iter().enumerate() {
                place(&mut items.pickups[i], pt);
            }
        }
    }
    if let Some(l) = &c.challenge {
        let n = l.points.len();
        if (n as u32).wrapping_sub(0xB) < 6 {
            // 10 targets, 1 finish object, n - 11 extras (FUN_8002DF88 sets
            // up their models: render side).
            items.challenge = [10, 1, (n as u8).wrapping_sub(0xB)];
            for (i, &pt) in l.points.iter().enumerate() {
                place(&mut items.pickups[FIRST_CHALLENGE + i], pt);
            }
        }
    }
    if let Some(l) = &c.tokens {
        let n = l.points.len();
        if n != 0 && n <= 0x80 {
            items.tokens = n as u8;
            for (i, &pt) in l.points.iter().enumerate() {
                let p = &mut items.pickups[FIRST_TOKEN + i];
                place(p, pt);
                p.colour = (rand() >> 13) as u8;
            }
        }
    }
    for pr in items.projectiles.iter_mut() {
        pr.active = false;
    }
    if !challenge_mode {
        // FUN_8008C4AC @0x8008EBxx: every challenge object hidden.
        let n = items.challenge.iter().map(|&c| c as usize).sum::<usize>();
        for i in 0..n {
            items.pickups[FIRST_CHALLENGE + i].state = 2;
        }
    }
}

/// FUN_8008C4AC @0x8008EB58 (an ADVENTURE's race load, phase 6): every
/// challenge object shown (state 1), then the ones already found hidden
/// (state 2): the targets with the course's secret 4, the Doozer goals
/// with 0x20, the movie object with 0x80.
pub fn adventure_load(items: &mut super::ItemState, course_flags: u16) {
    let [ten, goals, extra] = items.challenge.map(|c| c as usize);
    for i in 0..ten + goals + extra {
        let p = &mut items.pickups[FIRST_CHALLENGE + i];
        p.state = 1;
        let hide = (i < ten && course_flags & 4 != 0) || ((ten..ten + goals).contains(&i) && course_flags & 0x20 != 0) || (i == ten + goals && course_flags & 0x80 != 0);
        if hide {
            p.state = 2;
        }
    }
}

/// FUN_80081170(x, y, z, list, 0x20, 0, r): the first object of the list
/// whose list position is within `r` of (x, y, z) on every axis (TER units).
fn touching(list: &PickupList, x: i32, y: i32, z: i32, r: u32) -> Option<usize> {
    let span = r << 1;
    list.points.iter().position(|p| {
        (p[0].wrapping_sub(x) as u32).wrapping_add(r) < span
            && (p[1].wrapping_sub(y) as u32).wrapping_add(r) < span
            && (p[2].wrapping_sub(z) as u32).wrapping_add(r) < span
    })
}

/// FUN_8007BBF0: pickups for every kart, then every object's state.
/// `karts` in slot order; `cams` the human cameras (particle bursts only
/// near a camera); `rand` the BIOS rand() (item boxes' weapon draw).
pub fn frame(karts: &mut [KartState], g: &mut RaceGlobals, w: &World, c: &CoursePickups, cams: &Cameras, rand: &mut dyn FnMut() -> i32) {
    // 60 Hz mode: karts collect on the first tick of a pair, at the game's
    // 30 Hz sampling of their positions (every tick: +8 % tokens).
    if g.mode != 0x12 && g.hz60.beat() {
        for s in 0..karts.len() {
            let k0 = karts[0].finished;
            collect(&mut karts[s], s, k0, g, w, c, cams, rand);
        }
    }
    if g.items.challenge[0] != 0 && g.items.challenge[1] != 0 {
        // FUN_8002DC14: challenge objects' model matrices (render side).
    }
    if g.mode == 0x12 {
        return;
    }
    for i in 0..PICKUP_SLOTS {
        step(i, g, w);
    }
}

/// ADVENTURE (0x800B07E6): a secret found ends the run: `done`
/// (0x800B0803), the course's save bit, the chime, and the player is out
/// (+0x4C7 = 1, the AI takes it).
fn adventure_done(k: &mut KartState, g: &mut RaceGlobals, done: u8, bit: u16, event: u8) {
    g.adv.done = done;
    crate::sound::play(g, 0x24, 5000, 0x80, 0x80);
    g.adv.course_flags |= bit;
    k.pause_bits = 0x80;
    k.finished = 1;
    k.event_bits |= event;
    k.flags |= 1;
}

#[allow(clippy::too_many_arguments)]
fn collect(k: &mut KartState, slot: usize, kart0_finished: u8, g: &mut RaceGlobals, w: &World, c: &CoursePickups, cams: &Cameras, rand: &mut dyn FnMut() -> i32) {
    // 0x8007BC54: no special move, no pickup animation, no carried object,
    // no effect.
    if !(k.special == 0 && (k.pickup == -1 || k.wall_flag != 0) && k.item_e7 == -1 && k.effect == -1) {
        return;
    }
    let (x, y, z) = (k.drv_x >> 5, k.ref_y >> 5, k.drv_z >> 5);
    // A human camera close by: the pickup bursts into particles.
    let near = cams.near(k.x >> 5, k.y >> 5, k.z >> 5, 10000);
    let adv = g.challenge != 0;
    if g.items.boxes != 0 {
        // The radius is unsigned in the game (0x800C3068 = 0 < r).
        let r = (k.spark_offset as i32 + 0x80) as u32;
        if let Some(i) = c.boxes.as_ref().and_then(|l| touching(l, x, y, z, r)) {
            if g.items.pickups[i].state == 1 && 0 < r {
                let mut take = false;
                if k.flags & 0x60 == 0 {
                    if k.weapons[4] as i8 == -1 {
                        k.weapons[0] = 0xFF;
                        k.weapons[1] = w.tables.weapon_pick[(rand() >> 10) as usize];
                        k.weapons[4] = 0;
                        take = true;
                    } else if k.weapons[5] as i8 == -1 {
                        k.weapons[2] = 0xFF;
                        k.weapons[3] = w.tables.weapon_pick[(rand() >> 10) as usize];
                        k.weapons[5] = 0;
                        take = true;
                    } else if adv && k.player_index == 0 {
                        // ADVENTURE: the player takes a box with both slots
                        // full too (0x8007BED8).
                        take = true;
                    }
                }
                if take {
                    // 0x8007BF38: ADVENTURE counts the player's boxes; the
                    // last one is the course's secret 8 (0x800B0803 = 2).
                    if adv && k.player_index == 0 && k.finished == 0 && g.adv.course_flags & 8 == 0 {
                        k.adv_boxes = k.adv_boxes.wrapping_add(1);
                        if k.adv_boxes == g.items.boxes {
                            adventure_done(k, g, 2, 8, 2);
                        }
                    }
                    let p = &mut g.items.pickups[i];
                    p.state = 3;
                    p.scale_vel = 0;
                    if near {
                        p.burst = 5;
                    }
                    rumble(k, g, 0x96);
                    crate::sound::at(g, w, 0x16, 0x1600, 0x70, [k.x, k.y, k.z], -1);
                    g.items.events.push(ItemEvent::Pickup { kart: slot as u8, index: i as u16, kind: PickupKind::Box });
                } else {
                    // Slots full (or parked): the box only bounces.
                    let p = &mut g.items.pickups[i];
                    if p.floor_y.wrapping_sub(p.y) < 0x1000 {
                        p.vy = p.vy.wrapping_sub(0x180);
                    }
                }
            }
        }
    }
    // 0x8007C0B0: ADVENTURE's challenge objects (DAT chunk 0x0C, slots
    // 0x30..): `ten` targets, then `goals` Doozer goals, then the movie
    // object (and any extras).
    let (ten, goals) = (g.items.challenge[0] as usize, g.items.challenge[1] as usize);
    if adv && ten != 0 && goals != 0 {
        let r = (k.spark_offset as i32 + 0x80) as u32;
        if let Some(t) = c.challenge.as_ref().and_then(|l| touching(l, x, y, z, r)) {
            let i = FIRST_CHALLENGE + t;
            if g.items.pickups[i].state == 1 && 0 < r {
                let goal = (FIRST_CHALLENGE + ten..FIRST_CHALLENGE + ten + goals).contains(&i);
                if !goal {
                    if k.player_index == 0 && k.finished == 0 {
                        let movie = FIRST_CHALLENGE + ten + goals;
                        if i < movie {
                            // A target: the count, a cheer (event bits).
                            k.adv_targets = k.adv_targets.wrapping_add(1);
                            let b = rng_next(g, w);
                            if b < 0x55 {
                                k.event_bits |= 2;
                            } else if b < 0xAA {
                                k.event_bits |= 4;
                            }
                            k.event_bits |= 8;
                            if k.adv_targets as usize == ten {
                                adventure_done(k, g, 1, 4, 0);
                            }
                        } else if i == movie {
                            // The movie object: secret 0x80 (0x800B0803 =
                            // 0x10; FUN_8007916C ends the run).
                            g.adv.done = 0x10;
                            g.adv.course_flags |= 0x80;
                            crate::sound::play(g, 0x24, 5000, 0x80, 0x80);
                            k.event_bits |= 8;
                        }
                        let p = &mut g.items.pickups[i];
                        p.state = 3;
                        p.scale_vel = 0;
                        if near {
                            p.burst = 5;
                        }
                        rumble(k, g, 0x96);
                        crate::sound::at(g, w, 0x16, 0x1600, 0x70, [k.x, k.y, k.z], -1);
                    }
                } else if kart0_finished == 0 {
                    if k.flags & 0x40 == 0 {
                        // Anyone else at the goal: the kart flashes.
                        if k.player_index == 0 {
                            k.flash_timer = 0xC;
                            k.blink_count = 0;
                        }
                    } else {
                        // The Doozer (the parked kart with flag 0x40) pushed
                        // onto its goal: rescued (secret 0x20, 0x800B0803 =
                        // 8); +0x4EE = the goal (FUN_800627E0 takes it in).
                        k.event_bits |= 1;
                        k.item_ee = (i - FIRST_CHALLENGE - ten) as i8;
                        g.adv.done = 8;
                        crate::sound::play(g, 0x24, 5000, 0x80, 0x80);
                        g.adv.course_flags |= 0x20;
                    }
                }
            }
        }
    }
    if g.items.tokens != 0 {
        let r = (k.spark_offset as i32 + 0x60) as u32;
        if let Some(t) = c.tokens.as_ref().and_then(|l| touching(l, x, y, z, r)) {
            let i = FIRST_TOKEN + t;
            if g.items.pickups[i].state == 1 && 0 < r {
                if k.flags & 0x60 == 0 && (k.start_timer < METER_FULL || (adv && k.player_index == 0)) {
                    // 0x8007C5xx: ADVENTURE counts the player's tokens; the
                    // last one is secret 0x10 (0x800B0803 = 4).
                    if adv && k.player_index == 0 && k.finished == 0 && g.adv.course_flags & 0x10 == 0 {
                        k.adv_tokens = k.adv_tokens.wrapping_add(1);
                        if k.adv_tokens == g.items.tokens {
                            adventure_done(k, g, 4, 0x10, 2);
                        }
                    }
                    if g.mode != 7 || (k.player_index as i32) < g.karts as i32 {
                        k.start_timer = k.start_timer.wrapping_add(TOKEN_METER);
                    }
                    if k.start_timer > METER_FULL {
                        k.start_timer = METER_FULL;
                    }
                    let p = &mut g.items.pickups[i];
                    p.state = 3;
                    p.scale_vel = 0;
                    k.token_colour = p.colour;
                    if near {
                        p.burst = 7;
                    }
                    rumble(k, g, 100);
                    crate::sound::at(g, w, 0x18, 0x1538, 0x70, [k.x, k.y, k.z], -1);
                    g.items.events.push(ItemEvent::Pickup { kart: slot as u8, index: i as u16, kind: PickupKind::Token });
                } else {
                    let p = &mut g.items.pickups[i];
                    if p.floor_y.wrapping_sub(p.y) < 0x1000 {
                        p.vy = p.vy.wrapping_sub(0x180);
                    }
                }
            }
        }
    }
}

/// One object of the per-frame loop (0x8007C840..0x8007D0F4).
fn step(i: usize, g: &mut RaceGlobals, w: &World) {
    let dt = g.dt;
    let challenge = g.challenge != 0;
    let in_challenge_range = (i as u32).wrapping_sub(FIRST_CHALLENGE as u32) as u16 <= 0xF;
    {
        let p = &mut g.items.pickups[i];
        if p.state != 0 {
            // Bounce on the rest height under gravity.
            for _ in 0..crate::fixed::substeps(dt) {
                let y = p.y.wrapping_add(p.vy).wrapping_add(0x60);
                p.y = y;
                p.vy = p.vy.wrapping_add(0x60);
                if p.floor_y < y {
                    p.y = p.floor_y;
                    p.vy = -p.vy / 2;
                }
            }
        }
    }
    let state = g.items.pickups[i].state as i16;
    match state {
        2 => {
            if !challenge && !in_challenge_range {
                if g.items.pickups[i].timer == 0 {
                    let p = &mut g.items.pickups[i];
                    p.state = 4;
                    p.scale_vel = 0;
                } else {
                    let n = s16(g.items.pickups[i].timer as i32 - dt);
                    g.items.pickups[i].timer = n as i16;
                    if n < 0 {
                        g.items.pickups[i].timer = 0;
                        if i >= FIRST_TOKEN {
                            // A token comes back in a different colour.
                            let old = g.items.pickups[i].colour;
                            let mut b;
                            loop {
                                b = rng_next(g, w);
                                if b >> 6 != old {
                                    break;
                                }
                            }
                            g.items.pickups[i].colour = b >> 6;
                        }
                    }
                }
            }
            g.items.pickups[i].scale = 0;
        }
        1 => {
            let p = &mut g.items.pickups[i];
            if in_challenge_range && !challenge {
                p.state = 2;
                p.scale = 0;
            } else {
                p.scale = 0x80;
            }
        }
        3 => {
            let p = &mut g.items.pickups[i];
            for _ in 0..crate::fixed::substeps(dt) {
                p.scale_vel = p.scale_vel.wrapping_sub(1);
                p.scale = p.scale.wrapping_add(p.scale_vel);
            }
            if p.scale < 1 {
                p.state = 2;
                p.scale = 0;
                p.timer = 0x1E;
            }
        }
        4 => {
            let p = &mut g.items.pickups[i];
            for _ in 0..crate::fixed::substeps(dt) {
                p.scale_vel = p.scale_vel.wrapping_add(1);
                p.scale = p.scale.wrapping_add(p.scale_vel);
            }
            if p.scale > 0x7F {
                p.state = 1;
                p.scale = 0x80;
            }
        }
        _ => {}
    }
    burst(i, g, w);
}

/// The particle burst a collected object asked for (+0x1E): FUN_800870E8
/// particles (track B draws them); here only their random-byte draws.
fn burst(i: usize, g: &mut RaceGlobals, w: &World) {
    let n = g.items.pickups[i].burst as usize;
    if n == 0 {
        return;
    }
    let p = g.items.pickups[i];
    let b = |g: &mut RaceGlobals| rng_next(g, w) as i32;
    let sb = |v: i32| v as u8 as i8 as i32;
    if i < FIRST_TOKEN {
        // Boxes (and challenge objects): a fountain of 8-byte particles.
        let box_ = i < FIRST_CHALLENGE;
        for _ in 0..n {
            let r: Vec<i32> = (0..8).map(|_| b(g)).collect();
            let col = |v: i32| if box_ { (sb(v) >> 1).wrapping_sub(0x80) & 0xFF } else { ((sb(v) >> 2) + 0x60) & 0xFF };
            let sz = if box_ { 0x60 } else { 0x30 };
            super::particle(
                g,
                [p.x << 5, p.y, p.z << 5],
                [sb(r[0]) << 1, p.vy + r[1] * -2 - 0x80, sb(r[2]) << 1],
                0x20,
                r[3] << 4,
                div_pow2(sb(r[4]), 2),
                1,
                [col(r[5]), col(r[6]), col(r[7])],
                sz,
                sz,
                0x20,
            );
        }
    } else {
        // Tokens: sparkles in the token's colour (colours 1 and 2 also spin
        // every particle, the others only the first two).
        let (kind, rgb, full) = match p.colour {
            0 => (0xC, [0xA0, 0, 0xA0], false),
            1 => (0x18, [0x80; 3], true),
            2 => (0x1A, [0x80; 3], true),
            3 => (0xC, [0xC0, 0, 0], false),
            _ => (0, [0x80; 3], false),
        };
        for j in 0..n {
            let r: Vec<i32> = (0..6).map(|_| b(g)).collect();
            let (a, s) = if j < 2 || full { let a = b(g) << 4; (a, div_pow2(sb(b(g)), 1)) } else { (0, 0) };
            let mut k = 0xD;
            if j > 1 {
                k = kind;
                if j == n - 1 && p.colour == 1 {
                    k = 0x19;
                }
            }
            let c = if k == 0xD || k == 0x19 { [0x80; 3] } else { rgb };
            super::particle(
                g,
                [(p.x << 5) + div_pow2(sb(r[0]), 4) * 4, p.y + div_pow2(sb(r[1]), 4) * 4, (p.z << 5) + div_pow2(sb(r[2]), 4) * 4],
                [sb(r[3]), p.vy + sb(r[4]), sb(r[5])],
                8,
                a,
                s,
                k,
                c,
                0x30,
                0x30,
                0x18,
            );
        }
    }
    g.items.pickups[i].burst = 0;
}

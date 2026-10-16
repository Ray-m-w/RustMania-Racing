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
    if k.special != 0 {
        // 0x80067950: a special move only runs the ground contact.
        crate::ground::kart_ground(k, g, w, inp);
        return;
    }
    // FUN_800628FC: the stunt courses' pipes (chunk 0x14, stunt.rs); a
    // kart in a pipe skips the rest.
    crate::stunt::pipes(k, g, w);
    if k.item_e7 != -1 {
        crate::stunt::mirrors(k, w);
        return;
    }
    // FUN_80061FC0: proximity triggers (list 0x800D1C9C, count 0x800B0DC6).
    teleport(k, g, w);
    if k.pickup == -1 || k.wall_flag != 0 {
        // FUN_80061BC4: GRAVE's hooks (hooks.rs).
        crate::hooks::kart(k, g);
        // FUN_800632BC: turntables (objects.rs).
        if k.carried == -1 {
            crate::objects::turntable(k, g, w);
        }
        crate::ground::kart_ground(k, g, w, inp);
        // FUN_80063C24: jets (objects.rs).
        if k.carried == -1 {
            crate::objects::jet_push(k, g, w);
        }
        if k.flags & 0x40 != 0 && k.item_ee != -1 {
            doozer_goal(k, g, w);
        }
    }
    // FUN_80062E38: the mirror planes (stunt.rs; lists 0x800D1C68 /
    // 0x800D1CA4, null on the race courses: +0x4F8 = -1).
    crate::stunt::mirrors(k, w);
}

/// FUN_800627E0 (ADVENTURE, phase 6): the rescued Doozer (the parked kart
/// with flag 0x40, +0x4EE = its goal among the challenge objects after the
/// ten targets) eases onto the goal point, 1/16 of the way per VBlank, its
/// velocity cleared and its engine (+0x442) running down by 0x40 a VBlank.
fn doozer_goal(k: &mut KartState, g: &RaceGlobals, w: &World) {
    let (ten, goals) = (g.items.challenge[0] as usize, g.items.challenge[1]);
    if ten == 0 || goals == 0 {
        return;
    }
    let Some(p) = w.pickups.challenge.as_ref().and_then(|l| l.points.get((ten as i32 + k.item_ee as i32) as usize)).copied() else {
        return;
    };
    for _ in 0..substeps(g.dt) {
        k.x = k.x.wrapping_add(div_pow2((p[0] << 5).wrapping_sub(k.x), 4));
        k.y = k.y.wrapping_add(div_pow2((p[1] << 5).wrapping_sub(k.y), 4));
        k.z = k.z.wrapping_add(div_pow2((p[2] << 5).wrapping_sub(k.z), 4));
    }
    k.vx = 0;
    k.vy = 0;
    k.vz = 0;
    let e = (k.engine as u16 as i32).wrapping_sub(g.dt.wrapping_mul(0x40));
    k.engine = if (e << 16) < 0 { 0 } else { e as i16 };
}

/// FUN_80061FC0: touching a trigger object (within 16 units >> 10 of its
/// first point) takes the kart to its second point over 240 VBlanks: it
/// shrinks away (+0x442 to 0 over 60), flies (120: position eased toward a
/// point moving from the first to the second, heading along the line, body
/// flat), grows back at the second point (60), then is released. +0x4E6 =
/// trigger, +0x4B5 = clock, +0x4EF = 1, +0x4B3 = done. DOCK has two (the
/// course's own warp). The paired list (0x800D1C7C, count 0x800B0C68:
/// two-way links, state 2; MOUNTAIN, DINER) is checked first.
fn teleport(k: &mut KartState, g: &mut RaceGlobals, w: &World) {
    if k.effect != -1 {
        return;
    }
    let (x, y, z) = (k.x >> 10, k.y >> 10, k.z >> 10);
    let near = |p: [i32; 3]| {
        let (dx, dy, dz) = ((p[0] >> 5) - x, (p[1] >> 5) - y, (p[2] >> 5) - z);
        dx.wrapping_mul(dx).wrapping_add(dy.wrapping_mul(dy)).wrapping_add(dz.wrapping_mul(dz)) < 0x100
    };
    // The paired list (chunk 0x0D, 0x800D1C7C; FUN_80061F08: 0x800B0C68 =
    // count / 2 when the count is a multiple of 4): (point, heading point)
    // pairs; touching entry i's point takes the kart to its partner i ^ 1
    // (state 2). Checked before the one-way list.
    let paired: &[[i32; 3]] = match &w.course.paired {
        Some(l) if l.len() & 3 == 0 && l.len() >= 4 => l,
        _ => &[],
    };
    let mut use_paired = None;
    if !paired.is_empty() {
        if k.pickup == -1 {
            if let Some(i) = (0..paired.len() / 2).find(|&i| near(paired[2 * i])) {
                k.pickup = ((i & 0xFE) + ((i + 1) & 1)) as i8;
                k.pickup_timer = 0;
                k.pickup_state = 2;
                k.event_bits |= 8;
                use_paired = Some(true);
            }
        } else {
            use_paired = Some(k.pickup_state != 1);
        }
    }
    let use_paired = match use_paired {
        Some(u) => u,
        None => {
            // FUN_80061FC0 @0x8006216C: the one-way list (0x800B0DC6).
            if w.course.triggers.is_empty() {
                return;
            }
            if k.pickup == -1 {
                let Some(i) = w.course.triggers.iter().position(|tr| near(tr.pos)) else { return };
                k.pickup = i as i8;
                k.pickup_timer = 0;
                k.pickup_state = 1;
                k.event_bits |= 8;
                false
            } else {
                k.pickup_state == 2 && !paired.is_empty()
            }
        }
    };
    // from (piVar14), to (piVar13) and the heading point after `to`.
    let tr = if use_paired {
        let p = k.pickup as u8 as usize;
        let from = if p & 1 == 0 { paired[2 * p + 2] } else { paired[2 * p - 2] };
        crate::world::Trigger { pos: from, to: paired[2 * p], dir: paired[2 * p + 1] }
    } else {
        w.course.triggers[k.pickup as u8 as usize]
    };
    let t0 = k.pickup_timer;
    if t0 == 0 {
        crate::sound::at(g, w, 0x1E, 0x1600, 0x60, [k.x, k.y, k.z], -1);
    }
    // 60 Hz mode: the end-point jitter is drawn once per pair.
    let (r0, r1, r2) = g.replay_draws(|g| {
        let r0 = crate::update::rng_next(g, w) as i8 as i32;
        let r1 = crate::update::rng_next(g, w) as i8 as i32;
        let r2 = crate::update::rng_next(g, w) as i8 as i32;
        (r0, r1, r2)
    });
    let mut end = [tr.to[0] * 0x20 + r0 * 2, tr.to[1] * 0x20 + r1 * 2, tr.to[2] * 0x20 + r2 * 2];
    if g.game_state == 0xC {
        // Course 12 (MOUNTAIN): the end point 1.5 x 0x4000 further along
        // the heading (0x800621D8).
        let a = ratan2(&w.tables, tr.dir[0] - tr.to[0], tr.dir[2] - tr.to[2]) & 0xFFF;
        end[0] += (w.tables.sin_raw(a) * 3) / 2;
        end[2] += (w.tables.cos_raw(a) * 3) / 2;
    }
    let t = t0 as i32;
    let start = [tr.pos[0] << 5, tr.pos[1] << 5, tr.pos[2] << 5];
    let mut scale: Option<i32> = None;
    let target;
    if t < 0x3C {
        // Shrink at the start point.
        target = start;
        scale = Some(((t - 0x3C).abs()).min(0x3C));
    } else if t - 0x3C < 0x78 {
        // Fly: toward a point moving along the line.
        let s = t - 0x3C;
        target = [0, 1, 2].map(|i| start[i] + ((end[i] - start[i]) * s) / 0x78);
        k.engine = 0;
        k.heading = (ratan2(&w.tables, tr.dir[0] - tr.to[0], tr.dir[2] - tr.to[2]) & 0xFFF) as i16;
        k.tilt_nx = 0;
        k.tilt_ny = -0x1000;
        k.tilt_nz = 0;
        k.pitch = 0;
        k.roll = 0;
    } else if (t + 0x4C) & 0xFF >= 0x3C {
        // Released (0xF0): full size.
        target = end;
        k.engine = 0x1000;
    } else {
        // Grow at the end point.
        target = end;
        scale = Some((t - 0xB4).min(0x3C));
    }
    if let Some(sc) = scale {
        k.engine = (((sc << 16) >> 4) / 0x3C) as i16;
    }
    if k.pickup_timer < 0xF0 {
        for _ in 0..substeps(g.dt) {
            k.x = k.x.wrapping_add(div_pow2(target[0].wrapping_sub(k.x), 4));
            k.y = k.y.wrapping_add(div_pow2(target[1].wrapping_sub(k.y), 4));
            k.z = k.z.wrapping_add(div_pow2(target[2].wrapping_sub(k.z), 4));
        }
        k.vx = 0;
        k.vy = 0;
        k.vz = 0;
        let n = (k.pickup_timer as i32 + g.dt) as u8;
        k.pickup_timer = if n > 0xEF { 0xF0 } else { n };
        k.wall_flag = 0;
        if t0 < 0xB4 && 0xB3 < k.pickup_timer {
            crate::sound::at(g, w, 0x1E, 0x1600, 0x60, [k.x, k.y, k.z], -1);
        }
    } else {
        k.wall_flag = 1;
    }
    if k.wall_flag != 0 && (k.pickup_state != 2 || !near(tr.to)) {
        // Released (state 1; state 2 waits until the kart has left the
        // second point).
        k.pickup = -1;
        k.wall_flag = 0;
        k.pickup_state = 0;
    }
}

/// FUN_8003D208: 1 when (px, pz) is on the non-negative side of the line
/// (ax, az) -> (bx, bz).
fn side(px: i32, pz: i32, ax: i32, az: i32, bx: i32, bz: i32) -> i32 {
    let v = (px - ax).wrapping_mul(bz - az).wrapping_add((pz - az).wrapping_mul(ax - bx));
    (!v as u32 >> 31) as i32
}

/// FUN_80082708: lap time accumulation and the finish-line side bits.
pub fn lap_timing(k: &mut KartState, g: &mut RaceGlobals, w: &World, rand: &mut dyn FnMut() -> i32) {
    if g.challenge != 0 || g.countdown != 0 || g.mode == 20 {
        return;
    }
    let battle = g.mode == 2 || g.mode == 7;
    if battle && g.race_flags & 8 == 0 && k.finished == 0 && k.impact < 1000 && !(g.game_state == 0x1E && k.impact >= 0x7D) {
        // A battle (modes 2 / 7; phase 4, docs/spec/battle.md): no laps,
        // the kart's own clock +0x358 runs (at most 180000) while it is
        // standing (0x80082790).
        k.race_time = k.race_time.wrapping_add(g.dt * 5).min(180_000);
        return;
    }
    if (battle || g.race_flags & 8 != 0) && k.finished == 0 && (k.impact >= 1000 || (g.game_state == 0x1E && k.impact >= 0x7D)) {
        // Race flag 8 (DISCO: the menu sets it for course 0x1F) or a
        // battle: a kart with 1000 damage is knocked out (0x8008279C);
        // with race flag 8 lap timing goes on either way (LAB_80082B84).
        knocked_out(k, g, w);
        // The title (course 0x1E): the kart is re-created at once
        // (0x800828C4; title.rs); with race flag 8 the lap timing below
        // then runs on the new record.
        if g.game_state == crate::title::TITLE_COURSE as i32 {
            crate::title::respawn_in_update(k, g, w, rand);
        }
    }
    if battle && g.race_flags & 8 == 0 {
        return;
    }
    if g.race_type == 3 {
        // 0x80082A40: the stunt trial's gates and clock (stunt.rs).
        crate::stunt::timing(k, g, w);
        return;
    }
    if (k.lap as i32) >= g.laps as i32 || g.mode == 10 || g.mode == 17 {
        return;
    }
    if k.race_time >= 180_000 {
        return;
    }
    let lap = k.lap as usize;
    k.lap_time[lap] = k.lap_time[lap].wrapping_add(g.dt * 5);
    k.race_time = 0;
    for i in 0..=lap {
        k.race_time = k.race_time.wrapping_add(k.lap_time[i]);
    }
    if 180_000 < k.race_time {
        k.race_time = 180_000;
    }
    let Some(f) = &w.course.finish else { return };
    let q = |i: usize| f.words[i] >> 4;
    let (px, pz) = (k.x >> 9, k.z >> 9);
    let s = side(px, pz, q(7), q(9), q(4), q(6))
        | side(px, pz, q(1), q(3), q(4), q(6)) << 1
        | side(px, pz, q(7), q(9), q(10), q(12)) << 2;
    // REVERSE RACE (race flag 2, phase 6b; 0x80083378..0x80083438): the
    // line counts the other way round (6 after 7 completes a lap, 7 after
    // 6 is the wrong way); +0x4C0 starts at 7.
    let (fwd, back) = if g.race_flags & 2 != 0 { ((6, 7), (7, 6)) } else { ((7, 6), (6, 7)) };
    if (s, k.line_side) == fwd {
        if k.line_flag == 0 {
            lap_completed(k, g, w);
        }
        k.line_flag = 0;
    } else if (s, k.line_side) == back {
        // 0x80083458: crossed backwards: remember it so the next forward
        // crossing does not count a lap; a second backward crossing adds a
        // course length to the race progress.
        if k.line_flag == 1 {
            if let Some(len) = w.course.length {
                k.progress = k.progress.wrapping_add(len as i32);
            }
        }
        k.line_flag = 1;
    }
    k.line_side = s as u8;
}

/// 0x80082DA0: a lap was completed.
fn lap_completed(k: &mut KartState, g: &mut RaceGlobals, w: &World) {
    let lap = k.lap as usize;
    let t = k.lap_time[lap];
    // Round the lap time to the nearest multiple of 3.
    k.lap_time[lap] = (t / 3) * 3;
    if 1 < t % 3 {
        k.lap_time[lap] += 3;
    }
    let lt = k.lap_time[lap];
    if (k.player_index as i32) < g.humans as i32 && k.flags & 1 == 0 && g.race_flags & 2 == 0 && lt < g.lap_record {
        // New course lap record: the record table 0x800E0190 (and the
        // vehicle / character bytes next to it) take this lap (the save
        // block outside the simulation: SaveBlock::lap_record from
        // `lap_record`, mr-bevy save.rs).
        g.lap_record = lt;
    }
    if k.best_lap == 0 || lt < k.best_lap {
        k.best_lap = lt;
    }
    if g.lap_threshold == 0 || k.best_lap < g.lap_threshold {
        g.lap_threshold = k.best_lap;
    }
    k.lap = k.lap.wrapping_add(1);
    crate::sound::lap(k, g, w);
    let lap = k.lap as i32;
    let laps = g.laps as i32;
    if lap == laps || g.end_mode > 1 {
        race_finished(k, g, w);
        k.line_flag = 0;
        return;
    }
    if lap == laps - 1 {
        k.flash_timer = 8;
        k.blink_count = 0;
    } else if lap < laps {
        k.flash_timer = (lap - 1) as i8;
        k.blink_count = 0;
    }
}

/// 0x8008279C (race flag 8): an explosion (FUN_800870E8 kind 0x11 at the
/// driver, one random byte for its angle), the body squashes (FUN_80060878,
/// unless an effect runs), +0x4C7 = 2, the AI takes the kart (+0x4F9 0x80,
/// +0x496 | 1) and it spins (FUN_8007B26C(kart, 1)). An AI kart's voice
/// bits draw one more byte (a human's knock-out voice is audio only).
fn knocked_out(k: &mut KartState, g: &mut RaceGlobals, w: &World) {
    let b = crate::update::rng_next(g, w) as i32;
    crate::items::particle(g, [k.drv_x, k.ref_y, k.drv_z], [k.vx, k.vy, k.vz], 0, b << 4, 0, 0x11, [0x80; 3], 0x180, 0x180, 0x20);
    if k.effect == -1 {
        crate::items::specials::morph(k, [0x400; 3], [0x1000; 3], 0x3C);
    }
    if g.race_flags & 8 == 0 {
        // A battle (0x800827F0): the kart takes the place of the karts
        // left (0x800B0CD8, then one fewer), its damage 1000 + that place.
        k.race_pos = g.end_mode;
        k.impact = (g.end_mode as i16).wrapping_add(1000);
        g.end_mode = g.end_mode.wrapping_sub(1);
    }
    k.finished = 2;
    if !((k.player_index as i32) < g.humans as i32 && g.game_state != 0x1E) {
        let r = crate::update::rng_next(g, w) as i8;
        k.event_bits |= if r < 0 { 0x20 } else { 0x10 };
    }
    k.pause_bits = 0x80;
    k.flags |= 1;
    crate::collide::spin_out(k, g, w, 1);
}

/// 0x80083004: the kart crossed the line for the last time (its last lap,
/// or any lap once another kart has finished: 0x800B0CD8 > 1). It takes the
/// next finishing place, the game hands it to the AI (flags bit 0) and its
/// race progress is pinned so the finishing order stays sorted.
fn race_finished(k: &mut KartState, g: &mut RaceGlobals, w: &World) {
    k.finished = 1;
    k.pause_bits = 0x80;
    k.race_pos = g.end_mode;
    k.flags |= 1;
    if (k.player_index as i32) < g.humans as i32 {
        // FUN_80060580(character, 3 if won else 2): the finish voice (audio
        // queue 0x800B0744, not modelled).
    } else {
        let b = crate::update::rng_next(g, w) as i8;
        let bits = if g.end_mode == 1 { if b < 0 { 4 } else { 2 } } else if b < 0 { 0x20 } else { 0x10 };
        k.event_bits |= bits;
    }
    if g.race_flags & 0x10 != 0 && k.race_pos == 1 && k.player_index == 0 {
        // 0x800830xx: the "BEAT x" race won: the character is collected
        // (0x800E02BC) and the course's boss is beaten (secret 0x40).
        g.adv.characters |= 1 << (g.adv.tagged as u32 & 0x1F);
        g.adv.course_flags |= 0x40;
        crate::sound::play(g, 0x24, 5000, 0x80, 0x80);
    }
    if (k.lap as i32) < g.laps as i32 {
        // Finished early (someone else already has): count the karts still
        // racing that are on a later lap as ahead.
        for (j, &(fin, lap)) in g.field.iter().enumerate().take(g.karts.max(0) as usize) {
            if j as i32 != k.player_index as i32 && fin == 0 && (k.lap as i32 - 1) < lap as i32 {
                k.race_pos = k.race_pos.wrapping_add(1);
            }
        }
        k.race_time = 0;
        for i in 0..k.lap as usize {
            k.race_time = k.race_time.wrapping_add(k.lap_time[i]);
        }
    }
    k.pos_flash = 0x10;
    k.shown_pos = k.race_pos;
    if let Some(len) = w.course.length {
        k.progress = (len as i32).wrapping_mul(k.lap as i32).wrapping_add(1).wrapping_add(g.karts as i32).wrapping_sub(g.end_mode as i32);
        // 0x800832EC..0x8008339C: the player winning a plain race sets the
        // course's "won" bit in the save block (0x800E0196 + 0x20 * world +
        // 8 * slot): applied outside the simulation, from the finished
        // kart (mr_sim::save::SaveBlock::race_won, mr-bevy save.rs).
    }
    g.end_mode = g.end_mode.wrapping_add(1);
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
/// `karts`: every kart's (x, z) as the loop has left them (the battle's
/// target is read live).
/// `rand`: the BIOS rand() a backward walk (REVERSE RACE, +0x4DF = 1) may
/// draw past a segment without predecessors.
pub fn track_direction(k: &mut KartState, g: &mut RaceGlobals, w: &World, karts: &[(i32, i32)], rand: &mut dyn FnMut() -> i32) {
    if g.race_type == 3 {
        return;
    }
    if g.mode != 1 {
        // Modes 2 / 7 (battles): toward the nearest kart (+0x4DC); mode
        // 0x14 (CHICKEN CHASE, two players: not ported) toward the chicken
        // 0x800CB658 (0x80086C88).
        let target = match g.mode {
            2 | 7 => {
                let Some(t) = karts.get(k.nearest_kart as usize).filter(|_| k.nearest_kart >= 0) else { return };
                (s16(t.0 >> 9), s16(t.1 >> 9))
            }
            0x14 => unreachable_in_scope!("track direction for mode 0x14 (CHICKEN CHASE)"),
            _ => return,
        };
        let a = ratan2(&w.tables, target.0 - (k.x >> 9), target.1 - (k.z >> 9));
        steer_track_dir(k, g, a);
        return;
    }
    let mut n: i32 = 3;
    if k.warn_timer == 0 {
        let v = s16(k.speed_est >> 5).clamp(0x18, 0x37);
        n = v >> 3;
    }
    // +0x4DF = 1 (REVERSE RACE, phase 6b): FUN_80090670(seg, pt, n + 1)
    // with 0x800B0A20 set.
    let ahead = if k.path_mode != 0 {
        let wk = crate::ai::walk_backward_first(w, k.path_seg, k.path_pt, (n + 1) as i16, true, rand);
        (wk.seg != -1).then_some((wk.seg, wk.pt))
    } else {
        path_ahead(w, k.path_seg, k.path_pt, n)
    };
    let Some((seg, pt)) = ahead else { return };
    let ter = &w.ter;
    let s = &ter.path_segments[seg as usize];
    let p = &ter.path_points[s16(s.first as u16 as i32 + pt as i32) as usize];
    let px = s16(((p.x as u32) >> 4) as i32);
    let pz = s16(((p.z as u32) >> 4) as i32);
    let a = ratan2(&w.tables, px - (k.x >> 9), pz - (k.z >> 9));
    steer_track_dir(k, g, a);
}

/// 0x80086D7C: ease +0x48E an eighth of the way toward angle `a`, dt times.
fn steer_track_dir(k: &mut KartState, g: &RaceGlobals, a: i32) {
    for _ in 0..substeps(g.dt) {
        let d = s16((a & 0xFFF) - (k.track_dir as u16 as i32));
        let d = wrap_angle(d);
        k.track_dir = (((k.track_dir as i32) + div_pow2(s16(d), 3)) & 0xFFF) as i16;
    }
}

/// FUN_8009004C: the kart's position along the TER path: the nearest path
/// point (segments whose bounding box contains the kart, plus the current
/// one), the lateral offset and track width there, and the heading error
/// against the path (+0x48C, the wrong-way monitor's input).
pub fn path_position(k: &mut KartState, w: &World) {
    let t = &w.tables;
    let ter = &w.ter;
    let mut best_seg: i32 = -1;
    let mut best_pt: i32 = 0;
    let mut best = u32::MAX >> 1;
    k.path_flag_4cb = 0;
    let (px, py, pz) = (k.x >> 5, k.y >> 5, k.z >> 5);
    for (i, s) in ter.path_segments.iter().enumerate() {
        let inside = px >= s.bb_min[0] && px <= s.bb_max[0] && py >= s.bb_min[1] && py <= s.bb_max[1] && pz >= s.bb_min[2] && pz <= s.bb_max[2];
        if !(k.path_seg == -1 || i as i32 == k.path_seg as i32 || inside) {
            continue;
        }
        let n = s.count as i32;
        // The game measures point j - 1 and records index j (0x800900F0:
        // the point pointer starts at the segment's first point, the
        // counter at 1).
        let mut j = 1;
        while j < n - 1 {
            let p = &ter.path_points[(s.first as i32 + j - 1) as usize];
            let dx = (p.x >> 4) - (k.x >> 9);
            let dy = (p.y >> 4) - (k.y >> 9);
            let dz = (p.z >> 4) - (k.z >> 9);
            let d = dx.wrapping_mul(dx).wrapping_add(dy.wrapping_mul(dy)).wrapping_add(dz.wrapping_mul(dz)) as u32;
            if d < best {
                best = d;
                best_seg = i as i32;
                best_pt = j;
            }
            j += 1;
        }
    }
    k.path_seg = best_seg as i16;
    k.path_pt = best_pt as i16;
    if best_seg == -1 || best_pt == 0 {
        return;
    }
    let s = &ter.path_segments[best_seg as usize];
    let n = s.count as i32;
    if best_pt == n - 1 {
        return;
    }
    let idx = (s.first as i32 + best_pt) as usize;
    let pv = &ter.path_points[idx];
    let u = (-(pv.heading as i16 as i32)) & 0xFFF;
    let lat = ((k.x >> 5) - pv.x).wrapping_mul(t.cos_raw(u)).wrapping_add(((k.z >> 5) - pv.z).wrapping_mul(t.sin_raw(u)));
    k.path_lateral = div_pow2(lat, 14);
    k.path_width_l = pv.w[0] as i32 * -8;
    k.path_width_r = (pv.w[1] as i32) << 3;
    let dh = if best_pt < n - 2 {
        ter.path_points[idx + 1].heading as i16 as i32 - pv.heading as i16 as i32
    } else if best_pt < 2 {
        0
    } else {
        pv.heading as i16 as i32 - ter.path_points[idx - 1].heading as i16 as i32
    };
    let dh = wrap_angle(dh);
    let lat = k.path_lateral;
    k.path_side = if dh < 0 {
        (lat <= 0) as u8
    } else if lat < 0 {
        if dh < 1 { 1 } else { 0 }
    } else {
        1
    };
    if 0 < best_pt && best_pt < n - 1 {
        let d = s16(pv.heading as i32 - k.heading as u16 as i32);
        k.wobble = wrap_angle(d) as i16;
    }
}

//! The stunt courses' trial (race type 3, game mode 3: "1 PLAYER STUNT
//! TRIAL", the game's only timed solo mode; docs/spec/stunt-trial.md):
//! one kart against the clock through the course's gates (DAT chunk 0x04,
//! FUN_80081DD8), its place along the course's strip of quads (chunks 0x0F
//! / 0x10: FUN_8006F410 at the load, FUN_8006F64C every tick, FUN_8006FB2C
//! for the camera and the progress bar), the pipes that swallow the kart
//! and blow it out further on (chunk 0x14, FUN_800628FC) and the mirror
//! planes (chunks 0x08 / 0x17, FUN_80062E38).

use crate::fixed::{div_pow2, substeps};
use crate::globals::RaceGlobals;
use crate::kart::KartState;
use crate::tables::{ratan2, square_root0};
use crate::world::World;

/// FUN_8003D208: 1 when (px, pz) is on the non-negative side of the line
/// (ax, az) -> (bx, bz).
fn side(px: i32, pz: i32, ax: i32, az: i32, bx: i32, bz: i32) -> bool {
    let v = (px.wrapping_sub(ax)).wrapping_mul(bz.wrapping_sub(az)).wrapping_add((pz.wrapping_sub(az)).wrapping_mul(ax.wrapping_sub(bx)));
    v >= 0
}

/// The strip's two edges when the game uses them (both chunks present
/// with the same count: 0x800D1C84 / 0x800D1C88).
pub fn strip(w: &World) -> Option<(&[[i32; 3]], &[[i32; 3]])> {
    match &w.course.strip {
        Some((a, b)) if a.len() == b.len() => Some((a, b)),
        _ => None,
    }
}

/// Quad `i` of the strip (A[i], B[i], B[i+1], A[i+1]) holds (px, pz)
/// (x / z >> 4 of the points).
fn in_quad(a: &[[i32; 3]], b: &[[i32; 3]], i: usize, px: i32, pz: i32) -> bool {
    let p = |q: &[i32; 3]| (q[0] >> 4, q[2] >> 4);
    let (a0, b0, b1, a1) = (p(&a[i]), p(&b[i]), p(&b[i + 1]), p(&a[i + 1]));
    side(px, pz, a0.0, a0.1, b0.0, b0.1) && side(px, pz, b0.0, b0.1, b1.0, b1.1) && side(px, pz, b1.0, b1.1, a1.0, a1.1) && side(px, pz, a1.0, a1.1, a0.0, a0.1)
}

/// FUN_8006F1FC (course load): 0x800B0CC8 = the strip's quad count, or 0
/// when a quad's first corner turns the wrong way.
pub fn strip_quads(w: &World) -> i16 {
    let Some((a, b)) = strip(w) else { return 0 };
    let mut n = (a.len() as i32 - 1) as i16;
    let mut i = 0i32;
    while i < n as i32 {
        let (p, q, r) = (a[i as usize], b[i as usize], b[i as usize + 1]);
        if (q[2].wrapping_sub(p[2])).wrapping_mul(r[0].wrapping_sub(q[0])).wrapping_sub((p[0].wrapping_sub(q[0])).wrapping_mul(q[2].wrapping_sub(r[2]))) < 1 {
            n = 0;
        }
        i += 1;
    }
    n
}

/// FUN_8006F410(x >> 5, z >> 5): the first quad holding the point (0 when
/// none).
pub fn strip_find(w: &World, x5: i32, z5: i32) -> i16 {
    let Some((a, b)) = strip(w) else { return 0 };
    let (px, pz) = (x5 >> 4, z5 >> 4);
    let n = a.len() as i32 - 2;
    let mut i = 0;
    while i <= n {
        if in_quad(a, b, i as usize, px, pz) {
            return i as i16;
        }
        i += 1;
    }
    0
}

/// FUN_8006F64C (the update, race type 3): keep +0x490 on the quad under
/// the kart, searching up to 5 quads back and forth (back first).
pub fn strip_locate(k: &mut KartState, w: &World) {
    let Some((a, b)) = strip(w) else { return };
    let (px, pz) = (k.x >> 9, k.z >> 9);
    let i = k.strip as i32;
    let ok = |j: i32| j >= 0 && (j as usize + 1) < a.len() && in_quad(a, b, j as usize, px, pz);
    if ok(i) {
        return;
    }
    for d in 1..6 {
        if i - d >= 0 && ok(i - d) {
            k.strip = (i - d) as i16;
            return;
        }
        if i + d <= a.len() as i32 - 2 && ok(i + d) {
            k.strip = (i + d) as i16;
            return;
        }
    }
}

/// FUN_8006FB2C(x >> 5, z >> 5, quad): the kart's projection across the
/// strip at quad `i`: the points on edge A and edge B (world units) and
/// the fraction along the quad, 0..0x80 (0x800B0AE8).
pub fn strip_project(w: &World, x5: i32, z5: i32, i: i16) -> Option<([i32; 3], [i32; 3], i16)> {
    let (a, b) = strip(w)?;
    let t = &w.tables;
    let i = i as usize;
    let along = |p: &[i32; 3], q: &[i32; 3]| -> i32 {
        let ang = ratan2(t, q[0].wrapping_sub(p[0]), q[2].wrapping_sub(p[2]));
        let n = (-ang) & 0xFFF;
        ((x5.wrapping_sub(p[0])).wrapping_mul(t.cos(n)).wrapping_add((z5.wrapping_sub(p[2])).wrapping_mul(t.sin(n)))) / 0xFFF
    };
    let sh = |p: &[i32; 3]| [p[0] << 5, p[1] << 5, p[2] << 5];
    let d0 = along(&a[i], &b[i]);
    if d0 < 1 {
        return Some((sh(&a[i]), sh(&b[i]), 0));
    }
    let d1 = along(&a[i + 1], &b[i + 1]);
    if d1 >= 0 {
        return Some((sh(&a[i + 1]), sh(&b[i + 1]), 0x80));
    }
    let den = d0 - d1;
    let lerp = |p: &[i32; 3], q: &[i32; 3]| -> [i32; 3] { [0, 1, 2].map(|c| p[c] * 0x20 + ((q[c] - p[c]) * d0 * 0x20) / den) };
    Some((lerp(&a[i], &a[i + 1]), lerp(&b[i], &b[i + 1]), ((d0 << 7) / den) as i16))
}

/// FUN_80081DD8 (the lap timing of race type 3, before the clock): the
/// kart crossing its next gate (points 2g, 2g + 1 of chunk 0x04; mirrored
/// order with race flag 4) between the previous position (+0x2E8 / +0x2F0)
/// and this one counts it; the last gate finishes the trial: the time
/// rounded to a multiple of 3, the finishing place, the course record
/// (`g.lap_record`: the save block's entry for this course, mr_sim::save),
/// the AI takes the kart.
pub fn gates(k: &mut KartState, g: &mut RaceGlobals, w: &World) {
    if g.challenge != 0 {
        return;
    }
    let Some(pts) = &w.course.gates else { return };
    let n = pts.len() as i32;
    if n & 1 != 0 || n < 4 {
        return;
    }
    let reverse = g.race_flags & 4 != 0;
    let first = if reverse { n - (k.gate as i32 * 2 + 2) } else { k.gate as i32 * 2 };
    if first < 0 || first + 1 >= n {
        return;
    }
    let (p, q) = (pts[first as usize], pts[first as usize + 1]);
    let (px, pz, qx, qz) = (p[0] >> 4, p[2] >> 4, q[0] >> 4, q[2] >> 4);
    let (ox, oz) = (k.prev_x >> 9, k.prev_z >> 9);
    let (cx, cz) = (k.x >> 9, k.z >> 9);
    let crossed = if reverse {
        side(ox, oz, qx, qz, px, pz) && !side(cx, cz, qx, qz, px, pz) && side(cx, cz, ox, oz, qx, qz) && side(cx, cz, px, pz, ox, oz)
    } else {
        g.race_flags == 0 && side(ox, oz, px, pz, qx, qz) && !side(cx, cz, px, pz, qx, qz) && side(cx, cz, ox, oz, px, pz) && side(cx, cz, qx, qz, ox, oz)
    };
    if !crossed {
        return;
    }
    k.gate = k.gate.wrapping_add(1);
    if k.gate as i32 != n >> 1 {
        return;
    }
    let t = k.race_time;
    k.race_time = (t / 3) * 3;
    if 1 < t % 3 {
        k.race_time += 3;
    }
    k.race_pos = g.end_mode;
    k.finished = 2;
    k.shown_pos = k.race_pos;
    if (k.player_index as i32) < g.humans as i32 {
        // FUN_80060580(character, 3 if first else 2): the finish voice
        // (audio queue, not modelled).
    } else {
        let b = crate::update::rng_next(g, w) as i8;
        let bits = if k.race_pos == 1 { if b < 0 { 4 } else { 2 } } else if b < 0 { 0x20 } else { 0x10 };
        k.event_bits |= bits;
    }
    // 0x800B0C20 (the first finisher's time) is not modelled (results).
    let quads = strip_quads(w);
    if quads != 0 {
        k.progress = (strip(w).map_or(0, |(a, _)| a.len() as i32) + (g.humans as i32 - g.end_mode as i32) + 1) * 0x80;
    }
    if g.race_flags == 0 && k.flags & 1 == 0 && k.race_time < g.lap_record {
        // The course's record (0x800E0190 + 0x20 * world + 8 * slot) and
        // its holder: applied outside the simulation from `lap_record`
        // (mr_sim::save::SaveBlock::lap_record, mr-bevy save.rs), as a
        // race's lap record.
        g.lap_record = k.race_time;
    }
    g.end_mode = g.end_mode.wrapping_add(1);
    // 0x8008209C: no split screen, no race flags: the course's "won" bit
    // (flags 2) and sound 0x24 when new: from the finished kart, outside
    // the simulation (SaveBlock::race_won).
    k.pause_bits = 0x80;
    k.flags |= 1;
}

/// FUN_80082708 for race type 3 (after the start, until the kart is out):
/// the gates, then the clock (dt * 5 a frame, 300 a second; at most
/// 180000).
pub fn timing(k: &mut KartState, g: &mut RaceGlobals, w: &World) {
    if k.finished != 0 {
        return;
    }
    gates(k, g, w);
    k.race_time = k.race_time.wrapping_add(g.dt * 5);
    if k.race_time >= 0x2BF21 {
        k.race_time = 180_000;
    }
}

/// FUN_800837F8 for race type 3 (the race progress round robin's kart,
/// once the clock runs and the strip is valid): +0x388 = quad * 0x80 +
/// the fraction along it (+0x4A8, the camera's); mirrored with race flag
/// 4. Single kart: no ranking pass.
pub fn progress(k: &mut KartState, g: &RaceGlobals, w: &World) {
    if strip_quads(w) == 0 || g.race_clock == 0 || k.finished != 0 {
        return;
    }
    let p = (k.strip as i32) * 0x80 + k.strip_along as i32;
    k.progress = if g.race_flags & 4 != 0 { (strip(w).map_or(0, |(a, _)| a.len() as i32) - 1) * 0x80 - p } else { p };
}

/// FUN_800628FC (before the ground contact, kart not on a hook): touching
/// a pipe's mouth (point 0 of a group of 5, within 12 units) swallows the
/// kart: +0x4E7 = pipe, +0x4B6 = clock. Over 180 frames it shrinks at the
/// mouth (60), runs along points 0..4 (60), and grows at point 4 on the
/// ground below it (60), its position eased 1/16 a VBlank toward each
/// target and its velocity held at 0.
pub fn pipes(k: &mut KartState, g: &mut RaceGlobals, w: &World) {
    if k.effect != -1 {
        return;
    }
    let Some(pts) = &w.course.pipes else { return };
    let n = pts.len();
    if n % 5 != 0 || n < 5 {
        return;
    }
    if k.item_e7 == -1 {
        let (x, y, z) = (k.x >> 10, k.y >> 10, k.z >> 10);
        let hit = (0..n / 5).find(|&i| {
            let p = pts[5 * i];
            let (dx, dy, dz) = ((p[0] >> 5) - x, (p[1] >> 5) - y, (p[2] >> 5) - z);
            dx.wrapping_mul(dx).wrapping_add(dy.wrapping_mul(dy)).wrapping_add(dz.wrapping_mul(dz)) < 0x90
        });
        let Some(i) = hit else { return };
        k.item_e7 = i as i8;
        k.pipe_clock = 0;
        k.event_bits |= 8;
    }
    let p = &pts[5 * k.item_e7 as u8 as usize..];
    let t = k.pipe_clock as i32;
    let target;
    let mut scale: Option<i32> = None;
    if t < 0x3C {
        target = [p[0][0] << 5, p[0][1] << 5, p[0][2] << 5];
        scale = Some((t - 0x3C).abs());
    } else if t - 0x3C < 0x3C {
        let s = t - 0x3C;
        let j = ((s * 4) / 0x3C) as usize;
        let f = (s % 0xF) * 0x20;
        let (a, b) = (p[j], p[j + 1]);
        k.engine = 0;
        target = [0, 1, 2].map(|c| a[c] * 0x20 + ((b[c] - a[c]) * f) / 0xF);
        k.heading = (ratan2(&w.tables, p[4][0] - p[0][0], p[4][2] - p[0][2]) & 0xFFF) as i16;
        k.tilt_nx = 0;
        k.tilt_ny = -0x1000;
        k.tilt_nz = 0;
        k.pitch = 0;
        k.roll = 0;
    } else if t - 0x78 > 0x3B {
        k.item_e7 = -1;
        k.engine = 0x1000;
        target = [0; 3];
    } else {
        let q = [p[4][0] << 5, p[4][1] << 5, p[4][2] << 5];
        let y = w.ter.ground(q[0], q[1], q[2]).nearest_y;
        target = [q[0], y, q[2]];
        scale = Some(((t + 0xFF88) << 16) >> 16);
    }
    if let Some(s) = scale {
        let s = if s < 0 { 0 } else if s > 0x3C { 0x3C } else { s };
        k.engine = (((s << 16) >> 4) / 0x3C) as i16;
    }
    if k.pipe_clock < 0xB4 {
        for _ in 0..substeps(g.dt) {
            k.x = k.x.wrapping_add(div_pow2(target[0].wrapping_sub(k.x), 4));
            k.y = k.y.wrapping_add(div_pow2(target[1].wrapping_sub(k.y), 4));
            k.z = k.z.wrapping_add(div_pow2(target[2].wrapping_sub(k.z), 4));
        }
        k.vx = 0;
        k.vy = 0;
        k.vz = 0;
        let c = (k.pipe_clock as i32 + g.dt) as u8;
        k.pipe_clock = if c > 0xB4 { 0xB4 } else { c };
    }
}

/// FUN_80062E38 (after the ground contact): the mirror plane the kart is
/// reflected in (render side, FUN_8002976C): +0x4F8 = plane, +0x4C2 =
/// its kind (0: a z plane, 2: an x plane, chunk 0x08 walls; 1: a floor,
/// chunk 0x17 triangles), +0x39C = its coordinate (world units).
pub fn mirrors(k: &mut KartState, w: &World) {
    k.object_f8 = -1;
    let walls = &w.course.mirror_walls;
    if let Some(l) = walls {
        let n = l.len();
        if n % 3 == 0 && n / 3 != 0 {
            let mut best = i32::MAX;
            let (x5, z5, y5) = (k.x >> 5, k.z >> 5, k.ref_y >> 5);
            for i in 0..n / 3 {
                let (p0, p1, p2) = (l[3 * i], l[3 * i + 1], l[3 * i + 2]);
                if !(y5 <= (p0[1] + p1[1]) / 2 && p2[1] <= y5) {
                    continue;
                }
                let dx = (p1[0] - p0[0]).abs();
                let dz = (p1[2] - p0[2]).abs();
                if dx < dz {
                    let d = (x5 - p0[0]).abs();
                    if d < 0x1001 && d <= best {
                        let (lo, hi) = if p0[2] < p1[2] { (p0[2], p1[2]) } else { (p1[2], p0[2]) };
                        if lo <= z5 && z5 <= hi {
                            k.object_f8 = i as i8;
                            k.mirror_kind = 2;
                            k.mirror_at = p0[0] << 5;
                            best = d;
                        }
                    }
                } else {
                    let d = (z5 - p0[2]).abs();
                    if d < 0x1001 && d <= best {
                        let (lo, hi) = if p0[0] < p1[0] { (p0[0], p1[0]) } else { (p1[0], p0[0]) };
                        if lo <= x5 && x5 <= hi {
                            k.object_f8 = i as i8;
                            k.mirror_kind = 0;
                            k.mirror_at = p0[2] << 5;
                            best = d;
                        }
                    }
                }
            }
            if k.object_f8 != -1 {
                return;
            }
        }
    }
    if let Some(l) = &w.course.mirror_floors {
        let n = l.len();
        if n % 3 == 0 && n / 3 != 0 {
            let mut best = i32::MAX;
            let (x, z) = (k.x >> 10, k.z >> 10);
            for i in 0..n / 3 {
                let (p0, p1, p2) = (l[3 * i], l[3 * i + 1], l[3 * i + 2]);
                let y = (p0[1] + p1[1] + p2[1]) / 3;
                let d = (y - (k.y >> 5)).abs();
                let s = |a: [i32; 3], b: [i32; 3]| side(x, z, a[0] >> 5, a[2] >> 5, b[0] >> 5, b[2] >> 5);
                if d < 0x1001 && d <= best && s(p0, p1) && s(p1, p2) && s(p2, p0) {
                    k.mirror_at = y * 0x20;
                    k.object_f8 = i as i8;
                    k.mirror_kind = 1;
                    best = d;
                }
            }
        }
    }
}

/// The trial's start (FUN_800609F4 @0x80061...: +0x490 from FUN_8006F410
/// at the kart's place, +0x4A9 = 0x40).
pub fn init_kart(k: &mut KartState, w: &World) {
    k.strip = strip_find(w, k.x >> 5, k.z >> 5);
    k.strip_across = 0x40;
}

/// The camera's distance constants (exe data 0x800B078A / 0x800B078C).
pub fn camera_consts(w: &World) -> (i32, i32) {
    (w.exe.i16(0x800B_078A) as i32, w.exe.i16(0x800B_078C) as i32)
}

/// SquareRoot0 of a 2-D squared distance (>> 5 units).
pub fn dist2(w: &World, dx: i32, dz: i32) -> i32 {
    square_root0(&w.tables, dx.wrapping_mul(dx).wrapping_add(dz.wrapping_mul(dz)))
}

/// FUN_800702C4(phase): the points of the strip's two edges at `phase`
/// (0..0x7FFF over the whole strip; 64 steps a quad, halfway weights).
pub fn strip_point(w: &World, phase: i32) -> Option<([i32; 3], [i32; 3])> {
    let (a, b) = strip(w)?;
    let mut v = (phase & 0x7FFF).wrapping_mul(a.len() as i32).wrapping_mul(0x40);
    if v < 0 {
        v += 0x7FFF;
    }
    let u = v >> 15;
    let q = (if u < 0 { u + 0x3F } else { u } >> 6) as usize;
    let f = u & 0x3F;
    if q + 1 >= a.len() {
        return None;
    }
    let lerp = |p: &[[i32; 3]]| [0, 1, 2].map(|c| p[q][c] * 0x20 + ((p[q + 1][c] - p[q][c]).wrapping_mul(f) >> 1));
    Some((lerp(a), lerp(b)))
}

/// FUN_80070084(x, z) (TER units): a point's place along the strip, quad *
/// 0x80 + the fraction along it (FUN_8006FB2C), or -1 off the strip.
pub fn strip_progress(w: &World, x: i32, z: i32) -> i32 {
    let n = strip_quads(w);
    let Some((a, b)) = strip(w) else { return -1 };
    for i in 0..n.max(0) as usize {
        if in_quad(a, b, i, x >> 4, z >> 4) {
            return i as i32 * 0x80 + strip_project(w, x, z, i as i16).map_or(0, |p| p.2 as i32);
        }
    }
    -1
}

/// FUN_80034CE0 (course load): the start / finish banners (chunk 0x09,
/// point pairs, at most 2) and their places on the strip (0x800B0C90,
/// count 0x800B0E94): the progress bar's start and chequered marks.
pub fn banner_marks(w: &World) -> Vec<i32> {
    let Some(p) = &w.course.banners else { return Vec::new() };
    let n = p.len();
    if n & 1 != 0 || n >> 1 >= 3 {
        return Vec::new();
    }
    (0..n / 2).map(|i| strip_progress(w, (p[2 * i][0] + p[2 * i + 1][0]) / 2, (p[2 * i][2] + p[2 * i + 1][2]) / 2)).collect()
}

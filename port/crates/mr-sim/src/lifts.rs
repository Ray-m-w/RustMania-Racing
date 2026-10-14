//! Lifts (Session 6, track P): DAT chunk 0x18 (0x800D1CA8) records of four
//! points, set up by FUN_80063554 (course load, 0x800B0E01 of them at
//! 0x800DD708, 20 bytes each) and moved by FUN_800859EC (every race frame,
//! from FUN_8007A9DC) through the course's animated objects (0x800D8630,
//! 0x1C bytes each, 0x800B0F2E of them, set up by FUN_80084490's per-course
//! switch). An object with a lift (+0x17) steps it and moves its TER groups
//! (+0x14, +0x19 of them) by the lift's change: FUN_80042374 adds it to the
//! groups' vertex y and wall heights, so the karts drive on the moving
//! floor (BOG, ISLAND, HAPPY, DISCO; ROCK's lifts move scenery only).
//!
//! A lift waits at its low end (|P2 - P1| * 60 / 50 VBlanks), rises 0x39 a
//! frame to P1.y, waits there (|P3 - P2| * 60 / 50), and comes back down.

use crate::globals::RaceGlobals;
use crate::tables::{square_root0, Tables};
use crate::world::World;

pub const CHUNK: i16 = 0x18;

/// One lift (0x800DD708 + 20 * i).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Lift {
    /// +0: current y (DAT units); +4 / +8: the two ends (P0.y, P1.y).
    pub cur: i32,
    pub min: i32,
    pub max: i32,
    /// +0xC / +0xE: waits at each end (VBlanks); +0x10: wait left, -1 =
    /// moving.
    pub wait_min: i16,
    pub wait_max: i16,
    pub timer: i16,
    /// +0x12: step per frame (0x39 / -0x39 = 199, 0 = waiting).
    pub speed: i8,
    /// +0x13: animation counter (render side; course 0x17's texture).
    pub anim: u8,
}

/// The lift / terrain part of one animated object (0x800D8630 + 0x1C * i).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LiftObject {
    /// +0x17: lift index (-1 none).
    pub lift: i8,
    /// +0x14: first TER group (-1 none), +0x19: how many.
    pub group: i16,
    pub groups: u8,
    /// +0x12: first DAT special object slot (-1 none), +0x18: how many
    /// (the renderer moves them by the lift too, FUN_8002E060).
    pub slot: i16,
    pub slots: u8,
}

/// FUN_80084490's switch (jump table 0x80023378, course - 3), the objects
/// of the race courses with a lift field, in order (the other objects of
/// those courses have none). DOCK's object names lift 0, which DOCK lacks.
pub fn objects(course: u8) -> Vec<LiftObject> {
    let o = |lift: i8, slot: i16, slots: u8, group: i16, groups: u8| LiftObject { lift, group, groups, slot, slots };
    match course {
        // 0x80084BE4
        3 => vec![o(0, 1, 1, 0, 1)],
        // 0x800850B4
        6 => vec![o(0, 1, 4, 0, 2)],
        // 0x80085110
        8 => vec![o(0, 1, 3, 0, 1), o(1, 4, 3, 1, 1)],
        // 0x80085068
        9 => vec![o(0, 0x1F, 1, 0, 1)],
        // 0x80084A8C
        0x1C => vec![o(0, 1, 1, -1, 0), o(1, 2, 1, -1, 0), o(2, 3, 1, -1, 0)],
        // 0x80085394: twelve spinning objects, then the lifts.
        0x1F => {
            let mut v = vec![o(-1, -1, 0, -1, 0); 12];
            v.push(o(0, 0xD, 2, 0, 1));
            v.push(o(1, 0xF, 2, 1, 1));
            for i in 0..5 {
                v.push(o(i + 2, 0x11 + i as i16, 1, i as i16 + 2, 1));
            }
            v
        }
        _ => Vec::new(),
    }
}

fn dist(t: &Tables, a: [i32; 3], b: [i32; 3]) -> i32 {
    let d = |i: usize| b[i].wrapping_sub(a[i]);
    square_root0(t, d(0).wrapping_mul(d(0)).wrapping_add(d(1).wrapping_mul(d(1))).wrapping_add(d(2).wrapping_mul(d(2))))
}

/// FUN_80063554: the lifts from chunk 0x18 ((u16)count & 3 == 0, count > 3;
/// count / 4 lifts, at most 16).
pub fn load(w: &World, dat: &mr_assets::dat::Dat, bytes: &[u8]) -> Vec<Lift> {
    let le32 = |o: usize| i32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
    let Some(c) = dat.chunks.iter().find(|c| c.id == CHUNK) else { return Vec::new() };
    let n = c.count as u16;
    if n & 3 != 0 || n <= 3 {
        return Vec::new();
    }
    let t = &w.tables;
    (0..((n >> 2) as usize).min(16))
        .map(|i| {
            let b = c.offset + 4 + 48 * i;
            let p = |j: usize| [le32(b + 12 * j), le32(b + 12 * j + 4), le32(b + 12 * j + 8)];
            let wait_min = ((dist(t, p(1), p(2)) * 0x3C) / 0x32) as i16;
            let wait_max = ((dist(t, p(2), p(3)) * 0x3C) / 0x32) as i16;
            Lift { cur: p(0)[1], min: p(0)[1], max: p(1)[1], wait_min, wait_max, timer: wait_min, speed: 0, anim: 0 }
        })
        .collect()
}

/// FUN_800859EC's lift loop: every object with a valid lift steps it and
/// moves its TER groups by the change (recorded in `offsets`, per object;
/// the caller applies them to the TER with `sync`).
pub fn frame(g: &mut RaceGlobals, objs: &[LiftObject]) {
    let dt = g.dt;
    let st = &mut g.objects;
    for (oi, o) in objs.iter().enumerate() {
        if o.lift < 0 || o.lift as usize >= st.lifts.len() {
            continue;
        }
        let l = &mut st.lifts[o.lift as usize];
        let old = l.cur;
        let mut moving = l.timer == -1;
        if !moving {
            let t = l.timer as i32 - dt;
            l.timer = t as i16;
            if (t << 16) < 1 {
                l.timer = -1;
                l.speed = if l.cur == l.min { 0x39 } else { -0x39 };
                moving = true;
            }
        }
        if moving {
            let y = l.cur + l.speed as i32;
            l.cur = y;
            if y < l.max {
                if l.min >= y {
                    l.cur = l.min;
                    l.speed = 0;
                    l.timer = l.wait_min;
                }
            } else {
                l.cur = l.max;
                l.speed = 0;
                l.timer = l.wait_max;
            }
        }
        if l.speed > 0 {
            l.anim = l.anim.wrapping_add(dt as u8);
        } else if l.speed < 0 {
            l.anim = l.anim.wrapping_sub(dt as u8);
        }
        let d = l.cur - old;
        if d != 0 && o.group != -1 && o.groups != 0 {
            if st.lift_offsets.len() < objs.len() {
                st.lift_offsets.resize(objs.len(), 0);
            }
            st.lift_offsets[oi] = st.lift_offsets[oi].wrapping_add(d);
        }
    }
}

/// Apply the objects' accumulated lift offsets to the TER groups (what the
/// game's FUN_80042374 calls have done to the course in memory): each
/// group's wanted offset is the sum over the objects that move it; the
/// TER is moved by the difference from what it has.
pub fn sync(ter: &mut mr_assets::ter::Ter, applied: &mut Vec<i32>, objs: &[LiftObject], offsets: &[i32]) {
    let n = ter.groups.len();
    if applied.len() < n {
        applied.resize(n, 0);
    }
    let mut want = vec![0i32; n];
    for (o, &d) in objs.iter().zip(offsets.iter()) {
        if o.group < 0 {
            continue;
        }
        for gi in o.group as usize..o.group as usize + o.groups as usize {
            if gi < n {
                want[gi] = want[gi].wrapping_add(d);
            }
        }
    }
    for gi in 0..n {
        let d = want[gi].wrapping_sub(applied[gi]);
        if d != 0 {
            // FUN_80042374 takes a short; a lift moves well under 0x8000.
            ter.move_group_y(gi, d as i16);
            applied[gi] = want[gi];
        }
    }
}

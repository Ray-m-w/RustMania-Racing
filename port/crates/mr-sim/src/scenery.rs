//! The course's animated scenery objects (Session 6, track R): 0x800D8630,
//! 0x1C bytes each, 0x800B0F2E of them (at most 32). FUN_80084490 (course
//! load, after the turntables) fills them per course (jump table 0x80023378,
//! course - 3); FUN_800859EC (every race frame, the end of FUN_8007A9DC)
//! spins them and hands the angles to their DAT special slots
//! (FUN_8002D9E4), steps the lifts (lifts.rs) and moves the slots and TER
//! groups by the lift's change (FUN_8002E060 / FUN_80042374).
//!
//! The spin is render state (no collision reads it); it lives here because
//! it is driven by the race clock and the jets' state, and its course-load
//! draws are the simulation's random streams.

use crate::globals::RaceGlobals;

/// RAM: the records and their count (i16).
pub const ADDR: u32 = 0x800D_8630;
pub const COUNT: u32 = 0x800B_0F2E;
pub const SIZE: u32 = 0x1C;

/// One record of 0x800D8630.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnimObject {
    /// +0x00 / +0x02 / +0x04: angles x, y, z (12-bit after each frame).
    pub rot: [u16; 3],
    /// +0x06 / +0x0A / +0x0E: spin per VBlank at full speed.
    pub base_vel: [i16; 3],
    /// +0x08 / +0x0C / +0x10: spin now (base * fade / 64).
    pub vel: [i16; 3],
    /// +0x12: first DAT special slot (-1 none); +0x18: how many.
    pub slot: i16,
    pub slots: u8,
    /// +0x14: first TER group (-1 none); +0x19: how many.
    pub group: i16,
    pub groups: u8,
    /// +0x16: spin-up 0..0x40 (+2 a VBlank while its jet blows, -2 else).
    pub fade: i8,
    /// +0x17: lift (-1 none).
    pub lift: i8,
    /// +0x1A: the air jet that drives the spin (-1: always on).
    pub jet: i8,
}

impl Default for AnimObject {
    /// FUN_80084490's clear of all 32 records.
    fn default() -> Self {
        AnimObject { rot: [0; 3], base_vel: [0; 3], vel: [0; 3], slot: -1, slots: 0, group: -1, groups: 0, fade: 0, lift: -1, jet: -1 }
    }
}

impl AnimObject {
    pub fn read(b: &[u8]) -> AnimObject {
        let h = |o: usize| i16::from_le_bytes([b[o], b[o + 1]]);
        AnimObject {
            rot: [h(0) as u16, h(2) as u16, h(4) as u16],
            base_vel: [h(6), h(0xA), h(0xE)],
            vel: [h(8), h(0xC), h(0x10)],
            slot: h(0x12),
            group: h(0x14),
            fade: b[0x16] as i8,
            lift: b[0x17] as i8,
            slots: b[0x18],
            groups: b[0x19],
            jet: b[0x1A] as i8,
        }
    }
}

/// FUN_80084490's per-course switch for course `id` (race courses and the
/// others it names). `spin`: the turntables' spins (0x800BC64A + 4 i) as
/// drawn just before; `rand`: the BIOS rand() stream; `byte`: the random
/// byte stream (`*0x800B0BCC++`).
pub fn load(id: u8, spin: &[i16], rand: &mut dyn FnMut() -> i32, byte: &mut dyn FnMut() -> u8) -> Vec<AnimObject> {
    let tt = |i: usize| spin.get(i).copied().unwrap_or(0);
    // rand() / 8 (C division)
    let mut r8 = || {
        let r = rand();
        ((if r < 0 { r + 7 } else { r }) >> 3) as u16
    };
    let o = AnimObject::default;
    let mut v: Vec<AnimObject> = Vec::new();
    match id {
        // 0x80084BE4 / 0x800850B4 / 0x80085068: one object, the rest after
        // the switch (one record: group 0, lift 0).
        3 | 6 | 9 => {
            let (slot, slots, groups) = match id {
                3 => (1, 1, 1),
                6 => (1, 4, 2),
                _ => (0x1F, 1, 1),
            };
            v.push(AnimObject { slot, slots, groups, group: 0, lift: 0, ..o() });
        }
        // 0x80085110 (BOG)
        8 => {
            v.push(AnimObject { slot: 1, slots: 3, group: 0, groups: 1, lift: 0, ..o() });
            v.push(AnimObject { slot: 4, slots: 3, group: 1, groups: 1, lift: 1, ..o() });
        }
        // 0x800855FC (BASE): the turntable's disc, two fans on jets 1 / 0.
        0x0F => {
            v.push(AnimObject { rot: [0, r8(), 0], slots: 2, slot: 1, base_vel: [0, tt(0), 0], ..o() });
            let a = r8();
            v.push(AnimObject { slots: 1, jet: 1, rot: [0, a, 0], base_vel: [0, 0x60, 0], slot: 3, ..o() });
            let a = r8();
            v.push(AnimObject { rot: [a, 0, 0], base_vel: [0x60, 0, 0], slot: 4, slots: 1, jet: 0, ..o() });
        }
        // 0x80084B54 (CAPE): one spinner, direction from a random byte.
        0x12 => {
            let a = r8();
            let s = if (byte() as i8) < 0 { 5 } else { -5 };
            v.push(AnimObject { rot: [0, a, 0], base_vel: [0, s, 0], slot: 1, slots: 1, ..o() });
        }
        // 0x80084688 (OVAL, 0x1B): one spinner per turntable.
        0x1A | 0x1B => {
            for i in 0..6 {
                let a = r8();
                v.push(AnimObject { rot: [0, a, 0], slot: i as i16 + 1, slots: 1, base_vel: [0, tt(i), 0], ..o() });
            }
        }
        // 0x80084A8C (ROCK): three lifts moving scenery only.
        0x1C => {
            for i in 0..3 {
                v.push(AnimObject { slots: 1, lift: i as i8, slot: i as i16 + 1, ..o() });
            }
        }
        // 0x80085394 (DISCO): twelve spinners (speed and sign from two
        // random bytes each), the lifts, a turntable disc.
        0x1F => {
            for i in 0..12 {
                let a = r8();
                let b0 = byte() as i32;
                let b1 = byte() as i8;
                let mut s = (b0 / 0x1D + if i > 7 { 0x18 } else { 8 }) as i16;
                if b1 < 0 {
                    s = -s;
                }
                v.push(AnimObject { rot: [0, a, 0], slot: i + 1, base_vel: [0, s, 0], slots: 1, ..o() });
            }
            v.push(AnimObject { slots: 2, groups: 1, lift: 0, slot: 0xD, group: 0, ..o() });
            v.push(AnimObject { slot: 0xF, slots: 2, group: 1, groups: 1, lift: 1, ..o() });
            for i in 0..5i16 {
                v.push(AnimObject { slot: i + 0x11, slots: 1, group: i + 2, groups: 1, lift: i as i8 + 2, ..o() });
            }
            let a = r8();
            v.push(AnimObject { rot: [0, a, 0], slot: 0x16, slots: 1, base_vel: [0, tt(0), 0], ..o() });
        }
        _ => {}
    }
    // LAB_8008597C: spin-up 0, current spin = full spin.
    for a in v.iter_mut() {
        a.fade = 0;
        a.vel = a.base_vel;
    }
    v
}

/// rand() and random-byte draws of `load` for course `id`.
pub fn load_draws(id: u8) -> (u32, u32) {
    match id {
        0x0F => (3, 0),
        0x12 => (1, 1),
        0x1A | 0x1B => (6, 0),
        0x1F => (13, 24),
        _ => (0, 0),
    }
}

/// C division by 64 (toward zero).
fn div64(x: i32) -> i32 {
    (if x < 0 { x + 0x3F } else { x }) >> 6
}

/// FUN_800859EC's spin part (every race frame; `g.dt` VBlanks): the
/// turntables' phases (0x800BC648 += spin * dt) and every object's spin,
/// one VBlank at a time: the spin-up follows its jet (or is always on),
/// the spin is the full spin * spin-up / 64, the angles add it.
pub fn frame(g: &mut RaceGlobals) {
    let dt = g.dt;
    let st = &mut g.objects;
    for i in 0..st.turntable_phase.len().min(st.turntable_spin.len()) {
        st.turntable_phase[i] = ((st.turntable_phase[i] as i32 + st.turntable_spin[i] as i32 * dt) & 0xFFF) as i16;
    }
    for a in st.anim.iter_mut() {
        let mut n = dt;
        loop {
            let on = a.jet == -1 || st.jets.get(a.jet as u8 as usize).is_some_and(|j| j.active != 0);
            if on {
                let f = (a.fade as u8).wrapping_add(2) as i8;
                a.fade = if f > 0x40 { 0x40 } else { f };
            } else {
                let f = (a.fade as u8).wrapping_sub(2) as i8;
                a.fade = if f < 0 { 0 } else { f };
            }
            for k in 0..3 {
                a.vel[k] = div64(a.base_vel[k] as i32 * a.fade as i32) as i16;
                a.rot[k] = a.rot[k].wrapping_add(a.vel[k] as u16);
            }
            n -= 1;
            if (n << 16) <= 0 {
                break;
            }
        }
        for r in a.rot.iter_mut() {
            *r &= 0xFFF;
        }
    }
}

/// The records as the game holds them in memory `m`.
pub fn read(m: &crate::mem::Mem) -> Vec<AnimObject> {
    let n = m.i16(COUNT).clamp(0, 32) as u32;
    (0..n).map(|i| AnimObject::read(m.slice(ADDR + SIZE * i, SIZE as usize))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disco_spinners_take_two_bytes_each() {
        let mut k = 0;
        let mut rand = || {
            k += 8;
            k
        };
        let bytes = [0x3Au8, 0x80, 0x00, 0x01];
        let mut j = 0;
        let mut byte = || {
            let b = bytes[j % 4];
            j += 1;
            b
        };
        let v = load(0x1F, &[5], &mut rand, &mut byte);
        assert_eq!(v.len(), 20);
        assert_eq!((v[0].slot, v[0].rot[1], v[0].base_vel[1]), (1, 1, -(0x3A / 0x1D + 8)));
        assert_eq!(v[1].base_vel[1], 8);
        assert_eq!(v[8].base_vel[1], -(0x3A / 0x1D + 0x18));
        assert_eq!((v[12].slot, v[12].slots, v[12].lift, v[12].group), (0xD, 2, 0, 0));
        assert_eq!((v[19].slot, v[19].base_vel[1]), (0x16, 5));
        assert_eq!(j, 24);
    }

    #[test]
    fn spin_follows_the_jet() {
        let mut g = RaceGlobals::default();
        g.dt = 2;
        g.objects.jets = vec![crate::objects::Jet { active: 0, ..Default::default() }];
        g.objects.anim = vec![
            AnimObject { base_vel: [0x60, 0, 0], jet: 0, fade: 4, vel: [0x60, 0, 0], ..Default::default() },
            AnimObject { base_vel: [0, -5, 0], ..Default::default() },
        ];
        frame(&mut g);
        // fade 4 -> 2 -> 0: spin 3, then 0
        assert_eq!((g.objects.anim[0].fade, g.objects.anim[0].rot[0]), (0, 3));
        // fade 0 -> 2 -> 4: spin (-10 >> 6) = 0 both VBlanks
        assert_eq!((g.objects.anim[1].fade, g.objects.anim[1].rot[1]), (4, 0));
    }
}

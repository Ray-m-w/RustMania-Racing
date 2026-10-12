//! The vehicle template the game copies into a kart record at race load:
//! 0x174 bytes at 0x800D8FD0 + 0x174 * vehicle, built once by FUN_80087EB4
//! (defaults for all 48 vehicles, then per-vehicle overrides written with
//! immediate operands in its code) and FUN_80087B6C (wheel positions and
//! radii from the vehicle model's parts). FUN_800609F4 copies the template
//! to kart +0x14..+0x188.
//!
//! Only vehicle 0 (the RACER, Kermit's kart on DOCK) is in scope. Its values
//! are the immediates of FUN_80087EB4 (instruction addresses below; they
//! are code, not a data table), the stats from the vehicle table in the exe
//! (0x800ACDB0) and the part pivots of VEHIC_A/RACER.MDL. Verified against
//! the game's own FUN_80087EB4 run on a cleared table (tools/oracle/
//! mipsref.py) and the oracle's RAM (tests/standalone.rs).

use mr_assets::mdl::Mdl;

use crate::tables::Tables;

pub const TEMPLATE_SIZE: usize = 0x174;
/// The template lands at kart +0x14.
pub const KART_OFFSET: usize = 0x14;

/// (template offset, size, value, the instruction of FUN_80087EB4 that
/// stores it) for vehicle 0, after the default loop and the overrides.
const RACER_STORES: &[(usize, usize, i32, u32)] = &[
    // Wheel blocks (+0x2C each): +0x24 suspension rest, +0x26 flags
    // (2 = front, steered; 1 = rear, driven).
    (0x24, 2, 65, 0x8008_8130),
    (0x26, 2, 2, 0x8008_8110),
    (0x50, 2, 65, 0x8008_8138),
    (0x52, 2, 2, 0x8008_8118),
    (0x7C, 2, 66, 0x8008_8088),
    (0x7E, 2, 1, 0x8008_8120),
    (0xA8, 2, 66, 0x8008_8090),
    (0xAA, 2, 1, 0x8008_8128),
    // Render-side light / polygon parameters (kart +0x128..+0x163).
    (0x114, 2, -99, 0x8008_8050),
    (0x116, 2, -80, 0x8008_80E0),
    (0x118, 2, -298, 0x8008_80F4),
    (0x11A, 1, 0, 0x8008_7F34),
    (0x11B, 1, 32, 0x8008_7F38),
    (0x11F, 1, 50, 0x8008_7F40),
    (0x12C, 2, 99, 0x8008_805C),
    (0x12E, 2, -80, 0x8008_80E8),
    (0x130, 2, -298, 0x8008_80FC),
    (0x132, 1, 0, 0x8008_7F34),
    (0x133, 1, 32, 0x8008_7F38),
    (0x137, 1, 50, 0x8008_7F40),
    (0x14A, 1, 0, 0x8008_7F34),
    (0x14B, 1, 32, 0x8008_7F38),
    (0x14F, 1, 50, 0x8008_7F40),
    // Ground-probe corners (kart +0x164..+0x16A).
    (0x150, 2, -192, 0x8008_8160),
    (0x152, 2, 192, 0x8008_80B0),
    (0x154, 2, 282, 0x8008_809C),
    (0x156, 2, -229, 0x8008_80D4),
    // Shadow colours, +0x16E.
    (0x158, 1, 32, 0x8008_8150),
    (0x159, 1, 32, 0x8008_8158),
    (0x15A, 2, 23, 0x8008_8170),
    // Wheel count, engine audible, +0x172, engine class, chassis flags.
    (0x15C, 1, 4, 0x8008_8108),
    (0x15D, 1, 4, 0x8008_9B38),
    (0x15E, 1, 2, 0x8008_8140),
    (0x15F, 1, 16, 0x8008_80C4),
    (0x160, 1, 1, 0x8008_7F50),
    // Kart radius (+0x176).
    (0x162, 2, 254, 0x8008_9AB8),
    // Camera mount (+0x182, +0x184 = ride height, +0x186).
    (0x16E, 2, 0, 0x8008_7FEC),
    (0x170, 2, -145, 0x8008_8074),
    (0x172, 2, 5, 0x8008_8148),
];

/// FUN_80087EB4 + FUN_80087B6C for vehicle 0 (the RACER).
pub fn racer_template(t: &Tables, racer: &Mdl) -> [u8; TEMPLATE_SIZE] {
    let mut b = [0u8; TEMPLATE_SIZE];
    for &(off, size, v, _) in RACER_STORES {
        b[off..off + size].copy_from_slice(&v.to_le_bytes()[..size]);
    }
    // 0x80087F94..0x80087FD0: the five stat bytes of vehicle 0's row of the
    // exe table 0x800ACDB0, as halfwords at template +0x164..+0x16C.
    for i in 0..5 {
        let v = t.vehicle[0][i] as u16;
        b[0x164 + 2 * i..0x166 + 2 * i].copy_from_slice(&v.to_le_bytes());
    }
    // FUN_80087B6C via FUN_80060944: wheel i sits at model part i's pivot,
    // negated in x and z (0x80087C30..0x80087C78); radius = |y|, at least 0x20.
    let wheels = b[0x15C] as usize;
    for i in 0..wheels {
        let p = racer.parts[i].record.pivot;
        let base = 0x2C * i;
        let put = |b: &mut [u8; TEMPLATE_SIZE], o: usize, v: i16| b[base + o..base + o + 2].copy_from_slice(&v.to_le_bytes());
        put(&mut b, 0x18, (-p[0]) as i16);
        put(&mut b, 0x1A, p[1] as i16);
        put(&mut b, 0x1C, (-p[2]) as i16);
        let r = (p[1] as i16).wrapping_abs();
        put(&mut b, 0x22, if r < 0x20 { 0x20 } else { r });
    }
    b
}

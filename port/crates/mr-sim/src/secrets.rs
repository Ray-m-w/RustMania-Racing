//! Phase 6b: what COURSE SECRETS starts (docs/spec/adventure.md "COURSE
//! SECRETS"): the arenas' bosses (FUN_8008A9AC), the boss count of a BOSS
//! CHALLENGE (game mode 7, FUN_8008C4AC @0x8008D8xx) and the Doozers'
//! vehicle turn (0x800B0A14).

use crate::tables::ExeImage;

/// 0x800B0A14: the Doozer's vehicles in turn (BULLDOZE, DUMPER, ROLLER),
/// stepped by 0x800B0A10 (mod 3) for each Doozer placed (ADVENTURE's
/// parked Doozer, DOOZER BATTLE's karts) [RAM = the exe's data].
pub const DOOZER_VEHICLES: u32 = 0x800B_0A14;
/// The Doozer (character 24).
pub const DOOZER: u8 = 24;
/// DOGFIGHT (race flag 0x80): every kart on vehicle 8, the PLANE.
pub const PLANE: u8 = 8;
/// 0x800ACB3A + 0xC * course (u16): bits 0x80 / 0x100 / 0x200 count the
/// boss karts of the course's BOSS CHALLENGE (0x800B0C98).
pub const COURSE_BOSS_BITS: u32 = 0x800A_CB3A;

/// FUN_8008A9AC (course load): the course's bosses (0x800B0C7E.. character,
/// 0x800B0D2C.. vehicle; 0xFF none) and 0x800B0A61, how many BATTLE AS
/// BOSS offers (LEFT / RIGHT 0x800B07F3).
/// CANYON SWEETUMS / MAYHEM; T_SQUARE BEAUREGAARD / TAXI; THEATRE BILL,
/// GILL and JILL / SAFEARSE; CHAMBER STATLER and WALDORF / CLASSIC; LAGOON
/// THE CHIEF / LOG; LAB VAN NEUTER / DELOREAN.
pub fn bosses(course: u8) -> ([(u8, u8); 3], u8) {
    const N: (u8, u8) = (0xFF, 0xFF);
    match course {
        5 => ([(0x20, 0x13), N, N], 1),
        0xD => ([(0x25, 0x15), (0x26, 0x15), N], 2),
        0xE => ([(0x28, 0x17), N, N], 1),
        0x13 => ([(0x21, 0x14), N, N], 1),
        0x14 => ([(0x22, 0x18), (0x23, 0x18), (0x24, 0x18)], 3),
        0x15 => ([(0x27, 0x16), N, N], 1),
        _ => ([N; 3], 0),
    }
}

/// FUN_8008C4AC @0x8008D8xx (race type 2, 0x800B07E8 set): 0x800B0C98,
/// the boss karts: one per bit 0x80 / 0x100 / 0x200 of the course's word.
pub fn boss_count(exe: &ExeImage, course: u8) -> i16 {
    let b = exe.i16(COURSE_BOSS_BITS + 0xC * course as u32) as u16;
    [0x80u16, 0x100, 0x200].iter().filter(|&&m| b & m != 0).count() as i16
}

/// The Doozers' vehicle at turn `t` (0x800B0A14[t % 3]).
pub fn doozer_vehicle(exe: &ExeImage, t: u8) -> u8 {
    exe.u8(DOOZER_VEHICLES + (t % 3) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boss_tables() {
        assert_eq!(bosses(0x14).1, 3);
        assert_eq!(bosses(0xD).0[1], (0x26, 0x15));
        assert_eq!(bosses(3).1, 0);
    }
}

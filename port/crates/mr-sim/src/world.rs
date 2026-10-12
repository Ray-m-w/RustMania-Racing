//! What the kart update needs to know about the course and the race, beyond
//! the kart itself: constant tables, the terrain (mr_assets::ter), course
//! data from the .DAT file, and per-tick inputs produced by other parts of the
//! game (other karts, the water animation, ...).

use mr_assets::ter::Ter;

use crate::tables::{ExeImage, Tables};

/// The finish line quad from the course .DAT (pointer 0x800D1C54 in RAM):
/// three edges tested with FUN_8003D208 (which side of a line a point is).
/// Coordinates are the DAT's (world >> 5, read >> 4 by FUN_80082708).
#[derive(Clone, Debug, Default)]
pub struct FinishLine {
    /// The 13 words at +0x00..+0x30 of the record, as the game reads them.
    pub words: [i32; 13],
}

/// A proximity trigger object from the .DAT (list at 0x800D1C9C, count at
/// 0x800B0DC6, stride 36 bytes; position at +4/+8/+12). Touching one sets
/// kart +0x4E6 and enters the pickup code, which is out of scope here.
#[derive(Clone, Copy, Debug, Default)]
pub struct Trigger {
    pub pos: [i32; 3],
}

#[derive(Clone, Debug, Default)]
pub struct Course {
    pub finish: Option<FinishLine>,
    /// First halfword of the record at 0x800D1C5C (course length in progress
    /// units << 4), used by the catch-up (rubber band) speed.
    pub length: Option<u16>,
    pub triggers: Vec<Trigger>,
    /// Object lists the DOCK course does not have (0x800D1C68, 0x800D1C8C,
    /// 0x800D1C98, 0x800D1CA4, count byte 0x800B0C58, flag 0x800B0E00,
    /// byte 0x800B0C68). If any were present the corresponding code paths
    /// would be needed; they are `unreachable_in_scope!` instead.
    pub has_other_objects: bool,
}

/// Where the game keeps the TER sections in RAM. The kart record stores RAM
/// pointers to faces (+0x2E0, +0x2E4); to read and write records exactly the
/// harness needs these heap addresses (taken from the oracle's RAM; they are
/// not used by the simulation itself).
#[derive(Clone, Copy, Debug, Default)]
pub struct HeapLayout {
    /// 0x800B0FA4: address of face 0 (16 bytes per face).
    pub faces: u32,
}

/// Inputs from the rest of the race that the player kart's update reads but
/// that are produced elsewhere (other karts, the race-position code, the
/// water animation FUN_8003C5FC). The harness fills these from the oracle.
#[derive(Clone, Copy, Debug, Default)]
pub struct TickInputs {
    /// Race progress (+0x388) of the race leader (kart 0x800D8F94).
    pub leader_progress: i32,
    /// Water surface normal 0x800B0CA0..CA4 (animated by FUN_8003C5FC).
    pub water_normal: [i16; 3],
    /// Alternative normal 0x800B0EA8..EAC (FUN_8003C5FC).
    pub alt_normal: [i16; 3],
    /// The look-at target (+0x2C0..+0x2CC) as FUN_80041A20 / FUN_80041CC4
    /// leave it before the kart loop (render-side code, not ported); when
    /// set, applied after the contact pass. Only the body sway reads it.
    pub aim: Option<[i32; 4]>,
}

pub struct World {
    pub exe: ExeImage,
    pub tables: Tables,
    pub ter: Ter,
    pub course: Course,
    pub heap: HeapLayout,
}

impl World {
    /// Byte of the game's pseudo-random stream at `addr` (0x800B0BCC points
    /// into the executable's text, which is what the game reads).
    pub fn rng_byte(&self, addr: u32) -> u8 {
        self.exe.u8(addr)
    }

    pub fn face_ptr(&self, face: Option<usize>) -> u32 {
        match face {
            Some(f) => self.heap.faces + 16 * f as u32,
            None => 0,
        }
    }

    pub fn face_index(&self, ptr: u32) -> Option<usize> {
        if ptr == 0 {
            None
        } else {
            Some(((ptr - self.heap.faces) / 16) as usize)
        }
    }
}

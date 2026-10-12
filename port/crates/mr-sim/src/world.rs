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

/// A start position: the grid point and a second point the kart faces away
/// from (DAT chunk id 1 at 0x800D1C4C; FUN_800609F4).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GridSlot {
    pub pos: [i32; 3],
    pub behind: [i32; 3],
}

#[derive(Clone, Debug, Default)]
pub struct Course {
    /// Start grid (DAT chunk id 1), DAT units (world >> 5).
    pub grid: Vec<GridSlot>,
    /// Look-at points (DAT chunk id 7, 0x800D1C64), world units.
    pub look_points: Vec<[i32; 3]>,
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
    /// 0x800B0FA4: address of face 0 (16 bytes per face). The game's heap
    /// address when known (oracle harness); a standalone session uses
    /// `STANDALONE_FACES` so the pointer fields stay non-zero and decodable.
    pub faces: u32,
}

/// Stand-in face table address for a standalone session (not a real RAM
/// address; only the kart record's face pointer fields carry it).
pub const STANDALONE_FACES: u32 = 0x8019_87F0;

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

/// DAT chunk ids registered at 0x800D1C48 + 4 * id by the .DAT loader
/// FUN_8002E3DC (pointer to the chunk header: i16 count, i16 id, then
/// count points of three i32).
pub mod chunk {
    pub const GRID: i16 = 0x01;
    pub const FINISH: i16 = 0x03;
    pub const COURSE: i16 = 0x05;
    pub const LOOK_POINTS: i16 = 0x07;
    pub const TRIGGERS: i16 = 0x15;
    /// Lists read by FUN_80062E38 (0x800D1C68, 0x800D1CA4), FUN_800632BC
    /// (0x800D1C8C) and FUN_800628FC (0x800D1C98); absent on DOCK.
    pub const OTHER: [i16; 4] = [0x08, 0x11, 0x14, 0x17];
}

pub struct World {
    pub exe: ExeImage,
    pub tables: Tables,
    pub ter: Ter,
    pub course: Course,
    pub heap: HeapLayout,
    /// The driver model's data the render passes use (KERMIT.MDL).
    pub driver: crate::render::DriverModel,
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

#[derive(Debug)]
pub struct WorldError(pub String);

impl std::fmt::Display for WorldError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for WorldError {}

impl Course {
    /// The course records the kart code uses, from a parsed .DAT and its
    /// bytes (chunks by id, as FUN_8002E3DC registers them).
    pub fn from_dat(dat: &mr_assets::dat::Dat, bytes: &[u8]) -> Course {
        let le32 = |o: usize| i32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
        let find = |id: i16| dat.chunks.iter().find(|c| c.id == id);
        let grid = find(chunk::GRID)
            .map(|c| {
                (0..(c.count as usize) / 2)
                    .map(|i| {
                        let o = c.offset + 4 + 24 * i;
                        GridSlot { pos: [le32(o), le32(o + 4), le32(o + 8)], behind: [le32(o + 12), le32(o + 16), le32(o + 20)] }
                    })
                    .collect()
            })
            .unwrap_or_default();
        let look_points = find(chunk::LOOK_POINTS)
            .map(|c| (0..c.count as usize).map(|i| {
                let o = c.offset + 4 + 12 * i;
                [le32(o), le32(o + 4), le32(o + 8)]
            }).collect())
            .unwrap_or_default();
        let finish = find(chunk::FINISH).map(|c| {
            let mut words = [0i32; 13];
            for (i, w) in words.iter_mut().enumerate() {
                *w = le32(c.offset + 4 * i);
            }
            FinishLine { words }
        });
        let length = find(chunk::COURSE).map(|c| c.count as u16);
        // 36-byte trigger records (three points each); 0x800B0DC6 = count / 3.
        let triggers = find(chunk::TRIGGERS)
            .map(|c| {
                (0..(c.count as usize) / 3)
                    .map(|i| {
                        let o = c.offset + 36 * i;
                        Trigger { pos: [le32(o + 4), le32(o + 8), le32(o + 12)] }
                    })
                    .collect()
            })
            .unwrap_or_default();
        let has_other_objects = chunk::OTHER.iter().any(|&id| find(id).is_some());
        Course { grid, look_points, finish, length, triggers, has_other_objects }
    }
}

impl World {
    /// DOCK from the owner's disc alone: exe tables, DOCK.TER, DOCK.DAT.
    pub fn dock(disc: &mut mr_disc::Disc) -> Result<World, WorldError> {
        let e = |m: &str| WorldError(m.to_string());
        let exe = ExeImage::parse(&disc.read("SLUS_012.37").map_err(|x| e(&format!("read exe: {x:?}")))?).map_err(|x| e(&x.0))?;
        let ter = Ter::parse(&disc.read("COURSE_A/DOCK.TER").map_err(|x| e(&format!("read DOCK.TER: {x:?}")))?)
            .map_err(|x| e(&format!("parse DOCK.TER: {x:?}")))?;
        let dat_bytes = disc.read("COURSE_A/DOCK.DAT").map_err(|x| e(&format!("read DOCK.DAT: {x:?}")))?;
        let dat = mr_assets::dat::Dat::parse(&dat_bytes).map_err(|x| e(&format!("parse DOCK.DAT: {x:?}")))?;
        let kermit = mr_assets::mdl::Mdl::parse(&disc.read("CHARS_A/KERMIT.MDL").map_err(|x| e(&format!("read KERMIT.MDL: {x:?}")))?)
            .map_err(|x| e(&format!("parse KERMIT.MDL: {x:?}")))?;
        Ok(World {
            driver: crate::render::DriverModel { part0_pivot: kermit.parts[0].record.pivot },
            tables: Tables::from_exe(&exe),
            exe,
            ter,
            course: Course::from_dat(&dat, &dat_bytes),
            heap: HeapLayout { faces: STANDALONE_FACES },
        })
    }
}

//! The game's course table: which file set a course index (0x800B0FAC, the
//! "event") loads, which cup and slot it belongs to, what kind of race the
//! race loader FUN_8008C4AC sets up on it, and the per-course row the rest
//! of the game reads (music, fog colour, flags). Everything is read from the
//! executable on the owner's disc; nothing is pasted here.
//!
//! Sources (docs/spec/courses.md):
//! - 0x800ACAA0: course-name pointer table, 37 entries, indexed by course
//!   (FUN_8004627C: `<name>.raw/.dat/.ter/.axe`; "course_a\dock").
//! - 0x800AC1E4: cup table, 9 cups x 4 slots of course indices (i8, -1 =
//!   none). FUN_8008AB5C finds the course in it: 0x800B0E38 = cup,
//!   0x800B0E32 = slot, and the race type 0x800B0B9A: slot 3 of cups 0..5
//!   = 3 (stunt), slot 1 of cups 0..5 = 1 (race B), slot 2 of cups 0..5,
//!   cup 8, and courses 0x1B / 0x1E = 2 (arena), else 0 (race A).
//! - 0x800ACB34: 12-byte course rows: +0..2 fog RGB, +3 music track (i8,
//!   -1 none; FUN_8008C4AC @0x8008D2A0), +4 intro clip base (i16, -1 none),
//!   +6 flags (u16), +8 the cup's boss character (i8, challenge), +9 (i8),
//!   +0xA intro clip toggle, +0xB depth bias of the scenery LOD.
//! - 0x80023678: default lap records, 36 i32 [cup * 4 + slot] (FUN_8008F448
//!   copies them to 0x800E0190 + 8 * (cup * 4 + slot) at boot; 300 = 1 s).

use crate::tables::ExeImage;

pub mod addr {
    pub const NAME_TABLE: u32 = 0x800A_CAA0;
    pub const CUP_TABLE: u32 = 0x800A_C1E4;
    pub const COURSE_TABLE: u32 = 0x800A_CB34;
    pub const LAP_RECORDS: u32 = 0x8002_3678;
}

/// Courses in the name table (0x800ACAA0).
pub const COURSES: u8 = 37;
pub const CUPS: usize = 9;
/// Course 10: the front end (its name is "\", no files).
pub const FRONTEND: u8 = 10;
/// Course 0x1E: the attract demo course index (main @0x80078740).
pub const ATTRACT: u8 = 0x1E;
pub const DOCK: u8 = 3;

/// One course as the game's tables describe it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CourseInfo {
    /// Course index (0x800B0FAC).
    pub id: u8,
    /// Disc path without extension, upper case ("COURSE_A/DOCK").
    pub path: String,
    /// The file name ("DOCK").
    pub name: String,
    /// 0x800B0E38 / 0x800B0E32 (FUN_8008AB5C); -1 when not in a cup.
    pub cup: i8,
    pub slot: i8,
    /// 0x800B0B9A (FUN_8008AB5C): 0 race A, 1 race B, 2 arena, 3 stunt.
    pub race_type: i16,
    /// 0x800B0D46 the race loader picks from the race type for a 1-player
    /// game from the menus (FUN_8008C4AC @0x8008CDB4): 1 race, 2 arena,
    /// 3 stunt.
    pub mode: i16,
    /// Karts in a 1-player game (FUN_8008C4AC @0x8008D9A0..): 6, or 5 when
    /// the cup is above 6; 1 in a stunt course.
    pub karts: i16,
    /// Course row 0x800ACB34 + 12 * id.
    pub fog: [u8; 3],
    pub music: i8,
    pub intro_clip: i16,
    pub flags: u16,
    pub boss: i8,
    pub row9: i8,
    pub intro_toggle: u8,
    pub lod_bias: u8,
    /// The boot-time lap record (0x80023678[cup * 4 + slot]); 0 if no cup.
    pub lap_record: i32,
}

impl CourseInfo {
    /// The table row for course `id` (None for the front end or past the
    /// table).
    pub fn read(exe: &ExeImage, id: u8) -> Option<CourseInfo> {
        if id >= COURSES || id == FRONTEND {
            return None;
        }
        let p = u32::from_le_bytes(exe.bytes(addr::NAME_TABLE + 4 * id as u32, 4).try_into().unwrap());
        if !exe.contains(p) {
            return None;
        }
        let mut s = Vec::new();
        let mut a = p;
        while exe.u8(a) != 0 && s.len() < 64 {
            s.push(exe.u8(a));
            a += 1;
        }
        let raw = String::from_utf8_lossy(&s).to_ascii_uppercase().replace('\\', "/");
        let name = raw.rsplit('/').next().unwrap_or("").to_string();
        if name.is_empty() {
            return None;
        }
        let (mut cup, mut slot) = (-1i8, -1i8);
        'find: for c in 0..CUPS {
            for sl in 0..4 {
                if exe.u8(addr::CUP_TABLE + 4 * c as u32 + sl as u32) as i8 == id as i8 {
                    cup = c as i8;
                    slot = sl as i8;
                    break 'find;
                }
            }
        }
        // FUN_8008AB5C @0x8008ABF8.
        let race_type = if cup >= 0 && cup < 6 && slot == 3 {
            3
        } else if cup >= 0 && cup < 6 && slot == 1 {
            1
        } else if id == ATTRACT || id == 0x1B || (cup >= 0 && cup < 6 && slot == 2) || cup == 8 {
            2
        } else {
            0
        };
        let mode = match race_type {
            2 => 2,
            3 => 3,
            _ => 1,
        };
        // 0x8008D9xx: race type 3 -> 1 kart; else 6, 5 when cup > 6.
        let karts = if race_type == 3 { 1 } else if cup > 6 { 5 } else { 6 };
        let row = exe.bytes(addr::COURSE_TABLE + 12 * id as u32, 12);
        let lap_record = if cup >= 0 {
            i32::from_le_bytes(exe.bytes(addr::LAP_RECORDS + 4 * (cup as u32 * 4 + slot as u32), 4).try_into().unwrap())
        } else {
            0
        };
        Some(CourseInfo {
            id,
            path: raw,
            name,
            cup,
            slot,
            race_type,
            mode,
            karts,
            fog: [row[0], row[1], row[2]],
            music: row[3] as i8,
            intro_clip: i16::from_le_bytes([row[4], row[5]]),
            flags: u16::from_le_bytes([row[6], row[7]]),
            boss: row[8] as i8,
            row9: row[9] as i8,
            intro_toggle: row[10],
            lod_bias: row[11],
            lap_record,
        })
    }

    /// Every course in the table (front end excluded), index order.
    pub fn all(exe: &ExeImage) -> Vec<CourseInfo> {
        (0..COURSES).filter_map(|i| CourseInfo::read(exe, i)).collect()
    }

    /// The race courses: race type 0 or 1 (mode 1 races; arenas, stunt
    /// courses, the attract course and the front-end screens excluded).
    pub fn races(exe: &ExeImage) -> Vec<CourseInfo> {
        Self::all(exe).into_iter().filter(|c| c.is_race()).collect()
    }

    /// The stunt courses (cups 0..5, slot 3: race type 3, the stunt
    /// trial; mr_sim::stunt).
    pub fn stunts(exe: &ExeImage) -> Vec<CourseInfo> {
        Self::all(exe).into_iter().filter(|c| c.is_stunt()).collect()
    }

    pub fn is_stunt(&self) -> bool {
        self.race_type == 3 && self.mode == 3 && self.path.starts_with("STUNT")
    }

    pub fn is_race(&self) -> bool {
        self.cup >= 0 && self.mode == 1 && self.path.starts_with("COURSE_")
    }

    /// Find a course by file name ("DOCK", "new_york") or index.
    pub fn find(exe: &ExeImage, key: &str) -> Option<CourseInfo> {
        if let Ok(i) = key.parse::<u8>() {
            return CourseInfo::read(exe, i);
        }
        let k = key.to_ascii_uppercase().replace('-', "_");
        Self::all(exe).into_iter().find(|c| c.name == k || c.path == k)
    }

    /// A disc path of one of the course's files ("TER" -> "COURSE_A/DOCK.TER").
    pub fn file(&self, ext: &str) -> String {
        format!("{}.{}", self.path, ext)
    }
}

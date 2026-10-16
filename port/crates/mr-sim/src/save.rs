//! The game's save block: 0x800E0190..0x800E039C (0x20C bytes), the data a
//! memory-card save holds (docs/spec/frontend.md "Saves"). Everything the
//! menus unlock and the races record lives here; the game initialises it at
//! boot (FUN_8008F448, a new game), updates it in RAM as races end, and the
//! MEMORY CARD page writes it as one card block:
//! `[u32 word sum of the block][the 0x20C bytes]` at offset 0x200 of the
//! save file (after the card header and icon) [RAM: the oracle's block equals
//! its card file's payload byte for byte; the sum checked on that save].
//!
//! Layout (offsets from 0x800E0190) [CODE 8008f448, 8008efac, 8008be68,
//! 80082708, 8008c4ac, 80074a94]:
//! - 0x000 + 0x20 * world + 8 * slot (9 worlds x 4 slots, the world / course
//!   table 0x800AC1E4): i32 lap record (300 = 1 s), +4 the record holder's
//!   character, +5 vehicle, +6 u16 course flags:
//!   0x001 open, 0x002 won (first place in a plain race / battle),
//!   0x004 / 0x020 / 0x080 the course's secret pickups taken (FUN_8008C4AC
//!   @0x8008EBxx marks them gone), 0x008 / 0x010 further per-course secrets
//!   (FUN_8008BE68 counts them over the 24 courses of worlds 0..5),
//!   0x040 no boss to beat here (none, or beaten: race flag 0x10),
//!   0x100 set at a new game on every non-arena slot (an arena's when its
//!   game mode 7 is won, 0x8007916C); 0x1FF = everything (worlds 7, 8).
//! - 0x120 + 2 * league (4 leagues: RACE LEAGUE A, RACE LEAGUE B, BATTLE
//!   LEAGUE, RANDOM RACE LEAGUE): flags (1 open, 2 won), +1 best points
//!   (0x0F at a new game).
//! - 0x128: leagues + courses complete (FUN_8008BE68, 0..31); 0x129 bit 0
//!   DISCO won, bit 1 everything (the secret screen, course 0x1D);
//!   0x12A the best start boost so far (0x800E02BA).
//! - 0x12C: u32 characters collected (bit per character).
//! - 0x130 + 8 * character: 2 x u32 vehicles won with that character.
//! - 0x1F8 + league: laps per league; 0x1FC / 0x1FD laps of a 1 / 2 player
//!   race; 0x1FE / 0x1FF the MEET THE MUPPETS cursor.
//! - 0x200 / 0x201 music / sound volume (0..15, 12 at a new game);
//!   0x202 / 0x203 controller configuration per pad (1 = B); 0x204 / 0x205
//!   vibration (1 = on); 0x206 / 0x207 the screen offset main() sets (8, 2);
//!   0x208 / 0x209 the course-select cursor; 0x20A AI catch-up (8);
//!   0x20B unused.
//! The "GAME STATUS CHANGED - SAVE GAME" prompt compares the first 0x1F8
//! bytes (0x800B07DE) with the copy taken at the last load / save
//! (0x800D8420).

use crate::tables::ExeImage;

pub const BASE: u32 = 0x800E_0190;
pub const LEN: usize = 0x20C;
/// The bytes the "status changed" compare covers (0x800B07DE).
pub const COMPARE_LEN: usize = 0x1F8;

pub mod addr {
    /// World / course table: 9 worlds x 4 slots of course ids (i8).
    pub const WORLDS: u32 = 0x800A_C1E4;
    /// Default lap records, 36 i32.
    pub const LAP_RECORDS: u32 = 0x8002_3678;
    /// Course rows (12 bytes; +8 the boss character, -1 none).
    pub const COURSE_ROWS: u32 = 0x800A_CB34;
    /// Character table, 0x14 bytes per character: +0x0E flags (bit 0 a
    /// racer), +0x0F the character's own vehicle (0xFF none).
    pub const CHARACTERS: u32 = 0x800A_C220;
    /// League course lists: 3 x 6 course ids (RACE LEAGUE A, B, BATTLE).
    pub const LEAGUE_COURSES: u32 = 0x800A_D078;
}

/// The characters a new game draws record holders from (0x800C3028[c] !=
/// -1: the loaded racers; at boot every racer of the table) [INFER].
pub const RACERS: u8 = 25;

#[derive(Clone, PartialEq, Eq)]
pub struct SaveBlock(pub [u8; LEN]);

impl std::fmt::Debug for SaveBlock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SaveBlock(chars {:#x}, status {})", self.chars(), self.status())
    }
}

/// The exe tables the save code reads.
#[derive(Clone, Debug)]
pub struct SaveTables {
    pub worlds: [i8; 36],
    pub default_records: [i32; 36],
    /// Course row +8 per course id (0..37).
    pub boss: Vec<i8>,
    /// Character table +0x0E / +0x0F.
    pub char_flags: Vec<u8>,
    pub char_vehicle: Vec<u8>,
    pub league_courses: [[i8; 6]; 3],
}

impl SaveTables {
    pub fn from_exe(exe: &ExeImage) -> SaveTables {
        let mut worlds = [0i8; 36];
        let mut default_records = [0i32; 36];
        for i in 0..36 {
            worlds[i] = exe.u8(addr::WORLDS + i as u32) as i8;
            default_records[i] = exe.i32(addr::LAP_RECORDS + 4 * i as u32);
        }
        let boss = (0..40).map(|c| exe.u8(addr::COURSE_ROWS + 12 * c + 8) as i8).collect();
        let char_flags = (0..26).map(|c| exe.u8(addr::CHARACTERS + 0x14 * c + 0x0E)).collect();
        let char_vehicle = (0..26).map(|c| exe.u8(addr::CHARACTERS + 0x14 * c + 0x0F)).collect();
        let mut league_courses = [[0i8; 6]; 3];
        for (l, row) in league_courses.iter_mut().enumerate() {
            for (k, c) in row.iter_mut().enumerate() {
                *c = exe.u8(addr::LEAGUE_COURSES + 6 * l as u32 + k as u32) as i8;
            }
        }
        SaveTables { worlds, default_records, boss, char_flags, char_vehicle, league_courses }
    }

    /// The world / slot of course `id` (FUN_8008AB5C), if it has one.
    pub fn place(&self, id: u8) -> Option<(usize, usize)> {
        self.worlds.iter().position(|&c| c == id as i8).map(|i| (i / 4, i % 4))
    }
}

fn entry(w: usize, t: usize) -> usize {
    0x20 * w + 8 * t
}

/// Leagues (0x800E02B0 + 2 * league).
pub const LEAGUE_A: usize = 0;
pub const LEAGUE_B: usize = 1;
pub const LEAGUE_BATTLE: usize = 2;
pub const LEAGUE_RANDOM: usize = 3;

impl SaveBlock {
    pub fn zero() -> SaveBlock {
        SaveBlock([0; LEN])
    }

    pub fn u8(&self, o: usize) -> u8 {
        self.0[o]
    }
    pub fn set_u8(&mut self, o: usize, v: u8) {
        self.0[o] = v;
    }
    pub fn u16(&self, o: usize) -> u16 {
        u16::from_le_bytes([self.0[o], self.0[o + 1]])
    }
    pub fn set_u16(&mut self, o: usize, v: u16) {
        self.0[o..o + 2].copy_from_slice(&v.to_le_bytes());
    }
    pub fn u32(&self, o: usize) -> u32 {
        u32::from_le_bytes(self.0[o..o + 4].try_into().unwrap())
    }
    pub fn set_u32(&mut self, o: usize, v: u32) {
        self.0[o..o + 4].copy_from_slice(&v.to_le_bytes());
    }

    /// The card block's checksum: the 32-bit sum of the block's 131 words.
    pub fn checksum(&self) -> u32 {
        self.0.chunks(4).fold(0u32, |s, w| s.wrapping_add(u32::from_le_bytes(w.try_into().unwrap())))
    }

    // ------------------------------------------------------------ courses

    pub fn record(&self, w: usize, t: usize) -> i32 {
        self.u32(entry(w, t)) as i32
    }
    pub fn holder(&self, w: usize, t: usize) -> (u8, u8) {
        (self.u8(entry(w, t) + 4), self.u8(entry(w, t) + 5))
    }
    pub fn flags(&self, w: usize, t: usize) -> u16 {
        self.u16(entry(w, t) + 6)
    }
    pub fn set_flags(&mut self, w: usize, t: usize, f: u16) {
        self.set_u16(entry(w, t) + 6, f);
    }
    pub fn or_flags(&mut self, w: usize, t: usize, f: u16) {
        let v = self.flags(w, t) | f;
        self.set_flags(w, t, v);
    }
    pub fn course_open(&self, w: usize, t: usize) -> bool {
        self.flags(w, t) & 1 != 0
    }
    /// Open and won: the laps become adjustable (FUN_80074A94 `& 3 == 3`).
    pub fn course_won(&self, w: usize, t: usize) -> bool {
        self.flags(w, t) & 3 == 3
    }

    // ------------------------------------------------------------ leagues

    pub fn league_flags(&self, l: usize) -> u8 {
        self.u8(0x120 + 2 * l)
    }
    pub fn league_best(&self, l: usize) -> u8 {
        self.u8(0x121 + 2 * l)
    }
    pub fn league_laps(&self, l: usize) -> u8 {
        self.u8(0x1F8 + l)
    }
    pub fn set_league_laps(&mut self, l: usize, n: u8) {
        self.set_u8(0x1F8 + l, n);
    }

    /// 0x800E02B8: leagues and courses complete (0..31).
    pub fn status(&self) -> u8 {
        self.u8(0x128)
    }
    /// 0x800E02B9: bit 0 DISCO won, bit 1 everything.
    pub fn secrets(&self) -> u8 {
        self.u8(0x129)
    }

    // ------------------------------------------------------------ characters

    pub fn chars(&self) -> u32 {
        self.u32(0x12C)
    }
    pub fn vehicles(&self, c: u8) -> [u32; 2] {
        let o = 0x130 + 8 * c as usize;
        [self.u32(o), self.u32(o + 4)]
    }
    pub fn char_open(&self, c: u8) -> bool {
        c < 32 && self.chars() & (1 << c) != 0
    }
    pub fn vehicle_open(&self, c: u8, v: u8) -> bool {
        c < 25 && self.vehicles(c)[(v / 32) as usize & 1] & (1 << (v % 32)) != 0
    }

    // ------------------------------------------------------------ options

    /// 0x800E038C + players - 1: laps of a 1 / 2 player race.
    pub fn laps(&self, p: usize) -> u8 {
        self.u8(0x1FC + p)
    }
    pub fn set_laps(&mut self, p: usize, n: u8) {
        self.set_u8(0x1FC + p, n);
    }
    pub fn music_volume(&self) -> u8 {
        self.u8(0x200)
    }
    pub fn sound_volume(&self) -> u8 {
        self.u8(0x201)
    }
    pub fn set_volumes(&mut self, music: u8, sound: u8) {
        self.set_u8(0x200, music);
        self.set_u8(0x201, sound);
    }
    pub fn alt_controls(&self, pad: usize) -> bool {
        self.u8(0x202 + pad) != 0
    }
    pub fn set_alt_controls(&mut self, pad: usize, b: bool) {
        self.set_u8(0x202 + pad, b as u8);
    }
    /// 0x800E039A: the AI catch-up scale (/16).
    pub fn difficulty(&self) -> u8 {
        self.u8(0x20A)
    }

    // ------------------------------------------------------------ new game

    /// FUN_8008F448 (a new game, at boot) and main()'s 0x800E0396 = 8,
    /// 0x800E0397 = 2 after it. `rand`: the BIOS rand() (record holders:
    /// one racer per course, rand() / 0x2AB among the loaded racers).
    pub fn new_game(t: &SaveTables, rand: &mut dyn FnMut() -> i32) -> SaveBlock {
        let mut s = SaveBlock::zero();
        s.init_courses(t);
        for w in 0..9 {
            for k in 0..4 {
                let o = entry(w, k);
                s.set_u32(o, t.default_records[w * 4 + k] as u32);
                let c = loop {
                    let c = rand() / 0x2AB;
                    if (c as u8) < RACERS && c >= 0 && t.char_flags[c as usize] & 1 != 0 {
                        break c as usize;
                    }
                };
                s.set_u8(o + 4, c as u8);
                s.set_u8(o + 5, t.char_vehicle[c]);
            }
        }
        s.init_leagues();
        s.init_characters(t);
        s.set_u8(0x204, 1);
        s.set_u8(0x205, 1);
        for o in 0x1F8..0x1FE {
            s.set_u8(o, 3);
        }
        s.set_u32(0x200, 0xC0C);
        s.set_u8(0x206, 0);
        s.set_u8(0x207, 0);
        s.set_u8(0x20A, 8);
        s.set_u8(0x208, 0);
        s.set_u8(0x209, 0);
        s.set_u8(0x1FE, 0);
        s.set_u8(0x1FF, 0);
        s.set_u8(0x128, 0);
        s.set_u8(0x129, 0);
        s.set_u8(0x12A, 0);
        // main() @0x80078768 / 0x80078778
        s.set_u8(0x206, 8);
        s.set_u8(0x207, 2);
        s
    }

    /// FUN_8008EFAC: the course flags of a new game.
    fn init_courses(&mut self, t: &SaveTables) {
        for w in 0..9 {
            for k in 0..4 {
                if w < 7 {
                    self.set_flags(w, k, (w < 6) as u16);
                    if w == 6 {
                        self.or_flags(w, k, 0x20);
                    }
                    let c = t.worlds[w * 4 + k];
                    if c == -1 || t.boss.get(c as usize).copied().unwrap_or(-1) == -1 {
                        self.or_flags(w, k, 0x40);
                    }
                    if w >= 6 || k != 2 {
                        self.or_flags(w, k, 0x100);
                    }
                } else {
                    self.set_flags(w, k, 0x1FF);
                }
            }
        }
    }

    /// FUN_8008EF48: leagues 0..2 open, best 0x0F, 3 laps.
    fn init_leagues(&mut self) {
        for l in 0..4 {
            self.set_u8(0x120 + 2 * l, (l < 3) as u8);
            self.set_u8(0x121 + 2 * l, 0xF);
            self.set_u8(0x1F8 + l, 3);
        }
    }

    /// FUN_8008F1F0: characters 0..7, each with its own vehicle.
    fn init_characters(&mut self, t: &SaveTables) {
        self.set_u32(0x12C, 0xFF);
        for c in 0..25 {
            let o = 0x130 + 8 * c;
            self.set_u32(o, 0);
            self.set_u32(o + 4, 0);
            let v = t.char_vehicle[c];
            if v != 0xFF {
                let oo = o + 4 * (v as usize >> 5);
                let m = self.u32(oo) | 1 << (v & 0x1F);
                self.set_u32(oo, m);
            }
        }
    }

    /// The game's "everything" code (FUN_8008F884 case 7: every character
    /// and vehicle, every course 0x1FF, the leagues won with 36 points, then
    /// case 5: 0x800E02B9 = 3), followed by FUN_8008BE68.
    pub fn unlock_all(&mut self, t: &SaveTables) {
        // FUN_8008F2AC
        self.set_u32(0x12C, 0xFFFF_FFFF);
        for c in 0..25 {
            self.set_u32(0x130 + 8 * c, 0xFFFF_FFFF);
            self.set_u32(0x134 + 8 * c, 0xFFFF_FFFF);
        }
        for w in 0..9 {
            for k in 0..4 {
                self.set_flags(w, k, 0x1FF);
            }
        }
        for l in 0..4 {
            self.set_u8(0x120 + 2 * l, 3);
            self.set_u8(0x121 + 2 * l, 0x24);
        }
        self.set_u8(0x129, 3);
        self.progression(t);
    }

    // ------------------------------------------------------------ races

    /// FUN_8008ACE4 @0x8008B068, loading a course: the race laps go back
    /// to 3 unless this course (a race, type < 2) is won; every league's
    /// laps likewise unless the league is won.
    pub fn course_loaded(&mut self, place: Option<(usize, usize)>, race_type: i16) {
        if let Some((w, t)) = place {
            if race_type < 2 && !self.course_won(w, t) {
                self.set_laps(0, 3);
                self.set_laps(1, 3);
            }
        }
        for l in 0..4 {
            if self.league_flags(l) & 3 != 3 {
                self.set_league_laps(l, 3);
            }
        }
    }

    /// The lap-end record (FUN_80082708 @0x80082E80 for a human's kart in a
    /// race without race flag 2): a lap faster than the course record takes
    /// it, with the kart's character and vehicle. Returns true if it did.
    pub fn lap_record(&mut self, w: usize, t: usize, lap: i32, character: u8, vehicle: u8) -> bool {
        if lap > 0 && lap < self.record(w, t) {
            let o = entry(w, t);
            self.set_u32(o, lap as u32);
            self.set_u8(o + 4, character);
            self.set_u8(o + 5, vehicle);
            true
        } else {
            false
        }
    }

    /// FUN_80082708 @0x800832EC: player 1 crossing the line first in a
    /// 1-player race with no race options (or DISCO's option 8) on a course
    /// with a path sets the course's "won" bit (and plays sound 0x24 when
    /// new). Returns true when the bit is new.
    pub fn race_won(&mut self, w: usize, t: usize) -> bool {
        let f = self.flags(w, t);
        if f & 2 == 0 {
            self.set_flags(w, t, f | 2);
            true
        } else {
            false
        }
    }

    /// FUN_8008BE68, run by main() between course sessions (and after a
    /// 1-player league race): a won course opens the next slot of its
    /// world; all six worlds' race A / race B / arena slots open open
    /// their leagues; leagues A and B won open the random league; the
    /// per-course secret bits over the 24 courses of worlds 0..5 open the
    /// secret courses (world 6: bit 8 -> slot 0 (course 0x1A), 0x10 -> slot
    /// 1, 0x20 -> slot 2 and character 24, everything 0x127 with the
    /// leagues -> slot 3, DISCO); DISCO won and everything complete set
    /// 0x800E02B9; 0x800E02B8 counts the complete leagues and courses.
    /// Returns the course the game announces (0x800B0800: the newly opened
    /// course, or 0x1D when 0x800E02B9 changed; main() then loads it
    /// outside a league).
    pub fn progression(&mut self, t: &SaveTables) -> Option<u8> {
        let mut msg: Option<u8> = None;
        let mut open = [0i32; 3];
        let (mut s127, mut s20, mut s8, mut s10) = (0, 0, 0, 0);
        let count = |f: u16, s127: &mut i32, s20: &mut i32, s8: &mut i32, s10: &mut i32| {
            if f & 0x127 == 0x127 {
                *s127 += 1;
            }
            if f & 0x20 != 0 {
                *s20 += 1;
            }
            if f & 8 != 0 {
                *s8 += 1;
            }
            if f & 0x10 != 0 {
                *s10 += 1;
            }
        };
        let old_secrets = self.secrets();
        for w in 0..6 {
            for k in 0..3 {
                if self.flags(w, k) & 1 != 0 {
                    open[k] += 1;
                    if self.flags(w, k) & 2 != 0 && self.flags(w, k + 1) & 1 == 0 {
                        self.or_flags(w, k + 1, 1);
                        let c = t.worlds[w * 4 + k + 1];
                        if c != -1 {
                            msg = Some(c as u8);
                        }
                    }
                    count(self.flags(w, k), &mut s127, &mut s20, &mut s8, &mut s10);
                }
            }
            let f = self.flags(w, 3);
            if f & 1 != 0 {
                count(f, &mut s127, &mut s20, &mut s8, &mut s10);
            }
        }
        if open[0] == 6 {
            self.0[0x120] |= 1;
        }
        if open[1] == 6 {
            self.0[0x122] |= 1;
        }
        if open[2] == 6 {
            self.0[0x124] |= 1;
        }
        let ab_won = self.0[0x120] & 2 != 0 && self.0[0x122] & 2 != 0;
        if ab_won {
            self.0[0x126] |= 1;
        }
        if s8 == 24 && self.flags(6, 0) & 1 == 0 {
            self.or_flags(6, 0, 1);
            msg = Some(0x1A);
        }
        if s10 == 24 && self.flags(6, 1) & 1 == 0 {
            self.or_flags(6, 1, 1);
            msg = Some(0x1B);
        }
        if s20 == 24 && self.flags(6, 2) & 1 == 0 {
            self.or_flags(6, 2, 1);
            msg = Some(0x1C);
            let c = self.chars() | 0x100_0000;
            self.set_u32(0x12C, c);
        }
        if s127 == 24 && ab_won && self.0[0x124] & 2 != 0 && self.flags(6, 3) & 1 == 0 {
            msg = Some(0x1F);
            self.or_flags(6, 3, 1);
        }
        if self.flags(6, 3) & 2 != 0 {
            self.0[0x129] |= 1;
        }
        let all = (0..7).all(|w| (0..4).all(|k| self.flags(w, k) & 0x1FF == 0x1FF));
        let league_done = |s: &SaveBlock, l: usize, min: u8| s.0[0x120 + 2 * l] & 2 != 0 && s.0[0x121 + 2 * l] > min;
        if all && league_done(self, 0, 0x1D) && league_done(self, 1, 0x1D) && league_done(self, 2, 0x13) {
            self.0[0x129] |= 2;
        }
        if self.secrets() != old_secrets {
            msg = Some(0x1D);
        }
        let mut n = league_done(self, 0, 0x1D) as u8 + league_done(self, 1, 0x1D) as u8 + league_done(self, 2, 0x13) as u8;
        for w in 0..7 {
            for k in 0..4 {
                if self.flags(w, k) & 0x1FF == 0x1FF {
                    n += 1;
                }
            }
        }
        self.0[0x128] = n;
        msg
    }

    /// The end of a league (FUN_8008C4AC @0x8008CE98, 1 player): the player
    /// first in the final table wins the league (flags bit 1); a higher
    /// points total than the league's best is kept. Returns true when the
    /// game plays its fanfare (0x24): a new win, or a new best of 30+ (20+
    /// in the battle league).
    pub fn league_end(&mut self, l: usize, player_first: bool, points: u8) -> bool {
        let mut fanfare = false;
        if player_first && self.0[0x120 + 2 * l] & 2 == 0 {
            self.0[0x120 + 2 * l] |= 2;
            fanfare = true;
        }
        if self.0[0x121 + 2 * l] < points {
            let min = if l == LEAGUE_BATTLE { 0x13 } else { 0x1D };
            if points > min {
                fanfare = true;
            }
            self.0[0x121 + 2 * l] = points;
        }
        fanfare
    }

    // ------------------------------------------------------------ files

    /// The PS1 memory-card save (one 8 KiB block): checksum at 0x200, the
    /// block at 0x204.
    pub fn from_card_block(b: &[u8]) -> Option<SaveBlock> {
        if b.len() < 0x204 + LEN || &b[0..2] != b"SC" {
            return None;
        }
        let s = SaveBlock(b[0x204..0x204 + LEN].try_into().ok()?);
        (s.checksum() == u32::from_le_bytes(b[0x200..0x204].try_into().ok()?)).then_some(s)
    }

    /// A whole memory card image (128 KiB, "MC"): the first save whose
    /// directory entry names this game (BASLUS-01237...).
    pub fn from_card(card: &[u8]) -> Option<SaveBlock> {
        if card.len() < 0x20000 || &card[0..2] != b"MC" {
            return None;
        }
        (1..16).find_map(|i| {
            let e = &card[0x80 * i..0x80 * (i + 1)];
            (e[0] == 0x51 && e[0x0A..].starts_with(b"BASLUS-01237")).then(|| SaveBlock::from_card_block(&card[0x2000 * i..0x2000 * (i + 1)])).flatten()
        })
    }

    /// The port's save file: "MRMSAVE1", u32 format version 1, u32 checksum
    /// (as the card's), the block.
    pub fn to_file(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(16 + LEN);
        v.extend_from_slice(FILE_MAGIC);
        v.extend_from_slice(&FILE_VERSION.to_le_bytes());
        v.extend_from_slice(&self.checksum().to_le_bytes());
        v.extend_from_slice(&self.0);
        v
    }

    pub fn from_file(b: &[u8]) -> Result<SaveBlock, String> {
        if b.len() != 16 + LEN || &b[0..8] != FILE_MAGIC {
            return Err("not a port save file".into());
        }
        let ver = u32::from_le_bytes(b[8..12].try_into().unwrap());
        if ver != FILE_VERSION {
            return Err(format!("save file version {ver}, expected {FILE_VERSION}"));
        }
        let s = SaveBlock(b[16..].try_into().unwrap());
        if s.checksum() != u32::from_le_bytes(b[12..16].try_into().unwrap()) {
            return Err("save file checksum mismatch".into());
        }
        Ok(s)
    }
}

pub const FILE_MAGIC: &[u8; 8] = b"MRMSAVE1";
pub const FILE_VERSION: u32 = 1;

// ---------------------------------------------------------------- leagues

/// One kart's points from one league race (0x800C1E08 + 5 * kart, the
/// RACE POINTS columns): finishing place (3, 2, 1, then 0), fastest lap,
/// best start, most hits; and the total.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RacePoints {
    pub place: u8,
    pub fastest_lap: u8,
    pub best_start: u8,
    pub most_hits: u8,
    pub total: u8,
}

/// What FUN_8008C4AC reads of each kart after a league race.
#[derive(Clone, Copy, Debug)]
pub struct LeagueKart {
    /// Rank in the finishing order (0x800D8F94 order: 0 = first).
    pub rank: usize,
    /// +0x384: best lap.
    pub best_lap: i32,
    /// +0x492 / +0x494: hits dealt (the count) and taken (the tie-break).
    pub hits: i16,
    pub hits_taken: i16,
}

/// FUN_8008C4AC @0x8008C4xx..0x8008CC70 (entering game mode 0x11 after a
/// race league race): every kart's points. `fastest`: 0x800B0DD0 (the
/// race's best lap); `start_mask`: 0x800B0E8C (the karts with the best
/// start, bit per kart slot). Karts in slot order.
pub fn race_points(karts: &[LeagueKart], fastest: i32, start_mask: u8) -> Vec<RacePoints> {
    // the most hits dealt, ties to the fewest taken
    // (the game's loop, quirks included)
    let (mut most, mut tie) = (0i16, -1i16);
    for k in karts {
        if most <= k.hits {
            if tie == -1 || most < k.hits || k.hits_taken < tie {
                tie = k.hits_taken;
            }
            most = k.hits;
        }
    }
    karts
        .iter()
        .enumerate()
        .map(|(slot, k)| {
            let place = (3 - k.rank as i32).max(0) as u8;
            let fastest_lap = (fastest != 0 && k.best_lap == fastest) as u8;
            let best_start = (start_mask >> slot) & 1;
            let most_hits = (most > 0 && k.hits == most && k.hits_taken == tie) as u8;
            RacePoints { place, fastest_lap, best_start, most_hits, total: place + fastest_lap + best_start + most_hits }
        })
        .collect()
}

/// One kart of a BATTLE LEAGUE battle, for its points (phase 6b).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BattleKart {
    /// Finishing rank 0.. (0x800D8F94: by damage, least first).
    pub rank: usize,
    /// +0x43C damage dealt, +0x43E taken, +0x478 best crash, +0x4FB
    /// knock-outs.
    pub dealt: i16,
    pub taken: i16,
    pub best_crash: i16,
    pub knockouts: u8,
}

/// FUN_8008C4AC (entering game mode 0x11 after a BATTLE LEAGUE battle,
/// 0x800C1E4C = 1): place 3 / 2 / 1, +1 the best crash (+0x478, the
/// biggest of all, above 0), +1 the most knock-outs (+0x4FB; nobody when
/// two or more share it), +1 the most damage dealt (+0x43C, ties to the
/// fewest taken +0x43E: the race league's most-hits loop). The fields of
/// `RacePoints` keep the race's names: `fastest_lap` = best crash,
/// `best_start` = most knock-outs, `most_hits` = most damage.
pub fn battle_points(karts: &[BattleKart]) -> Vec<RacePoints> {
    let (mut most, mut tie) = (0i16, -1i16);
    let (mut ko, mut ko_n) = (0i32, 0);
    let mut crash = 0i16;
    for k in karts {
        if most <= k.dealt {
            if tie == -1 || most < k.dealt || k.taken < tie {
                tie = k.taken;
            }
            most = k.dealt;
        }
        let v = k.knockouts as i8 as i32;
        if ko < v {
            ko_n = 1;
            ko = v;
        } else if ko != 0 && v == ko {
            ko_n += 1;
        }
        crash = crash.max(k.best_crash);
    }
    if ko_n > 1 {
        ko = 0;
    }
    karts
        .iter()
        .map(|k| {
            let place = (3 - k.rank as i32).max(0) as u8;
            let best = (crash > 0 && k.best_crash == crash) as u8;
            let kos = (ko > 0 && k.knockouts as i8 as i32 == ko) as u8;
            let dmg = (most > 0 && k.dealt == most && k.taken == tie) as u8;
            RacePoints { place, fastest_lap: best, best_start: kos, most_hits: dmg, total: place + best + kos + dmg }
        })
        .collect()
}

/// The table order (0x800C1E30): kart slots by points, most first; the
/// game's bubble sort keeps slot order among equals.
pub fn table_order(points: &[u8]) -> Vec<usize> {
    let mut o: Vec<usize> = (0..points.len()).collect();
    let n = o.len();
    for _ in 0..n.saturating_sub(1) {
        for i in 0..n - 1 {
            if points[o[i]] < points[o[i + 1]] {
                o.swap(i, i + 1);
            }
        }
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tables() -> Option<SaveTables> {
        let mut disc = mr_disc::Disc::open_default().ok()?;
        let exe = disc.read("SLUS_012.37").ok()?;
        Some(SaveTables::from_exe(&ExeImage::parse(&exe).ok()?))
    }

    #[test]
    fn points_and_table() {
        let k = |rank, best_lap, hits, hits_taken| LeagueKart { rank, best_lap, hits, hits_taken };
        let karts = [k(2, 900, 3, 5), k(0, 870, 4, 7), k(1, 870, 4, 6), k(3, 950, 0, 0), k(4, 999, 1, 0), k(5, 0, 0, 0)];
        let p = race_points(&karts, 870, 0b0000_1000);
        let tot: Vec<u8> = p.iter().map(|r| r.total).collect();
        // slot 2: 2nd place + fastest lap + most hits (fewer taken: 6 < 7)
        assert_eq!(tot, vec![1, 4, 4, 1, 0, 0]);
        assert_eq!(table_order(&tot), vec![1, 2, 0, 3, 4, 5]);
    }

    #[test]
    fn new_game_and_progression() {
        let Some(t) = tables() else { return };
        let mut seed = 1u32;
        let mut rand = || crate::session::bios_rand(&mut seed);
        let mut s = SaveBlock::new_game(&t, &mut rand);
        // the oracle's (new) save: characters 0..7, worlds 0..5 open, the
        // secret courses closed (0x160 / 0x120), worlds 7 / 8 0x1FF
        assert_eq!(s.chars(), 0xFF);
        assert!(s.course_open(0, 0) && s.course_open(5, 3) && !s.course_open(6, 0));
        assert_eq!(s.flags(7, 0), 0x1FF);
        assert_eq!(s.record(0, 0), t.default_records[0]);
        assert_eq!((s.laps(0), s.music_volume(), s.difficulty()), (3, 12, 8));
        let before = s.clone();
        assert_eq!(s.progression(&t), None);
        assert_eq!(s, before, "a new game is stable under FUN_8008BE68");
        // the 24 courses' bit 8 open the first secret course
        for w in 0..6 {
            for k in 0..4 {
                s.or_flags(w, k, 8);
            }
        }
        assert_eq!(s.progression(&t), Some(0x1A));
        assert!(s.course_open(6, 0));
        // file round trip
        let f = s.to_file();
        assert_eq!(SaveBlock::from_file(&f).unwrap(), s);
        let mut bad = f.clone();
        bad[40] ^= 1;
        assert!(SaveBlock::from_file(&bad).is_err());
        // everything
        s.unlock_all(&t);
        assert_eq!(s.status(), 31);
    }

    /// tools/oracle/save_check.py unlock (analysis/s7/saves, local): the
    /// game's FUN_8008BE68 between DOCK's course menu and the next course on
    /// a poked block (every world 0..5 course with flag 8, DOCK won, ISLAND
    /// closed): the block after equals the port's, and the game loaded the
    /// course the port announces (0x1A, OVAL).
    #[test]
    fn progression_matches_oracle() {
        let Some(t) = tables() else { return };
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../analysis/s7/saves");
        let (Ok(before), Ok(after)) = (std::fs::read(dir.join("unlock_before.bin")), std::fs::read(dir.join("unlock_after.bin"))) else { return };
        let mut s = SaveBlock(before.try_into().unwrap());
        let msg = s.progression(&t);
        assert_eq!(s.0.to_vec(), after, "block after FUN_8008BE68");
        assert_eq!(msg, Some(0x1A), "the game loaded course 26");
    }

    /// tools/oracle/save_check.py league (analysis/s7/saves/league.txt,
    /// local): RACE LEAGUE A's first race on BOG ended by the player's kart
    /// marked finished; the game's points per kart (0x800C1E08 + 5k) and
    /// table (0x800C1E30) from the karts' ranks, best laps, hits and the
    /// best-start mask equal `race_points` / `table_order`.
    #[test]
    fn league_points_match_oracle() {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../analysis/s7/saves/league.txt");
        let Ok(text) = std::fs::read_to_string(p) else { return };
        let num = |l: &str, key: &str| -> i64 { l.split_whitespace().skip_while(|w| *w != key).nth(1).unwrap().parse().unwrap() };
        let mut fastest = 0;
        let mut mask = 0u8;
        let mut karts = Vec::new();
        let mut game = Vec::new();
        let mut table = Vec::new();
        for l in text.lines() {
            if l.starts_with("fastest") {
                fastest = num(l, "fastest") as i32;
            } else if l.starts_with("start_mask") {
                mask = num(l, "start_mask") as u8;
            } else if l.starts_with("kart") {
                karts.push(LeagueKart { rank: num(l, "rank") as usize, best_lap: num(l, "best") as i32, hits: num(l, "hits") as i16, hits_taken: num(l, "taken") as i16 });
                let g: Vec<u8> = l.split_whitespace().skip_while(|w| *w != "game").nth(1).unwrap().split(',').map(|x| x.parse().unwrap()).collect();
                game.push(RacePoints { place: g[0], fastest_lap: g[1], best_start: g[2], most_hits: g[3], total: g[4] });
            } else if let Some(t) = l.strip_prefix("table ") {
                table = t.split(',').map(|x| x.parse::<usize>().unwrap()).collect();
            }
        }
        let ours = race_points(&karts, fastest, mask);
        assert_eq!(ours, game);
        let totals: Vec<u8> = ours.iter().map(|p| p.total).collect();
        assert_eq!(table_order(&totals), table);
    }

    /// Phase 6b: a BATTLE LEAGUE battle's points (oracle, local:
    /// tools/oracle/save_check.py battle_league -> analysis/oracle/secrets/
    /// battle_league/battle_league.txt: CANYON, the player put out 300
    /// VBlanks after GO, FUN_80082450 settling the five AI karts) equal the
    /// game's 0x800C1E08.. and its table 0x800C1E30.
    #[test]
    fn battle_league_points_match_oracle() {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../analysis/oracle/secrets/battle_league/battle_league.txt");
        let Ok(text) = std::fs::read_to_string(p) else { return };
        let num = |l: &str, key: &str| -> i64 { l.split_whitespace().skip_while(|w| *w != key).nth(1).unwrap().parse().unwrap() };
        let mut karts = Vec::new();
        let mut game = Vec::new();
        let mut table = Vec::new();
        for l in text.lines() {
            if l.starts_with("kart") {
                karts.push(BattleKart { rank: num(l, "rank") as usize, dealt: num(l, "dealt") as i16, taken: num(l, "taken") as i16, best_crash: num(l, "crash") as i16, knockouts: num(l, "kos") as u8 });
                let g: Vec<u8> = l.split_whitespace().skip_while(|w| *w != "game").nth(1).unwrap().split(',').map(|x| x.parse().unwrap()).collect();
                game.push(RacePoints { place: g[0], fastest_lap: g[1], best_start: g[2], most_hits: g[3], total: g[4] });
            } else if let Some(t) = l.strip_prefix("table ") {
                table = t.split(',').map(|x| x.parse::<usize>().unwrap()).collect();
            }
        }
        let ours = battle_points(&karts);
        assert_eq!(ours, game);
        let totals: Vec<u8> = ours.iter().map(|p| p.total).collect();
        assert_eq!(table_order(&totals), table);
    }

    /// The oracle's block (course captures' ram0, local) against a new game:
    /// equal except the random record holders.
    #[test]
    fn new_game_matches_oracle_ram() {
        let Some(t) = tables() else { return };
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../analysis/oracle/courses/BOG");
        let (Ok(meta), Ok(frames)) = (std::fs::read_to_string(root.join("meta.json")), std::fs::read(root.join("frames.bin"))) else { return };
        // "regions": [[addr, len], ...]; the first frame
        let Some(r) = meta.split("\"regions\"").nth(1).and_then(|r| r.split("]]").next()) else { return };
        let nums: Vec<u64> = r.split(|c: char| !c.is_ascii_digit()).filter(|x| !x.is_empty()).filter_map(|x| x.parse().ok()).collect();
        let v: Vec<(u32, usize)> = nums.chunks(2).filter(|c| c.len() == 2).map(|c| (c[0] as u32, c[1] as usize)).collect();
        let mut o = 0;
        let mut ram = None;
        for (a, n) in v {
            if a == 0x800E_0180 {
                ram = frames.get(o + 0x10..o + 0x10 + LEN).map(|b| SaveBlock(b.try_into().unwrap()));
            }
            o += n;
        }
        let Some(ram) = ram else { return };
        let mut seed = 1u32;
        let mut rand = || crate::session::bios_rand(&mut seed);
        let mut s = SaveBlock::new_game(&t, &mut rand);
        for w in 0..9 {
            for k in 0..4 {
                let e = entry(w, k);
                s.0[e + 4] = ram.0[e + 4];
                s.0[e + 5] = ram.0[e + 5];
            }
        }
        let diff: Vec<usize> = (0..LEN).filter(|&i| s.0[i] != ram.0[i]).collect();
        assert!(diff.is_empty(), "new game differs from the oracle's block at {diff:x?}");
    }
}

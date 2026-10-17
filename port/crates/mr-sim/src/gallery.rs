//! MEET THE MUPPETS (phase 11; docs/spec/frontend.md "MEET THE MUPPETS"):
//! game mode 0xD on the frontend course 0x1E. The 25 characters stand in a
//! 5 x 5 gallery at x = (col * 600 + 50000) * 32, y = row * 24000 - 24000,
//! z = -29376 (FUN_80026920 at the load); camera mode 6 looks at the one
//! under the cursor (camera.rs). Each frame FUN_80026A50 moves every
//! character's body-sway springs toward a look direction it redraws now
//! and then, and draws the nine around the cursor (those not collected
//! black) as drivers without a vehicle through a scratch kart record at
//! 0x800C49BC. FUN_80074A94 (game mode 0xD) is the logic: the cursor, CROSS
//! = TEST DRIVE (game mode 0xE) with the character, TRIANGLE back, and the
//! voice clips (FUN_80060580): a clip of the character under the cursor
//! once it rests there and nothing plays, and a clip on CROSS for three
//! characters.
//!
//! Check: tools/oracle/xa_probe.py (analysis/s8/meet, local): the record
//! table 0x800B3B78 and the scratch record equal `Gallery::new` /
//! `record` (tests/gallery.rs); the clips' timing in VBlanks.

use crate::camera::GalleryCam;
use crate::fixed::{div_pow2, substeps};
use crate::globals::RaceGlobals;
use crate::render::{mul_matrix0, rot_lean, rot_xyz, Mat};
use crate::world::World;

/// The 5 x 5 grid of characters (row-major, -1 none) [CODE 0x800AC208].
pub const GRID: u32 = 0x800A_C208;
/// Two heights per character (i16, i16): the camera centres on half their
/// sum [CODE 0x800AC038].
pub const HEIGHTS: u32 = 0x800A_C038;
/// The character table (0x14-byte rows from 0x800AC22E: +0 voice flags, +1
/// own vehicle, +2 voice clip base) [CODE].
pub const CHARACTERS: u32 = 0x800A_C22E;
/// The character select's vehicle list (25 entries) [CODE 0x800AC6C0].
pub const VEHICLE_LIST: u32 = 0x800A_C6C0;
/// The game's identity matrix (FUN_80086DF0's +0x2AE).
const IDENTITY: u32 = 0x800A_87B0;
/// 0x800B074B's start value and the idle clip's spacing (VBlanks).
pub const CLIP_WAIT: i32 = 0x3C;
/// FUN_80074A94: 0x800B0AF0 >= 0x708 idle VBlanks leave for the title.
pub const IDLE_OUT: i32 = 0x708;
/// TEST DRIVE leaves for the title after 0x1C20 idle VBlanks.
pub const TEST_DRIVE_IDLE_OUT: i32 = 0x1C20;
/// The record's z (FUN_80026920: -0x72C0 sign-extended).
pub const Z: i32 = -0x72C0;

/// One character's record (0x800B3B78 + 0x64 * (5 * row + col)).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GalleryChar {
    /// +0 / +4 / +8.
    pub pos: [i32; 3],
    /// +0xC: the 18 body-sway spring words (FUN_80064738).
    pub sway: [i32; 18],
    /// +0x54..+0x5C: the smoothed outputs (the kart record's +0x454..+0x45C).
    pub out: [i16; 5],
    /// +0x5E: VBlanks until the next look draw.
    pub timer: i16,
    /// +0x60: the look direction: 0 ahead, 1 / 2 the sides.
    pub look: i8,
}

/// What the logic did this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GalleryOut {
    /// FUN_80060580(character, n).
    pub clip: Option<(u8, u8)>,
    /// The column changed: sound 0xE (0x1800, 0x40, 0x40).
    pub nav: bool,
    /// CROSS on a collected character: TEST DRIVE with it (game mode 0xE).
    pub test_drive: Option<u8>,
    /// TRIANGLE or idle: the title (game mode 0x10, sub-state 0).
    pub leave: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gallery {
    pub chars: [GalleryChar; 25],
    /// 0x800B0E24 / 0x800B0E25.
    pub row: i8,
    pub col: i8,
    /// 0x800B074B: VBlanks before the next idle clip.
    pub clip_timer: i32,
    /// 0x800B074A: the character of the last idle clip (-1 none).
    pub last_clip: i8,
    /// 0x800B0AF0: idle VBlanks (FUN_8005C2EC adds dt; menu input zeroes it).
    pub idle: i32,
    /// 0x800E038F: the column as last saved (the column sound compares).
    pub saved_col: i8,
}

/// The character at (row, col) of the grid, if any.
pub fn character_at(w: &World, row: i8, col: i8) -> Option<u8> {
    if !(0..5).contains(&row) || !(0..5).contains(&col) {
        return None;
    }
    let c = w.exe.u8(GRID + (row as u32) * 5 + col as u32) as i8;
    (c >= 0).then_some(c as u8)
}

/// Half the sum of the character's two heights (camera mode 6): the
/// first is |the character model's part 0 pivot y| (FUN_80087ACC writes it
/// over the table's when the model loads), the second the table's.
pub fn height(w: &World, c: u8, pivot0_y: i32) -> i32 {
    let b = w.exe.i16(HEIGHTS + 4 * c as u32 + 2) as i32;
    (pivot0_y.abs() + b) / 2
}

/// FUN_8008B36C(c): the vehicles TEST DRIVE offers: the character's own
/// (0x800AC22F[c * 0x14]) first, then those of the vehicle list it has won
/// (0x800E02C0 + 8 c, `won(v)`).
pub fn vehicle_list(w: &World, c: u8, won: &dyn Fn(u8) -> bool) -> Vec<u8> {
    let own = w.exe.u8(CHARACTERS + 1 + 0x14 * c as u32);
    let mut out = vec![own];
    for i in 0..25 {
        let v = w.exe.u8(VEHICLE_LIST + i);
        if v != 0xFF && v != own && won(v) {
            out.push(v);
        }
    }
    out
}

impl Gallery {
    /// FUN_8008C4AC entering game mode 0xD: the cursor from the save
    /// (0x800E038E / F), FUN_80026920 (one random byte per character for
    /// its first look timer), 0x800B074A = -1, 0x800B074B = 0x3C.
    pub fn new(g: &mut RaceGlobals, w: &World, cursor: (i8, i8)) -> Gallery {
        let mut chars = [GalleryChar::default(); 25];
        for row in 0..5 {
            for col in 0..5 {
                let c = &mut chars[row * 5 + col];
                c.pos = [(col as i32 * 600 + 50000) * 0x20, row as i32 * 24000 - 24000, Z];
                c.timer = w.rng_byte(g.rng) as i16 + 0x3C;
                g.rng = g.rng.wrapping_add(1);
            }
        }
        let (row, col) = (cursor.0.clamp(0, 4), cursor.1.clamp(0, 4));
        Gallery { chars, row, col, clip_timer: CLIP_WAIT, last_clip: -1, idle: 0, saved_col: col }
    }

    /// FUN_80026A50's per-character step (all 25 every frame): a new look
    /// once the timer runs out (a random byte / 0x56, not the current one,
    /// never 1 in column 4 or 2 in column 0; then a timer of a random byte
    /// + 0x3C), the springs toward it (target 0, -0xC000 or 0xC000; gain 0,
    /// effect -1), the outputs smoothed toward the springs / 64 by a
    /// quarter per VBlank.
    pub fn animate(&mut self, g: &mut RaceGlobals, w: &World) {
        for (i, c) in self.chars.iter_mut().enumerate() {
            let col = (i % 5) as i32;
            if c.timer != 0 {
                let t = (c.timer as i32).wrapping_sub(g.dt) as i16;
                c.timer = t;
                if t <= 0 {
                    loop {
                        let u = (w.rng_byte(g.rng) / 0x56) as i32;
                        g.rng = g.rng.wrapping_add(1);
                        if u == c.look as i32 || (u == 1 && col == 4) || (u == 2 && col == 0) {
                            continue;
                        }
                        c.look = u as i8;
                        break;
                    }
                    c.timer = w.rng_byte(g.rng) as i16 + 0x3C;
                    g.rng = g.rng.wrapping_add(1);
                }
            }
            let target = match c.look {
                1 => -0xC000,
                2 => 0xC000,
                _ => 0,
            };
            crate::update::sway_springs_raw(&mut c.sway, g, w, target, 0, -1);
            let q = |x: i32| div_pow2(x, 6);
            let (ta, tb) = (q(c.sway[1]), q(c.sway[7]));
            let (tc, td, te) = (q(c.sway[2]), q(c.sway[14]), q(c.sway[8]));
            for _ in 0..substeps(g.dt) {
                let step = |v: &mut i16, t: i32| *v = (*v as i32 + (t as i16 as i32 - *v as i32) / 4) as i16;
                step(&mut c.out[0], ta);
                step(&mut c.out[1], tb);
                step(&mut c.out[2], tc);
                step(&mut c.out[3], td);
                step(&mut c.out[4], te);
            }
        }
    }

    /// FUN_80073138 for game mode 0xD (new presses this frame): UP / DOWN
    /// move the row 0..4 with sound 0xE, LEFT / RIGHT the column (its sound
    /// comes from `logic`); any of them zeroes the idle count. Returns
    /// whether a row sound plays.
    pub fn input(&mut self, up: bool, down: bool, left: bool, right: bool) -> bool {
        let mut sound = false;
        if up {
            if self.row > 0 {
                self.row -= 1;
                sound = true;
            }
            self.idle = 0;
        } else if down {
            if self.row < 4 {
                self.row += 1;
                sound = true;
            }
            self.idle = 0;
        }
        if left {
            self.col -= 1;
            self.idle = 0;
        } else if right {
            self.col += 1;
            self.idle = 0;
        }
        sound
    }

    /// FUN_80074A94, game mode 0xD (once the screen has faded in): the
    /// column clamped (sound 0xE when it moved), CROSS / TRIANGLE, then the
    /// idle clip: while nothing plays (`xa_playing`: 0x800B0760 bit 0) the
    /// wait 0x800B074B counts down; at 0 with more than 0x3C idle VBlanks a
    /// collected character under the cursor other than the last one speaks:
    /// clip 0 for character 0xD, 1 for 0xE / 0x17, else a random byte >> 7;
    /// the wait starts again. CROSS on a collected character starts TEST
    /// DRIVE, with its clip when nothing plays (1 for 0xD, 0 for 0xE /
    /// 0x17, none for the others). `characters`: 0x800E02BC.
    pub fn logic(&mut self, g: &mut RaceGlobals, w: &World, cross: bool, back: bool, xa_playing: bool, characters: u32) -> GalleryOut {
        let mut out = GalleryOut::default();
        if cross || back {
            self.idle = 0;
        }
        if self.idle >= IDLE_OUT || (back && !cross) {
            out.leave = true;
            return out;
        }
        self.col = self.col.clamp(0, 4);
        out.nav = self.col != self.saved_col;
        self.saved_col = self.col;
        let have = |c: u8| characters & (1 << (c as u32 & 0x1F)) != 0;
        if cross {
            if let Some(c) = character_at(w, self.row, self.col).filter(|&c| have(c)) {
                out.test_drive = Some(c);
                if !xa_playing {
                    let n = match c {
                        0xD => Some(1),
                        0xE | 0x17 => Some(0),
                        _ => None,
                    };
                    out.clip = n.map(|n| (c, n));
                }
            }
        }
        if !xa_playing {
            self.clip_timer = (self.clip_timer - g.dt).max(0);
        }
        if let Some(c) = character_at(w, self.row, self.col) {
            if !xa_playing && self.clip_timer == 0 && self.idle > CLIP_WAIT && c as i8 != self.last_clip && have(c) && out.clip.is_none() {
                let n = match c {
                    0xD => 0,
                    0xE | 0x17 => 1,
                    _ => {
                        let b = w.rng_byte(g.rng) >> 7;
                        g.rng = g.rng.wrapping_add(1);
                        b
                    }
                };
                out.clip = Some((c, n));
                self.clip_timer = CLIP_WAIT;
                self.last_clip = c as i8;
            }
        }
        // FUN_8005C2EC: the idle count.
        self.idle += g.dt;
        out
    }

    /// Camera mode 6's inputs; `near`: the clip playing is the cursor's
    /// character's (0x800B0760 bit 0 and 0x800B074A); `pivot0_y(c)`: the
    /// character model's part 0 pivot y.
    pub fn camera(&self, w: &World, playing: bool, pivot0_y: &dyn Fn(u8) -> i32) -> GalleryCam {
        let c = character_at(w, self.row, self.col);
        GalleryCam {
            row: self.row,
            col: self.col,
            height: c.map_or(0, |c| height(w, c, pivot0_y(c))),
            near: playing && c.is_some_and(|c| c as i8 == self.last_clip),
        }
    }

    /// The characters FUN_80026A50 draws: within one row and one column of
    /// the cursor, as (index 5 * row + col, character, record).
    pub fn drawn(&self, w: &World, characters: u32) -> Vec<(usize, u8, Vec<u8>)> {
        let mut out = Vec::new();
        for row in 0..5i8 {
            for col in 0..5i8 {
                if (row - self.row).abs() >= 2 || (col - self.col).abs() >= 2 {
                    continue;
                }
                let Some(c) = character_at(w, row, col) else { continue };
                let i = row as usize * 5 + col as usize;
                out.push((i, c, record(w, &self.chars[i], c, characters & (1 << (c as u32 & 0x1F)) != 0)));
            }
        }
        out
    }
}

fn put_m(r: &mut [u8], o: usize, m: &Mat) {
    for i in 0..9 {
        r[o + 2 * i..o + 2 * i + 2].copy_from_slice(&m[i / 3][i % 3].to_le_bytes());
    }
}

/// The scratch kart record FUN_80026A50 fills for one character before
/// FUN_800276E8 draws it (0x800C49BC): position (and driver position) the
/// record's, colour factors +0x414.. 0x1000 (0 when not collected: black),
/// +0x454.. the smoothed outputs, no vehicle (+0x4A4 = -1), the character
/// (+0x4B2); driver matrix +0x1CC = FUN_800252A4(out1 / 2, out0 / 2), then
/// FUN_80086DF0: +0x29C = FUN_80024AB4(out2, out3 (halved for characters
/// 0xE and 0x20), out4), +0x2AE identity.
pub fn record(w: &World, ch: &GalleryChar, c: u8, collected: bool) -> Vec<u8> {
    let t = &w.tables;
    let mut r = vec![0u8; crate::kart::RECORD_SIZE];
    let p32 = |r: &mut Vec<u8>, o: usize, v: i32| r[o..o + 4].copy_from_slice(&v.to_le_bytes());
    for (i, v) in ch.pos.iter().enumerate() {
        p32(&mut r, 4 * i, *v);
        p32(&mut r, 0x330 + 8 * i, *v);
    }
    let tint: i16 = if collected { 0x1000 } else { 0 };
    for i in 0..6 {
        r[0x414 + 2 * i..0x416 + 2 * i].copy_from_slice(&tint.to_le_bytes());
    }
    for (i, v) in ch.out.iter().enumerate() {
        r[0x454 + 2 * i..0x456 + 2 * i].copy_from_slice(&v.to_le_bytes());
    }
    r[0x4A4] = 0xFF;
    r[0x4A5] = c;
    r[0x4B1] = 0xFF;
    r[0x4B2] = c;
    r[0x12..0x14].copy_from_slice(&0x4000u16.to_le_bytes());
    let half = |v: i16| ((v as i32 - ((v as i32) >> 31)) >> 1) & 0xFFF;
    let ident: Mat = [[0x1000, 0, 0], [0, 0x1000, 0], [0, 0, 0x1000]];
    let m = mul_matrix0(&ident, &rot_lean(t, half(ch.out[1]), half(ch.out[0])));
    put_m(&mut r, 0x1CC, &m);
    let mut d = ch.out[3] as i32;
    if c == 0x0E || c == 0x20 {
        d = (d - (d >> 31)) >> 1;
    }
    put_m(&mut r, 0x29C, &rot_xyz(t, ch.out[2] as i32 & 0xFFF, d & 0xFFF, ch.out[4] as i32 & 0xFFF));
    let id = w.exe.bytes(IDENTITY, 18);
    r[0x2AE..0x2AE + 18].copy_from_slice(&id);
    r
}

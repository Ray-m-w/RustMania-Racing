//! Which movie is which, and the movie player's rules (docs/spec/fmv.md).
//!
//! FMV.BIN is the movie player: an overlay FUN_8005CE60 loads to 0x800E3DD8
//! and calls at FUN_800E4618(movie). Its tables [CODE overlay 0x800E4618]:
//! - 0x800E7AF0: 31 name pointers ("fmv2\tt_15" etc.; ".str" appended);
//! - 0x800E7B6C: 31 i16 last frame numbers (the play loop ends there).
//!
//! FUN_800E4EB0(name, end, 3 (24-bit), x 0, y 0 (0x18 for 0x1D), 0,
//! skip_after (75 for 0x1C..0x1E, else 0)) plays one; its rules, per
//! displayed frame (frame number `f`, 0x800E7BAC):
//! - audio: CdMix level `f * 7` while f < 17 (fade in to 0x70), `(end - f)
//!   * 7` from end - 16 on; else 0x70; the SPU CD volume = the music level;
//! - picture: from end - 16 on, every byte >> (8 - (end - f) / 2) (the
//!   jump table at 0x800E3F8C over 0x800F8DB0: level n keeps the top n bits);
//! - skip: START or CROSS held (0x800B0DA8 & 0x4008) once f > skip_after and
//!   f + 16 < end: sound 0xF (FUN_8005EECC(0xF, 0x1800, 0x60, 0x60)), then
//!   11 frames of fade (count i = 1..11: CdMix (12 - i) * 112 / 12, level
//!   (12 - (i + 1)) * 8 / 12), then the player returns.

/// The overlay's load address and its tables.
pub const OVERLAY_BASE: u32 = 0x800E_3DD8;
pub const NAME_TABLE: u32 = 0x800E_7AF0;
pub const END_TABLE: u32 = 0x800E_7B6C;
pub const MOVIES: usize = 31;

/// The boot movies (main @0x80078964: 0x800B0804 = 0x1C at power-on, then
/// 0x1C -> 0x1D -> none; a 0x1E request plays 0x1E, 0x1C, 0x1D).
pub const BOOT: [u8; 2] = [0x1C, 0x1D];
/// The movies skippable only after frame 75.
pub fn skip_after(movie: u8) -> u32 {
    if (0x1C..=0x1E).contains(&movie) {
        75
    } else {
        0
    }
}
/// The picture's y offset in its 240-line area (0x1D, 320 x 192, is centred).
pub fn y_offset(movie: u8) -> u32 {
    if movie == 0x1D {
        0x18
    } else {
        0
    }
}

/// Frames of fade after a skip; frames of fade at the end.
pub const SKIP_FADE: u32 = 12;
pub const END_FADE: u32 = 16;
/// CdMix level while a movie plays.
pub const LEVEL: u8 = 0x70;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Movie {
    /// The overlay's name ("fmv2\\tt_15").
    pub name: String,
    /// The disc file ("FMV2/TT_15.STR").
    pub path: String,
    /// The last frame number (the play loop stops when it is reached).
    pub end: u32,
}

/// The 31 movies from FMV.BIN (read from the disc; nothing embedded).
pub fn table(fmv_bin: &[u8]) -> Vec<Movie> {
    let at = |a: u32| (a - OVERLAY_BASE) as usize;
    let rd32 = |a: u32| fmv_bin.get(at(a)..at(a) + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let rd16 = |a: u32| fmv_bin.get(at(a)..at(a) + 2).map(|b| i16::from_le_bytes([b[0], b[1]]));
    (0..MOVIES as u32)
        .filter_map(|i| {
            let p = rd32(NAME_TABLE + 4 * i)?;
            let s = fmv_bin.get(at(p)..)?;
            let name = String::from_utf8_lossy(&s[..s.iter().position(|&c| c == 0)?]).into_owned();
            let path = format!("{}.STR", name.replace('\\', "/").to_uppercase());
            Some(Movie { name, path, end: rd16(END_TABLE + 2 * i)?.max(0) as u32 })
        })
        .collect()
}

/// A course's movie (course table 0x800ACB34 row + 9, i8; -1 none).
pub fn course_movie(exe: &[u8], course: u8) -> Option<u8> {
    const T_ADDR: u32 = 0x8001_0000;
    let o = (0x800A_CB34 + 12 * course as u32 + 9 - T_ADDR) as usize + 0x800;
    exe.get(o).map(|&b| b as i8).filter(|&m| m >= 0).map(|m| m as u8)
}

/// The player's state machine (the overlay's loop), one call per displayed
/// frame. Says what the frame looks and sounds like and when to stop.
#[derive(Debug, Clone)]
pub struct Player {
    pub movie: u8,
    pub end: u32,
    /// Skip fade progress (0 = not skipping).
    skip: u32,
    pub done: bool,
}

/// One displayed frame's treatment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Look {
    /// Picture level 0..8 (8 = unchanged; n keeps a byte's top n bits).
    pub level: u8,
    /// CdMix level for the movie's audio.
    pub mix: u8,
}

impl Player {
    pub fn new(movie: u8, end: u32) -> Player {
        Player { movie, end, skip: 0, done: false }
    }

    /// Frame `f` is shown with START / CROSS `held`; returns its look and
    /// whether the confirm sound plays now.
    pub fn frame(&mut self, f: u32, held: bool) -> (Look, bool) {
        let mut sound = false;
        let look = if self.skip != 0 {
            // 0x800E5234: CdMix from the count before the step, the
            // picture level from the count after it.
            let mix = ((SKIP_FADE - self.skip) * 112 / SKIP_FADE) as u8;
            self.skip += 1;
            Look { level: ((SKIP_FADE - self.skip) * 8 / SKIP_FADE) as u8, mix }
        } else if f < 17 {
            Look { level: 8, mix: (f * 7) as u8 }
        } else if f + END_FADE >= self.end {
            let left = self.end.saturating_sub(f);
            Look { level: (left / 2).min(8) as u8, mix: (left * 7).min(LEVEL as u32) as u8 }
        } else {
            Look { level: 8, mix: LEVEL }
        };
        if self.skip == 0 && held && f > skip_after(self.movie) && f + END_FADE < self.end {
            self.skip = 1;
            sound = true;
        }
        if self.skip == SKIP_FADE || f >= self.end {
            self.done = true;
        }
        (look, sound)
    }
}

/// Apply a picture level to RGB bytes (the overlay's per-word masks).
pub fn darken(rgb: &mut [u8], level: u8) {
    if level >= 8 {
        return;
    }
    let sh = 8 - level as u32;
    for b in rgb {
        *b = if sh >= 8 { 0 } else { *b >> sh };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_disc_tables() {
        let Ok(mut disc) = mr_disc::Disc::open_default() else { return };
        let fmv = disc.read("FMV/FMV.BIN").unwrap();
        // the MDEC tables libpress uploads
        let q = crate::mdec::QUANT;
        assert_eq!(&fmv[0x3E0C..0x3E4C], &q[..]);
        assert_eq!(&fmv[0x3E4C..0x3E8C], &q[..]);
        let sc: Vec<i16> = fmv[0x3E90..0x3F10].chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
        assert_eq!(&sc[..], &crate::mdec::SCALE[..]);
        let t = table(&fmv);
        assert_eq!(t.len(), 31);
        assert_eq!(t[0x1C].path, "FMV2/HI_15.STR");
        assert_eq!(t[0x1D].path, "FMV2/TT_15.STR");
        assert_eq!(t[0x1D].end, 301);
        assert_eq!(t[5].path, "FMV/GMC1_15.STR");
        assert_eq!(t[5].end, 684);
        // every movie but 0x1E (fmv2\midway: no such file) is on the disc
        for (i, m) in t.iter().enumerate() {
            assert_eq!(disc.entry(&m.path).is_some(), i != 0x1E, "{}", m.path);
        }
        let exe = disc.read("SLUS_012.37").unwrap();
        assert_eq!(course_movie(&exe, 3), Some(19)); // DOCK: fmv2\mti2_15
        assert_eq!(course_movie(&exe, 0), None);
    }

    #[test]
    fn player_rules() {
        let mut p = Player::new(0x1C, 241);
        assert_eq!(p.frame(1, false).0, Look { level: 8, mix: 7 });
        assert_eq!(p.frame(16, false).0.mix, 112);
        // START before frame 76 does nothing on a boot movie
        assert!(!p.frame(75, true).1);
        assert!(p.frame(76, true).1);
        let mut n = 0;
        while !p.done {
            p.frame(77 + n, true);
            n += 1;
        }
        // 11 faded frames (counts 1 -> 2 .. 11 -> 12), the last at level 0
        assert_eq!(n, 11);
        let mut p = Player::new(19, 225);
        assert_eq!(p.frame(225 - 16, false).0, Look { level: 8, mix: 112 });
        assert_eq!(p.frame(224, false).0, Look { level: 0, mix: 7 });
        assert!(!p.done);
        p.frame(225, false);
        assert!(p.done);
        let mut b = [0xFF, 0x80];
        darken(&mut b, 4);
        assert_eq!(b, [0x0F, 0x08]);
    }
}

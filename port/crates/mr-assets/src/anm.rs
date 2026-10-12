//! anm: Muppet RaceMania `.ANM` files, the companion of every `.MDL`.
//!
//! An `.ANM` is a small header (four runtime halfwords and a table of
//! animation-block offsets) and one block: a per-part channel table and two
//! identical "frames" of per-part rest poses (pivot x4, two halfwords, and
//! an optional 4.12 scale triple). The game loads it first (FUN_800925a8),
//! uses the header as the model's slot range and scans the channel table
//! for -2 (neck part -> seam list); nothing in the game reads the poses
//! (xref census of the anm pointer table DAT_800dd598 and of the globals
//! FUN_80092458 sets). Wheel spin, steering and the driver's head/body
//! motion come from per-part matrices in the kart object instead
//! (FUN_80026f3c / FUN_800276e8 read kart +0x20C + 0x12*part); see ANM.md.
//!
//! Behaviour reproduced: FUN_800925a8 (load, runtime header fields),
//! FUN_80092458 (offset relocation, header size rule, -2 scan).
//! Verified against the running recomp: KERMIT.ANM and RACER.ANM in RAM at
//! DAT_800dd598[7] / [33] equal the file except the runtime fields
//! (+0 = 1, +4/+6 = first/end model slot, offsets relocated to pointers).
//! Spec: docs/spec/formats/ANM.md.

use crate::le;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnmError {
    Truncated { at: usize, need: usize, len: usize },
    /// Block header with a non-negative first halfword (12-byte header
    /// variant of FUN_80092458; no file on disc uses it).
    ShortHeader { at: usize },
    /// Channel offsets / record sizes inconsistent with the scale mask.
    Channels { at: usize, reason: &'static str },
    /// Bytes left over after the last block.
    Trailing { at: usize, len: usize },
}

impl std::fmt::Display for AnmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for AnmError {}

fn need(b: &[u8], at: usize, n: usize) -> Result<(), AnmError> {
    if at.checked_add(n).map_or(true, |e| e > b.len()) {
        Err(AnmError::Truncated { at, need: n, len: b.len() })
    } else {
        Ok(())
    }
}

/// Channel table entry of one model part.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    /// Offset of the part's pose record in each frame, in halfwords.
    Pose(u16),
    /// -2: the part is a neck stretched between two seam rings; its model
    /// slot is appended to the seam list DAT_800d3dd0 [CODE 80092458].
    Seam,
}

/// One part's pose record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pose {
    /// Pivot in quarter units: equals 4 * the MDL record pivot for every
    /// model on disc [data] (RAM slot pivots match the MDL [RAM]).
    pub pos4: [i16; 3],
    /// Two halfwords; first always 0, second 0 or 1168 (wheel/rotor parts
    /// of DUMPER, TRIKE, ROLLER) [INFER: angle, 4096 = 360 deg].
    pub rot: [i16; 2],
    /// Present when the part's bit is set in `scale_mask`: 4.12 scale
    /// (RACER rear wheels 3522, LOVEBUG 4096/3540/3540) [INFER].
    pub scale: Option<[i16; 3]>,
}

impl Pose {
    /// Pivot in model units (pos4 / 4).
    pub fn pivot(&self) -> [i32; 3] {
        [self.pos4[0] as i32 / 4, self.pos4[1] as i32 / 4, self.pos4[2] as i32 / 4]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// File offset of the block.
    pub offset: usize,
    /// Header halfwords h[0..8]: h[0] = -(header bytes) = -16; h[1] = 3,
    /// h[2] = 2 [INFER unknown]; h[3] = part count; h[4] = frame length in
    /// halfwords; h[5] = part count (the one the loader reads); h[6] =
    /// frame count (2); h[7] = -1 [INFER terminator].
    pub header: [i16; 8],
    pub channels: Vec<Channel>,
    /// Halfword after the channel table: bit i set = part i's record has a
    /// scale triple (8 halfwords instead of 5) [data: holds for all 65].
    pub scale_mask: u16,
    /// `frame_count` frames of per-part poses (None for seam parts).
    pub frames: Vec<Vec<Option<Pose>>>,
}

impl Block {
    pub fn part_count(&self) -> usize {
        self.header[5] as usize
    }
    pub fn frame_len_hw(&self) -> usize {
        self.header[4] as usize
    }
    pub fn frame_count(&self) -> usize {
        self.header[6] as usize
    }
    /// Parts whose channel is `Seam`.
    pub fn seam_parts(&self) -> Vec<usize> {
        self.channels.iter().enumerate().filter(|(_, c)| **c == Channel::Seam).map(|(i, _)| i).collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Anm {
    /// +0: 0 on disc; the loader stores 1 [CODE 800925a8].
    pub loaded_flag: u16,
    /// +2 number of block offsets.
    pub block_slots: u16,
    /// +4 / +6: 0 on disc; the loader stores the model's first slot and
    /// end slot (first + MDL slot count) [CODE, RAM].
    pub first_slot: u16,
    pub end_slot: u16,
    /// +8: u32 offset of each block from the file start, 0 = none. The
    /// loader relocates them to pointers in place [CODE 80092458].
    pub block_offsets: Vec<u32>,
    pub blocks: Vec<Block>,
}

impl Anm {
    pub fn parse(b: &[u8]) -> Result<Anm, AnmError> {
        need(b, 0, 8)?;
        let block_slots = le::u16(b, 2);
        need(b, 8, block_slots as usize * 4)?;
        let block_offsets: Vec<u32> = (0..block_slots as usize).map(|i| le::u32(b, 8 + i * 4)).collect();
        let mut end = 8 + block_slots as usize * 4;
        let mut blocks = Vec::new();
        let mut offs: Vec<usize> = block_offsets.iter().filter(|&&o| o != 0).map(|&o| o as usize).collect();
        offs.sort();
        for o in offs {
            if o < end {
                return Err(AnmError::Channels { at: o, reason: "block overlaps previous data" });
            }
            if o != end {
                return Err(AnmError::Trailing { at: end, len: o - end });
            }
            let (blk, used) = parse_block(b, o)?;
            blocks.push(blk);
            end = o + used;
        }
        if end != b.len() {
            return Err(AnmError::Trailing { at: end, len: b.len() - end });
        }
        Ok(Anm {
            loaded_flag: le::u16(b, 0),
            block_slots,
            first_slot: le::u16(b, 4),
            end_slot: le::u16(b, 6),
            block_offsets,
            blocks,
        })
    }

    /// The block FUN_80092458 uses: the first non-zero offset in table order.
    pub fn active_block(&self) -> Option<&Block> {
        let o = *self.block_offsets.iter().find(|&&o| o != 0)? as usize;
        self.blocks.iter().find(|b| b.offset == o)
    }
}

fn parse_block(b: &[u8], o: usize) -> Result<(Block, usize), AnmError> {
    need(b, o, 2)?;
    let h0 = le::i16(b, o);
    if h0 >= 0 {
        return Err(AnmError::ShortHeader { at: o });
    }
    let hsize = (-(h0 as i32)) as usize;
    if hsize != 16 {
        return Err(AnmError::Channels { at: o, reason: "header size other than 16" });
    }
    need(b, o, hsize)?;
    let mut header = [0i16; 8];
    for (i, h) in header.iter_mut().enumerate() {
        *h = le::i16(b, o + 2 * i);
    }
    let nparts = header[5].max(0) as usize;
    let flen = header[4].max(0) as usize;
    let nframes = header[6].max(0) as usize;
    let tab = o + hsize;
    need(b, tab, nparts * 2 + 2)?;
    let mut channels = Vec::with_capacity(nparts);
    for i in 0..nparts {
        let v = le::i16(b, tab + 2 * i);
        channels.push(match v {
            -2 => Channel::Seam,
            v if v >= 0 => Channel::Pose(v as u16),
            _ => return Err(AnmError::Channels { at: tab + 2 * i, reason: "negative channel other than -2" }),
        });
    }
    let mask_at = tab + nparts * 2;
    let scale_mask = le::u16(b, mask_at);
    let frames_at = mask_at + 2;
    need(b, frames_at, nframes * flen * 2)?;
    // record extents: sorted pose offsets, last runs to the frame end
    let mut starts: Vec<(usize, usize)> = channels
        .iter()
        .enumerate()
        .filter_map(|(i, c)| match c {
            Channel::Pose(off) => Some((*off as usize, i)),
            Channel::Seam => None,
        })
        .collect();
    starts.sort();
    let mut extent = vec![(0usize, 0usize); nparts];
    for (k, &(off, part)) in starts.iter().enumerate() {
        let e = starts.get(k + 1).map(|s| s.0).unwrap_or(flen);
        let len = e.checked_sub(off).ok_or(AnmError::Channels { at: tab, reason: "offsets out of order" })?;
        let want = if scale_mask & (1 << part) != 0 { 8 } else { 5 };
        if len != want {
            return Err(AnmError::Channels { at: tab, reason: "record length disagrees with scale mask" });
        }
        extent[part] = (off, len);
    }
    if starts.first().map_or(flen != 0, |s| s.0 != 0) {
        return Err(AnmError::Channels { at: tab, reason: "frame not tiled by records" });
    }
    let frames = (0..nframes)
        .map(|f| {
            let base = frames_at + f * flen * 2;
            channels
                .iter()
                .enumerate()
                .map(|(i, c)| match c {
                    Channel::Seam => None,
                    Channel::Pose(_) => {
                        let (off, len) = extent[i];
                        let h = |k: usize| le::i16(b, base + (off + k) * 2);
                        Some(Pose {
                            pos4: [h(0), h(1), h(2)],
                            rot: [h(3), h(4)],
                            scale: if len == 8 { Some([h(5), h(6), h(7)]) } else { None },
                        })
                    }
                })
                .collect()
        })
        .collect();
    let used = frames_at + nframes * flen * 2 - o;
    Ok((Block { offset: o, header, channels, scale_mask, frames }, used))
}

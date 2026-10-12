//! raw: Muppet RaceMania `.RAW` texture containers (`COURSE_x/<track>.RAW`,
//! `ARENA/*.RAW`, `STUNT/*.RAW`, `FRONTEND/*.RAW`, `GFX/*.RAW`).
//!
//! A `.RAW` file is a list of RNC ProPack method-2 packed chunks ended by
//! `FF FF FF FF FF FF FF FF`. Each unpacked chunk starts with a u32 LE type:
//! types below 0x20 are texture pages (palette + 4/8-bit texels) that the
//! loader uploads to VRAM together with up to five derived CLUT rows; 0x20,
//! 0x24, 0x25 and the first 0x28..0x2F / 0x58..0x5F chunk are full-screen
//! images; 0x22 holds animated-texture frames that stay in RAM; other types
//! are kept in RAM for other systems.
//!
//! Behaviour reproduced: the track loader FUN_8004627c passes "<course>.raw"
//! to FUN_800441d8 (whole-file mode, flag 0x800 of DAT_800b0cf8 clear), which
//! unpacks each chunk with FUN_800203f0 (RNC2), reduces types 0..3 in place
//! with FUN_80044008, builds CLUTs with FUN_80045c48 (blend toward a colour)
//! and FUN_80045f1c (desaturate per 16-entry row), and LoadImage()s them.
//!
//! Verified against the running recomp (DOCK, 1-player race): every page and
//! CLUT rectangle this module produces matches VRAM 100%, except the five
//! 64x64/32x16/... sub-rectangles that the game overwrites each frame with
//! frames of the 0x22 animation chunks; the 0x22 chunks match RAM byte for
//! byte; the slot table matches 0x800dff00. See docs/spec/formats/RAW.md.

use crate::le;
use crate::vram::Vram;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawError {
    /// Input ended inside a chunk header or packed stream.
    Truncated { at: usize },
    /// RNC header length / CRC check failed.
    Crc { chunk_offset: usize, which: &'static str },
    /// Unpacked size disagrees with the header.
    Size { chunk_offset: usize, expected: usize, got: usize },
    /// An LZ back-reference pointed before the start of the output.
    BadOffset { chunk_offset: usize },
    /// A chunk too short for the fields its type requires.
    ShortChunk { chunk_offset: usize, kind: u32 },
}

impl std::fmt::Display for RawError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for RawError {}

// ---------------------------------------------------------------- RNC2 ----

/// CRC-16 used by RNC ProPack (reflected poly 0xA001, init 0).
pub fn rnc_crc16(data: &[u8]) -> u16 {
    let mut c: u16 = 0;
    for &b in data {
        c ^= b as u16;
        for _ in 0..8 {
            c = if c & 1 != 0 { (c >> 1) ^ 0xA001 } else { c >> 1 };
        }
    }
    c
}

/// The 14-byte chunk header: an RNC ProPack header without its "RNC\x02"
/// signature. Big-endian fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RncHeader {
    pub unpacked_len: u32,
    pub packed_len: u32,
    pub unpacked_crc: u16,
    pub packed_crc: u16,
    /// RNC "leeway" (in-place unpack safety margin). Not used by the game.
    pub leeway: u8,
    /// RNC pack-chunk count. Not used by the game.
    pub pack_chunks: u8,
}

pub const RNC_HEADER_LEN: usize = 14;

impl RncHeader {
    pub fn parse(b: &[u8], at: usize) -> Result<Self, RawError> {
        if b.len() < at + RNC_HEADER_LEN {
            return Err(RawError::Truncated { at });
        }
        let be32 = |o: usize| u32::from_be_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
        let be16 = |o: usize| u16::from_be_bytes([b[o], b[o + 1]]);
        Ok(RncHeader {
            unpacked_len: be32(at),
            packed_len: be32(at + 4),
            unpacked_crc: be16(at + 8),
            packed_crc: be16(at + 10),
            leeway: b[at + 12],
            pack_chunks: b[at + 13],
        })
    }
}

struct Bits<'a> {
    src: &'a [u8],
    pos: usize,
    /// 8-bit shift register with a sentinel 1 below the remaining bits
    /// (FUN_800203f0 keeps it the same way: `add`, refill on zero).
    buf: u8,
}

impl<'a> Bits<'a> {
    fn byte(&mut self) -> Result<u8, RawError> {
        let b = *self.src.get(self.pos).ok_or(RawError::Truncated { at: self.pos })?;
        self.pos += 1;
        Ok(b)
    }
    fn bit(&mut self) -> Result<u32, RawError> {
        let mut c = (self.buf >> 7) as u32;
        self.buf <<= 1;
        if self.buf == 0 {
            let b = self.byte()?;
            c = (b >> 7) as u32;
            self.buf = (b << 1) | 1;
        }
        Ok(c)
    }
    fn bits(&mut self, n: u32) -> Result<u32, RawError> {
        let mut v = 0;
        for _ in 0..n {
            v = (v << 1) | self.bit()?;
        }
        Ok(v)
    }
    /// RNC2 match offset: optional high byte (Huffman-like prefix) + low byte, +1.
    fn offset(&mut self) -> Result<usize, RawError> {
        let mut hi = 0u32;
        if self.bit()? != 0 {
            hi = self.bit()?;
            if self.bit()? != 0 {
                hi = ((hi << 1) | self.bit()?) | 4;
                if self.bit()? == 0 {
                    hi = (hi << 1) | self.bit()?;
                }
            } else if hi == 0 {
                hi = self.bit()? + 2;
            }
        }
        Ok((((hi << 8) | self.byte()? as u32) + 1) as usize)
    }
}

/// Unpack one RNC method-2 chunk starting at `at` (its 14-byte header).
/// Returns the unpacked bytes and the header. Both CRCs and the length are
/// checked (the game checks none of them).
pub fn unpack_rnc2(file: &[u8], at: usize) -> Result<(Vec<u8>, RncHeader), RawError> {
    let h = RncHeader::parse(file, at)?;
    let start = at + RNC_HEADER_LEN;
    let end = start + h.packed_len as usize;
    if file.len() < end {
        return Err(RawError::Truncated { at: file.len() });
    }
    if rnc_crc16(&file[start..end]) != h.packed_crc {
        return Err(RawError::Crc { chunk_offset: at, which: "packed" });
    }
    let mut out: Vec<u8> = Vec::with_capacity(h.unpacked_len as usize);
    let mut r = Bits { src: &file[..end], pos: start, buf: 0 };
    r.bits(2)?; // two leading "lock/key" bits, ignored
    loop {
        if r.bit()? == 0 {
            out.push(r.byte()?);
            continue;
        }
        let (len, off);
        if r.bit()? == 0 {
            // 10: length 4..9; 9 = literal run
            let mut l = 4 + r.bit()?;
            if r.bit()? != 0 {
                l = ((l - 1) << 1) + r.bit()?;
                if l == 9 {
                    let n = ((r.bits(4)? + 3) * 4) as usize;
                    let s = r.pos;
                    let lit = file.get(s..s + n).filter(|_| s + n <= end).ok_or(RawError::Truncated { at: s })?;
                    out.extend_from_slice(lit);
                    r.pos += n;
                    continue;
                }
            }
            len = l as usize;
            off = r.offset()?;
        } else if r.bit()? == 0 {
            // 110: length 2, one-byte offset
            len = 2;
            off = r.byte()? as usize + 1;
        } else if r.bit()? == 0 {
            // 1110: length 3
            len = 3;
            off = r.offset()?;
        } else {
            // 1111: length byte + 8; byte 0 = end-of-pack-chunk marker
            let b = r.byte()? as usize;
            if b == 0 {
                if r.bit()? == 0 {
                    break;
                }
                continue;
            }
            len = b + 8;
            off = r.offset()?;
        }
        if off > out.len() {
            return Err(RawError::BadOffset { chunk_offset: at });
        }
        for _ in 0..len {
            let v = out[out.len() - off];
            out.push(v);
        }
    }
    if out.len() != h.unpacked_len as usize {
        return Err(RawError::Size { chunk_offset: at, expected: h.unpacked_len as usize, got: out.len() });
    }
    if rnc_crc16(&out) != h.unpacked_crc {
        return Err(RawError::Crc { chunk_offset: at, which: "unpacked" });
    }
    Ok((out, h))
}

// -------------------------------------------------------------- chunks ----

/// One unpacked chunk of a `.RAW` file.
#[derive(Debug, Clone)]
pub struct Chunk {
    /// Byte offset of the chunk's 14-byte header in the file.
    pub file_offset: usize,
    pub header: RncHeader,
    /// u32 LE at offset 0 of the unpacked data.
    pub kind: u32,
    /// Unpacked bytes, including the type word.
    pub data: Vec<u8>,
}

/// The container: chunks, then the 8-byte FF terminator, then padding.
#[derive(Debug, Clone)]
pub struct RawFile {
    pub chunks: Vec<Chunk>,
    /// Offset of the `FF FF FF FF` terminator (the game stops at its first
    /// four bytes; every file on the disc has eight).
    pub terminator: usize,
    /// Bytes after the 8-byte terminator (zero padding on the disc).
    pub trailing: usize,
}

pub fn parse(file: &[u8]) -> Result<RawFile, RawError> {
    let mut chunks = Vec::new();
    let mut at = 0;
    loop {
        if file.len() < at + 4 {
            return Err(RawError::Truncated { at });
        }
        if file[at..at + 4] == [0xFF; 4] {
            let trailing = file.len().saturating_sub(at + 8);
            return Ok(RawFile { chunks, terminator: at, trailing });
        }
        let (data, header) = unpack_rnc2(file, at)?;
        if data.len() < 4 {
            return Err(RawError::ShortChunk { chunk_offset: at, kind: 0 });
        }
        let kind = le::u32(&data, 0);
        chunks.push(Chunk { file_offset: at, header, kind, data });
        at += RNC_HEADER_LEN + header.packed_len as usize;
    }
}

// ------------------------------------------------------------- loading ----

/// Inputs the loader reads from game state besides the file.
#[derive(Debug, Clone, Copy)]
pub struct LoadEnv {
    /// DAT_800b0fac, the course index (DOCK = 3 [RAM]). Course 0x1e skips
    /// the FUN_80044008 reduction of page types 0..3.
    pub course: u32,
    /// Course colour from the 12-byte table at 0x800acb34 (+0,+1,+2).
    pub fog_rgb: [u8; 3],
    /// u16 at +6 of the same entry; bit 0x10 selects desaturated CLUTs
    /// (FUN_80045f1c) instead of fog-blended ones (FUN_80045c48).
    pub course_flags: u16,
    /// Bit 0x400 of DAT_800b0cf8: type-0x20 images go to the second cursor.
    pub image_to_second_cursor: bool,
    /// DAT_800d89b0[256]: per-CLUT-entry high byte (the loader fills it
    /// with 0x80 = semi-transparency bit before decoding).
    pub stp: [u8; 256],
}

pub const DOCK_COURSE: u32 = 3;
/// RAM address of the course table read by FUN_800441d8.
pub const COURSE_TABLE_ADDR: u32 = 0x800a_cb34;
pub const COURSE_TABLE_STRIDE: u32 = 12;

impl LoadEnv {
    pub fn new(course: u32, fog_rgb: [u8; 3], course_flags: u16) -> Self {
        LoadEnv { course, fog_rgb, course_flags, image_to_second_cursor: false, stp: [0x80; 256] }
    }

    /// Read the course colour/flags from the PS-X EXE (SLUS_012.37 on disc,
    /// t_addr 0x80010000, 0x800-byte header). The runtime sets bit 0x0800
    /// of the flags (and 0x80 in byte +9) after load; bit 0x10 and the
    /// colour are static [RAM].
    pub fn from_exe(exe: &[u8], course: u32) -> Option<Self> {
        let off = (COURSE_TABLE_ADDR + course * COURSE_TABLE_STRIDE) as usize - 0x8001_0000 + 0x800;
        let e = exe.get(off..off + 12)?;
        Some(Self::new(course, [e[0], e[1], e[2]], le::u16(e, 6)))
    }
}

/// RAM address of FUN_8004627c's course-name pointer table
/// (`PTR_s_course_c_speedway_800acaa0`), indexed by DAT_800b0fac.
pub const COURSE_NAME_TABLE_ADDR: u32 = 0x800a_caa0;
/// Course index whose `.RAW` comes from `frontend\mem_0<n>.raw` instead.
pub const FRONTEND_COURSE: u32 = 10;

/// Disc path of course `course`'s `.RAW` ("COURSE_A/DOCK.RAW" for 3), read
/// from the exe's name table [CODE 8004627c]. None for the frontend (10)
/// or an index outside the table.
pub fn course_raw_path(exe: &[u8], course: u32) -> Option<String> {
    let at = |addr: u32| (addr as usize).checked_sub(0x8001_0000).map(|o| o + 0x800);
    if course == FRONTEND_COURSE || course > 36 {
        return None;
    }
    let p = le::u32(exe, at(COURSE_NAME_TABLE_ADDR + course * 4)?);
    let s = exe.get(at(p)?..)?;
    let n = s.iter().position(|&b| b == 0)?;
    let name = std::str::from_utf8(&s[..n]).ok()?;
    Some(format!("{}.RAW", name.replace('\\', "/").to_ascii_uppercase()))
}

/// What an upload came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadKind {
    /// Texels of page chunk `kind` (types < 0x20, also 0x24 / 0x58..0x5F).
    Page,
    /// CLUT row `variant` (0..4) of chunk `kind`: 0 = base, 1 = second
    /// base copy, 2..4 = fog/desaturated variants (types 0..5 only).
    Clut { variant: u8 },
    /// Direct 15-bit image (types 0x20, 0x25, 0x28..0x2F).
    Image,
}

/// One LoadImage() the loader performs.
#[derive(Debug, Clone)]
pub struct Upload {
    pub chunk_index: usize,
    pub kind: u32,
    pub what: UploadKind,
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
    /// w*h halfwords, row-major.
    pub data: Vec<u16>,
}

/// Entry of the 16-byte-per-slot table at 0x800dff00 (slot = page type,
/// 0xc for type 0x24, 5 for 0x58..0x5F).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TexSlot {
    /// GetTPage(mode, 0, x, y); None when the page is 15-bit (the loader
    /// leaves the field untouched then).
    pub tpage: Option<u16>,
    /// CLUT attribute: (row << 6) | (0x300 >> 4).
    pub clut: u16,
    pub x: i16,
    pub y: i16,
    /// 0 = 4-bit, 1 = 8-bit; None for 15-bit (field untouched).
    pub mode: Option<u16>,
}

/// A 0x22 chunk: animated-texture frames kept in RAM (pointer stored in
/// DAT_800bbb20[slot]); something else copies one frame at a time to VRAM.
#[derive(Debug, Clone)]
pub struct AnimStrip {
    pub chunk_index: usize,
    pub slot: u32,
    /// Texture size in texels (u16 at +8, +10).
    pub w: u16,
    pub h: u16,
    /// Frames as VRAM halfwords, assuming 4-bit texels (w/4 halfwords per
    /// row) [INFER: every frame found in VRAM was 4-bit sized].
    pub frames: Vec<Vec<u16>>,
}

/// A chunk the loader keeps in its work buffer (not uploaded, or uploaded
/// and also kept), in buffer order.
#[derive(Debug, Clone)]
pub struct Retained {
    pub chunk_index: usize,
    pub kind: u32,
    /// Byte offset from the work buffer start (DAT_800b0c14 at entry).
    pub buffer_offset: usize,
    /// Bytes as left in RAM (type 0x21 is adjusted in place).
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Default)]
pub struct Decoded {
    pub uploads: Vec<Upload>,
    /// Index = slot 0..31 (0x800dff00 + 16*slot).
    pub slots: [Option<TexSlot>; 32],
    pub anims: Vec<AnimStrip>,
    pub retained: Vec<Retained>,
}

impl Decoded {
    /// Apply every upload in order, like the game's LoadImage() sequence.
    pub fn apply(&self, vram: &mut Vram) {
        apply(&self.uploads, vram);
    }
}

/// Apply uploads to a VRAM model in order.
pub fn apply(uploads: &[Upload], vram: &mut Vram) {
    for u in uploads {
        vram.load_image(u.x as usize, u.y as usize, u.w as usize, u.h as usize, &u.data);
    }
}

/// PSYQ GetTPage(tp, abr, x, y).
pub fn get_tpage(tp: u16, abr: u16, x: i16, y: i16) -> u16 {
    let (x, y) = (x as u16, y as u16);
    ((tp & 3) << 7) | ((abr & 3) << 5) | ((y & 0x100) >> 4) | ((x & 0x3FF) >> 6) | ((y & 0x200) << 2)
}

fn isqrt(n: u32) -> u32 {
    let mut x = (n as f64).sqrt() as u32;
    while x * x > n {
        x -= 1;
    }
    while (x + 1) * (x + 1) <= n {
        x += 1;
    }
    x
}

fn is_key(r: u8, g: u8, b: u8) -> bool {
    r == 0 && g >= 0xFB && b == 0
}

/// FUN_80045c48: 256 RGB triples -> 256 CLUT entries, blended toward
/// `rgb` by sqrt(a * (a/4 + 192)) / 255 (a = 0: plain conversion).
/// (0,>=FB,0) is the transparent key -> 0x0000; black -> 0x0001.
pub fn clut_blend(pal: &[u8], amount: i32, rgb: [u8; 3], stp: &[u8; 256]) -> Vec<u16> {
    let a = amount.max(0);
    let t = isqrt((a * ((a >> 2) + 0xC0)) as u32) as i32;
    let it = 0xFF - t;
    (0..256)
        .map(|i| {
            let (r, g, b) = (pal[i * 3], pal[i * 3 + 1], pal[i * 3 + 2]);
            if is_key(r, g, b) {
                return 0;
            }
            let c = if t == 0 {
                (r as i32 >> 3) + ((g as i32 >> 3) << 5) + ((b as i32 >> 3) << 10)
            } else {
                let ch = |f: u8, v: u8| (f as i32 * t + v as i32 * it + 0x7F) / 0x7F8;
                ch(rgb[0], r) + (ch(rgb[1], g) << 5) + (ch(rgb[2], b) << 10)
            };
            let c = if c == 0 { 1 } else { c as u16 };
            c | ((stp[i] as u16) << 8)
        })
        .collect()
}

/// FUN_80045f1c: per 16-entry row, move each colour toward the row's mean
/// (keys excluded) by `amount`/256.
pub fn clut_desaturate(pal: &[u8], amount: i32, stp: &[u8; 256]) -> Vec<u16> {
    let mut out = vec![0u16; 256];
    for row in 0..16 {
        let p = &pal[row * 48..row * 48 + 48];
        let (mut n, mut s) = (0i32, [0i32; 3]);
        for i in 0..16 {
            let c = &p[i * 3..i * 3 + 3];
            if !is_key(c[0], c[1], c[2]) {
                n += 1;
                for k in 0..3 {
                    s[k] += c[k] as i32;
                }
            }
        }
        if n != 0 {
            for k in 0..3 {
                s[k] /= n; // C division, operands non-negative
            }
        }
        for i in 0..16 {
            let c = &p[i * 3..i * 3 + 3];
            let idx = row * 16 + i;
            if is_key(c[0], c[1], c[2]) {
                out[idx] = 0;
                continue;
            }
            let ch = |k: usize| {
                let mut x = (s[k] - c[k] as i32) * amount;
                if x < 0 {
                    x += 0xFF;
                }
                ((c[k] as i32 + (x >> 8)) as i16 as i32) >> 3
            };
            let v = ch(0) + (ch(1) << 5) + (ch(2) << 10);
            let v = if v == 0 { 1 } else { v };
            out[idx] = (v as u16) | ((stp[idx] as u16) << 8);
        }
    }
    out
}

/// FUN_80044008: in-place reduction applied to page types 0..3 (`w`, `h`
/// in texels, `mode` 0 = 4-bit, else 8-bit). Rows 0..h/2 get every other
/// texel of every other row; rows 0..h/4 then get every fourth texel of
/// every fourth (reduced-buffer) row; the rest of the page stays as stored.
pub fn reduce_in_place(px: &mut [u8], w: i32, h: i32, mode: u16) {
    let (mut i, mut o) = (0usize, 0usize);
    if mode == 0 {
        let mut r = 0;
        while r < h {
            let mut x = 0;
            while x < w {
                px[o] = (px[i] & 0xF).wrapping_add(px[i + 1] << 4);
                o += 1;
                i += 2;
                x += 4;
            }
            i += (w / 2) as usize;
            o += (w / 4) as usize;
            r += 2;
        }
        let (mut i, mut o) = (0usize, 0usize);
        let mut r = 0;
        while r < h {
            let mut x = 0;
            while x < w {
                px[o] = (px[i] & 0xF).wrapping_add(px[i + 2] << 4);
                o += 1;
                i += 4;
                x += 8;
            }
            o += ((w * 3) >> 3) as usize;
            i += (w * 3 / 2) as usize;
            r += 4;
        }
    } else {
        let mut r = 0;
        while r < h {
            let mut x = 0;
            while x < w {
                px[o] = px[i];
                o += 1;
                i += 2;
                x += 2;
            }
            i += w as usize;
            o += (w / 2) as usize;
            r += 2;
        }
        let (mut i, mut o) = (0usize, 0usize);
        let mut r = 0;
        while r < h {
            let mut x = 0;
            while x < w {
                px[o] = px[i];
                o += 1;
                i += 4;
                x += 4;
            }
            i += (w * 3) as usize;
            o += ((w * 3) >> 2) as usize;
            r += 4;
        }
    }
}

fn halfwords(b: &[u8], off: usize, n: usize) -> Option<Vec<u16>> {
    let s = b.get(off..off + n * 2)?;
    Some(s.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect())
}

#[derive(Clone, Copy)]
struct Rect {
    x: i16,
    y: i16,
}

/// Texel-width -> halfword-width per page mode, as the loader computes it.
fn page_hw(w: i16, mode: u16) -> i16 {
    match mode & 3 {
        0 => w / 4, // C: (w + (w<0?3:0)) >> 2
        1 => w / 2,
        _ => w,
    }
}

/// Decode a whole `.RAW` file the way FUN_800441d8 does in whole-file mode
/// (track/arena/stunt loads). Returns every VRAM upload in order plus the
/// RAM-side results.
pub fn decode(file: &[u8], env: &LoadEnv) -> Result<Decoded, RawError> {
    let raw = parse(file)?;
    decode_chunks(&raw.chunks, env)
}

pub fn decode_chunks(chunks: &[Chunk], env: &LoadEnv) -> Result<Decoded, RawError> {
    let mut d = Decoded::default();
    let mut a = Rect { x: 0, y: 0 }; // RStack_70
    let mut b = Rect { x: 0x280, y: 0x100 }; // RStack_68
    let mut clut_row: i16 = 0xFF; // uStack_50
    let mut image_state = 0u8; // DAT_800b0c1a
    let mut buf_off = 0usize; // work-buffer cursor (piVar20 - start)
    let stp = &env.stp;

    for (ci, c) in chunks.iter().enumerate() {
        let t = c.kind;
        let mut data = c.data.clone();
        let short = || RawError::ShortChunk { chunk_offset: c.file_offset, kind: t };
        macro_rules! need {
            ($n:expr) => {
                if data.len() < $n {
                    return Err(short());
                }
            };
        }
        let mut keep = false;
        let push = |d: &mut Decoded, what, x: i16, y: i16, w: i16, h: i16, px: Vec<u16>| {
            d.uploads.push(Upload { chunk_index: ci, kind: t, what, x: x as u16, y: y as u16, w: w as u16, h: h as u16, data: px })
        };

        if t < 0x20 {
            need!(12);
            let w = le::i16(&data, 4);
            let mut h = le::i16(&data, 6);
            let pal_len = le::u16(&data, 8) as i16;
            let mode = le::u16(&data, 10);
            let pix = 0xC + (pal_len / 2) as usize * 2;
            need!(0xC + 0x300);
            if t < 4 && env.course != 0x1E {
                let n = data.len() - pix;
                reduce_in_place(&mut data[pix..pix + n], w as i32, h as i32, mode);
            }
            let pal = data[0xC..0xC + 0x300].to_vec();
            let clut = |d: &mut Decoded, row: i16, variant: u8, v: Vec<u16>| {
                d.uploads.push(Upload { chunk_index: ci, kind: t, what: UploadKind::Clut { variant }, x: 0x300, y: row as u16, w: 256, h: 1, data: v })
            };
            if t == 0xD {
                clut(&mut d, 0xC0, 0, clut_blend(&pal, 0, [0x38; 3], stp));
            } else if t > 5 {
                clut(&mut d, clut_row, 0, clut_blend(&pal, 0, [0x38; 3], stp));
                clut_row -= 1;
            } else {
                let ti = t as i16;
                clut(&mut d, ti + 0xC1, 0, clut_blend(&pal, 0, [0, 0x38, 0], stp));
                clut(&mut d, ti + 199, 1, clut_blend(&pal, 0, [0, 0x38, 0], stp));
                if env.course_flags & 0x10 == 0 {
                    clut(&mut d, ti + 0xCD, 2, clut_blend(&pal, 0x40, env.fog_rgb, stp));
                    clut(&mut d, ti + 0xD3, 3, clut_blend(&pal, 0x80, env.fog_rgb, stp));
                    clut(&mut d, ti + 0xD9, 4, clut_blend(&pal, 0xC0, env.fog_rgb, stp));
                } else {
                    clut(&mut d, ti + 0xCD, 2, clut_desaturate(&pal, 0x80, stp));
                    clut(&mut d, ti + 0xD3, 3, clut_desaturate(&pal, 0xE0, stp));
                }
            }
            // slot table entry
            let slot_i = t as usize;
            let mut s = d.slots[slot_i].unwrap_or_default();
            s.x = a.x;
            s.y = a.y;
            if t == 0xD || t == 0xE {
                s.x = 0x3C0;
                if t == 0xD {
                    s.y = 0x100;
                }
            }
            let row = if t < 7 { t as i16 + 0xC1 } else { clut_row + 1 };
            s.clut = ((row as u16) << 6) | 0x30;
            if a.x > 0x2C0 && h > 0xC0 {
                h = 0xC0;
            }
            let mut rw = w;
            let mut rh = h;
            match mode & 3 {
                0 | 1 => {
                    s.mode = Some(mode & 3);
                    s.tpage = Some(get_tpage(mode & 3, 0, s.x, s.y));
                    rw = page_hw(w, mode);
                }
                _ => {}
            }
            d.slots[slot_i] = Some(s);
            let (mut ux, mut uy) = (a.x, a.y);
            if t == 0xD || t == 0xE {
                ux = 0x3C0;
                if t == 0xD {
                    uy = 0x100;
                } else {
                    rh = 0x80;
                }
            } else if a.x > 0x2FF {
                rh = 0xC0;
            }
            let px = halfwords(&data, pix, rw as usize * rh as usize).ok_or_else(short)?;
            push(&mut d, UploadKind::Page, ux, uy, rw, rh, px);
            if !(t == 0xD || t == 0xE) {
                a.x = ((a.x as i32 + rw as i32 + 0x3F) & 0xFFC0) as i16;
            }
        }

        if t == 0x20 {
            need!(0x10);
            image_state = 1;
            let cur = if env.image_to_second_cursor { &mut b } else { &mut a };
            let w = le::u32(&data, 4) as i16;
            let h = le::u32(&data, 8) as i16;
            let px = halfwords(&data, 0x10, w as usize * h as usize).ok_or_else(short)?;
            let (x, y) = (cur.x, cur.y);
            cur.x += w;
            push(&mut d, UploadKind::Image, x, y, w, h, px);
        }

        if t == 0x24 {
            need!(0x10 + 0x300);
            let w = le::i16(&data, 8);
            let mut h = le::i16(&data, 10);
            let pal_len = le::u16(&data, 12) as i16;
            let mode = le::u16(&data, 14);
            let pal = data[0x10..0x10 + 0x300].to_vec();
            d.uploads.push(Upload { chunk_index: ci, kind: t, what: UploadKind::Clut { variant: 0 }, x: 0x300, y: clut_row as u16, w: 256, h: 1, data: clut_blend(&pal, 0, [0x38; 3], stp) });
            image_state = 2;
            let mut s = d.slots[0xC].unwrap_or_default();
            s.clut = ((clut_row as u16) << 6) | 0x30;
            clut_row -= 1;
            s.x = a.x;
            s.y = a.y;
            if a.x > 0x2C0 && h > 0xC0 {
                h = 0xC0;
            }
            let mut rw = w;
            if mode & 3 <= 1 {
                s.mode = Some(mode & 3);
                s.tpage = Some(get_tpage(mode & 3, 0, a.x, a.y));
                rw = page_hw(w, mode);
            }
            d.slots[0xC] = Some(s);
            let rh = if a.x > 0x2FF { 0xC0 } else { h };
            let pix = 0x10 + (pal_len / 2) as usize * 2;
            let px = halfwords(&data, pix, rw as usize * rh as usize).ok_or_else(short)?;
            push(&mut d, UploadKind::Page, a.x, a.y, rw, rh, px);
            a.x += rw;
        }

        if t == 0x25 {
            need!(0x10);
            let w = le::u32(&data, 4) as i16;
            let h = le::u32(&data, 8) as i16;
            let px = halfwords(&data, 0x10, w as usize * h as usize).ok_or_else(short)?;
            let (x, y) = (b.x, b.y);
            b.x += w;
            push(&mut d, UploadKind::Image, x, y, w, h, px);
        }

        if t == 0x21 {
            // Sprite descriptor kept in RAM and adjusted in place [CODE only].
            need!(0x1C);
            keep = true;
            let q = |v: i16| if v < 0 { (v as i32 + 3) >> 2 } else { (v as i32) >> 2 } as i16;
            let put = |data: &mut Vec<u8>, o: usize, v: i16| data[o..o + 2].copy_from_slice(&v.to_le_bytes());
            let v10 = q(le::i16(&data, 0x10));
            put(&mut data, 0x10, v10);
            let slot = le::i16(&data, 4) as usize;
            let st = d.slots.get(slot).copied().flatten().unwrap_or_default();
            let v16 = le::i16(&data, 0x14).wrapping_add(st.y);
            put(&mut data, 0x16, v16);
            let v14 = le::i16(&data, 0x10).wrapping_add(st.x);
            put(&mut data, 0x14, v14);
            let h18 = le::i16(&data, 0x18);
            let v6 = le::i16(&data, 0x1A).wrapping_mul(h18 / 2);
            put(&mut data, 6, v6);
            put(&mut data, 0x18, q(h18));
        }
        if t == 0x22 {
            need!(12);
            keep = true;
            let w = le::u16(&data, 8);
            let h = le::u16(&data, 10);
            let fsz = (w as usize / 2) * h as usize;
            let frames = if fsz == 0 {
                Vec::new()
            } else {
                data[12..].chunks_exact(fsz).map(|f| f.chunks_exact(2).map(|p| u16::from_le_bytes([p[0], p[1]])).collect()).collect()
            };
            d.anims.push(AnimStrip { chunk_index: ci, slot: le::u32(&data, 4), w, h, frames });
        }
        if t == 0x26 || (0x30..0x58).contains(&t) || (0x101..=0x103).contains(&t) {
            keep = true;
        }
        if (0x28..0x30).contains(&t) {
            keep = true;
            if image_state == 0 {
                need!(0x10);
                image_state = 1;
                let w = le::u32(&data, 4) as i16;
                let h = le::u32(&data, 8) as i16;
                let px = halfwords(&data, 0x10, w as usize * h as usize).ok_or_else(short)?;
                push(&mut d, UploadKind::Image, b.x, b.y, w, h, px);
                b.x += 0x140;
                clut_row -= 1;
            }
        }
        if (0x58..0x60).contains(&t) {
            keep = true;
            if image_state == 0 {
                need!(0xC + 0x300);
                let w = le::i16(&data, 4);
                let h = le::i16(&data, 6);
                let pal_len = le::u16(&data, 8) as i16;
                let mode = le::u16(&data, 10);
                let pal = data[0xC..0xC + 0x300].to_vec();
                d.uploads.push(Upload { chunk_index: ci, kind: t, what: UploadKind::Clut { variant: 0 }, x: 0x300, y: clut_row as u16, w: 256, h: 1, data: clut_blend(&pal, 0, [0x38; 3], stp) });
                image_state = 2;
                let mut s = d.slots[5].unwrap_or_default();
                s.clut = ((clut_row as u16) << 6) | 0x30;
                clut_row -= 1;
                s.x = b.x;
                s.y = b.y;
                let mut rw = w;
                if mode & 3 <= 1 {
                    s.mode = Some(mode & 3);
                    s.tpage = Some(get_tpage(mode & 3, 0, b.x, b.y));
                    rw = page_hw(w, mode);
                }
                d.slots[5] = Some(s);
                let pix = 0xC + (pal_len / 2) as usize * 2;
                let px = halfwords(&data, pix, rw as usize * h as usize).ok_or_else(short)?;
                push(&mut d, UploadKind::Page, b.x, b.y, rw, h, px);
                b.x += 0x140;
            }
        }
        if keep {
            let len = data.len();
            d.retained.push(Retained { chunk_index: ci, kind: t, buffer_offset: buf_off, data });
            buf_off += (len + 3) & !3;
        }
    }
    Ok(d)
}

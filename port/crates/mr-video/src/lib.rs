//! mr-video: the game's STR movies (docs/spec/fmv.md).
//!
//! - `str_` sector demux: an STR file interleaves video sectors (Mode 2
//!   Form 1 data, submode 0x48, a 32-byte header 0x0160 / 0x8001 then 2,016
//!   bytes of the frame's bitstream) with CD-XA audio sectors (submode 0x64,
//!   decoded by mr_audio::xa) [DISC];
//! - `vlc`: the version-2 bitstream -> MDEC run-level halfwords;
//! - `mdec`: the MDEC (dequantise, IDCT, YCbCr -> RGB) in integers;
//! - `movies`: the game's movie table (FMV.BIN, the overlay at 0x800E3DD8)
//!   and the player's rules (skip, fades).
//!
//! Streaming: `StrReader` reads a few raw sectors at a time from the disc and
//! returns frames and audio as they complete; nothing holds a whole movie.

pub mod mdec;
pub mod movies;
pub mod vlc;

use mr_audio::xa::{self, Coding, Subheader};

/// The drive reads movies at double speed: 150 sectors a second [CODE
/// FMV.BIN overlay: CdlSetmode with the speed bit; DISC: every movie's audio
/// (4,032 samples a sector, 1 sector in 32 at 18,900 Hz or 1 in 16 at
/// 37,800 Hz) lasts exactly its sectors / 150].
pub const SECTORS_PER_SECOND: u32 = 150;

/// Byte offsets in a raw 2,352-byte sector.
const USER: usize = 24;
const VIDEO_HEADER: usize = 32;
/// Video payload bytes per sector (2,048 - 32).
pub const VIDEO_PAYLOAD: usize = 2016;

/// A video sector's header (user data bytes 0..32).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoSector {
    /// This sector's index within its frame.
    pub sector: u16,
    /// Sectors in the frame.
    pub sectors: u16,
    /// Frame number (1-based in these files).
    pub frame: u32,
    /// Bitstream bytes in the frame (header word +0x0C, x 4).
    pub size: u32,
    pub width: u16,
    pub height: u16,
}

impl VideoSector {
    pub fn parse(raw: &[u8]) -> Option<VideoSector> {
        if raw.len() < xa::RAW || Subheader::parse(raw).is_audio() {
            return None;
        }
        let u = &raw[USER..];
        let h = |o: usize| u16::from_le_bytes([u[o], u[o + 1]]);
        let w = |o: usize| u32::from_le_bytes([u[o], u[o + 1], u[o + 2], u[o + 3]]);
        if h(0) != 0x0160 || h(2) != 0x8001 {
            return None;
        }
        Some(VideoSector { sector: h(4), sectors: h(6), frame: w(8), size: w(12) * 4, width: h(16), height: h(18) })
    }
}

/// One complete frame of a movie.
#[derive(Debug, Clone)]
pub struct Frame {
    pub number: u32,
    pub width: u16,
    pub height: u16,
    /// Sector index (from the file start) of the frame's last sector.
    pub last_sector: u32,
    /// The bitstream (8-byte header + data), the sectors' payloads joined.
    pub data: Vec<u8>,
}

impl Frame {
    /// When the frame is complete: its last sector's end, in seconds.
    pub fn ready_at(&self) -> f64 {
        (self.last_sector + 1) as f64 / SECTORS_PER_SECOND as f64
    }

    /// Decode to 24-bit RGB (`width * height * 3`).
    pub fn decode(&self, m: &mdec::Mdec, rgb: &mut Vec<u8>) -> Result<(), vlc::Error> {
        let (w, h) = (self.width as usize, self.height as usize);
        let blocks = w.div_ceil(16) * h.div_ceil(16) * 6;
        let mut hw = Vec::with_capacity(blocks * 8);
        vlc::decode(&self.data, blocks, &mut hw)?;
        m.picture(&hw, w, h, rgb);
        Ok(())
    }
}

/// What the reader returns.
#[derive(Debug)]
pub enum Item {
    Frame(Frame),
    /// One audio sector: 44,100 Hz stereo frames (the oracle's resampler),
    /// and the sector index it was read at.
    Audio { sector: u32, pcm: Vec<[i16; 2]> },
}

/// Streams one STR file from the disc.
pub struct StrReader {
    lba: u32,
    sectors: u32,
    next: u32,
    buf: Vec<u8>,
    building: Option<(u32, Vec<u8>, u16, u16)>,
    xa: xa::Decoder,
    audio_channel: Option<(u8, u8)>,
    pcm: Vec<[i16; 2]>,
    pending: std::collections::VecDeque<Item>,
}

/// Raw sectors read per disc access.
const CHUNK: u32 = 16;

impl StrReader {
    pub fn new(lba: u32, sectors: u32) -> StrReader {
        StrReader {
            lba,
            sectors,
            next: 0,
            buf: Vec::new(),
            building: None,
            xa: xa::Decoder::new(),
            audio_channel: None,
            pcm: Vec::new(),
            pending: Default::default(),
        }
    }

    /// Open a file by its disc path (e.g. "FMV2/TT_15.STR").
    pub fn open(disc: &mr_disc::Disc, path: &str) -> Option<StrReader> {
        let e = disc.entry(path)?;
        Some(StrReader::new(e.lba, (e.size as u32).div_ceil(2048)))
    }

    pub fn sectors(&self) -> u32 {
        self.sectors
    }

    /// The next frame or audio sector; None at the end of the file.
    pub fn next(&mut self, disc: &mut mr_disc::Disc) -> Result<Option<Item>, mr_disc::Error> {
        loop {
            if let Some(i) = self.pending.pop_front() {
                return Ok(Some(i));
            }
            if self.next >= self.sectors {
                if let Some((n, data, w, h)) = self.building.take() {
                    return Ok(Some(Item::Frame(Frame { number: n, width: w, height: h, last_sector: self.sectors - 1, data })));
                }
                return Ok(None);
            }
            let n = CHUNK.min(self.sectors - self.next);
            self.buf.clear();
            disc.read_raw(self.lba + self.next, n, &mut self.buf)?;
            let buf = std::mem::take(&mut self.buf);
            for (k, raw) in buf.chunks_exact(xa::RAW).enumerate() {
                self.sector(self.next + k as u32, raw);
            }
            self.buf = buf;
            self.next += n;
        }
    }

    fn sector(&mut self, idx: u32, raw: &[u8]) {
        let sh = Subheader::parse(raw);
        if sh.is_audio() {
            // One stream per movie: the first audio sector's file / channel.
            let ch = *self.audio_channel.get_or_insert((sh.file, sh.channel));
            if ch != (sh.file, sh.channel) {
                return;
            }
            let Some(c) = Coding::parse(sh.coding) else { return };
            self.pcm.clear();
            self.xa.decode_sector(&raw[USER..], c, &mut self.pcm);
            let mut out = Vec::with_capacity(self.pcm.len() * 3);
            xa::resample_sector_44100(&self.pcm, c.rate, &mut out);
            self.pending.push_back(Item::Audio { sector: idx, pcm: out });
            return;
        }
        let Some(v) = VideoSector::parse(raw) else { return };
        if self.building.as_ref().is_some_and(|b| b.0 != v.frame) {
            let (n, data, w, h) = self.building.take().unwrap();
            self.pending.push_back(Item::Frame(Frame { number: n, width: w, height: h, last_sector: idx - 1, data }));
        }
        let b = self.building.get_or_insert_with(|| (v.frame, vec![0u8; v.sectors as usize * VIDEO_PAYLOAD], v.width, v.height));
        let o = v.sector as usize * VIDEO_PAYLOAD;
        if o + VIDEO_PAYLOAD <= b.1.len() {
            b.1[o..o + VIDEO_PAYLOAD].copy_from_slice(&raw[USER + VIDEO_HEADER..USER + VIDEO_HEADER + VIDEO_PAYLOAD]);
        }
        if v.sector + 1 == v.sectors {
            let (n, data, w, h) = self.building.take().unwrap();
            self.pending.push_back(Item::Frame(Frame { number: n, width: w, height: h, last_sector: idx, data }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_frame_of_a_movie_decodes() {
        let Ok(mut disc) = mr_disc::Disc::open_default() else { return };
        let mut r = StrReader::open(&disc, "FMV2/TT_15.STR").expect("TT_15.STR");
        let m = mdec::Mdec::default();
        let (mut frames, mut audio, mut rgb) = (0u32, 0usize, Vec::new());
        let mut last = 0;
        while let Some(i) = r.next(&mut disc).unwrap() {
            match i {
                Item::Frame(f) => {
                    assert_eq!((f.width, f.height), (320, 192));
                    assert_eq!(f.number, last + 1);
                    last = f.number;
                    // the bitstream decodes all 240 macroblocks, and the
                    // MDEC halfwords fit the header's word count
                    let mut hw = Vec::new();
                    let hd = vlc::decode(&f.data, 20 * 12 * 6, &mut hw).unwrap_or_else(|e| panic!("frame {}: {e:?}", f.number));
                    assert!(hw.len().div_ceil(2) <= hd.mdec_words as usize, "frame {}: {} > {}", f.number, hw.len(), hd.mdec_words);
                    assert_eq!(m.picture(&hw, 320, 192, &mut rgb), 240);
                    frames += 1;
                }
                Item::Audio { pcm, .. } => audio += pcm.len(),
            }
        }
        assert_eq!(frames, 301);
        // 113 mono sectors x 4,032 samples at 37,800 Hz -> 4,704 at 44,100
        assert_eq!(audio, 113 * 4704);
    }
}

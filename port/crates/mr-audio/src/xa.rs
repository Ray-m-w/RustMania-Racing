//! CD-XA ADPCM: the race music (TUNES/MUSIC1..4.XA) and the voice clips
//! (VOICES/*.XA), decoded from raw 2352-byte Mode 2 Form 2 sectors.
//!
//! Format (psx-spx "CDROM XA Audio ADPCM Compression", checked against the
//! oracle's decoder psxrecomp runtime/src/cdrom.c xa_decode_sector_*):
//! - raw sector: 12 sync, 4 header, subheader at 16..20 (file, channel,
//!   submode, coding; repeated at 20..24), 2324 bytes of payload at 24:
//!   18 sound groups of 128 bytes, then 20 unused bytes.
//! - submode: 0x04 audio, 0x20 form 2, 0x40 real time, 0x80 end of file.
//! - coding: bit 0 stereo, bits 2-3 rate (0 = 37,800 Hz, 1 = 18,900 Hz),
//!   bits 4-5 bits per sample (0 = 4, 1 = 8), bit 6 emphasis.
//! - sound group: 16 header bytes (4..11 = 8 unit parameters, the rest
//!   copies), 112 data bytes = 28 words of 4 bytes. 4-bit: 8 sound units,
//!   unit u sample j = nibble (u & 1) of byte 16 + j*4 + (u >> 1). 8-bit: 4
//!   units, sample j = byte 16 + j*4 + u. Stereo: even units left, odd right.
//! - parameter byte: low nibble = range (shift), bits 4-5 = filter 0..3.
//!   sample = (signed << 12 >> range) + ((k0*s1 + k1*s2 + 32) >> 6), clamped
//!   to i16; k0 = [0, 60, 115, 98], k1 = [0, 0, -52, -55]. Range 13..15 is
//!   reserved and behaves as 9 (psx-spx); none occur on this disc's music.
//!
//! How the game plays a track (FUN_800602c4 @ 0x800602C4, see
//! docs/spec/audio.md): track id n < 0x20 is TUNES/MUSIC(n/8+1).XA channel
//! n%8; n >= 0x20 is the VOICES file ((n-0x20)/16 + 4) of the game's XA file
//! table (0x800ABF7C, filled by CdSearchFile at boot) channel (n-0x20)%16.
//! CdlSetfilter(file 1, channel), CdlSetmode 0xE8 (double speed, XA-ADPCM,
//! whole sector, filter), CdlReadS from the file's first sector. The end of
//! a channel's audio is a data sector (submode bit 2 clear) whose payload
//! starts with u16 0x0160 and whose next u16 carries the channel in bits
//! 10..14: the read callback FUN_80060160 then either seeks back to the
//! file start (looping track) or pauses and mutes.

/// Bytes in a raw CD sector.
pub const RAW: usize = 2352;
/// Sound groups per Form 2 sector.
pub const GROUPS: usize = 18;
/// Payload offset in a raw sector.
pub const DATA_OFFSET: usize = 24;
/// The subheader file number every music and voice sector carries (the game
/// always sets CdlSetfilter file = 1).
pub const GAME_XA_FILE: u8 = 1;
/// Marker word at the start of an end-of-channel data sector.
pub const END_MARKER: u16 = 0x0160;

const K0: [i32; 4] = [0, 60, 115, 98];
const K1: [i32; 4] = [0, 0, -52, -55];

/// The subheader of a Mode 2 sector (bytes 16..20).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Subheader {
    pub file: u8,
    pub channel: u8,
    pub submode: u8,
    pub coding: u8,
}

impl Subheader {
    pub fn parse(raw: &[u8]) -> Subheader {
        Subheader { file: raw[16], channel: raw[17], submode: raw[18], coding: raw[19] }
    }
    pub fn is_audio(&self) -> bool {
        self.submode & 0x04 != 0
    }
    pub fn is_eof(&self) -> bool {
        self.submode & 0x80 != 0
    }
    pub fn is_realtime(&self) -> bool {
        self.submode & 0x40 != 0
    }
}

/// Decoded `coding` byte of an audio sector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Coding {
    pub stereo: bool,
    /// 37,800 or 18,900.
    pub rate: u32,
    /// 4 or 8.
    pub bits: u8,
}

impl Coding {
    /// None for the reserved rate / depth codes.
    pub fn parse(c: u8) -> Option<Coding> {
        let rate = match (c >> 2) & 3 {
            0 => 37_800,
            1 => 18_900,
            _ => return None,
        };
        let bits = match (c >> 4) & 3 {
            0 => 4,
            1 => 8,
            _ => return None,
        };
        Some(Coding { stereo: c & 1 != 0, rate, bits })
    }

    /// Output frames (stereo pairs; mono is duplicated) one sector decodes to.
    pub fn frames_per_sector(&self) -> usize {
        let units = if self.bits == 4 { 8 } else { 4 };
        let samples = GROUPS * units * 28;
        if self.stereo { samples / 2 } else { samples }
    }
}

/// True if `raw` is the end-of-channel marker sector for `channel` (the test
/// FUN_80060160 makes on every data sector the drive hands the CPU).
pub fn is_end_marker(raw: &[u8], channel: u8) -> bool {
    let sh = Subheader::parse(raw);
    if sh.is_audio() {
        return false;
    }
    let w0 = u16::from_le_bytes([raw[DATA_OFFSET], raw[DATA_OFFSET + 1]]);
    let w1 = u16::from_le_bytes([raw[DATA_OFFSET + 2], raw[DATA_OFFSET + 3]]);
    w0 == END_MARKER && ((w1 & 0x7C00) >> 10) as u8 == channel
}

/// ADPCM decoder state: the two previous samples per side. One decoder per
/// stream; the drive keeps it across sectors of the same file/channel.
#[derive(Debug, Clone, Default)]
pub struct Decoder {
    hist: [[i32; 2]; 2],
}

#[inline]
fn clamp16(v: i32) -> i32 {
    v.clamp(-32768, 32767)
}

impl Decoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.hist = [[0; 2]; 2];
    }

    #[inline]
    fn unit(&mut self, side: usize, param: u8, raw: impl Fn(usize) -> i32, out: &mut [i32; 28]) {
        let mut range = (param & 0x0F) as u32;
        if range > 12 {
            range = 9;
        }
        let f = ((param >> 4) & 3) as usize;
        let h = &mut self.hist[side];
        for (j, o) in out.iter_mut().enumerate() {
            // raw(j) is the sample already placed in the top bits of an i16
            // (4-bit: << 12, 8-bit: << 8).
            let t = raw(j) >> range;
            let s = clamp16(t + ((K0[f] * h[0] + K1[f] * h[1] + 32) >> 6));
            h[1] = h[0];
            h[0] = s;
            *o = s;
        }
    }

    /// Decode one audio sector's payload (raw sector bytes 24.., at least
    /// 18 * 128 bytes) at its native rate; appends stereo frames to `out`
    /// (mono samples are duplicated to both sides). Returns frames appended.
    pub fn decode_sector(&mut self, payload: &[u8], coding: Coding, out: &mut Vec<[i16; 2]>) -> usize {
        let start = out.len();
        let mut buf = [0i32; 28];
        let mut left = [0i32; 28];
        for g in 0..GROUPS {
            let grp = &payload[g * 128..g * 128 + 128];
            let units = if coding.bits == 4 { 8 } else { 4 };
            for u in 0..units {
                let param = grp[4 + u];
                if coding.bits == 4 {
                    let sh = (u & 1) * 4;
                    let col = u >> 1;
                    let raw = |j: usize| ((grp[16 + j * 4 + col] >> sh) as i32 & 0xF) << 28 >> 16;
                    if coding.stereo {
                        let side = u & 1;
                        if side == 0 {
                            self.unit(0, param, raw, &mut left);
                        } else {
                            self.unit(1, param, raw, &mut buf);
                            out.extend(left.iter().zip(buf.iter()).map(|(&l, &r)| [l as i16, r as i16]));
                        }
                    } else {
                        self.unit(0, param, raw, &mut buf);
                        out.extend(buf.iter().map(|&s| [s as i16, s as i16]));
                    }
                } else {
                    let raw = |j: usize| (grp[16 + j * 4 + u] as i8 as i32) << 8;
                    if coding.stereo {
                        let side = u & 1;
                        if side == 0 {
                            self.unit(0, param, raw, &mut left);
                        } else {
                            self.unit(1, param, raw, &mut buf);
                            out.extend(left.iter().zip(buf.iter()).map(|(&l, &r)| [l as i16, r as i16]));
                        }
                    } else {
                        self.unit(0, param, raw, &mut buf);
                        out.extend(buf.iter().map(|&s| [s as i16, s as i16]));
                    }
                }
            }
        }
        if !coding.stereo {
            self.hist[1] = self.hist[0];
        }
        out.len() - start
    }
}

/// Resample one sector's native frames to 44,100 Hz exactly as the oracle's
/// CD path does (psxrecomp cdrom.c xa_resample_to_44100: linear, per sector,
/// phase restarting at 0, the last input frame held). For 37,800 Hz a
/// sector's 2,016 frames become 2,352. Appends to `out`.
pub fn resample_sector_44100(input: &[[i16; 2]], rate: u32, out: &mut Vec<[i16; 2]>) {
    let n = input.len();
    if n == 0 || rate == 0 {
        return;
    }
    let mut pos = 0usize;
    let mut phase: i32 = 0;
    while pos < n {
        let next = if pos + 1 < n { pos + 1 } else { pos };
        let (c, x) = (input[pos], input[next]);
        let l = c[0] as i32 + ((x[0] as i32 - c[0] as i32) * phase) / 44_100;
        let r = c[1] as i32 + ((x[1] as i32 - c[1] as i32) * phase) / 44_100;
        out.push([l as i16, r as i16]);
        phase += rate as i32;
        while phase >= 44_100 {
            phase -= 44_100;
            pos += 1;
        }
    }
}

/// The CD controller's audio volume matrix (CdMix / CdlATV, 0x80 = unity):
/// ll = left to left, lr = left to right, rr, rl. Applied after resampling
/// like the oracle (and the hardware) do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CdMix {
    pub ll: u8,
    pub lr: u8,
    pub rr: u8,
    pub rl: u8,
}

impl CdMix {
    pub const UNITY: CdMix = CdMix { ll: 0x80, lr: 0, rr: 0x80, rl: 0 };
    /// What the game sets for music: CdlATV {v, 0, v, 0} with v = the music
    /// level byte 0x800B0A50 (voices: 0x800B0E70).
    pub fn level(v: u8) -> CdMix {
        CdMix { ll: v, lr: 0, rr: v, rl: 0 }
    }
    pub fn apply(&self, f: [i16; 2]) -> [i16; 2] {
        let (l, r) = (f[0] as i32, f[1] as i32);
        let lo = ((l * self.ll as i32) >> 7) + ((r * self.rl as i32) >> 7);
        let ro = ((l * self.lr as i32) >> 7) + ((r * self.rr as i32) >> 7);
        [clamp16(lo) as i16, clamp16(ro) as i16]
    }
}

/// Where a game track id lives (FUN_800602c4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrackRef {
    /// Index into the game's XA file table (0x800ABF7C): 0..3 = TUNES/MUSIC1..4.
    pub file_index: u8,
    pub channel: u8,
    /// Music (< 0x20) uses the music level 0x800B0A50, voices 0x800B0E70.
    pub is_music: bool,
}

impl TrackRef {
    pub fn from_track_id(id: u16) -> TrackRef {
        let id = id & 0x3FFF;
        if id < 0x20 {
            TrackRef { file_index: (id >> 3) as u8, channel: (id & 7) as u8, is_music: true }
        } else {
            let v = id - 0x20;
            TrackRef { file_index: ((v >> 4) + 4) as u8, channel: (v & 0xF) as u8, is_music: false }
        }
    }

    /// Disc path for the music files (file_index 0..3). Voice files follow
    /// the boot-time table order, which this crate does not hard-code.
    pub fn music_path(&self) -> Option<String> {
        if self.file_index < 4 {
            Some(format!("TUNES/MUSIC{}.XA", self.file_index + 1))
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------- exe tables

/// Load address of the exe's text (PS-X EXE t_addr; file offset 0x800).
pub const EXE_T_ADDR: u32 = 0x8001_0000;
/// The XA file table: 12-byte rows {name pointer, first sector, end sector}
/// (the sectors are filled by CdSearchFile at boot; FUN_800602C4 reads
/// 0x800ABF7C / 0x800ABF80 = first / end of row `file_index`).
pub const XA_FILE_TABLE: u32 = 0x800A_BF78;
/// Course rows, 12 bytes: +3 music track id (i8, -1 none; DAT_800ACB37),
/// +4 intro voice track base (i16, -1 none; DAT_800ACB38), +0xA bit 0 =
/// which of the two intro clips plays next, bit 7 = already played
/// (DAT_800ACB3E). Index = course (0x800B0FAC; DOCK = 3).
pub const COURSE_TABLE: u32 = 0x800A_CB34;

fn exe_at(exe: &[u8], addr: u32, n: usize) -> Option<&[u8]> {
    let o = addr.checked_sub(EXE_T_ADDR)? as usize + 0x800;
    exe.get(o..o + n)
}

/// Disc paths of the XA file table rows ("TUNES/MUSIC1.XA", ...), in table
/// order, read from the exe (SLUS_012.37). Stops at the first row whose name
/// pointer is not inside the exe (16 rows on this disc).
pub fn exe_xa_file_names(exe: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    for i in 0..64u32 {
        let Some(p) = exe_at(exe, XA_FILE_TABLE + i * 12, 4) else { break };
        let p = u32::from_le_bytes(p.try_into().unwrap());
        let Some(s) = exe_at(exe, p, 32) else { break };
        let Some(end) = s.iter().position(|&c| c == 0) else { break };
        let name = String::from_utf8_lossy(&s[..end]).into_owned();
        if !name.starts_with('\\') {
            break;
        }
        out.push(mr_disc::normalize(&name));
    }
    out
}

/// The course's music track id (row +3), None for -1.
pub fn exe_course_music(exe: &[u8], course: u8) -> Option<u16> {
    let b = *exe_at(exe, COURSE_TABLE + course as u32 * 12 + 3, 1)?.first()? as i8;
    (b >= 0).then_some(b as u16)
}

/// The course's intro voice clip as the queue entry FUN_8008C4AC builds:
/// (row +4) + (row +0xA & 1), | 0xC000 (one-shot voice). The toggle starts
/// at 0 at power-on. None when the row has none.
pub fn exe_course_voice(exe: &[u8], course: u8) -> Option<u16> {
    let r = exe_at(exe, COURSE_TABLE + course as u32 * 12, 12)?;
    let v = i16::from_le_bytes([r[4], r[5]]);
    (v != -1).then(|| (v as u16).wrapping_add((r[10] & 1) as u16) | 0xC000)
}

/// Raw sector access for the stream (implemented for mr_disc::Disc).
pub trait SectorSource {
    /// Append `count` raw 2352-byte sectors from `lba` to `out`.
    fn read_raw(&mut self, lba: u32, count: u32, out: &mut Vec<u8>) -> bool;
}

impl SectorSource for mr_disc::Disc {
    fn read_raw(&mut self, lba: u32, count: u32, out: &mut Vec<u8>) -> bool {
        mr_disc::Disc::read_raw(self, lba, count, out).is_ok()
    }
}

/// What one `XaStream::step` produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// One audio sector of the channel was decoded; frames appended.
    Audio(usize),
    /// A sector of another channel or a data sector: nothing to play.
    Skip,
    /// The end marker was read and the track restarted from its first sector.
    Looped,
    /// The end marker (or the end of the file) was reached on a one-shot
    /// track; the stream is finished.
    Ended,
}

/// One XA channel played the game's way: sequential sector reads from the
/// file start, filter on (file 1, channel), end marker -> loop or stop.
pub struct XaStream {
    pub first_lba: u32,
    /// One past the file's last sector (the drive would run on; the game's
    /// end markers always come first on this disc).
    pub end_lba: u32,
    pub channel: u8,
    pub looping: bool,
    lba: u32,
    dec: Decoder,
    ended: bool,
    cache: Vec<u8>,
    cache_lba: u32,
    native: Vec<[i16; 2]>,
    /// Sectors read per disc access.
    pub batch: u32,
    /// Times the end marker restarted the track.
    pub loops: u32,
}

impl XaStream {
    pub fn new(first_lba: u32, end_lba: u32, channel: u8, looping: bool) -> XaStream {
        XaStream {
            first_lba,
            end_lba,
            channel,
            looping,
            lba: first_lba,
            dec: Decoder::new(),
            ended: false,
            cache: Vec::new(),
            cache_lba: 0,
            native: Vec::with_capacity(4032),
            batch: 32,
            loops: 0,
        }
    }

    /// The stream of `track_id` in a music file given its first sector and
    /// sector count (from mr_disc's directory: lba, size / 2048).
    pub fn for_track(track_id: u16, file_lba: u32, file_sectors: u32, looping: bool) -> XaStream {
        let t = TrackRef::from_track_id(track_id);
        XaStream::new(file_lba, file_lba + file_sectors, t.channel, looping)
    }

    pub fn is_ended(&self) -> bool {
        self.ended
    }

    /// The next sector the drive will read.
    pub fn lba(&self) -> u32 {
        self.lba
    }

    fn sector<'a>(&'a mut self, src: &mut dyn SectorSource) -> Option<&'a [u8]> {
        let n = (self.cache.len() / RAW) as u32;
        if self.lba < self.cache_lba || self.lba >= self.cache_lba + n {
            self.cache.clear();
            let count = self.batch.min(self.end_lba.saturating_sub(self.lba)).max(1);
            if !src.read_raw(self.lba, count, &mut self.cache) {
                return None;
            }
            self.cache_lba = self.lba;
        }
        let o = (self.lba - self.cache_lba) as usize * RAW;
        Some(&self.cache[o..o + RAW])
    }

    /// Read one sector. Audio of the channel is decoded and resampled to
    /// 44,100 Hz (the oracle's resampler) and appended to `out` (no CdMix).
    pub fn step(&mut self, src: &mut dyn SectorSource, out: &mut Vec<[i16; 2]>) -> Step {
        if self.ended {
            return Step::Ended;
        }
        if self.lba >= self.end_lba {
            return self.at_end();
        }
        let ch = self.channel;
        let Some(raw) = self.sector(src) else {
            self.ended = true;
            return Step::Ended;
        };
        let sh = Subheader::parse(raw);
        if sh.is_audio() {
            if sh.file != GAME_XA_FILE || sh.channel != ch {
                self.lba += 1;
                return Step::Skip;
            }
            let Some(coding) = Coding::parse(sh.coding) else {
                self.lba += 1;
                return Step::Skip;
            };
            // Borrow juggling: copy the payload's 2304 bytes out of the cache.
            let mut payload = [0u8; GROUPS * 128];
            payload.copy_from_slice(&raw[DATA_OFFSET..DATA_OFFSET + GROUPS * 128]);
            self.native.clear();
            let mut native = std::mem::take(&mut self.native);
            self.dec.decode_sector(&payload, coding, &mut native);
            let before = out.len();
            resample_sector_44100(&native, coding.rate, out);
            self.native = native;
            self.lba += 1;
            Step::Audio(out.len() - before)
        } else if is_end_marker(raw, ch) {
            self.at_end()
        } else {
            self.lba += 1;
            Step::Skip
        }
    }

    fn at_end(&mut self) -> Step {
        if self.looping {
            self.lba = self.first_lba;
            self.loops += 1;
            Step::Looped
        } else {
            self.ended = true;
            Step::Ended
        }
    }

    /// Decode until at least `want` frames were appended (or the end).
    pub fn fill(&mut self, src: &mut dyn SectorSource, out: &mut Vec<[i16; 2]>, want: usize) -> Step {
        let start = out.len();
        let mut guard = 0u32;
        loop {
            let s = self.step(src, out);
            if s == Step::Ended {
                return s;
            }
            if out.len() - start >= want {
                return s;
            }
            // A looping stream with no audio at all must not spin forever.
            guard = if matches!(s, Step::Audio(_)) { 0 } else { guard + 1 };
            if guard > 100_000 {
                self.ended = true;
                return Step::Ended;
            }
        }
    }
}

// ---------------------------------------------------------------- the queue

/// Queue entry flag: play once (no loop); the game's voice clips.
pub const ONE_SHOT: u16 = 0x8000;
/// Set with ONE_SHOT on the course intro clip (0xC000 | id).
pub const INTRO: u16 = 0x4000;

/// The game's XA request queue (0x800B0740 current, 0x800B0744/46/48 the
/// three pending entries, bit 0 of 0x800B0760 = playing): who plays next and
/// when. Music tracks loop; one-shot entries (voice clips) interrupt a
/// looping track at once, and the course track queued behind them starts
/// again from its first sector when the clip's end marker is read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct XaQueue {
    pub cur: Option<u16>,
    pub q: [Option<u16>; 3],
    pub playing: bool,
}

impl XaQueue {
    fn one_shot(e: Option<u16>) -> bool {
        e.is_some_and(|v| v & ONE_SHOT != 0)
    }

    /// FUN_800604C8: the slot after the last one-shot entry (0 if none,
    /// 3 = full of clips).
    pub fn free_slot(&self) -> usize {
        let mut i = 3;
        while !Self::one_shot(self.q[i - 1]) {
            i -= 1;
            if i < 1 {
                return 0;
            }
        }
        i
    }

    /// FUN_80060580 (and FUN_800606D0): queue `clip` once, then the course
    /// track behind it (`course_music`, None = the course has none / mode
    /// 0xD). Dropped when three clips are already pending.
    pub fn request_clip(&mut self, clip: u16, course_music: Option<u16>) {
        let i = self.free_slot();
        if i == 3 {
            return;
        }
        self.q[i] = Some(clip | ONE_SHOT);
        if i + 1 < 3 {
            if let Some(m) = course_music {
                self.q[i + 1] = Some(m);
            }
        }
    }

    /// A raw entry (flags included) after the last pending one-shot, then
    /// the course track: FUN_800606D0's taunt (0xC000 | 0xD0 + n) and
    /// FUN_8008C4AC's boss clip in game mode 7 (0x8000 | base).
    pub fn request_entry(&mut self, entry: u16, course_music: Option<u16>) {
        let i = self.free_slot();
        if i == 3 {
            return;
        }
        self.q[i] = Some(entry);
        if i + 1 < 3 {
            if let Some(m) = course_music {
                self.q[i + 1] = Some(m);
            }
        }
    }

    /// The race-load part of FUN_8008C4AC for a race entered from the menus
    /// (mode 0): a clip in progress is dropped with the queue; the course
    /// intro clip (once per course per power-on: the caller passes None
    /// after the first time) and the course track are queued. A course
    /// track already looping is left alone (it keeps playing on a restart).
    pub fn race_load(&mut self, course_music: Option<u16>, intro: Option<u16>) {
        if Self::one_shot(self.cur) {
            self.cur = None;
            self.q = [None; 3];
        }
        if let Some(v) = intro {
            let i = self.free_slot();
            if i < 3 {
                self.q[i] = Some(v | ONE_SHOT | INTRO);
            }
        }
        let i = self.free_slot();
        if i < 3 {
            if let Some(m) = course_music {
                if i > 0 || self.cur.is_none() || Self::one_shot(self.cur) {
                    self.q[i] = Some(m);
                }
            }
        }
    }

    /// FUN_8005FB4C, once per frame: start the head of the queue when
    /// nothing plays or a looping track plays. Returns (track id, loop) to
    /// start (CdlSetfilter + CdlReadS from the file start).
    pub fn frame(&mut self) -> Option<(u16, bool)> {
        let head = self.q[0]?;
        if self.playing && self.cur.is_some() && Self::one_shot(self.cur) {
            return None;
        }
        self.cur = Some(head);
        self.q = [self.q[1], self.q[2], None];
        self.playing = true;
        Some((head & 0x3FFF, head & ONE_SHOT == 0))
    }

    /// FUN_80060160 on a one-shot's end marker: pause, mute, not playing.
    pub fn clip_ended(&mut self) {
        self.playing = false;
    }

    /// FUN_8005FA18: stop and forget everything.
    pub fn stop(&mut self) {
        *self = XaQueue::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A raw audio sector with every unit's parameter = `param` and every
    /// data byte = `byte`.
    fn sector(file: u8, ch: u8, coding: u8, param: u8, byte: u8) -> Vec<u8> {
        let mut s = vec![0u8; RAW];
        s[16..20].copy_from_slice(&[file, ch, 0x64, coding]);
        s[20..24].copy_from_slice(&[file, ch, 0x64, coding]);
        for g in 0..GROUPS {
            let o = DATA_OFFSET + g * 128;
            for i in 0..16 {
                s[o + i] = param;
            }
            for i in 16..128 {
                s[o + i] = byte;
            }
        }
        s
    }

    fn marker(ch: u8) -> Vec<u8> {
        let mut s = vec![0u8; RAW];
        s[16..20].copy_from_slice(&[1, ch, 0x48, 0]);
        s[24..26].copy_from_slice(&0x0160u16.to_le_bytes());
        s[26..28].copy_from_slice(&(((ch as u16) << 10) | 0x8001 & 0x83FF).to_le_bytes());
        s
    }

    #[test]
    fn queue_matches_the_oracle_race() {
        // Race load from the menus: intro clip 0xB0, then DOCK's track 16.
        let mut q = XaQueue::default();
        q.race_load(Some(16), Some(0xB0));
        assert_eq!(q.frame(), Some((0xB0, false)));
        assert_eq!(q.frame(), None); // the clip is not interrupted
        q.clip_ended();
        assert_eq!(q.frame(), Some((16, true)));
        assert_eq!(q.frame(), None);
        // The player's finish (FUN_80082708: Kermit base 0x50 + 2): the clip
        // interrupts the looping track, which restarts after it (oracle VB
        // 7043..7320).
        q.request_clip(0x52, Some(16));
        assert_eq!(q.q, [Some(0x8052), Some(16), None]);
        assert_eq!(q.frame(), Some((0x52, false)));
        q.clip_ended();
        assert_eq!(q.frame(), Some((16, true)));
        // Restart (back through the menus, mode 0): the track keeps looping.
        q.race_load(Some(16), None);
        assert_eq!(q.frame(), None);
        assert_eq!(q.cur, Some(16));
        // Three clips pending: a fourth is dropped.
        let mut q = XaQueue::default();
        for c in [0x50, 0x51, 0x52, 0x53] {
            q.request_clip(c, Some(16));
        }
        assert_eq!(q.q, [Some(0x8050), Some(0x8051), Some(0x8052)]);
    }

    #[test]
    fn coding_bits() {
        let c = Coding::parse(0x01).unwrap();
        assert_eq!((c.stereo, c.rate, c.bits), (true, 37_800, 4));
        assert_eq!(c.frames_per_sector(), 2016);
        let c = Coding::parse(0x00).unwrap();
        assert_eq!(c.frames_per_sector(), 4032);
        let c = Coding::parse(0x15).unwrap();
        assert_eq!((c.stereo, c.rate, c.bits), (true, 18_900, 8));
        assert_eq!(c.frames_per_sector(), 1008);
        assert!(Coding::parse(0x08).is_none());
        assert!(Coding::parse(0x20).is_none());
    }

    #[test]
    fn track_ids_follow_fun_800602c4() {
        assert_eq!(TrackRef::from_track_id(16), TrackRef { file_index: 2, channel: 0, is_music: true });
        assert_eq!(TrackRef::from_track_id(27), TrackRef { file_index: 3, channel: 3, is_music: true });
        // 0xC0B0 & 0x3FFF = 0xB0: voices file 4 + (0x90 >> 4) = 13, channel 0.
        assert_eq!(TrackRef::from_track_id(0xC0B0), TrackRef { file_index: 13, channel: 0, is_music: false });
        assert_eq!(TrackRef::from_track_id(16).music_path().unwrap(), "TUNES/MUSIC3.XA");
    }

    #[test]
    fn filter0_is_the_shifted_nibble() {
        // range 12 -> shift 12: sample = nibble. 0x71: low nibble 1, high 7.
        let s = sector(1, 0, 0x01, 0x0C, 0x71);
        let mut d = Decoder::new();
        let mut out = Vec::new();
        let n = d.decode_sector(&s[DATA_OFFSET..], Coding::parse(0x01).unwrap(), &mut out);
        assert_eq!(n, 2016);
        assert!(out.iter().all(|&f| f == [1, 7]));
        // range 0: nibble << 12; 0x8 = -8 -> -32768.
        let s = sector(1, 0, 0x00, 0x00, 0x88);
        let mut out = Vec::new();
        Decoder::new().decode_sector(&s[DATA_OFFSET..], Coding::parse(0x00).unwrap(), &mut out);
        assert_eq!(out.len(), 4032);
        assert!(out.iter().all(|&f| f == [-32768, -32768]));
    }

    #[test]
    fn filter_recursion_and_clamp() {
        // filter 1 (k0 = 60): s = t + (60 * s1 + 32) >> 6, t = 1 << 12 >> 12 = 1.
        let s = sector(1, 0, 0x00, 0x1C, 0x11);
        let mut out = Vec::new();
        Decoder::new().decode_sector(&s[DATA_OFFSET..], Coding::parse(0x00).unwrap(), &mut out);
        let mut h = 0i32;
        for (i, f) in out.iter().take(28).enumerate() {
            h = 1 + ((60 * h + 32) >> 6);
            assert_eq!(f[0] as i32, h, "sample {i}");
        }
        // filter 2 (115, -52) at full positive input clamps at 32767.
        let s = sector(1, 0, 0x01, 0x20, 0x77);
        let mut out = Vec::new();
        Decoder::new().decode_sector(&s[DATA_OFFSET..], Coding::parse(0x01).unwrap(), &mut out);
        assert!(out.iter().skip(4).all(|f| f[0] == 32767 && f[1] == 32767));
    }

    #[test]
    fn eight_bit_units() {
        // 8-bit mono: 4 units * 28 samples per group; byte 0x40 = 64 << 8 >> 8.
        let s = sector(1, 0, 0x10, 0x08, 0x40);
        let mut out = Vec::new();
        let n = Decoder::new().decode_sector(&s[DATA_OFFSET..], Coding::parse(0x10).unwrap(), &mut out);
        assert_eq!(n, 2016);
        assert!(out.iter().all(|&f| f == [64, 64]));
    }

    #[test]
    fn resampler_matches_the_oracle_shape() {
        let input: Vec<[i16; 2]> = (0..2016).map(|i| [i as i16 * 7, -(i as i16)]).collect();
        let mut out = Vec::new();
        resample_sector_44100(&input, 37_800, &mut out);
        assert_eq!(out.len(), 2352);
        assert_eq!(out[0], input[0]);
        // out[7] sits exactly on input[6] (ratio 6/7).
        assert_eq!(out[7], input[6]);
        // Between: phase 37800 at out[1] -> c + d * 37800 / 44100.
        assert_eq!(out[1][0] as i32, (7 * 37_800) / 44_100);
        let mut o2 = Vec::new();
        resample_sector_44100(&input[..4032 / 2], 18_900, &mut o2);
        assert_eq!(o2.len(), 4704);
    }

    #[test]
    fn cdmix_scales() {
        assert_eq!(CdMix::UNITY.apply([1000, -1000]), [1000, -1000]);
        assert_eq!(CdMix::level(0x6C).apply([1000, -1000]), [843, -844]);
        let swap = CdMix { ll: 0, lr: 0x80, rr: 0, rl: 0x80 };
        assert_eq!(swap.apply([5, 9]), [9, 5]);
    }

    struct Mem(Vec<Vec<u8>>, u32);
    impl SectorSource for Mem {
        fn read_raw(&mut self, lba: u32, count: u32, out: &mut Vec<u8>) -> bool {
            for l in lba..lba + count {
                match self.0.get((l - self.1) as usize) {
                    Some(s) => out.extend_from_slice(s),
                    None => return false,
                }
            }
            true
        }
    }

    #[test]
    fn stream_demuxes_and_loops_on_the_end_marker() {
        // Two channels interleaved, channel 1 ends with an end marker after
        // two audio sectors.
        let a = |ch, b| sector(1, ch, 0x01, 0x0C, b);
        let secs = vec![a(0, 0x11), a(1, 0x22), a(0, 0x11), a(1, 0x22), a(0, 0x11), marker(1), a(0, 0x11), marker(1)];
        let mut src = Mem(secs, 1000);
        let mut st = XaStream::new(1000, 1008, 1, true);
        let mut out = Vec::new();
        let mut steps = Vec::new();
        for _ in 0..8 {
            steps.push(st.step(&mut src, &mut out));
        }
        assert_eq!(
            steps,
            vec![Step::Skip, Step::Audio(2352), Step::Skip, Step::Audio(2352), Step::Skip, Step::Looped, Step::Skip, Step::Audio(2352)]
        );
        assert!(out.iter().all(|&f| f == [2, 2]));
        assert_eq!(st.loops, 1);
        // One-shot: ends at the marker.
        let mut st = XaStream::new(1000, 1008, 1, false);
        let mut out = Vec::new();
        assert_eq!(st.fill(&mut src, &mut out, 1 << 20), Step::Ended);
        assert_eq!(out.len(), 2 * 2352);
        // Channel 0 has no marker: ends at the file end.
        let mut st = XaStream::new(1000, 1008, 0, false);
        let mut out = Vec::new();
        assert_eq!(st.fill(&mut src, &mut out, 1 << 20), Step::Ended);
        assert_eq!(out.len(), 4 * 2352);
        assert!(is_end_marker(&marker(3), 3) && !is_end_marker(&marker(3), 2));
    }
}

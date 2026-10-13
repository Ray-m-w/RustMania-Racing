//! The game's sound driver between the race code and the SPU: the game's own
//! layer (one-shots FUN_8005EECC, looping channels FUN_8005F168 with the
//! VBlank callback FUN_8005EFD8 and the voice-steal fix-up FUN_8005ED54) on
//! top of the PSYQ libsnd "utility" calls it uses (SsUtKeyOn 0x8009B3F0,
//! SsUtKeyOnV 0x8009B890, SsUtKeyOffV 0x8009BBEC, SsUtSetDetVVol 0x8009BC9C,
//! SpuSetVoicePitch 0x800A0840) and libsnd's voice manager (_SsVmAlloc
//! 0x8009BE30, _SsVmDoAllocate 0x8009C0A0, _SsVmKeyOnNow 0x8009D2B0,
//! SsPitchFromNote 0x8009CB14, _SsVmFlush 0x8009C280 run from the VBlank
//! tick SsSeqCalledTbyT). Integer arithmetic exactly as the game's code;
//! the SPU behind it is `spu::Spu`.
//!
//! Timing: the game calls one-shots / channel refreshes during its frame;
//! libsnd collects them in a register shadow and writes them to the SPU at
//! the next VBlank (`vblank`), where the key-ons happen. SpuSetVoicePitch
//! writes the SPU at once.
//!
//! Spec: docs/spec/audio.md (SFX + engine).

use crate::spu::{Spu, VOICES};
use crate::vab::Vab;

/// Constants the driver reads from the executable / the game's RAM.
#[derive(Clone, Debug)]
pub struct Consts {
    /// libsnd's pitch tables: semitones [exe 0x800AF1C8, 12 x u16] and
    /// fine steps [exe 0x800AF1E0, 128 x u16].
    pub semi: [u16; 12],
    pub fine: [u16; 128],
    /// 0x800B0E70: sound-effect master volume (FUN_8005F71C, from the
    /// options; 96 in the oracle's save).
    pub sfx_master: i16,
    /// 0x800C5460: added to every tone's release rate (0).
    pub release_adj: i16,
    /// 0x800D1598: libsnd mono flag (0).
    pub mono: bool,
    /// 0x800AF178: voices libsnd must not allocate (0).
    pub reserved: u32,
    /// 0x800D4BE0: voices libsnd manages (24).
    pub voices: usize,
    /// SPU RAM address of the VB body (SpuMalloc in SsVabOpenHead; 0x1010).
    pub base: u32,
}

pub const EXE_SEMI: u32 = 0x800A_F1C8;
pub const EXE_FINE: u32 = 0x800A_F1E0;

impl Consts {
    /// The tables from the PS-X EXE image (file offset = addr - 0x80010000
    /// + 0x800), the rest at the values the game runs with in a race.
    pub fn from_exe(exe: &[u8]) -> Option<Consts> {
        let rd = |a: u32| -> Option<u16> {
            let o = (a - 0x8001_0000 + 0x800) as usize;
            Some(u16::from_le_bytes([*exe.get(o)?, *exe.get(o + 1)?]))
        };
        let mut semi = [0u16; 12];
        for (i, s) in semi.iter_mut().enumerate() {
            *s = rd(EXE_SEMI + 2 * i as u32)?;
        }
        let mut fine = [0u16; 128];
        for (i, s) in fine.iter_mut().enumerate() {
            *s = rd(EXE_FINE + 2 * i as u32)?;
        }
        Some(Consts { semi, fine, sfx_master: 96, release_adj: 0, mono: false, reserved: 0, voices: VOICES, base: 0x1010 })
    }
}

/// libsnd's per-voice manager record (0x800BC108 + 0x38 v; the fields the
/// utility calls use).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VmVoice {
    /// +0x00: VAG number (0 = released).
    pub vag: i16,
    /// +0x02: allocation age (bumped on every allocation, 0 when taken).
    pub age: i16,
    /// +0x04: pitch at key-on.
    pub pitch: u16,
    /// +0x06: envelope level read back at each VBlank flush.
    pub env: u16,
    /// +0x0E note, +0x10 owner (0x21 = utility), +0x12 tone block,
    /// +0x14 program, +0x16 tone, +0x18 vab.
    pub note: i16,
    pub seq: i16,
    pub prog_idx: i16,
    pub prog: i16,
    pub tone: i16,
    pub vab: i16,
    /// +0x1A: tone priority.
    pub prio: i16,
    /// +0x1D: keyed (1) until the envelope has been 0 for 15 flushes.
    pub key: u8,
    /// +0x36: volume byte.
    pub vol: i16,
}

/// libsnd's register shadow (0x800D8B70 + 16 v) and dirty flags (0x800C1E50).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Shadow {
    pub vol_l: u16,
    pub vol_r: u16,
    pub pitch: u16,
    /// Start address / 8.
    pub addr: u16,
    pub adsr1: u16,
    pub adsr2: u16,
    /// 1|2 volume, 4 pitch, 8 address, 0x10|0x20 ADSR.
    pub dirty: u8,
}

/// The game's looping-sound channel (0x800D8AB0 + 0x18 c, 8 channels).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Channel {
    /// +0x00: owner key (engine: player index; skid: player + 2; ...).
    pub owner: i32,
    /// +0x04: sound id - 1 (program * 128 + tone).
    pub sound: i16,
    /// +0x06 current pitch, +0x08 target, +0x0A glide start, +0x0C glide
    /// ticks left.
    pub cur: i16,
    pub target: i16,
    pub from: i16,
    pub glide: i16,
    /// +0x0E: fade level (0..0x40 in steps of 8).
    pub fade: i16,
    /// +0x10: VBlanks until the channel stops unless refreshed (6).
    pub timer: i16,
    /// +0x12: SPU voice, -1 = none.
    pub voice: i16,
    /// +0x14 / +0x16: requested volumes (0..150).
    pub vol_l: i16,
    pub vol_r: i16,
}

impl Default for Channel {
    fn default() -> Self {
        Channel { owner: 0, sound: 0, cur: 0, target: 0, from: 0, glide: 0, fade: 0, timer: 0, voice: -1, vol_l: 0, vol_r: 0 }
    }
}

/// One key-on as the SPU saw it (what the oracle's spu_events KEYON shows).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyOn {
    /// VBlanks since the driver started.
    pub vblank: u64,
    pub voice: u8,
    /// Start address in bytes.
    pub addr: u32,
    pub pitch: u16,
    pub vol_l: u16,
    pub vol_r: u16,
    pub adsr1: u16,
    pub adsr2: u16,
}

/// The parameters SsUtKeyOn derives from a sound id and pitch (what the
/// game's FUN_8005EECC / FUN_8005F168 hand to libsnd).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UtArgs {
    pub prog: i16,
    pub tone: i16,
    pub note: i16,
    pub fine: i16,
    pub voll: i16,
    pub volr: i16,
}

/// `(x + (x < 0 ? 127 : 0)) >> 7`: MIPS division by 128 rounding to zero.
fn div128(x: i32) -> i32 {
    (if x < 0 { x + 127 } else { x }) >> 7
}

/// FUN_8005EECC's argument split: sound id -> program / tone, pitch ->
/// note / fine, volumes * master >> 7 capped at 160.
pub fn oneshot_args(id: i32, pitch: i32, vol_l: i32, vol_r: i32, master: i16) -> UtArgs {
    let s = id - 1;
    let cap = |v: i32| -> i16 {
        let x = ((v as i16 as i32) * master as i32 >> 7) as i16;
        if x > 160 {
            160
        } else {
            x
        }
    };
    UtArgs { prog: div128(s) as i16, tone: (s & 0x7F) as i16, note: div128(pitch) as i16, fine: (pitch & 0x7F) as i16, voll: cap(vol_l), volr: cap(vol_r) }
}

pub struct Driver {
    pub vab: Vab,
    pub c: Consts,
    pub vm: [VmVoice; VOICES],
    pub sh: [Shadow; VOICES],
    /// Pending key-on / key-off masks (0x800B4940/42, 0x800E3DBC/3DD0).
    pub kon: u32,
    pub koff: u32,
    /// Zero-envelope history ring (0x800E3D78, 16 x mask) and its index
    /// (0x800DD848).
    pub env_hist: [u32; 16],
    pub env_idx: usize,
    pub chans: [Channel; 8],
    pub spu: Spu,
    pub vblanks: u64,
    /// Key-ons since the last `take_keyons`.
    pub keyons: Vec<KeyOn>,
    /// Output sample position (fraction of the 44100 Hz clock per VBlank).
    frac: u64,
}

/// SPU samples per VBlank: 44100 / 59.94 (NTSC field rate 60000/1001).
pub const SAMPLES_PER_VBLANK_NUM: u64 = 44_100 * 1001;
pub const SAMPLES_PER_VBLANK_DEN: u64 = 60_000;

impl Driver {
    /// A driver with the bank transferred to SPU RAM at `c.base` and every
    /// voice and channel idle.
    pub fn new(vab: Vab, c: Consts) -> Driver {
        let mut spu = Spu::new();
        spu.write_ram(c.base as usize, &vab.body);
        Driver {
            vab,
            c,
            vm: [VmVoice::default(); VOICES],
            sh: [Shadow::default(); VOICES],
            kon: 0,
            koff: 0,
            env_hist: [0; 16],
            env_idx: 0,
            chans: [Channel::default(); 8],
            spu,
            vblanks: 0,
            keyons: Vec::new(),
            frac: 0,
        }
    }

    // ---- libsnd ------------------------------------------------------

    /// SsPitchFromNote(note, fine, center, shift) [0x8009CB14].
    pub fn pitch_from_note(&self, note: i16, fine: i16, center: u8, shift: u8) -> u16 {
        let f = ((shift as i32 + fine as i32) as i16) as i32;
        let q = div128(f);
        let mut n = note as i32 + q - center as i32;
        let mut r = f - q * 128;
        if (r as i16) < 0 {
            r += 128;
            let v = r as i16 as i32;
            n = n - 1 + div128(v);
        }
        let ns = n as i16 as i32;
        let oct = ns / 12;
        let mut a2 = oct - 2;
        let mut semi = ns - oct * 12;
        if (semi as i16) < 0 {
            semi += 12;
            a2 = oct - 3;
        }
        let si = (semi as i16 as i32) as usize;
        let fi = (r as i16 as i32) as usize;
        let p = ((self.c.semi[si % 12] as u32 * self.c.fine[fi % 128] as u32) >> 16) as u32;
        let a2 = a2 as i16 as i32;
        if a2 >= 0 {
            return 0x3FFF;
        }
        let s = (-a2) as u32;
        (((p + (1u32 << (s - 1))) >> s) & 0xFFFF) as u16
    }

    /// _SsVmAlloc [0x8009BE30]: a free voice (not keyed, envelope 0), else
    /// the lowest-priority voice, ties to the lowest envelope, then the
    /// oldest; `voices` when nothing has a priority <= the tone's.
    fn vm_alloc(&mut self, prior: i8) -> usize {
        let nv = self.c.voices;
        let mut best = 99usize;
        let mut cand = 99usize;
        let mut minprio = prior as i32 & 0xFFFF;
        let mut minenv = 0xFFFFu32;
        let mut maxage = 0i32;
        let mut found = 0u32;
        for i in 0..nv {
            if self.c.reserved & (1 << i) != 0 {
                continue;
            }
            let v = &self.vm[i];
            if v.key == 0 && v.env == 0 {
                best = i;
                break;
            }
            let p = v.prio as i32;
            if p < minprio {
                minprio = v.prio as u16 as i32;
                cand = i;
                minenv = v.env as u32;
                maxage = v.age as u16 as i32;
                found = 1;
            } else if p == minprio {
                found += 1;
                let e = v.env as u32;
                if e < minenv {
                    maxage = v.age as u16 as i32;
                    minenv = e;
                    cand = i;
                } else if e == minenv && (maxage as i16 as i32) < v.age as i32 {
                    maxage = v.age as u16 as i32;
                    cand = i;
                }
            }
        }
        if best == 99 {
            best = if found & 0xFF == 0 { nv } else { cand };
        }
        if best < nv {
            for i in 0..nv {
                if self.c.reserved & (1 << i) == 0 {
                    self.vm[i].age = self.vm[i].age.wrapping_add(1);
                }
            }
            self.vm[best].age = 0;
            self.vm[best].prio = prior as i16;
        }
        best
    }

    /// SsUtKeyOn / SsUtKeyOnV after the voice is chosen: _SsVmDoAllocate
    /// and _SsVmKeyOnNow.
    fn key_on_voice(&mut self, v: usize, a: &UtArgs) {
        let (voll, pan) = if a.voll == a.volr {
            (a.voll, 0x40u8)
        } else if a.volr < a.voll {
            (a.voll, (((a.volr as i32) << 6) / a.voll as i32) as u8)
        } else {
            (a.volr, (0x7F - ((a.voll as i32) << 6) / a.volr as i32) as u8)
        };
        let p = self.vab.programs[a.prog as usize];
        let t = *self.vab.tone(a.prog as usize, a.tone as usize).expect("tone");
        let vm = &mut self.vm[v];
        vm.seq = 0x21;
        vm.vab = 0;
        vm.prog = a.prog;
        vm.prog_idx = p.tone_base as i16;
        vm.vag = t.vag;
        vm.note = a.note;
        vm.tone = a.tone;
        vm.key = 1;
        vm.age = 0;
        vm.vol = voll as i8 as i16;
        // _SsVmDoAllocate
        vm.env = 0x7FFF;
        for h in self.env_hist.iter_mut() {
            *h &= !(1 << v);
        }
        let sh = &mut self.sh[v];
        sh.addr = self.vab.vag_spu_addr(self.c.base, t.vag as usize).unwrap_or(0);
        sh.adsr1 = t.adsr1;
        let rr = (self.c.release_adj as u16).wrapping_add(t.adsr2 & 0x1F);
        let rr = if (rr as i16) > 0x1F { 0x1F } else { rr };
        sh.adsr2 = rr | (t.adsr2 & 0xFFE0);
        sh.dirty |= 8 | 0x30;
        // note2pitch2 + _SsVmKeyOnNow
        let pitch = self.pitch_from_note(a.note, a.fine, t.center, t.shift);
        let (l, r) = keyon_volume(voll as i8, self.vab.mvol, p.mvol as i8, t.vol as i8, t.pan, p.mpan, pan, self.c.mono);
        let sh = &mut self.sh[v];
        sh.pitch = pitch;
        sh.vol_l = l as u16;
        sh.vol_r = r as u16;
        sh.dirty |= 7;
        self.vm[v].pitch = pitch;
        self.kon |= 1 << v;
        self.koff &= !(1 << v);
    }

    /// SsUtKeyOn(vab 0, prog, tone, note, fine, voll, volr) -> voice or -1.
    pub fn ut_key_on(&mut self, a: &UtArgs) -> i16 {
        let Some(t) = self.vab.tone(a.prog as usize, a.tone as usize).copied() else { return -1 };
        if (a.prog as usize) >= self.vab.programs.len() || t.vag == 0 {
            return -1;
        }
        let v = self.vm_alloc(t.prior as i8);
        if v >= self.c.voices {
            return -1;
        }
        self.key_on_voice(v, a);
        v as i16
    }

    /// SsUtKeyOnV(voice, vab 0, prog, tone, note, fine, voll, volr).
    pub fn ut_key_on_v(&mut self, voice: i16, a: &UtArgs) -> i16 {
        if voice < 0 || voice as usize >= VOICES {
            return -1;
        }
        let Some(t) = self.vab.tone(a.prog as usize, a.tone as usize).copied() else { return -1 };
        if t.vag == 0 {
            return -1;
        }
        self.key_on_voice(voice as usize, a);
        voice
    }

    /// SsUtSetDetVVol(voice, l, r): volumes straight into the shadow.
    pub fn set_det_vvol(&mut self, voice: i16, l: i16, r: i16) {
        if (voice as u16) < 0x18 {
            let sh = &mut self.sh[voice as usize];
            sh.vol_l = l as u16;
            sh.vol_r = r as u16;
            sh.dirty |= 3;
        }
    }

    /// SpuSetVoicePitch: the SPU register at once.
    pub fn set_voice_pitch(&mut self, voice: i16, pitch: u16) {
        if (voice as u16 as usize) < VOICES {
            self.spu.voices[voice as usize].pitch = pitch;
        }
    }

    /// _SsVmFlush [0x8009C280]: envelopes back into the voice table, voices
    /// silent for 15 flushes released, the shadow written, keys set.
    fn flush(&mut self) {
        self.env_idx = (self.env_idx + 1) & 0xF;
        self.env_hist[self.env_idx] = 0;
        for v in 0..self.c.voices {
            let e = self.spu.envelope(v) as u16;
            self.vm[v].env = e;
            if e == 0 {
                self.env_hist[self.env_idx] |= 1 << v;
            }
        }
        let silent = self.env_hist[..15].iter().fold(u32::MAX, |m, &h| m & h);
        for v in 0..self.c.voices {
            if silent & (1 << v) != 0 {
                self.vm[v].key = 0;
            }
        }
        self.kon &= !self.koff;
        for v in 0..VOICES {
            let sh = &mut self.sh[v];
            let vo = &mut self.spu.voices[v];
            if sh.dirty & 1 != 0 {
                vo.vol_l = sh.vol_l;
                vo.vol_r = sh.vol_r;
            }
            if sh.dirty & 4 != 0 {
                vo.pitch = sh.pitch;
            }
            if sh.dirty & 8 != 0 {
                vo.start = sh.addr;
            }
            if sh.dirty & 0x10 != 0 {
                vo.adsr1 = sh.adsr1;
                vo.adsr2 = sh.adsr2;
            }
            sh.dirty = 0;
        }
        self.spu.key_off(self.koff);
        self.spu.key_on(self.kon);
        for v in 0..VOICES {
            if self.kon & (1 << v) != 0 {
                let vo = &self.spu.voices[v];
                self.keyons.push(KeyOn {
                    vblank: self.vblanks,
                    voice: v as u8,
                    addr: vo.start as u32 * 8,
                    pitch: vo.pitch,
                    vol_l: vo.vol_l,
                    vol_r: vo.vol_r,
                    adsr1: vo.adsr1,
                    adsr2: vo.adsr2,
                });
            }
        }
        self.kon = 0;
        self.koff = 0;
    }

    // ---- the game's layer -------------------------------------------

    /// FUN_8005ED54(voice, except): a key-on that took `voice` from a
    /// looping channel re-keys that channel on a new voice (repeatedly, as
    /// the new voice may in turn be another channel's).
    fn restore_stolen(&mut self, mut voice: i16, mut except: i32) {
        let m = self.c.sfx_master as i32;
        for _ in 0..64 {
            let mut again = false;
            for c in 0..8 {
                let ch = self.chans[c];
                if ch.voice == voice && except != c as i32 {
                    let cap = |v: i16| -> i16 {
                        let x = ((v as i32 * m) >> 7) as i16;
                        if x > 160 {
                            160
                        } else {
                            x
                        }
                    };
                    let s = ch.sound as i32;
                    let p = ch.cur as i32;
                    let a = UtArgs { prog: div128(s) as i16, tone: (s & 0x7F) as i16, note: div128(p) as i16, fine: (p & 0x7F) as i16, voll: cap(ch.vol_l), volr: cap(ch.vol_r) };
                    let nv = self.ut_key_on(&a);
                    voice = nv;
                    again = true;
                    self.chans[c].voice = nv;
                    except = c as i32;
                }
            }
            if !again {
                return;
            }
        }
    }

    /// FUN_8005EECC(id, pitch, vol_l, vol_r): a one-shot.
    pub fn play(&mut self, id: i32, pitch: i32, vol_l: i32, vol_r: i32) {
        let a = oneshot_args(id, pitch, vol_l, vol_r, self.c.sfx_master);
        let v = self.ut_key_on(&a);
        if v != -1 {
            self.restore_stolen(v, -1);
        }
    }

    /// FUN_8005F168(owner, id, pitch, vol, glide): start or refresh a
    /// looping channel. `dt` = 0x800B0AEC (glide timing). Returns the
    /// game's (volume L + R) / 2, or -1 when no channel is free.
    pub fn loop_sound(&mut self, owner: i32, id: i32, pitch: i32, vol: i32, glide: i32, dt: i32) -> i32 {
        let sound = (id - 1) as i16;
        let mut c = -1i32;
        for i in 0..8 {
            if self.chans[i].owner == owner {
                c = i as i32;
                break;
            }
            if self.chans[i].voice == -1 {
                c = i as i32;
            }
        }
        if c < 0 {
            return -1;
        }
        let c = c as usize;
        let mut l = vol.clamp(0, 150);
        let mut r = vol.clamp(0, 150);
        if l > 0 && r > 0 {
            let ch = self.chans[c];
            if ch.owner == owner && ch.sound != sound && ch.timer > 0 {
                let a = UtArgs { prog: 0, tone: 0, note: 34, fine: 0, voll: 0, volr: 0 };
                self.ut_key_on_v(ch.voice, &a);
                self.chans[c].voice = -1;
            }
            let m = self.c.sfx_master as i32;
            {
                let ch = &mut self.chans[c];
                ch.timer = 6;
                ch.owner = owner;
                ch.sound = sound;
                ch.vol_l = l as i16;
                ch.vol_r = r as i16;
                ch.fade = 0;
            }
            l = (l * m) >> 7;
            r = (r * m) >> 7;
            if l > 160 {
                l = 160;
            }
            if r > 160 {
                r = 160;
            }
            if self.chans[c].voice == -1 {
                let cap = |v: i32| -> i16 {
                    let x = ((v as i16 as i32) * m >> 7) as i16;
                    if x > 160 {
                        160
                    } else {
                        x
                    }
                };
                let s = sound as i32;
                let a = UtArgs { prog: div128(s) as i16, tone: (s & 0x7F) as i16, note: div128(pitch) as i16, fine: (pitch & 0x7F) as i16, voll: cap(l), volr: cap(r) };
                let v = self.ut_key_on(&a);
                let ch = &mut self.chans[c];
                ch.voice = v;
                ch.cur = pitch as i16;
                ch.glide = glide as i16;
                ch.target = pitch as i16;
                ch.from = pitch as i16;
                if v != -1 {
                    self.restore_stolen(v, c as i32);
                }
            } else {
                let voice = self.chans[c].voice;
                self.set_det_vvol(voice, ((l << 22) >> 16) as i16, ((r << 22) >> 16) as i16);
                let ch = &mut self.chans[c];
                if ch.glide > 0 {
                    let g = ch.glide as i32 - dt;
                    ch.glide = g as i16;
                    if (g as i16) < 0 {
                        ch.glide = 0;
                    }
                } else {
                    let old = ch.target;
                    ch.glide = glide as i16;
                    ch.target = pitch as i16;
                    ch.from = old;
                }
                if glide != 0 {
                    let v = (ch.from as i32 * ch.glide as i32).wrapping_add(ch.target as i32 * (glide - ch.glide as i32));
                    ch.cur = (v / glide) as i16;
                } else {
                    ch.cur = ch.target;
                }
                let p = ((ch.cur >> 2) as u16) & 0xFFFF;
                self.set_voice_pitch(voice, p);
            }
        }
        (l + r) / 2
    }

    /// One VBlank: the game's tick callback FUN_8005EFD8 (channel timers:
    /// a channel not refreshed for 6 VBlanks is silenced by a zero-volume
    /// key-on of tone 0 on its voice), then libsnd's flush.
    pub fn vblank(&mut self) {
        for c in 0..8 {
            let ch = self.chans[c];
            if ch.voice < 0 {
                continue;
            }
            if ch.timer <= 0 {
                let a = UtArgs { prog: 0, tone: 0, note: 34, fine: 0, voll: 0, volr: 0 };
                self.ut_key_on_v(ch.voice, &a);
                self.chans[c].voice = -1;
                self.chans[c].timer = 0;
                continue;
            }
            let mut fade = None;
            if ch.timer < 13 {
                if ch.fade > 0 {
                    fade = Some(ch.fade - 8);
                }
            } else if ch.fade < 64 {
                fade = Some(ch.fade + 8);
            }
            if let Some(f) = fade {
                self.chans[c].fade = f;
                let l = f.wrapping_mul(ch.vol_l);
                let r = f.wrapping_mul(ch.vol_r);
                self.set_det_vvol(ch.voice, l, r);
            }
            self.chans[c].timer = ch.timer - 1;
        }
        self.flush();
        self.vblanks += 1;
    }

    /// Samples of SPU output that belong to the next VBlank (735 or 736).
    pub fn samples_this_vblank(&mut self) -> usize {
        self.frac += SAMPLES_PER_VBLANK_NUM;
        let n = self.frac / SAMPLES_PER_VBLANK_DEN;
        self.frac %= SAMPLES_PER_VBLANK_DEN;
        n as usize
    }

    /// One VBlank of driver work followed by its SPU output (interleaved
    /// stereo appended to `out`).
    pub fn vblank_render(&mut self, out: &mut Vec<i16>) {
        self.vblank();
        let n = self.samples_this_vblank();
        self.spu.render(n, out);
    }

    pub fn take_keyons(&mut self) -> Vec<KeyOn> {
        std::mem::take(&mut self.keyons)
    }

    /// Seed the driver from a RAM image of the running game (2 MB main RAM
    /// at 0x80000000): channel table, libsnd voice table, shadow and
    /// envelope history; SPU voices that libsnd holds keyed are set
    /// sustaining at their read-back envelope. Verification only.
    pub fn load_ram(&mut self, ram: &[u8]) {
        let rd16 = |a: u32| i16::from_le_bytes([ram[(a & 0x1F_FFFF) as usize], ram[(a & 0x1F_FFFF) as usize + 1]]);
        let rd32 = |a: u32| i32::from_le_bytes(ram[(a & 0x1F_FFFF) as usize..(a & 0x1F_FFFF) as usize + 4].try_into().unwrap());
        for c in 0..8 {
            let b = 0x800D_8AB0 + 0x18 * c as u32;
            self.chans[c] = Channel {
                owner: rd32(b),
                sound: rd16(b + 4),
                cur: rd16(b + 6),
                target: rd16(b + 8),
                from: rd16(b + 0xA),
                glide: rd16(b + 0xC),
                fade: rd16(b + 0xE),
                timer: rd16(b + 0x10),
                voice: rd16(b + 0x12),
                vol_l: rd16(b + 0x14),
                vol_r: rd16(b + 0x16),
            };
        }
        for v in 0..VOICES {
            let b = 0x800B_C108 + 0x38 * v as u32;
            self.vm[v] = VmVoice {
                vag: rd16(b),
                age: rd16(b + 2),
                pitch: rd16(b + 4) as u16,
                env: rd16(b + 6) as u16,
                note: rd16(b + 0xE),
                seq: rd16(b + 0x10),
                prog_idx: rd16(b + 0x12),
                prog: rd16(b + 0x14),
                tone: rd16(b + 0x16),
                vab: rd16(b + 0x18),
                prio: rd16(b + 0x1A),
                key: ram[((b + 0x1D) & 0x1F_FFFF) as usize],
                vol: rd16(b + 0x36),
            };
            let s = 0x800D_8B70 + 16 * v as u32;
            self.sh[v] = Shadow {
                vol_l: rd16(s) as u16,
                vol_r: rd16(s + 2) as u16,
                pitch: rd16(s + 4) as u16,
                addr: rd16(s + 6) as u16,
                adsr1: rd16(s + 8) as u16,
                adsr2: rd16(s + 0xA) as u16,
                dirty: ram[((0x800C_1E50 + v as u32) & 0x1F_FFFF) as usize],
            };
            let vo = &mut self.spu.voices[v];
            vo.vol_l = self.sh[v].vol_l;
            vo.vol_r = self.sh[v].vol_r;
            vo.pitch = self.sh[v].pitch;
            vo.start = self.sh[v].addr;
            vo.adsr1 = self.sh[v].adsr1;
            vo.adsr2 = self.sh[v].adsr2;
            if self.vm[v].env != 0 {
                self.spu.key_on(1 << v);
                let vo = &mut self.spu.voices[v];
                vo.env = self.vm[v].env as i32;
                vo.phase = crate::spu::Phase::Sustain;
            }
        }
        for i in 0..16 {
            self.env_hist[i] = rd32(0x800E_3D78 + 4 * i as u32) as u32;
        }
        self.env_idx = (rd32(0x800D_D848) & 0xF) as usize;
        self.kon = (rd16(0x800B_4940) as u16 as u32) | ((rd16(0x800B_4942) as u16 as u32) << 16);
        self.koff = (rd16(0x800E_3DBC) as u16 as u32) | ((rd16(0x800E_3DD0) as u16 as u32) << 16);
        self.c.sfx_master = rd16(0x800B_0E70);
        self.c.release_adj = rd16(0x800C_5460);
        self.c.mono = rd16(0x800D_1598) == 1;
        self.c.reserved = rd32(0x800A_F178) as u32;
        self.c.base = rd32(0x800E_37E8) as u32;
    }
}

/// _SsVmKeyOnNow's volume [0x8009D2B0..0x8009D56C]: the key volume (signed
/// byte), the bank's, program's and tone's volumes, then the tone, program
/// and key pans (each < 64 scales the right side by pan / 63, else the left
/// by (127 - pan) / 63), MIPS 32-bit arithmetic; the shadow keeps the low
/// 16 bits. Returns (left, right) before truncation.
#[allow(clippy::too_many_arguments)]
pub fn keyon_volume(voll: i8, vab_mvol: u8, prog_mvol: i8, tone_vol: i8, tone_pan: u8, prog_pan: u8, key_pan: u8, mono: bool) -> (u32, u32) {
    let mv = vab_mvol as i32;
    let a = (voll as i32).wrapping_mul(mv * 0x4000 - mv);
    let a3 = a / 0x3F01;
    let v0 = a3.wrapping_mul(prog_mvol as i32).wrapping_mul(tone_vol as i32) as u32;
    let a2 = v0 / 0x3F01;
    let (mut l, mut r) = (a2, a2);
    for pan in [tone_pan, prog_pan, key_pan] {
        if pan < 64 {
            r = r.wrapping_mul(pan as u32) / 63;
        } else {
            l = l.wrapping_mul(127 - pan as u32) / 63;
        }
    }
    if mono {
        if l < r {
            l = r;
        } else {
            r = l;
        }
    }
    (l, r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_split() {
        let a = oneshot_args(0x24, 5000, 0x80, 0x80, 96);
        assert_eq!((a.prog, a.tone, a.note, a.fine), (0, 0x23, 39, 5000 & 0x7F));
        assert_eq!((a.voll, a.volr), (96, 96));
        let a = oneshot_args(0x57, 0x1400, 0xA0, 0xA0, 0x7F);
        assert_eq!(a.voll, 158);
        let a = oneshot_args(0x57, 0x1400, 0x200, 0x200, 0x7F);
        assert_eq!(a.voll, 160);
    }

    #[test]
    fn volume_centre_pan() {
        // Full volumes, centre pans: 127 * 127 * 127 / 127^2 ... = 0x3FFF.
        let (l, r) = keyon_volume(127, 127, 127, 127, 64, 64, 64, false);
        assert_eq!((l, r), (0x3FFF * 63 / 63, 0x3FFF));
        // Pan 0 on the tone: right side silent.
        let (l, r) = keyon_volume(127, 127, 127, 127, 0, 64, 64, false);
        assert_eq!((l, r), (0x3FFF, 0));
    }
}

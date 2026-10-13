//! particles: the game's effect particles, ported exactly (Session 4 track B;
//! docs/spec/effects.md).
//!
//! Two lists live in the race state and move every race frame:
//!
//! - [`Particles`]: 64 sprite particles at 0x800E0C20 (stride 0x3C) plus a
//!   side table at 0x800D73D0 (stride 8), a ring allocated by FUN_800870E8
//!   ([`Particles::spawn`]; ring index DAT_800B0C60), moved and faded by
//!   FUN_8008727C ([`Particles::update`], called from the race frame
//!   FUN_8007916C), drawn by FUN_80010E90 as camera-facing (or, flag 0x10,
//!   ground-flat) textured quads from the sprite definitions at
//!   *0x800D1B40 (exe table 0x800ACF40). Smoke, flames, stars, explosion
//!   puffs, splashes, the turbo-token sparkle, ...: the item code (track A)
//!   spawns them with its own random draws.
//! - [`Sparks`]: 32 line streaks at 0x800B4540 (stride 0x20), a ring
//!   (DAT_800B07EB) filled by FUN_800326B4 ([`Sparks::spawn`]) and moved by
//!   FUN_800327D0 ([`Sparks::update`]); drawn by FUN_80032928 as 3D lines
//!   from pos to pos + vel * len.
//!
//! The 0x800DF688 buffer the brief names is not a particle list: it is the
//! 64-entry batch of camera-facing billboards (FUN_80030474 adds one,
//! FUN_80011E3C draws a batch) that the render pass rebuilds every frame
//! from the pickups, projectiles, kart status effects and this module's
//! lists. That part is presentation (mr-bevy effects.rs).
//!
//! Integers exactly as the game, no floats, no randomness here (the
//! spawners' random draws are the callers').

use crate::mem::Mem;

/// Addresses [CODE].
pub mod addr {
    pub const LIST: u32 = 0x800E_0C20;
    pub const SIDE: u32 = 0x800D_73D0;
    pub const NEXT: u32 = 0x800B_0C60;
    /// 4 bytes per particle type: sprite (index into the 0x800D1B40 defs),
    /// frame count, blend flags (| 0x200), behaviour (fade / size mode).
    pub const TYPES: u32 = 0x800A_CFFC;
    pub const SPARKS: u32 = 0x800B_4540;
    pub const SPARK_NEXT: u32 = 0x800B_07EB;
    /// 1.14 sine table (the wobble behaviour).
    pub const SIN: u32 = 0x800A_88C0;
}

pub const N: usize = 64;
pub const STRIDE: u32 = 0x3C;
pub const N_SPARKS: usize = 32;

/// One sprite particle (0x800E0C20 + 0x3C * i and 0x800D73D0 + 8 * i).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Particle {
    /// +0x00..: world position (kart units).
    pub pos: [i32; 3],
    /// +0x0C..: velocity per VBlank.
    pub vel: [i32; 3],
    /// +0x18: added to vel.y every VBlank.
    pub gravity: i32,
    /// +0x1C: -1 at spawn (an owner link the item code may set).
    pub link: i32,
    /// +0x20: not touched by the spawner / mover.
    pub word20: i32,
    /// +0x24: VBlanks left; <= 0 = free (-1 for one frame after dying).
    pub life: i16,
    /// +0x26 / +0x28: half width / height (view units = world >> 5).
    pub w: i16,
    pub h: i16,
    /// +0x2A, +0x2B: zeroed at spawn.
    pub b2a: u8,
    pub b2b: u8,
    /// +0x2C: sprite definition (0 = not drawn), from the type table.
    pub sprite: u8,
    /// +0x2D: particle type (index of the 0x800ACFFC table).
    pub kind: u8,
    /// +0x2E: untouched.
    pub b2e: u8,
    /// +0x2F: behaviour (fade and size mode, 1..10).
    pub behaviour: u8,
    /// +0x30: animation frames of the sprite.
    pub frames: u8,
    /// +0x31: current frame (0..frames-1, by age).
    pub frame: u8,
    /// +0x32: draw flags: 0x60 blend (0x60 = opaque), 0x10 ground-flat,
    /// 0x200 set at spawn (the drawer keeps it as "on screen").
    pub flags: u16,
    /// +0x34: 12-bit rotation; +0x36: rotation per VBlank.
    pub angle: u16,
    pub spin: i16,
    /// +0x38..+0x3A: colour now (faded); +0x3B: GPU code 0x2C / 0x2E.
    pub rgb: [u8; 3],
    pub code: u8,
    /// Side table: base colour, life at spawn (byte), size at spawn.
    pub base_rgb: [u8; 3],
    pub life0: u8,
    pub w0: i16,
    pub h0: i16,
}

/// Arguments of FUN_800870E8, in its order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Spawn {
    pub pos: [i32; 3],
    pub vel: [i32; 3],
    pub gravity: i32,
    pub angle: u16,
    pub spin: i16,
    pub kind: u8,
    pub rgb: [u8; 3],
    pub w: i16,
    pub h: i16,
    pub life: i16,
}

/// The particle list and its allocator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Particles {
    pub list: [Particle; N],
    /// DAT_800B0C60: index of the last particle spawned.
    pub next: i32,
    /// The type table (0x800ACFFC), from the executable.
    pub types: Vec<[u8; 4]>,
    /// The two 12-bit phases of the wobble behaviour. In the game they are
    /// two stack shorts FUN_8008727C never initialises (whatever the
    /// previous call left there), advanced by 0xA0 / 0x80 per entry. Here
    /// they start each update at this value [INFER, ledger].
    pub phase: [u16; 2],
}

/// The 1.14 sine table the wobble reads (4096 entries).
pub type SinTable = [i16];

fn sext16(v: i32) -> i32 {
    v as i16 as i32
}

impl Particles {
    /// An empty list with the type table read from the executable image
    /// (`exe` = SLUS_012.37 file bytes).
    pub fn from_exe(exe: &[u8]) -> Particles {
        let off = |a: u32| (a - 0x8001_0000) as usize + 0x800;
        let t = off(addr::TYPES);
        let n = (0x800A_D0A0 - addr::TYPES) as usize / 4;
        let types = (0..n).map(|k| [exe[t + 4 * k], exe[t + 4 * k + 1], exe[t + 4 * k + 2], exe[t + 4 * k + 3]]).collect();
        Particles { list: [Particle::default(); N], next: 0, types, phase: [0, 0] }
    }

    /// FUN_800870E8: take the next ring slot (pre-increment, wraps at 64)
    /// unless the size is zero. Returns the slot.
    pub fn spawn(&mut self, s: &Spawn) -> Option<usize> {
        if s.w == 0 || s.h == 0 {
            return None;
        }
        self.next += 1;
        if self.next > 0x3F {
            self.next = 0;
        }
        let i = self.next as usize;
        let t = self.types.get(s.kind as usize).copied().unwrap_or([0; 4]);
        let flags = t[2] as u16 | 0x200;
        let p = &mut self.list[i];
        let keep20 = p.word20;
        let keep2e = p.b2e;
        *p = Particle {
            pos: s.pos,
            vel: s.vel,
            gravity: s.gravity,
            link: -1,
            word20: keep20,
            life: s.life,
            w: s.w,
            h: s.h,
            b2a: 0,
            b2b: 0,
            sprite: t[0],
            kind: s.kind,
            b2e: keep2e,
            behaviour: t[3],
            frames: t[1],
            frame: 0,
            flags,
            angle: s.angle,
            spin: s.spin,
            rgb: s.rgb,
            // FUN_800306A8: POLY_FT4, semi-transparent unless the blend is 0x60
            code: if flags & 0x60 == 0x60 { 0x2C } else { 0x2E },
            base_rgb: s.rgb,
            life0: s.life as u8,
            w0: s.w,
            h0: s.h,
        };
        Some(i)
    }

    /// FUN_8008727C: one race frame of `dt` VBlanks. `sin` = the 1.14 sine
    /// table (wobble behaviour).
    pub fn update(&mut self, dt: i32, sin: &SinTable) {
        let mut ph = self.phase;
        for p in self.list.iter_mut() {
            if p.life > 0 {
                Self::step(p, dt, sin, ph);
            }
            ph[0] = (ph[0] + 0xA0) & 0xFFF;
            ph[1] = (ph[1] + 0x80) & 0xFFF;
        }
        for p in self.list.iter_mut() {
            if p.life == -1 {
                p.life = 0;
                p.sprite = 0;
            }
        }
    }

    fn step(p: &mut Particle, dt: i32, sin: &SinTable, ph: [u16; 2]) {
        let life = sext16(p.life as i32 - dt);
        p.life = if life < 0 { 0 } else { life as i16 };
        // move: dt sub-steps; vel.y gains gravity before it is added
        let mut n = dt as u16 as i32;
        loop {
            n -= 1;
            p.pos[0] = p.pos[0].wrapping_add(p.vel[0]);
            p.pos[2] = p.pos[2].wrapping_add(p.vel[2]);
            p.vel[1] = p.vel[1].wrapping_add(p.gravity);
            p.pos[1] = p.pos[1].wrapping_add(p.vel[1]);
            if (n << 16) <= 0 {
                break;
            }
        }
        let life = p.life as i32;
        let life0 = p.life0 as i32;
        // animation frame by age
        let f = if p.frames != 0 { p.frames as i32 - 1 } else { 0 };
        if life0 != 0 {
            p.frame = (f * (life0 - life) / life0) as u8;
        }
        if p.spin != 0 {
            p.angle = ((p.angle as i32 + p.spin as i32 * (dt as i16 as i32)) & 0xFFF) as u16;
        }
        // behaviour -> (fade, size)
        let (fade, size) = match p.behaviour {
            1 => (2, 1),
            2 | 5 => (1, 0),
            3 => (2, 0),
            4 => (1, 1),
            6 => (0, 1),
            7 => (0, 2),
            8 => (2, 3),
            9 => (1, 4),
            10 => (0, 6),
            _ => (0, 0),
        };
        let q = life0 >> 2;
        let base = p.base_rgb;
        let scale = |num: i32| -> [u8; 3] { base.map(|c| if q == 0 { 0 } else { (c as i32 * num / q) as u8 }) };
        if fade == 1 {
            if life < q {
                p.rgb = scale(life);
            }
        } else if fade == 2 {
            if life < q {
                p.rgb = scale(life);
            } else if life < life0 - q {
                p.rgb = base;
            } else {
                p.rgb = scale(life0 - (life + 1));
            }
        }
        let (w0, h0) = (p.w0 as i32, p.h0 as i32);
        match size {
            // grow to double over the life
            1 => {
                if life0 != 0 {
                    let d = life0 - life;
                    p.w = (w0 + w0 * d / life0) as i16;
                    p.h = (h0 + h0 * d / life0) as i16;
                }
            }
            // shrink over the last quarter
            2 => {
                if life < q {
                    p.w = (w0 * life / q) as i16;
                    p.h = (h0 * life / q) as i16;
                }
            }
            // grow 8 per VBlank
            3 => {
                p.w = (p.w as i32 + dt * 8) as i16;
                p.h = (p.h as i32 + dt * 8) as i16;
            }
            // wobble from the sine table
            4 => {
                p.w = (sin[(ph[0] & 0xFFF) as usize] as i32 >> 7) as i16;
                p.h = (sin[(ph[1] & 0xFFF) as usize] as i32 >> 7) as i16;
            }
            // grow 1 per VBlank
            5 => {
                p.w = (p.w as i32 + dt) as i16;
                p.h = (p.h as i32 + dt) as i16;
            }
            // shrink over the whole life
            6 => {
                if life0 != 0 {
                    p.w = (w0 * life / life0) as i16;
                    p.h = (h0 * life / life0) as i16;
                }
            }
            _ => {}
        }
        if p.life == 0 {
            p.life = -1;
        }
    }

    /// Read the list from game memory (oracle captures).
    pub fn read(m: &Mem, types: Vec<[u8; 4]>) -> Particles {
        let mut list = [Particle::default(); N];
        for (i, p) in list.iter_mut().enumerate() {
            let a = addr::LIST + STRIDE * i as u32;
            let s = addr::SIDE + 8 * i as u32;
            *p = Particle {
                pos: [m.i32(a), m.i32(a + 4), m.i32(a + 8)],
                vel: [m.i32(a + 0xC), m.i32(a + 0x10), m.i32(a + 0x14)],
                gravity: m.i32(a + 0x18),
                link: m.i32(a + 0x1C),
                word20: m.i32(a + 0x20),
                life: m.i16(a + 0x24),
                w: m.i16(a + 0x26),
                h: m.i16(a + 0x28),
                b2a: m.u8(a + 0x2A),
                b2b: m.u8(a + 0x2B),
                sprite: m.u8(a + 0x2C),
                kind: m.u8(a + 0x2D),
                b2e: m.u8(a + 0x2E),
                behaviour: m.u8(a + 0x2F),
                frames: m.u8(a + 0x30),
                frame: m.u8(a + 0x31),
                flags: m.u16(a + 0x32),
                angle: m.u16(a + 0x34),
                spin: m.i16(a + 0x36),
                rgb: [m.u8(a + 0x38), m.u8(a + 0x39), m.u8(a + 0x3A)],
                code: m.u8(a + 0x3B),
                base_rgb: [m.u8(s), m.u8(s + 1), m.u8(s + 2)],
                life0: m.u8(s + 3),
                w0: m.i16(s + 4),
                h0: m.i16(s + 6),
            };
        }
        Particles { list, next: m.i32(addr::NEXT), types, phase: [0, 0] }
    }

    /// Write the list back in the game's layout (oracle harness, renderers
    /// that read game memory).
    pub fn write(&self, m: &mut Mem) {
        for (i, p) in self.list.iter().enumerate() {
            let a = addr::LIST + STRIDE * i as u32;
            let s = addr::SIDE + 8 * i as u32;
            for (k, v) in p.pos.iter().chain(p.vel.iter()).chain([p.gravity, p.link, p.word20].iter()).enumerate() {
                m.put_u32(a + 4 * k as u32, *v as u32);
            }
            m.put_u16(a + 0x24, p.life as u16);
            m.put_u16(a + 0x26, p.w as u16);
            m.put_u16(a + 0x28, p.h as u16);
            m.write(a + 0x2A, &[p.b2a, p.b2b, p.sprite, p.kind, p.b2e, p.behaviour, p.frames, p.frame]);
            m.put_u16(a + 0x32, p.flags);
            m.put_u16(a + 0x34, p.angle);
            m.put_u16(a + 0x36, p.spin as u16);
            m.write(a + 0x38, &[p.rgb[0], p.rgb[1], p.rgb[2], p.code]);
            m.write(s, &[p.base_rgb[0], p.base_rgb[1], p.base_rgb[2], p.life0]);
            m.put_u16(s + 4, p.w0 as u16);
            m.put_u16(s + 6, p.h0 as u16);
        }
        m.put_u32(addr::NEXT, self.next as u32);
    }
}

/// One streak (0x800B4540 + 0x20 * i).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Spark {
    /// +0x00..: world position; +0x0C..: velocity per VBlank (no gravity).
    pub pos: [i32; 3],
    pub vel: [i32; 3],
    /// +0x18: VBlanks left; >= 0x80 (0xFF) = free.
    pub life: u8,
    /// +0x19: tail length (the line runs from pos to pos + vel * len).
    pub len: u8,
    /// +0x1A..+0x1C: colour at spawn; +0x1D..+0x1F: colour now (fades over
    /// the last 8 VBlanks).
    pub base: [u8; 3],
    pub rgb: [u8; 3],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sparks {
    pub list: [Spark; N_SPARKS],
    /// DAT_800B07EB: next slot.
    pub next: u8,
}

impl Default for Sparks {
    fn default() -> Self {
        Sparks { list: [Spark { life: 0xFF, ..Spark::default() }; N_SPARKS], next: 0 }
    }
}

impl Sparks {
    /// FUN_800326B4(x, y, z, vx, vy, vz, life, len, r, g, b): overwrite the
    /// next ring slot (the current colour is left to the next update).
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(&mut self, pos: [i32; 3], vel: [i32; 3], life: u8, len: u8, rgb: [u8; 3]) {
        let s = &mut self.list[self.next as usize];
        s.life = life;
        s.pos = pos;
        s.vel = vel;
        s.len = len;
        s.base = rgb;
        self.next += 1;
        if self.next == 0x20 {
            self.next = 0;
        }
    }

    /// FUN_800327D0: one race frame of `dt` VBlanks (every slot, live or not).
    pub fn update(&mut self, dt: i32) {
        for s in self.list.iter_mut() {
            let l = (s.life as i32 - dt) as u8;
            s.life = if l & 0x80 != 0 { 0xFF } else { l };
            let ls = s.life as i8 as i32;
            if ls < 8 {
                for k in 0..3 {
                    let v = s.base[k] as i32 * ls;
                    s.rgb[k] = ((if v < 0 { v + 7 } else { v }) >> 3) as u8;
                }
            } else {
                s.rgb = s.base;
            }
            s.pos[0] = s.pos[0].wrapping_add(s.vel[0].wrapping_mul(dt));
            s.pos[2] = s.pos[2].wrapping_add(s.vel[2].wrapping_mul(dt));
            let mut n = dt;
            loop {
                n -= 1;
                s.pos[1] = s.pos[1].wrapping_add(s.vel[1]);
                if (n << 16) <= 0 {
                    break;
                }
            }
        }
    }

    pub fn read(m: &Mem) -> Sparks {
        let mut list = [Spark::default(); N_SPARKS];
        for (i, s) in list.iter_mut().enumerate() {
            let a = addr::SPARKS + 0x20 * i as u32;
            *s = Spark {
                pos: [m.i32(a), m.i32(a + 4), m.i32(a + 8)],
                vel: [m.i32(a + 0xC), m.i32(a + 0x10), m.i32(a + 0x14)],
                life: m.u8(a + 0x18),
                len: m.u8(a + 0x19),
                base: [m.u8(a + 0x1A), m.u8(a + 0x1B), m.u8(a + 0x1C)],
                rgb: [m.u8(a + 0x1D), m.u8(a + 0x1E), m.u8(a + 0x1F)],
            };
        }
        Sparks { list, next: m.u8(addr::SPARK_NEXT) }
    }

    pub fn write(&self, m: &mut Mem) {
        for (i, s) in self.list.iter().enumerate() {
            let a = addr::SPARKS + 0x20 * i as u32;
            for (k, v) in s.pos.iter().chain(s.vel.iter()).enumerate() {
                m.put_u32(a + 4 * k as u32, *v as u32);
            }
            m.write(a + 0x18, &[s.life, s.len, s.base[0], s.base[1], s.base[2], s.rgb[0], s.rgb[1], s.rgb[2]]);
        }
        m.put_u8(addr::SPARK_NEXT, self.next);
    }
}

// ---------------------------------------------------------------------------
// The flat-quad list 0x800C84B8 (Session 5 track F)
// ---------------------------------------------------------------------------

/// Addresses of the flat-quad list [CODE].
pub mod flat_addr {
    pub const LIST: u32 = 0x800C_84B8;
    pub const NEXT: u32 = 0x800B_07EC;
}
pub const N_FLATS: usize = 32;
pub const FLAT_STRIDE: u32 = 0x34;

/// One quad of the list 0x800C84B8 (0x34 bytes), drawn by FUN_800342B8:
/// a GT4 at `pos` with four corners (view units, scaled by `scale / 128`,
/// rotated by the camera only), optionally split 2 x 2 with the texture
/// mirrored into each quarter (flag 0x80).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Flat {
    /// +0x00: world position.
    pub pos: [i32; 3],
    /// +0x0C..+0x23: corners v0, v1, v2, v3 (the GPU gets v0, v1, v3, v2).
    pub corners: [[i16; 3]; 4],
    /// +0x24 u, +0x25 v, +0x26 width, +0x27 height (texels).
    pub uv: [u8; 4],
    /// +0x28: tpage ABR bits (0x60 = opaque, 0x20 additive, 0x40 B - F).
    pub abr: i16,
    /// +0x2A: VBlanks left (bit 15: dies after this frame's draw; 0 free).
    pub life: u16,
    /// +0x2C texture slot, +0x2D CLUT add.
    pub slot: u8,
    pub clut: u8,
    /// +0x2E: scale (0x80 = 1.0); +0x2F flags: 0x80 split 2 x 2, 1 colour
    /// gradient (far edge black), 2 fade over the last 16 VBlanks.
    pub scale: u8,
    pub flags: u8,
    /// +0x30: scale growth per VBlank (signed).
    pub grow: i8,
    /// +0x31..+0x33.
    pub rgb: [u8; 3],
}

/// The list, its ring index DAT_800B07EC and the per-wheel skid intensity
/// bytes FUN_80033440 keeps in the kart record (+0x3F + 0x2C * wheel), held
/// here per kart slot.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Flats {
    pub list: [Flat; N_FLATS],
    pub next: u8,
    pub skid: [[u8; 8]; 8],
}

fn i16r(r: &[u8], o: usize) -> i32 {
    i16::from_le_bytes([r[o], r[o + 1]]) as i32
}
fn i32r(r: &[u8], o: usize) -> i32 {
    i32::from_le_bytes([r[o], r[o + 1], r[o + 2], r[o + 3]])
}
/// C `(a + (2^k - 1 if a < 0)) >> k` (division toward zero by 2^k).
fn dz(a: i32, k: u32) -> i32 {
    (if a < 0 { a + ((1 << k) - 1) } else { a }) >> k
}

impl Flats {
    fn take(&mut self) -> &mut Flat {
        let i = self.next as usize % N_FLATS;
        self.next = self.next.wrapping_add(1);
        if self.next == 0x20 {
            self.next = 0;
        }
        &mut self.list[i]
    }

    /// FUN_800330D4(x, y, z, 0, size): a splash ring on the water, flat
    /// (the default matrix 0x800A87B0 is the identity), additive, growing
    /// 2 per VBlank from 0x40 and fading out over its 32 VBlanks; split
    /// 2 x 2 with slot 0x15's (0x60, 0xE0) 32 x 32 quarter ring mirrored.
    pub fn splash(&mut self, pos: [i32; 3], size: i32) {
        let s = size;
        let c = |a: i32| (a >> 12) as i16;
        // a = (0x1000, 0, 0), c = (0, 0, 0x1000): v0 = s(c - a), v1 = s(a + c),
        // v2 = s(a - c), v3 = -s(a + c)
        let f = self.take();
        *f = Flat {
            pos,
            corners: [
                [c(-s * 0x1000), 0, c(s * 0x1000)],
                [c(s * 0x1000), 0, c(s * 0x1000)],
                [c(s * 0x1000), 0, c(-s * 0x1000)],
                [c(-s * 0x1000), 0, c(-s * 0x1000)],
            ],
            uv: [0x60, 0xE0, 0x20, 0x20],
            abr: 0x20,
            life: 0x20,
            slot: 0x15,
            clut: 0xD,
            scale: 0x40,
            flags: 0x82,
            grow: 2,
            rgb: [0x80; 3],
        };
    }

    /// FUN_80033440(kart): per wheel (+0x171 of them) the skid intensity
    /// (+0x3F) rises 5 per VBlank to 0x55 while sliding on the ground
    /// (+0x4E0 on the ground and +0x4C9, or a driven wheel with +0x486 /
    /// +0x4C4 / +0x4B9, or +0x4E1) and falls 4 per VBlank otherwise; with
    /// some left, a B - F quad from the wheel's last to its new position
    /// (when it moved at least +0x38 per VBlank), one frame long.
    pub fn skids(&mut self, slot: usize, r: &[u8], dt: i32, t: &crate::tables::Tables) {
        let on_ground = r[0x4E0] != 0;
        let n = (r[0x171] as usize).min(8);
        for i in 0..n {
            let b = 0x14 + 0x2C * i;
            let flags = u16::from_le_bytes([r[b + 0x26], r[b + 0x27]]);
            let sliding = on_ground
                && (r[0x4C9] != 0
                    || (flags & 1 != 0 && (i16r(r, 0x486) != 0 || r[0x4C4] != 0 || r[0x4B9] != 0))
                    || r[0x4E1] != 0);
            let k = &mut self.skid[slot.min(7)][i];
            if !sliding {
                let v = (*k as i32 - 4 * dt) as u8;
                *k = if (v as i8) < 0 { 0 } else { v };
            } else {
                let v = (*k as i32 + 5 * dt) as u8;
                *k = if (v as i8) > 0x55 { 0x55 } else { v };
            }
            let intensity = *k;
            if !on_ground || intensity == 0 {
                continue;
            }
            let ddx = (i32r(r, b).wrapping_sub(i32r(r, b + 4))).wrapping_mul(0x800) >> 16;
            let ddz = (i32r(r, b + 0x10).wrapping_sub(i32r(r, b + 0x14))).wrapping_mul(0x800) >> 16;
            let len = crate::tables::square_root0(t, ddx * ddx + ddz * ddz);
            let spd = ((len << 3) / dt.max(1)) as i16 as i32;
            let min_len = i16r(r, b + 0x24);
            if spd < min_len {
                continue;
            }
            let mut a = (crate::tables::ratan2(t, ddx, ddz) as u32).wrapping_sub(u16::from_le_bytes([r[0xE], r[0xF]]) as u32);
            let ai = ((a << 16) as i32) >> 16;
            if ai > 0x800 {
                a = a.wrapping_sub(0x1000);
            } else if ai < -0x800 {
                a = a.wrapping_add(0x1000);
            }
            let ang = (a & 0xFFF) as i32;
            let s = dz(t.sin_raw(ang), 2);
            let c = dz(t.cos_raw(ang), 2);
            let h = min_len / 2;
            let (ox, oz) = (i16r(r, b + 0x18), i16r(r, b + 0x1C));
            let q = |v: i32| dz(v, 12);
            let p0 = (-(ox + q(-h * c)), -(oz + q(-(-h * s))));
            let p1 = (-(ox + q(h * c)), -(oz + q(-(h * s))));
            let p2 = (-(ox + q(h * c + -spd * s)), -(oz + q(-(h * s) + -spd * c)));
            let p3 = (-(ox + q(-h * c + -spd * s)), -(oz + q(h * s + -spd * c)));
            let m = |o: usize| i16r(r, o);
            let rot = |p: (i32, i32)| -> [i16; 3] {
                let (x, z) = (p.0 as i16 as i32, p.1 as i16 as i32);
                [((x * m(0x18C) + z * m(0x190)) >> 12) as i16, ((x * m(0x192) + z * m(0x196)) >> 12) as i16, ((x * m(0x198) + z * m(0x19C)) >> 12) as i16]
            };
            let pos = [i32r(r, 0), i32r(r, 4), i32r(r, 8)];
            let col = intensity.wrapping_mul(3);
            let f = self.take();
            *f = Flat {
                pos,
                corners: [rot(p0), rot(p1), rot(p2), rot(p3)],
                uv: [0xF, 0xF, 2, 2],
                abr: 0x40,
                life: 1,
                slot: 0x14,
                clut: 0,
                scale: 0x80,
                flags: 1,
                grow: 0,
                rgb: [col; 3],
            };
        }
    }

    /// FUN_80033B98: one race frame of `dt` VBlanks. A quad whose life runs
    /// out is still drawn this frame (life = 0x8000) and freed by the next.
    pub fn update(&mut self, dt: i32) {
        for f in self.list.iter_mut() {
            let l = f.life & 0x7FFF;
            f.life = l;
            if l != 0 {
                let n = (l as i32 - dt) as i16;
                f.life = n as u16;
                if n < 0 {
                    f.life = 0x8000;
                }
                let s = f.scale as i32 + f.grow as i32 * dt;
                f.scale = if (s as i16) > 0xFF { 0xFF } else { s as u8 };
            }
        }
    }

    pub fn read(m: &Mem) -> Flats {
        let mut list = [Flat::default(); N_FLATS];
        for (i, f) in list.iter_mut().enumerate() {
            let a = flat_addr::LIST + FLAT_STRIDE * i as u32;
            let mut corners = [[0i16; 3]; 4];
            for (k, c) in corners.iter_mut().enumerate() {
                for (j, v) in c.iter_mut().enumerate() {
                    *v = m.i16(a + 0xC + 6 * k as u32 + 2 * j as u32);
                }
            }
            *f = Flat {
                pos: [m.i32(a), m.i32(a + 4), m.i32(a + 8)],
                corners,
                uv: [m.u8(a + 0x24), m.u8(a + 0x25), m.u8(a + 0x26), m.u8(a + 0x27)],
                abr: m.i16(a + 0x28),
                life: m.u16(a + 0x2A),
                slot: m.u8(a + 0x2C),
                clut: m.u8(a + 0x2D),
                scale: m.u8(a + 0x2E),
                flags: m.u8(a + 0x2F),
                grow: m.u8(a + 0x30) as i8,
                rgb: [m.u8(a + 0x31), m.u8(a + 0x32), m.u8(a + 0x33)],
            };
        }
        Flats { list, next: m.u8(flat_addr::NEXT), skid: [[0; 8]; 8] }
    }
}

#[cfg(test)]
mod flat_tests {
    use super::*;

    #[test]
    fn splash_grows_fades_and_dies_after_its_last_draw() {
        let mut f = Flats::default();
        f.splash([0, 0, 0], 0xC0);
        assert_eq!(f.list[0].corners[0], [-0xC0, 0, 0xC0]);
        assert_eq!(f.next, 1);
        for _ in 0..15 {
            f.update(2);
        }
        assert_eq!(f.list[0].life, 2);
        assert_eq!(f.list[0].scale, 0x40 + 2 * 2 * 15);
        f.update(2);
        assert_eq!(f.list[0].life, 0);
        f.update(2);
        assert_eq!(f.list[0].life, 0);
        f.splash([0, 0, 0], 1);
        f.list[1].life = 1;
        f.update(2);
        assert_eq!(f.list[1].life, 0x8000, "drawn once more, then freed");
        f.update(2);
        assert_eq!(f.list[1].life, 0);
    }

    #[test]
    fn ring_wraps_at_32() {
        let mut f = Flats::default();
        for _ in 0..33 {
            f.splash([0, 0, 0], 1);
        }
        assert_eq!(f.next, 1);
    }
}

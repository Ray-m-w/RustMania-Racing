//! Items, weapons and specials (Session 4): the race-wide item state and the
//! game code that runs it.
//!
//! - pickups.rs: the pickup objects (item boxes, turbo tokens): FUN_8007B448
//!   (race load) and FUN_8007BBF0 (every frame, after the kart loop).
//! - weapons.rs: firing (FUN_800645F4 -> FUN_80068C6C, from the kart update)
//!   and the projectiles (FUN_80069648, after the kart loop): flight, walls,
//!   ground, hits on karts.
//! - specials.rs: the character special (R2, FUN_8006B828), the special move
//!   (full-meter R1, kart +0x4D6, FUN_800913F4) and the effect driver
//!   FUN_8006C388 (effect +0x4F1, timer +0x468, global 0x800B07F4/07F6).
//!
//! Integers exactly as the game; the random byte pointer 0x800B0BCC and the
//! BIOS rand() are drawn where the game draws them (particle and sound calls
//! are not ported, their random draws are). Model and addresses:
//! docs/spec/items.md.

pub mod kart_fx;
pub mod pickups;
pub mod specials;
pub mod weapons;

use crate::mem::Mem;

/// Pickup object table [RAM 0x800E1B70], 0x20 bytes per object.
pub const PICKUP_TABLE: u32 = 0x800E_1B70;
pub const PICKUP_SLOTS: usize = 0xC0;
/// Item boxes occupy slots 0..0x30, challenge objects 0x30..0x40, turbo
/// tokens 0x40.. (FUN_8007B448).
pub const FIRST_CHALLENGE: usize = 0x30;
pub const FIRST_TOKEN: usize = 0x40;
/// Projectile table [RAM 0x800DDC50], 4 x 0x48 bytes.
pub const PROJECTILE_TABLE: u32 = 0x800D_DC50;
pub const PROJECTILES: usize = 4;

/// What a pickup slot holds (by its index range).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickupKind {
    /// Item box: gives a weapon slot (0..0x30).
    Box,
    /// Challenge-mode object (0x30..0x40): hidden in a race.
    Challenge,
    /// Turbo token: +12 on the turbo meter (0x40..).
    Token,
}

/// One pickup object (0x800E1B70 + 0x20 * i).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pickup {
    /// +0x00 / +0x08: position, TER units (world >> 5).
    pub x: i32,
    pub z: i32,
    /// +0x04: height, world units (bounces on `floor_y` under gravity).
    pub y: i32,
    /// +0x0C: rest height (world units).
    pub floor_y: i32,
    /// +0x10: vertical speed.
    pub vy: i32,
    /// +0x14: 0 unused, 1 present, 2 gone (respawn timer), 3 collected
    /// (shrinking), 4 growing back.
    pub state: u16,
    /// +0x16: draw scale 0..=0x80.
    pub scale: i16,
    /// +0x18: scale speed (shrink / grow).
    pub scale_vel: i16,
    /// +0x1A: respawn timer (VBlanks).
    pub timer: i16,
    /// +0x1C / +0x1D: not used by the race code (kept for the record).
    pub b1c: u8,
    pub b1d: u8,
    /// +0x1E: particle burst requested (count, drawn by FUN_8007BBF0).
    pub burst: u8,
    /// +0x1F: token colour 0..3 (rand() >> 13 at load; re-rolled on
    /// respawn); copied to the kart's +0x4F4 when collected.
    pub colour: u8,
}

impl Pickup {
    pub fn read(b: &[u8]) -> Pickup {
        let i = |o: usize| i32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        let h = |o: usize| i16::from_le_bytes([b[o], b[o + 1]]);
        Pickup {
            x: i(0),
            y: i(4),
            z: i(8),
            floor_y: i(0xC),
            vy: i(0x10),
            state: h(0x14) as u16,
            scale: h(0x16),
            scale_vel: h(0x18),
            timer: h(0x1A),
            b1c: b[0x1C],
            b1d: b[0x1D],
            burst: b[0x1E],
            colour: b[0x1F],
        }
    }

    pub fn write(&self, b: &mut [u8]) {
        b[0..4].copy_from_slice(&self.x.to_le_bytes());
        b[4..8].copy_from_slice(&self.y.to_le_bytes());
        b[8..12].copy_from_slice(&self.z.to_le_bytes());
        b[0xC..0x10].copy_from_slice(&self.floor_y.to_le_bytes());
        b[0x10..0x14].copy_from_slice(&self.vy.to_le_bytes());
        b[0x14..0x16].copy_from_slice(&self.state.to_le_bytes());
        b[0x16..0x18].copy_from_slice(&self.scale.to_le_bytes());
        b[0x18..0x1A].copy_from_slice(&self.scale_vel.to_le_bytes());
        b[0x1A..0x1C].copy_from_slice(&self.timer.to_le_bytes());
        b[0x1C] = self.b1c;
        b[0x1D] = self.b1d;
        b[0x1E] = self.burst;
        b[0x1F] = self.colour;
    }

    /// Position in world (kart) units.
    pub fn pos(&self) -> [i32; 3] {
        [self.x << 5, self.y, self.z << 5]
    }
}

pub fn pickup_kind(i: usize) -> PickupKind {
    if i < FIRST_CHALLENGE {
        PickupKind::Box
    } else if i < FIRST_TOKEN {
        PickupKind::Challenge
    } else {
        PickupKind::Token
    }
}

/// One projectile (0x800DDC50 + 0x48 * i). Field names after their use in
/// FUN_80068C6C / FUN_80069648.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Projectile {
    /// +0x00..+0x08: position, world units.
    pub pos: [i32; 3],
    /// +0x0C..+0x14: velocity per VBlank.
    pub vel: [i32; 3],
    /// +0x18: ground height under it (hover reference).
    pub ground_y: i32,
    /// +0x1C / +0x20: the path point it chases (flag 0x100), world units.
    pub target: [i32; 2],
    /// +0x24 / +0x28: the chase point moving toward `target`.
    pub chase: [i32; 2],
    /// +0x2C: slot of the kart that fired it.
    pub owner: u8,
    pub b2d: u8,
    /// +0x2E: live.
    pub active: bool,
    /// +0x2F / +0x30: touching the floor / the FACE_ALT surface (latched
    /// for one frame, bounce once).
    pub on_floor: u8,
    pub on_alt: u8,
    pub b31: u8,
    /// +0x32: path direction (1 forward, -1 back).
    pub dir: i8,
    /// +0x33: on the ground this frame.
    pub grounded: u8,
    /// +0x34: weapon flags (0x1D, 0x280, 0x588, 0x78 for types 0..3).
    pub flags: u16,
    /// +0x36 / +0x38: age and lifetime, VBlanks.
    pub age: i16,
    pub life: i16,
    /// +0x3A / +0x3C: draw angle and its rate.
    pub spin: i16,
    pub spin_rate: i16,
    /// +0x3E / +0x40: TER path segment / point of `target`.
    pub seg: i16,
    pub pt: i16,
    /// +0x42: sound repeat timer.
    pub sound_timer: i16,
    /// +0x44: RAM pointer of the face under it (as the game keeps it).
    pub face_ptr: u32,
}

impl Projectile {
    pub fn read(b: &[u8]) -> Projectile {
        let i = |o: usize| i32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        let h = |o: usize| i16::from_le_bytes([b[o], b[o + 1]]);
        Projectile {
            pos: [i(0), i(4), i(8)],
            vel: [i(0xC), i(0x10), i(0x14)],
            ground_y: i(0x18),
            target: [i(0x1C), i(0x20)],
            chase: [i(0x24), i(0x28)],
            owner: b[0x2C],
            b2d: b[0x2D],
            active: b[0x2E] != 0,
            on_floor: b[0x2F],
            on_alt: b[0x30],
            b31: b[0x31],
            dir: b[0x32] as i8,
            grounded: b[0x33],
            flags: h(0x34) as u16,
            age: h(0x36),
            life: h(0x38),
            spin: h(0x3A),
            spin_rate: h(0x3C),
            seg: h(0x3E),
            pt: h(0x40),
            sound_timer: h(0x42),
            face_ptr: i(0x44) as u32,
        }
    }

    pub fn write(&self, b: &mut [u8]) {
        let mut p32 = |o: usize, v: i32| b[o..o + 4].copy_from_slice(&v.to_le_bytes());
        for k in 0..3 {
            p32(4 * k, self.pos[k]);
            p32(0xC + 4 * k, self.vel[k]);
        }
        p32(0x18, self.ground_y);
        p32(0x1C, self.target[0]);
        p32(0x20, self.target[1]);
        p32(0x24, self.chase[0]);
        p32(0x28, self.chase[1]);
        p32(0x44, self.face_ptr as i32);
        b[0x2C] = self.owner;
        b[0x2D] = self.b2d;
        b[0x2E] = self.active as u8;
        b[0x2F] = self.on_floor;
        b[0x30] = self.on_alt;
        b[0x31] = self.b31;
        b[0x32] = self.dir as u8;
        b[0x33] = self.grounded;
        let mut p16 = |o: usize, v: i16| b[o..o + 2].copy_from_slice(&v.to_le_bytes());
        p16(0x34, self.flags as i16);
        p16(0x36, self.age);
        p16(0x38, self.life);
        p16(0x3A, self.spin);
        p16(0x3C, self.spin_rate);
        p16(0x3E, self.seg);
        p16(0x40, self.pt);
        p16(0x42, self.sound_timer);
    }
}

/// Something that happened this tick, for the HUD / effects / sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemEvent {
    /// A kart drove through a present pickup and took it.
    Pickup { kart: u8, index: u16, kind: PickupKind },
    /// A weapon was fired from slot 0 (L1) / 1 (L2).
    Fired { kart: u8, slot: u8, weapon: u8, projectile: u8 },
    /// A projectile hit a kart ("-1 HIT" for the victim, "+1" for the shooter).
    Hit { victim: u8, shooter: u8, projectile: u8 },
    /// A projectile ended (lifetime, wall, replaced): explosion at `pos`.
    Explode { projectile: u8, pos: [i32; 3] },
    /// R2 with a full meter: the character special (`kind` = the
    /// character's special type, 0..24) on every other kart.
    Special { kart: u8, kind: u8 },
    /// R1 with a full meter: the special move (+0x4D6).
    SpecialMove { kart: u8 },
}

/// Race-wide item state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemState {
    pub pickups: Vec<Pickup>,
    /// 0x800B0B60 / 0x800B0A52: item boxes / turbo tokens on the course.
    pub boxes: u8,
    pub tokens: u8,
    /// 0x800B0EBC / 0x800B0B98 / 0x800B0A58: challenge-object counts.
    pub challenge: [u8; 3],
    pub projectiles: [Projectile; PROJECTILES],
    /// 0x800B07E0: the projectile slot the next shot uses (round robin).
    pub next_projectile: u8,
    /// 0x800B07F4: character special in progress (its type), -1 none.
    pub special: i8,
    /// 0x800B07F6: its clock, VBlanks (0..=300).
    pub special_clock: i16,
    /// 0x800B0E95: slot of the kart that started it.
    pub special_owner: u8,
    /// 0x800B0B90: four token spin angles FUN_8003C5FC sets each frame
    /// (camera roll + a wobble; the tokens' draw angles).
    pub token_spin: [i16; 4],
    /// 0x800B07B0 / 0x800B07B1: two blink toggles and their timers stepped
    /// by FUN_8007A9DC (random byte draws).
    pub blink: [u8; 4],
    /// 0x800E02C0: save-data bits, two words per character: which
    /// characters' vehicles its special has hit (FUN_8006B828).
    pub hit_bits: Vec<u32>,
    /// Part counts of each kart's vehicle and character models (the model
    /// table 0x800DD598 entries the kart's +0x4A4 / +0x4A5 name; effect 8).
    pub kart_parts: [(i16, i16); 8],
    /// This tick's events (not game state).
    pub events: Vec<ItemEvent>,
}

impl Default for ItemState {
    fn default() -> Self {
        ItemState {
            pickups: vec![Pickup::default(); PICKUP_SLOTS],
            boxes: 0,
            tokens: 0,
            challenge: [0; 3],
            projectiles: [Projectile::default(); PROJECTILES],
            next_projectile: 0,
            special: -1,
            special_clock: 0,
            special_owner: 0,
            blink: [0; 4],
            token_spin: [0; 4],
            hit_bits: vec![0; 2 * HIT_CHARACTERS],
            kart_parts: [(0, 0); 8],
            events: Vec::new(),
        }
    }
}

/// Characters with a word pair at 0x800E02C0.
pub const HIT_CHARACTERS: usize = 25;
pub const HIT_BITS: u32 = 0x800E_02C0;
/// The model table (pointers to model descriptors; +4 first part, +6 end).
pub const MODEL_TABLE: u32 = 0x800D_D598;

pub mod addr {
    pub const BOXES: u32 = 0x800B_0B60;
    pub const TOKENS: u32 = 0x800B_0A52;
    pub const CHALLENGE: [u32; 3] = [0x800B_0EBC, 0x800B_0B98, 0x800B_0A58];
    pub const NEXT_PROJECTILE: u32 = 0x800B_07E0;
    pub const SPECIAL: u32 = 0x800B_07F4;
    pub const SPECIAL_CLOCK: u32 = 0x800B_07F6;
    pub const SPECIAL_OWNER: u32 = 0x800B_0E95;
    pub const BLINK: u32 = 0x800B_07B0;
    pub const TOKEN_SPIN: u32 = 0x800B_0B90;
}

impl ItemState {
    /// The item state as the game holds it in memory.
    pub fn read(m: &Mem) -> ItemState {
        let pickups = (0..PICKUP_SLOTS).map(|i| Pickup::read(m.slice(PICKUP_TABLE + 0x20 * i as u32, 0x20))).collect();
        let projectiles = std::array::from_fn(|i| Projectile::read(m.slice(PROJECTILE_TABLE + 0x48 * i as u32, 0x48)));
        ItemState {
            pickups,
            boxes: m.u8(addr::BOXES),
            tokens: m.u8(addr::TOKENS),
            challenge: [m.u8(addr::CHALLENGE[0]), m.u8(addr::CHALLENGE[1]), m.u8(addr::CHALLENGE[2])],
            projectiles,
            next_projectile: m.u8(addr::NEXT_PROJECTILE),
            special: m.i8(addr::SPECIAL),
            special_clock: m.i16(addr::SPECIAL_CLOCK),
            special_owner: m.u8(addr::SPECIAL_OWNER),
            blink: [m.u8(addr::BLINK), m.u8(addr::BLINK + 1), m.u8(addr::BLINK + 2), m.u8(addr::BLINK + 3)],
            token_spin: std::array::from_fn(|k| m.i16(addr::TOKEN_SPIN + 2 * k as u32)),
            hit_bits: (0..2 * HIT_CHARACTERS as u32).map(|i| m.u32(HIT_BITS + 4 * i)).collect(),
            kart_parts: std::array::from_fn(|j| {
                let b = crate::kart::KART_ARRAY + 0x504 * j as u32;
                let parts = |slot: u8| -> i16 {
                    if slot == 0xFF {
                        return 0;
                    }
                    let d = m.u32(MODEL_TABLE + 4 * slot as u32);
                    if d & 0x1FFF_FFFF >= 0x20_0000 || d == 0 {
                        return 0;
                    }
                    m.u16(d + 6).wrapping_sub(m.u16(d + 4)) as i16
                };
                (parts(m.u8(b + 0x4A4)), parts(m.u8(b + 0x4A5)))
            }),
            events: Vec::new(),
        }
    }

    pub fn write(&self, m: &mut Mem) {
        for (i, p) in self.pickups.iter().enumerate() {
            p.write(m.slice_mut(PICKUP_TABLE + 0x20 * i as u32, 0x20));
        }
        for (i, p) in self.projectiles.iter().enumerate() {
            p.write(m.slice_mut(PROJECTILE_TABLE + 0x48 * i as u32, 0x48));
        }
        m.put_u8(addr::BOXES, self.boxes);
        m.put_u8(addr::TOKENS, self.tokens);
        for k in 0..3 {
            m.put_u8(addr::CHALLENGE[k], self.challenge[k]);
        }
        m.put_u8(addr::NEXT_PROJECTILE, self.next_projectile);
        m.put_u8(addr::SPECIAL, self.special as u8);
        m.put_u16(addr::SPECIAL_CLOCK, self.special_clock as u16);
        m.put_u8(addr::SPECIAL_OWNER, self.special_owner);
        for k in 0..4 {
            m.put_u8(addr::BLINK + k as u32, self.blink[k]);
        }
        for k in 0..4 {
            m.put_u16(addr::TOKEN_SPIN + 2 * k as u32, self.token_spin[k] as u16);
        }
        for (i, w) in self.hit_bits.iter().enumerate() {
            m.put_u32(HIT_BITS + 4 * i as u32, *w);
        }
    }
}

/// Particles and spark streaks (track B's `mr_sim::particles`): the game's
/// lists 0x800E0C20 / 0x800B4540, filled by the item code's spawns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fx {
    pub particles: crate::particles::Particles,
    pub sparks: crate::particles::Sparks,
    /// The flat-quad list 0x800C84B8 (splash rings, skid quads; track F).
    pub flats: crate::particles::Flats,
}

/// FUN_800870E8(x, y, z, vx, vy, vz, gravity, angle, spin, kind, r, g, b,
/// w, h, life) into the race's particle list (none in analysis runs).
#[allow(clippy::too_many_arguments)]
pub fn particle(g: &mut crate::globals::RaceGlobals, pos: [i32; 3], vel: [i32; 3], gravity: i32, angle: i32, spin: i32, kind: i32, rgb: [i32; 3], w: i32, h: i32, life: i32) {
    if let Some(fx) = g.fx.as_mut() {
        fx.particles.spawn(&crate::particles::Spawn {
            pos,
            vel,
            gravity,
            angle: angle as u16,
            spin: spin as i16,
            kind: kind as u8,
            rgb: [rgb[0] as u8, rgb[1] as u8, rgb[2] as u8],
            w: w as i16,
            h: h as i16,
            life: life as i16,
        });
    }
}

/// FUN_800326B4(x, y, z, vx, vy, vz, life, len, r, g, b): a spark streak.
pub fn spark(g: &mut crate::globals::RaceGlobals, pos: [i32; 3], vel: [i32; 3], life: i32, len: i32, rgb: [i32; 3]) {
    if let Some(fx) = g.fx.as_mut() {
        fx.sparks.spawn(pos, vel, life as u8, len as u8, [rgb[0] as u8, rgb[1] as u8, rgb[2] as u8]);
    }
}

/// Positions the item code checks against: the human players' cameras
/// (+0x34..+0x3C of 0x800C4F40 + 0x70 * i, TER units) as they were left by
/// the previous frame (the camera runs after the item code).
#[derive(Clone, Copy, Debug, Default)]
pub struct Cameras {
    pub pos: [[i32; 3]; 2],
    pub n: usize,
}

impl Cameras {
    /// FUN_8007B3A0(x, y, z, r) == 0 / the inlined copy in FUN_8007BBF0:
    /// within `r` of some human camera on every axis (TER units).
    pub fn near(&self, x: i32, y: i32, z: i32, r: i32) -> bool {
        let span = (r as u32) << 1;
        self.pos[..self.n].iter().any(|c| {
            (x.wrapping_sub(c[0]).wrapping_add(r) as u32) <= span
                && (z.wrapping_sub(c[2]).wrapping_add(r) as u32) <= span
                && (y.wrapping_sub(c[1]).wrapping_add(r) as u32) <= span
        })
    }
}

/// The item code of the race frame after the kart loop and the render pass
/// (FUN_8007916C @0x80079A8C): FUN_8006C388 (effects of a character
/// special), FUN_8007F03C (kart particles, kart_fx.rs, from `recs` = the
/// karts' records; `skip` Some: a fixed pointer step instead, analysis),
/// FUN_8007A9DC (two blink timers), FUN_8007BBF0 (pickups), FUN_80069648
/// (projectiles). `items` false: Session 3's items-off race (only the blink
/// timers run).
#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
pub fn after_kart_loop(
    karts: &mut [crate::kart::KartState],
    recs: &mut [&mut Vec<u8>],
    g: &mut crate::globals::RaceGlobals,
    w: &crate::world::World,
    cams: &Cameras,
    items: bool,
    skip: Option<u32>,
    rand: &mut dyn FnMut() -> i32,
) {
    if items {
        specials::effects_frame(karts, g, w, cams, rand);
    }
    match skip {
        // Analysis: the searched pointer step in place of FUN_8007F03C.
        Some(d) => g.rng = g.rng.wrapping_add(d),
        None if items => {
            // FUN_8007F03C reads the live records: write the karts back
            // after the effects.
            for (k, rec) in karts.iter().zip(recs.iter_mut()) {
                k.to_record(rec);
            }
            let rs: Vec<&[u8]> = recs.iter().map(|r| r.as_slice()).collect();
            // 60 Hz mode: the spawner runs once per pair with the pair's
            // dt (its spawn tries, random draws and rain tries keep the
            // game's rate; the particles themselves move every tick).
            if g.hz60.beat() {
                let dt = g.dt;
                if g.hz60.on {
                    g.dt = 2;
                }
                kart_fx::frame(&rs, g, w, cams);
                g.dt = dt;
            }
        }
        None => {}
    }
    blink_timers(g, w);
    if items {
        pickups::frame(karts, g, w, &w.pickups, cams, rand);
        weapons::frame(karts, g, w, rand);
    }
}

/// FUN_8007A9DC's first loop: two toggles (0x800B07B0 / 0x800B07B2) flip
/// every (*rng >> 4) + 7 VBlanks (timers 0x800B07B1 / 0x800B07B3). The rest
/// of the function animates textures (render side).
pub fn blink_timers(g: &mut crate::globals::RaceGlobals, w: &crate::world::World) {
    for i in 0..2 {
        let t = (g.items.blink[2 * i + 1] as i32).wrapping_sub(g.dt);
        g.items.blink[2 * i + 1] = t as u8;
        if (t << 24) < 1 {
            let b = crate::update::rng_next(g, w);
            g.items.blink[2 * i + 1] = (b >> 4) + 7;
            g.items.blink[2 * i] = (g.items.blink[2 * i] + 1) & 1;
        }
    }
}

/// The end of FUN_8003C5FC (0x8003C9xx): four token spin angles, the
/// active camera's roll plus a sine wobble on the frame counter
/// 0x800B0D90.
pub fn token_spins(g: &mut crate::globals::RaceGlobals, w: &crate::world::World, camera_roll: i16) {
    let t = &w.tables;
    let f = (g.frame & 0x3F) * 0x40;
    let i1 = (f + 0x400) & 0xFFF;
    let i2 = (i1 + 0x400) & 0xFFF;
    let i3 = (i2 + 0x400) & 0xFC0;
    for (k, idx) in [f, i1, i2, i3].into_iter().enumerate() {
        let v = (camera_roll as i32).wrapping_add(crate::fixed::div_pow2(t.sin_raw(idx), 6) as i16 as i32) & 0xFFF;
        g.items.token_spin[k] = v as i16;
    }
}

//! The race code's sound calls as events (docs/spec/audio.md, "SFX +
//! engine"). The game plays sound effects through two calls of its own
//! sound driver: FUN_8005EECC(id, pitch, vol_l, vol_r), a one-shot, and
//! FUN_8005F168(owner, id, pitch, vol, glide), a looping channel kept alive
//! by calling it every frame (engine, scrape, slide). Most sites go through
//! FUN_8007A620(id, pitch, vol, x, y, z, owner), which scales the volume by
//! the distance to the nearest human player's camera and then makes one of
//! those two calls. This module ports the argument logic of every call the
//! race frame makes (what is played, at what pitch and volume) and records
//! the resulting driver calls per frame in `RaceGlobals::sound`; the driver
//! itself (channels, libsnd voices, the SPU) is mr-audio's `driver`, run by
//! the front end. Integer-only and deterministic: the simulation never
//! reads back driver state (the game's code does not either).
//!
//! Sound ids are 1-based tone numbers of program 0 of SFX/SFX_6.VAB;
//! pitches are libsnd note * 128 + fine (FUN_8005EECC splits them).

use crate::globals::RaceGlobals;
use crate::kart::KartState;
use crate::tables::square_root0;
use crate::world::World;

/// One call into the game's sound driver.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundCall {
    /// FUN_8005EECC(id, pitch, vol_l, vol_r): a one-shot.
    Play { id: i16, pitch: i16, vol_l: i16, vol_r: i16 },
    /// FUN_8005F168(owner, id, pitch, vol, glide): start or refresh the
    /// looping channel `owner` (the game's keys: player index = engine,
    /// + 2 = scrape, + 4 = slide, + 6 = impact).
    Loop { owner: i16, id: i16, pitch: i16, vol: i16, glide: i16 },
}

/// The calls of one race frame, in call order, and the frame's length in
/// VBlanks (0x800B0AEC; the driver's VBlank work runs `dt` times after it).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SoundFrame {
    pub dt: i32,
    pub calls: Vec<SoundCall>,
}

/// Frames kept when nobody drains them (tests, analysis runs).
pub const MAX_FRAMES: usize = 256;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SoundState {
    /// Eye position (camera +0x28, 0x800C4F68 + 0x70 c) of each human
    /// player's camera as the frame's sound calls see it (the cameras move
    /// after the kart loop, so this is the previous frame's).
    pub listeners: [[i32; 3]; 2],
    /// Frames not yet drained by the front end (oldest first).
    pub frames: Vec<SoundFrame>,
    /// Kart +0x4F5 per slot: VBlanks until the character may speak again
    /// (FUN_8007A7F4). Kept here: nothing else reads it.
    pub voice_timer: [u8; 8],
    /// Phase 11: the race frame's requests to the game's CD-XA queue
    /// (0x800B0744..48, docs/spec/audio.md "The queue"), in call order, not
    /// yet drained by the front end (mr-bevy audio.rs `MusicDirector`).
    pub xa: Vec<XaReq>,
}

/// A request the race code makes of the CD-XA queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XaReq {
    /// FUN_80060580(character, n): the character's voice clip n (base
    /// 0x800AC230[character * 0x14] + n), then the course track again.
    Clip { character: u8, n: u8 },
    /// FUN_800606D0: the results taunt (VOICES/WIN.XA, channel
    /// 0x800B074C, cycling 0..6), then the course track again.
    Taunt,
}

impl SoundState {
    fn push(&mut self, c: SoundCall) {
        if self.frames.is_empty() {
            self.frames.push(SoundFrame { dt: 2, calls: Vec::new() });
        }
        self.frames.last_mut().unwrap().calls.push(c);
    }

    /// Take every recorded frame (the front end calls this after ticking).
    pub fn drain(&mut self) -> Vec<SoundFrame> {
        std::mem::take(&mut self.frames)
    }
}

/// Start of a race frame (session): a new call list, this frame's dt, the
/// listener (player 1's camera eye before the camera moves), and
/// FUN_800306D0's reset of every kart's character-voice request bits
/// (+0x4DD), which the kart code sets during the frame and FUN_8007A7F4
/// reads at its end.
pub fn begin_frame(g: &mut RaceGlobals, karts: &mut [KartState], dt: i32, eye: [i32; 3]) {
    let s = &mut g.sound;
    if s.frames.len() >= MAX_FRAMES {
        s.frames.remove(0);
    }
    s.frames.push(SoundFrame { dt, calls: Vec::new() });
    s.listeners[0] = eye;
    for k in karts.iter_mut() {
        k.event_bits = 0;
    }
}

/// FUN_80060580(character, n) from the race code: the character's voice
/// clip (the call itself returns at once in the attract demo, 0x800B0BD2;
/// the character's voice flag and the queue are the front end's).
pub fn voice_clip(g: &mut RaceGlobals, character: u8, n: u8) {
    if g.demo == 0 && g.sound.xa.len() < MAX_FRAMES {
        g.sound.xa.push(XaReq::Clip { character, n });
    }
}

/// FUN_8005EECC(id, pitch, vol_l, vol_r).
pub fn play(g: &mut RaceGlobals, id: i32, pitch: i32, vol_l: i32, vol_r: i32) {
    g.sound.push(SoundCall::Play { id: id as i16, pitch: pitch as i16, vol_l: vol_l as i16, vol_r: vol_r as i16 });
}

/// FUN_8007A620(id, pitch, vol, x, y, z, owner) [0x8007A620]: the squared
/// distance (in 1024-unit steps) to the nearest human camera within 256
/// steps sets the volume, `vol * (0x2000 - 32 * sqrt(d)) / 0x2000`; then a
/// one-shot (owner -1) or the channel `owner`.
pub fn at(g: &mut RaceGlobals, w: &World, id: i32, pitch: i32, vol: i32, pos: [i32; 3], owner: i32) {
    let mut best = i32::MAX;
    for c in 0..(g.humans.max(0) as usize).min(2) {
        let e = g.sound.listeners[c];
        let dx = pos[0].wrapping_sub(e[0]) >> 10;
        let dy = pos[1].wrapping_sub(e[1]) >> 10;
        let dz = pos[2].wrapping_sub(e[2]) >> 10;
        let d = dx.wrapping_mul(dx).wrapping_add(dy.wrapping_mul(dy)).wrapping_add(dz.wrapping_mul(dz));
        if d <= 0xFFFF && d < best {
            best = d;
        }
    }
    if best == i32::MAX {
        return;
    }
    let a = 0x2000 - square_root0(&w.tables, best) * 32;
    if a <= 0 {
        return;
    }
    let v = a.wrapping_mul(vol as i16 as i32);
    let v = (if v < 0 { v + 0x1FFF } else { v }) >> 13;
    if v <= 0 {
        return;
    }
    let (id, pitch) = (id as i16, pitch as i16);
    let owner = owner as i16;
    if owner == -1 {
        g.sound.push(SoundCall::Play { id, pitch, vol_l: v as i16, vol_r: v as i16 });
    } else {
        g.sound.push(SoundCall::Loop { owner, id, pitch, vol: v as i16, glide: 0 });
    }
}

fn pos(k: &KartState) -> [i32; 3] {
    [k.x, k.y, k.z]
}

/// Truncating halving as the compiler emits it (`(x + (x >>> 31)) >> 1`).
fn half(x: i32) -> i32 {
    (x + ((x as u32) >> 31) as i32) >> 1
}

/// 0x80064D40: engine sound of a human player's kart (called when the
/// player index < humans and mode != 0x10). The vehicle's engine class
/// (+0x173) picks the sample and base pitch; the pitch rises with the
/// throttle lurch |+0x480| * 8, +0x80 when revving in the air, the rev
/// timer +0x4DE (* 1024 / 60) and half a random byte; class 0x10 runs at
/// 10/16 and 0x40 at 1/2 of it, and everything halves on water. Then the
/// scrape loop (sound 0x1B, owner player + 2) while on the ground with an
/// audible engine (+0x171): volume from the ground speed +0x400.
pub fn engine(k: &KartState, g: &mut RaceGlobals, w: &World) {
    let (id, base) = match (k.vehicle_class as i32 - 1) as i8 {
        0x01 => (1, 0x1400),
        0x03 => (3, 0x1400),
        0x07 => (5, 0x1400),
        0x0F => (6, 0x1401),
        0x1F => (7, 0x1400),
        0x3F => (6, 0x13FF),
        _ => (2, 0x1400),
    };
    let mut p = base + (k.lurch as i32 * 8).abs();
    if k.ground_contact == 0 && k.throttle_hold != 0 {
        p += 0x80;
    }
    p += ((k.engine_rev as i8 as i32) << 10) / 60;
    let r = crate::update::rng_next(g, w) as i8 as i32;
    p += half(r);
    if base == 0x1401 {
        let v = p * 10;
        p = (if v < 0 { v + 15 } else { v }) >> 4;
    } else if base == 0x13FF {
        p = half(p);
    }
    if k.float_state != 0 {
        p = half(p);
    }
    at(g, w, id, p, 0x60, pos(k), k.player_index as i32);
    if k.ground_contact != 0 && k.engine_audible != 0 {
        let s = (k.speed_est >> 5).min(64);
        let pitch = if k.float_state != 0 { 4000 } else { 5500 };
        let v = s * 7;
        let v = (if v < 0 { v + 3 } else { v }) >> 2;
        at(g, w, 0x1B, pitch, v, pos(k), k.player_index as i32 + 2);
    }
}

/// 0x8006519C: the SELECT view toggle clicks (menu sound 0xF).
pub fn view_toggle(g: &mut RaceGlobals) {
    play(g, 0xF, 0x1800, 0x60, 0x60);
}

/// 0x80065DB8: a shoulder button pressed with nothing to use (the "not
/// ready" request 0x800B0EC4 set by R1 / R2 / an empty weapon slot):
/// sound 9 at the kart.
pub fn not_ready(k: &KartState, g: &mut RaceGlobals, w: &World) {
    if g.sound_event != 0 {
        let pitch = if k.float_state != 0 { 4000 } else { 0x157C };
        at(g, w, 9, pitch, 0x60, pos(k), -1);
    }
}

/// 0x800663B0: take-off of a jump (sound 0x21).
pub fn jump(k: &KartState, g: &mut RaceGlobals, w: &World) {
    let pitch = if k.float_state != 0 { 4000 } else { 0x157C };
    at(g, w, 0x21, pitch, 0x80, pos(k), -1);
}

/// 0x80066D2C: tyre skid when coasting off the throttle (the random draw
/// that decides it is the caller's; sound 8).
pub fn skid(k: &KartState, g: &mut RaceGlobals, w: &World) {
    let pitch = if k.float_state != 0 { 0xCC0 } else { 0x1100 };
    at(g, w, 8, pitch, 0x80, pos(k), -1);
}

/// 0x80067834: impact (+0x440 > 800) of a human player's kart: sound 0xD
/// on channel player + 6, pitch 5000 + 2 * a random byte.
pub fn impact(k: &KartState, g: &mut RaceGlobals, w: &World) {
    let b = crate::update::rng_next(g, w) as i32;
    at(g, w, 0xD, b * 2 + 5000, 0x60, pos(k), k.player_index as i32 + 6);
}

/// 0x80067D9C: sliding / drifting / reversing on the ground: a loop on
/// channel player + 4, sound 0x1C on surfaces 0, 6, 9, 0xC..0xE, else
/// 0x1A, volume min(ground speed >> 5, 64).
pub fn slide(k: &KartState, g: &mut RaceGlobals, w: &World) {
    let id = match k.surface {
        0 | 6 | 9 | 0xC | 0xD | 0xE => 0x1C,
        _ => 0x1A,
    };
    let v = (k.speed_est >> 5).min(0x40);
    at(g, w, id, 5000, v, pos(k), k.player_index as i32 + 4);
}

/// 0x8003EA54: wall hit (sound 0x1D, the rumble strength capped at 0x80).
pub fn wall_hit(k: &KartState, g: &mut RaceGlobals, w: &World, strength: i32) {
    at(g, w, 0x1D, 0x1600, strength.min(0x80), pos(k), -1);
}

/// 0x8003F338: on water: with the splash flag +0x4DB clear a random byte
/// < 0x10 plays 0x1F; with it set 0x20.
pub fn splash(k: &KartState, g: &mut RaceGlobals, w: &World) {
    let id = if k.splash == 0 {
        // 60 Hz mode: tried once per pair.
        if g.hz60.beat() && crate::update::rng_next(g, w) < 0x10 {
            0x1F
        } else {
            return;
        }
    } else {
        0x20
    };
    at(g, w, id, 0x1600, 0x60, pos(k), -1);
}

/// 0x8003F8F8: landing (sound 10, and 0x12 on surface 0xD), volume from
/// the vertical speed (the rumble strength capped at 0x80).
pub fn landing(k: &KartState, g: &mut RaceGlobals, w: &World, strength: i32) {
    let v = strength.min(0x80);
    at(g, w, 10, 0x1600, v, pos(k), -1);
    if k.surface == 0xD {
        at(g, w, 0x12, 0x1600, v, pos(k), -1);
    }
}

fn mid(a: &KartState, b: &KartState) -> [i32; 3] {
    [a.x.wrapping_add(b.x) / 2, a.y.wrapping_add(b.y) / 2, a.z.wrapping_add(b.z) / 2]
}

/// 0x800403A0: one kart lands on another (sound 0x21 between them).
pub fn kart_land(a: &KartState, b: &KartState, g: &mut RaceGlobals, w: &World) {
    at(g, w, 0x21, 0x1600, 0x60, mid(a, b), -1);
}

/// 0x80040A8C: karts bump side by side (sound 0xC, the rumble strength
/// capped at 0x80).
pub fn kart_bump(a: &KartState, b: &KartState, g: &mut RaceGlobals, w: &World, strength: i32) {
    at(g, w, 0xC, 0x1600, strength.min(0x80), mid(a, b), -1);
}

/// 0x8005C3E0 (FUN_8005C2EC, before the countdown steps by dt): the
/// countdown beeps (sound 9) when the countdown, below 120 VBlanks, crosses
/// a multiple of 30, and the GO sound (0x23) when it runs out; in race-like
/// modes (1, 2, 3, 7, 0x14) outside the attract demo / mode-10 state.
pub fn countdown(g: &mut RaceGlobals) {
    let on = if g.game_state == 0x1E || g.game_state == 10 {
        false
    } else {
        matches!(g.mode, 1 | 2 | 3 | 7 | 0x14)
    };
    let n = g.countdown - g.dt;
    if g.demo == 0 && on && n < 0x78 && n / 0x1E < (g.countdown as i16 as i32) / 0x1E {
        play(g, 9, 0x1194, 0x80, 0x80);
    }
    if n < 1 && on {
        play(g, 0x23, 0x157C, 0x80, 0x80);
    }
}

/// 0x80082DE8: a lap completed (sound 0x22 at the kart).
pub fn lap(k: &KartState, g: &mut RaceGlobals, w: &World) {
    at(g, w, 0x22, 0x1500, 0xA0, pos(k), -1);
}

/// Character voice table [exe 0x800AC232 + 0x14 * character]: six sound
/// ids (signed bytes, -1 = none) for the request bits 0..5 of +0x4DD.
pub const VOICE_TABLE: u32 = 0x800A_C232;

/// FUN_8007A7F4(karts) [race frame, after the item code]: per kart, the
/// lowest request bit of +0x4DD with a voice in the character's row plays
/// it (pitch 0x1800, 0x1500 for character 0x11; volume 0xA0 at the kart)
/// once the kart's voice timer is 0, which then holds it for 60 VBlanks; a
/// bit without a voice passes the request to the next bit.
pub fn character_voices(karts: &mut [KartState], g: &mut RaceGlobals, w: &World) {
    for (s, k) in karts.iter_mut().enumerate().take(8) {
        let t = &mut g.sound.voice_timer[s];
        if *t != 0 {
            let n = *t as i32 - g.dt;
            *t = if (n as i8) < 0 { 0 } else { n as u8 };
        }
        for i in 0..6u32 {
            if (k.event_bits as i8 as i32 >> i) & 1 == 0 {
                continue;
            }
            let ch = k.character as u32;
            let id = w.exe.u8(VOICE_TABLE + i + ch * 0x14) as i8 as i32;
            if id != -1 {
                if g.sound.voice_timer[s] == 0 {
                    let pitch = if ch == 0x11 { 0x1500 } else { 0x1800 };
                    at(g, w, id, pitch, 0xA0, pos(k), -1);
                    g.sound.voice_timer[s] = 60;
                }
                break;
            }
            if i < 5 {
                k.event_bits |= (1 << (i + 1)) as u8;
            }
        }
    }
}

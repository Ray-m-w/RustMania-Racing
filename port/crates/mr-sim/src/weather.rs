//! Falling weather (Session 6, track P): FUN_8008FEA8 (course load) and
//! FUN_8008650C (every race frame, after the camera and the splashes).
//!
//! On when the course row's flags (0x800ACB3A + 12 * course) have 0x20
//! (snow: LONDON, GRAVE, GLACIER) or 0x40 (rain: BOG); 0x800B0ACA. Sixteen
//! drops at 0x800D75D0 (0x2C bytes each). Each frame the next slot
//! (0x800B0ECC) if free is placed at random around the human camera of
//! that slot's parity (two rand(): x, z = camera eye - 0x17FFD + rand * 6 +
//! forward * 1000 / 32), on the highest surface below the course top
//! (FUN_8003DDB0 at y = TER min y << 5); if a surface is found the drop
//! takes a life of 0x40 + (signed byte) / 4 VBlanks (one random byte) and a fall
//! speed (rand() / 16 + 0x800 rain, rand() / 64 + 0x400 snow) and starts
//! that far above it; the slot index moves on. Every drop falls speed * dt
//! a frame; at the end of its life a rain drop splashes (a particle of kind
//! 0x10, or on water a splash ring FUN_800330D4 and sound 0x1F); snow
//! settles on the ground. The random draws are the simulation's concern
//! (they are the AI's streams too); the drops themselves are drawn by the
//! renderer.

use crate::globals::RaceGlobals;
use crate::world::World;

pub const DROPS: usize = 16;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Drop {
    /// +0x00 / +0x04 / +0x08: position (world units); y falls.
    pub pos: [i32; 3],
    /// +0x0C: the surface it falls to.
    pub ground: i32,
    /// +0x10: fall per VBlank.
    pub speed: i32,
    /// +0x14: VBlanks left (-1 = free), +0x16: life.
    pub timer: i16,
    pub life: i16,
    /// +0x1A / +0x1B: sprite width / height (0x60; a settled snow flake
    /// 0x80 x 0x40 below cup 7).
    pub size: [u8; 2],
    /// +0x1D: the surface is the water (0x800B0C00 == 0x800B0A54).
    pub on_water: u8,
    /// +0x23..+0x25: colour now (faded), +0x26..+0x28: full colour.
    pub rgb: [u8; 3],
    pub base: [u8; 3],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Weather {
    /// Course flag 0x20 (snow) rather than 0x40 (rain).
    pub snow: bool,
    pub drops: [Drop; DROPS],
    /// 0x800B0ECC: the next slot to fill.
    pub next: u8,
}

impl Weather {
    /// FUN_8008FEA8: None when the course has no weather.
    pub fn load(w: &World) -> Option<Weather> {
        let f = w.info.flags;
        let snow = f & 0x20 != 0;
        if !snow && f & 0x40 == 0 {
            return None;
        }
        let d = Drop { timer: -1, ..Default::default() };
        Some(Weather { snow, drops: [d; DROPS], next: 0 })
    }
}

/// The camera fields FUN_8008650C reads (0x800C4F40 + 0x70 * i): eye x / z
/// (+0x28 / +0x30) and forward x / z (+0x20 / +0x24).
#[derive(Clone, Copy, Debug, Default)]
pub struct WeatherCamera {
    pub eye: [i32; 3],
    pub forward: [i16; 3],
}

/// FUN_8008650C.
pub fn frame(g: &mut RaceGlobals, w: &World, cams: &[WeatherCamera], rand: &mut dyn FnMut() -> i32) {
    let Some(mut wt) = g.objects.weather.take() else { return };
    let dt = g.dt;
    let slot = wt.next as usize;
    if wt.drops[slot].timer == -1 {
        let humans = (g.humans as i32).max(1);
        let c = cams.get(((slot as i32 & 1) % humans) as usize).copied().unwrap_or_default();
        let x = c.eye[0] - 0x17FFD + rand().wrapping_mul(6) + ((c.forward[0] as i32 * 1000) >> 5);
        let z = c.eye[2] - 0x17FFD + rand().wrapping_mul(6) + ((c.forward[2] as i32 * 1000) >> 5);
        let q = w.ter.ground(x, w.ter.min[1] << 5, z);
        if q.found {
            let d = &mut wt.drops[slot];
            d.on_water = (q.height == q.water_y) as u8;
            d.pos[0] = x;
            d.pos[2] = z;
            d.ground = q.height;
            // A signed byte (lb; 0x8008667C): 0x30..0x5F VBlanks.
            let b = crate::update::rng_next(g, w) as i8 as i32;
            let life = crate::fixed::div_pow2(b, 2) as i16 + 0x40;
            d.life = life;
            d.timer = life;
            d.size = [0x60, 0x60];
            if !wt.snow {
                let r = rand();
                d.speed = (r >> 4) + 0x800;
                d.base = [0, 0xFF, 0xFF];
                d.pos[1] = d.ground - d.speed * life as i32;
            } else {
                let r = rand();
                d.speed = (r >> 6) + 0x400;
                d.base = [0x80, 0x80, 0x80];
                d.pos[1] = d.ground - crate::fixed::div_pow2(d.speed * life as i32 * 3, 2);
            }
            wt.next = (wt.next + 1) % DROPS as u8;
        }
    }
    for d in wt.drops.iter_mut() {
        if d.timer == -1 {
            continue;
        }
        let t = d.timer as i32 - dt;
        d.timer = t as i16;
        if (t << 16) < 0 {
            d.timer = -1;
            if !wt.snow {
                let pos = [d.pos[0], d.ground, d.pos[2]];
                if d.on_water == 0 {
                    crate::items::particle(g, pos, [0, 0, 0], 0, 0, 0, 0x10, [0, 0x80, 0x80], 0x60, 0x60, 0x10);
                } else {
                    if let Some(fx) = g.fx.as_mut() {
                        fx.flats.splash(pos, 0xC0);
                    }
                    crate::sound::at(g, w, 0x1F, 0x157C, 0x40, pos, -1);
                }
            }
            continue;
        }
        d.pos[1] = d.pos[1].wrapping_add(d.speed.wrapping_mul(dt));
        if wt.snow && d.ground < d.pos[1] {
            d.pos[1] = d.ground;
            if g.cup < 7 {
                d.size = [0x80, 0x40];
            }
        }
        let q = d.life as i32 / 4;
        let tm = d.timer as i32;
        if !wt.snow || q <= tm {
            if tm < d.life as i32 - q {
                d.rgb = d.base;
            } else {
                let k = s16(d.life as i32 - (tm + 1));
                d.rgb = d.base.map(|c| (c as i32 * k / q) as u8);
            }
        } else {
            d.rgb = d.base.map(|c| (c as i32 * tm / q) as u8);
        }
    }
    g.objects.weather = Some(wt);
}

fn s16(x: i32) -> i32 {
    x as i16 as i32
}

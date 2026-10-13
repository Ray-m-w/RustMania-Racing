//! Animated textures (RAW type 0x22 strips): the game copies one frame of
//! each strip into VRAM every race frame, FUN_8007A9DC -> FUN_800311AC(slot,
//! 0, frame) [CODE 8007a9dc, 800311ac]; docs/spec/effects.md "Animated
//! textures". On DOCK: the water (slots 0xA, 0xC), the turbo-meter and
//! weapon-slot stars (0x19, 0x27; texture slot 0x15), and 0x1E.
//!
//! - Frame per slot from the frame counter DAT_800B0D90 (course 3):
//!   0xA: (f & 0x3F) >> 2; 0xB: (f % 0x36) / 2; 0xC: (f & 0x3F) >> 3
//!   (>> 4 on other courses); 0xD: (f & 0x3F) >> 2; 0x19, 0x27:
//!   (f & 0x1F) >> 1. Slot 0x1E (FUN_8005C2EC): 7 - ((c & 0xF) >> 1) of
//!   the counter DAT_800B07D4 (+dt every frame; its phase is approximated
//!   by the frame counter, render ledger).
//! - Destination: the sprite definition *0x800D1B40[slot] (exe table
//!   0x800ACF40): its texture slot's page + frame 0's (u, v); 4-bit strips
//!   (DAT_800BBB20) are w/4 halfwords wide, 8-bit (DAT_800CB680) w/2.

use bevy::prelude::*;
use mr_assets::raw::{self, AnimStrip};

use crate::vramtex::VramTextures;

struct Strip {
    slot: u32,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    frames: Vec<Vec<u16>>,
    shown: Option<usize>,
}

#[derive(Resource, Default)]
struct AnimTex {
    strips: Vec<Strip>,
}

pub struct AnimTexPlugin;

impl Plugin for AnimTexPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AnimTex>().add_systems(Startup, setup.after(crate::scene::setup)).add_systems(Update, step);
    }
}

fn exe_u32(exe: &[u8], a: u32) -> u32 {
    let o = (a - 0x8001_0000) as usize + 0x800;
    u32::from_le_bytes([exe[o], exe[o + 1], exe[o + 2], exe[o + 3]])
}

fn setup(mut at: ResMut<AnimTex>) {
    let mut disc = mr_disc::Disc::open_default().expect("open the disc");
    let exe = disc.read("SLUS_012.37").expect("read the executable");
    let path = raw::course_raw_path(&exe, crate::scene::DOCK).expect("course");
    let file = disc.read(&path).expect("read RAW");
    let env = raw::LoadEnv::from_exe(&exe, crate::scene::DOCK).expect("env");
    let dec = raw::decode(&file, &env).expect("decode RAW");
    for a in &dec.anims {
        if let Some(s) = place(&exe, &dec.slots, a) {
            at.strips.push(s);
        }
    }
}

/// Where FUN_800311AC puts frames of `a` (4-bit strips; the RAW decoder
/// splits frames as 4-bit, which every DOCK strip is).
fn place(exe: &[u8], slots: &[Option<raw::TexSlot>; 32], a: &AnimStrip) -> Option<Strip> {
    let def = exe_u32(exe, 0x800A_CF40 + 4 * a.slot);
    if def == 0 {
        return None;
    }
    let o = (def - 0x8001_0000) as usize + 0x800;
    let tex_slot = u16::from_le_bytes([exe[o], exe[o + 1]]) as usize;
    let (u, v) = (exe[o + 8] as usize, exe[o + 9] as usize);
    let ts = slots.get(tex_slot)?.as_ref()?;
    let tp = ts.tpage?;
    let (px, py, _) = mr_assets::vram::tpage(tp);
    Some(Strip { slot: a.slot, x: px + (u >> 2), y: py + v, w: a.w as usize / 4, h: a.h as usize, frames: a.frames.clone(), shown: None })
}

/// The frame FUN_8007A9DC (or FUN_8005C2EC for 0x1E) shows at frame count `f` on DOCK.
pub fn frame_for(slot: u32, f: i32) -> Option<usize> {
    let r = match slot {
        0xA | 0xD => (f & 0x3F) >> 2,
        0xB => (f % 0x36) / 2,
        0xC => (f & 0x3F) >> 3,
        0x19 | 0x27 => (f & 0x1F) >> 1,
        0x1E => 7 - ((f & 0xF) >> 1),
        _ => return None,
    };
    Some(r as usize)
}

fn step(p: Res<crate::Player>, mut at: ResMut<AnimTex>, tex: Option<ResMut<VramTextures>>, mut images: ResMut<Assets<Image>>) {
    let Some(mut tex) = tex else { return };
    let f = match &p.driver {
        crate::Driver::Sim(run) => run.session.globals.frame,
        // replays: the frame counter advances 2 per tick
        _ => (p.tick * 2) as i32,
    };
    for s in at.strips.iter_mut() {
        let Some(k) = frame_for(s.slot, f) else { continue };
        let k = k.min(s.frames.len().saturating_sub(1));
        if s.shown == Some(k) || s.frames.is_empty() {
            continue;
        }
        let data = &s.frames[k];
        if data.len() < s.w * s.h {
            continue;
        }
        tex.write_rect(&mut images, s.x, s.y, s.w, s.h, data);
        s.shown = Some(k);
    }
}

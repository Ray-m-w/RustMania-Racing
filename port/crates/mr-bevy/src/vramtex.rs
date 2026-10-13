//! Textures from the decoded VRAM: one 256x256 RGBA image per (texpage, CLUT)
//! pair a mesh uses, sampled exactly like the PS1 GPU (4/8/15-bit texels via
//! the CLUT, 0x0000 transparent). Nearest filtering keeps the original look.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use mr_assets::vram::{self, Vram};

#[derive(Resource, Default)]
pub struct VramTextures {
    pub vram: Vram,
    cache: HashMap<(u16, u16), Handle<Image>>,
}

impl VramTextures {
    pub fn new(vram: Vram) -> Self {
        VramTextures { vram, cache: HashMap::new() }
    }

    /// RGBA image of the 256x256 texel space addressed by `tpage`/`clut`.
    pub fn get(&mut self, images: &mut Assets<Image>, tpage: u16, clut: u16) -> Handle<Image> {
        if let Some(h) = self.cache.get(&(tpage, clut)) {
            return h.clone();
        }
        let (px, py, depth) = vram::tpage(tpage);
        let (cx, cy) = vram::clut(clut);
        let mut data = Vec::with_capacity(256 * 256 * 4);
        for v in 0..256u32 {
            for u in 0..256u32 {
                let t = self.vram.texel(depth, px, py, cx, cy, u as u8, v as u8);
                data.extend_from_slice(&vram::rgba(t));
            }
        }
        // Kept in the main world too: animated textures (animtex.rs) rewrite
        // texels in place.
        let mut img = Image::new(
            Extent3d { width: 256, height: 256, depth_or_array_layers: 1 },
            TextureDimension::D2,
            data,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
        );
        img.sampler = ImageSampler::nearest();
        let h = images.add(img);
        self.cache.insert((tpage, clut), h.clone());
        h
    }

    /// Write a w*h block of VRAM halfwords at (x, y) (LoadImage) and redraw
    /// the texels of every cached page image that samples it.
    pub fn write_rect(&mut self, images: &mut Assets<Image>, x: usize, y: usize, w: usize, h: usize, data: &[u16]) {
        self.vram.load_image(x, y, w, h, data);
        for (&(tp, cl), handle) in self.cache.iter() {
            let (px, py, depth) = vram::tpage(tp);
            let per = match depth {
                vram::Depth::B4 => 4,
                vram::Depth::B8 => 2,
                vram::Depth::B15 => 1,
            };
            let pw = 256 / per;
            if x + w <= px || x >= px + pw || y + h <= py || y >= py + 256 {
                continue;
            }
            let (cx, cy) = vram::clut(cl);
            let u0 = (x.max(px) - px) * per;
            let u1 = ((x + w).min(px + pw) - px) * per;
            let v0 = y.max(py) - py;
            let v1 = (y + h).min(py + 256) - py;
            let Some(mut img) = images.get_mut(handle) else { continue };
            let Some(d) = img.data.as_mut() else { continue };
            for v in v0..v1 {
                for u in u0..u1 {
                    let t = self.vram.texel(depth, px, py, cx, cy, u as u8, v as u8);
                    let o = (v * 256 + u) * 4;
                    d[o..o + 4].copy_from_slice(&vram::rgba(t));
                }
            }
        }
    }
}

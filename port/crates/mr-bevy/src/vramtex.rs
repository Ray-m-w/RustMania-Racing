//! Textures from the decoded VRAM: one 256x256 RGBA image per (texpage, CLUT)
//! pair a mesh uses, decoded exactly like the PS1 GPU (4/8/15-bit texels via
//! the CLUT, 0x0000 transparent).
//!
//! Two looks (settings `texture_filter`):
//! - `nearest`: point sampling, one level: the original look.
//! - `smooth` (default): bilinear + trilinear mipmaps + anisotropic filtering.
//!   To keep filtering from showing what the PS1 never samples:
//!   * transparent texels (colour 0) get the colour of their nearest opaque
//!     neighbours (alpha stays 0), so bilinear and mip averaging at cut-out
//!     edges blend toward the edge colour instead of black (no dark fringe);
//!   * mip levels average colour weighted by alpha (no dark bleed either) and
//!     stop at 16x16 texels per mip texel (`SMOOTH_MIPS` = 5 levels): pages
//!     are laid out in 64x64 tiles (4-bit pages: one CLUT per tile, DAT.md),
//!     and a 2x2 box chain aligned to the page never mixes texels of two
//!     tiles at these levels; only bilinear taps at a tile's border row can
//!     reach the neighbour, as at level 0.
//!   The meshes map vertex UVs to texel centres (psxmesh), so bilinear taps
//!   inside a polygon stay within its own texel rectangle.
//! The HUD keeps point sampling in both modes (`get_nearest`): its sprites are
//! pixel art drawn at a fixed scale.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use mr_assets::vram::{self, Vram};

use crate::settings::TexFilter;

/// Mip levels of a smoothed page (256 -> 16).
pub const SMOOTH_MIPS: u32 = 5;
/// Dilation passes that push opaque colours into transparent texels.
const BLEED_PASSES: usize = 6;

#[derive(Resource, Default)]
pub struct VramTextures {
    pub vram: Vram,
    /// (tpage, clut, smooth) -> image
    cache: HashMap<(u16, u16, bool), Handle<Image>>,
    /// Level 0 of each smoothed page as last built (rebuilds only on change).
    smooth_base: HashMap<(u16, u16), Vec<u8>>,
    pub filter: Option<TexFilter>,
}

impl VramTextures {
    pub fn new(vram: Vram) -> Self {
        VramTextures { vram, cache: HashMap::new(), smooth_base: HashMap::new(), filter: None }
    }

    fn smooth(&self) -> bool {
        self.filter.unwrap_or_else(|| crate::settings::Settings::global().texture_filter()) == TexFilter::Smooth
    }

    /// The page image for 3D geometry, filtered as the settings say.
    pub fn get(&mut self, images: &mut Assets<Image>, tpage: u16, clut: u16) -> Handle<Image> {
        let s = self.smooth();
        self.get_with(images, tpage, clut, s)
    }

    /// The page image with point sampling (HUD, or anything pixel-exact).
    pub fn get_nearest(&mut self, images: &mut Assets<Image>, tpage: u16, clut: u16) -> Handle<Image> {
        self.get_with(images, tpage, clut, false)
    }

    fn get_with(&mut self, images: &mut Assets<Image>, tpage: u16, clut: u16, smooth: bool) -> Handle<Image> {
        if let Some(h) = self.cache.get(&(tpage, clut, smooth)) {
            return h.clone();
        }
        let base = self.base_level(tpage, clut);
        let (data, levels) = if smooth {
            let chain = smooth_chain(&base);
            self.smooth_base.insert((tpage, clut), base);
            (chain, SMOOTH_MIPS)
        } else {
            (base, 1)
        };
        // Kept in the main world too: animated textures (animtex.rs) rewrite
        // texels in place.
        let mut img = Image::new(
            Extent3d { width: 256, height: 256, depth_or_array_layers: 1 },
            TextureDimension::D2,
            data,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
        );
        img.texture_descriptor.mip_level_count = levels;
        img.sampler = if smooth {
            ImageSampler::Descriptor(ImageSamplerDescriptor {
                address_mode_u: ImageAddressMode::ClampToEdge,
                address_mode_v: ImageAddressMode::ClampToEdge,
                mag_filter: ImageFilterMode::Linear,
                min_filter: ImageFilterMode::Linear,
                mipmap_filter: ImageFilterMode::Linear,
                anisotropy_clamp: 16,
                ..default()
            })
        } else {
            ImageSampler::nearest()
        };
        let h = images.add(img);
        self.cache.insert((tpage, clut, smooth), h.clone());
        h
    }

    /// Level 0 of a page: 256x256 RGBA, exactly the GPU's texels.
    fn base_level(&self, tpage: u16, clut: u16) -> Vec<u8> {
        let (px, py, depth) = vram::tpage(tpage);
        let (cx, cy) = vram::clut(clut);
        let mut data = Vec::with_capacity(256 * 256 * 4);
        for v in 0..256u32 {
            for u in 0..256u32 {
                let t = self.vram.texel(depth, px, py, cx, cy, u as u8, v as u8);
                data.extend_from_slice(&vram::rgba(t));
            }
        }
        data
    }

    /// Write a w*h block of VRAM halfwords at (x, y) (LoadImage) and redraw
    /// the texels of every cached page image that samples it (smoothed pages
    /// are rebuilt whole: bleed and mips depend on the neighbourhood).
    pub fn write_rect(&mut self, images: &mut Assets<Image>, x: usize, y: usize, w: usize, h: usize, data: &[u16]) {
        self.vram.load_image(x, y, w, h, data);
        let keys: Vec<_> = self.cache.keys().copied().collect();
        for (tp, cl, smooth) in keys {
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
            let handle = self.cache[&(tp, cl, smooth)].clone();
            if smooth {
                let base = self.base_level(tp, cl);
                if self.smooth_base.get(&(tp, cl)) == Some(&base) {
                    continue;
                }
                let chain = smooth_chain(&base);
                self.smooth_base.insert((tp, cl), base);
                if let Some(mut img) = images.get_mut(&handle) {
                    img.data = Some(chain);
                }
                continue;
            }
            let (cx, cy) = vram::clut(cl);
            let u0 = (x.max(px) - px) * per;
            let u1 = ((x + w).min(px + pw) - px) * per;
            let v0 = y.max(py) - py;
            let v1 = (y + h).min(py + 256) - py;
            let Some(mut img) = images.get_mut(&handle) else { continue };
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

/// Fill the colour of transparent texels from opaque neighbours (alpha kept 0).
pub fn bleed(data: &mut [u8], w: usize, h: usize) {
    let mut set: Vec<bool> = data.chunks_exact(4).map(|p| p[3] != 0).collect();
    for _ in 0..BLEED_PASSES {
        let mut next = set.clone();
        let mut any = false;
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                if set[i] {
                    continue;
                }
                let mut acc = [0u32; 3];
                let mut n = 0;
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        let (xx, yy) = (x as i32 + dx, y as i32 + dy);
                        if xx < 0 || yy < 0 || xx >= w as i32 || yy >= h as i32 {
                            continue;
                        }
                        let j = yy as usize * w + xx as usize;
                        if set[j] {
                            for c in 0..3 {
                                acc[c] += data[j * 4 + c] as u32;
                            }
                            n += 1;
                        }
                    }
                }
                if n > 0 {
                    for c in 0..3 {
                        data[i * 4 + c] = (acc[c] / n) as u8;
                    }
                    next[i] = true;
                    any = true;
                }
            }
        }
        set = next;
        if !any {
            break;
        }
    }
}

fn srgb_to_lin(c: u8) -> f32 {
    static LUT: std::sync::OnceLock<[f32; 256]> = std::sync::OnceLock::new();
    LUT.get_or_init(|| {
        std::array::from_fn(|i| {
            let c = i as f32 / 255.0;
            if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        })
    })[c as usize]
}

fn lin_to_srgb(c: f32) -> u8 {
    static LUT: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    let lut = LUT.get_or_init(|| {
        (0..=4096)
            .map(|i| {
                let c = i as f32 / 4096.0;
                let s = if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
                (s * 255.0 + 0.5) as u8
            })
            .collect()
    });
    lut[(c.clamp(0.0, 1.0) * 4096.0 + 0.5) as usize]
}

/// Level 0 (bled) followed by `SMOOTH_MIPS - 1` box-filtered levels, colours
/// averaged in linear light weighted by alpha (transparent texels only count
/// when the whole 2x2 block is transparent: then their bled colours).
pub fn smooth_chain(base: &[u8]) -> Vec<u8> {
    let mut lvl = base.to_vec();
    bleed(&mut lvl, 256, 256);
    let mut out = lvl.clone();
    let mut size = 256usize;
    for _ in 1..SMOOTH_MIPS {
        let n = size / 2;
        let mut next = vec![0u8; n * n * 4];
        for y in 0..n {
            for x in 0..n {
                let mut acc = [0f32; 3];
                let mut wsum = 0f32;
                let mut a = 0u32;
                let mut plain = [0f32; 3];
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let o = ((2 * y + dy) * size + 2 * x + dx) * 4;
                    let wgt = lvl[o + 3] as f32 / 255.0;
                    for c in 0..3 {
                        let l = srgb_to_lin(lvl[o + c]);
                        acc[c] += l * wgt;
                        plain[c] += l * 0.25;
                    }
                    wsum += wgt;
                    a += lvl[o + 3] as u32;
                }
                let o = (y * n + x) * 4;
                for c in 0..3 {
                    next[o + c] = lin_to_srgb(if wsum > 0.0 { acc[c] / wsum } else { plain[c] });
                }
                next[o + 3] = ((a + 2) / 4) as u8;
            }
        }
        out.extend_from_slice(&next);
        lvl = next;
        size = n;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bleed_fills_transparent_with_neighbour_colour() {
        // 4x1: red opaque, then three transparent texels
        let mut d = vec![200, 10, 10, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        bleed(&mut d, 4, 1);
        assert_eq!(&d[4..8], &[200, 10, 10, 0]);
        assert_eq!(&d[12..16], &[200, 10, 10, 0]);
    }

    #[test]
    fn mips_do_not_darken_cutout_edges() {
        let mut base = vec![0u8; 256 * 256 * 4];
        // one opaque white texel in each 2x2 block of the first tile
        for y in (0..64).step_by(2) {
            for x in (0..64).step_by(2) {
                let o = (y * 256 + x) * 4;
                base[o..o + 4].copy_from_slice(&[255, 255, 255, 255]);
            }
        }
        let chain = smooth_chain(&base);
        let l1 = &chain[256 * 256 * 4..];
        assert_eq!(&l1[0..4], &[255, 255, 255, 64]);
        assert_eq!(chain.len(), (0..SMOOTH_MIPS).map(|k| (256 >> k) * (256 >> k) * 4).sum::<usize>());
    }
}

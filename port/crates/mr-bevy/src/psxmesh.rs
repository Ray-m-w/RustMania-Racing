//! PS1 polygons -> Bevy meshes, keeping the GPU's look:
//! - one mesh per (texpage, CLUT) pair, textured from the decoded VRAM through
//!   `VramTextures` (nearest sampling, texel 0x0000 transparent -> alpha mask);
//! - no lighting: colours are the game's per-vertex RGB (unlit material);
//! - texture modulation as the GPU does it: texel * rgb / 128, so 128 is
//!   neutral and up to 2x brightens. Modulation happens in the game's (gamma)
//!   space; the vertex colour is converted to linear so Bevy's linear-space
//!   multiply gives the same result;
//! - untextured polygons are plain Gouraud colour (rgb / 255).

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use crate::vramtex::VramTextures;

#[derive(Clone, Debug)]
pub struct PsxPoly {
    /// Game world (or model) coordinates, x right, y down, z forward.
    pub pos: Vec<[i32; 3]>,
    pub uv: Vec<[u8; 2]>,
    pub rgb: Vec<[u8; 3]>,
    /// Some((tpage, clut)) when textured.
    pub tex: Option<(u16, u16)>,
    pub semi: bool,
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

/// Build meshes + materials for a set of polygons. `scale` maps game units to
/// Bevy units through `crate::to_bevy`-style axis flips.
/// Materials shared between meshes, keyed by (texture, semi-transparent).
/// `depth_bias` is applied to every material this cache creates (used to put
/// the game's "ground" draw list behind coplanar detail, as its ordering
/// table does: the PS1 has no depth buffer).
#[derive(Default)]
pub struct MatCache(pub HashMap<(Option<(u16, u16)>, bool), Handle<StandardMaterial>>, pub f32);

pub fn build(
    polys: &[PsxPoly],
    textures: &mut VramTextures,
    images: &mut Assets<Image>,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<StandardMaterial>,
    to_bevy: impl Fn([i32; 3]) -> Vec3,
) -> Vec<(Handle<Mesh>, Handle<StandardMaterial>)> {
    build_cached(polys, textures, images, meshes, mats, &mut MatCache::default(), to_bevy)
}

pub fn build_cached(
    polys: &[PsxPoly],
    textures: &mut VramTextures,
    images: &mut Assets<Image>,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<StandardMaterial>,
    cache: &mut MatCache,
    to_bevy: impl Fn([i32; 3]) -> Vec3,
) -> Vec<(Handle<Mesh>, Handle<StandardMaterial>)> {
    let mut groups: HashMap<(Option<(u16, u16)>, bool), Vec<&PsxPoly>> = HashMap::new();
    for p in polys {
        groups.entry((p.tex, p.semi)).or_default().push(p);
    }
    let mut out = Vec::new();
    let mut keys: Vec<_> = groups.keys().cloned().collect();
    keys.sort();
    for key in keys {
        let list = &groups[&key];
        let (mut pos, mut uv, mut col) = (Vec::new(), Vec::new(), Vec::new());
        let mut idx = Vec::new();
        for p in list {
            let base = pos.len() as u32;
            for i in 0..p.pos.len() {
                pos.push(to_bevy(p.pos[i]).to_array());
                let t = p.uv.get(i).copied().unwrap_or([0, 0]);
                // texel centres
                uv.push([(t[0] as f32 + 0.5) / 256.0, (t[1] as f32 + 0.5) / 256.0]);
                let c = p.rgb.get(i).copied().unwrap_or([128, 128, 128]);
                let f = |x: u8| {
                    if key.0.is_some() {
                        // gamma-space factor x/128 expressed as a linear multiplier
                        let g = x as f32 / 128.0;
                        srgb_to_linear((g * 0.5).min(1.0)) / srgb_to_linear(0.5)
                    } else {
                        srgb_to_linear(x as f32 / 255.0)
                    }
                };
                col.push([f(c[0]), f(c[1]), f(c[2]), 1.0]);
            }
            // PS1 quads are drawn as (0,1,2) + (1,2,3); triangles as (0,1,2).
            idx.extend_from_slice(&[base, base + 1, base + 2]);
            if p.pos.len() == 4 {
                idx.extend_from_slice(&[base + 1, base + 3, base + 2]);
            }
        }
        let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
            .with_inserted_indices(Indices::U32(idx));
        let mat = cache.0.entry(key).or_insert_with(|| {
            let mut m = StandardMaterial {
                base_color: Color::WHITE,
                unlit: true,
                cull_mode: None,
                double_sided: true,
                depth_bias: cache.1,
                ..default()
            };
            if let Some((tp, cl)) = key.0 {
                m.base_color_texture = Some(textures.get(images, tp, cl));
                m.alpha_mode = if key.1 { AlphaMode::Blend } else { AlphaMode::Mask(0.5) };
                if key.1 {
                    m.base_color = Color::srgba(1.0, 1.0, 1.0, 0.5);
                }
            } else if key.1 {
                m.alpha_mode = AlphaMode::Blend;
                m.base_color = Color::srgba(1.0, 1.0, 1.0, 0.5);
            }
            mats.add(m)
        });
        out.push((meshes.add(mesh), mat.clone()));
    }
    out
}

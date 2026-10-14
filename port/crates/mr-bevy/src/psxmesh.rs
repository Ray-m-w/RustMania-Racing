//! PS1 polygons -> Bevy meshes, keeping the GPU's look:
//! - one mesh per (texpage, CLUT) pair, textured from the decoded VRAM through
//!   `VramTextures` (nearest or smoothed per settings, texel 0x0000
//!   transparent -> alpha mask / alpha to coverage: `cutout_mode`);
//! - no lighting: colours are the game's per-vertex RGB (unlit material);
//! - texture modulation as the GPU does it: texel * rgb / 128, so 128 is
//!   neutral and up to 2x brightens. Modulation happens in the game's (gamma)
//!   space; the vertex colour is converted to linear so Bevy's linear-space
//!   multiply gives the same result;
//! - untextured polygons are plain Gouraud colour (rgb / 255);
//! - semi-transparent polygons are not meshes: they become `FxSemi`
//!   primitives (`semi_prims`) that the PS1 effects pass (psxfx.rs) draws
//!   with the GPU's ABR blending in the game's colour space, depth-tested
//!   against the scene, in the ordering table's order.

use std::collections::HashMap;
use std::sync::Arc;

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
    /// Blend mode when `semi` (0 B/2+F/2, 1 B+F, 2 B-F, 3 B+F/4).
    pub abr: u8,
    /// The game culls this polygon's back face (NCLIP; front = counter-
    /// clockwise seen from a y-up view, GPU vertex order 0, 1, 2). Applied to
    /// the semi-transparent ones (an additive back face would add twice);
    /// opaque meshes stay two-sided.
    pub cull: bool,
}

/// Semi-transparent polygons of an entity, in its local space (Bevy units):
/// pushed every frame to the effects pass while the entity is visible
/// (`psxfx::push_semi`), placed by its `GlobalTransform`.
#[derive(Component, Clone)]
pub struct FxSemi(pub Arc<Vec<crate::psxfx::FxPrim>>);

/// The semi-transparent polygons of `polys` as effects-pass primitives for
/// camera `view` (psxfx::VIEW_MAIN / VIEW_FAR) and sort bucket `bucket`
/// (-1 = the game's ground list, drawn under everything else). None when
/// there are none.
pub fn semi_prims(polys: &[PsxPoly], view: u8, bucket: i8, to_bevy: impl Fn([i32; 3]) -> Vec3) -> Option<FxSemi> {
    semi_prims_pulled(polys, view, bucket, 0.01, to_bevy)
}

/// `semi_prims` with a depth pull (negative: pushed away from the eye; the
/// scenery's ordering-table bias, scenepush.rs).
pub fn semi_prims_pulled(polys: &[PsxPoly], view: u8, bucket: i8, pull: f32, to_bevy: impl Fn([i32; 3]) -> Vec3) -> Option<FxSemi> {
    use crate::psxfx::{Blend, FxPrim, FxVert};
    let mut out = Vec::new();
    for p in polys.iter().filter(|p| p.semi && p.pos.len() >= 3) {
        let n = p.pos.len().min(4);
        let mut v = [FxVert::default(); 4];
        let mut center = Vec3::ZERO;
        for (i, slot) in v.iter_mut().enumerate().take(n) {
            let pos = to_bevy(p.pos[i]);
            center += pos;
            let t = p.uv.get(i).copied().unwrap_or([0, 0]);
            let c = p.rgb.get(i).copied().unwrap_or([128, 128, 128]);
            let k = if p.tex.is_some() { 128.0 } else { 255.0 };
            // texel centres (as the meshes): floor() picks the texel, the
            // filtered path samples its centre
            *slot = FxVert { pos, uv: [t[0] as f32 + 0.5, t[1] as f32 + 0.5], rgb: [c[0] as f32 / k, c[1] as f32 / k, c[2] as f32 / k] };
        }
        out.push(FxPrim {
            v,
            page: p.tex.map(|(tp, cl)| (tp & !0x60, cl)),
            blend: Blend::Abr(p.abr & 3),
            center: center / n as f32,
            // scenery: plain depth test, always; a hair of tolerance for
            // decals coplanar with opaque scenery
            depth_pull: pull,
            view,
            bucket,
            always_depth: true,
            smooth: true,
            tri: n == 3,
            cull_back: p.cull,
            ..Default::default()
        });
    }
    (!out.is_empty()).then(|| FxSemi(Arc::new(out)))
}

/// Alpha mode of opaque textured polygons (colour-0 texels cut out): a hard
/// 0.5 threshold with point sampling (the PS1's binary transparency); with
/// smoothed textures, alpha to coverage (MSAA samples follow the filtered
/// alpha: soft, fringe-free cut-out edges; Bevy falls back to the threshold
/// when MSAA is off).
pub fn cutout_mode() -> AlphaMode {
    match crate::settings::Settings::global().texture_filter() {
        crate::settings::TexFilter::Smooth => AlphaMode::AlphaToCoverage,
        crate::settings::TexFilter::Nearest => AlphaMode::Mask(0.5),
    }
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
    let mut out = Vec::new();
    for (key, mesh) in meshes_by_key(polys, to_bevy) {
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
                m.alpha_mode = if key.1 { AlphaMode::Blend } else { cutout_mode() };
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

/// The opaque polygons' meshes, one per (texture, semi) key in key order
/// (no assets created: `build_cached` adds them; animated geometry such as
/// the banner rewrites its existing mesh assets in place with them).
pub fn meshes_by_key(polys: &[PsxPoly], to_bevy: impl Fn([i32; 3]) -> Vec3) -> Vec<((Option<(u16, u16)>, bool), Mesh)> {
    let mut groups: HashMap<(Option<(u16, u16)>, bool), Vec<&PsxPoly>> = HashMap::new();
    // semi-transparent polygons go to the effects pass (`semi_prims`)
    for p in polys.iter().filter(|p| !p.semi) {
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
        out.push((key, mesh));
    }
    out
}

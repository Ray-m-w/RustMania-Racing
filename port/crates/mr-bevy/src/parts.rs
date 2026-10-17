//! DAT kind 2/3 "parts" models: camera-facing textured quads (PARK's and
//! SPEEDWAY's tree canopies, HAPPY's / BOG's / GRAVE's / LONDON's ...
//! billboards; 11 of the 18 race courses have some). Session 6 visual check:
//! the port never drew them (`object_polys` only reads 8-byte-vertex
//! meshes).
//!
//! FUN_80028010's switch on the object flags: case 2 calls FUN_8002b7f8 with
//! the rotation camera * object, a GTE rotation diag(1.6, 1, 0) (a record's
//! (x, y) offsets lie in the screen plane; 1.6 = the 512-pixel screen's
//! aspect, which the port's camera already has) and the translation = the
//! pivot in view space, + dz in depth; case 0xA (scaled descriptor) the
//! same with diag(1.6 sx, sy) / 4096; case 3 calls FUN_8002c328 with
//! camera * Ry(-DAT_800b0f0a), z column zeroed: the quad stays upright and
//! turns with the camera's heading. Sorted at T[OTZ + bias] like the meshes
//! (scenepush.rs). Drawn through the effects pass (opaque or ABR as the
//! record's tex bits say), each frame from the chase camera.
//! Not reproduced: kind 3's near subdivision (DivideGT4; affine only) and
//! the per-record far cut.

use bevy::prelude::*;
use mr_assets::dat::{self, Dat, PartQuad, SinTable};

use crate::psxfx::{Blend, FxPrim, FxPrims, FxVert};

/// One parts object (an entity with its `TrackObject`, so the per-frame
/// selection shows / hides it like the meshes).
#[derive(Component)]
pub struct PartsObj {
    /// 2 = screen-aligned, 3 = upright.
    pub kind: u8,
    pub far: bool,
    /// Per-axis scale of the offsets (1.0 or the scaled descriptor's / 4096).
    pub scale: [f32; 2],
    /// OT bias as a push (Bevy units).
    pub push: f32,
    pub quads: Vec<PartPrim>,
}

pub struct PartPrim {
    /// Pivot (Bevy world).
    pub centre: Vec3,
    pub q: PartQuad,
    /// (tpage without ABR, clut), blend.
    pub page: (u16, u16),
    pub blend: Blend,
}

/// Build the parts objects' records (scene.rs setup).
pub fn build(dat: &Dat, o: &dat::Object, sin: &SinTable, slots: &[Option<dat::TexSlot>; 32], push: f32) -> Option<PartsObj> {
    let model = dat.parts_model(o)?;
    let d = dat.desc(o);
    let f = o.loaded_flags();
    let kind = f & 7;
    if kind != 2 && kind != 3 {
        return None;
    }
    let rot = dat::rot_matrix(sin, d.angles);
    let ws = o.layer.world_scale();
    let scale = match d.scale {
        Some(s) if kind == 2 => [s[0] as f32 / 4096.0, s[1] as f32 / 4096.0],
        _ => [1.0, 1.0],
    };
    let mut quads = Vec::new();
    for (part, q) in model.quads() {
        let pv = model.headers[part];
        let w: [i32; 3] = std::array::from_fn(|r| {
            let s: i64 = (0..3).map(|c| rot[r][c] as i64 * pv[c] as i64).sum();
            ws * (d.origin[r] + (s >> 12) as i32)
        });
        let Some(slot) = slots[(q.tex & 0x1F) as usize] else { continue };
        let tpage = slot.tpage.wrapping_add(q.tex & 0x60);
        let mut clut = slot.clut;
        if slot.fixed_clut == 0 || kind == 3 {
            clut = clut.wrapping_add((q.uv[0][0] as u16 >> 6) + 4 * (q.uv[0][1] as u16 >> 6));
        }
        let blend = if q.tex & 0x60 == 0x60 { Blend::Opaque } else { Blend::Abr(((tpage >> 5) & 3) as u8) };
        quads.push(PartPrim { centre: crate::to_bevy(w[0] * 32, w[1] * 32, w[2] * 32), q, page: (tpage & !0x60, clut), blend });
    }
    Some(PartsObj { kind, far: ws != 1, scale, push, quads })
}

fn prims(p: &PartsObj, right: Vec3, up: Vec3, fwd: Vec3, out: &mut Vec<FxPrim>) {
    // one view (DAT) unit in Bevy units, x the layer's scale
    let k = 32.0 / crate::UNITS * if p.far { 4.0 } else { 1.0 };
    let (r, u) = if p.kind == 3 {
        let h = Vec3::new(right.x, 0.0, right.z).normalize_or(right);
        (h, Vec3::Y)
    } else {
        (right, up)
    };
    for pp in &p.quads {
        let q = &pp.q;
        let dz = if p.kind == 2 { q.dz as f32 } else { 0.0 };
        let pos = |i: usize| pp.centre + r * (q.xy[i][0] as f32 * p.scale[0] * k) - u * (q.xy[i][1] as f32 * p.scale[1] * k) + fwd * (dz * k);
        // cyclic record vertices -> GPU order 0, 1, 3, 2
        let ord = [0usize, 1, 3, 2];
        out.push(FxPrim {
            v: std::array::from_fn(|j| {
                let i = ord[j];
                FxVert {
                    pos: pos(i),
                    uv: [q.uv[i][0] as f32 + 0.5, q.uv[i][1] as f32 + 0.5],
                    rgb: q.rgb[i].map(|c| c as f32 / 128.0),
                }
            }),
            page: Some(pp.page),
            blend: pp.blend,
            center: pp.centre,
            depth_pull: 0.01 - p.push,
            view: if p.far { crate::psxfx::VIEW_FAR } else { crate::psxfx::VIEW_MAIN },
            always_depth: true,
            smooth: true,
            ..Default::default()
        });
    }
}

#[allow(clippy::type_complexity)]
fn draw(
    q: Query<(&PartsObj, &InheritedVisibility, Option<&crate::eyes::EyeMask>)>,
    cam: Query<&Transform, With<crate::ChaseCamera>>,
    cam2: Query<(&Transform, &Camera), (With<crate::ChaseCamera2>, Without<crate::ChaseCamera>)>,
    mut fx: ResMut<FxPrims>,
) {
    let Ok(ct) = cam.single() else { return };
    // phase 13: split screen: each eye's selection, facing its own camera
    // (the game's pass per camera; eyes.rs)
    let c2 = cam2.single().ok().filter(|(_, c)| c.is_active).map(|(t, _)| *t);
    let axes = |t: &Transform| (t.rotation * Vec3::X, t.rotation * Vec3::Y, t.rotation * -Vec3::Z);
    for (p, vis, eye) in q.iter() {
        if !vis.get() {
            continue;
        }
        let mask = eye.map_or(3, |e| e.0 & 3);
        let Some(t2) = c2.as_ref() else {
            let (right, up, fwd) = axes(ct);
            prims(p, right, up, fwd, &mut fx.list);
            continue;
        };
        for (t, bit, two) in [(ct, 1u8, false), (t2, 2, true)] {
            if mask & bit == 0 {
                continue;
            }
            let (right, up, fwd) = axes(t);
            let n = fx.list.len();
            prims(p, right, up, fwd, &mut fx.list);
            for q in fx.list[n..].iter_mut() {
                q.eye_only = true;
                if two {
                    q.view = if q.view == crate::psxfx::VIEW_FAR { crate::psxfx::VIEW_FAR2 } else { crate::psxfx::VIEW_MAIN2 };
                }
            }
        }
    }
}

pub struct PartsPlugin;

impl Plugin for PartsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, draw.after(crate::effects::draw));
    }
}

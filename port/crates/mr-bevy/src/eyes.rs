//! Phase 13: per-eye visibility in split screen (docs/spec/camera.md "Split
//! screen"). FUN_80078F38 draws the world twice, once per camera, and every
//! per-frame selection the draw makes runs again for that eye: the kart LOD
//! +0x4B4 (FUN_8002669C before each pass), the scenery's visibility stream
//! and cull (FUN_80010000 / FUN_8001031C with the split draw distances of
//! FUN_8002E2F0 and the frustum's vertical row x 3/2), the banners, the
//! pickups and billboards (effects.rs). The port draws one Bevy world for
//! both chase cameras, so what only one eye shows moves to that eye's
//! layers: `EyeMask` on an entity sets the render layers of its mesh
//! descendants (bit 0 = player 1's view, bit 1 = player 2's; 3 = the plain
//! layers both see; 0 is never set: hide the entity instead).

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;

use crate::scene::{EYE1_LAYER, EYE2_LAYER, FAR_EYE1_LAYER, FAR_EYE2_LAYER, FAR_LAYER};

/// The views an entity's meshes show in (bit 0 player 1, bit 1 player 2).
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub struct EyeMask(pub u8);

impl Default for EyeMask {
    fn default() -> Self {
        EyeMask(3)
    }
}

/// The layers of a near (`far` = false) or far-pass mesh shown in `mask`.
pub fn layers_for(far: bool, mask: u8) -> RenderLayers {
    match (far, mask & 3) {
        (false, 1) => RenderLayers::layer(EYE1_LAYER),
        (false, 2) => RenderLayers::layer(EYE2_LAYER),
        (false, _) => RenderLayers::layer(0),
        (true, 1) => RenderLayers::layer(FAR_EYE1_LAYER),
        (true, 2) => RenderLayers::layer(FAR_EYE2_LAYER),
        (true, _) => RenderLayers::layer(FAR_LAYER),
    }
}

fn is_far(l: Option<&RenderLayers>) -> bool {
    l.is_some_and(|l| l.intersects(&RenderLayers::from_layers(&[FAR_LAYER, FAR_EYE1_LAYER, FAR_EYE2_LAYER])))
}

/// Visibility and mask for one eye selection: hidden in both views ->
/// Hidden; else Inherited with the mask of the views that show it.
pub fn split_vis(p1: bool, p2: bool) -> (Visibility, u8) {
    let m = p1 as u8 | (p2 as u8) << 1;
    (if m == 0 { Visibility::Hidden } else { Visibility::Inherited }, if m == 0 { 3 } else { m })
}

/// An `EyeMask` change: the render layers of the entity (when it holds a
/// mesh) and of its mesh descendants.
#[allow(clippy::type_complexity)]
fn apply(
    q: Query<(Entity, &EyeMask), (Changed<EyeMask>, Without<crate::psxmesh::FxSemi>)>,
    children: Query<&Children>,
    meshes: Query<Option<&RenderLayers>, With<Mesh3d>>,
    semis: Query<Option<&EyeMask>, With<crate::psxmesh::FxSemi>>,
    mut commands: Commands,
) {
    for (e, m) in q.iter() {
        let mut stack = vec![e];
        while let Some(x) = stack.pop() {
            if let Ok(l) = meshes.get(x) {
                let want = layers_for(is_far(l), m.0);
                if l != Some(&want) {
                    commands.entity(x).insert(want);
                }
            }
            // the semi-transparent polygons the effects pass draws
            // (psxfx.rs push_semi reads the mask)
            if x != e {
                if let Ok(sm) = semis.get(x) {
                    if sm != Some(m) {
                        commands.entity(x).insert(*m);
                    }
                }
            }
            if let Ok(c) = children.get(x) {
                stack.extend(c.iter());
            }
        }
    }
}

pub struct EyesPlugin;

impl Plugin for EyesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostUpdate, apply.before(bevy::camera::visibility::VisibilitySystems::CheckVisibility));
    }
}

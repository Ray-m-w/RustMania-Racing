//! Enhanced look prototype (settings `lighting = enhanced`, off by default;
//! the owner decides by eye: analysis/s5/lighting/).
//!
//! Atmospheric depth over the game's baked vertex colours: now that the whole
//! course is drawn, far scenery fades softly toward the sky's own horizon
//! colour (DOCK.AXE vertices near the horizon), so distance reads as depth
//! instead of a hard cut. Nothing within the game's own draw range
//! (~4 x cd0 = 9,500 DAT units = 75 Bevy units) changes much; the vertex
//! colours are untouched (Bevy's distance fog is applied after the unlit
//! colour). Only the main (layer-0) camera fogs: the sky and the far
//! backdrop keep their colours.

use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;

use crate::settings::{Lighting, Settings};

/// Average colour of the sky dome near the horizon (set by scene::setup).
#[derive(Resource, Clone, Copy, Debug)]
pub struct SkyHorizon(pub Color);

/// Fog distances in Bevy units (1 Bevy unit = 4096 world = 128 DAT units).
pub const FOG_START: f32 = 40.0;
pub const FOG_END: f32 = 650.0;
/// Strongest haze at FOG_END (fog colour alpha).
pub const FOG_MAX: f32 = 0.6;

pub fn plugin(app: &mut App) {
    app.add_systems(Update, apply);
}

fn apply(
    settings: Res<Settings>,
    sky: Option<Res<SkyHorizon>>,
    cams: Query<Entity, (With<crate::ChaseCamera>, Without<DistanceFog>)>,
    mut commands: Commands,
) {
    if settings.lighting() != Lighting::Enhanced {
        return;
    }
    let Some(sky) = sky else { return };
    for e in cams.iter() {
        commands.entity(e).insert(DistanceFog {
            color: sky.0.with_alpha(FOG_MAX),
            directional_light_color: Color::NONE,
            directional_light_exponent: 8.0,
            falloff: FogFalloff::Linear { start: FOG_START, end: FOG_END },
        });
    }
}

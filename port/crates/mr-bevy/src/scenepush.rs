//! The game's ordering-table bias for layer-0 scenery, as a depth push
//! (ot_push.wgsl). FUN_80012390 sorts every scenery quad at T[OTZ + bias]
//! (bias = course byte 0x800ACB3F + 16 * descriptor b_lo: 96..336 view
//! units / 4), the kart parts at T[OTZ] (FUN_8001EC2C), so a kart whose
//! record sits in the drawn ground (the courses' DAT surface is up to ~150
//! DAT units above the TER the karts run on: GHOST, SPEEDWAY, ROCK, RALLY)
//! is still drawn whole over it. Session 6 visual check (GHOST "wheels in
//! the floor"); `MRM_NO_OT_PUSH=1` turns it off (dev A/B).

use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use std::collections::HashMap;

pub type PushMaterial = ExtendedMaterial<StandardMaterial, OtPush>;

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct OtPush {
    /// x: push away from the eye in Bevy units.
    #[uniform(100)]
    pub p: Vec4,
}

const OT_PUSH_SHADER: Handle<Shader> = bevy::asset::uuid_handle!("2b8e4d71-5c3a-4f19-9a6e-0d4c7b1e8f25");

impl MaterialExtension for OtPush {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(OT_PUSH_SHADER)
    }
}

pub fn enabled() -> bool {
    std::env::var("MRM_NO_OT_PUSH").is_err()
}

/// OT bias (ordering-table units) -> Bevy units: 4 view units (DAT units)
/// per OT step, 128 DAT units per Bevy unit (`crate::to_bevy`).
pub fn push_units(bias: i32) -> f32 {
    (4 * bias) as f32 * 32.0 / crate::UNITS
}

/// FUN_80012390's far cut for a layer-0 object as a view depth in Bevy
/// units: OTZ >= cd4 <=> average SZ >= 4 * cd4 DAT units; cd4 = de8 for the
/// near list, cd0 for the ground list (flags 0x40 / 0x80), 3000 for flag
/// 0x80 (FUN_80027de0). 0 = no cut (the default full view).
pub fn far_cut(flags: u8, draw: &mr_assets::dat::DrawParams) -> f32 {
    let cd4 = if flags & 0x80 != 0 { 3000 } else if flags & 0x40 != 0 { draw.cd0 } else { draw.de8 };
    (4 * cd4) as f32 * 32.0 / crate::UNITS
}

/// One pushed copy of each (StandardMaterial, push) pair.
#[derive(Default)]
pub struct PushCache(HashMap<(AssetId<StandardMaterial>, u32, u32), Handle<PushMaterial>>);

impl PushCache {
    pub fn get(&mut self, std: &Assets<StandardMaterial>, push: &mut Assets<PushMaterial>, h: &Handle<StandardMaterial>, units: f32, far: f32) -> Handle<PushMaterial> {
        self.0
            .entry((h.id(), units.to_bits(), far.to_bits()))
            .or_insert_with(|| {
                let mut base = std.get(h).cloned().unwrap_or_default();
                // the push replaces the ground list's constant depth bias
                base.depth_bias = 0.0;
                push.add(ExtendedMaterial { base, extension: OtPush { p: Vec4::new(units, far, 0.0, 0.0) } })
            })
            .clone()
    }
}

pub struct ScenePushPlugin;

impl Plugin for ScenePushPlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::load_internal_asset!(app, OT_PUSH_SHADER, "ot_push.wgsl", Shader::from_wgsl);
        app.add_plugins(MaterialPlugin::<PushMaterial>::default());
    }
}

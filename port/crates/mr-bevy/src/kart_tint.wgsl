// Kart models (karts.rs): the StandardMaterial's unlit colour, then the
// kart's colour factors (+0x414.. / +0x416.., 4.12) multiplied in the game's
// gamma space, as the GTE colour matrix does: lin(min(srgb(c) * f, 1)).
#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
}

struct KartTint {
    rgb: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> kart_tint: KartTint;

fn to_srgb(c: f32) -> f32 {
    if (c <= 0.0031308) {
        return c * 12.92;
    }
    return 1.055 * pow(c, 1.0 / 2.4) - 0.055;
}

fn to_lin(c: f32) -> f32 {
    if (c <= 0.04045) {
        return c / 12.92;
    }
    return pow((c + 0.055) / 1.055, 2.4);
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);
    let c = pbr_input.material.base_color;
    let g = min(vec3<f32>(to_srgb(c.r), to_srgb(c.g), to_srgb(c.b)) * kart_tint.rgb.rgb, vec3<f32>(1.0));
    var out: FragmentOutput;
    out.color = main_pass_post_lighting_processing(pbr_input, vec4<f32>(to_lin(g.r), to_lin(g.g), to_lin(g.b), c.a));
    return out;
}

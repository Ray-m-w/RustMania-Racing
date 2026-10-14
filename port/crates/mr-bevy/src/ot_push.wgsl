// Layer-0 scenery (scene.rs, scenepush.rs): the StandardMaterial's unlit
// colour, with the fragment's depth pushed away from the eye by the object's
// ordering-table bias. The game sorts a scenery quad at T[OTZ + bias]
// (FUN_80012390; bias = course byte 0x800ACB3F + 16 * b_lo, OTZ = average
// SZ / 4) and a kart polygon at T[OTZ] (FUN_8001EC2C), so against karts,
// sprites and its own ungrounded neighbours a scenery polygon sits 4 * bias
// view units further back than its geometry. Reverse-Z, infinite far:
// depth = near / view distance.
#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, main_pass_post_lighting_processing},
    forward_io::VertexOutput,
    mesh_view_bindings::view,
}

struct OtPush {
    // x: push (Bevy units); y: far cut (Bevy units of view depth, 0 = none):
    // the layer-0 renderer skips a polygon whose OTZ (average SZ / 4) is
    // >= DAT_800b0cd4 (FUN_80012390; near list de8, ground list cd0,
    // flag 0x80 3000), done here per fragment (faithful draw distance only)
    p: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> ot_push: OtPush;

struct Out {
    @location(0) color: vec4<f32>,
    @builtin(frag_depth) depth: f32,
};

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> Out {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);
    var out: Out;
    out.color = main_pass_post_lighting_processing(pbr_input, pbr_input.material.base_color);
    let near = view.clip_from_view[3][2];
    let z = in.position.z;
    if (ot_push.p.y > 0.0 && z > 0.0 && near / z >= ot_push.p.y) {
        discard;
    }
    if (z > 0.0) {
        out.depth = near / (near / z + ot_push.p.x);
    } else {
        out.depth = z;
    }
    return out;
}

// psxfx.rs composite: the gamma-space effects copy written back into the
// (sRGB) main texture: the raw bytes are decoded so the sRGB store
// re-encodes them unchanged.
@group(0) @binding(0) var fx_src: texture_2d<f32>;

struct CompOut {
    @builtin(position) pos: vec4<f32>,
};

@vertex
fn comp_vs(@builtin(vertex_index) i: u32) -> CompOut {
    var o: CompOut;
    let x = f32((i << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(i & 2u) * 2.0 - 1.0;
    o.pos = vec4<f32>(x, y, 0.0, 1.0);
    return o;
}

fn srgb_to_linear(c: f32) -> f32 {
    if (c <= 0.04045) {
        return c / 12.92;
    }
    return pow((c + 0.055) / 1.055, 2.4);
}

@fragment
fn comp_fs(in: CompOut) -> @location(0) vec4<f32> {
    let t = textureLoad(fx_src, vec2<i32>(floor(in.pos.xy)), 0);
#ifdef LINEAR_SRC
    // fx_blend = linear: the working copy is sRGB too (already decoded)
    return t;
#endif
    return vec4<f32>(srgb_to_linear(t.r), srgb_to_linear(t.g), srgb_to_linear(t.b), t.a);
}

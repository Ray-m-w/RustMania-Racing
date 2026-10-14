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
    return vec4<f32>(t.rgb, 1.0);
#endif
    var c = t.rgb;
    if (t.a < 0.5) {
        // written by the pass: the GPU's 15-bit store (truncate to 5 bits),
        // on the display lattice floor(c5 * 255 / 31) (bytes x: c5 =
        // floor((x + 1) * 31 / 255) maps every lattice byte to itself)
        let x = round(c * 255.0);
        let c5 = floor((x + 1.0) * 31.0 / 255.0);
        c = floor(c5 * 255.0 / 31.0) / 255.0;
    }
    return vec4<f32>(srgb_to_linear(c.r), srgb_to_linear(c.g), srgb_to_linear(c.b), 1.0);
}

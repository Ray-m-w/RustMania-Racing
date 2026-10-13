// PS1 semi-transparency for the effects pass (psxfx.rs).
//
// The colour target is a non-sRGB copy of the frame: its bytes are the
// sRGB-encoded (gamma) values, as the PS1's 15-bit framebuffer holds them, so
// the fixed-function blend states of the pipelines (B/2+F/2, B+F, B-F, B+F/4)
// act in the game's colour space exactly as the GPU's do.
//
// Occlusion: the sprite is tested against the scene's depth (reverse-Z) in
// the fragment shader. An occluder that is an upward-facing surface below the
// sprite's centre (+ eps) and within `radius` of it does not hide it: the
// game draws its "ground" list under the sprites (ordering table), so sprites
// sitting in or on the ground stay whole; terrain further away still hides.

struct FxView {
    clip_from_world: mat4x4<f32>,
    world_from_clip: mat4x4<f32>,
    // x, y, width, height of the viewport (physical pixels)
    viewport: vec4<f32>,
    // x: |normal.y| above which an occluder counts as ground; y: unused (the
    // depth test is per primitive: flag DEPTH); z: the projection's near
    // plane (depth = near / view z); w: 1 = orthographic (the HUD: depths
    // compared directly)
    params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> view: FxView;
#ifdef MSAA_DEPTH
@group(0) @binding(1) var depth_tex: texture_depth_multisampled_2d;
#else
@group(0) @binding(1) var depth_tex: texture_depth_2d;
#endif
@group(1) @binding(0) var page: texture_2d<f32>;
// The page as the scenery shows it (texture_filter: smooth = bilinear +
// mips, sRGB); flag SMOOTH. Else the raw page again.
@group(1) @binding(1) var page_f: texture_2d<f32>;
@group(1) @binding(2) var page_s: sampler;

struct VIn {
    @location(0) pos: vec4<f32>,
    // u, v (texel units), flags, output scale
    @location(1) uvf: vec4<f32>,
    // modulation (gamma space: texel * rgb / 128; untextured: rgb / 255), alpha
    @location(2) col: vec4<f32>,
    // centre (world), ground eps (< 0: no ground exemption)
    @location(3) ctr: vec4<f32>,
    // x: depth pull toward the eye for the occlusion test (the ordering
    // table's per-sprite bias); y: the ground exemption's radius around the
    // centre (Bevy units)
    @location(4) extra: vec4<f32>,
};

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) col: vec4<f32>,
    @location(2) @interpolate(flat) flags: u32,
    @location(3) @interpolate(flat) scale: f32,
    @location(4) @interpolate(flat) ctr: vec4<f32>,
    // view depth of the fragment minus the pull
    @location(5) vdepth: f32,
    @location(6) @interpolate(flat) radius: f32,
};

@vertex
fn vs(v: VIn) -> VOut {
    var o: VOut;
    o.clip = view.clip_from_world * vec4<f32>(v.pos.xyz, 1.0);
    o.uv = v.uvf.xy;
    o.col = v.col;
    o.flags = u32(v.uvf.z);
    o.scale = v.uvf.w;
    o.ctr = v.ctr;
    o.vdepth = o.clip.w - v.extra.x;
    o.radius = v.extra.y;
    return o;
}

fn scene_depth(p: vec2<i32>) -> f32 {
    return textureLoad(depth_tex, p, 0);
}

fn world_at(p: vec2<i32>, d: f32) -> vec3<f32> {
    let vp = view.viewport;
    let ndc = vec2<f32>(
        (f32(p.x) + 0.5 - vp.x) / vp.z * 2.0 - 1.0,
        1.0 - (f32(p.y) + 0.5 - vp.y) / vp.w * 2.0,
    );
    let h = view.world_from_clip * vec4<f32>(ndc, d, 1.0);
    return h.xyz / h.w;
}

const TEXTURED: u32 = 1u;
const SEMI_TEXELS: u32 = 2u;
const OPAQUE_TEXELS: u32 = 4u;
const DEPTH: u32 = 8u;
const SMOOTH: u32 = 16u;

fn srgb(c: f32) -> f32 {
    if (c <= 0.0031308) {
        return c * 12.92;
    }
    return 1.055 * pow(c, 1.0 / 2.4) - 0.055;
}

@fragment
fn fs(in: VOut) -> @location(0) vec4<f32> {
    // derivatives first (uniform control flow)
    let uvn = in.uv / 256.0;
    let gx = dpdx(uvn);
    let gy = dpdy(uvn);
    let fp = vec2<i32>(floor(in.clip.xy));
    if ((in.flags & DEPTH) != 0u) {
        let ds = scene_depth(fp);
        var hidden = false;
        if (view.params.w > 0.5) {
            // orthographic reverse-Z: a larger depth is nearer
            hidden = ds > in.clip.z + 1e-6;
        } else {
            // reverse-Z, infinite far: depth = near / view z (0 = sky / nothing)
            hidden = ds > 0.0 && view.params.z / ds < in.vdepth;
        }
        if (hidden) {
            var exempt = false;
            if (in.ctr.w >= 0.0) {
                let ps = world_at(fp, ds);
                if (ps.y <= in.ctr.y + in.ctr.w && distance(ps, in.ctr.xyz) <= in.radius) {
                    let px = world_at(fp + vec2<i32>(1, 0), scene_depth(fp + vec2<i32>(1, 0)));
                    let py = world_at(fp + vec2<i32>(0, 1), scene_depth(fp + vec2<i32>(0, 1)));
                    let n = cross(px - ps, py - ps);
                    let l = length(n);
                    if (l > 0.0 && abs(n.y) / l > view.params.x) {
                        exempt = true;
                    }
                }
            }
            if (!exempt) {
                discard;
            }
        }
    }
    var c = in.col.rgb;
    if ((in.flags & TEXTURED) != 0u) {
        let t = textureLoad(page, clamp(vec2<i32>(floor(in.uv)), vec2<i32>(0), vec2<i32>(255)), 0);
        var rgb = t.rgb;
        if ((in.flags & SMOOTH) != 0u) {
            // filtered colour (linear light from the sRGB page, back to the
            // game's space), cut out where the filtered coverage is < 0.5
            let f = textureSampleGrad(page_f, page_s, uvn, gx, gy);
            if (f.a < 0.5) {
                discard;
            }
            rgb = vec3<f32>(srgb(f.r), srgb(f.g), srgb(f.b));
        } else if (t.a == 0.0) {
            discard;
        }
        let stp = t.a < 0.75;
        if ((in.flags & SEMI_TEXELS) != 0u && !stp) {
            discard;
        }
        if ((in.flags & OPAQUE_TEXELS) != 0u && stp) {
            discard;
        }
        c = min(rgb * c, vec3<f32>(1.0));
    }
    c = min(c, vec3<f32>(1.0));
#ifdef LINEAR_OUT
    // fx_blend = linear: an sRGB target, so hand it linear light
    c = vec3<f32>(lin(c.r), lin(c.g), lin(c.b));
#endif
    return vec4<f32>(c * in.scale, in.col.a);
}

fn lin(c: f32) -> f32 {
    if (c <= 0.04045) {
        return c / 12.92;
    }
    return pow((c + 0.055) / 1.055, 2.4);
}

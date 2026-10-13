// PS1 semi-transparency for the effects pass (psxfx.rs).
//
// The colour target is a non-sRGB copy of the frame: its bytes are the
// sRGB-encoded (gamma) values, as the PS1's 15-bit framebuffer holds them, so
// the fixed-function blend states of the pipelines (B/2+F/2, B+F, B-F, B+F/4)
// act in the game's colour space exactly as the GPU's do.
//
// Occlusion: the sprite is tested against the scene's depth (reverse-Z) in
// the fragment shader. An occluder that is an upward-facing surface below the
// sprite's centre (+ eps) does not hide it: the game draws its "ground" list
// under the sprites (ordering table), so sprites sitting in or on the ground
// stay whole.

struct FxView {
    clip_from_world: mat4x4<f32>,
    world_from_clip: mat4x4<f32>,
    // x, y, width, height of the viewport (physical pixels)
    viewport: vec4<f32>,
    // x: |normal.y| above which an occluder counts as ground; y: 1 = test
    // depth; z: the projection's near plane (depth = near / view z)
    params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> view: FxView;
#ifdef MSAA_DEPTH
@group(0) @binding(1) var depth_tex: texture_depth_multisampled_2d;
#else
@group(0) @binding(1) var depth_tex: texture_depth_2d;
#endif
@group(1) @binding(0) var page: texture_2d<f32>;

struct VIn {
    @location(0) pos: vec4<f32>,
    // u, v (texel units), flags, output scale
    @location(1) uvf: vec4<f32>,
    // modulation (gamma space: texel * rgb / 128; untextured: rgb / 255), alpha
    @location(2) col: vec4<f32>,
    // centre (world), ground eps (< 0: no ground exemption)
    @location(3) ctr: vec4<f32>,
    // x: depth pull toward the eye for the occlusion test (the ordering
    // table's per-sprite bias), world units
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

@fragment
fn fs(in: VOut) -> @location(0) vec4<f32> {
    let fp = vec2<i32>(floor(in.clip.xy));
    if (view.params.y > 0.5) {
        let ds = scene_depth(fp);
        // reverse-Z, infinite far: depth = near / view z (0 = sky / nothing)
        if (ds > 0.0 && view.params.z / ds < in.vdepth) {
            var exempt = false;
            if (in.ctr.w >= 0.0) {
                let ps = world_at(fp, ds);
                if (ps.y <= in.ctr.y + in.ctr.w) {
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
        if (t.a == 0.0) {
            discard;
        }
        let stp = t.a < 0.75;
        if ((in.flags & SEMI_TEXELS) != 0u && !stp) {
            discard;
        }
        if ((in.flags & OPAQUE_TEXELS) != 0u && stp) {
            discard;
        }
        c = min(t.rgb * c, vec3<f32>(1.0));
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

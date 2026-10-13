//! The PS1 effects pass: billboards, streaks and kart shadows drawn with the
//! GPU's semi-transparency in the game's colour space, hidden by the scene.
//!
//! Why a pass of its own: Bevy blends in linear light (sRGB render target),
//! the PS1 blends the 15-bit framebuffer values themselves (gamma-encoded).
//! B + F in linear space makes dim additive texels add ~2.4x too little light
//! and B - F cannot be expressed at all. So, for the chase camera, after the
//! main transparent pass:
//! 1. the frame (sRGB main texture, resolved) is copied to a non-sRGB texture
//!    of the same bytes (copy-compatible formats: only the sRGB-ness differs);
//! 2. the effect primitives are drawn into it, back to front by view depth
//!    (the ordering table's order), with fixed-function blend states that act
//!    on those bytes: ABR 0 = B/2 + F/2, 1 = B + F, 2 = B - F, 3 = B + F/4
//!    (wgpu clamps to 0..1 like the GPU clamps to 0..31); semi-transparent
//!    textured primitives honour the texel STP bit (STP clear = opaque texel);
//!    texel 0x0000 is transparent;
//! 3. the result is drawn back over the main texture (decoded in the shader,
//!    re-encoded by the sRGB store: the same bytes), all MSAA samples.
//! Remaining error: 8-bit instead of 5-bit channels (no 15-bit quantisation,
//! no dithering) and the 5->8 bit texel expansion (c * 255 / 31).
//!
//! Occlusion (Session 5, "sprite depth"): every fragment is depth-tested
//! against the scene's depth buffer, except where the occluder is an
//! upward-facing surface below the sprite's centre (the game draws its
//! "ground" list under everything else in the ordering table, so a sprite
//! sitting in or on the ground is never cut by it). See psxfx.wgsl.
//!
//! Sources push `FxPrim`s into `FxPrims` during Update (effects.rs,
//! kartshadow.rs); `build_frame` (PostUpdate) sorts and packs them for the
//! render world.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::core_pipeline::core_3d::main_transparent_pass_3d;
use bevy::core_pipeline::schedule::{Core3d, Core3dSystems};
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::camera::ExtractedCamera;
use bevy::render::extract_component::{ExtractComponent, ExtractComponentPlugin};
use bevy::render::extract_resource::{ExtractResource, ExtractResourcePlugin};
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::binding_types::{texture_2d, texture_depth_2d, texture_depth_2d_multisampled, uniform_buffer_sized};
use bevy::render::render_resource::*;
use bevy::render::renderer::{RenderContext, ViewQuery};
use bevy::render::texture::GpuImage;
use bevy::render::view::{ExtractedView, Msaa, ViewDepthTexture, ViewTarget};
use bevy::render::{RenderApp, RenderStartup};
use bevy::mesh::VertexBufferLayout;
use bevy::shader::{Shader, ShaderDefVal};
use mr_assets::vram::{self, Vram};

/// How a primitive is blended (the tpage ABR bits when semi-transparent).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Blend {
    Opaque,
    /// 0 B/2+F/2, 1 B+F, 2 B-F, 3 B+F/4
    Abr(u8),
}

#[derive(Clone, Copy, Debug)]
pub struct FxVert {
    /// Bevy world position.
    pub pos: Vec3,
    /// Texel coordinates (0..256).
    pub uv: [f32; 2],
    /// Modulation in the game's space: rgb / 128 for textured primitives
    /// (texel * rgb / 128), rgb / 255 for untextured ones.
    pub rgb: [f32; 3],
}

/// One quad (PS1 vertex order: triangles 0,1,2 and 1,3,2).
#[derive(Clone, Debug)]
pub struct FxPrim {
    pub v: [FxVert; 4],
    /// (tpage without the ABR bits, clut) when textured.
    pub page: Option<(u16, u16)>,
    pub blend: Blend,
    /// Sort point and the occlusion rule's centre (Bevy world).
    pub center: Vec3,
    /// Upward-facing occluders below `center.y + ground_eps` do not hide the
    /// primitive; negative = plain depth test.
    pub ground_eps: f32,
    /// Pulled toward the eye by this much (Bevy units) for the occlusion
    /// test only: the ordering-table bias some sprites carry (FUN_80011E3C
    /// sorts at OTZ * 4 + bias * 4).
    pub depth_pull: f32,
}

/// What the sources queue this frame (cleared by `build_frame`).
#[derive(Resource, Default)]
pub struct FxPrims {
    pub list: Vec<FxPrim>,
}

/// The chase camera's view: the pass runs for views with this marker.
#[derive(Component, Clone, Copy, ExtractComponent)]
pub struct PsxFxView;

/// One draw of the packed frame.
#[derive(Clone, Debug)]
struct GpuDraw {
    page: Option<AssetId<Image>>,
    /// 0 opaque, 1..=4 ABR 0..3
    pipe: u8,
    first: u32,
    count: u32,
}

/// The packed frame for the render world.
#[derive(Resource, Clone, Default, ExtractResource)]
pub struct FxGpuFrame {
    verts: Vec<f32>,
    draws: Vec<GpuDraw>,
    /// Depth-test against the scene.
    depth_test: bool,
    /// Blend in linear light (fx_blend = linear).
    linear: bool,
}

/// Page images in the game's raw colours (Rgba8Unorm, not sRGB): alpha 0 =
/// texel 0x0000 (transparent), 128 = STP set, 255 = STP clear.
#[derive(Resource, Default)]
pub struct FxPages {
    cache: HashMap<(u16, u16), Handle<Image>>,
    white: Option<Handle<Image>>,
}

fn texel_rgba(t: u16) -> [u8; 4] {
    if t == 0 {
        return [0, 0, 0, 0];
    }
    let c = vram::rgba(t & 0x7FFF);
    [c[0], c[1], c[2], if t & 0x8000 != 0 { 128 } else { 255 }]
}

impl FxPages {
    pub fn get(&mut self, v: &Vram, images: &mut Assets<Image>, tpage: u16, clut: u16) -> Handle<Image> {
        let key = (tpage & !0x60, clut);
        if let Some(h) = self.cache.get(&key) {
            return h.clone();
        }
        let (px, py, depth) = vram::tpage(key.0);
        let (cx, cy) = vram::clut(clut);
        let mut data = Vec::with_capacity(256 * 256 * 4);
        for y in 0..256u32 {
            for x in 0..256u32 {
                data.extend_from_slice(&texel_rgba(v.texel(depth, px, py, cx, cy, x as u8, y as u8)));
            }
        }
        let h = images.add(page_image(256, data));
        self.cache.insert(key, h.clone());
        h
    }

    /// Redraw the texels of cached pages that sample VRAM rectangle (x, y, w, h)
    /// (after an animated-texture LoadImage, animtex.rs).
    pub fn refresh(&mut self, v: &Vram, images: &mut Assets<Image>, x: usize, y: usize, w: usize, h: usize) {
        for (&(tp, cl), handle) in self.cache.iter() {
            let (px, py, depth) = vram::tpage(tp);
            let per = match depth {
                vram::Depth::B4 => 4,
                vram::Depth::B8 => 2,
                vram::Depth::B15 => 1,
            };
            let pw = 256 / per;
            if x + w <= px || x >= px + pw || y + h <= py || y >= py + 256 {
                continue;
            }
            let (cx, cy) = vram::clut(cl);
            let u0 = (x.max(px) - px) * per;
            let u1 = ((x + w).min(px + pw) - px) * per;
            let v0 = y.max(py) - py;
            let v1 = (y + h).min(py + 256) - py;
            let Some(mut img) = images.get_mut(handle) else { continue };
            let Some(d) = img.data.as_mut() else { continue };
            for vv in v0..v1 {
                for u in u0..u1 {
                    let o = (vv * 256 + u) * 4;
                    d[o..o + 4].copy_from_slice(&texel_rgba(v.texel(depth, px, py, cx, cy, u as u8, vv as u8)));
                }
            }
        }
    }

    /// Debug: write every cached page as DIR/page_<tpage>_<clut>_<tag>.png
    /// (alpha shown as is: 0 transparent, 128 STP).
    pub fn dump(&self, images: &Assets<Image>, dir: &str, tag: &str) {
        for (&(tp, cl), h) in &self.cache {
            if let Some(d) = images.get(h).and_then(|i| i.data.clone()) {
                let _ = std::fs::write(format!("{dir}/page_{tp:04x}_{cl:04x}_{tag}.png"), mr_assets::img::png_rgba(256, 256, &d));
            }
        }
    }

    fn white(&mut self, images: &mut Assets<Image>) -> Handle<Image> {
        self.white.get_or_insert_with(|| images.add(page_image(1, vec![255; 4]))).clone()
    }
}

fn page_image(n: u32, data: Vec<u8>) -> Image {
    let mut img = Image::new(
        Extent3d { width: n, height: n, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    img.sampler = ImageSampler::nearest();
    img
}

/// Options (Settings rows under "F effects").
#[derive(Resource, Clone, Copy)]
pub struct FxOptions {
    pub depth_test: bool,
    /// `fx_blend = linear`: blend in linear light instead of the GPU's
    /// gamma space (the working copy stays sRGB).
    pub linear: bool,
}

pub struct PsxFxPlugin;

const SHADER: &str = include_str!("psxfx.wgsl");
const COMP_SHADER: &str = include_str!("psxfx_comp.wgsl");

#[derive(Resource, Clone)]
struct FxShaders {
    main: Handle<Shader>,
    comp: Handle<Shader>,
}

impl Plugin for PsxFxPlugin {
    fn build(&self, app: &mut App) {
        let (main, comp) = {
            let mut shaders = app.world_mut().resource_mut::<Assets<Shader>>();
            (shaders.add(Shader::from_wgsl(SHADER, "mr-bevy/psxfx.wgsl")), shaders.add(Shader::from_wgsl(COMP_SHADER, "mr-bevy/psxfx_comp.wgsl")))
        };
        let s = crate::settings::Settings::global();
        app.insert_resource(FxOptions { depth_test: s.bool("fx_depth_test", true), linear: s.get("fx_blend") == Some("linear") })
            .init_resource::<FxPrims>()
            .init_resource::<FxPages>()
            .init_resource::<FxGpuFrame>()
            .add_plugins((ExtractResourcePlugin::<FxGpuFrame>::default(), ExtractComponentPlugin::<PsxFxView>::default()))
            .add_systems(Update, mark_camera)
            .add_systems(PostUpdate, build_frame.after(bevy::transform::TransformSystems::Propagate));
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
        render_app
            .insert_resource(FxShaders { main, comp })
            .add_systems(RenderStartup, init_pipes)
            .add_systems(Core3d, fx_pass.after(main_transparent_pass_3d).in_set(Core3dSystems::MainPass));
    }
}

/// Give the chase camera the pass marker and a depth texture the pass can
/// read (TEXTURE_BINDING; the target's depth texture is shared by the
/// cameras that draw into it, usages OR'ed).
fn mark_camera(mut commands: Commands, mut q: Query<(Entity, &mut Camera3d), (With<crate::ChaseCamera>, Without<PsxFxView>)>) {
    for (e, mut c3) in q.iter_mut() {
        let u = TextureUsages::from_bits_truncate(c3.depth_texture_usages.0) | TextureUsages::TEXTURE_BINDING | TextureUsages::RENDER_ATTACHMENT;
        c3.depth_texture_usages = u.into();
        commands.entity(e).insert(PsxFxView);
    }
}

/// f32s per vertex (5 x vec4).
const VERT_F32: usize = 20;
const FLAG_TEXTURED: u32 = 1;
const FLAG_SEMI_TEXELS: u32 = 2;
const FLAG_OPAQUE_TEXELS: u32 = 4;

/// Sort (far to near, as the ordering table draws) and pack.
fn build_frame(
    mut prims: ResMut<FxPrims>,
    mut pages: ResMut<FxPages>,
    vt: Option<Res<crate::vramtex::VramTextures>>,
    mut images: ResMut<Assets<Image>>,
    opts: Res<FxOptions>,
    cam: Query<&GlobalTransform, With<crate::ChaseCamera>>,
    mut out: ResMut<FxGpuFrame>,
) {
    let list = std::mem::take(&mut prims.list);
    let (Some(vt), Ok(ct)) = (vt, cam.single()) else {
        if !out.draws.is_empty() {
            *out = FxGpuFrame::default();
        }
        return;
    };
    if list.is_empty() && out.draws.is_empty() {
        return;
    }
    let eye = ct.translation();
    let fwd = ct.forward().as_vec3();
    let mut order: Vec<(f32, usize)> = list.iter().enumerate().map(|(i, p)| ((p.center - eye).dot(fwd) - p.depth_pull, i)).collect();
    // far to near; equal depths keep submission order
    order.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal).then(a.1.cmp(&b.1)));
    let mut f = FxGpuFrame { verts: Vec::with_capacity(list.len() * 6 * VERT_F32), draws: Vec::new(), depth_test: opts.depth_test, linear: opts.linear };
    let mut page_ids: HashMap<(u16, u16), AssetId<Image>> = HashMap::new();
    let white = pages.white(&mut images).id();
    for &(_, i) in &order {
        let p = &list[i];
        let page = p.page.map(|(tp, cl)| *page_ids.entry((tp, cl)).or_insert_with(|| pages.get(&vt.vram, &mut images, tp, cl).id()));
        // (pipeline, texel phase, output scale, alpha) per draw
        let mut phases: Vec<(u8, u32, f32, f32)> = Vec::with_capacity(2);
        match (p.blend, page.is_some()) {
            (Blend::Opaque, _) => phases.push((0, 0, 1.0, 1.0)),
            (Blend::Abr(a), textured) => {
                if textured {
                    phases.push((0, FLAG_OPAQUE_TEXELS, 1.0, 1.0));
                }
                let (scale, alpha) = match a & 3 {
                    0 => (1.0, 0.5),
                    3 => (0.25, 1.0),
                    _ => (1.0, 1.0),
                };
                phases.push((1 + (a & 3), if textured { FLAG_SEMI_TEXELS } else { 0 }, scale, alpha));
            }
        }
        for (pipe, phase, scale, alpha) in phases {
            let flags = phase | if page.is_some() { FLAG_TEXTURED } else { 0 };
            let first = (f.verts.len() / VERT_F32) as u32;
            for k in [0usize, 1, 2, 1, 3, 2] {
                let v = &p.v[k];
                f.verts.extend_from_slice(&[v.pos.x, v.pos.y, v.pos.z, 1.0]);
                f.verts.extend_from_slice(&[v.uv[0], v.uv[1], flags as f32, scale]);
                f.verts.extend_from_slice(&[v.rgb[0], v.rgb[1], v.rgb[2], alpha]);
                f.verts.extend_from_slice(&[p.center.x, p.center.y, p.center.z, p.ground_eps]);
                f.verts.extend_from_slice(&[p.depth_pull, 0.0, 0.0, 0.0]);
            }
            let pg = Some(page.unwrap_or(white));
            match f.draws.last_mut() {
                Some(d) if d.page == pg && d.pipe == pipe && d.first + d.count == first => d.count += 6,
                _ => f.draws.push(GpuDraw { page: pg, pipe, first, count: 6 }),
            }
        }
    }
    *out = f;
}

// ---------------------------------------------------------------------------
// Render world
// ---------------------------------------------------------------------------

#[derive(Resource)]
struct FxPipes {
    view_layout: BindGroupLayoutDescriptor,
    view_layout_ms: BindGroupLayoutDescriptor,
    page_layout: BindGroupLayoutDescriptor,
    comp_layout: BindGroupLayoutDescriptor,
    shaders: FxShaders,
}

fn init_pipes(mut commands: Commands, shaders: Res<FxShaders>) {
    let uni = uniform_buffer_sized(false, None);
    commands.insert_resource(FxPipes {
        view_layout: BindGroupLayoutDescriptor::new(
            "psxfx_view",
            &BindGroupLayoutEntries::sequential(ShaderStages::VERTEX_FRAGMENT, (uni.clone(), texture_depth_2d())),
        ),
        view_layout_ms: BindGroupLayoutDescriptor::new(
            "psxfx_view_ms",
            &BindGroupLayoutEntries::sequential(ShaderStages::VERTEX_FRAGMENT, (uni, texture_depth_2d_multisampled())),
        ),
        page_layout: BindGroupLayoutDescriptor::new(
            "psxfx_page",
            &BindGroupLayoutEntries::single(ShaderStages::FRAGMENT, texture_2d(TextureSampleType::Float { filterable: false })),
        ),
        comp_layout: BindGroupLayoutDescriptor::new(
            "psxfx_comp",
            &BindGroupLayoutEntries::single(ShaderStages::FRAGMENT, texture_2d(TextureSampleType::Float { filterable: false })),
        ),
        shaders: shaders.clone(),
    });
}

fn blend_state(pipe: u8) -> Option<BlendState> {
    let c = |src, dst, op| BlendComponent { src_factor: src, dst_factor: dst, operation: op };
    let keep_alpha = c(BlendFactor::Zero, BlendFactor::One, BlendOperation::Add);
    let color = match pipe {
        0 => return None,
        // B/2 + F/2 (alpha 0.5 from the shader)
        1 => c(BlendFactor::SrcAlpha, BlendFactor::OneMinusSrcAlpha, BlendOperation::Add),
        // B + F, B + F/4 (F scaled in the shader)
        2 | 4 => c(BlendFactor::One, BlendFactor::One, BlendOperation::Add),
        // B - F
        3 => c(BlendFactor::One, BlendFactor::One, BlendOperation::ReverseSubtract),
        _ => return None,
    };
    Some(BlendState { color, alpha: keep_alpha })
}

fn fx_descriptor(p: &FxPipes, pipe: u8, ms: bool, format: TextureFormat) -> RenderPipelineDescriptor {
    let mut defs: Vec<ShaderDefVal> = if ms { vec!["MSAA_DEPTH".into()] } else { vec![] };
    if format.is_srgb() {
        defs.push("LINEAR_OUT".into());
    }
    let attrs = (0..5u32).map(|i| VertexAttribute { format: VertexFormat::Float32x4, offset: 16 * i as u64, shader_location: i }).collect();
    RenderPipelineDescriptor {
        label: Some("psxfx".into()),
        layout: vec![if ms { p.view_layout_ms.clone() } else { p.view_layout.clone() }, p.page_layout.clone()],
        vertex: VertexState {
            shader: p.shaders.main.clone(),
            shader_defs: defs.clone(),
            entry_point: Some("vs".into()),
            buffers: vec![VertexBufferLayout { array_stride: (VERT_F32 * 4) as u64, step_mode: VertexStepMode::Vertex, attributes: attrs }],
        },
        fragment: Some(FragmentState {
            shader: p.shaders.main.clone(),
            shader_defs: defs,
            entry_point: Some("fs".into()),
            targets: vec![Some(ColorTargetState {
                format,
                blend: blend_state(pipe),
                write_mask: if pipe == 0 { ColorWrites::COLOR } else { ColorWrites::ALL },
            })],
        }),
        primitive: PrimitiveState { cull_mode: None, ..default() },
        ..default()
    }
}

fn comp_descriptor(p: &FxPipes, samples: u32, format: TextureFormat, linear: bool) -> RenderPipelineDescriptor {
    let defs: Vec<ShaderDefVal> = if linear { vec!["LINEAR_SRC".into()] } else { vec![] };
    RenderPipelineDescriptor {
        label: Some("psxfx_comp".into()),
        layout: vec![p.comp_layout.clone()],
        vertex: VertexState { shader: p.shaders.comp.clone(), shader_defs: vec![], entry_point: Some("comp_vs".into()), buffers: vec![] },
        fragment: Some(FragmentState {
            shader: p.shaders.comp.clone(),
            shader_defs: defs,
            entry_point: Some("comp_fs".into()),
            targets: vec![Some(ColorTargetState { format, blend: None, write_mask: ColorWrites::ALL })],
        }),
        multisample: MultisampleState { count: samples, ..default() },
        ..default()
    }
}

#[derive(Default)]
struct PassState {
    tex: Option<(UVec2, TextureFormat, Texture, TextureView)>,
    fx_ids: HashMap<(u8, bool, TextureFormat), CachedRenderPipelineId>,
    comp_ids: HashMap<(u32, TextureFormat, bool), CachedRenderPipelineId>,
}

#[allow(clippy::too_many_arguments)]
fn fx_pass(
    view: ViewQuery<(&ExtractedCamera, &ExtractedView, &ViewTarget, &ViewDepthTexture, &Msaa), With<PsxFxView>>,
    frame: Option<Res<FxGpuFrame>>,
    pipes: Option<Res<FxPipes>>,
    cache: Res<PipelineCache>,
    gpu_images: Res<RenderAssets<GpuImage>>,
    mut st: Local<PassState>,
    mut ctx: RenderContext,
) {
    let (Some(frame), Some(pipes)) = (frame, pipes) else { return };
    if frame.draws.is_empty() {
        return;
    }
    let (camera, ev, target, depth, msaa) = view.into_inner();
    let Some(size) = camera.physical_target_size else { return };
    let fmt = ev.target_format;
    if !matches!(fmt, TextureFormat::Rgba8UnormSrgb | TextureFormat::Bgra8UnormSrgb) {
        return;
    }
    let fx_fmt = if frame.linear { fmt } else { fmt.remove_srgb_suffix() };
    let samples = msaa.samples();
    let ms = samples > 1;
    // pipelines (queued on first use; drawn once compiled)
    let st = &mut *st;
    let mut fx_pipe = |pipe: u8| -> Option<CachedRenderPipelineId> {
        let id = *st.fx_ids.entry((pipe, ms, fx_fmt)).or_insert_with(|| cache.queue_render_pipeline(fx_descriptor(&pipes, pipe, ms, fx_fmt)));
        cache.get_render_pipeline(id).map(|_| id)
    };
    let mut ready = true;
    let mut ids = [CachedRenderPipelineId::INVALID; 5];
    for (k, id) in ids.iter_mut().enumerate() {
        match fx_pipe(k as u8) {
            Some(i) => *id = i,
            None => ready = false,
        }
    }
    let comp_id = *st.comp_ids.entry((samples, fmt, frame.linear)).or_insert_with(|| cache.queue_render_pipeline(comp_descriptor(&pipes, samples, fmt, frame.linear)));
    let Some(comp_pipe) = cache.get_render_pipeline(comp_id) else { return };
    if !ready {
        return;
    }
    let device = ctx.render_device().clone();
    // the gamma-space working copy
    if st.tex.as_ref().map(|t| (t.0, t.1)) != Some((size, fx_fmt)) {
        let tex = device.create_texture(&TextureDescriptor {
            label: Some("psxfx_frame"),
            size: Extent3d { width: size.x, height: size.y, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: fx_fmt,
            usage: TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let tv = tex.create_view(&TextureViewDescriptor::default());
        st.tex = Some((size, fx_fmt, tex, tv));
    }
    let (_, _, fx_tex, fx_view) = st.tex.as_ref().unwrap();
    // view uniform: clip_from_world, world_from_clip, viewport, params
    let clip_from_world = ev.clip_from_world.unwrap_or_else(|| ev.clip_from_view * ev.world_from_view.to_matrix().inverse());
    let world_from_clip = clip_from_world.inverse();
    let mut u: Vec<u8> = Vec::with_capacity(160);
    for m in [clip_from_world, world_from_clip] {
        for x in m.to_cols_array() {
            u.extend_from_slice(&x.to_le_bytes());
        }
    }
    let vp = ev.viewport.as_vec4();
    // infinite reverse-Z perspective: clip_from_view[3][2] is the near plane
    let near = ev.clip_from_view.w_axis.z;
    for x in [vp.x, vp.y, vp.z, vp.w, 0.6, if frame.depth_test { 1.0 } else { 0.0 }, near, 0.0] {
        u.extend_from_slice(&x.to_le_bytes());
    }
    let ubuf = device.create_buffer_with_data(&BufferInitDescriptor { label: Some("psxfx_view"), contents: &u, usage: BufferUsages::UNIFORM });
    let vbytes: Vec<u8> = frame.verts.iter().flat_map(|x| x.to_le_bytes()).collect();
    let vbuf = device.create_buffer_with_data(&BufferInitDescriptor { label: Some("psxfx_verts"), contents: &vbytes, usage: BufferUsages::VERTEX });
    let view_bg = device.create_bind_group(
        "psxfx_view",
        &cache.get_bind_group_layout(if ms { &pipes.view_layout_ms } else { &pipes.view_layout }),
        &BindGroupEntries::sequential((ubuf.as_entire_binding(), depth.view())),
    );
    let page_layout = cache.get_bind_group_layout(&pipes.page_layout);
    let mut page_bgs: HashMap<AssetId<Image>, BindGroup> = HashMap::new();
    for d in &frame.draws {
        if let Some(pg) = d.page {
            if !page_bgs.contains_key(&pg) {
                if let Some(gi) = gpu_images.get(pg) {
                    page_bgs.insert(pg, device.create_bind_group("psxfx_page", &page_layout, &BindGroupEntries::single(&gi.texture_view)));
                }
            }
        }
    }
    let comp_bg = device.create_bind_group("psxfx_comp", &cache.get_bind_group_layout(&pipes.comp_layout), &BindGroupEntries::single(fx_view));
    // 1. frame -> working copy (same bytes)
    ctx.command_encoder().copy_texture_to_texture(
        target.main_texture().as_image_copy(),
        fx_tex.as_image_copy(),
        Extent3d { width: size.x, height: size.y, depth_or_array_layers: 1 },
    );
    // 2. the primitives, in order
    {
        let mut pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
            label: Some("psxfx"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: fx_view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations { load: LoadOp::Load, store: StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let v = ev.viewport;
        pass.set_viewport(v.x as f32, v.y as f32, v.z.max(1) as f32, v.w.max(1) as f32, 0.0, 1.0);
        pass.set_vertex_buffer(0, *vbuf.slice(..));
        pass.set_bind_group(0, &*view_bg, &[]);
        for d in &frame.draws {
            let Some(bg) = d.page.and_then(|p| page_bgs.get(&p)) else { continue };
            let Some(pl) = cache.get_render_pipeline(ids[d.pipe as usize]) else { continue };
            pass.set_pipeline(pl);
            pass.set_bind_group(1, &**bg, &[]);
            pass.draw(d.first..d.first + d.count, 0..1);
        }
    }
    // 3. back into the main texture (every MSAA sample + resolve)
    {
        let mut pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
            label: Some("psxfx_comp"),
            color_attachments: &[Some(target.get_color_attachment())],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(comp_pipe);
        pass.set_bind_group(0, &*comp_bg, &[]);
        pass.draw(0..3, 0..1);
    }
}

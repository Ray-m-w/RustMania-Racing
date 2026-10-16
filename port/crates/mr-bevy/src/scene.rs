//! Scene construction from the owner's disc: VRAM (DOCK.RAW), track geometry
//! and the kart model. Track/kart meshes are filled in from mr-assets'
//! DAT/MDL decoders; until then the kart is a placeholder box.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use mr_assets::axe::Axe;
use mr_assets::dat::{self, Dat, SinTable};
use mr_assets::mdl::{self, Mdl, SlotAttr};
use mr_assets::{raw, ter::Ter, vram::Vram};

use crate::psxmesh::{self, PsxPoly};
use bevy::camera::visibility::RenderLayers;

/// Render layer of the far pass (sky + DAT layer 1), drawn by a camera that
/// renders before the main one; the main camera then clears depth only.
pub const FAR_LAYER: usize = 1;

/// Marker for the far-pass camera (child of the chase camera).
#[derive(Component)]
pub struct FarCamera;

/// Make the chase camera draw over a far pass: spawn the far camera as its
/// child (same projection, order -1, FAR_LAYER only) and set the main
/// camera to keep the colour buffer. Runs once (called from select_objects).
pub fn setup_far_pass(commands: &mut Commands, main: Entity, cam: &mut Camera, proj: &Projection, msaa: Msaa) {
    cam.order = 1;
    cam.clear_color = ClearColorConfig::None;
    commands.entity(main).with_children(|c| {
        c.spawn((
            Camera3d::default(),
            Camera { order: 0, ..default() },
            proj.clone(),
            bevy::core_pipeline::tonemapping::Tonemapping::None,
            Transform::default(),
            RenderLayers::layer(FAR_LAYER),
            // same sample count as the main camera: they share the target
            msaa,
            FarCamera,
        ));
    });
}

use crate::ghost::GhostKart;
use crate::vramtex::VramTextures;
use crate::PlayerKart;

/// The frame clear colour on DOCK (GP0 0x02 fill at the start of each race
/// frame: 0x025F1300 = rgb 0, 19, 95) [GP0, analysis/s5/fx/race3e/gpu] =
/// DOCK's course-row fog colour; courses::clear_rgb gives every course's.
#[allow(dead_code)]
pub const DOCK_CLEAR: [u8; 3] = [0, 0x13, 0x5F];

/// Decoded course VRAM + the texture slot table (0x800DFF00) models use.
pub fn load_vram(disc: &mut mr_disc::Disc, exe: &[u8], course: u32) -> (Vram, [Option<SlotAttr>; 32], [Option<dat::TexSlot>; 32]) {
    let path = raw::course_raw_path(exe, course).expect("course name");
    let file = disc.read(&path).expect("read course RAW");
    let env = raw::LoadEnv::from_exe(exe, course).expect("course env");
    let dec = raw::decode(&file, &env).expect("decode RAW");
    let mut v = Vram::default();
    dec.apply(&mut v);
    let mut slots = [None; 32];
    let mut dslots = [None; 32];
    for (k, s) in dec.slots.iter().enumerate() {
        if let Some(s) = s {
            if let Some(tp) = s.tpage {
                slots[k] = Some(SlotAttr { tpage: tp, clut: s.clut });
                // slot +8 = page mode: nonzero (8-bit) -> one CLUT per page
                dslots[k] = Some(dat::TexSlot { tpage: tp, clut: s.clut, fixed_clut: s.mode.unwrap_or(0) });
            }
        }
    }
    (v, slots, dslots)
}

/// Model polygons in model units (x right, y down, z forward; world = model * 32),
/// at the rest pose (vertex + part pivot), in GPU vertex order.
pub fn model_polys(m: &Mdl, slots: &[Option<SlotAttr>; 32]) -> Vec<PsxPoly> {
    let mut out = Vec::new();
    for (_, part) in m.meshes() {
        let flag = part.tex_flag().unwrap_or(0);
        let slot = slots[(flag & 0x1F) as usize];
        for poly in part.polys() {
            let ord = poly.gpu_order();
            let pv = part.record.pivot;
            out.push(PsxPoly {
                pos: ord.iter().map(|&i| {
                    let p = poly.verts[i].pos;
                    [p[0] as i32 + pv[0], p[1] as i32 + pv[1], p[2] as i32 + pv[2]]
                }).collect(),
                uv: ord.iter().map(|&i| poly.verts[i].uv).collect(),
                rgb: ord.iter().map(|&i| poly.colors.get(i).copied().unwrap_or([128, 128, 128])).collect(),
                tex: slot.map(|s| mdl::tpage_clut(flag, s, poly)),
                semi: poly.semi_transparent(),
                abr: (flag >> 5) & 3,
                cull: !poly.double_sided(),
            });
        }
    }
    out
}

/// One mesh part's polygons in part-local model units (no pivot), GPU order.
pub fn part_polys(part: &mdl::Part, slots: &[Option<SlotAttr>; 32]) -> Vec<PsxPoly> {
    let flag = part.tex_flag().unwrap_or(0);
    let slot = slots[(flag & 0x1F) as usize];
    part.polys()
        .iter()
        .map(|poly| {
            let ord = poly.gpu_order();
            PsxPoly {
                pos: ord.iter().map(|&i| poly.verts[i].pos.map(|c| c as i32)).collect(),
                uv: ord.iter().map(|&i| poly.verts[i].uv).collect(),
                rgb: ord.iter().map(|&i| poly.colors.get(i).copied().unwrap_or([128, 128, 128])).collect(),
                tex: slot.map(|s| mdl::tpage_clut(flag, s, poly)),
                semi: poly.semi_transparent(),
                abr: (flag >> 5) & 3,
                cull: !poly.double_sided(),
            }
        })
        .collect()
}

pub fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut pmats: ResMut<Assets<crate::scenepush::PushMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let mut disc = mr_disc::Disc::open_default().expect("open the disc (MRM_DISC or game.toml)");
    let exe = disc.read("SLUS_012.37").expect("read the executable");
    // The course of this run (courses.rs: --course, default DOCK).
    let course = crate::courses::info();
    let (vram, slots, dslots) = load_vram(&mut disc, &exe, course.id as u32);
    let mut tex = VramTextures::new(vram);

    // Track scenery: every DAT object, world = origin + (M v) >> 12 (x4 for
    // the quarter-unit layer), exactly the game's integer transform.
    let dat_bytes = disc.read(&course.file("DAT")).expect("read DAT");
    let dat = Dat::parse(&dat_bytes).expect("parse DAT");
    // phase 6: the challenge objects' points (chunk 0x0C), for their slots
    let challenge_points = mr_sim::items::pickups::CoursePickups::from_dat(&dat, &dat_bytes).challenge.map(|l| l.points).unwrap_or_default();
    let sin = SinTable::from_exe(&exe).expect("sin table");
    // One entity per DAT object (TrackObject), so per-frame object selection
    // (the game's visibility stream / LOD) can show and hide them.
    let textured_only = std::env::args().any(|a| a == "--textured-only");
    let world = |p: [i32; 3]| crate::to_bevy(p[0] * 32, p[1] * 32, p[2] * 32);
    let mut cache = psxmesh::MatCache::default();
    let mut ground_cache = psxmesh::MatCache(Default::default(), -2000.0);
    let mut far_cache = psxmesh::MatCache(Default::default(), 0.0);
    let info = classify_layers(&dat, &sin, &dslots);
    let course_bias = dat::course_depth_bias(&exe, course.id as usize).unwrap_or(0);
    let mut push_cache = crate::scenepush::PushCache::default();
    let draw = dat::DrawParams::course_flags(&exe, course.id as usize).map(dat::DrawParams::race_1p).unwrap_or(dat::DrawParams::DOCK_1P);
    if let Some(path) = crate::settings::arg_value("--layer-report") {
        let _ = std::fs::write(&path, layer_report(&dat, &sin, &dslots, &info));
    }
    for (i, o) in dat.objects.iter().enumerate() {
        // special-slot scenery the race code moves / hides (movers.rs):
        // built in mesh space, placed by its pose every frame
        let mover = crate::movers::mover_slot(&dat, i).map(|s| crate::movers::mover(&dat, i, s)).or_else(|| crate::movers::challenge_mover(&dat, i, &challenge_points));
        let mut polys = if mover.is_some() { crate::movers::local_polys(&dat, o, &sin, &dslots) } else { object_polys(&dat, o, &sin, &dslots) };
        if textured_only {
            polys.retain(|p| p.tex.is_some());
        }
        if polys.is_empty() {
            // kind 2/3 parts models: camera-facing quads (parts.rs)
            let far = i >= dat.layer0_count;
            let push = if !far && crate::scenepush::enabled() { crate::scenepush::push_units(dat::lod_bias(course_bias, dat.desc(o).b_lo)) } else { 0.0 };
            if let Some(p) = crate::parts::build(&dat, o, &sin, &dslots, push) {
                commands.spawn((Transform::default(), Visibility::default(), crate::TrackObject(i), p));
            }
            continue;
        }
        // FUN_80010000 puts layer-0 objects with flag 0x40/0x80 on the
        // "ground" list, which the game's ordering table draws under the
        // near list (DAT.md "Per-frame object selection").
        // Layer 1 (the far stand-ins, drawn by FUN_80029F40 before layer 0)
        // sits under everything else.
        // Layer 1 (the far stand-ins) goes to its own render layer, drawn by
        // FAR_CAMERA before everything else: the game sorts layer 1 into
        // ordering-table offsets >= 4000 (table 0x800BA758, FUN_8001a304)
        // while layer 0 uses 0..~3600 out to its LOD-2 range (table
        // 0x800CB788), so every layer-0 polygon is painted over layer 1
        // regardless of depth (PS1 has no depth buffer).
        let far = i >= dat.layer0_count;
        let ground = !far && o.loaded_flags() & 0xC0 != 0;
        let c = if far { &mut far_cache } else if ground { &mut ground_cache } else { &mut cache };
        let parts = psxmesh::build_cached(&polys, &mut tex, &mut images, &mut meshes, &mut mats, c, world);
        // Semi-transparent polygons (water, glows): the effects pass, in the
        // far camera's pass for layer 1, under everything for the ground list.
        let view = if far { crate::psxfx::VIEW_FAR } else { crate::psxfx::VIEW_MAIN };
        // Layer 0: the object's ordering-table bias as a depth push
        // (scenepush.rs; the ground list a hair further, as its old depth bias)
        let push = (!far && crate::scenepush::enabled())
            .then(|| crate::scenepush::push_units(dat::lod_bias(course_bias, dat.desc(o).b_lo)) + if ground { 0.02 } else { 0.0 });
        // faithful draw distance: the layer-0 renderer's per-polygon far cut
        let far_cut = if crate::settings::Settings::global().faithful_draw_distance() && !crate::title::on_title() {
            crate::scenepush::far_cut(o.loaded_flags(), &draw)
        } else {
            0.0
        };
        let semi = psxmesh::semi_prims_pulled(&polys, view, if ground { -1 } else { 0 }, 0.01 - push.unwrap_or(0.0), world);
        let parts: Vec<_> = parts
            .into_iter()
            .map(|(mesh, mat)| (mesh, push.map(|u| push_cache.get(&mats, &mut pmats, &mat, u, far_cut)).ok_or(mat)))
            .collect();
        let mut e = commands.spawn((Transform::default(), Visibility::default(), crate::TrackObject(i)));
        if let Some(m) = mover {
            e.insert((crate::movers::pose_transform(&m, None).unwrap_or_default(), m));
        }
        e.with_children(|c| {
            for (mesh, mat) in parts {
                match mat {
                    Ok(pm) => {
                        c.spawn((Mesh3d(mesh), MeshMaterial3d(pm)));
                    }
                    Err(mat) if far => {
                        c.spawn((Mesh3d(mesh), MeshMaterial3d(mat), RenderLayers::layer(FAR_LAYER)));
                    }
                    Err(mat) => {
                        c.spawn((Mesh3d(mesh), MeshMaterial3d(mat)));
                    }
                }
            }
            if let Some(s) = semi {
                c.spawn((Transform::default(), Visibility::default(), s));
            }
        });
    }

    // The waving start/finish banner (DAT chunk id 9, FUN_800351e8): an
    // animated cloth, rebuilt every tick by update_banners.
    if let Some(b) = dat.banners() {
        if let Some(slot) = dslots[dat::BANNER_TEX_SLOT as usize] {
            let tex_key = (slot.tpage.wrapping_add(dat::BANNER_TPAGE_ADD), slot.clut.wrapping_add(dat::BANNER_CLUT_ADD));
            let mut mesh_handles = Vec::new();
            let mut cache = psxmesh::MatCache::default();
            for i in 0..b.banners.len() {
                let polys = banner_polys(&b, i, &sin, BANNER_PHASE_ROW0, tex_key);
                let parts = psxmesh::build_cached(&polys, &mut tex, &mut images, &mut meshes, &mut mats, &mut cache, world);
                for (mesh, mat) in parts {
                    commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat), Transform::default(), Visibility::Hidden, BannerMesh(i)));
                    mesh_handles.push((i, mesh));
                }
            }
            commands.insert_resource(BannerState { banners: b, sin: SinTable(sin.0.clone()), tex_key, meshes: mesh_handles });
        }
    }

    // Kart shadows (kartshadow.rs): texture slot 0x14.
    if let Some(slot) = dslots[crate::kartshadow::SHADOW_SLOT as usize] {
        commands.insert_resource(crate::kartshadow::ShadowState::new(SinTable(sin.0.clone()), slot));
    }

    commands.insert_resource(crate::TrackVis { dat, zone: 0, info, draw });

    // Sky dome (DOCK.AXE): gouraud quads drawn around the eye with zero
    // translation (FUN_80034AE4), i.e. directions only. Rendered as a big
    // shell that follows the camera (crate::follow_sky).
    if let Ok(axe) = disc.read(&course.file("AXE")).map(|b| Axe::parse(&b)) {
        let axe = axe.expect("parse AXE");
        let polys: Vec<PsxPoly> = axe
            .drawn_quads()
            .iter()
            .map(|q| PsxPoly {
                pos: q.iter().map(|(p, _)| {
                    let v = Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32).normalize_or_zero() * 1_000_000.0;
                    [v.x as i32, v.y as i32, v.z as i32]
                }).collect(),
                uv: vec![[0, 0]; 4],
                rgb: q.iter().map(|(_, c)| *c).collect(),
                tex: None,
                semi: false,
                abr: 0,
                cull: false,
            })
            .collect();
        // Horizon colour for the enhanced look's haze (lighting.rs): the
        // dome's vertices within ~10 degrees of the horizon.
        let (mut acc, mut n) = (Vec3::ZERO, 0.0);
        for q in axe.drawn_quads() {
            for (p, c) in q.iter() {
                let d = Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32).normalize_or_zero();
                if d.y.abs() < 0.17 {
                    let l = Color::srgb_u8(c[0], c[1], c[2]).to_linear();
                    acc += Vec3::new(l.red, l.green, l.blue);
                    n += 1.0;
                }
            }
        }
        if n > 0.0 {
            let a = acc / n;
            commands.insert_resource(crate::lighting::SkyHorizon(Color::linear_rgb(a.x, a.y, a.z)));
        }
        let sky = |p: [i32; 3]| Vec3::new(p[0] as f32, -(p[1] as f32), -(p[2] as f32)) / 1000.0;
        commands.spawn((Transform::default(), Visibility::default(), crate::Sky)).with_children(|c| {
            // The sky sits in one ordering-table slot behind everything: draw
            // it with the far pass.
            for (mesh, mat) in psxmesh::build(&polys, &mut tex, &mut images, &mut meshes, &mut mats, sky) {
                c.spawn((Mesh3d(mesh), MeshMaterial3d(mat), RenderLayers::layer(FAR_LAYER)));
            }
        });
    }

    // Debug: the collision surface (DOCK.TER), flat shaded (--show-ter).
    let ter = Ter::parse(&disc.read(&course.file("TER")).expect("read TER")).expect("parse TER");
    if std::env::args().any(|a| a == "--show-ter") { commands.spawn((
        Mesh3d(meshes.add(ter_mesh(&ter))),
        MeshMaterial3d(mats.add(StandardMaterial {
            base_color: Color::WHITE,
            cull_mode: None,
            double_sided: true,
            perceptual_roughness: 1.0,
            ..default()
        })),
        Transform::default(),
    )); }

    // Karts: one rig per fed kart, any character / vehicle (crate::karts).
    commands.insert_resource(crate::karts::ModelLib::new(
        mr_disc::Disc::open_default().expect("open the disc"),
        exe.clone(),
        slots,
    ));
    let racer = Mdl::parse(&disc.read("VEHIC_A/RACER.MDL").expect("read MDL")).expect("parse MDL");
    let kermit = Mdl::parse(&disc.read("CHARS_A/KERMIT.MDL").expect("read MDL")).expect("parse MDL");
    commands.spawn((Transform::default(), Visibility::default(), PlayerKart));

    // The ghost stays one rigid rest-pose mesh.
    let mut polys = model_polys(&racer, &slots);
    polys.extend(model_polys(&kermit, &slots));
    let model_to_bevy = |p: [i32; 3]| crate::to_bevy(p[0] * 32, p[1] * 32, p[2] * 32);
    let parts = psxmesh::build(&polys, &mut tex, &mut images, &mut meshes, &mut mats, model_to_bevy);
    let ghost_mat = mats.add(StandardMaterial {
        base_color: Color::srgba(0.4, 0.7, 1.0, 0.35),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    commands
        .spawn((Transform::default(), Visibility::Hidden, GhostKart))
        .with_children(|c| {
            for (mesh, _) in &parts {
                c.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(ghost_mat.clone())));
            }
        });
    commands.insert_resource(tex);
}

/// TER floor faces as a flat-shaded mesh (TER units are world units >> 5).
pub fn ter_mesh(ter: &Ter) -> Mesh {
    let mut pos = Vec::new();
    let mut col = Vec::new();
    let mut nrm = Vec::new();
    for f in &ter.faces {
        if f.disabled() {
            continue;
        }
        let c = f.corners();
        let tris: &[[usize; 3]] = if f.is_quad() { &[[0, 1, 2], [0, 2, 3]] } else { &[[0, 1, 2]] };
        let k = f.kind as f32;
        let rgb = if f.kind == mr_assets::ter::KIND_WATER {
            [0.2, 0.35, 0.8, 1.0]
        } else {
            [0.45 + 0.03 * (k % 7.0), 0.33 + 0.02 * (k % 5.0), 0.2, 1.0]
        };
        for t in tris {
            let p: Vec<Vec3> = t
                .iter()
                .map(|&i| {
                    let v = ter.vertices[c[i] as usize];
                    crate::to_bevy(v.x * 32, v.y * 32, v.z * 32)
                })
                .collect();
            let n = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
            for q in p {
                pos.push(q.to_array());
                nrm.push(n.to_array());
                col.push(rgb);
            }
        }
    }
    let n = pos.len() as u32;
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nrm)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
        .with_inserted_indices(Indices::U32((0..n).collect()))
}

/// One DAT object's polygons in DAT world units (= TER units = kart units >> 5),
/// GPU vertex order: world = origin + (M v) >> 12 (x4 for the quarter layer),
/// exactly the game's integer transform.
pub fn object_polys(dat: &Dat, o: &dat::Object, sin: &SinTable, slots: &[Option<dat::TexSlot>; 32]) -> Vec<PsxPoly> {
    let mut out = Vec::new();
    let Some(mesh) = dat.mesh(o) else { return out };
    let t = dat.transform(o, sin);
    for g in &mesh.groups {
        let slot = if g.is_textured() { slots[g.tex_slot() as usize] } else { None };
        let n = if g.is_triangle() { 3 } else { 4 };
        let ord: &[usize] = if n == 4 { &[0, 1, 3, 2] } else { &[0, 1, 2] };
        for p in &g.prims {
            out.push(PsxPoly {
                pos: ord.iter().map(|&k| t.apply(mesh.vertices[p.idx[k] as usize].pos)).collect(),
                uv: ord.iter().map(|&k| p.uv[k]).collect(),
                rgb: ord.iter().map(|&k| mesh.vertices[p.idx[k] as usize].rgb()).collect(),
                tex: slot.map(|s| dat::prim_tpage_clut(s, g, p)),
                semi: g.semi_transparent(),
                abr: g.blend_bits(),
                cull: !g.double_sided(),
            });
        }
    }
    out
}


// ---------------------------------------------------------------------------
// Start/finish banner (DAT.md "Banner")
// ---------------------------------------------------------------------------

/// Cloth phase (DAT_800afee4, DAT_800afee6) at trace row 0 of the lap1
/// grid savestate: measured on the oracle (row 506 = (3171, 102)), stepping
/// (0x37, 0x3E) per race frame. The game never resets it, so it depends on
/// the power-on history; any value is a valid "look".
pub const BANNER_PHASE_ROW0: (u16, u16) = (4013, 1498);

#[derive(Component)]
pub struct BannerMesh(pub usize);

#[derive(Resource)]
pub struct BannerState {
    pub banners: dat::Banners,
    pub sin: SinTable,
    pub tex_key: (u16, u16),
    pub meshes: Vec<(usize, Handle<Mesh>)>,
}

/// Banner `i` as PS1 polygons (GPU vertex order 0, 1, 3, 2) for `row0` + ticks.
pub fn banner_polys(b: &dat::Banners, i: usize, sin: &SinTable, phase: (u16, u16), tex: (u16, u16)) -> Vec<PsxPoly> {
    let cloth = b.cloth(sin, phase);
    b.quads(i, &cloth, sin)
        .iter()
        .map(|q| {
            let ord = [0usize, 1, 3, 2];
            PsxPoly {
                pos: ord.iter().map(|&k| q.pos[k]).collect(),
                uv: ord.iter().map(|&k| q.uv[k]).collect(),
                rgb: ord.iter().map(|&k| [q.shade[k]; 3]).collect(),
                tex: Some(tex),
                semi: false,
                abr: 0,
                cull: false,
            }
        })
        .collect()
}

/// Per frame: animate the cloth for the current sim tick (phase = row-0
/// phase + tick * (0x37, 0x3E)) and show each banner only when its midpoint
/// is within 0x2000 DAT units of the eye on every axis (FUN_800351e8).
pub fn update_banners(
    p: Res<crate::Player>,
    settings: Res<crate::settings::Settings>,
    st: Option<ResMut<BannerState>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut q: Query<(&BannerMesh, &mut Visibility)>,
) {
    // (no banner on the frontend course: the title, title.rs)
    let Some(mut st) = st else { return };
    if !p.is_changed() {
        return;
    }
    let row = (p.tick - 1).max(0) as u32;
    let phase = dat::Banners::phase_after(BANNER_PHASE_ROW0, row);
    let world = |v: [i32; 3]| crate::to_bevy(v[0] * 32, v[1] * 32, v[2] * 32);
    let st = &mut *st;
    for (i, handle) in st.meshes.clone() {
        let polys = banner_polys(&st.banners, i, &st.sin, phase, st.tex_key);
        // the cloth's mesh rewritten in place (one texture: one mesh); no
        // temporary assets (a mesh added and removed in one frame tripped
        // bevy_render's mesh allocator: "use-after-free" errors)
        for (_, m) in psxmesh::meshes_by_key(&polys, world) {
            if let Some(mut dst) = meshes.get_mut(&handle) {
                *dst = m;
            }
        }
    }
    let eye = p.pose.cam.map(|(e, _)| [e[0] / 32, e[1] / 32, e[2] / 32]);
    let full = !settings.faithful_draw_distance();
    for (b, mut v) in q.iter_mut() {
        let show = full || eye.is_some_and(|e| st.banners.visible(b.0, e));
        let want = if show { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
    }
}

/// Track objects outside the DAT scenery lists (the banner) - registered with
/// one line in main.rs.
pub struct ObjectsPlugin;

impl Plugin for ObjectsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (update_banners.before(crate::shot), crate::kartshadow::update_shadows.before(crate::shot)));
    }
}

// ---------------------------------------------------------------------------
// Draw distance (Session 5): which objects the full-course view draws
// ---------------------------------------------------------------------------

/// Per-object facts the full draw-distance mode needs, from the data alone.
pub struct LayerInfo {
    /// Layer-1 objects that are real backdrop: most of their surface lies
    /// away from every layer-0 surface (not a coarse copy of near scenery).
    pub backdrop: Vec<bool>,
    /// Fraction of each layer-1 object's area within `COVER_R` DAT units of a
    /// layer-0 surface (layer-0 objects: 1.0).
    pub coverage: Vec<f32>,
    /// The visibility stream sets this object's bit at some node of some
    /// zone (or the track has no zone).
    pub ever_visible: Vec<bool>,
}

/// Sample spacing and the "same place" radius, DAT layer-0 units (world >> 5).
pub const COVER_STEP: f32 = 96.0;
pub const COVER_R: f32 = 192.0;
/// A layer-1 object is kept as backdrop when less than this fraction of its
/// area has layer-0 scenery nearby.
pub const BACKDROP_MAX_COVER: f32 = 0.5;

/// Area-weighted sample points of a polygon list (DAT units), each with its weight.
fn surface_samples(polys: &[PsxPoly], step: f32, out: &mut Vec<(Vec3, f32)>) {
    for p in polys {
        let v: Vec<Vec3> = p.pos.iter().map(|q| Vec3::new(q[0] as f32, q[1] as f32, q[2] as f32)).collect();
        let tris: &[[usize; 3]] = if v.len() == 4 { &[[0, 1, 2], [1, 3, 2]] } else { &[[0, 1, 2]] };
        for t in tris {
            let (a, b, c) = (v[t[0]], v[t[1]], v[t[2]]);
            let area = 0.5 * (b - a).cross(c - a).length();
            let len = (b - a).length().max((c - a).length()).max((c - b).length());
            let n = ((len / step).ceil() as usize).clamp(1, 64);
            let w = area / (n * n) as f32;
            for i in 0..n {
                for j in 0..n - i {
                    // centre of sub-triangle (i, j) of an n x n split
                    let (s, r) = ((i as f32 + 1.0 / 3.0) / n as f32, (j as f32 + 1.0 / 3.0) / n as f32);
                    out.push((a + (b - a) * s + (c - a) * r, w));
                }
            }
        }
    }
}

/// Drawn in the full draw-distance mode: every layer-0 object the game can
/// show (its visibility bit is set at some node, kind != 5), except the
/// special-slot objects (the item code moves those at run time; they sit
/// parked off the course otherwise), plus the layer-1 objects that are real
/// backdrop. Layer-1 stand-ins of layer-0 scenery are dropped.
pub fn full_view_object(dat: &Dat, info: &LayerInfo, i: usize) -> bool {
    // the pickups' slots (effects.rs draws the items); the other special
    // slots are course scenery (movers.rs)
    // (phase 6: the challenge objects' slots are movers, shown while
    // FUN_8002DC14 scales them: ADVENTURE)
    if dat.special.iter().enumerate().any(|(s, o)| *o == Some(i) && crate::movers::pickup_slot(dat, s)) {
        return true;
    }
    if i >= dat.layer0_count {
        return info.backdrop[i];
    }
    info.ever_visible[i] && dat.objects[i].loaded_flags() != 5
}

pub fn classify_layers(dat: &Dat, sin: &SinTable, slots: &[Option<dat::TexSlot>; 32]) -> LayerInfo {
    use std::collections::HashSet;
    let n = dat.objects.len();
    let cell = |p: Vec3| ((p.x / COVER_R).floor() as i32, (p.y / COVER_R).floor() as i32, (p.z / COVER_R).floor() as i32);
    let mut grid: HashSet<(i32, i32, i32)> = HashSet::new();
    let mut pts = Vec::new();
    for o in &dat.objects[..dat.layer0_count] {
        pts.clear();
        surface_samples(&object_polys(dat, o, sin, slots), COVER_STEP, &mut pts);
        grid.extend(pts.iter().map(|(p, _)| cell(*p)));
    }
    let mut coverage = vec![1.0f32; n];
    let mut backdrop = vec![false; n];
    for i in dat.layer0_count..n {
        pts.clear();
        surface_samples(&object_polys(dat, &dat.objects[i], sin, slots), COVER_STEP, &mut pts);
        let (mut near, mut all) = (0.0f32, 0.0f32);
        for (p, w) in &pts {
            let (cx, cy, cz) = cell(*p);
            // any layer-0 sample in the 3x3x3 block: within ~COVER_R..2*COVER_R
            let hit = (-1..=1).any(|dx| (-1..=1).any(|dy| (-1..=1).any(|dz| grid.contains(&(cx + dx, cy + dy, cz + dz)))));
            all += w;
            if hit {
                near += w;
            }
        }
        coverage[i] = if all > 0.0 { near / all } else { 1.0 };
        backdrop[i] = coverage[i] < BACKDROP_MAX_COVER;
    }
    let mut ever_visible = vec![dat.vis.is_empty(); n];
    for z in &dat.vis {
        for node in 0..=z.separators().len() {
            let m = z.mask_at(node);
            for (i, e) in ever_visible.iter_mut().enumerate() {
                let (byte, bit) = if i < dat.layer0_count { (i / 8, i % 8) } else { (255 - (i - dat.layer0_count) / 8, (i - dat.layer0_count) % 8) };
                if m.get(byte).is_some_and(|b| b & (0x80 >> bit) != 0) {
                    *e = true;
                }
            }
        }
    }
    LayerInfo { backdrop, coverage, ever_visible }
}

/// Text table of the layer classification (`--layer-report PATH`).
pub fn layer_report(dat: &Dat, sin: &SinTable, slots: &[Option<dat::TexSlot>; 32], info: &LayerInfo) -> String {
    let mut s = String::from("# object layer flags textured/polys centre(layer units) radius coverage backdrop ever_visible
");
    for (i, o) in dat.objects.iter().enumerate() {
        let polys = object_polys(dat, o, sin, slots);
        let tex = polys.iter().filter(|p| p.tex.is_some()).count();
        s += &format!(
            "{i} {} {:#04x} {tex}/{} {:?} {} {:.2} {} {}
",
            if i < dat.layer0_count { 0 } else { 1 },
            o.loaded_flags(),
            polys.len(),
            o.centre,
            o.radius,
            info.coverage[i],
            info.backdrop[i],
            info.ever_visible[i]
        );
    }
    s
}

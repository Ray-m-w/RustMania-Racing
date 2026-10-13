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
pub fn setup_far_pass(commands: &mut Commands, main: Entity, cam: &mut Camera, proj: &Projection) {
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
            FarCamera,
        ));
    });
}

use crate::ghost::GhostKart;
use crate::vramtex::VramTextures;
use crate::PlayerKart;

/// Course index of DOCK in the game's course table (0x800ACAA0; RAW.md).
pub const DOCK: u32 = 3;

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
            }
        })
        .collect()
}

pub fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let mut disc = mr_disc::Disc::open_default().expect("open the disc (MRM_DISC or game.toml)");
    let exe = disc.read("SLUS_012.37").expect("read the executable");
    let (vram, slots, dslots) = load_vram(&mut disc, &exe, DOCK);
    let mut tex = VramTextures::new(vram);

    // Track scenery: every DAT object, world = origin + (M v) >> 12 (x4 for
    // the quarter-unit layer), exactly the game's integer transform.
    let dat = Dat::parse(&disc.read("COURSE_A/DOCK.DAT").expect("read DAT")).expect("parse DAT");
    let sin = SinTable::from_exe(&exe).expect("sin table");
    // One entity per DAT object (TrackObject), so per-frame object selection
    // (the game's visibility stream / LOD) can show and hide them.
    let textured_only = std::env::args().any(|a| a == "--textured-only");
    let world = |p: [i32; 3]| crate::to_bevy(p[0] * 32, p[1] * 32, p[2] * 32);
    let mut cache = psxmesh::MatCache::default();
    let mut ground_cache = psxmesh::MatCache(Default::default(), -2000.0);
    let mut far_cache = psxmesh::MatCache(Default::default(), 0.0);
    for (i, o) in dat.objects.iter().enumerate() {
        let mut polys = object_polys(&dat, o, &sin, &dslots);
        if textured_only {
            polys.retain(|p| p.tex.is_some());
        }
        if polys.is_empty() {
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
        commands
            .spawn((Transform::default(), Visibility::default(), crate::TrackObject(i)))
            .with_children(|c| {
                for (mesh, mat) in parts {
                    if far {
                        c.spawn((Mesh3d(mesh), MeshMaterial3d(mat), RenderLayers::layer(FAR_LAYER)));
                    } else {
                        c.spawn((Mesh3d(mesh), MeshMaterial3d(mat)));
                    }
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
            commands.insert_resource(BannerState { banners: b, sin: SinTable(sin.0.clone()), tex_key, cache, meshes: mesh_handles });
        }
    }

    // Kart shadows (kartshadow.rs): texture slot 0x14.
    if let Some(slot) = dslots[crate::kartshadow::SHADOW_SLOT as usize] {
        commands.insert_resource(crate::kartshadow::ShadowState::new(SinTable(sin.0.clone()), slot));
    }

    commands.insert_resource(crate::TrackVis { dat, zone: 0 });

    // Sky dome (DOCK.AXE): gouraud quads drawn around the eye with zero
    // translation (FUN_80034AE4), i.e. directions only. Rendered as a big
    // shell that follows the camera (crate::follow_sky).
    if let Ok(axe) = disc.read("COURSE_A/DOCK.AXE").map(|b| Axe::parse(&b)) {
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
            })
            .collect();
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
    let ter = Ter::parse(&disc.read("COURSE_A/DOCK.TER").expect("read TER")).expect("parse TER");
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
    pub cache: psxmesh::MatCache,
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
            }
        })
        .collect()
}

/// Per frame: animate the cloth for the current sim tick (phase = row-0
/// phase + tick * (0x37, 0x3E)) and show each banner only when its midpoint
/// is within 0x2000 DAT units of the eye on every axis (FUN_800351e8).
pub fn update_banners(
    p: Res<crate::Player>,
    mut st: ResMut<BannerState>,
    mut tex: ResMut<VramTextures>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut q: Query<(&BannerMesh, &mut Visibility)>,
) {
    if !p.is_changed() {
        return;
    }
    let row = (p.tick - 1).max(0) as u32;
    let phase = dat::Banners::phase_after(BANNER_PHASE_ROW0, row);
    let world = |v: [i32; 3]| crate::to_bevy(v[0] * 32, v[1] * 32, v[2] * 32);
    let st = &mut *st;
    for (i, handle) in st.meshes.clone() {
        let polys = banner_polys(&st.banners, i, &st.sin, phase, st.tex_key);
        let parts = psxmesh::build_cached(&polys, &mut tex, &mut images, &mut meshes, &mut mats, &mut st.cache, world);
        for (tmp, _) in parts {
            if let Some(m) = meshes.remove(&tmp) {
                let _ = meshes.insert(&handle, m);
            }
        }
    }
    let eye = p.pose.cam.map(|(e, _)| [e[0] / 32, e[1] / 32, e[2] / 32]);
    for (b, mut v) in q.iter_mut() {
        let show = eye.is_some_and(|e| st.banners.visible(b.0, e));
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

//! Scene construction from the owner's disc: VRAM (DOCK.RAW), track geometry
//! and the kart model. Track/kart meshes are filled in from mr-assets'
//! DAT/MDL decoders; until then the kart is a placeholder box.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use mr_assets::dat::{self, Dat, SinTable};
use mr_assets::mdl::{self, Mdl, SlotAttr};
use mr_assets::{raw, ter::Ter, vram::Vram};

use crate::psxmesh::{self, PsxPoly};

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
    let tpolys = track_polys(&dat, &sin, &dslots);
    if std::env::args().any(|a| a == "--debug-track") {
        let mut lo = [i32::MAX; 3];
        let mut hi = [i32::MIN; 3];
        for p in &tpolys {
            for v in &p.pos {
                for k in 0..3 {
                    lo[k] = lo[k].min(v[k]);
                    hi[k] = hi[k].max(v[k]);
                }
            }
        }
        let tex = tpolys.iter().filter(|p| p.tex.is_some()).count();
        eprintln!("track: {} polys ({} textured), bbox {:?}..{:?}", tpolys.len(), tex, lo, hi);
    }
    // DAT world units = TER units = kart position units >> 5 (layer-1 extents
    // equal TER vertex extents; examples/units.rs)
    let world = |p: [i32; 3]| crate::to_bevy(p[0] * 32, p[1] * 32, p[2] * 32);
    for (mesh, mat) in psxmesh::build(&tpolys, &mut tex, &mut images, &mut meshes, &mut mats, world) {
        commands.spawn((Mesh3d(mesh), MeshMaterial3d(mat), Transform::default()));
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

    // Kermit in the RACER: both models share the kart's transform; model
    // units are world units / 32. [MDL.md; per-part matrices (wheel spin,
    // steering, the driver's motion) are not animated yet.]
    let mut polys = Vec::new();
    for path in ["VEHIC_A/RACER.MDL", "CHARS_A/KERMIT.MDL"] {
        let m = Mdl::parse(&disc.read(path).expect("read MDL")).expect("parse MDL");
        polys.extend(model_polys(&m, &slots));
    }
    let model_to_bevy = |p: [i32; 3]| crate::to_bevy(p[0] * 32, p[1] * 32, p[2] * 32);
    let parts = psxmesh::build(&polys, &mut tex, &mut images, &mut meshes, &mut mats, model_to_bevy);
    let ghost_mat = mats.add(StandardMaterial {
        base_color: Color::srgba(0.4, 0.7, 1.0, 0.35),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    commands
        .spawn((Transform::default(), Visibility::default(), PlayerKart))
        .with_children(|c| {
            for (mesh, mat) in &parts {
                c.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone())));
            }
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

/// All DAT objects as world-space PS1 polygons (GPU vertex order).
pub fn track_polys(dat: &Dat, sin: &SinTable, slots: &[Option<dat::TexSlot>; 32]) -> Vec<PsxPoly> {
    let mut out = Vec::new();
    for o in &dat.objects {
        let Some(mesh) = dat.mesh(o) else { continue };
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
    }
    out
}

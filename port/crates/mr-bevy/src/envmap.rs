//! The DAT's environment-mapped primitives (kinds 8 / 9 / 0xC / 0xE: the
//! "two packet slots" of FUN_80025B8C), phase 14.
//!
//! FUN_800152C4 (and its triangle / quad siblings of FUN_80012390 /
//! FUN_80012620) draws each such primitive twice in the same ordering-table
//! slot [CODE 800152c4][GPU analysis/s8/end/end2.json: 142 GT4 + 7 GT3
//! pairs on THE END]:
//! 1. an opaque polygon with the group's texture slot and the DAT UVs (the
//!    meshes, scene.rs / movers.rs: not semi-transparent whatever the group's
//!    blend bits say);
//! 2. a semi-transparent polygon (tpage / CLUT of texture slot 0x0E as the
//!    RAW loader set them, 0x800DFFE0 / 0x800DFFE2: ABR 0 on THE END and the
//!    OPTIONS scene) with the same vertex colours and per-vertex UVs from the
//!    vertex normal (the 4-byte records after a negative-count mesh's
//!    vertices, x / y / z bytes << 5) through the GTE colour matrix = the
//!    view rotation's rows normalised, the third negated (FUN_80012390's
//!    preamble), and the vertex's screen position:
//!    (X, Y, Z) = LCM * n; if Z < 1 { base = X < 0 ? 0x20 : 0x5F; X = -X >> 1 }
//!    else base = 0x40;
//!    u = ((X + (sx - 0x100) * 8) >> 7) + base, v = ((Y + (sy - 0x78) * 8) >> 7) + 0x40
//!    (both & 0xFF; sx = 256 + 256 x / z, sy = 120 + 160 y / z: H = 160, row 0
//!    of the camera matrix * 1.6, camera.md).
//!
//! Here: the second polygons are rebuilt every frame from the entity's
//! transform and the chase camera and queued for the effects pass (exact
//! ABR, texel STP honoured) a hair in front of the opaque one.

use bevy::prelude::*;

use crate::psxfx::{Blend, FxPrim, FxPrims, FxVert, VIEW_MAIN};

/// Texture slot of the reflection texture (0x800DFF00 + 0xE * 16).
pub const ENV_SLOT: usize = 0x0E;

#[derive(Clone, Copy, Debug)]
pub struct EnvPrim {
    /// Local (entity-space) Bevy positions, GPU vertex order.
    pub pos: [Vec3; 4],
    /// Local normals (Bevy axes), length = the game's |n| (bytes << 5).
    pub nrm: [Vec3; 4],
    pub rgb: [[u8; 3]; 4],
    pub tri: bool,
    pub cull: bool,
}

/// An object's environment-mapped primitives, placed by its
/// `GlobalTransform` (identity for world-baked scenery).
#[derive(Component, Clone)]
pub struct EnvPrims {
    pub prims: Vec<EnvPrim>,
    /// Slot 0x0E's (tpage, clut) and ABR.
    pub page: (u16, u16),
    pub abr: u8,
}

/// The environment-mapped primitives of one DAT object (`local`: mesh space,
/// else the object's world transform as scene::object_polys), or None.
pub fn object_env(dat: &mr_assets::dat::Dat, o: &mr_assets::dat::Object, sin: &mr_assets::dat::SinTable, slots: &[Option<mr_assets::dat::TexSlot>; 32], local: bool) -> Option<EnvPrims> {
    let slot = slots[ENV_SLOT]?;
    let mesh = dat.mesh(o)?;
    let t = dat.transform(o, sin);
    let world = |p: [i32; 3]| crate::to_bevy(p[0] * 32, p[1] * 32, p[2] * 32);
    let mut prims = Vec::new();
    for g in mesh.groups.iter().filter(|g| g.double_slot() && g.is_textured()) {
        let n = if g.is_triangle() { 3 } else { 4 };
        let ord: &[usize] = if n == 4 { &[0, 1, 3, 2] } else { &[0, 1, 2] };
        for p in &g.prims {
            let mut e = EnvPrim { pos: [Vec3::ZERO; 4], nrm: [Vec3::ZERO; 4], rgb: [[0; 3]; 4], tri: n == 3, cull: !g.double_sided() };
            for (k, &i) in ord.iter().enumerate() {
                let v = &mesh.vertices[p.idx[i] as usize];
                let pos = if local { v.pos.map(|c| c as i32) } else { t.apply(v.pos) };
                e.pos[k] = world(pos);
                let b = v.extra.unwrap_or([0; 4]);
                let raw = Vec3::new((b[0] as i8 as f32) * 32.0, (b[1] as i8 as f32) * 32.0, (b[2] as i8 as f32) * 32.0);
                // game axes -> Bevy (x, -y, -z); scenery baked in world space
                // turns its normals with the object (the rotation part only)
                let raw = if local { raw } else { Vec3::from_array(std::array::from_fn(|r| (0..3).map(|c| t.m[r][c] as f32 * raw[c]).sum::<f32>() / 4096.0)) };
                e.nrm[k] = Vec3::new(raw.x, -raw.y, -raw.z);
                e.rgb[k] = v.rgb();
            }
            prims.push(e);
        }
    }
    (!prims.is_empty()).then_some(EnvPrims { prims, page: (slot.tpage & !0x60, slot.clut), abr: ((slot.tpage >> 5) & 3) as u8 })
}

/// FUN_800152C4's reflection UV of one vertex: `view` = the vertex in view
/// space (Bevy: -z forward), `n` = its normal in view space (game length).
pub fn env_uv(view: Vec3, n: Vec3) -> Option<[u8; 2]> {
    let z = -view.z;
    if z <= 1e-5 {
        return None;
    }
    let sx = (256.0 * view.x / z).round() as i32;
    let sy = (-160.0 * view.y / z).round() as i32;
    // the colour matrix: x right, y down (game), z toward the eye
    Some(env_uv_game([sx, sy], [n.x.floor() as i32, (-n.y).floor() as i32, n.z.floor() as i32]))
}

/// The game's integer rule: `s` = the screen position relative to (OFX,
/// OFY), `mac` = the colour matrix times the normal (MAC1..3).
pub fn env_uv_game(s: [i32; 2], mac: [i32; 3]) -> [u8; 2] {
    let (mut x, y, z) = (mac[0], mac[1], mac[2]);
    let mut base = 0x40;
    if z < 1 {
        base = if x < 0 { 0x20 } else { 0x5F };
        x = -x >> 1;
    }
    let u = (((x + s[0] * 8) >> 7) + base) & 0xFF;
    let v = (((y + s[1] * 8) >> 7) + 0x40) & 0xFF;
    [u as u8, v as u8]
}

/// Queue the reflection polygons of every visible object for the main view.
pub fn push_env(q: Query<(&EnvPrims, &GlobalTransform, &InheritedVisibility)>, cam: Query<&GlobalTransform, With<crate::ChaseCamera>>, mut out: ResMut<FxPrims>) {
    let Ok(cam) = cam.single() else { return };
    let inv = cam.affine().inverse();
    let cam_rot = cam.affine().matrix3.transpose();
    for (env, gt, vis) in q.iter() {
        if !vis.get() {
            continue;
        }
        let a = gt.affine();
        for e in &env.prims {
            let n = if e.tri { 3 } else { 4 };
            let mut v = [FxVert::default(); 4];
            let mut center = Vec3::ZERO;
            let mut ok = true;
            for k in 0..n {
                let w = a.transform_point3(e.pos[k]);
                let view = inv.transform_point3(w);
                let len = e.nrm[k].length();
                let nv = (cam_rot * (a.matrix3 * e.nrm[k]).normalize_or_zero()) * len;
                let Some(uv) = env_uv(view, nv) else {
                    ok = false;
                    break;
                };
                let c = e.rgb[k];
                v[k] = FxVert { pos: w, uv: [uv[0] as f32 + 0.5, uv[1] as f32 + 0.5], rgb: [c[0] as f32 / 128.0, c[1] as f32 / 128.0, c[2] as f32 / 128.0] };
                center += w;
            }
            if !ok {
                continue;
            }
            out.list.push(FxPrim {
                v,
                page: Some(env.page),
                blend: Blend::Abr(env.abr),
                center: center / n as f32,
                // the same ordering-table slot, drawn after the opaque one
                depth_pull: 0.01,
                view: VIEW_MAIN,
                always_depth: true,
                smooth: false,
                tri: n == 3,
                cull_back: e.cull,
                eye_only: true,
                ..Default::default()
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The semi-transparent GT4 / GT3 packets (0x3E / 0x36) of a GP0 frame
    /// JSON on CLUT `clut`: (screen vertices, UVs).
    fn oracle_env(json: &str, clut: u16) -> Vec<(Vec<[i32; 2]>, Vec<[u8; 2]>)> {
        let mut out = Vec::new();
        for chunk in json.split("\"words\": [").skip(1) {
            let w: Vec<u32> = chunk.split(']').next().unwrap_or("").split(',').filter_map(|w| u32::from_str_radix(w.trim().trim_matches('"').trim_start_matches("0x"), 16).ok()).collect();
            let n = match w.first().map(|c| c >> 24) {
                Some(0x3E) if w.len() == 12 => 4,
                Some(0x36) if w.len() == 9 => 3,
                _ => continue,
            };
            if (w[2] >> 16) as u16 != clut {
                continue;
            }
            let xy = (0..n).map(|k| [(w[1 + 3 * k] & 0xFFFF) as i16 as i32, (w[1 + 3 * k] >> 16) as i16 as i32]).collect();
            let uv = (0..n).map(|k| [(w[2 + 3 * k] & 0xFF) as u8, ((w[2 + 3 * k] >> 8) & 0xFF) as u8]).collect();
            out.push((xy, uv));
        }
        out
    }

    type Poly = (Vec<[i32; 2]>, Vec<[u8; 2]>);

    /// The camera record a GP0 capture read from the frame ring
    /// (frame + k), as hex.
    fn ring_camera(json: &str, k: i32) -> Option<Vec<u8>> {
        let at = json.find(&format!("\"{k}\": {{"))?;
        let rest = &json[at..];
        let h = rest.find("\"hex\": \"")? + 8;
        let hex = &rest[h..h + rest[h..].find('"')?];
        (0..hex.len() / 2).map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).ok()).collect()
    }

    /// THE END's reflection polygons against the oracle's GP0 frames
    /// (analysis/s8/end_env/g800, g1600, local: the title state with
    /// 0x800E02B9 = 3, OPTIONS -> THE END; each with the camera record of
    /// the frame from the ring): every semi-transparent GT4 / GT3 on slot
    /// 0x0E's CLUT matched to a DAT primitive by its screen vertices, its
    /// UVs from the vertex normals, the colour matrix and the screen
    /// positions (env_uv_game).
    #[test]
    fn gp0_end_reflections() {
        let root = mr_sim::oracle::repo_root().join("analysis/s8/end_env");
        let Ok(mut disc) = mr_disc::Disc::open_default() else { return };
        let exe_b = disc.read("SLUS_012.37").unwrap();
        let exe = mr_sim::tables::ExeImage::parse(&exe_b).unwrap();
        let info = mr_sim::course::CourseInfo::read(&exe, crate::ending::COURSE).unwrap();
        let dat = mr_assets::dat::Dat::parse(&disc.read(&info.file("DAT")).unwrap()).unwrap();
        let sin = mr_assets::dat::SinTable::from_exe(&exe_b).unwrap();
        let env = mr_assets::raw::LoadEnv::from_exe(&exe_b, info.id as u32).unwrap();
        let dec = mr_assets::raw::decode(&disc.read(&mr_assets::raw::course_raw_path(&exe_b, info.id as u32).unwrap()).unwrap(), &env).unwrap();
        let clut = dec.slots[ENV_SLOT].unwrap().clut;
        // the environment-mapped vertices: (world units, normal << 5, object matrix)
        let mut prims: Vec<Vec<([f64; 3], [f64; 3], [[i16; 3]; 3])>> = Vec::new();
        for o in &dat.objects {
            let Some(mesh) = dat.mesh(o) else { continue };
            let t = dat.transform(o, &sin);
            for g in mesh.groups.iter().filter(|g| g.double_slot()) {
                let ord: &[usize] = if g.is_triangle() { &[0, 1, 2] } else { &[0, 1, 3, 2] };
                for p in &g.prims {
                    prims.push(
                        ord.iter()
                            .map(|&k| {
                                let v = &mesh.vertices[p.idx[k] as usize];
                                let w = t.apply_f(v.pos).map(|c| c * 32.0);
                                let b = v.extra.unwrap_or([0; 4]);
                                (w, [b[0] as i8 as f64 * 32.0, b[1] as i8 as f64 * 32.0, b[2] as i8 as f64 * 32.0], t.m)
                            })
                            .collect(),
                    );
                }
            }
        }
        let ours_for = |view: &mr_sim::camera::CameraView| -> Vec<Poly> {
            let r = view.gte_rotation.map(|row| row.map(|c| c as f64 / 4096.0));
            prims
                .iter()
                .filter_map(|p| {
                    let mut xy = Vec::new();
                    let mut uv = Vec::new();
                    for (w, n, m) in p {
                        let d = [w[0] - view.eye[0] as f64, w[1] - view.eye[1] as f64, w[2] - view.eye[2] as f64];
                        let c: [f64; 3] = std::array::from_fn(|i| (0..3).map(|k| r[i][k] * d[k]).sum::<f64>() / 32.0);
                        if c[2] <= 1.0 {
                            return None;
                        }
                        // the oracle runs the recomp's widescreen package: its GTE
                        // squashes x by 3/4 (game.toml [widescreen]), and the
                        // game reads the squashed SXY back for the UVs too
                        let sx = (160.0 * c[0] / c[2] * 0.75).floor() as i32;
                        let sy = (160.0 * c[1] / c[2]).floor() as i32;
                        // the colour matrix: rows of (camera * object) normalised, the third negated
                        let rm: [[f64; 3]; 3] = std::array::from_fn(|i| std::array::from_fn(|j| (0..3).map(|k| r[i][k] * m[k][j] as f64 / 4096.0).sum::<f64>()));
                        let mac: [i32; 3] = std::array::from_fn(|i| {
                            let row = rm[i];
                            let l = (row[0] * row[0] + row[1] * row[1] + row[2] * row[2]).sqrt();
                            let v = (row[0] * n[0] + row[1] * n[1] + row[2] * n[2]) / l;
                            (if i == 2 { -v } else { v }).floor() as i32
                        });
                        xy.push([sx + 256, sy + 120]);
                        uv.push(env_uv_game([sx, sy], mac));
                    }
                    Some((xy, uv))
                })
                .collect()
        };
        let near = |a: &[[i32; 2]], b: &[[i32; 2]]| a.len() == b.len() && a.iter().zip(b).all(|(p, q)| (p[0] - q[0]).abs() <= 1 && (p[1] - q[1]).abs() <= 1);
        let uv_near = |a: &[[u8; 2]], b: &[[u8; 2]]| a.iter().zip(b).all(|(p, q)| (p[0] as i32 - q[0] as i32).abs() <= 1 && (p[1] as i32 - q[1] as i32).abs() <= 1);
        for name in ["g800.json", "g1600.json"] {
            let Ok(text) = std::fs::read_to_string(root.join(name)) else { return };
            let theirs = oracle_env(&text, clut);
            assert!(theirs.len() > 50, "{name}: {} oracle polygons", theirs.len());
            let mut best = (0usize, 0usize, 0i32);
            for k in -3..3 {
                let Some(cam) = ring_camera(&text, k) else { continue };
                let view = mr_sim::camera::CameraState::from_bytes(&cam).view(0);
                let ours = ours_for(&view);
                let placed: Vec<&Poly> = theirs.iter().filter(|t| ours.iter().any(|o| near(&o.0, &t.0))).collect();
                let uv_ok = placed.iter().filter(|t| ours.iter().any(|o| near(&o.0, &t.0) && uv_near(&o.1, &t.1))).count();
                if std::env::var("MR_ENV_DEBUG").is_ok() {
                    let bb = |v: &[Poly]| { let xs: Vec<i32> = v.iter().flat_map(|p| p.0.iter().map(|q| q[0])).collect(); let ys: Vec<i32> = v.iter().flat_map(|p| p.0.iter().map(|q| q[1])).collect(); (xs.iter().min().copied(), xs.iter().max().copied(), ys.iter().min().copied(), ys.iter().max().copied(), v.len()) };
                    println!("  k {k}: eye {:?} ours {:?} theirs {:?}", view.eye, bb(&ours), bb(&theirs));
                }
                if placed.len() > best.0 {
                    best = (placed.len(), uv_ok, k);
                }
            }
            println!("{name}: {} oracle reflection polygons, {} placed (camera of frame {:+}), {} with the UVs (+-1)", theirs.len(), best.0, best.2, best.1);
            assert!(best.0 * 50 >= theirs.len() * 49, "{name}: placed {} of {}", best.0, theirs.len());
            assert!(best.1 * 50 >= best.0 * 49, "{name}: UVs {} of {}", best.1, best.0);
        }
    }
}

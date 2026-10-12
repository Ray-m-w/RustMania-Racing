//! Per-part placement of the player's vehicle (RACER) and driver (KERMIT)
//! from the kart record, as FUN_80026F3C (vehicle) and FUN_800276E8 (driver)
//! compute it before emitting each part's polygons. Spec: MDL.md
//! "Placing the parts (kart record -> part transforms)".
//!
//! The integer part (`KartModel::part_transforms`, `neck_vertices`) is pure
//! std and works in world space; the game does the same products with the
//! camera matrix folded into the base (RT = V * base * part), which only
//! changes rounding (verified against the live game: view-space RT/TR of
//! every part equal V * ours within +-2 / +-1). Bevy types appear only in
//! the conversion helpers at the end.

use mr_assets::mdl::{Mdl, PartData};

/// Kart record size (0x800C30A8 + 0x504 * kart).
pub const KART_LEN: usize = 0x504;

pub const IDENTITY: [[i32; 3]; 3] = [[4096, 0, 0], [0, 4096, 0], [0, 0, 4096]];

/// One drawn part's transform: model point p (part-local, model units =
/// world / 32) goes to `trans + (rot * p) >> 12` (model units, world space).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PartXform {
    /// 0 = vehicle (RACER), 1 = driver (KERMIT).
    pub model: usize,
    pub part: usize,
    pub rot: [[i32; 3]; 3],
    pub trans: [i32; 3],
    /// False when the game skips the part (kart +0x4F1 == 8 and part >= the
    /// kart's remaining-part count).
    pub visible: bool,
}

fn s16(k: &[u8], o: usize) -> i32 {
    i16::from_le_bytes([k[o], k[o + 1]]) as i32
}
fn i32le(k: &[u8], o: usize) -> i32 {
    i32::from_le_bytes([k[o], k[o + 1], k[o + 2], k[o + 3]])
}
/// 9 halfwords, row-major, 4.12.
pub fn mat(k: &[u8], o: usize) -> [[i32; 3]; 3] {
    let mut m = [[0; 3]; 3];
    for r in 0..3 {
        for c in 0..3 {
            m[r][c] = s16(k, o + 2 * (3 * r + c));
        }
    }
    m
}
/// GTE-style product, (a * b) >> 12 per element.
pub fn mul(a: &[[i32; 3]; 3], b: &[[i32; 3]; 3]) -> [[i32; 3]; 3] {
    let mut m = [[0; 3]; 3];
    for r in 0..3 {
        for c in 0..3 {
            m[r][c] = (0..3).map(|q| a[r][q] as i64 * b[q][c] as i64).sum::<i64>().wrapping_shr(12) as i32;
        }
    }
    m
}
pub fn apply(a: &[[i32; 3]; 3], v: [i32; 3]) -> [i32; 3] {
    let mut o = [0; 3];
    for r in 0..3 {
        o[r] = ((0..3).map(|q| a[r][q] as i64 * v[q] as i64).sum::<i64>() >> 12) as i32;
    }
    o
}

/// Kart record offsets the renderers read [CODE 80026F3C, 800276E8].
pub mod off {
    /// Vehicle base position: kart +0x00/+0x04/+0x08 (i32) plus the s16
    /// offsets at +0x46A/+0x46C/+0x46E.
    pub const POS: usize = 0x000;
    pub const POS_OFS: usize = 0x46A;
    /// Vehicle base matrix (body), used for parts below `SWITCH_PART`.
    pub const BODY_M: usize = 0x18C;
    /// Second base matrix, from the part whose index == byte +0x171 onward.
    pub const BODY2_M: usize = 0x1AC;
    /// Driver base matrix and position (i32 at +0x330, +0x338, +0x340).
    pub const DRIVER_M: usize = 0x1CC;
    pub const DRIVER_POS: usize = 0x330;
    /// Byte: vehicle parts below it take a matrix from `PART_M`.
    pub const PART_M_COUNT: usize = 0x170;
    /// Byte: index of the part that switches the base to `BODY2_M`; parts
    /// below it (wheels) take their pivot y from `SUSP_Y`.
    pub const SWITCH_PART: usize = 0x171;
    pub const PART_M: usize = 0x20C; // + 0x12 * part
    pub const SUSP_Y: usize = 0x2E; // s16, + 0x2C * part
    pub const SUSP_K: usize = 0x348; // i32
    pub const SUSP_F: usize = 0x180; // s16
    /// Driver parts 0 and 1 (head, body) take a matrix from here.
    pub const DRIVER_PART_M: usize = 0x29C; // + 0x12 * part
    pub const HIDE_MODE: usize = 0x4F1; // byte, 8 = parts lost
    pub const VEH_PARTS_LEFT: usize = 0x300; // i32
    pub const DRV_PARTS_LEFT: usize = 0x304; // i32
}

/// Rest data of the two models needed to place their parts.
#[derive(Clone, Debug)]
pub struct KartModel {
    /// Mesh-part pivots (model units), in slot order.
    pub vehicle_pivots: Vec<[i32; 3]>,
    pub driver_pivots: Vec<[i32; 3]>,
    /// Driver parts drawn by the seam renderer (slot kind 2).
    pub driver_necks: Vec<usize>,
    /// For each neck part: per polygon, per GPU-order vertex, (owner part,
    /// ring-local position) - the ring vertex it is stretched to.
    pub neck_bones: Vec<Vec<Vec<(usize, [i32; 3])>>>,
}

impl KartModel {
    pub fn new(vehicle: &Mdl, driver: &Mdl) -> KartModel {
        let piv = |m: &Mdl| m.meshes().map(|(_, p)| p.record.pivot).collect::<Vec<_>>();
        let meshes: Vec<_> = driver.meshes().map(|(_, p)| p).collect();
        let mut necks = Vec::new();
        let mut bones = Vec::new();
        for (slot, part) in meshes.iter().enumerate() {
            if part.record.point_w[1] == 0 {
                continue;
            }
            necks.push(slot);
            let (ra, rb) = part.seam_rings();
            let rings: Vec<_> = driver
                .parts
                .iter()
                .filter_map(|p| match &p.data {
                    PartData::Ring(r) if r.ring_number() == ra || r.ring_number() == rb => Some(r.clone()),
                    _ => None,
                })
                .collect();
            let mut per_poly = Vec::new();
            for poly in part.polys() {
                let mut vs = Vec::new();
                for &i in poly.gpu_order() {
                    let v = poly.verts[i].pos.map(|c| c as i32);
                    let model = [v[0] + part.record.pivot[0], v[1] + part.record.pivot[1], v[2] + part.record.pivot[2]];
                    // the ring vertex this neck vertex sits on (rest pose:
                    // ring local + owner pivot == neck vertex [MDL.md])
                    let mut best = (meshes.len(), [0; 3], i64::MAX);
                    for r in &rings {
                        let owner = r.owner_part();
                        let op = meshes.get(owner).map(|p| p.record.pivot).unwrap_or([0; 3]);
                        for (rv, _) in &r.verts {
                            let local = rv.map(|c| c as i32);
                            let d: i64 = (0..3).map(|k| ((local[k] + op[k] - model[k]) as i64).pow(2)).sum();
                            if d < best.2 {
                                best = (owner, local, d);
                            }
                        }
                    }
                    vs.push((best.0, best.1));
                }
                per_poly.push(vs);
            }
            bones.push(per_poly);
        }
        KartModel { vehicle_pivots: piv(vehicle), driver_pivots: piv(driver), driver_necks: necks, neck_bones: bones }
    }

    /// Every drawn part of both models for one kart record (vehicle parts
    /// first, then driver parts; neck parts carry the body transform and
    /// are drawn from `neck_vertices`).
    pub fn part_transforms(&self, k: &[u8]) -> Vec<PartXform> {
        let mut out = Vec::new();
        // vehicle: FUN_80026F3C
        let p = [
            (i32le(k, off::POS) + s16(k, off::POS_OFS)) >> 5,
            (i32le(k, off::POS + 4) + s16(k, off::POS_OFS + 2)) >> 5,
            (i32le(k, off::POS + 8) + s16(k, off::POS_OFS + 4)) >> 5,
        ];
        let mut base = mat(k, off::BODY_M);
        let n_mat = k[off::PART_M_COUNT] as usize;
        let switch = k[off::SWITCH_PART] as usize;
        let hide = k[off::HIDE_MODE] == 8;
        for (i, piv) in self.vehicle_pivots.iter().enumerate() {
            if i == switch {
                base = mat(k, off::BODY2_M);
            }
            let mut piv = *piv;
            if i < switch {
                // suspension: y = s16(+0x2E + 0x2C*i) - (i32(+0x348) * (0x80 - s16(+0x180))) >> 15
                let kk = i32le(k, off::SUSP_K) as i64 * (0x80 - s16(k, off::SUSP_F)) as i64;
                piv[1] = s16(k, off::SUSP_Y + 0x2C * i) - (kk >> 15) as i32;
            }
            let pm = if i < n_mat { mat(k, off::PART_M + 0x12 * i) } else { IDENTITY };
            let t = apply(&base, piv);
            out.push(PartXform {
                model: 0,
                part: i,
                rot: mul(&base, &pm),
                trans: [p[0] + t[0], p[1] + t[1], p[2] + t[2]],
                visible: !(hide && i as i32 >= i32le(k, off::VEH_PARTS_LEFT)),
            });
        }
        // driver: FUN_800276E8
        let p = [
            i32le(k, off::DRIVER_POS) >> 5,
            i32le(k, off::DRIVER_POS + 8) >> 5,
            i32le(k, off::DRIVER_POS + 16) >> 5,
        ];
        let base = mat(k, off::DRIVER_M);
        for (i, piv) in self.driver_pivots.iter().enumerate() {
            let pm = if i < 2 { mat(k, off::DRIVER_PART_M + 0x12 * i) } else { IDENTITY };
            let t = apply(&base, *piv);
            out.push(PartXform {
                model: 1,
                part: i,
                rot: mul(&base, &pm),
                trans: [p[0] + t[0], p[1] + t[1], p[2] + t[2]],
                visible: !(hide && i as i32 >= i32le(k, off::DRV_PARTS_LEFT)),
            });
        }
        out
    }

    /// World positions (model units) of each neck polygon's vertices (GPU
    /// order): every vertex follows the ring vertex of its owner part, as
    /// FUN_80039D90 transforms both rings with the head and body matrices.
    pub fn neck_vertices(&self, parts: &[PartXform]) -> Vec<Vec<Vec<[i32; 3]>>> {
        let drv: Vec<&PartXform> = parts.iter().filter(|p| p.model == 1).collect();
        self.neck_bones
            .iter()
            .map(|polys| {
                polys
                    .iter()
                    .map(|vs| {
                        vs.iter()
                            .map(|(owner, local)| {
                                let x = drv[*owner];
                                let t = apply(&x.rot, *local);
                                [x.trans[0] + t[0], x.trans[1] + t[1], x.trans[2] + t[2]]
                            })
                            .collect()
                    })
                    .collect()
            })
            .collect()
    }
}

/// A kart record for a pose without one (mr-sim): body matrix and position
/// for both models, no part matrices beyond identity, wheels at their rest
/// pivots (vehicle_pivots), driver at the kart position.
pub fn synth_record(x: i32, y: i32, z: i32, m: [i32; 9], vehicle_pivots: &[[i32; 3]]) -> Vec<u8> {
    let mut k = vec![0u8; KART_LEN];
    let put32 = |k: &mut Vec<u8>, o: usize, v: i32| k[o..o + 4].copy_from_slice(&v.to_le_bytes());
    let put16 = |k: &mut Vec<u8>, o: usize, v: i32| k[o..o + 2].copy_from_slice(&(v as i16).to_le_bytes());
    put32(&mut k, 0, x);
    put32(&mut k, 4, y);
    put32(&mut k, 8, z);
    put32(&mut k, off::DRIVER_POS, x);
    put32(&mut k, off::DRIVER_POS + 8, y);
    put32(&mut k, off::DRIVER_POS + 16, z);
    for base in [off::BODY_M, off::BODY2_M, off::DRIVER_M] {
        for (i, v) in m.iter().enumerate() {
            put16(&mut k, base + 2 * i, *v);
        }
    }
    let n = vehicle_pivots.len().saturating_sub(1).min(8);
    k[off::PART_M_COUNT] = n as u8;
    k[off::SWITCH_PART] = n as u8;
    put16(&mut k, off::SUSP_F, 0x80);
    for i in 0..n {
        for d in 0..3 {
            put16(&mut k, off::PART_M + 0x12 * i + 2 * (3 * d + d), 4096);
        }
        put16(&mut k, off::SUSP_Y + 0x2C * i, vehicle_pivots[i][1]);
    }
    for i in 0..2 {
        for d in 0..3 {
            put16(&mut k, off::DRIVER_PART_M + 0x12 * i + 2 * (3 * d + d), 4096);
        }
    }
    k
}

// ---------------------------------------------------------- Bevy side ----

use bevy::prelude::{Mat3, Quat, Transform, Vec3};

/// Bevy transform of a part: translation = trans * 32 in world units,
/// rotation = F R F with F = diag(1, -1, -1) (the game->Bevy axis flip).
pub fn to_transform(x: &PartXform) -> Transform {
    let f = [1.0f32, -1.0, -1.0];
    let e = |r: usize, c: usize| f[r] * f[c] * x.rot[r][c] as f32 / 4096.0;
    let m = Mat3::from_cols(
        Vec3::new(e(0, 0), e(1, 0), e(2, 0)),
        Vec3::new(e(0, 1), e(1, 1), e(2, 1)),
        Vec3::new(e(0, 2), e(1, 2), e(2, 2)),
    );
    Transform {
        translation: crate::to_bevy(x.trans[0] * 32, x.trans[1] * 32, x.trans[2] * 32),
        rotation: Quat::from_mat3(&m).normalize(),
        scale: Vec3::ONE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn models() -> Option<(Mdl, Mdl)> {
        let mut d = mr_disc::Disc::open_default().ok()?;
        let r = Mdl::parse(&d.read("VEHIC_A/RACER.MDL").ok()?).ok()?;
        let k = Mdl::parse(&d.read("CHARS_A/KERMIT.MDL").ok()?).ok()?;
        Some((r, k))
    }

    #[test]
    fn rest_pose_reproduces_vertex_plus_pivot() {
        let Some((r, k)) = models() else { eprintln!("SKIP: no disc"); return };
        let km = KartModel::new(&r, &k);
        let id = [4096, 0, 0, 0, 4096, 0, 0, 0, 4096];
        let rec = synth_record(320, 640, -960, id, &km.vehicle_pivots);
        let xs = km.part_transforms(&rec);
        assert_eq!(xs.len(), 5 + 3);
        for x in &xs {
            assert_eq!(x.rot, IDENTITY);
            let piv = if x.model == 0 { km.vehicle_pivots[x.part] } else { km.driver_pivots[x.part] };
            assert_eq!(x.trans, [10 + piv[0], 20 + piv[1], -30 + piv[2]], "{x:?}");
        }
        // neck: every vertex lands on its rest position (ring + owner pivot)
        let neck = km.neck_vertices(&xs);
        let part = k.meshes().nth(km.driver_necks[0]).unwrap().1;
        for (poly, got) in part.polys().iter().zip(&neck[0]) {
            for (j, &i) in poly.gpu_order().iter().enumerate() {
                let v = poly.verts[i].pos;
                assert_eq!(got[j], [10 + v[0] as i32, 20 + v[1] as i32, -30 + v[2] as i32]);
            }
        }
    }

    /// lap1 oracle sidecar (if present): rotations stay orthonormal-ish,
    /// wheels hang below the body, the driver sits inside the vehicle.
    #[test]
    fn lap1_sidecar_is_plausible() {
        let Some((r, k)) = models() else { eprintln!("SKIP: no disc"); return };
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../analysis/traces/lap1.oracle.kart.bin");
        let Ok(bin) = std::fs::read(path) else { eprintln!("SKIP: no {path}"); return };
        let km = KartModel::new(&r, &k);
        for rec in bin.chunks_exact(KART_LEN).step_by(37) {
            let xs = km.part_transforms(rec);
            for x in &xs {
                for row in x.rot {
                    let n = ((row.iter().map(|&v| (v as i64).pow(2)).sum::<i64>()) as f64).sqrt();
                    assert!((n - 4096.0).abs() < 40.0, "{x:?}");
                }
            }
            let body = xs[4].trans;
            let drv = xs[6].trans; // kermit body
            let d: f64 = (0..3).map(|i| ((body[i] - drv[i]) as f64).powi(2)).sum::<f64>().sqrt();
            assert!(d < 200.0, "driver {drv:?} body {body:?}");
        }
    }
}

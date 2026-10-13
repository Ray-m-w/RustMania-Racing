//! N karts, any character + vehicle: one rig (vehicle parts, driver parts,
//! neck polygons) per kart slot, placed every frame from that kart's record
//! with `kartmodel` (FUN_80026F3C / FUN_800276E8, MDL.md "Placing the parts").
//!
//! Input: the `KartFeed` resource, refreshed once per logic tick with
//! `(slot, record)` pairs - from the player's replay sidecar / mr-sim
//! session record (`feed_player`), from a physics capture (capture.rs), or
//! from `Session::kart_records()` via `KartFeed::set_records`. Character
//! and vehicle come from the record (+0x4B2 / +0x4B1); model paths from the
//! exe's path tables (0x800AD1CC characters, 0x800AD10C vehicles, read at
//! run time); textures from the course `.RAW` slot table (all six DOCK
//! racers use slots 0x10/0x12/0x13, which DOCK.RAW carries).

use std::collections::HashMap;

use bevy::prelude::*;
use mr_assets::mdl::{Mdl, SlotAttr};

use crate::kartmodel::{self, KartModel, KART_LEN};
use crate::psxmesh::{self, PsxPoly};
use crate::vramtex::VramTextures;

pub const VEHICLE_PATHS: u32 = 0x800A_D10C;
pub const CHARACTER_PATHS: u32 = 0x800A_D1CC;

/// One kart this tick.
#[derive(Clone, Debug)]
pub struct KartFrame {
    pub slot: u8,
    pub character: u8,
    pub vehicle: u8,
    pub record: Vec<u8>,
}

impl KartFrame {
    pub fn from_record(slot: u8, record: &[u8]) -> KartFrame {
        KartFrame { slot, character: record[0x4B2], vehicle: record[0x4B1], record: record[..KART_LEN].to_vec() }
    }
}

/// The karts to draw: this tick and the previous one (for interpolation).
#[derive(Resource, Default)]
pub struct KartFeed {
    pub now: Vec<KartFrame>,
    pub prev: Vec<KartFrame>,
    /// Set when another source (capture replay, a session) owns the feed;
    /// `feed_player` then leaves it alone.
    pub external: bool,
}

impl KartFeed {
    /// Start a new tick from `(slot, record)` pairs.
    pub fn set_records<'a>(&mut self, recs: impl IntoIterator<Item = (u8, &'a [u8])>) {
        let now = recs.into_iter().map(|(s, r)| KartFrame::from_record(s, r)).collect();
        self.prev = std::mem::replace(&mut self.now, now);
    }
}

/// `<dir>\name` from one of the exe's path tables, as a disc path + ext.
pub fn model_path(exe: &[u8], table: u32, i: u8, ext: &str) -> Option<String> {
    let at = |a: u32| (a as usize).checked_sub(0x8001_0000).map(|o| o + 0x800);
    let o = at(table + 4 * i as u32)?;
    let p = u32::from_le_bytes(exe.get(o..o + 4)?.try_into().ok()?);
    let s = exe.get(at(p)?..)?;
    let n = s.iter().position(|&b| b == 0)?;
    let name = std::str::from_utf8(&s[..n]).ok()?;
    if !name.contains('\\') || name.ends_with('\\') {
        return None;
    }
    Some(format!("{}.{}", name.replace('\\', "/").to_ascii_uppercase(), ext))
}

/// Meshes shared by every kart with this (character, vehicle).
struct RigTemplate {
    model: KartModel,
    /// ((model 0 vehicle / 1 driver, slot part), meshes) for rigid parts.
    parts: Vec<((usize, usize), Vec<(Handle<Mesh>, Handle<StandardMaterial>)>)>,
    /// Neck polygons (per neck, per polygon), built per kart.
    necks: Vec<Vec<PsxPoly>>,
}

/// Disc + exe + texture slots, and the rig templates built so far.
#[derive(Resource)]
pub struct ModelLib {
    disc: mr_disc::Disc,
    exe: Vec<u8>,
    slots: [Option<SlotAttr>; 32],
    templates: HashMap<(u8, u8), RigTemplate>,
}

impl ModelLib {
    pub fn new(disc: mr_disc::Disc, exe: Vec<u8>, slots: [Option<SlotAttr>; 32]) -> Self {
        ModelLib { disc, exe, slots, templates: HashMap::new() }
    }

    fn mdl(&mut self, table: u32, i: u8) -> Option<Mdl> {
        let p = model_path(&self.exe, table, i, "MDL")?;
        Mdl::parse(&self.disc.read(&p).ok()?).ok()
    }

    /// Rest pivots of a vehicle (for synthesized records).
    pub fn vehicle_pivots(&mut self, vehicle: u8) -> Vec<[i32; 3]> {
        self.mdl(VEHICLE_PATHS, vehicle).map(|m| m.meshes().map(|(_, p)| p.record.pivot).collect()).unwrap_or_default()
    }

    fn template(
        &mut self,
        character: u8,
        vehicle: u8,
        tex: &mut VramTextures,
        images: &mut Assets<Image>,
        meshes: &mut Assets<Mesh>,
        mats: &mut Assets<StandardMaterial>,
    ) -> Option<&RigTemplate> {
        if !self.templates.contains_key(&(character, vehicle)) {
            let veh = self.mdl(VEHICLE_PATHS, vehicle)?;
            let drv = self.mdl(CHARACTER_PATHS, character)?;
            let model = KartModel::new(&veh, &drv);
            let local = |p: [i32; 3]| crate::to_bevy(p[0] * 32, p[1] * 32, p[2] * 32);
            let mut parts = Vec::new();
            let mut necks = vec![Vec::new(); model.driver_necks.len()];
            for (mi, m) in [&veh, &drv].into_iter().enumerate() {
                for (slot, (_, part)) in m.meshes().enumerate() {
                    let polys = crate::scene::part_polys(part, &self.slots);
                    if mi == 1 {
                        if let Some(ni) = model.driver_necks.iter().position(|&n| n == slot) {
                            necks[ni] = polys;
                            continue;
                        }
                    }
                    parts.push(((mi, slot), psxmesh::build(&polys, tex, images, meshes, mats, local)));
                }
            }
            self.templates.insert((character, vehicle), RigTemplate { model, parts, necks });
        }
        self.templates.get(&(character, vehicle))
    }
}

/// Root of one kart's rig.
#[derive(Component)]
pub struct KartRigRoot {
    pub slot: u8,
    pub character: u8,
    pub vehicle: u8,
}

#[derive(Component)]
pub struct RigPart {
    pub slot: u8,
    pub model: usize,
    pub part: usize,
}

/// One neck polygon of a kart (world-space vertices rewritten per frame).
#[derive(Component)]
pub struct RigNeck {
    pub slot: u8,
    pub neck: usize,
    pub poly: usize,
    pub mesh: Handle<Mesh>,
}

/// Spawn / replace rigs so there is exactly one per fed kart.
#[allow(clippy::too_many_arguments)]
pub fn sync_rigs(
    feed: Res<KartFeed>,
    mut lib: ResMut<ModelLib>,
    tex: Option<ResMut<VramTextures>>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    roots: Query<(Entity, &KartRigRoot)>,
    mut commands: Commands,
) {
    let Some(mut tex) = tex else { return };
    for (e, r) in roots.iter() {
        if !feed.now.iter().any(|k| k.slot == r.slot && k.character == r.character && k.vehicle == r.vehicle) {
            commands.entity(e).despawn();
        }
    }
    for k in &feed.now {
        if roots.iter().any(|(_, r)| r.slot == k.slot && r.character == k.character && r.vehicle == k.vehicle) {
            continue;
        }
        let Some(t) = lib.template(k.character, k.vehicle, &mut tex, &mut images, &mut meshes, &mut mats) else {
            warn!("kart slot {}: no model for character {} / vehicle {}", k.slot, k.character, k.vehicle);
            continue;
        };
        let parts: Vec<_> = t.parts.iter().map(|(id, m)| (*id, m.clone())).collect();
        let necks: Vec<Vec<PsxPoly>> = t.necks.clone();
        let slot = k.slot;
        commands
            .spawn((Transform::default(), Visibility::default(), KartRigRoot { slot, character: k.character, vehicle: k.vehicle }))
            .with_children(|c| {
                for ((model, part), ms) in parts {
                    c.spawn((Transform::default(), Visibility::default(), RigPart { slot, model, part })).with_children(|c| {
                        for (mesh, mat) in ms {
                            c.spawn((Mesh3d(mesh), MeshMaterial3d(mat)));
                        }
                    });
                }
                for (ni, polys) in necks.iter().enumerate() {
                    for (pi, poly) in polys.iter().enumerate() {
                        let id = |p: [i32; 3]| crate::to_bevy(p[0] * 32, p[1] * 32, p[2] * 32);
                        for (mesh, mat) in psxmesh::build(std::slice::from_ref(poly), &mut tex, &mut images, &mut meshes, &mut mats, id) {
                            c.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat), Transform::default(), Visibility::default(), RigNeck { slot, neck: ni, poly: pi, mesh }));
                        }
                    }
                }
            });
    }
}

/// Place every rig's parts and neck from this tick's and last tick's records.
pub fn place_rigs(
    feed: Res<KartFeed>,
    lib: Res<ModelLib>,
    time: Res<Time<Fixed>>,
    mut parts: Query<(&RigPart, &mut Transform, &mut Visibility)>,
    necks: Query<&RigNeck>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let a = time.overstep_fraction();
    let mut xs: HashMap<u8, (Vec<kartmodel::PartXform>, Vec<kartmodel::PartXform>, &RigTemplate)> = HashMap::new();
    for k in &feed.now {
        let Some(t) = lib.templates.get(&(k.character, k.vehicle)) else { continue };
        let prev = feed.prev.iter().find(|p| p.slot == k.slot && p.character == k.character && p.vehicle == k.vehicle).unwrap_or(k);
        let mut x0 = t.model.part_transforms(&prev.record);
        let mut x1 = t.model.part_transforms(&k.record);
        // byte +0x4A4 / +0x4A5 = -1: no vehicle / no driver model loaded
        for (x, r) in [(&mut x0, &prev.record), (&mut x1, &k.record)] {
            for p in x.iter_mut() {
                if r[0x4A4 + p.model] == 0xFF {
                    p.visible = false;
                }
            }
        }
        xs.insert(k.slot, (x0, x1, t));
    }
    for (rp, mut tr, mut vis) in parts.iter_mut() {
        let Some((x0, x1, _)) = xs.get(&rp.slot) else { continue };
        let find = |v: &[kartmodel::PartXform]| v.iter().find(|x| x.model == rp.model && x.part == rp.part).copied();
        let (Some(b0), Some(b1)) = (find(x0), find(x1)) else { continue };
        let (t0, t1) = (kartmodel::to_transform(&b0), kartmodel::to_transform(&b1));
        tr.translation = t0.translation.lerp(t1.translation, a);
        tr.rotation = t0.rotation.slerp(t1.rotation, a);
        let want = if b1.visible { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
    }
    let mut necks_now: HashMap<u8, (Vec<Vec<Vec<[i32; 3]>>>, Vec<Vec<Vec<[i32; 3]>>>)> = HashMap::new();
    for (slot, (x0, x1, t)) in &xs {
        necks_now.insert(*slot, (t.model.neck_vertices(x0), t.model.neck_vertices(x1)));
    }
    for n in necks.iter() {
        let Some((n0, n1)) = necks_now.get(&n.slot) else { continue };
        let (Some(v0), Some(v1)) = (n0.get(n.neck).and_then(|v| v.get(n.poly)), n1.get(n.neck).and_then(|v| v.get(n.poly))) else { continue };
        let Some(mut mesh) = meshes.get_mut(&n.mesh) else { continue };
        let pos: Vec<[f32; 3]> = v0
            .iter()
            .zip(v1)
            .map(|(p0, p1)| {
                crate::to_bevy(p0[0] * 32, p0[1] * 32, p0[2] * 32).lerp(crate::to_bevy(p1[0] * 32, p1[1] * 32, p1[2] * 32), a).to_array()
            })
            .collect();
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    }
}

/// Feed the player's kart (slot 0) from `Player`: the replay sidecar or the
/// mr-sim session record; without one, a record synthesized from the pose
/// (Kermit / RACER, rest part matrices).
pub fn feed_player(p: Res<crate::Player>, mut feed: ResMut<KartFeed>, mut lib: ResMut<ModelLib>) {
    if feed.external || !p.is_changed() {
        return;
    }
    let rec = match &p.kart {
        Some(r) if r.len() >= KART_LEN => r.clone(),
        _ => {
            let pose = p.pose;
            let id = if pose.m == [0; 9] { [4096, 0, 0, 0, 4096, 0, 0, 0, 4096] } else { pose.m };
            let piv = lib.vehicle_pivots(0);
            kartmodel::synth_record(pose.x, pose.y, pose.z, id, &piv)
        }
    };
    feed.set_records([(0u8, &rec[..])]);
}

pub struct KartsPlugin;

impl Plugin for KartsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<KartFeed>()
            .add_systems(FixedUpdate, feed_player.after(crate::sim_tick))
            .add_systems(Update, (sync_rigs, place_rigs.after(sync_rigs)));
    }
}

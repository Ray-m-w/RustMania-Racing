//! Breakable scenery (Session 6, track P): FUN_80083B08 (race load) lists
//! TER groups that a weapon can knock down (0x800D7FB0, 4 bytes each: DAT
//! special slot i16, slot count u8, TER group u8; 0x800B0F24 of them) and
//! enables them; FUN_80069334 (a projectile hitting a wall or floor of such
//! a group, from FUN_80069648) disables the group (FUN_8004229C: its faces
//! and walls stop colliding), hides its DAT objects and puffs two
//! explosions. GRAVE starts with group 2 disabled and enables it when
//! group 1 breaks; on GALLERY group 0 breaking takes group 1 with it (and
//! group 1 cannot be hit on its own).
//!
//! The tables are FUN_80083B08's switch (jump table 0x80023318, course - 1,
//! courses 1..23), checked against the oracle's RAM on every race course.

use crate::globals::RaceGlobals;
use crate::world::World;

/// One entry of 0x800D7FB0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Breakable {
    /// +0: first DAT special slot (-1 none), +2: how many.
    pub slot: i16,
    pub slots: u8,
    /// +3: TER group.
    pub group: u8,
    /// The first slot's object origin << 5 (FUN_8002DF0C: the descriptor's
    /// origin), where the second explosion goes.
    pub origin: Option<[i32; 3]>,
}

/// FUN_80083B08's entries for course `id`.
pub fn table(id: u8) -> Vec<(i16, u8, u8)> {
    let seq = |n: u8, first_slot: i16, first_group: u8| (0..n).map(|i| (first_slot + i as i16, 1u8, first_group + i)).collect::<Vec<_>>();
    match id {
        // 0x80083DA4: groups 0..12, slot = group + 1.
        1 => seq(13, 1, 0),
        // 0x800840A0
        2 => seq(10, 1, 0),
        // 0x80084274
        4 => {
            let mut v = vec![(1, 2, 0), (-1, 1, 1)];
            v.extend(seq(15, 4, 2));
            v
        }
        // 0x80083B54
        6 => {
            let mut v = vec![(5, 3, 2)];
            v.extend(seq(6, 8, 3));
            v
        }
        // 0x80083E10
        7 => seq(2, 1, 0),
        // 0x8008410C
        9 => vec![(1, 3, 1), (4, 3, 2), (7, 2, 3), (9, 2, 4)],
        // 0x80083FB8
        15 => seq(4, 7, 0),
        // 0x80083F4C
        17 => seq(2, 1, 0),
        _ => Vec::new(),
    }
}

/// The course's breakables with the explosion points.
pub fn load(id: u8, dat: &mr_assets::dat::Dat) -> Vec<Breakable> {
    table(id)
        .into_iter()
        .map(|(slot, slots, group)| {
            let origin = if slot >= 0 {
                dat.special.get(slot as usize).copied().flatten().and_then(|oi| {
                    let o = &dat.objects[oi];
                    dat.descriptors.get(&o.desc_offset).map(|d| [d.origin[0] << 5, d.origin[1] << 5, d.origin[2] << 5])
                })
            } else {
                None
            };
            Breakable { slot, slots, group, origin }
        })
        .collect()
}

/// TER group enabled flags after the race load (FUN_80083B08): the listed
/// groups enabled, GRAVE's group 2 disabled; None = left as on the disc.
pub fn race_load(id: u8, groups: usize, list: &[Breakable]) -> Vec<Option<bool>> {
    let mut e = vec![None; groups];
    for b in list {
        if (b.group as usize) < groups {
            e[b.group as usize] = Some(true);
        }
    }
    if id == 7 && groups > 2 {
        e[2] = Some(false);
    }
    e
}

/// FUN_80069334(projectile, group): a projectile hit TER group `group`; if
/// it is breakable it breaks. Returns true when it did (the projectile is
/// spent: its +0x2E cleared). `pos` / `spin` / `owner` are the projectile's.
pub fn hit(g: &mut RaceGlobals, w: &World, group: i32, pos: [i32; 3], spin: i32) -> bool {
    let id = w.info.id;
    let list = &w.course.objects.breakables;
    let Some(b) = list.iter().find(|b| b.group as i32 == group && !(id == 4 && group == 1)).copied() else { return false };
    set(g, b.group as usize, false);
    let r = crate::update::rng_next(g, w) as i32;
    crate::items::particle(g, pos, [0; 3], 0, r << 4, spin, 0x11, [0x80; 3], 0x100, 0x100, 0x20);
    crate::sound::at(g, w, 0x14, 0x1600, 0x80, pos, -1);
    if b.slot != -1 {
        let o = b.origin.unwrap_or(pos);
        let r = crate::update::rng_next(g, w) as i32;
        crate::items::particle(g, o, [0; 3], 0, r << 4, (-spin) & 0xFFF, 0x11, [0x80; 3], 0x300, 0x300, 0x20);
        g.objects.hidden_slots.push((b.slot, b.slots));
    }
    if id == 4 && group == 0 {
        set(g, 1, false);
    } else if id == 7 && group == 1 {
        set(g, 2, true);
    }
    true
}

fn set(g: &mut RaceGlobals, group: usize, on: bool) {
    let e = &mut g.objects.group_enabled;
    if e.len() <= group {
        e.resize(group + 1, None);
    }
    e[group] = Some(on);
}

/// Apply the wanted group states to the TER (FUN_800421C4 / FUN_8004229C).
pub fn sync(ter: &mut mr_assets::ter::Ter, applied: &mut Vec<Option<bool>>, want: &[Option<bool>]) {
    if applied.len() < want.len() {
        applied.resize(want.len(), None);
    }
    for (gi, w) in want.iter().enumerate() {
        if let Some(on) = *w {
            if applied[gi] != Some(on) {
                ter.set_group_enabled(gi, on);
                applied[gi] = Some(on);
            }
        }
    }
}

/// FUN_8003C568: the TER group a wall entry belongs to (first match).
pub fn wall_group(ter: &mr_assets::ter::Ter, wall: usize) -> i32 {
    ter.groups
        .iter()
        .position(|g| (g.wall_start as usize) <= wall && wall < g.wall_start as usize + g.wall_count.max(0) as usize)
        .map(|i| i as i32)
        .unwrap_or(-1)
}

/// FUN_8003C4D4: the TER group a face belongs to (first match).
pub fn face_group(ter: &mr_assets::ter::Ter, face: usize) -> i32 {
    ter.groups
        .iter()
        .position(|g| (g.face_start as usize) <= face && face < g.face_start as usize + g.face_count.max(0) as usize)
        .map(|i| i as i32)
        .unwrap_or(-1)
}

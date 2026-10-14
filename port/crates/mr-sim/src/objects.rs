//! Course objects the kart code reads beyond DOCK's (Session 6, track P):
//! DAT point lists registered by FUN_8002E3DC at 0x800D1C48 + 4 * id and
//! the code that drives them. docs/spec/courses.md "Course objects".
//!
//! - Air jets, chunk 0x0E (0x800D1C80; LONDON, BASE, NEW_YORK, CAPE, DISCO,
//!   GLACIER): records of five points. FUN_800637B8 (course load) derives
//!   their on / off times and reach, FUN_80063A98 (every race frame) switches
//!   them on and off from the frame counter, FUN_80063C24 (kart update,
//!   after the ground contact) pushes a kart inside an active one, and the
//!   first loop of FUN_8007F03C puffs smoke from them.
//! - Turntables, chunk 0x11 (0x800D1C8C; BASE, OVAL, DISCO): records of
//!   three points (centre, inner and outer radius). FUN_80084490 (course
//!   load) draws each one's phase and spin (+-5 per VBlank, x3 on courses
//!   0x1A / 0x1B), FUN_800632BC (kart update, before the ground contact)
//!   turns a grounded kart on the ring with it.
//! - Hooks, chunk 0x02 (0x800D1C50; GRAVE): see `hooks`.

use crate::fixed::{div_pow2, s16};
use crate::globals::RaceGlobals;
use crate::kart::KartState;
use crate::tables::{ratan2, square_root0, Tables};
use crate::world::World;

/// DAT chunk ids (0x800D1C48 + 4 * id).
pub mod chunk {
    pub const HOOKS: i16 = 0x02;
    pub const JETS: i16 = 0x0E;
    pub const TURNTABLES: i16 = 0x11;
}

/// The lists as the course data holds them (DAT units: world >> 5).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CourseObjects {
    /// Chunk 0x0E: count % 5 == 0 and count > 4; at most 32 (0x800B0C58).
    pub jets: Vec<[[i32; 3]; 5]>,
    /// Chunk 0x11: count % 3 == 0; at most 8 (0x800B0B2A).
    pub turntables: Vec<[[i32; 3]; 3]>,
    /// Chunk 0x02: count & 0x1F == 0; count >> 5 groups of 32 points, at
    /// most 8 (0x800B0E00).
    pub hooks: Vec<[[i32; 3]; 32]>,
    /// Chunk 0x18 lifts as loaded (lifts.rs) and the course's objects that
    /// drive them (FUN_80084490's switch).
    pub lifts: Vec<crate::lifts::Lift>,
    pub lift_objects: Vec<crate::lifts::LiftObject>,
    /// FUN_80083B08's breakable groups (breakables.rs).
    pub breakables: Vec<crate::breakables::Breakable>,
}

impl CourseObjects {
    pub fn from_dat(dat: &mr_assets::dat::Dat, bytes: &[u8]) -> CourseObjects {
        let le32 = |o: usize| i32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
        let find = |id: i16| dat.chunks.iter().find(|c| c.id == id);
        let pt = |o: usize| [le32(o), le32(o + 4), le32(o + 8)];
        let mut out = CourseObjects::default();
        // FUN_800637B8: (u16)count % 5 == 0 && count > 4; n = min(count / 5, 32).
        if let Some(c) = find(chunk::JETS) {
            let n = c.count as u16;
            if n % 5 == 0 && n > 4 {
                for i in 0..((n / 5) as usize).min(32) {
                    let b = c.offset + 4 + 60 * i;
                    out.jets.push(std::array::from_fn(|j| pt(b + 12 * j)));
                }
            }
        }
        // FUN_80084490: (u16)count % 3 == 0; n = min(count / 3, 8).
        if let Some(c) = find(chunk::TURNTABLES) {
            let n = c.count as u16;
            if n % 3 == 0 {
                for i in 0..((n / 3) as usize).min(8) {
                    let b = c.offset + 4 + 36 * i;
                    out.turntables.push(std::array::from_fn(|j| pt(b + 12 * j)));
                }
            }
        }
        // FUN_800616A0: (u16)count & 0x1F == 0; n = min(count >> 5, 8).
        if let Some(c) = find(chunk::HOOKS) {
            let n = c.count as u16;
            if n & 0x1F == 0 {
                for i in 0..((n >> 5) as usize).min(8) {
                    let b = c.offset + 4 + 12 * 32 * i;
                    out.hooks.push(std::array::from_fn(|j| pt(b + 12 * j)));
                }
            }
        }
        out
    }
}

/// One jet's timing (0x800D19C0 + 12 * i), FUN_800637B8.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Jet {
    /// +0: VBlanks on (|P3 - P2| * 60 / 50; 0 = always on).
    pub on: i16,
    /// +2: VBlanks off (|P4 - P3| * 60 / 50; 0 = always on).
    pub off: i16,
    /// +4 / +6: |P2 - P0| and |P1 - P0| (reach).
    pub d02: i16,
    pub d01: i16,
    /// +8: blowing now.
    pub active: u8,
}

/// Run-time state of the course objects (part of the race globals).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ObjectState {
    pub jets: Vec<Jet>,
    /// 0x800BC648 + 4 * i: turntable phase (rand() / 8 at course load;
    /// render side) and spin per VBlank (+0x2: +-5, x3 on courses
    /// 0x1A / 0x1B).
    pub turntable_phase: Vec<i16>,
    pub turntable_spin: Vec<i16>,
    /// GRAVE's hooks (0x800D15A0, 3 per group).
    pub hooks: Vec<crate::hooks::Hook>,
    /// Falling weather (weather.rs); None when the course has none.
    pub weather: Option<crate::weather::Weather>,
    /// Lifts (lifts.rs) and each lift object's accumulated move of its
    /// TER groups.
    pub lifts: Vec<crate::lifts::Lift>,
    pub lift_offsets: Vec<i32>,
    /// TER group enabled states the race wants (breakables.rs; None = as
    /// on the disc), and the DAT slots knocked down (render side).
    pub group_enabled: Vec<Option<bool>>,
    pub hidden_slots: Vec<(i16, u8)>,
    /// The course's animated scenery (scenery.rs; 0x800D8630): spin
    /// angles and spin-up per object (render side).
    pub anim: Vec<crate::scenery::AnimObject>,
}

/// |a - b| with the game's SquareRoot0 of the squared sum (wrapping).
fn dist(t: &Tables, a: [i32; 3], b: [i32; 3]) -> i32 {
    let d = |i: usize| b[i].wrapping_sub(a[i]);
    let s = d(0).wrapping_mul(d(0)).wrapping_add(d(1).wrapping_mul(d(1))).wrapping_add(d(2).wrapping_mul(d(2)));
    square_root0(t, s)
}

/// FUN_800637B8 (course load): every jet's timing; all start active.
pub fn jets_load(w: &World) -> Vec<Jet> {
    let t = &w.tables;
    w.course.objects
        .jets
        .iter()
        .map(|p| {
            let mut j = Jet { d01: dist(t, p[0], p[1]) as i16, d02: dist(t, p[0], p[2]) as i16, active: 1, ..Default::default() };
            let d23 = dist(t, p[2], p[3]);
            if d23 >= 0x19 {
                let d34 = dist(t, p[3], p[4]);
                if d34 >= 0x19 {
                    j.on = ((d23 * 0x3C) / 0x32) as i16;
                    j.off = ((d34 * 0x3C) / 0x32) as i16;
                }
            }
            j
        })
        .collect()
}

/// FUN_80063A98 (every race frame, after the water animation): a jet blows
/// while frame % (on + off) < on (frame = 0x800B0D90); switching on plays
/// sound 0x11 at its first point.
pub fn jets_frame(g: &mut RaceGlobals, w: &World) {
    for i in 0..g.objects.jets.len() {
        let j = g.objects.jets[i];
        let prev = j.active;
        let (on, off) = (j.on as i32, j.off as i32);
        let active = if on == 0 || off == 0 { 1 } else if g.frame % (on + off) < on { 1 } else { 0 };
        g.objects.jets[i].active = active;
        if on != 0 && off != 0 && prev == 0 && active == 1 {
            let p = w.course.objects.jets[i][0];
            crate::sound::at(g, w, 0x11, 0x1600, 0x40, [p[0] << 5, p[1] << 5, p[2] << 5], -1);
        }
    }
}

/// FUN_80063C24(kart): the push of the nearest jet when it is active and
/// the kart is inside its reach (after the ground contact, kart not on a
/// hook). +0x4E3 = jet / 3 when pushed, else -1. One random byte (signed)
/// per kart near an active jet.
pub fn jet_push(k: &mut KartState, g: &mut RaceGlobals, w: &World) {
    k.object_e3 = -1;
    let jets = &w.course.objects.jets;
    if jets.is_empty() {
        return;
    }
    let t = &w.tables;
    // Nearest by the first point, against the driver position (+0x330..).
    let (kx, ky, kz) = (k.drv_x >> 10, k.ref_y >> 10, k.drv_z >> 10);
    let mut best = i32::MAX;
    let mut n = 0usize;
    for (i, p) in jets.iter().enumerate() {
        let dx = (p[0][0] >> 5).wrapping_sub(kx);
        let dy = (p[0][1] >> 5).wrapping_sub(ky);
        let dz = (p[0][2] >> 5).wrapping_sub(kz);
        let d = dx.wrapping_mul(dx).wrapping_add(dy.wrapping_mul(dy)).wrapping_add(dz.wrapping_mul(dz));
        if d < best {
            best = d;
            n = i;
        }
    }
    let j = g.objects.jets[n];
    if j.active == 0 {
        return;
    }
    let r = div_pow2((j.d01 as i32).max(j.d02 as i32), 4);
    if best > r.wrapping_mul(r) {
        return;
    }
    let p = &jets[n];
    let [x0, y0, z0] = p[0];
    let [x1, y1, z1] = p[1];
    let a8 = (k.drv_x >> 5) - x0;
    let a5 = (k.drv_z >> 5) - z0;
    let a26 = -((k.ref_y >> 5) - y0);
    // The jet's axis: yaw u10 about x, then u12.
    let u10 = ratan2(t, z1 - z0, -(y1 - y0)) & 0xFFF;
    let c21 = t.cos((-u10) & 0xFFF);
    let s18 = t.sin((-u10) & 0xFFF);
    let i23 = div_pow2((-((k.drv_prev_y >> 5) - y0)).wrapping_mul(c21).wrapping_sub(((k.drv_prev_z >> 5) - z0).wrapping_mul(s18)), 12);
    let i4 = div_pow2(a26.wrapping_mul(c21).wrapping_sub(a5.wrapping_mul(s18)), 12);
    let i26 = div_pow2(a26.wrapping_mul(s18).wrapping_add(a5.wrapping_mul(c21)), 12);
    let i14 = div_pow2((-(y1 - y0)).wrapping_mul(c21).wrapping_sub((z1 - z0).wrapping_mul(s18)), 12);
    let u12 = (-ratan2(t, x1 - x0, i14)) & 0xFFF;
    let c7 = t.cos((-u12) & 0xFFF);
    let s5 = t.sin((-u12) & 0xFFF);
    let prev_along = div_pow2(((k.drv_prev_x >> 5) - x0).wrapping_mul(s5).wrapping_add(i23.wrapping_mul(c7)), 12);
    let across1 = div_pow2(a8.wrapping_mul(c7).wrapping_sub(i4.wrapping_mul(s5)), 12);
    let along = div_pow2(a8.wrapping_mul(s5).wrapping_add(i4.wrapping_mul(c7)), 12);
    let len = div_pow2((x1 - x0).wrapping_mul(s5).wrapping_add(i14.wrapping_mul(c7)), 12);
    if along <= -0x41 {
        return;
    }
    // 60 Hz mode: the byte is drawn once per pair.
    let b = g.replay_draws(|g| crate::update::rng_next(g, w)) as i8 as i32;
    let reach = len + div_pow2(len.wrapping_mul(b), 9);
    let behind = along < prev_along;
    let [x2, y2, z2] = p[2];
    let rr = (x2 - x0).wrapping_mul(x2 - x0).wrapping_add((y2 - y0).wrapping_mul(y2 - y0)).wrapping_add((z2 - z0).wrapping_mul(z2 - z0));
    if along > reach + 0x40 || across1.wrapping_mul(across1).wrapping_add(i26.wrapping_mul(i26)) > rr {
        return;
    }
    let mut f = div_pow2(reach - along.max(0), 3) + 0x60;
    if behind {
        f *= 2;
    }
    let h = f.wrapping_mul(t.cos_raw(u12)) >> 14;
    let px = div_pow2(((-(f.wrapping_mul(t.sin_raw(u12)))) >> 14).wrapping_mul(3), 2);
    let py = div_pow2((h.wrapping_mul(t.cos_raw(u10)) >> 14).wrapping_mul(3), 2);
    let pz = div_pow2((h.wrapping_mul(t.sin_raw(u10)) >> 14).wrapping_mul(3), 2);
    k.push_x = k.push_x.wrapping_add(px as i16);
    k.gravity_mod = k.gravity_mod.wrapping_sub(py as i16);
    k.push_z = k.push_z.wrapping_add(pz as i16);
    k.object_e3 = (n as i32 / 3) as i8;
}

/// The first loop of FUN_8007F03C (after the kart loop): an active jet
/// draws a random byte; under 0x40 and within 10000 TER units of a human
/// camera it puffs smoke (two rand(), a random direction FUN_8003C44C,
/// two more bytes, one particle of kind 0x14).
pub fn jets_smoke(g: &mut RaceGlobals, w: &World, cams: &crate::items::Cameras, rand: &mut dyn FnMut() -> i32) {
    let t = &w.tables;
    for i in 0..g.objects.jets.len() {
        if g.objects.jets[i].active != 1 {
            continue;
        }
        let b = crate::update::rng_next(g, w);
        if b >= 0x40 {
            continue;
        }
        let p = &w.course.objects.jets[i];
        if !cams.near(p[0][0], p[0][1], p[0][2], 10000) {
            continue;
        }
        let d02 = g.objects.jets[i].d02 as i32;
        let s = 0x8000 / (d02 / 2);
        let a = rand() / 8;
        let c = rand() / 8;
        let dir = random_dir(t, a, c);
        let b0 = crate::update::rng_next(g, w) as i8 as i32;
        let b1 = crate::update::rng_next(g, w) as i8 as i32;
        let size = s16((((b0 + if b0 < 0 { 3 } else { 0 }) as u32) >> 2) as i32 + d02 / 3);
        let pos = [p[0][0] * 0x20 + (dir[0] << 8) / s, p[0][1] * 0x20 + (dir[1] << 8) / s, p[0][2] * 0x20 + (dir[2] << 8) / s];
        let vel = [(p[1][0] - p[0][0]) / 2, (p[1][1] - p[0][1]) / 2, (p[1][2] - p[0][2]) / 2];
        crate::items::particle(g, pos, vel, 0, 0, b1 / 4, 0x14, [0x80, 0x80, 0x80], size, size, 0x40);
    }
}

/// FUN_8003C44C(a, b): a unit vector (4.12) from two angles.
pub fn random_dir(t: &Tables, a: i32, b: i32) -> [i32; 3] {
    // Indices are the i16 arguments (rand() / 8: 0..4095).
    let (a, b) = (s16(a), s16(b));
    let cb = t.cos_raw(b);
    [s16(-t.sin_raw(b) >> 2), s16((-(cb.wrapping_mul(t.cos_raw(a)))) >> 16), s16(cb.wrapping_mul(t.sin_raw(a)) >> 16)]
}

/// FUN_80084490's turntable part (course load): per turntable a phase
/// rand() / 8 and a spin direction (rand() < 0x4000: +5, else -5; x3 on
/// courses 0x1A / 0x1B).
pub fn turntables_load(w: &World, rand: &mut dyn FnMut() -> i32) -> (Vec<i16>, Vec<i16>) {
    let mut phase = Vec::new();
    let mut spin = Vec::new();
    for _ in &w.course.objects.turntables {
        phase.push((rand() / 8) as i16);
        let mut s: i16 = if rand() < 0x4000 { 5 } else { -5 };
        if (0x1A..=0x1B).contains(&w.info.id) {
            s *= 3;
        }
        spin.push(s);
    }
    (phase, spin)
}

/// rand() calls of FUN_80084490's per-course switch (course load, after
/// the turntables; jump table 0x80023378 indexed by course - 3): the
/// animated scenery's phases (render side). Race courses only.
pub fn course_load_switch_rands(id: u8) -> u32 {
    match id {
        0x0F => 3,  // 0x800855FC
        0x12 => 1,  // 0x80084B54
        0x1A | 0x1B => 6, // 0x80084688
        0x1F => 13, // 0x80085394
        _ => 0,
    }
}

/// FUN_800632BC(kart): +0x4E4 = -1; a grounded kart (+0x4E0) on a
/// turntable's ring (|y - centre y| <= 0x100 DAT units, inner radius <=
/// r < outer radius) is turned about the centre by the spin * dt and
/// +0x4E4 = the turntable.
pub fn turntable(k: &mut KartState, g: &RaceGlobals, w: &World) {
    k.object_e4 = -1;
    if k.ground_contact == 0 {
        return;
    }
    let t = &w.tables;
    for (i, p) in w.course.objects.turntables.iter().enumerate() {
        if ((k.prev_y >> 5) - p[0][1]).abs() >= 0x101 {
            continue;
        }
        let (cx, cz) = (p[0][0], p[0][2]);
        let a = (p[1][0] - cx) >> 5;
        let b = (p[1][2] - cz) >> 5;
        let c = (p[2][0] - cx) >> 5;
        let d = (p[2][2] - cz) >> 5;
        let rx = (k.x >> 10) - (cx >> 5);
        let rz = (k.z >> 10) - (cz >> 5);
        let r2 = rx.wrapping_mul(rx).wrapping_add(rz.wrapping_mul(rz));
        if a * a + b * b <= r2 && r2 < c * c + d * d {
            k.object_e4 = i as i8;
            let ang = (g.objects.turntable_spin[i] as i32).wrapping_mul(g.dt);
            let u = ang & 0xFFF;
            let (co, si) = (t.cos(u), t.sin(u));
            let ex = k.x.wrapping_sub(cx * 0x20);
            let ez = k.z.wrapping_sub(cz * 0x20);
            k.x = (cx * 0x20).wrapping_add(ex.wrapping_mul(co).wrapping_add(ez.wrapping_mul(si)) >> 12);
            k.heading = ((k.heading as i32 + ang) & 0xFFF) as i16;
            k.z = (cz * 0x20).wrapping_add(ez.wrapping_mul(co).wrapping_sub(ex.wrapping_mul(si)) >> 12);
            return;
        }
    }
}

/// RAM addresses of the run-time state.
pub mod addr {
    pub const JET_COUNT: u32 = 0x800B_0C58;
    pub const JETS: u32 = 0x800D_19C0;
    pub const TURNTABLE_COUNT: u32 = 0x800B_0B2A;
    pub const TURNTABLES: u32 = 0x800B_C648;
    pub const HOOK_GROUPS: u32 = 0x800B_0E00;
    pub const HOOKS: u32 = 0x800D_15A0;
}

impl ObjectState {
    /// The state as the game holds it in memory `m`.
    pub fn read(m: &crate::mem::Mem) -> ObjectState {
        let nj = m.i8(addr::JET_COUNT).max(0) as u32;
        let jets = (0..nj)
            .map(|i| {
                let a = addr::JETS + 12 * i;
                Jet { on: m.i16(a), off: m.i16(a + 2), d02: m.i16(a + 4), d01: m.i16(a + 6), active: m.u8(a + 8) }
            })
            .collect();
        let nt = m.u8(addr::TURNTABLE_COUNT) as u32;
        let turntable_phase = (0..nt).map(|i| m.i16(addr::TURNTABLES + 4 * i)).collect();
        let turntable_spin = (0..nt).map(|i| m.i16(addr::TURNTABLES + 4 * i + 2)).collect();
        let nh = 3 * m.i8(addr::HOOK_GROUPS).max(0) as u32;
        let hooks = (0..nh)
            .map(|i| {
                let a = addr::HOOKS + 0x2C * i;
                crate::hooks::Hook {
                    pos: [m.i32(a), m.i32(a + 4), m.i32(a + 8)],
                    trail: [m.i32(a + 0x10), m.i32(a + 0x14), m.i32(a + 0x18)],
                    phase: m.u16(a + 0x24),
                    wrapped: m.u8(a + 0x26),
                    carrier: m.i8(a + 0x27),
                }
            })
            .collect();
        let weather = if m.u8(0x800B_0ACA) != 0 {
            let course = m.i32(0x800B_0FAC) as u32;
            let snow = m.u16(0x800A_CB3A + 12 * course) & 0x20 != 0;
            let drops = std::array::from_fn(|i| {
                let a = 0x800D_75D0 + 0x2C * i as u32;
                crate::weather::Drop {
                    pos: [m.i32(a), m.i32(a + 4), m.i32(a + 8)],
                    ground: m.i32(a + 0xC),
                    speed: m.i32(a + 0x10),
                    timer: m.i16(a + 0x14),
                    life: m.i16(a + 0x16),
                    size: [m.u8(a + 0x1A), m.u8(a + 0x1B)],
                    on_water: m.u8(a + 0x1D),
                    rgb: [m.u8(a + 0x23), m.u8(a + 0x24), m.u8(a + 0x25)],
                    base: [m.u8(a + 0x26), m.u8(a + 0x27), m.u8(a + 0x28)],
                }
            });
            Some(crate::weather::Weather { snow, drops, next: m.u8(0x800B_0ECC) })
        } else {
            None
        };
        let nl = m.i8(0x800B_0E01).max(0) as u32;
        let lifts: Vec<crate::lifts::Lift> = (0..nl)
            .map(|i| {
                let a = 0x800D_D708 + 20 * i;
                crate::lifts::Lift {
                    cur: m.i32(a),
                    min: m.i32(a + 4),
                    max: m.i32(a + 8),
                    wait_min: m.i16(a + 0xC),
                    wait_max: m.i16(a + 0xE),
                    timer: m.i16(a + 0x10),
                    speed: m.i8(a + 0x12),
                    anim: m.u8(a + 0x13),
                }
            })
            .collect();
        // Each lift object has moved its groups by its lift's travel from
        // the low end (one object per lift on every race course).
        let course = m.i32(0x800B_0FAC);
        let lift_offsets = crate::lifts::objects(course as u8)
            .iter()
            .map(|o| if o.lift >= 0 && (o.lift as usize) < lifts.len() && o.group >= 0 { let l = lifts[o.lift as usize]; l.cur - l.min } else { 0 })
            .collect();
        // Breakable groups: a listed group is enabled unless its first face
        // or wall carries the disabled bit in RAM (TER at 0x800B0FA4 /
        // 0x800B0F5C, group table 0x800B0E44).
        let ng = m.i16(0x800B_0F50).max(0) as u32;
        let gt = m.u32(0x800B_0E44);
        let group_enabled = (0..ng)
            .map(|gi| {
                let a = gt + 0x14 * gi;
                let (fs, fc, ws, wc) = (m.u16(a + 6) as u32, m.i16(a + 0xC), m.u16(a + 8) as u32, m.i16(a + 0xE));
                if fc > 0 {
                    Some(m.u8(m.u32(0x800B_0FA4) + fs + 0xE) & 0x20 == 0)
                } else if wc > 0 {
                    Some(m.u16(m.u32(0x800B_0F5C) + ws) & 1 == 0)
                } else {
                    None
                }
            })
            .collect();
        ObjectState { jets, turntable_phase, turntable_spin, hooks, weather, lifts, lift_offsets, group_enabled, hidden_slots: Vec::new(), anim: crate::scenery::read(m) }
    }
}

/// FUN_80081BF4(list, first, last, phase, out, closed): the point at
/// `phase` (0..0x7FFF) along points first..=last of a DAT point list
/// (first == last: the whole list); `closed` (param 6) counts the step from
/// the last point back to the first. Linear between neighbours in 1/64
/// steps; world units (points << 5).
pub fn path_lerp(pts: &[[i32; 3]], first: i16, last: i16, phase: i32, closed: bool) -> [i32; 3] {
    let count = pts.len() as i32;
    if count == 0 {
        return [0; 3];
    }
    let mut a = (first.max(0)) as i32;
    let mut b = last as i32;
    if count <= b {
        b = count - 1;
    }
    if b < a {
        std::mem::swap(&mut a, &mut b);
    }
    let n;
    if a == b {
        // The whole list as a loop: count steps (the n of 0x80081C4C
        // stays the point count; param 6 is not added).
        a = 0;
        b = count - 1;
        n = count;
    } else {
        n = b - a + closed as i32;
    }
    let v = div_pow2((phase & 0x7FFF).wrapping_mul(n).wrapping_mul(0x40), 15);
    let i = a + div_pow2(v, 6);
    let f = v & 0x3F;
    let mut j = i + 1;
    if s16(j) == b + 1 {
        j = a;
    }
    let (p, q) = (pts[(i as usize).min(pts.len() - 1)], pts[(j as usize).min(pts.len() - 1)]);
    [0, 1, 2].map(|c| p[c].wrapping_mul(0x20).wrapping_add((q[c].wrapping_sub(p[c])).wrapping_mul(f) >> 1))
}

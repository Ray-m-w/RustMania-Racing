//! Race-load initialisation of the human player's kart: FUN_800609F4(kart,
//! character, vehicle, grid position, flags) as the race loader FUN_8008C4AC
//! calls it for player 1 (flags 0), with the vehicle template of vehicle.rs.
//! Builds the whole 0x504-byte record the way the game does (cleared, then
//! written field by field), so it can be compared with the oracle byte for
//! byte. Fields that only other subsystems use and that need code not ported
//! here are left 0 and listed in docs/spec/kart-physics-ledger.md.

use crate::camera::{TrailEntry, TRAIL_LEN};
use crate::fixed::div_pow2;
use crate::kart::{KartState, RECORD_SIZE};
use crate::tables::ratan2;
use crate::vehicle::{TEMPLATE_SIZE, KART_OFFSET};
use crate::world::World;

/// Race settings FUN_800609F4 reads.
#[derive(Clone, Copy, Debug)]
pub struct RaceSetup {
    pub mode: i16,
    pub race_type: i16,
    pub race_flags: u16,
    pub challenge: i16,
    pub humans: i16,
    /// 0x800B0DFE: karts in the race.
    pub karts: i16,
}

/// Exe addresses FUN_800609F4 reads.
pub mod addr {
    /// Identity matrix (MATRIX, 4.12) copied into the record's matrices.
    pub const IDENTITY: u32 = 0x800A_87B0;
    /// Character table, 4 bytes per character: (camera mount +0x188, +0x18A).
    /// The first halfword is rewritten at model load (FUN_80087ACC).
    pub const CHARACTER_MOUNT: u32 = 0x800A_C038;
    /// Position-history rings, 12 x 16 bytes per kart (BSS).
    pub const HISTORY: u32 = 0x800D_4098;
}

pub struct KartInit<'a> {
    pub world: &'a World,
    pub template: &'a [u8; TEMPLATE_SIZE],
    pub setup: RaceSetup,
    pub slot: u8,
    pub character: u8,
    pub vehicle: u8,
    pub grid: i32,
    pub flags: u16,
    /// The character model's first part pivot y (FUN_80087ACC stores its
    /// absolute value over the first halfword of the character's row of
    /// 0x800AC038 when the model is loaded).
    pub character_pivot_y: i32,
}

fn put16(r: &mut [u8], o: usize, v: i32) {
    r[o..o + 2].copy_from_slice(&(v as u16).to_le_bytes());
}
fn put32(r: &mut [u8], o: usize, v: i32) {
    r[o..o + 4].copy_from_slice(&v.to_le_bytes());
}
fn get16(r: &[u8], o: usize) -> i32 {
    i16::from_le_bytes([r[o], r[o + 1]]) as i32
}

/// FUN_8003CCBC: body pitch and roll that put the kart's up axis on the
/// surface normal (the same formula FUN_8003E91C uses each tick).
pub fn tilt_from_normal(w: &World, n: [i32; 3]) -> (i32, i32) {
    let t = &w.tables;
    let ab0 = (-ratan2(t, n[2], -n[1])) & 0xFFF;
    let c = (-n[1]).wrapping_mul(t.cos(ab0)).wrapping_sub(n[2].wrapping_mul(t.sin(ab0)));
    let ab4 = ratan2(t, n[0], div_pow2(c, 12)) & 0xFFF;
    (ab0, ab4)
}

/// FUN_800609F4. `rng` is the random byte pointer 0x800B0BCC, `rand` the
/// BIOS rand() (initial wheel angles only).
pub fn init_kart(p: &KartInit, rng: &mut u32, rand: &mut dyn FnMut() -> i32) -> (Vec<u8>, KartState) {
    let w = p.world;
    let t = &w.tables;
    let s = p.setup;
    let mut r = vec![0u8; RECORD_SIZE];
    let mut next_rng = || {
        let b = w.rng_byte(*rng);
        *rng = rng.wrapping_add(1);
        b
    };
    r[0x4D7] = p.slot;
    // 0x80060A3C: grid index. In a mode-1 race of type 0 or 1 (race A / B;
    // only types 2 and 3 offset the index) with no option bits the kart
    // takes grid position `grid`.
    // The title (mode 0x10, race type 2: 0x80060B04 takes 6 + 1 for the
    // index, then the plain path for flags 0x20 / 0x40 clear and race flags
    // 1/2/4 clear: index = `grid` again) [CODE 800609f4].
    // The stunt trial (mode 3, race type 3; 0x80060B3C): one kart
    // (0x800B0DFE == 1) takes grid point `grid` + 2.
    // A battle (mode 2, race type 2; phase 4): 0x80060B04 takes 6 + 1,
    // then the plain path again (flags 0x20 / 0x40 and race flags 1/2/4
    // clear, no challenge): index = `grid`.
    let title = s.mode == 0x10 && s.race_type == 2;
    let stunt = s.mode == 3 && s.race_type == 3;
    let battle = s.mode == 2 && s.race_type == 2;
    if !(s.mode == 1 && (0..=1).contains(&s.race_type) || title || stunt || battle) || s.race_flags & 7 != 0 || s.challenge != 0 || p.flags & 0x60 != 0 {
        crate::unreachable_in_scope!("grid placement outside a plain one-player race");
    }
    let gi = (p.grid + if stunt && s.karts == 1 { 2 } else { 0 }) as usize;
    let Some(g) = w.course.grid.get(gi) else {
        crate::unreachable_in_scope!("grid index past the course's grid (centre-of-course start)");
    };
    let (x, y0, z) = (g.pos[0] << 5, g.pos[1] << 5, g.pos[2] << 5);
    let heading = ratan2(t, g.pos[0] - g.behind[0], g.pos[2] - g.behind[2]) & 0xFFF;
    put32(&mut r, 0x000, x);
    put32(&mut r, 0x004, y0);
    put32(&mut r, 0x008, z);
    put16(&mut r, 0x00E, heading);
    // 0x80060C88: ground under the grid point (query at the grid y).
    let gq = w.ter.ground(x, y0, z);
    let ground_y = gq.nearest_y;
    put32(&mut r, 0x31C, ground_y);
    r[0x502] = gq.found as u8;
    put32(&mut r, 0x320, ground_y);
    match gq.nearest {
        None => {
            put16(&mut r, 0x00C, 0);
            put16(&mut r, 0x010, 0);
            put32(&mut r, 0x2E0, 0);
        }
        Some(h) => {
            let f = &w.ter.faces[h.face];
            let (pitch, roll) = tilt_from_normal(w, [f.normal[0] as i32, f.normal[1] as i32, f.normal[2] as i32]);
            put16(&mut r, 0x00C, pitch);
            put16(&mut r, 0x010, roll);
            put32(&mut r, 0x2E0, w.face_ptr(Some(h.face)) as i32);
        }
    }
    // FUN_8003CD7C on the new record (+0x470/+0x474 only depend on angles).
    {
        let mut k = KartState::from_record(&r);
        crate::ground::slope_steer(&mut k, t);
        put16(&mut r, 0x470, k.slope_roll as i32);
        put16(&mut r, 0x474, k.slope_turn as i32);
    }
    r[0x4EE] = 0xFF;
    let (s0, s6) = (i32::from_le_bytes(r[0x3A0..0x3A4].try_into().unwrap()), i32::from_le_bytes(r[0x3B8..0x3BC].try_into().unwrap()));
    put16(&mut r, 0x450, s0);
    put16(&mut r, 0x452, s6);
    let b1 = next_rng();
    put32(&mut r, 0x004, ground_y);
    for o in [0x4E3, 0x4E4, 0x4E6, 0x4E7, 0x4E8, 0x4D4, 0x4D5, 0x4D0, 0x4D1, 0x4D2, 0x4D3] {
        r[o] = 0xFF;
    }
    put32(&mut r, 0x2F0, z);
    put32(&mut r, 0x2E8, x);
    put16(&mut r, 0x47E, 0x60);
    r[0x4E0] = 1;
    r[0x4CC] = 1;
    r[0x4CD] = 1;
    r[0x4CE] = 1;
    r[0x4F4] = b1 & 3;
    put32(&mut r, 0x2EC, ground_y);
    r[0x4CF] = 1;
    for i in 0..18 {
        put16(&mut r, 0x414 + 2 * i, 0x1000);
    }
    r[0x4F1] = 0xFF;
    if (p.slot as i16) < s.humans {
        r[0x4F2] = p.slot;
    }
    let b2 = next_rng();
    put16(&mut r, 0x2D2, -0x1000);
    r[0x4DA] = b2 >> 6;
    // 0x80060EA0..: seven + two matrices set to the identity (render side).
    let ident = w.exe.bytes(addr::IDENTITY, 18);
    for i in 0..7 {
        r[0x20C + 0x12 * i..0x20C + 0x12 * i + 18].copy_from_slice(&ident);
    }
    for i in 0..2 {
        r[0x29C + 0x12 * i..0x29C + 0x12 * i + 18].copy_from_slice(&ident);
    }
    put16(&mut r, 0x44C, 1);
    r[0x4FE] = 0xFF;
    r[0x4F9] = 0x80;
    put16(&mut r, 0x498, -1);
    put16(&mut r, 0x49A, -1);
    put16(&mut r, 0x496, p.flags as i32);
    if s.mode == 1 {
        r[0x4DF] = ((s.race_flags >> 1) & 1) as u8;
    } else {
        // 0x80061098 (outside mode 1: the title): a random byte's top bit.
        r[0x4DF] = next_rng() >> 7;
    }
    if s.race_type == 2 || s.race_flags & 1 != 0 {
        // 0x800610C4: battle AI (FUN_80090FAC) bit and its first timer,
        // (rand() / 5462 + 5) * 60 VBlanks (+0x4A2).
        put16(&mut r, 0x496, (p.flags | 8) as i32);
        let t = (crate::fixed::s16(rand() / 0x1556) + 5) * 0x3C;
        put16(&mut r, 0x4A2, t);
    }
    // FUN_8009004C: path position.
    {
        let mut k = KartState::from_record(&r);
        crate::progress::path_position(&mut k, w);
        k.to_record(&mut r);
    }
    r[0x4B4] = 8;
    put16(&mut r, 0x442, 0x1000);
    // Body matrix +0x18C = identity, copied to +0x1AC and +0x1CC.
    r[0x18C..0x18C + 18].copy_from_slice(&ident);
    let m: Vec<u8> = r[0x18C..0x1AC].to_vec();
    r[0x1AC..0x1CC].copy_from_slice(&m);
    r[0x1CC..0x1EC].copy_from_slice(&m);
    r[0x4FC] = 0xFF;
    r[0x4F8] = 0xFF;
    put16(&mut r, 0x412, 1 << (p.slot & 31));
    // Any vehicle: `p.template` is its row of the template table (vehicle.rs
    // for the RACER, ai::vehicle_templates + ai::place_wheels for all).
    r[0x4B1] = p.vehicle;
    r[0x4B2] = p.character;
    // (0x4A4 / 0x4A5: model and character slot numbers, loader state.)
    r[KART_OFFSET..KART_OFFSET + TEMPLATE_SIZE].copy_from_slice(p.template);
    // Camera mount pair from the character table: 4 bytes per character
    // (0x80061314 `sll v0, v0, 2`; Ghidra shows `* 2` on a halfword
    // pointer). Was `2 * character`, which only Kermit (0) survived.
    let mut cm = w.exe.bytes(addr::CHARACTER_MOUNT + 4 * p.character as u32, 4);
    cm[0..2].copy_from_slice(&(p.character_pivot_y.unsigned_abs() as u16).to_le_bytes());
    r[0x188..0x18C].copy_from_slice(&cm);
    put16(&mut r, 0x178, 0x40);
    let stat = |i: usize| get16(p.template, 0x166 + 2 * i);
    put16(&mut r, 0x17A, (stat(0) * 0x4B) / 100);
    put16(&mut r, 0x17C, (stat(1) * 0x60) / 100);
    put16(&mut r, 0x17E, (stat(2) * 0x24) / 100);
    put16(&mut r, 0x180, (stat(3) << 7) / 100);
    for i in 0..r[0x170] as usize {
        let b = 0x14 + 0x2C * i;
        if u16::from_le_bytes([r[b + 0x26], r[b + 0x27]]) & 0xC0 == 0 {
            put16(&mut r, b + 0x1E, div_pow2(rand(), 3));
        }
    }
    put16(&mut r, 0x2D8, -0x1000);
    r[0x175] = 4;
    r[0x4BE] = (p.grid + 1) as u8;
    r[0x4CA] = (p.grid + 1) as u8;
    let probe_top = (get16(&r, 0x184) - get16(&r, 0x188) - get16(&r, 0x18A)) * 0x20;
    put16(&mut r, 0x438, probe_top);
    put32(&mut r, 0x344, z);
    put32(&mut r, 0x340, z);
    put32(&mut r, 0x334, x);
    put32(&mut r, 0x330, x);
    let yr = ground_y + get16(&r, 0x184) * 0x20;
    put32(&mut r, 0x33C, yr);
    put32(&mut r, 0x338, yr);
    r[0x4C1] = 1;
    r[0x4D8] = 0xFF;
    r[0x4C0] = if s.race_flags & 2 != 0 { 7 } else { 6 };
    r[0x4B8] = 0xFF;
    // (0x490 = FUN_8006F410 zone number: not ported.)
    r[0x4A9] = 0x40;
    let mut k;
    if (p.slot as i16) < s.karts {
        let hp = addr::HISTORY + (16 * crate::camera::GAME_TRAIL_LEN) as u32 * p.slot as u32;
        put32(&mut r, 0x2DC, hp as i32);
        let h = heading;
        let hx = x + t.sin_raw(h) * -2;
        let hz = z + t.cos_raw(h) * -2;
        let hy = w.ter.ground(hx, ground_y, hz).nearest_y;
        k = KartState::from_record(&r);
        k.trail = [TrailEntry { pos: [hx, hy, hz], count: 2 }; TRAIL_LEN];
    } else {
        k = KartState::from_record(&r);
    }
    // +0x488: FUN_80081170(x >> 5, y >> 5, z >> 5, course, 1, 0, 0): the
    // nearest course point over the whole list (Session 3 searched from
    // point 0 with mode 0x10's bisection; equal on DOCK, not on SPEEDWAY).
    // No course points (the stunt courses): -1.
    k.course_point = crate::race::course_point_all(w, &k).unwrap_or(-1);
    // 0x800611xx: the strip quad (FUN_8006F410; 0 without a strip) and
    // +0x4A9 = 0x40, on every course.
    crate::stunt::init_kart(&mut k, w);
    k.to_record(&mut r);
    (r, k)
}

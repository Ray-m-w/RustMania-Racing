//! Weapons: L1 / L2 fire the two weapon slots (FUN_800645F4 -> FUN_80068C6C,
//! inside the kart update) and the four projectiles fly, bounce and hit
//! (FUN_80069648, after the kart loop).
//!
//! A slot (+0x4D0/+0x4D1 L1, +0x4D2/+0x4D3 L2: current / next type; charge
//! +0x4D4 / +0x4D5) is filled by an item box (pickups.rs), charges to 0x40
//! in the kart update while its type cycles, and is emptied (0xFF) when
//! fired. The weapon type gives the projectile's flags:
//! type 0: 0x1D, 1: 0x280, 2: 0x588, 3: 0x78 [CODE 0x80064650..0x800646A4].
//! Flag bits as FUN_80068C6C / FUN_80069648 use them:
//!   0x01 launched up / lifetime 180, re-spins on a bounce
//!   0x04 bounces off floors (FUN_80041E10 reflection) instead of stopping
//!   0x08 bounces off walls instead of exploding
//!   0x10 gravity
//!   0x20 dropped behind (inherits the kart's velocity), lifetime 300,
//!        slides along the floor normal
//!   0x40 hops at random, no impact sound, short probe
//!   0x80 hovers 0x2000 above the ground
//!   0x100 follows the TER path (lifetime 240)
//!   0x200 / 0x400 trail particles; 0x400 also a looping sound

use super::{ItemEvent, Projectile, PROJECTILES};
use crate::fixed::{div, div_pow2, mul, s16, substeps};
use crate::globals::RaceGlobals;
use crate::kart::KartState;
use crate::tables::square_root0;
use crate::update::{rng_next, rumble, Others};
use crate::world::World;

/// Projectile flags for weapon types 0..3 [CODE 0x80064650..0x800646A4].
pub fn weapon_flags(t: i8) -> Option<u16> {
    match t {
        0 => Some(0x1D),
        1 => Some(0x280),
        2 => Some(0x588),
        3 => Some(0x78),
        _ => None,
    }
}

/// FUN_800645F4(kart, slot): L1 (slot 0) / L2 (slot 1) held. Fires the slot
/// when it is charged (0x40), else only a "not ready" UI sound request.
pub fn fire_button(k: &mut KartState, slot_kart: usize, others: &mut Others, g: &mut RaceGlobals, w: &World, slot: usize, rand: &mut dyn FnMut() -> i32) {
    if k.engine < 0x20 {
        return;
    }
    if k.weapons[4 + slot] != 0x40 {
        let latch = if slot == 0 { k.latch_l1 } else { k.latch_l2 };
        if latch == 0 && k.flags & 1 == 0 {
            g.sound_event = slot as u8 + 1;
        }
    } else {
        let t = k.weapons[2 * slot] as i8;
        if let Some(f) = weapon_flags(t) {
            let p = fire(k, slot_kart, others, g, w, f, rand);
            g.items.events.push(ItemEvent::Fired { kart: slot_kart as u8, slot: slot as u8, weapon: t as u8, projectile: p });
        }
        k.weapons[2 * slot] = 0xFF;
        k.weapons[4 + slot] = 0xFF;
    }
    if slot == 0 {
        k.latch_l1 = 1;
    } else {
        k.latch_l2 = 1;
    }
}

/// FUN_8007D58C(x, y, z, m): (-x, y, -z) through the 4.12 matrix, >> 7
/// (0x800DFD88..0x800DFD90).
pub fn rotate7(x: i32, y: i32, z: i32, m: &[[i16; 3]; 3]) -> [i32; 3] {
    let (x, z) = (x.wrapping_neg(), z.wrapping_neg());
    let r = |row: &[i16; 3]| {
        mul(x, row[0] as i32).wrapping_add(mul(y, row[1] as i32)).wrapping_add(mul(z, row[2] as i32)) >> 7
    };
    [r(&m[0]), r(&m[1]), r(&m[2])]
}

fn path_point(w: &World, seg: i16, pt: i16) -> [i32; 3] {
    let s = &w.ter.path_segments[seg as u16 as usize];
    let p = &w.ter.path_points[s.first.wrapping_add(pt) as u16 as usize];
    [p.x, p.y, p.z]
}

/// The explosion of a live projectile (lifetime over, wall, replaced):
/// rumble on the owner's pad and the particle's random draw.
fn explode(i: usize, k_owner: Option<&mut KartState>, g: &mut RaceGlobals, w: &World) {
    if let Some(o) = k_owner {
        rumble(o, g, 0x4B);
    }
    // FUN_800870E8 explosion particle: angle = *rng++ << 4.
    let b = rng_next(g, w) as i32;
    let pos = g.items.projectiles[i].pos;
    let spin = g.items.projectiles[i].spin_rate as i32;
    super::particle(g, pos, [0; 3], 0, b << 4, spin, 0x11, [0x80; 3], 0x180, 0x180, 0x20);
    crate::sound::at(g, w, 0x14, 0x1600, 0x80, pos, -1);
    g.items.events.push(ItemEvent::Explode { projectile: i as u8, pos });
}

/// FUN_80068C6C(kart, flags): launch from the next projectile slot
/// (0x800B07E0, round robin; a live projectile there explodes first).
/// Returns the slot used.
pub fn fire(k: &mut KartState, slot_kart: usize, others: &mut Others, g: &mut RaceGlobals, w: &World, flags: u16, rand: &mut dyn FnMut() -> i32) -> u8 {
    let i = g.items.next_projectile as usize % PROJECTILES;
    if g.items.projectiles[i].active {
        let owner = g.items.projectiles[i].owner as usize;
        if owner == slot_kart {
            explode(i, Some(k), g, w);
        } else {
            let o = others.get_mut(owner);
            explode(i, o, g, w);
        }
    }
    rumble(k, g, 0x7D);
    let t = &w.tables;
    let mut p = g.items.projectiles[i];
    p.flags = flags;
    p.pos = [k.x, k.look_y, k.z];
    p.ground_y = k.look_y;
    let e = k.engine as i32;
    let d;
    if flags & 0x20 == 0 {
        let (y, z) = if flags & 1 == 0 { (0, 0x80) } else { (-0x40, 0x70) };
        let v = rotate7(0, y, z, &k.vmat);
        p.vel = [div(v[0] << 12, e), div(v[1] << 12, e), div(v[2] << 12, e)];
        d = 0x10;
    } else {
        let v = rotate7(0, 0, -0x20, &k.vmat);
        p.vel = [
            v[0].wrapping_add(div(k.vx << 11, e)),
            v[1].wrapping_add(div(k.vy << 11, e)),
            v[2].wrapping_add(div(k.vz << 11, e)),
        ];
        d = 4;
    }
    // The kart recoils.
    k.push_x = k.push_x.wrapping_sub(div(p.vel[0], d) as i16);
    k.gravity_mod = k.gravity_mod.wrapping_sub(div(p.vel[1], d) as i16);
    k.push_z = k.push_z.wrapping_sub(div(p.vel[2], d) as i16);
    p.owner = k.player_index as u8;
    p.b2d = 0;
    p.active = true;
    p.age = 0;
    p.sound_timer = 0;
    p.dir = if (k.wobble as i32).abs() < 0x400 { 1 } else { -1 };
    p.life = if flags & 0x20 != 0 {
        300
    } else if flags & 1 != 0 {
        180
    } else if flags & 0x100 != 0 {
        240
    } else {
        120
    };
    crate::sound::at(g, w, if flags & 0x40 != 0 { 0x17 } else { 0x13 }, 0x1600, 0x60, p.pos, -1);
    p.on_floor = 0;
    p.on_alt = 0;
    p.b31 = 0;
    let wk = if p.dir > 0 {
        crate::ai::walk_forward(w, k.path_seg, k.path_pt, 2, false, rand)
    } else {
        crate::ai::walk_backward(w, k.path_seg, k.path_pt, 2, rand)
    };
    p.seg = wk.seg;
    // (With no path the game keeps the walk's stale point global here; the
    // point is never read then.)
    p.pt = wk.pt;
    let r0 = rng_next(g, w);
    let r1 = rng_next(g, w);
    p.spin = (r0 as i16) << 4;
    p.spin_rate = if r1 < 0x80 { 0x40 } else { -0x40 };
    p.chase = [p.pos[0], p.pos[2]];
    p.face_ptr = k.floor_face_ptr;
    if p.seg == -1 {
        p.target = [p.pos[0], p.pos[2]];
    } else {
        let q = path_point(w, p.seg, p.pt);
        p.target = [q[0] << 5, q[2] << 5];
    }
    g.items.projectiles[i] = p;
    let n = g.items.next_projectile.wrapping_add(1);
    g.items.next_projectile = if n == 4 { 0 } else { n };
    let _ = t;
    i as u8
}

/// FUN_80041E10(v, n): reflect `v` off the unit (4.12) normal `n` when it
/// moves into it (0x800B0EDC..0x800B0EE4); otherwise `v` unchanged.
pub fn reflect(t: &crate::tables::Tables, v: [i32; 3], n: [i16; 3]) -> [i32; 3] {
    let len = square_root0(t, mul(v[0], v[0]).wrapping_add(mul(v[1], v[1])).wrapping_add(mul(v[2], v[2]))) + 1;
    let ux = s16(div(v[0] << 12, len));
    let uy = s16(div(v[1] << 12, len));
    let uz = s16(div(v[2] << 12, len));
    let (nx, ny, nz) = (n[0] as i32, n[1] as i32, n[2] as i32);
    let d = div_pow2(mul(-ux, nx).wrapping_add(mul(-uy, ny)).wrapping_add(mul(-uz, nz)), 12);
    if d < 0 {
        return v;
    }
    let rx = div_pow2(mul(nx, d), 11) + ux;
    let ry = div_pow2(mul(ny, d), 11) + uy;
    let rz = div_pow2(mul(nz, d), 11) + uz;
    [div_pow2(mul(rx, len), 12), div_pow2(mul(ry, len), 12), div_pow2(mul(rz, len), 12)]
}

/// Three wall / floor sparks (FUN_800326B4) and, for flag 1, a new spin
/// rate: their random draws.
fn sparks(p: &mut Projectile, g: &mut RaceGlobals, w: &World, respin: bool, dir: Spray) {
    for _ in 0..3 {
        let r0 = rng_next(g, w) as i8 as i32;
        let r1 = rng_next(g, w) as i8 as i32;
        let r2 = rng_next(g, w) as i8 as i32;
        let vel = match dir {
            Spray::Wall(ax, az) => [ax + r0 * 2, r1 << 1, az + r2 * 2],
            Spray::Floor => [r0 << 1, r1 * 2 - 0x100, r2 << 1],
            Spray::Ceiling => [r0 << 1, r1 * 2 + 0x100, r2 << 1],
        };
        super::spark(g, p.pos, vel, 0x18, 0x10, [0xFF, 0x7F, 0]);
    }
    if respin {
        p.spin_rate = rng_next(g, w) as i8 as i16;
    }
}

/// Which way a bounce's sparks fly: off a wall (its normal * 0x100), up
/// from a floor, down from a ceiling.
#[derive(Clone, Copy)]
enum Spray {
    Wall(i32, i32),
    Floor,
    Ceiling,
}

/// FUN_80069648: every live projectile, after the kart loop.
pub fn frame(karts: &mut [KartState], g: &mut RaceGlobals, w: &World, rand: &mut dyn FnMut() -> i32) {
    for i in 0..PROJECTILES {
        step(i, karts, g, w, rand);
    }
}

fn step(i: usize, karts: &mut [KartState], g: &mut RaceGlobals, w: &World, rand: &mut dyn FnMut() -> i32) {
    if !g.items.projectiles[i].active {
        return;
    }
    let t = &w.tables;
    let dt = g.dt;
    {
        let p = &mut g.items.projectiles[i];
        let age = s16(p.age as i32 + dt);
        p.age = age as i16;
        if (p.life as i32) < age {
            let o = p.owner as usize;
            explode(i, karts.get_mut(o), g, w);
            g.items.projectiles[i].active = false;
            return;
        }
    }
    let mut p = g.items.projectiles[i];
    let snd = s16(p.sound_timer as i32 - dt);
    p.sound_timer = snd as i16;
    p.spin = ((p.spin as i32).wrapping_add(mul(p.spin_rate as i32, dt)) & 0xFFF) as i16;
    if snd < 0 {
        p.sound_timer = 0;
    }
    let was_floor = p.on_floor;
    let was_alt = p.on_alt;
    // 60 Hz mode: the random hop, the whine and the trail are tried once
    // per pair (their per-second rates stay the game's).
    let beat = g.hz60.beat();
    if p.flags & 0x40 != 0 && p.grounded != 0 && beat && rng_next(g, w) < 0x20 {
        p.vel[1] = -0x540;
        crate::sound::at(g, w, 0x15, 0x1600, 0x80, p.pos, -1);
    }
    if p.flags & 0x400 != 0 && p.sound_timer == 0 && beat && rng_next(g, w) < 0x20 {
        // The whine, again in 30..93 VBlanks.
        crate::sound::at(g, w, 0xB, 0x1600, 0xA0, p.pos, -1);
        p.sound_timer = ((rng_next(g, w) >> 2) as i16) + 0x1E;
    }
    let old = p.pos;
    for _ in 0..substeps(dt) {
        if p.flags & 0x100 != 0 && p.seg != -1 && g.mode == 1 {
            let dx = (p.target[0].wrapping_sub(p.chase[0])) >> 5;
            let dz = (p.target[1].wrapping_sub(p.chase[1])) >> 5;
            if square_root0(t, mul(dx, dx).wrapping_add(mul(dz, dz))) < 0xA0 {
                // Next path point (FUN_80090424 / FUN_80090670, one point).
                let wk = if p.dir > 0 {
                    crate::ai::walk_forward(w, p.seg, p.pt, 1, false, rand)
                } else {
                    crate::ai::walk_backward(w, p.seg, p.pt, 1, rand)
                };
                let q = path_point(w, wk.seg, wk.pt);
                p.target = [q[0] << 5, q[2] << 5];
                p.seg = wk.seg;
                p.pt = wk.pt;
            }
            let dx = (p.target[0].wrapping_sub(p.chase[0])) >> 4;
            let dz = (p.target[1].wrapping_sub(p.chase[1])) >> 4;
            let n = square_root0(t, mul(dx, dx).wrapping_add(mul(dz, dz))) + 1;
            p.chase[0] = p.chase[0].wrapping_add(mul(div(dx << 12, n), 0xA0) >> 7);
            p.chase[1] = p.chase[1].wrapping_add(mul(div(dz << 12, n), 0xA0) >> 7);
            p.vel[0] = p.vel[0].wrapping_add(div_pow2(p.chase[0].wrapping_sub(p.pos[0]), 6));
            p.vel[2] = p.vel[2].wrapping_add(div_pow2(p.chase[1].wrapping_sub(p.pos[2]), 6));
        }
        if p.flags & 0x20 != 0 {
            if p.grounded != 0 {
                let f = w.face_index(p.face_ptr).expect("projectile on the ground without a face");
                let nrm = w.ter.faces[f].normal;
                p.vel[0] = p.vel[0].wrapping_add(div_pow2(nrm[0] as i32, 5));
                p.vel[2] = p.vel[2].wrapping_add(div_pow2(nrm[2] as i32, 5));
            }
            p.vel[0] = div_pow2(mul(p.vel[0], 0x7F), 7);
            p.vel[2] = div_pow2(mul(p.vel[2], 0x7F), 7);
        }
        let clamp = |v: i32, lim: i32| -> i32 {
            if v > lim {
                v + div_pow2(lim - v, 2)
            } else if v < -lim {
                v + div_pow2(-lim - v, 2)
            } else {
                v
            }
        };
        p.vel[0] = clamp(p.vel[0], 0x1000);
        p.vel[1] = clamp(p.vel[1], 0x1800);
        p.vel[2] = clamp(p.vel[2], 0x1000);
        if p.flags & 0x10 != 0 {
            p.vel[1] = p.vel[1].wrapping_add(0x60);
        }
        if p.flags & 0x80 != 0 {
            let target = p.ground_y.wrapping_sub(0x2000);
            let y = p.pos[1];
            p.vel[1] = p.vel[1].wrapping_add(div_pow2(target.wrapping_sub(y), 7));
            let vy = p.vel[1];
            let keeps = if y < target {
                Some(y.wrapping_add(vy) < target)
            } else if target < y {
                Some(target < y.wrapping_add(vy))
            } else {
                None
            };
            if keeps == Some(false) {
                // Crossing the hover height: damped.
                p.vel[1] = div_pow2(vy.wrapping_mul(3), 2);
            }
        }
        p.pos[0] = p.pos[0].wrapping_add(p.vel[0]);
        p.pos[1] = p.pos[1].wrapping_add(p.vel[1]);
        p.pos[2] = p.pos[2].wrapping_add(p.vel[2]);
    }
    // Probe span and wall bounce strength (0x800B0F62 / 0x800B0DA4 / 0x800B0C30).
    let (top, bottom, bounce) = if p.flags & 0x40 != 0 {
        (-0x2000, 0, 8)
    } else if p.flags & 0x600 != 0 {
        (-0x2000, 0x2000, 8)
    } else {
        (-0x1000, 0x1000, 0x10)
    };
    let q = crate::ground::WallQuery { heading: 0, probe_top: top, probe_bottom: bottom, vx: p.vel[0], vz: p.vel[2], bounce, dt };
    if let Some(h) = crate::ground::wall_test(t, &w.ter, &q, old, p.pos) {
        // FUN_8003C568: the wall's TER group; a breakable one breaks and
        // takes the projectile (FUN_80069334, breakables.rs). The game then
        // skips the ground step and the kart hits (0x80069EE0).
        let grp = crate::breakables::wall_group(&w.ter, h.wall);
        if grp != -1 && crate::breakables::hit(g, w, grp, p.pos, p.spin_rate as i32) {
            p = broke(p, karts, g, w);
            g.items.projectiles[i] = p;
            return;
        } else if p.flags & 8 == 0 {
            p.active = false;
            g.items.projectiles[i] = p;
            let o = p.owner as usize;
            explode(i, karts.get_mut(o), g, w);
            p = g.items.projectiles[i];
        } else {
            p.pos[0] = h.x;
            p.vel[2] = h.vz;
            p.vel[0] = h.vx;
            p.pos[2] = h.z;
            let respin = p.flags & 1 != 0;
            // The wall's direction (0x800B0ED0 / 0x800B0ED8: its points).
            let wl = w.ter.walls[h.wall];
            let (a, b) = (w.ter.wall_points[wl.p1 as usize], w.ter.wall_points[wl.p0 as usize]);
            let (nz, nx) = (a.z.wrapping_sub(b.z), b.x.wrapping_sub(a.x));
            let len = square_root0(t, mul(nz, nz).wrapping_add(mul(nx, nx))) + 1;
            sparks(&mut p, g, w, respin, Spray::Wall(div(nz << 8, len), div(nx << 8, len)));
            if p.flags & 0x40 == 0 {
                crate::sound::at(g, w, 0x21, 0x1600, 0x80, p.pos, -1);
            }
        }
    }
    // Ground (FUN_8003DDB0 at the new position).
    let gq = w.ter.ground(p.pos[0], p.pos[1], p.pos[2]);
    let top_y = p.pos[1].wrapping_add(top);
    let bot_y = p.pos[1].wrapping_add(bottom);
    p.on_floor = 0;
    p.on_alt = 0;
    if let Some(near) = gq.nearest {
        let mut alt_hit = false;
        if let Some(alt) = gq.alt {
            if alt.y < bot_y {
                alt_hit = true;
                if top_y < alt.y {
                    // Under a FACE_ALT surface (a ceiling): pushed below it.
                    p.pos[1] = alt.y.wrapping_sub(top);
                    if was_alt == 0 {
                        let n = w.ter.faces[alt.face].normal;
                        if p.flags & 4 == 0 {
                            p.on_alt = 1;
                        } else {
                            p.on_alt = 1;
                            p.vel = reflect(t, p.vel, n);
                            sparks(&mut p, g, w, true, Spray::Ceiling);
                        }
                        if p.flags & 0x40 == 0 {
                            crate::sound::at(g, w, 0x21, 0x1600, 0x80, p.pos, -1);
                        }
                    }
                    if p.flags & 4 == 0 {
                        p.vel[1] = div(p.pos[1].wrapping_sub(old[1]), dt);
                    }
                }
                let below = gq.below.unwrap_or(near);
                p.ground_y = below.y;
                p.face_ptr = w.face_ptr(Some(below.face));
            }
        }
        if !alt_hit || top_y < gq.nearest_y {
            if gq.nearest_y < bot_y {
                p.grounded = 1;
                p.pos[1] = gq.nearest_y.wrapping_sub(bottom);
                if was_floor == 0 {
                    if p.flags & 4 == 0 {
                        p.on_floor = 1;
                    } else {
                        let n = w.ter.faces[near.face].normal;
                        p.vel = reflect(t, p.vel, n);
                        p.on_floor = 1;
                        sparks(&mut p, g, w, true, Spray::Floor);
                    }
                    if p.flags & 0x40 == 0 {
                        crate::sound::at(g, w, 0x21, 0x1600, 0x80, p.pos, -1);
                    }
                }
                if p.flags & 4 == 0 {
                    p.vel[1] = div(p.pos[1].wrapping_sub(old[1]), dt);
                }
            } else {
                p.grounded = 0;
            }
            p.ground_y = gq.nearest_y;
            p.face_ptr = w.face_ptr(Some(near.face));
        }
        if p.ground_y < bot_y {
            p.pos[1] = p.ground_y.wrapping_sub(bottom);
        }
    }
    g.items.projectiles[i] = p;
    // Karts hit (even by a projectile that exploded on a wall this frame).
    let n = karts.len();
    for j in 0..n {
        let k = &karts[j];
        if !(k.special == 0 && (k.pickup == -1 || k.wall_flag != 0) && k.item_e7 == -1) {
            continue;
        }
        if j as u16 == p.owner as u16 {
            continue;
        }
        let near = |a: i32, b: i32| (a.wrapping_sub(b).wrapping_add(0x4000) as u32) < 0x8000;
        if !(near(p.pos[0], k.drv_x) && near(p.pos[1], k.ref_y) && near(p.pos[2], k.drv_z)) {
            continue;
        }
        hit(i, j, karts, g, w);
        break;
    }
    let p = g.items.projectiles[i];
    if p.active {
        // FUN_800861F0 shadow (render); trail particles.
        if g.hz60.beat() && rng_next(g, w) < 0x80 {
            let c = ((rng_next(g, w) >> 2) + 0x40) as i32;
            let v = [div_pow2(p.vel[0], 3), div_pow2(p.vel[1], 3), div_pow2(p.vel[2], 3)];
            if p.flags & 0x600 != 0 {
                let r0 = rng_next(g, w) as i8 as i32;
                let r1 = rng_next(g, w) as i8 as i32;
                let r2 = rng_next(g, w) as i8 as i32;
                let pos = [p.pos[0] + r0 * 0x20, p.pos[1] + r1 * 0x20, p.pos[2] + r2 * 0x20];
                if p.flags & 0x200 != 0 {
                    super::particle(g, pos, v, 8, 0, 0, 0xF, [0, 0x80, 0x80], c, c, 0x20);
                } else {
                    let a = (rng_next(g, w) as i32) << 4;
                    let s = div_pow2(rng_next(g, w) as i8 as i32, 1);
                    super::particle(g, pos, v, 8, a, s, 9, [0x80, 0x80, 0x80], c, c, 0x20);
                }
            }
        }
    }
}

/// A projectile that broke a breakable group (FUN_80069334 returned 1:
/// it is spent, the owner's pad rumbles 0x96 there): one random byte sets
/// the owner's +0x4DD event bit (8 below 0x55, 4 below 0xAA, else 2)
/// (0x80069EE0 / 0x8006A568).
fn broke(mut p: Projectile, karts: &mut [KartState], g: &mut RaceGlobals, w: &World) -> Projectile {
    p.active = false;
    if let Some(ow) = karts.get_mut(p.owner as usize) {
        rumble(ow, g, 0x96);
    }
    let b = rng_next(g, w);
    if let Some(ow) = karts.get_mut(p.owner as usize) {
        ow.event_bits |= if b < 0x55 { 8 } else if b < 0xAA { 4 } else { 2 };
    }
    p
}

/// A projectile reached kart `j` (0x8006A948..0x8006AEAC).
fn hit(i: usize, j: usize, karts: &mut [KartState], g: &mut RaceGlobals, w: &World) {
    g.items.projectiles[i].active = false;
    let p = g.items.projectiles[i];
    let o = p.owner as usize;
    {
        let v = &mut karts[j];
        crate::collide::spin_out(v, g, w, 1);
        v.vx = v.vx / 2 + p.vel[0] / 2;
        v.vy = v.vy / 2 - 0x4CC;
        v.vz = v.vz / 2 + p.vel[2] / 2;
    }
    let owner_finished = karts.get(o).map(|k| k.finished).unwrap_or(0);
    let karts_n = g.karts as i32;
    if karts[j].finished == 0
        && owner_finished == 0
        && ((karts[j].player_index as i32) < karts_n || karts.get(o).map(|k| (k.player_index as i32) < karts_n).unwrap_or(false))
    {
        if g.mode == 2 || g.mode == 7 || g.race_flags & 8 != 0 {
            // 0x8006AB18: a hit deals 100 damage (race flag 8 / battle).
            let v = &mut karts[j];
            let old = v.impact;
            v.stun = v.stun.wrapping_add(100);
            v.impact = v.impact.wrapping_add(100);
            let ko = old < 1000 && 999 < v.impact;
            v.flash_timer = 10;
            v.blink_count = 0;
            v.hit_dir = -100;
            if let Some(ow) = karts.get_mut(o) {
                ow.hits_dealt = ow.hits_dealt.wrapping_add(100);
                ow.flash_timer = 10;
                ow.blink_count = 0;
                ow.hit_dir = 100;
                if ko {
                    ow.knockouts = ow.knockouts.wrapping_add(1);
                    ow.flash_timer = 0x10;
                    ow.blink_count = 0;
                }
            }
        }
        if g.mode == 1 {
            if let Some(ow) = karts.get_mut(o) {
                ow.contacts_from_above = ow.contacts_from_above.wrapping_add(1);
            }
            let v = &mut karts[j];
            v.contacts_below = v.contacts_below.wrapping_add(1);
            if matches!(g.race_flags, 0 | 2 | 0x10) {
                if let Some(ow) = karts.get_mut(o) {
                    // "+1 HIT" on the shooter's HUD.
                    ow.flash_timer = 0xF;
                    ow.blink_count = 0;
                    ow.hit_dir = 1;
                }
                let v = &mut karts[j];
                // "-1 HIT" on the victim's.
                v.flash_timer = 0xF;
                v.blink_count = 0;
                v.hit_dir = -1;
            }
        }
        if g.mode == 20 {
            crate::unreachable_in_scope!("mode 20 projectile hit");
        }
    }
    // Voice requests (event bits +0x4DD): the shooter's and the victim's.
    let r = rng_next(g, w) as i8;
    if let Some(ow) = karts.get_mut(o) {
        ow.event_bits |= if r < 0 { 4 } else { 2 };
    }
    let r = rng_next(g, w) as i8;
    let v = &mut karts[j];
    v.event_bits |= if r < 0 { 0x20 } else { 0x10 };
    rumble(v, g, 200);
    let b = rng_next(g, w) as i32;
    let vv = [karts[j].vx, karts[j].vy, karts[j].vz];
    super::particle(g, p.pos, vv, 0, b << 4, p.spin_rate as i32, 0x11, [0x80; 3], 0x180, 0x180, 0x20);
    crate::sound::at(g, w, 0x14, 0x1600, 0x80, p.pos, -1);
    crate::sound::at(g, w, 10, 0x1600, 0x80, p.pos, -1);
    g.items.events.push(ItemEvent::Hit { victim: j as u8, shooter: o as u8, projectile: i as u8 });
}

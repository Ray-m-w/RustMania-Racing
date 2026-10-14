//! FUN_80064B5C: the per-tick kart update (input -> throttle/brake -> speed
//! -> steering/heading -> vertical motion -> ground contact -> wheels and
//! body). Comments cite the instruction addresses of the game code each
//! block reproduces. Paths the solo player never takes on DOCK are
//! `unreachable_in_scope!` (docs/spec/kart-physics-ledger.md).

use crate::fixed::{div, div_pow2, mul, s16, s8, substeps, wrap_angle};
use crate::globals::RaceGlobals;
use crate::kart::KartState;
use crate::tables::{ratan2, square_root0};
use crate::world::{TickInputs, World};
use crate::progress;

/// The "Copy the buttons" target buttons, halfwords [exe 0x800B077C].
pub const COPY_BUTTONS: u32 = 0x800B_077C;

/// Read the next pseudo-random byte (`*rng++`).
pub fn rng_next(g: &mut RaceGlobals, w: &World) -> u8 {
    let b = w.rng_byte(g.rng);
    g.rng = g.rng.wrapping_add(1);
    b
}

/// The decoded controls (0x80065534..0x800655D8).
#[derive(Clone, Copy, Debug, Default)]
pub struct Controls {
    pub accel: bool,
    pub brake: bool,
    pub left: bool,
    pub right: bool,
    /// The jump button bit: SQUARE (0x8000), or CIRCLE (0x2000) under the
    /// alternate layout.
    pub jump_mask: u16,
}

/// Button bits (active high, PS1 pad order).
pub mod btn {
    pub const SELECT: u16 = 0x0001;
    pub const RIGHT: u16 = 0x0020;
    pub const LEFT: u16 = 0x0080;
    pub const L2: u16 = 0x0100;
    pub const R2: u16 = 0x0200;
    pub const L1: u16 = 0x0400;
    pub const R1: u16 = 0x0800;
    pub const TRIANGLE: u16 = 0x1000;
    pub const CIRCLE: u16 = 0x2000;
    pub const CROSS: u16 = 0x4000;
    pub const SQUARE: u16 = 0x8000;
}

/// Speed and grip limits chosen for this tick (0x80065E00..0x800660F0).
#[derive(Clone, Copy, Debug, Default)]
struct Limits {
    /// Forward speed cap (s6) and the soft sideways cap.
    max_fwd: i32,
    /// Hard forward cap used by the boost (iStack_138).
    hard_fwd: i32,
    /// Thrust per sub-step (uStack_160).
    thrust: i32,
    /// Rolling drag per sub-step (iStack_158).
    drag: i32,
    /// Sideways grip per sub-step (s5).
    grip: i32,
    /// Steering rate (iStack_130).
    steer_rate: i32,
}

/// The other karts of the race frame while one kart updates (slot order,
/// the updating kart's slot left out): what the update touches besides its
/// own kart (a replaced projectile's owner, the R2 special's victims).
pub struct Others<'a> {
    pub before: &'a mut [KartState],
    pub after: &'a mut [KartState],
}

impl<'a> Others<'a> {
    pub fn none() -> Others<'static> {
        Others { before: &mut [], after: &mut [] }
    }

    /// The updating kart's slot.
    pub fn slot(&self) -> usize {
        self.before.len()
    }

    /// Kart `j` (None for the updating kart itself or past the end).
    pub fn get_mut(&mut self, j: usize) -> Option<&mut KartState> {
        let s = self.before.len();
        if j < s {
            self.before.get_mut(j)
        } else if j == s {
            None
        } else {
            self.after.get_mut(j - s - 1)
        }
    }

    pub fn len(&self) -> usize {
        self.before.len() + 1 + self.after.len()
    }
}

/// FUN_80064B5C for a kart with no other karts in reach and no BIOS
/// rand() source (analysis of one kart; a weapon fired at a path fork, or
/// an R2 special, needs `kart_update_at`).
pub fn kart_update(k: &mut KartState, g: &mut RaceGlobals, w: &World, inp: &TickInputs, buttons: u16, player: i16) {
    let mut nr = || -> i32 { unreachable_in_scope!("BIOS rand() in a kart update without a rand source") };
    kart_update_full(k, &mut Others::none(), g, w, inp, buttons, player, &mut nr)
}

/// FUN_80064B5C(&karts[s], buttons, player) inside the race frame's kart
/// loop: the other karts are reachable, `rand` is the BIOS rand().
#[allow(clippy::too_many_arguments)]
pub fn kart_update_at(ks: &mut [KartState], s: usize, g: &mut RaceGlobals, w: &World, inp: &TickInputs, buttons: u16, player: i16, rand: &mut dyn FnMut() -> i32) {
    let (before, rest) = ks.split_at_mut(s);
    let (k, after) = rest.split_first_mut().expect("kart slot in range");
    kart_update_full(k, &mut Others { before, after }, g, w, inp, buttons, player, rand)
}

#[allow(clippy::too_many_arguments)]
pub fn kart_update_full(k: &mut KartState, others: &mut Others, g: &mut RaceGlobals, w: &World, inp: &TickInputs, buttons: u16, player: i16, rand: &mut dyn FnMut() -> i32) {
    let t = &w.tables;
    let dt = g.dt;
    let mut buttons = buttons as u32;

    // 0x80064B5C: mode 0x12 forces no input; store the buttons.
    if g.mode == 0x12 {
        buttons = 0;
    }
    k.buttons = buttons as u16;
    g.sound_event = 0;

    // 0x80064BA8: "Copy the buttons" start game, the last 300 VBlanks of
    // the countdown: press the face button the HUD shows (+0x4DA indexes the
    // exe table 0x800B077C: SQUARE, TRIANGLE, CIRCLE, CROSS) to add to the
    // start boost +0x446 (2 if quick, 1 after 0x1D VBlanks of hold time);
    // a wrong button takes one away. +0x4D8 = button held (one press counts
    // once).
    if k.flags & 1 == 0 && 0 < g.countdown && g.countdown < g.countdown_len && g.mode == 1 {
        let n = k.start_hold as i32 + dt;
        k.start_hold = n as u8;
        if (n & 0xFF) > 0x3C {
            k.start_hold = 0x3C;
        }
        if buttons & 0xF000 == 0 {
            k.copy_hold = 0;
        } else {
            let want = w.exe.i16(COPY_BUTTONS + 2 * k.copy_button as u32) as u16 as u32;
            if buttons & 0xF000 == want {
                if k.copy_hold == 0 {
                    rumble(k, g, 200);
                    let inc = if k.start_hold > 0x1D { 1 } else { 2 };
                    k.boost_count = k.boost_count.wrapping_add(inc);
                    k.engine_rev = 0x3C;
                    let mut b;
                    loop {
                        b = rng_next(g, w);
                        if (b >> 6) as i32 != k.copy_button as i32 {
                            break;
                        }
                    }
                    k.copy_button = (b >> 6) as i8;
                    k.start_hold = 0;
                }
            } else if k.copy_hold == 0 {
                let n = k.boost_count as i32 - 1;
                k.boost_count = n as i16;
                if s16(n) < 0 {
                    k.boost_count = 0;
                }
            }
            k.copy_hold = 1;
        }
    }

    // 0x80064D18: engine rev timer.
    let r = k.engine_rev as i32 - dt;
    k.engine_rev = if s8(r) < 0 { 0 } else { r as u8 };

    // 0x80064D40: engine sound pitch draws one random byte per tick.
    let human = (k.player_index as i32) < g.humans as i32;
    // 60 Hz mode: the byte is drawn once per pair (`replay_draws`).
    if human && g.mode != 0x10 {
        g.replay_draws(|g| crate::sound::engine(k, g, w));
    }

    // 0x80064F94: which camera is nearest (only matters with 2+ humans).
    if human {
        k.camera = k.player_index;
    } else if g.humans < 2 {
        k.camera = 0;
    } else {
        unreachable_in_scope!("nearest-camera search (2+ human players)");
    }

    // 0x80065118: SELECT toggles the view; TRIANGLE fires an item.
    if g.race_over == 0 && k.finished == 0 && human && g.race_type != 3 && g.mode != 0x12 && g.game_state != 0x1E {
        if g.mode == 1 {
            if buttons & btn::SELECT as u32 == 0 {
                k.pause_bits &= 0x7F;
            } else if k.pause_bits & 0x80 == 0 {
                k.pause_bits = ((k.pause_bits.wrapping_add(1)) & 1) | 0x80;
                crate::sound::view_toggle(g);
            }
        }
        if 300 < g.race_clock {
            if k.buttons & btn::TRIANGLE == 0 || k.special != 0 || (k.pickup != -1 && k.wall_flag == 0) {
                k.item_fire = 0;
                k.look_target = 0xFF;
            } else if k.item_fire == 0 {
                // FUN_800728C0: lock the camera onto the nearest kart.
                if g.mode == 0x14 {
                    unreachable_in_scope!("TRIANGLE in mode 0x14");
                }
                k.look_target = k.nearest_kart as u8;
                if !(g.mode == 2 || g.mode == 7 || (g.mode == 1 && k.nearest_kart != -1)) {
                    unreachable_in_scope!("TRIANGLE look-at search (FUN_800728C0, no nearest kart)");
                }
                k.item_fire = 1;
            }
        }
    }

    // 0x80065228: position history ring (12 x {x, y, z, dt}), newest first.
    if k.history_ptr != 0 && g.countdown == 0 {
        crate::camera::push_trail(&mut k.trail, [k.x, k.y, k.z], dt);
    }

    // 0x80065330: ground speed = |(x, z) - previous| / dt.
    let dx = k.prev_x.wrapping_sub(k.x);
    let dz = k.prev_z.wrapping_sub(k.z);
    let sq = square_root0(t, mul(dx, dx).wrapping_add(mul(dz, dz)));
    k.speed_est = div(sq, dt);

    // 0x80065398: engine factor eases back to 0x1000.
    if k.special == 0 {
        if (k.pickup == -1 || k.wall_flag != 0) && k.item_e7 == -1 && k.effect == -1 && k.item_ee == -1 {
            let e = k.engine as i32;
            if e < 0x1000 {
                let n = e + dt * 0x80;
                k.engine = n as i16;
                if s16(n) > 0x1000 {
                    k.engine = 0x1000;
                }
            } else if e > 0x1000 {
                let n = e - dt * 0x80;
                k.engine = n as i16;
                if s16(n) < 0x1000 {
                    k.engine = 0x1000;
                }
            }
        }
    } else {
        // 0x800653D0: the special move (+0x4D6): scale from the meter,
        // meter drain, FUN_800913F4 along the path.
        crate::items::specials::special_move_tick(k, g, w, rand);
    }

    // 0x80065510: controls.
    if g.countdown != 0 {
        buttons &= btn::CROSS as u32;
    }
    let alt = player != -1 && g.alt_controls[player as usize] == 1;
    let mut c = Controls {
        accel: buttons & btn::CROSS as u32 != 0,
        brake: if alt { buttons & btn::SQUARE as u32 != 0 } else { buttons & btn::CIRCLE as u32 != 0 },
        left: buttons & btn::LEFT as u32 != 0,
        right: buttons & btn::RIGHT as u32 != 0,
        jump_mask: if alt { btn::CIRCLE } else { btn::SQUARE },
    };
    if k.status & 4 != 0 || k.stun > 999 {
        c.brake = false;
        c.accel = false;
    }
    if k.status & 8 != 0 || k.stun > 999 {
        c.right = false;
        c.left = false;
    }

    // 0x800655E0: handling numbers from the vehicle stats (exe 0x800ACDB0).
    let st = &t.vehicle[k.vehicle as usize];
    k.steer_centre = 0x40;
    k.accel = ((st[1] as i32 * 0x4B) / 100) as i16;
    k.top_speed = ((st[2] as i32 * 0x60) / 100) as i16;
    k.steer_rate = ((st[3] as i32 * 0x24) / 100) as i16;
    k.stat_d = ((((st[4] as i32) << 7) / 100) as i16) as i16;

    // 0x80065714: catch-up (rubber band) toward the leader.
    if g.mode == 1 && k.race_pos > 1 && w.course.length.is_some() && k.finished == 0 {
        let len = (w.course.length.unwrap() >> 4) as i32;
        let d = s16(inp.leader_progress.wrapping_sub(k.progress));
        let v = if d > len { len } else if d < 0 { 0 } else { d };
        let target = s16(div(s16(v) * 0xB4, len));
        let cur = k.catchup as i32;
        let mut n = cur;
        if cur < target {
            n = cur + dt;
            if target < s16(n) {
                n = target;
            }
        } else if target < cur {
            n = cur - dt;
            if s16(n) < target {
                n = target;
            }
        }
        k.catchup = n as u8;
        // 0x80065850: an AI kart's catch-up is scaled by the difficulty.
        crate::ai::catchup_scale(k, g, g.difficulty, g.cup);
    } else {
        k.catchup = 0;
    }

    // 0x80065954: weapon slots recharge; a recharging slot keeps drawing a
    // new weapon type every 8 ticks of charge.
    for i in 0..2 {
        let b = k.weapons[4 + i] as i8 as i32;
        if b != -1 && b < 0x40 {
            let n = b + dt;
            k.weapons[4 + i] = n as u8;
            if s8(n) < 0x40 {
                if (n & 7) < (b & 7) {
                    k.weapons[2 * i] = k.weapons[2 * i + 1];
                    let mut v;
                    loop {
                        let r = rng_next(g, w);
                        v = t.weapon_pick[(r >> 3) as usize];
                        if v as i32 != k.weapons[2 * i] as i8 as i32 {
                            break;
                        }
                    }
                    k.weapons[2 * i + 1] = v;
                }
            } else {
                k.weapons[2 * i] = k.weapons[2 * i + 1];
                k.weapons[4 + i] = 0x40;
            }
        }
    }

    // 0x80065A50: shoulder buttons (turbo latch, look-back, specials).
    'shoulder: {
    if k.special == 0 && (k.pickup == -1 || k.wall_flag != 0) && k.effect == -1 && k.stun < 1000 {
        if buttons & btn::R1 as u32 == 0 {
            k.latch_r1 = 0;
        } else {
            if k.start_timer == 0 && k.latch_r1 == 0 && k.flags & 1 == 0 {
                g.sound_event = 3;
            }
            k.latch_r1 = 1;
            if k.start_timer == 0x1A4 && k.path_seg != -1 && g.mode == 1 {
                // 0x80065B5C: the special move; L1 / L2 / R2 are not read
                // this tick (goto 0x80065E00).
                let slot = others.slot();
                crate::items::specials::special_move_start(k, g, w, slot, rand);
                break 'shoulder;
            }
        }
        if buttons & btn::L1 as u32 == 0 {
            k.latch_l1 = 0;
        } else {
            let slot = others.slot();
            crate::items::weapons::fire_button(k, slot, others, g, w, 0, rand);
        }
        if buttons & btn::L2 as u32 == 0 {
            k.latch_l2 = 0;
        } else {
            let slot = others.slot();
            crate::items::weapons::fire_button(k, slot, others, g, w, 1, rand);
        }
        if buttons & btn::R2 as u32 == 0 {
            k.latch_r2 = 0;
        } else {
            let kind = crate::items::specials::special_kind(w, k.character);
            if k.start_timer == 0x1A4
                && kind != -1
                && g.items.special == -1
                && g.challenge == 0
                && g.mode != 0xE
                && (g.mode != 3 || g.split_screen != 0)
            {
                // 0x80065CF0: the character special on every other kart.
                crate::items::specials::character_special(k, others, g, w);
                k.start_timer = 0;
                crate::ai::reroll(k, g, w, g.karts);
            } else if k.latch_r2 == 0 && k.flags & 1 == 0 {
                g.sound_event = 4;
            }
            k.latch_r2 = 1;
        }
        crate::sound::not_ready(k, g, w);
    }
    }

    // 0x80065E00: remember the previous position and speeds.
    let mut accel = k.accel as i32;
    k.prev_y = k.y;
    k.prev_z = k.z;
    k.prev_vy = k.vy as i16;
    k.prev_x = k.x;
    let old_fwd = k.fwd_speed;
    let old_side = k.side_speed;
    let centre = k.steer_centre as i32;
    let cap = centre + ((0x50 - centre) * k.catchup as i32) / 0xB4;
    let top = k.top_speed as i32;

    // 0x80065E78..0x80066990: limits, jump, spin, steering. Skipped in a
    // special move (+0x4D6), which leaves the stack values the later code
    // reads as the previous kart update left them (see `RaceGlobals::stack`).
    let mut lim = Limits::default();
    let mut yaw: i32 = 0;
    if k.special == 0 {
    {
        let saved = k.float_state;
        if k.status & 0x400 != 0 {
            k.float_state = 2;
        }
        match k.float_state {
            0 => {
                lim.max_fwd = cap * 0x20;
                lim.hard_fwd = 0xE00;
                lim.thrust = top;
            }
            1 => {
                if k.ground_contact == 0 && k.chassis_flags & 2 == 0 {
                    lim.max_fwd = (cap * 0x20) / 3;
                    lim.thrust = (top << 1) / 3;
                    lim.hard_fwd = 0x4AA;
                } else {
                    lim.max_fwd = (cap * 0x40) / 3;
                    lim.thrust = div_pow2(top * 3, 2);
                    lim.hard_fwd = 0x955;
                }
            }
            _ => {
                lim.max_fwd = (cap * 0x20) / 3;
                lim.thrust = (top << 1) / 3;
                lim.hard_fwd = 0x4AA;
                accel = div_pow2(accel, 2);
            }
        }
        lim.drag = accel / 2;
        lim.steer_rate = k.steer_rate as i32;
        k.float_state = saved;
        if k.ground_contact == 0 {
            accel /= 2;
            lim.drag /= 2;
            lim.thrust /= 2;
            lim.grip = accel;
        } else {
            if saved == 0 {
                let s = k.surface as i32;
                lim.max_fwd = div_pow2(lim.max_fwd * t.surface(s, 0), 7);
                lim.thrust = div_pow2(lim.thrust * t.surface(s, 1), 7);
                lim.drag = div_pow2(lim.drag * t.surface(s, 2), 7);
                accel = div_pow2(accel * t.surface(s, 3), 7);
            }
            lim.grip = accel;
            if k.status & 0x1000 != 0 {
                lim.grip = accel / 2;
                lim.drag /= 2;
            }
        }
    }

    // 0x80066104: jump button.
    let jm = c.jump_mask as u32;
    if buttons & jm != 0 && (k.effect == 2 || k.status & 8 != 0 || k.stun > 999) {
        buttons -= jm;
    }
    // (60 Hz mode: the random jump is tried once per pair.)
    if k.ground_contact != 0 && (k.effect as u8).wrapping_sub(0x13) < 2 && g.hz60.beat() && rng_next(g, w) < 0x20 {
        buttons |= jm;
    }
    if buttons & jm == 0 {
        if k.ground_contact != 0 {
            k.jump_state = 0;
        }
        if k.jump_state == 1 {
            k.jump_state = 2;
        }
        if k.jump_state == 5 {
            k.jump_state = 6;
        }
    } else if k.jump_state == 1 || k.jump_state > 3 {
        if k.ground_contact != 0 {
            k.jump_state = 5;
        } else if k.jump_state == 6 {
            k.jump_state = 3;
        }
    } else if (k.ground_contact == 0 || k.ceiling_hit != 0)
        && k.carried == -1
        && (k.chassis_flags & 2 == 0 || k.float_state != 1)
    {
        if k.jump_state == 2 {
            k.jump_state = 3;
        }
    } else {
        // 0x80066314: take off.
        k.jump_state = 1;
        k.ground_contact = 0;
        if k.carried == -1 {
            let e = k.engine as i32;
            let half = (e - (e >> 31)) >> 1;
            k.vy = div_pow2((half + 0x800) * -0x700, 12);
        } else {
            // A jump drops the kart off GRAVE's hook.
            crate::hooks::drop(k, g);
        }
        crate::sound::jump(k, g, w);
    }
    if k.ground_contact != 0 && k.carried_drop != 0 {
        k.carried_drop = 0;
    }

    if k.item_e7 != -1 || (k.pickup != -1 && k.pickup_timer < 0xF0) {
        c.left = false;
        c.right = false;
    }
    if k.fwd_speed < 0 {
        std::mem::swap(&mut c.left, &mut c.right);
    }

    // 0x8006641C: spin-out yaw.
    if k.spin_timer != 0 {
        let n = k.spin_timer as i32 - dt;
        k.spin_timer = n as i16;
        if s16(n) < 0 {
            k.spin_timer = 0;
        }
    }
    for _ in 0..substeps(dt) {
        if k.spin_timer == 0 {
            let r = k.spin_rate as i32;
            let n = r + div_pow2(-r, 2);
            k.spin_rate = n as i16;
            if s16(n).abs() < 4 {
                k.spin_rate = 0;
            }
        }
        yaw = yaw.wrapping_add(k.spin_rate as u16 as i32);
        k.heading = k.heading.wrapping_add(k.spin_rate);
    }

    // 0x80066500: accelerate + brake together: power turn.
    k.accel_spin = 0;
    if g.hz60.beat() {
        k.hz60_steer = k.steer;
    }
    if c.accel && c.brake {
        if k.ground_contact != 0 && k.fwd_speed > 0 {
            let v = (k.fwd_speed >> 6).min(0x10);
            // The yaw reads the steer value as the tick starts, a tick
            // behind the steering below. 60 Hz mode: the value at the start
            // of the pair on both ticks (with the mid-pair value the power
            // turn turned 25 % further per tap).
            let steer = if g.hz60.on { k.hz60_steer } else { k.steer };
            let d = div_pow2(steer as i32 * v * dt, 6);
            yaw = yaw.wrapping_add(d);
            k.heading = k.heading.wrapping_add(d as i16);
            lim.grip = div_pow2(lim.grip * v * 3, 5);
            k.accel_spin = 1;
        }
        c.brake = false;
    }
    if k.ground_contact != 0 {
        let d = k.slope_turn as i32 * dt;
        yaw = yaw.wrapping_add(d);
        k.heading = k.heading.wrapping_add(d as i16);
    }

    // 0x80066600: steering.
    let rate = lim.steer_rate;
    let steer_in: Option<i32> = match (c.left, c.right) {
        (true, false) => Some(if k.fwd_speed >= 0 { -rate } else { rate }),
        (false, true) => Some(if k.fwd_speed < 0 { -rate } else { rate }),
        _ => None,
    };
    for _ in 0..substeps(dt) {
        match steer_in {
            None => {
                let s = k.steer as i32;
                if s > 0 {
                    let n = s - rate;
                    k.steer = n as i16;
                    if s16(n) < 0 {
                        k.steer = 0;
                    }
                } else if s < 0 {
                    let n = s + rate;
                    k.steer = n as i16;
                    if s16(n) > 0 {
                        k.steer = 0;
                    }
                }
            }
            Some(d) => {
                let n = s16((k.steer as u16 as i32).wrapping_add(d));
                k.steer = n as i16;
                if rate * 8 < n {
                    k.steer = (rate * 8) as i16;
                } else if n < rate * -8 {
                    k.steer = (rate * -8) as i16;
                }
            }
        }
        let v = div_pow2(k.steer as i32, 3);
        if k.spin_rate == 0 {
            yaw = yaw.wrapping_add(v);
            k.heading = k.heading.wrapping_add(v as i16);
        }
        if k.status & 0x40110 != 0 {
            // 0x80066790: an effect steers: toward the kart's camera (0x10,
            // 0x100: sideways to it) or toward +0x300 / +0x308 (0x40000).
            let (tx, tz) = if k.status & 0x110 == 0 {
                (k.target_x, k.target_z)
            } else {
                let c = inp.cameras[(k.camera as usize).min(1)];
                (c[0], c[2])
            };
            let a = ratan2(t, tx.wrapping_sub(k.x) >> 5, tz.wrapping_sub(k.z) >> 5) & 0xFFF;
            let mut d = wrap_angle(a - k.heading as i32);
            if k.status & 0x100 != 0 {
                let u = if d < 0 { a + 0x400 } else { a - 0x400 };
                d = wrap_angle((u & 0xFFF) - k.heading as i32);
            }
            let v = div_pow2(d, 5);
            yaw = yaw.wrapping_add(v);
            k.heading = k.heading.wrapping_add(v as i16);
        }
    }
    if k.status & 0x40 != 0 {
        // 0x800668A8: spun by a hit (status 0x40, +0x468 counting up to
        // 300): the heading turns by +0x300 scaled by a ramp that rises over
        // the first 32 VBlanks and falls over the last 32.
        let t = k.homing_timer as i32;
        let ramp = if t < 32 { t } else { s16(300 - t).min(32) };
        k.target_yaw = div_pow2(s16(ramp).wrapping_mul(k.target_x), 5);
        k.heading = k.heading.wrapping_add(k.target_yaw.wrapping_mul(dt) as i16);
    }
    k.heading = ((k.heading as u16) & 0xFFF) as i16;
    g.stack.steer_rate = lim.steer_rate;
    } else {
        lim.steer_rate = g.stack.steer_rate;
    }
    // In a special move the yaw sum is register fp as the caller left it
    // (low halfword 0 in every oracle frame: +0x448 stays 0).
    let yaw16 = s16(yaw);

    // 0x8006699C: steering lag follows the steer value.
    if k.status & 1 == 0 {
        let target = if k.fwd_speed < 0 { -(k.steer as i32) } else { k.steer as i32 };
        let cur = k.steer_lag as i32;
        if cur < target {
            let n = cur + lim.steer_rate * dt;
            k.steer_lag = n as i16;
            if target < s16(n) {
                k.steer_lag = target as i16;
            }
        } else if target < cur {
            let n = cur - lim.steer_rate * dt;
            k.steer_lag = n as i16;
            if s16(n) < target {
                k.steer_lag = target as i16;
            }
        }
    }

    // 0x80066A08: throttle state, lurch, skid, conveyor, turbo; velocity;
    // drift; vertical motion. In a special move (0x80066A0C): boosting, the
    // drift decays, the kart-frame speeds come from the velocity
    // FUN_800913F4 set (+0x390 = the sideways formula: the game's own).
    let side0;
    if k.special != 0 {
        k.boosting = 1;
        let n = (k.drift as u16 as i32).wrapping_sub(dt * 4);
        k.drift = n as i16;
        if s16(n) < 0 {
            k.drift = 0;
        }
        let a = (-(k.heading as i32)) & 0xFFF;
        let (ca, sa) = (t.cos(a), t.sin(a));
        k.fwd_speed = div_pow2(mul(k.vx, ca).wrapping_add(mul(k.vz, sa)), 12);
        k.side_speed = div_pow2(mul(k.vz, ca).wrapping_sub(mul(k.vx, sa)), 12);
        // The stack slot 0x48(sp) the sway inputs read.
        side0 = g.stack.side0;
    } else {
    if !c.accel || c.brake {
        if k.boosting == 0 {
            if !c.brake || c.accel {
                k.throttle_hold = 0;
                k.brake_hold = 0;
                k.coast_hold = k.coast_hold.wrapping_add(dt as u8);
                let l = k.lurch as i32;
                if l > 0 {
                    let n = l - dt * 6;
                    k.lurch = n as i16;
                    if s16(n) < 0 {
                        k.lurch = 0;
                    }
                } else if l < 0 {
                    let n = l + dt * 3;
                    k.lurch = n as i16;
                    if s16(n) > 0 {
                        k.lurch = 0;
                    }
                }
            } else {
                k.throttle_hold = 0;
                k.coast_hold = k.coast_hold.wrapping_add(dt as u8);
                k.brake_hold = k.brake_hold.wrapping_add(dt as u8);
                let n = (k.lurch as u16 as i32) - dt * 4;
                k.lurch = n as i16;
                if s16(n) < -0x80 {
                    k.lurch = -0x80;
                }
            }
        } else {
            boost_lurch(k, dt);
        }
    } else {
        boost_lurch(k, dt);
    }
    if k.throttle_hold > 0x80 {
        k.throttle_hold = 0x80;
    }
    if k.coast_hold > 0x80 {
        k.coast_hold = 0x80;
    }
    if k.brake_hold > 0x80 {
        k.brake_hold = 0x80;
    }
    // 60 Hz mode: tried once per pair; the hold counts 1 per tick, so the
    // window is 1..6 (three tries, as the game's 2, 4, 6).
    let skid_window = if g.hz60.on { 7 } else { 8 };
    if g.countdown == 0 && k.vehicle_class & 0x50 != 0 && k.coast_hold != 0 && k.coast_hold < skid_window && g.hz60.beat() && rng_next(g, w) < 0x10 {
        k.skid = 1;
        crate::sound::skid(k, g, w);
    } else {
        k.skid = 0;
    }
    if k.conveyor != 0 {
        let d = k.conveyor_dir as i32;
        k.x = k.x.wrapping_add(div_pow2(t.sin_raw(d), 5) * dt);
        k.z = k.z.wrapping_add(div_pow2(t.cos_raw(d), 5) * dt);
    }
    let boost_count = g.countdown == 0 && k.boost_count != 0;
    let thrust;
    if boost_count
        || (k.start_timer != 0 && buttons & btn::R1 as u32 != 0 && k.effect == -1 && (k.pickup == -1 || k.wall_flag != 0))
    {
        k.boosting = 1;
        k.bounce_kick = k.bounce_kick.wrapping_add(0xFA);
        let n = k.boost_flash as i32 + dt * 3;
        k.boost_flash = n as u8;
        if s8(n) > 0x40 {
            k.boost_flash = 0x40;
        }
        if boost_count {
            let n = k.boost_count as u16 as i32 - dt;
            k.boost_count = n as i16;
            if s16(n) < 0 {
                k.boost_count = 0;
            }
        } else {
            let n = k.start_timer as u16 as i32 - dt;
            k.start_timer = n as i16;
            if s16(n) < 0 {
                k.start_timer = 0;
            }
        }
        rumble(k, g, 0x96);
        thrust = 0x80;
        lim.max_fwd = 0xE00;
        lim.hard_fwd = 0xE00;
        lim.grip = 0x4B;
    } else {
        let n = k.boost_flash as i32 - dt * 2;
        k.boost_flash = n as u8;
        if s8(n) < 0 {
            k.boost_flash = 0;
        }
        k.boosting = 0;
        thrust = if !c.accel {
            if c.brake {
                div_pow2(lim.thrust * -3, 2)
            } else {
                0
            }
        } else if !c.brake {
            if g.countdown == 0 { lim.thrust } else { 0 }
        } else {
            0
        };
    }

    // 0x80067028: velocity in the kart frame, integrate dt sub-steps.
    // The game converts once per tick with the heading after both sub-steps
    // of steering, so its first VBlank already moves in the frame of the
    // second. 60 Hz mode: on the first tick of a pair the frame is the
    // heading plus this tick's yaw (the turn the second tick repeats with
    // the same buttons); the second tick uses its own heading. Without it a
    // power slide built 2 % more sideways speed and the hairpin cost the
    // AI-driven kart +0.2 s (kart-physics-ledger.md "60 Hz mode").
    let hconv = if g.hz60.on && g.hz60.beat() { (k.heading as i32 + yaw16) & 0xFFF } else { k.heading as i32 };
    let a = (-hconv) & 0xFFF;
    let (ca, sa) = (t.cos(a), t.sin(a));
    let mut fwd;
    if g.hz60.on {
        (side0, fwd) = to_kart_frame_60(k, ca, sa);
    } else {
        side0 = div_pow2(mul(k.vx, ca).wrapping_add(mul(k.vz, sa)), 12);
        fwd = div_pow2(mul(k.vz, ca).wrapping_sub(mul(k.vx, sa)), 12);
    }
    g.stack.side0 = side0;
    let px = k.push_x as i32;
    let pz = k.push_z as i32;
    let push_side = div_pow2(px * ca + pz * sa, 12);
    let push_fwd = div_pow2(pz * ca - px * sa, 12);
    let mut side = side0;
    let (mut sum_side, mut sum_fwd) = (0i32, 0i32);
    let max = lim.max_fwd;
    let back = (-max) / 2;
    for _ in 0..substeps(dt) {
        side = side.wrapping_add(push_side);
        let mut over;
        if side < 0 {
            side += lim.grip;
            over = max < side;
            if side > 0 {
                side = 0;
                over = max < 0;
            }
        } else {
            over = max < side;
            if side > 0 {
                side -= lim.grip;
                over = max < side;
                if side < 0 {
                    side = 0;
                    over = max < 0;
                }
            }
        }
        if over {
            side += div_pow2(max - side, 3);
        } else if side < -max {
            side += div_pow2(-max - side, 3);
        }
        sum_side = sum_side.wrapping_add(side);

        fwd = fwd.wrapping_add(push_fwd);
        if thrust == 0 {
            if fwd < 0 {
                fwd += lim.drag;
                if fwd > 0 {
                    fwd = 0;
                }
            } else if fwd > 0 {
                fwd -= lim.drag;
                if fwd < 0 {
                    fwd = 0;
                }
            }
        } else {
            fwd += thrust;
        }
        if fwd < back {
            fwd += div_pow2(back - fwd, 3);
        } else {
            if lim.hard_fwd < fwd {
                fwd += div_pow2(lim.hard_fwd - fwd, 2);
            }
            if k.boosting == 0 && max < fwd {
                fwd += div_pow2(max - fwd, 3);
            }
        }
        sum_fwd = sum_fwd.wrapping_add(fwd);
    }
    k.push_x = 0;
    k.push_z = 0;
    k.fwd_speed = fwd;
    k.side_speed = side;
    let h = hconv;
    let (ch, sh) = (t.cos(h), t.sin(h));
    let (nx, nz) = (mul(side, ch).wrapping_add(mul(fwd, sh)), mul(fwd, ch).wrapping_sub(mul(side, sh)));
    k.vx = div_pow2(nx, 12);
    k.vz = div_pow2(nz, 12);
    if g.hz60.on {
        k.hz60_vel = Some([k.vx, k.vz, nx.wrapping_sub(k.vx << 12), nz.wrapping_sub(k.vz << 12)]);
    }
    let mx = div_pow2(mul(sum_side, ch).wrapping_add(mul(sum_fwd, sh)), 12);
    let mz = div_pow2(mul(sum_fwd, ch).wrapping_sub(mul(sum_side, sh)), 12);
    k.x = k.x.wrapping_add(mx);
    k.z = k.z.wrapping_add(mz);
    if k.status & 0x20 != 0 {
        // 0x80067448: pulled toward +0x300 / +0x308.
        for _ in 0..substeps(dt) {
            k.vx = k.vx.wrapping_add(div_pow2(k.target_x.wrapping_sub(k.x), 6));
            k.vz = k.vz.wrapping_add(div_pow2(k.target_z.wrapping_sub(k.z), 6));
            k.x = k.x.wrapping_add(k.vx);
            k.z = k.z.wrapping_add(k.vz);
        }
    }

    // 0x800674A0: drift meter and reversing flag.
    let saved_float = k.float_state;
    let saved_water = k.water_y;
    if k.status & 0x400 != 0 {
        // 0x800674D0: floated: water physics at a bobbing height above the
        // support surface (lower for effect 1).
        let base = if k.effect == 1 { 0x2000 } else { 0x3000 };
        let idx = ((g.frame & 0x1F) * 0x80 + (k.player_index as i32) * 0x200) & 0xFFF;
        let y = k.support_y.wrapping_add((k.ride_height as i32) * 0x20).wrapping_sub(base).wrapping_add(div_pow2(t.sin_raw(idx), 3));
        if y.wrapping_sub(k.ref_y) < 0x4000 {
            k.float_state = 2;
            k.water_y = y;
        }
    }
    if k.status & 1 == 0 {
        if g.countdown == 0 {
            let thr = if k.float_state != 2 { (t.surface(k.surface as i32, 2) >> 3) as u8 } else { 0 };
            let inc = thr < k.throttle_hold && (k.float_state == 2 || k.fwd_speed < div_pow2(lim.max_fwd, 2));
            if inc {
                let n = k.drift as u16 as i32 + dt * 4;
                k.drift = n as i16;
                if s16(n) > 0x100 {
                    k.drift = 0x100;
                }
            } else {
                let n = k.drift as u16 as i32 - dt * 4;
                k.drift = n as i16;
                if s16(n) < 0 {
                    k.drift = 0;
                }
            }
        }
        k.reversing = if k.fwd_speed >= 1 && k.brake_hold != 0 { 1 } else { 0 };
    } else {
        k.drift = 0;
        k.reversing = 0;
    }

    // 0x80067608: vertical motion (water float or gravity).
    if k.carried == -1 && k.item_e7 == -1 && (k.pickup == -1 || k.wall_flag != 0) {
        let floats = k.chassis_flags & 2 != 0 && k.effect == -1;
        if k.float_state == 2 || (k.float_state == 1 && k.chassis_flags & 2 != 0) {
            let target = if k.chassis_flags & 2 == 0 || k.effect != -1 {
                k.water_y + (k.ride_height as i32) * -0x20 + 1
            } else {
                k.water_y + 0x201
            };
            for _ in 0..substeps(dt) {
                let kk = if floats { 0x14 } else { 0x50 };
                let vy = k.vy;
                k.y = k.y.wrapping_add(div(target.wrapping_sub(k.y), kk));
                if vy < 1 {
                    if vy < 0 {
                        k.vy = vy + 0x30;
                        if k.vy > 0 {
                            k.vy = 0;
                        }
                    }
                } else {
                    k.vy = if k.ground_contact == 0 { vy - 0x30 } else { vy - 0x60 };
                    if k.vy < 0 {
                        k.vy = 0;
                    }
                }
                k.y = k.y.wrapping_add(k.vy);
            }
        } else {
            for _ in 0..substeps(dt) {
                k.vy = k.vy + 0x60 + k.gravity_mod as i32;
                if k.vy > 0x1800 {
                    k.vy = 0x1800;
                } else if k.vy < -0xC00 {
                    if k.vy < -0x1800 {
                        k.vy = -0x1800;
                    }
                    let up = -k.vy;
                    k.vy += div_pow2(up - 0xC00, 3);
                }
                k.y = k.y.wrapping_add(k.vy);
            }
        }
        k.gravity_mod = 0;
        k.float_state = saved_float;
        k.water_y = saved_water & 0xFF;
    }
    }

    // 0x80067834: impact sound draws a random byte.
    if human && k.impact > 800 {
        g.replay_draws(|g| crate::sound::impact(k, g, w));
    }

    // 0x8006789C: object interactions and ground contact.
    progress::contact_phase(k, g, w, inp);

    // 0x80067960: wheel spin.
    if k.wheel_count != 0 {
        for i in 0..k.wheel_count as usize {
            if k.status & 0x0100_0001 != 0 {
                k.wheels[i].spin_rate = 0;
                continue;
            }
            let wh = k.wheels[i];
            let f = wh.flags;
            let mut v: i32 = 0;
            if f & 0x38 == 0 {
                if k.special == 0 && f & 0xC0 == 0 {
                    let rolling;
                    if k.ground_contact == 0 {
                        if k.float_state == 2 && f & 1 == 0 {
                            v = div(k.fwd_speed.wrapping_mul(11), (wh.radius as i32) << 1);
                            rolling = false;
                        } else if f & 1 != 0 {
                            rolling = true;
                        } else {
                            let r = wh.spin_rate as i32;
                            v = if r > 0 {
                                (r - dt * 12).max(0)
                            } else if r < 0 {
                                let n = r + dt * 12;
                                if n > 0 { 0 } else { n }
                            } else {
                                r
                            };
                            rolling = false;
                        }
                    } else {
                        rolling = true;
                    }
                    if rolling {
                        v = div(k.fwd_speed.wrapping_mul(11), wh.radius as i32);
                        if f & 1 != 0 {
                            v += k.drift as i32;
                        }
                    }
                    if k.fwd_speed > 0 && k.brake_hold != 0 {
                        v = 0;
                    }
                }
            } else {
                let l = k.lurch as i32;
                let half = (l - (l >> 31)) >> 1;
                v = -(half.abs() + 0x40);
            }
            let wm = &mut k.wheels[i];
            for _ in 0..substeps(dt) {
                let r = wm.spin_rate as i32;
                let n = s16(r + div_pow2(v - r, 2));
                wm.spin_rate = n as i16;
                if (v - n).abs() < 4 {
                    wm.spin_rate = v as i16;
                }
                wm.spin = wm.spin.wrapping_add(wm.spin_rate);
            }
            wm.spin = ((wm.spin as u16) & 0xFFF) as i16;
        }
    }

    // 0x80067D1C: sliding flag; skid rumble.
    k.sliding = if k.ground_contact != 0 { (k.fwd_speed.abs() < k.side_speed.abs()) as u8 } else { 0 };
    if human
        && k.ground_contact != 0
        && k.engine_audible != 0
        && k.float_state == 0
        && (k.sliding != 0 || k.drift != 0 || k.reversing != 0)
    {
        rumble(k, g, 0x4B);
        crate::sound::slide(k, g, w);
    }

    // 0x80067E64: lap timing, race end, respawn timer.
    if g.mode != 0x12 {
        if k.finished == 0 {
            progress::lap_timing(k, g, w);
        } else {
            k.finished_time = k.finished_time.wrapping_add(dt);
        }
        if g.race_type == 3 {
            unreachable_in_scope!("time-trial ghost FUN_8006F64C");
        }
        if g.game_state != 0x1E && k.finished == 0 {
            let m = g.mode as i32;
            let end = ((m - 2) as u32) < 2 && 179_999 < k.race_time
                || (m == 7 && 89_999 < k.race_time)
                || (m == 0x14 && 179_999 < g.race_clock)
                || (m == 1 && 179_999 < g.race_clock && (k.lap as i16) < g.laps)
                || (g.challenge != 0);
            if end {
                unreachable_in_scope!("race over: kart handed to the AI (+0x4C7 = 2)");
            }
        }
        if k.respawn_timer != 0 {
            let n = k.respawn_timer as i32 - dt;
            k.respawn_timer = n as i16;
            if n.wrapping_mul(0x10000) < 1 {
                // 0x80068078: FUN_8009192C(kart, 1): re-roll the special-move
                // preference.
                crate::ai::reroll(k, g, w, g.karts);
            }
        }
    }

    // 0x80068090: body sway inputs.
    let dside = k.side_speed - old_side;
    // The sideways speed itself (not a change) is added once per tick: at
    // 60 Hz half of it per tick keeps the game's rate.
    let s2 = if g.hz60.on { side0 / 4 } else { side0 / 2 };
    let a7 = dside * 2 - s2;
    let a8 = (old_fwd - k.fwd_speed) * 2;
    k.sway[9] = k.sway[9].wrapping_add(a7);
    k.sway[10] = k.sway[10].wrapping_add(s2 + dside * -2);
    k.sway[11] = k.sway[11].wrapping_add(a7);
    k.sway[3] = k.sway[3].wrapping_add(a8);
    k.sway[4] = k.sway[4].wrapping_add(a8);
    k.sway[5] = k.sway[5].wrapping_add(a8 >> 1);
    if k.status & 1 == 0 {
        let target = if k.status & 0x80 == 0 {
            let a = ratan2(t, (k.aim_x >> 5) - (k.look_x >> 10), (k.aim_z >> 5) - (k.look_z >> 10));
            ((wrap_angle((a & 0xFFF) - k.heading as i32)) << 6) as i32
        } else {
            0
        };
        let damp = (0x40 - (k.speed_est >> 5)).max(0);
        sway_springs(k, g, w, target, s16(damp), k.effect as i32);
        for _ in 0..substeps(dt) {
            let old = k.bounce;
            let mut v = k.bounce_vel.wrapping_add(k.bounce_kick);
            if old < 0 {
                v += 0x80;
            } else if old > 0 {
                v -= 0x80;
            }
            k.bounce_vel = v;
            let n = old.wrapping_add(v);
            k.bounce = n;
            if (old < 0 && 0 < n) || (0 < old && n < 0) {
                k.bounce_vel = div_pow2(k.bounce_vel.wrapping_mul(3), 2);
            }
            if k.bounce < -0x6000 {
                k.bounce = -0x6000;
            } else if k.bounce > 0x6000 {
                k.bounce = 0x6000;
            }
            if k.bounce.abs() == 0x6000 {
                k.bounce_vel = 0;
            }
        }
    }
    k.bounce_kick = 0;

    // 0x800682A0: flash timer.
    if k.flash_timer != -1 {
        let n = k.blink_count as i32 + dt;
        k.blink_count = n as u8;
        if (n & 0xFF) > 0x3C {
            k.flash_timer = -1;
        }
    }

    // 0x800682DC: yaw rate = heading change this tick / dt.
    if k.status & 1 == 0 {
        k.yaw_rate = div(yaw16, dt) as i16;
    } else {
        // 0x80068324: frozen (status 1): the heading keeps turning at the
        // yaw rate, scaled by the ground speed.
        let v = (k.speed_est >> 5).min(0x40);
        let r = div_pow2((k.yaw_rate as i32).wrapping_mul(v), 6);
        let mut h = k.heading as u16;
        for _ in 0..substeps(dt) {
            h = h.wrapping_add(r as u16);
        }
        k.heading = (h & 0xFFF) as i16;
    }

    // 0x80068384: body shape interpolation.
    if k.shape_t < k.shape_n {
        let n = k.shape_t as i32 + dt;
        k.shape_t = n as u8;
        if (k.shape_n as i32) < (n & 0xFF) {
            k.shape_t = k.shape_n;
        }
    }
    if k.shape_t == k.shape_n {
        for j in 0..6 {
            k.shape[j] = k.shape[12 + j];
            k.shape[6 + j] = k.shape[12 + j];
        }
    } else {
        let tt = k.shape_t as i32;
        let nn = k.shape_n as i32;
        for j in 0..6 {
            let from = k.shape[6 + j] as i32;
            let to = k.shape[12 + j] as i32;
            k.shape[j] = (from + div((to - from) * tt, nn)) as i16;
        }
    }

    // 0x80068640: boost glow follows the turbo meter.
    let tgt = if k.start_timer < 0x1A4 { 0x60 } else { 0 };
    let cur = k.boost_glow as i32;
    if tgt < cur {
        let n = cur - dt * 4;
        k.boost_glow = n as i16;
        if s16(n) < tgt {
            k.boost_glow = tgt as i16;
        }
    } else if cur < tgt {
        let n = cur + dt * 4;
        k.boost_glow = n as i16;
        if tgt < s16(n) {
            k.boost_glow = tgt as i16;
        }
    }
    if k.status & 0x2000 != 0 {
        // 0x800686F0: drag: a push against the velocity.
        k.push_x = k.push_x.wrapping_add(div_pow2(k.vx.wrapping_neg(), 4) as i16);
        k.push_z = k.push_z.wrapping_add(div_pow2(k.vz.wrapping_neg(), 4) as i16);
    }

    // 0x80068718: leaving a conveyor: velocity from the position change.
    if k.conveyor_prev != 0 && k.conveyor == 0 {
        let vz = div(k.z - k.prev_z, dt);
        k.vx = div(k.x - k.prev_x, dt);
        k.vz = vz;
    }

    // 0x80068790: effect wobble (none on the solo path).
    {
        // 0x80068790: effect wobble of the body-sway springs.
        // Per-tick kicks: halved per tick in the 60 Hz mode.
        let h = if g.hz60.on { 1 } else { 0 };
        let mut add = None;
        if k.status & 0x8000 != 0 {
            let idx = (((k.homing_timer as i32).wrapping_mul(0xC0)) & 0x1FC0) / 2;
            add = Some(div_pow2(t.sin_raw(idx), 5 + h));
        } else if k.status & 0x80 != 0 {
            k.sway[5] = k.sway[5].wrapping_add(0x200 >> h);
        } else if k.effect == 0x16 {
            let idx = ((k.homing_timer as i32) & 0x3F) * 0x40;
            add = Some(div_pow2(t.sin_raw(idx), 6 + h));
        } else if k.effect == 0x15 {
            k.sway[5] = k.sway[5].wrapping_sub(0x200 >> h);
        }
        if let Some(a) = add {
            k.sway[10] = k.sway[10].wrapping_add(a);
            k.sway[11] = k.sway[11].wrapping_add(a);
        }
    }

    // 0x800688AC: smoothed sway outputs.
    let a = div_pow2(k.sway[1], 6);
    let b = bias(k.sway[7], 0x3F);
    for _ in 0..substeps(dt) {
        let s = k.sway_a as i32;
        k.sway_a = (s + (s16(a) - s) / 4) as i16;
        let s = k.sway_b as i32;
        k.sway_b = (s + ((b.wrapping_shl(10) >> 16) - s) / 4) as i16;
    }
    if k.blink_timer != 0 {
        let n = k.blink_timer as i32 - dt;
        k.blink_timer = n as i16;
        if n.wrapping_mul(0x10000) < 1 {
            k.blink = (k.blink.wrapping_add(1)) & 1;
            let r = rng_next(g, w);
            k.blink_timer = (((r >> 6) as i32 + 4) * 0x3C) as i16;
        }
    }
    let a = div_pow2(k.sway[2], 6);
    let b = div_pow2(k.sway[14], 6);
    let cc = bias(k.sway[8], 0x3F);
    for _ in 0..substeps(dt) {
        let s = k.sway_c as i32;
        k.sway_c = (s + (s16(a) - s) / 4) as i16;
        let s = k.sway_d as i32;
        k.sway_d = (s + (s16(b) - s) / 4) as i16;
        let s = k.sway_e as i32;
        k.sway_e = (s + ((cc.wrapping_shl(10) >> 16) - s) / 4) as i16;
    }
    if k.warn_timer != 0 {
        let n = k.warn_timer as i32 - dt;
        k.warn_timer = n as i8;
        if s8(n) < 0 {
            k.warn_timer = 0;
        }
    }

    // 0x80068B9C: wrong-way warning.
    if g.mode == 1 && human && k.pause_bits & 0x7F == 0 {
        let wb = (k.wobble as i32).abs();
        if g.race_flags & 2 == 0 {
            if wb < 0x600 {
                return;
            }
        } else if 0x1FF < wb {
            return;
        }
        if k.warn_timer == 0 {
            k.warn_timer = 0x1E;
        }
    }
}

/// 60 Hz mode: the world -> kart frame conversion of the velocity
/// (0x80067028) from the full-precision velocity the previous conversion
/// back left (`KartState::hz60_vel`), truncated once. A 30 Hz tick rounds
/// once each way; two 60 Hz ticks would round four times and lose speed
/// (top speed -0.2 %): carrying the remainder of one direction keeps two
/// roundings per 1/30 s.
fn to_kart_frame_60(k: &KartState, ca: i32, sa: i32) -> (i32, i32) {
    let (fx, fz) = match k.hz60_vel {
        Some([vx, vz, rx, rz]) if vx == k.vx && vz == k.vz => ((vx as i64) * 4096 + rx as i64, (vz as i64) * 4096 + rz as i64),
        _ => ((k.vx as i64) * 4096, (k.vz as i64) * 4096),
    };
    let tz = |v: i64| -> i32 { (if v < 0 { -((-v) >> 24) } else { v >> 24 }) as i32 };
    let side = tz(fx * ca as i64 + fz * sa as i64);
    let fwd = tz(fz * ca as i64 - fx * sa as i64);
    (side, fwd)
}

/// `x + bias` when negative: the first half of div_pow2, kept separate where
/// the game shifts the biased value in a different way.
fn bias(x: i32, b: i32) -> i32 {
    if x < 0 {
        x + b
    } else {
        x
    }
}

/// 0x80066B4C: accelerating (or boosting): lurch rises.
fn boost_lurch(k: &mut KartState, dt: i32) {
    k.coast_hold = 0;
    k.brake_hold = 0;
    k.throttle_hold = k.throttle_hold.wrapping_add(dt as u8);
    let inc = if k.boosting == 0 { dt << 2 } else { dt << 3 };
    let n = k.lurch as i32 + inc;
    k.lurch = n as i16;
    if s16(n) > 0x100 {
        k.lurch = 0x100;
    }
}

/// FUN_800434C8(kart, strength): pad rumble request (kept: it is kart state).
pub fn rumble(k: &mut KartState, g: &RaceGlobals, strength: i32) {
    if g.game_state == 0x1E {
        return;
    }
    let a = k.rumble_a as i32;
    let b = k.rumble_b as i32;
    if a != 0 && b != 0 {
        let cur = div(k.rumble as i32 * a, b);
        if strength <= cur {
            return;
        }
    }
    k.rumble = strength as u8;
    let q = div_pow2(strength, 2) as i8;
    k.rumble_b = q;
    k.rumble_a = q;
}

/// FUN_80064738: three body-sway spring axes, `dt` sub-steps each.
/// sway[] word i = kart +0x3A0 + 4i. Axis j (0..2) uses: p = sway[j] (pos),
/// pv = sway[3+j] (pos vel), q = sway[6+j], qv = sway[9+j], r = sway[12+j],
/// rv = sway[15+j]. Axes 0 and 1 are driven by sway[1+j] / sway[7+j]
/// scaled by `gain`; axis 2 chases `target` and kicks with random bytes.
pub fn sway_springs(k: &mut KartState, g: &mut RaceGlobals, w: &World, target: i32, gain: i32, effect: i32) {
    let s = &mut k.sway;
    for j in 0..3usize {
        for _ in 0..substeps(g.dt) {
            if j < 2 {
                let gg = if j != 0 { if effect == -1 { 0x30 } else { 0x18 } } else { gain };
                s[3 + j] = s[3 + j].wrapping_add(div_pow2(mul(s[1 + j], gg), 12));
                s[9 + j] = s[9 + j].wrapping_add(div_pow2(mul(s[7 + j], gg), 12));
            } else {
                let gg = if effect == -1 { 3 } else { 2 };
                let d = if gg == 3 { (target - s[12 + j]) >> 7 } else { (target - s[12 + j]) >> 8 };
                s[15 + j] = s[15 + j].wrapping_add(d);
                let r0 = w.rng_byte(g.rng) as i8 as i32;
                let r1 = w.rng_byte(g.rng.wrapping_add(1)) as i8 as i32;
                g.rng = g.rng.wrapping_add(2);
                s[3 + j] = s[3 + j].wrapping_add(r0 * gg);
                s[9 + j] = s[9 + j].wrapping_add(r1 * gg);
            }
            spring(s, j, 3 + j, 0x4000);
            spring(s, 6 + j, 9 + j, 0x4000);
            if j == 2 {
                spring(s, 12 + j, 15 + j, 0x10000);
            }
        }
    }
}

/// One damped spring step toward 0 with clamp (0x80064870 pattern).
fn spring(s: &mut [i32; 18], pi: usize, vi: usize, lim: i32) {
    let old = s[pi];
    if old < 0 {
        s[vi] += 0x80;
    } else if old > 0 {
        s[vi] -= 0x80;
    }
    let n = old.wrapping_add(s[vi]);
    s[pi] = n;
    if (old.wrapping_shl(16) < 0 && 0 < n) || (0 < old.wrapping_shl(16) && n < 0) {
        s[vi] = div_pow2(s[vi].wrapping_mul(3), 2);
    }
    if s[pi] < -lim {
        s[pi] = -lim;
    } else if lim < s[pi] {
        s[pi] = lim;
    }
    if s[pi].abs() == lim {
        s[vi] = 0;
    }
}

//! The computer drivers: FUN_800912F8 (one AI kart's buttons for this tick)
//! and its callees, the AI-only pieces of the kart update, and the race
//! loader's choice of opponents.
//!
//! An AI kart is an ordinary kart record with flag bit 0 of +0x496 set. The
//! race frame FUN_8007916C runs the SAME kart update FUN_80064B5C on it as on
//! the player, with player index -1 and, instead of the pad, the button word
//! this module produces (0 when +0x496 & 0x60). So the AI is a virtual pad:
//! steer toward a point a few TER path points ahead (offset back across the
//! racing line), always accelerate, brake while the turn is sharp, jump at
//! random in water / when stuck, press R1 (turbo) in water or on straights
//! when behind the player, L1/L2 (weapons) at random.
//!
//! Model, pseudocode and addresses: docs/spec/ai.md. Integers exactly as the
//! game (MIPS truncating arithmetic, 12-bit angles, 1.0 = 0x4000 sines).
//! Random numbers: the BIOS rand() (A0:2F, seed at 0x80009010) is passed in
//! as a closure; the game's random byte pointer 0x800B0BCC is
//! `RaceGlobals::rng` read through `World::rng_byte`, as in the kart update.
//!
//! Not one-for-one (revisit list, docs/spec/kart-physics-ledger.md): the
//! rand() stream is shared with other game code; weapons, the turbo meter and
//! the full-meter specials are out of scope this session (the buttons are
//! produced, the consequences are not ported).

use crate::fixed::{s16, wrap_angle};
use crate::globals::RaceGlobals;
use crate::kart::KartState;
use crate::tables::{ratan2, ExeImage};
use crate::world::World;

/// Button bits the AI produces (active high, PS1 pad order).
pub mod btn {
    pub const RIGHT: u16 = 0x0020;
    pub const LEFT: u16 = 0x0080;
    /// L2 / L1: fire weapon slot 1 / 0.
    pub const L2: u16 = 0x0100;
    pub const L1: u16 = 0x0400;
    /// R2: character special with a full meter.
    pub const R2: u16 = 0x0200;
    /// R1: turbo (uses the meter +0x48A; special move when it is full).
    pub const R1: u16 = 0x0800;
    pub const CIRCLE: u16 = 0x2000;
    pub const CROSS: u16 = 0x4000;
    pub const SQUARE: u16 = 0x8000;
    /// Buttons whose effects are out of scope this session (weapons, turbo,
    /// specials): `ai_buttons` produces them, `ItemsOff` masks them.
    pub const ITEMS: u16 = L1 | L2 | R1 | R2;
}

/// Full turbo meter (+0x48A): the value at which R1 / R2 become specials.
pub const METER_FULL: i16 = 0x1A4;

/// AI fields of the kart record that `KartState` does not model (only the
/// AI code reads or writes them). The steering target +0x3E8.. is
/// `KartState::ai_tx/ai_ty/ai_tz` (the special move FUN_800913F4 writes it
/// too).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AiState {
    /// +0x4A0: summed heading change over the points walked this tick
    /// (0x800B0CBE): the curvature ahead; small = straight.
    pub turn: i16,
    /// +0x4A2: battle-mode timer (FUN_80090FAC).
    pub follow_timer: i16,
}

impl AiState {
    pub fn from_record(r: &[u8]) -> AiState {
        let i32_ = |o: usize| i32::from_le_bytes(r[o..o + 4].try_into().unwrap());
        let i16_ = |o: usize| i16::from_le_bytes([r[o], r[o + 1]]);
        let _ = i32_;
        AiState { turn: i16_(0x4A0), follow_timer: i16_(0x4A2) }
    }

    pub fn to_record(&self, r: &mut [u8]) {
        r[0x4A0..0x4A2].copy_from_slice(&self.turn.to_le_bytes());
        r[0x4A2..0x4A4].copy_from_slice(&self.follow_timer.to_le_bytes());
    }
}

/// Race-wide values the AI reads that `RaceGlobals` does not carry.
#[derive(Clone, Copy, Debug, Default)]
pub struct AiContext<'a> {
    /// 0x800B0DFE: karts in the race (6 on DOCK). Also the staleness limit
    /// of the path position (+0x4CB).
    pub karts: i16,
    /// Kart 0 (the first human) +0x496 and +0x4BE: an AI behind kart 0
    /// "attacks" (R1 on straights, specials with a full meter).
    pub kart0_flags: u16,
    pub kart0_race_pos: u8,
    /// 0x800E039A: catch-up scale for AI karts, /16 (8 in the oracle's race).
    pub difficulty: u8,
    /// 0x800B0E38: cup / course slot; the AI catch-up applies below 7.
    pub cup: i8,
    /// Battle mode only (FUN_80090FAC): every kart's driver position
    /// (+0x330 / +0x338 / +0x340, world units) for the chase target +0x4DC.
    pub driver_pos: &'a [[i32; 3]],
}

/// Where the path walk (FUN_80090424 / FUN_80090670) ended.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Walk {
    /// 0x800B0B20: segment (-1: the kart is not on the path).
    pub seg: i16,
    /// 0x800B0F3C: point index in the segment.
    pub pt: i16,
    /// 0x800B0CBE: summed heading change between consecutive points walked.
    pub turn: i16,
}

fn point_heading(w: &World, seg: i32, pt: i32) -> u16 {
    let s = &w.ter.path_segments[s16(seg) as usize];
    w.ter.path_points[s16(s.first as i32 + s16(pt)) as usize].heading
}

fn link(w: &World, i: i32) -> i32 {
    w.ter.links[i as usize] as i32
}

/// Pick a successor (or predecessor) link of `seg`: links `first..first+n`.
/// The first link is taken outright; with `first_only` (0x800B0A20 set,
/// the player's track-direction walk) that is final, otherwise each later
/// link replaces it with probability 1/2 (`rand() < 0x4000`). -1: no links.
fn pick_link(w: &World, first: i32, n: i32, first_only: bool, rand: &mut dyn FnMut() -> i32) -> i32 {
    let mut cand: i32 = -1;
    let mut i = 0;
    while i < n {
        if cand == -1 || rand() < 0x4000 {
            cand = link(w, first + i);
            if first_only {
                break;
            }
        }
        i += 1;
    }
    cand
}

/// FUN_80090424: walk `n` points forward along the TER path from (seg, pt).
/// At a segment end the next segment is a successor link (random among
/// several, see `pick_link`), entered at point 1; a dead end stays on the
/// last point. `first_only` = the global 0x800B0A20 (the player's walk sets
/// it; the AI's never does). The walk always clears it afterwards.
pub fn walk_forward(w: &World, seg: i16, pt: i16, n: i16, first_only: bool, rand: &mut dyn FnMut() -> i32) -> Walk {
    let mut out = Walk { seg, pt, turn: 0 };
    if seg == -1 {
        // 0x80090464: segment -1; the point global keeps its old value
        // (the callers never read it then).
        out.seg = -1;
        return out;
    }
    let ter = &w.ter;
    let mut seg = seg as i32;
    let mut pt = pt as i32;
    let mut n = n as i32;
    let mut turn: i32 = 0;
    while s16(n) > 0 {
        let cur = pt;
        pt = s16(pt + 1);
        let s = &ter.path_segments[s16(seg) as usize];
        let h0 = point_heading(w, seg, cur);
        if !(s16(pt) < s.count as i32) {
            let next = if s.n_next > 0 { pick_link(w, s.link_start as i32, s.n_next as i32, first_only, rand) } else { -1 };
            if next == -1 {
                pt = s.count as i32 - 1;
            } else {
                seg = next;
                pt = 1;
            }
        }
        let h1 = point_heading(w, seg, pt);
        turn += wrap_angle(s16(h1 as i32 - h0 as i32));
        n -= 1;
    }
    out.seg = s16(seg) as i16;
    out.pt = s16(pt) as i16;
    out.turn = turn as i16;
    out
}

/// FUN_80090670: walk `n` points backward (path mode 1: the kart drives the
/// course the other way). A segment without predecessors is first left
/// forward (FUN_80090424 one point at a time, at most n + 1 points); a
/// predecessor (random among several) is entered at its second-to-last
/// point; none: point 0.
pub fn walk_backward(w: &World, seg: i16, pt: i16, n: i16, rand: &mut dyn FnMut() -> i32) -> Walk {
    let mut out = Walk { seg, pt, turn: 0 };
    if seg == -1 {
        out.seg = -1;
        return out;
    }
    let ter = &w.ter;
    let mut seg = seg as i32;
    let mut pt = pt as i32;
    let mut n = n as i32;
    let mut turn: i32 = 0;
    if ter.path_segments[s16(seg) as usize].n_prev == 0 {
        // 0x800906E4: step forward until the segment changes.
        let start = seg;
        n += 1;
        loop {
            if s16(n) == 0 {
                out.seg = s16(seg) as i16;
                out.pt = pt as u16 as i16;
                out.turn = turn as i16;
                return out;
            }
            let wk = walk_forward(w, seg as i16, pt as i16, 1, false, rand);
            n -= 1;
            seg = wk.seg as i32;
            pt = wk.pt as u16 as i32;
            turn = wk.turn as i32;
            if seg != start {
                break;
            }
        }
    }
    while s16(n) > 0 {
        let cur = pt;
        pt = s16(pt - 1);
        let s = &ter.path_segments[s16(seg) as usize];
        let h0 = point_heading(w, seg, cur);
        if pt < 0 {
            let prev = if s.n_prev > 0 {
                pick_link(w, s.link_start as i32 + s.n_next as i32, s.n_prev as i32, false, rand)
            } else {
                -1
            };
            if prev == -1 {
                pt = 0;
            } else {
                seg = prev;
                pt = ter.path_segments[prev as usize].count as i32 - 2;
            }
        }
        let h1 = point_heading(w, seg, pt);
        turn += wrap_angle(s16(h1 as i32 - h0 as i32));
        n -= 1;
    }
    out.seg = s16(seg) as i16;
    out.pt = s16(pt) as i16;
    out.turn = turn as i16;
    out
}

/// One kart of FUN_8009192C's loop: +0x47A = 0, then one random byte b:
/// b >> 6 == 0 -> +0x496 bit 2 (R1 special), 1 -> bit 4 (R2 special),
/// 2/3 -> no preference and the re-roll timer +0x47A = ((next byte >> 3) +
/// 29) * 60 (29..60 s in dt units). Race types 2/3 never take bit 2;
/// mode 7 forces "no preference" for karts at or past `karts`.
fn roll(k: &mut KartState, g: &mut RaceGlobals, w: &World, karts: i16) {
    k.respawn_timer = 0;
    let b = w.rng_byte(g.rng);
    g.rng = g.rng.wrapping_add(1);
    let mut v = b >> 6;
    if v == 0 && (g.race_type as u16).wrapping_sub(2) < 2 {
        v = 1;
    }
    if g.mode == 7 && (k.player_index as i32) >= karts as i32 {
        v = 2;
    }
    match v {
        0 => k.flags |= 2,
        1 => k.flags |= 4,
        _ => {
            let b2 = w.rng_byte(g.rng);
            g.rng = g.rng.wrapping_add(1);
            k.respawn_timer = (((b2 >> 3) as i32 + 29) * 60) as i16;
        }
    }
}

/// FUN_8009192C(kart, 1): re-roll one kart's special-move preference (the
/// update calls it when the timer +0x47A runs out; the AI when it may not
/// use a full meter). Clears +0x496 bits 2 and 4 first.
pub fn reroll(k: &mut KartState, g: &mut RaceGlobals, w: &World, karts: i16) {
    k.flags &= !(2 | 4);
    roll(k, g, w, karts);
}

/// FUN_8009192C(&kart0, karts) as the race loader calls it after placing
/// every kart (0x8008E9xx): bits 2 / 4 are cleared on kart 0 only, then
/// every kart (humans too: the player's +0x496 = 2 in the oracle's race)
/// is rolled in slot order.
pub fn race_load_roll(karts: &mut [KartState], g: &mut RaceGlobals, w: &World, n: i16) {
    if let Some(k0) = karts.first_mut() {
        k0.flags &= !(2 | 4);
    }
    for k in karts.iter_mut() {
        roll(k, g, w, n);
    }
}

/// FUN_80090914: buttons to drive kart `k` at the point `t` (TER units).
/// `seg` is a segment the kart must be on (-1: none); `turn` is the
/// curvature ahead the record holds (+0x4A0, `AiState::turn`, written by
/// this tick's path walk). May reset a full meter and re-roll (`reroll`)
/// when the AI is not allowed to use it.
#[allow(clippy::too_many_arguments)]
pub fn steer_at(
    k: &mut KartState,
    g: &mut RaceGlobals,
    ctx: &AiContext,
    w: &World,
    t: [i32; 3],
    seg: i16,
    turn: i16,
    rand: &mut dyn FnMut() -> i32,
) -> u16 {
    let mut b: u16 = 0;
    let a = ratan2(&w.tables, t[0] - (k.x >> 5), t[2] - (k.z >> 5));
    let d = s16(wrap_angle(s16((a & 0xFFF) - k.heading as u16 as i32)));
    let thr = s16((k.steer_rate as i32) << 2);
    if thr < d {
        b |= btn::RIGHT;
    } else if d < -thr {
        b |= btn::LEFT;
    }
    // 0x800909C0: a paused / stalled AI (+0x4F9 & 0x7F) only steers.
    if k.pause_bits & 0x7F != 0 {
        return b;
    }
    let sharp = (k.steer_rate as i32) * 4 < d.abs();
    if g.countdown == 0 {
        b |= btn::CROSS;
    }
    if sharp || (seg != -1 && seg != k.path_seg) {
        b |= btn::CIRCLE;
    }
    // 0x80090A38: jump (1/4 per tick) in water, on surfaces 1 / 5, when
    // nearly stopped after the start, or touching an object.
    let mut jump = k.float_state != 0 || k.surface == 1 || k.surface == 5;
    if g.race_clock > 150 && k.speed_est < 0x180 {
        jump = true;
    }
    if k.object_e4 != -1 {
        jump = true;
    }
    if jump && rand() < 0x2000 {
        b |= btn::SQUARE;
    }
    // 0x80090C64: turbo whenever afloat.
    let water_turbo = |k: &KartState, b: u16| if k.float_state != 0 { b | btn::R1 } else { b };
    let f = k.flags;
    if f & 0x18 != 0 {
        // Battle-mode AI (FUN_80090FAC states).
        if f & 2 != 0 {
            if k.start_timer != METER_FULL {
                return water_turbo(k, b);
            }
            if k.close_kart == 0 || k.impact < 801 {
                return b;
            }
            return b | btn::R1;
        }
        if f & 4 != 0 {
            if k.start_timer != METER_FULL {
                return water_turbo(k, b);
            }
            if k.close_kart == 0 || f & 0x10 == 0 {
                return b;
            }
            return b | btn::R2;
        }
        if k.close_kart != 0 {
            return b | btn::R1;
        }
        return water_turbo(k, b);
    }
    // 0x80090B6C: behind kart 0 (or split screen, or kart 0 is an AI):
    // use a full meter, and turbo on straights.
    let behind = g.split_screen != 0
        || ctx.kart0_flags & 1 != 0
        || (k.player_index > 0 && ctx.kart0_race_pos < k.race_pos);
    if f & 2 != 0 || f & 4 != 0 {
        if k.start_timer != METER_FULL {
            return water_turbo(k, b);
        }
        if behind {
            return b | if f & 2 != 0 { btn::R1 } else { btn::R2 };
        }
        // Not allowed to use it: drop to half and re-roll the preference.
        k.start_timer = 210;
        reroll(k, g, w, ctx.karts);
        return b;
    }
    if behind && (turn as i32).abs() < 0x180 {
        return b | btn::R1;
    }
    water_turbo(k, b)
}

/// The random weapon press shared by FUN_80090D00 and FUN_80090FAC: one
/// rand() < 0x400 (1/32), then L2 if a second rand() > 0x3FFF, else L1.
fn weapon_roll(rand: &mut dyn FnMut() -> i32) -> u16 {
    let r = rand();
    let r = if r < 0 { r + 0x7F } else { r };
    if (r << 9) >> 16 < 8 {
        if rand() > 0x3FFF {
            btn::L2
        } else {
            btn::L1
        }
    } else {
        0
    }
}

/// FUN_80090D00: follow the TER path. Look ahead 0..3 points by speed (+1
/// when the path position is stale), aim at that point moved across the
/// line by twice the kart's lateral offset (only with 3+ points ahead), and
/// steer at it.
pub fn follow_path(
    k: &mut KartState,
    ai: &mut AiState,
    g: &mut RaceGlobals,
    ctx: &AiContext,
    w: &World,
    mode: i8,
    rand: &mut dyn FnMut() -> i32,
) -> u16 {
    if k.path_seg == -1 || k.special != 0 {
        return 0;
    }
    let t = &w.tables;
    let v = k.speed_est >> 4;
    let mut n: i32 = 0;
    if v >= 12 {
        n = v.clamp(64, 192) >> 6;
    }
    // +0x4CB: ticks (dt units) since FUN_8009004C last placed this kart on
    // the path (it runs for one kart per frame, round robin).
    let c = k.path_flag_4cb as i32 + g.dt;
    k.path_flag_4cb = c as u8;
    if (ctx.karts as i32) < (c & 0xFF) {
        n += 1;
    }
    let wk = if mode == 0 {
        walk_forward(w, k.path_seg, k.path_pt, s16(n) as i16, false, rand)
    } else {
        walk_backward(w, k.path_seg, k.path_pt, s16(n + 1) as i16, rand)
    };
    ai.turn = wk.turn;
    let s = &w.ter.path_segments[wk.seg as usize];
    let p = &w.ter.path_points[s16(s.first as i32 + wk.pt as i32) as usize];
    let mut tx = p.x;
    let ty = p.y;
    let mut tz = p.z;
    let off = if k.path_side == 0 && s16(n) > 2 { k.path_lateral.wrapping_neg().wrapping_mul(2) } else { 0 };
    let a = ((p.heading as i32) + 0x400) & 0xFFF;
    tx = tx.wrapping_add(crate::fixed::div_pow2(off.wrapping_mul(t.sin_raw(a)), 14));
    tz = tz.wrapping_add(crate::fixed::div_pow2(off.wrapping_mul(t.cos_raw(a)), 14));
    (k.ai_tx, k.ai_ty, k.ai_tz) = (tx, ty, tz);
    let seg = k.path_seg;
    let mut b = steer_at(k, g, ctx, w, [tx, ty, tz], seg, ai.turn, rand);
    if k.pause_bits & 0x7F == 0 {
        b |= weapon_roll(rand);
    }
    b
}

/// `rand() / 5462` as the compiler emits it (multiply by 0x0BFFA003, take
/// bits 40.., subtract the sign): the battle timer's 0..5 seconds.
fn div5462(r: i32) -> i32 {
    let q = ((r as i64 * 0x0BFF_A003) >> 40) as i32;
    s16(q - (r >> 31))
}

/// FUN_80090FAC: battle mode (race type 2 or race flag 1). Alternates
/// between chasing the nearest kart (+0x4DC, its driver position) and
/// following the path in a random direction, on a 5..10 s timer.
pub fn battle(
    k: &mut KartState,
    ai: &mut AiState,
    g: &mut RaceGlobals,
    ctx: &AiContext,
    w: &World,
    rand: &mut dyn FnMut() -> i32,
) -> u16 {
    if g.countdown == 0 {
        let t = (ai.follow_timer as u16 as i32) - g.dt;
        ai.follow_timer = t as i16;
        let mut to_path = false;
        let mut set_mode: Option<u8> = None;
        if t.wrapping_mul(0x10000) < 1 {
            ai.follow_timer = 0;
            if k.flags & 8 == 0 {
                to_path = true;
            } else if k.close_kart != 0 {
                k.flags = (k.flags & 6) | 0x11;
                ai.follow_timer = ((div5462(rand()) + 5) * 0x3C) as i16;
            } else if rand() <= 0x1F {
                set_mode = Some((k.path_mode.wrapping_add(1)) & 1);
            }
        } else if k.flags & 0x10 != 0 && k.close_kart == 0 {
            to_path = true;
        }
        if to_path {
            k.flags = (k.flags & 6) | 9;
            let r = rand();
            let b = w.rng_byte(g.rng);
            g.rng = g.rng.wrapping_add(1);
            ai.follow_timer = ((div5462(r) + 5) * 0x3C) as i16;
            set_mode = Some(b >> 7);
        }
        if let Some(m) = set_mode {
            k.path_mode = m;
        }
    }
    if k.flags & 8 == 0 || k.path_seg == -1 {
        if k.special != 0 {
            return 0;
        }
        let mut b = 0;
        if k.nearest_kart != -1 {
            let p = ctx.driver_pos[k.nearest_kart as usize];
            (k.ai_tx, k.ai_ty, k.ai_tz) = (p[0] >> 5, p[1] >> 5, p[2] >> 5);
            b = steer_at(k, g, ctx, w, [k.ai_tx, k.ai_ty, k.ai_tz], -1, ai.turn, rand);
        }
        b | weapon_roll(rand)
    } else {
        follow_path(k, ai, g, ctx, w, k.path_mode as i8, rand)
    }
}

/// FUN_800912F8 as the race frame calls it: the AI kart's button word for
/// this tick (0 when +0x496 & 0x60: the kart is parked by the race code).
/// Call after the frame's contact pass and path-position update, before the
/// kart's own update (FUN_80064B5C with these buttons and player -1).
pub fn ai_buttons(
    k: &mut KartState,
    ai: &mut AiState,
    g: &mut RaceGlobals,
    ctx: &AiContext,
    w: &World,
    rand: &mut dyn FnMut() -> i32,
) -> u16 {
    if k.flags & 0x60 != 0 {
        return 0;
    }
    if g.race_type == 3 {
        // Time-trial ghost after the finish: spin on the spot.
        let mut b = 0;
        if k.finished != 0 {
            let b0 = w.rng_byte(g.rng);
            let b1 = w.rng_byte(g.rng.wrapping_add(1));
            g.rng = g.rng.wrapping_add(2);
            if b0 < 0x10 {
                b = btn::SQUARE;
            }
            if b1 == 0 {
                k.path_mode = (k.path_mode.wrapping_add(1)) & 1;
            }
            b |= if (k.path_mode as i8) < 2 { btn::LEFT } else { btn::RIGHT };
        }
        return b;
    }
    if g.race_type == 2 || g.race_flags & 1 != 0 {
        return battle(k, ai, g, ctx, w, rand);
    }
    follow_path(k, ai, g, ctx, w, k.path_mode as i8, rand)
}

/// 0x80065850 (inside FUN_80064B5C, right after the catch-up step): an AI
/// kart's catch-up +0x4C3 is scaled by the difficulty 0x800E039A (x/16,
/// toward zero) in a one-screen race that is not over, below cup slot 7
/// (0x800B0E38) and outside game state 0x1F; +0x496 bit 0x20 exempts a
/// kart. update.rs has `unreachable_in_scope!("AI catch-up scaling")` there:
/// replace it with this call.
pub fn catchup_scale(k: &mut KartState, g: &RaceGlobals, difficulty: u8, cup: i8) {
    if g.split_screen != 0 || g.race_over != 0 || k.flags & 1 == 0 || k.flags & 0x20 != 0 {
        return;
    }
    if (cup as i32) < 7 && g.game_state != 0x1F {
        let v = (k.catchup as i32) * difficulty as i32;
        k.catchup = crate::fixed::div_pow2(v, 4) as u8;
    }
}

/// Session 3: items are out of scope, so the port's AI drops the buttons
/// whose effects are not ported (weapons L1/L2, turbo R1, special R2). With
/// the turbo meter never filling (FUN_8007BBF0 not ported) and weapon slots
/// empty, the game itself ignores them; masking keeps the record's +0x44E
/// comparable with an items-off oracle run.
pub fn items_off(b: u16) -> u16 {
    b & !btn::ITEMS
}

// ---------------------------------------------------------------------
// Opponents (race loader FUN_8008C4AC, 0x8008E49C..0x8008E7C4).
// ---------------------------------------------------------------------

/// Exe tables the race loader reads to pick opponents.
pub mod addr {
    /// Character table, 0x14 bytes per character: +0x0E flags (bit 0 = can
    /// race), +0x0F default vehicle.
    pub const CHARACTERS: u32 = 0x800A_C220;
    /// Vehicle table, 4 bytes per vehicle: +3 flags (bit 0 = selectable).
    pub const VEHICLES: u32 = 0x800A_C5E4;
    /// Model paths (char *[48]): vehicles ("vehic_a\racer"), characters
    /// ("chars_a\kermit"); the loader appends ".mdl" / ".anm".
    pub const VEHICLE_PATHS: u32 = 0x800A_D10C;
    pub const CHARACTER_PATHS: u32 = 0x800A_D1CC;
}

pub const N_CHARACTERS: usize = 48;

/// A character's racing flag and default vehicle (exe 0x800AC22E/F).
pub fn character_entry(exe: &ExeImage, c: u8) -> (bool, u8) {
    let base = addr::CHARACTERS + 0x14 * c as u32;
    (exe.u8(base + 0x0E) & 1 != 0, exe.u8(base + 0x0F))
}

/// A model path from one of the exe's path tables, as a disc path
/// ("VEHIC_A/RACER.MDL"), or None for an empty entry.
pub fn model_path(exe: &ExeImage, table: u32, i: u8, ext: &str) -> Option<String> {
    let p = u32::from_le_bytes([exe.u8(table + 4 * i as u32), exe.u8(table + 4 * i as u32 + 1), exe.u8(table + 4 * i as u32 + 2), exe.u8(table + 4 * i as u32 + 3)]);
    if !exe.contains(p) {
        return None;
    }
    let mut s = String::new();
    let mut a = p;
    loop {
        let c = exe.u8(a);
        if c == 0 {
            break;
        }
        s.push(c as char);
        a += 1;
    }
    if !s.contains('\\') || s.ends_with('\\') {
        return None;
    }
    Some(format!("{}.{}", s.replace('\\', "/").to_ascii_uppercase(), ext))
}

/// One opponent the race loader placed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Opponent {
    pub slot: u8,
    pub character: u8,
    pub vehicle: u8,
    pub grid: i32,
}

/// The race loader's choice for one AI slot in a plain race (race flags
/// 0x10 / 0x80 / 0x100 clear; 0x8008E4FC): draw characters `rand() / 683`
/// (0..47) until one that is not already in the race (`taken`: the karts
/// placed so far, humans included), has a model (`available`: RAM
/// 0x800C3028 != -1, all 25 racers on the disc) and can race (character
/// table flag); its vehicle is the character's default, re-drawn at random
/// (selectable vehicles) only while it is 0xFF or not loaded
/// (`vehicle_loaded`: RAM 0x800D1560 != -1), which no racer needs.
/// The game calls FUN_800609F4 (init::init_kart, flags 1) for the slot right
/// after the draw, and that draws rand() too (wheel angles): interleave the
/// two per slot to keep the rand() stream in the game's order.
pub fn pick_opponent(
    exe: &ExeImage,
    taken: &[u8],
    available: &dyn Fn(u8) -> bool,
    vehicle_loaded: &dyn Fn(u8) -> bool,
    rand: &mut dyn FnMut() -> i32,
) -> (u8, u8) {
    let c = loop {
        let c = s16(rand() / 0x2AB) as u8;
        if taken.contains(&c) {
            continue;
        }
        let (racer, _) = character_entry(exe, c);
        if available(c) && racer {
            break c;
        }
    };
    let (_, mut v) = character_entry(exe, c);
    while v == 0xFF || !vehicle_loaded(v) {
        loop {
            v = s16(rand() / 0x2AB) as u8;
            if exe.u8(addr::VEHICLES + 4 * v as u32 + 3) & 1 != 0 {
                break;
            }
        }
    }
    (c, v)
}

/// `pick_opponent` for slots humans..karts in a row (grid slots continue
/// from `grid`, modulo `karts`). Only for when the rand() order relative to
/// init_kart does not matter.
#[allow(clippy::too_many_arguments)]
pub fn pick_opponents(
    exe: &ExeImage,
    taken: &[u8],
    humans: usize,
    karts: usize,
    mut grid: i32,
    available: &dyn Fn(u8) -> bool,
    vehicle_loaded: &dyn Fn(u8) -> bool,
    rand: &mut dyn FnMut() -> i32,
) -> Vec<Opponent> {
    let mut chosen: Vec<u8> = taken.to_vec();
    let mut out = Vec::new();
    for slot in humans..karts {
        let (c, v) = pick_opponent(exe, &chosen, available, vehicle_loaded, rand);
        chosen.push(c);
        out.push(Opponent { slot: slot as u8, character: c, vehicle: v, grid });
        grid = (grid + 1) % karts as i32;
    }
    out
}

/// 0x8008E3xx: the first human's grid slot, `*rng++ % karts`; later karts
/// (humans, then AI) take the following slots modulo `karts`.
pub fn grid_start(g: &mut RaceGlobals, w: &World, karts: i16) -> i32 {
    let b = w.rng_byte(g.rng);
    g.rng = g.rng.wrapping_add(1);
    b as i32 % karts as i32
}

// ---------------------------------------------------------------------
// Vehicle templates for every vehicle (FUN_80087EB4 + FUN_80087B6C).
// ---------------------------------------------------------------------

/// The template table in RAM: 48 vehicles x 0x174 bytes (copied to kart
/// +0x14..+0x188 by FUN_800609F4).
pub const TEMPLATES: u32 = 0x800D_8FD0;
pub const N_VEHICLES: usize = 48;
pub const TEMPLATE_SIZE: usize = 0x174;
/// FUN_80087EB4's per-vehicle values: a straight run of `li` / `lui` /
/// `sh` / `sb` (and loads of values it stored a moment before) between the
/// defaults loop and the radius loop. They are data written as code; the
/// decoder below replays exactly those instructions over the table.
pub const OVERRIDES: (u32, u32) = (0x8008_8018, 0x8008_99DC);

#[derive(Debug)]
pub struct DecodeError(pub String);

/// FUN_80087EB4 without its last call (FUN_80087D10: the menu's stat
/// ranges): all 48 templates as the game builds them at boot, before the
/// track loader adds wheel positions (`place_wheels`).
pub fn vehicle_templates(exe: &ExeImage, t: &crate::tables::Tables) -> Result<Vec<[u8; TEMPLATE_SIZE]>, DecodeError> {
    let mut buf = vec![0u8; N_VEHICLES * TEMPLATE_SIZE];
    // 0x80087EFC: defaults for every vehicle.
    for v in 0..N_VEHICLES {
        let b = &mut buf[v * TEMPLATE_SIZE..(v + 1) * TEMPLATE_SIZE];
        let put16 = |b: &mut [u8], o: usize, x: i32| b[o..o + 2].copy_from_slice(&(x as u16).to_le_bytes());
        b[0x15C] = 0;
        b[0x15E] = 0;
        for j in 0..3 {
            b[0x11A + 0x18 * j] = 0;
            b[0x11B + 0x18 * j] = 0x20;
            b[0x11F + 0x18 * j] = 0x32;
        }
        b[0x15F] = 0x40;
        b[0x160] = 1;
        put16(b, 0x154, 200);
        put16(b, 0x152, 200);
        put16(b, 0x150, -200);
        put16(b, 0x156, -200);
        b[0x158] = 160;
        b[0x159] = 0;
        let st = &t.vehicle[v];
        for i in 0..5 {
            put16(b, 0x164 + 2 * i, st[i] as i32);
        }
        put16(b, 0x15A, 87);
        put16(b, 0x16E, 0);
        put16(b, 0x170, -175);
        put16(b, 0x172, 0);
    }
    // 0x80088018..0x800899DC: the per-vehicle values.
    let mut r = [0u32; 32];
    let base = TEMPLATES;
    let end = TEMPLATES + (N_VEHICLES * TEMPLATE_SIZE) as u32;
    let idx = |a: u32, n: u32| -> Result<usize, DecodeError> {
        if a < base || a + n > end {
            return Err(DecodeError(format!("access {a:08X} outside the template table")));
        }
        Ok((a - base) as usize)
    };
    let mut pc = OVERRIDES.0;
    while pc < OVERRIDES.1 {
        let ins = u32::from_le_bytes([exe.u8(pc), exe.u8(pc + 1), exe.u8(pc + 2), exe.u8(pc + 3)]);
        let op = ins >> 26;
        let rs = ((ins >> 21) & 31) as usize;
        let rt = ((ins >> 16) & 31) as usize;
        let rd = ((ins >> 11) & 31) as usize;
        let imm = ins as u16 as i16 as i32 as u32;
        let ea = r[rs].wrapping_add(imm);
        match op {
            0x00 => match ins & 63 {
                0x00 if ins == 0 => {}
                0x21 => r[rd] = r[rs].wrapping_add(r[rt]),
                0x25 => r[rd] = r[rs] | r[rt],
                f => return Err(DecodeError(format!("{pc:08X}: special funct {f:#x}"))),
            },
            0x09 => r[rt] = r[rs].wrapping_add(imm),
            0x0D => r[rt] = r[rs] | (ins & 0xFFFF),
            0x0F => r[rt] = (ins & 0xFFFF) << 16,
            0x20 => r[rt] = buf[idx(ea, 1)?] as i8 as i32 as u32,
            0x24 => r[rt] = buf[idx(ea, 1)?] as u32,
            0x21 => { let i = idx(ea, 2)?; r[rt] = i16::from_le_bytes([buf[i], buf[i + 1]]) as i32 as u32 }
            0x25 => { let i = idx(ea, 2)?; r[rt] = u16::from_le_bytes([buf[i], buf[i + 1]]) as u32 }
            0x23 => { let i = idx(ea, 4)?; r[rt] = u32::from_le_bytes(buf[i..i + 4].try_into().unwrap()) }
            0x28 => { let i = idx(ea, 1)?; buf[i] = r[rt] as u8 }
            0x29 => { let i = idx(ea, 2)?; buf[i..i + 2].copy_from_slice(&(r[rt] as u16).to_le_bytes()) }
            0x2B => { let i = idx(ea, 4)?; buf[i..i + 4].copy_from_slice(&r[rt].to_le_bytes()) }
            _ => return Err(DecodeError(format!("{pc:08X}: opcode {op:#x} in the store run"))),
        }
        r[0] = 0;
        pc += 4;
    }
    // 0x800899DC: kart radius from the probe corners, and the wheels that
    // carry the kart (flags & 0x78 == 0; suspension rest at least 0x40).
    let mut out = Vec::with_capacity(N_VEHICLES);
    for v in 0..N_VEHICLES {
        let mut b = [0u8; TEMPLATE_SIZE];
        b.copy_from_slice(&buf[v * TEMPLATE_SIZE..(v + 1) * TEMPLATE_SIZE]);
        let h = |b: &[u8; TEMPLATE_SIZE], o: usize| i16::from_le_bytes([b[o], b[o + 1]]) as i32;
        let q = |x: i32| crate::fixed::s16(crate::fixed::div_pow2(x * 3, 2));
        let d1 = q(h(&b, 0x150)).pow(2) + q(h(&b, 0x154)).pow(2);
        let d2 = q(h(&b, 0x152)).pow(2) + q(h(&b, 0x156)).pow(2);
        let rad = crate::tables::square_root0(t, d1.max(d2));
        b[0x162..0x164].copy_from_slice(&(rad as u16).to_le_bytes());
        let mut n = 0u8;
        for i in 0..b[0x15C] as usize {
            let w = 0x2C * i;
            if h(&b, w + 0x26) & 0x78 == 0 {
                n = n.wrapping_add(1);
                if h(&b, w + 0x24) < 0x40 {
                    b[w + 0x24..w + 0x26].copy_from_slice(&0x40u16.to_le_bytes());
                }
            }
        }
        b[0x15D] = n;
        out.push(b);
    }
    Ok(out)
}

/// FUN_80087B6C for one loaded vehicle: wheel i sits at the vehicle
/// model's part i pivot, x and z negated (FUN_80060944); its radius is
/// |y|, at least 0x20.
pub fn place_wheels(b: &mut [u8; TEMPLATE_SIZE], model: &mr_assets::mdl::Mdl) {
    for i in 0..b[0x15C] as usize {
        let p = model.parts[i].record.pivot;
        let w = 0x2C * i;
        b[w + 0x18..w + 0x1A].copy_from_slice(&((-p[0]) as i16).to_le_bytes());
        b[w + 0x1A..w + 0x1C].copy_from_slice(&(p[1] as i16).to_le_bytes());
        b[w + 0x1C..w + 0x1E].copy_from_slice(&((-p[2]) as i16).to_le_bytes());
        let r = (p[1] as i16).wrapping_abs();
        b[w + 0x22..w + 0x24].copy_from_slice(&(if r < 0x20 { 0x20i16 } else { r }).to_le_bytes());
    }
}

/// FUN_800609F4 for an AI slot (flags 1): `init::init_kart` with the
/// vehicle's template (`vehicle_templates` + `place_wheels`) and the
/// character model's first part pivot y (CHARS_*/<name>.MDL,
/// `model_path(exe, addr::CHARACTER_PATHS, character, "MDL")`).
#[allow(clippy::too_many_arguments)]
pub fn init_opponent(
    w: &World,
    template: &[u8; TEMPLATE_SIZE],
    setup: crate::init::RaceSetup,
    o: Opponent,
    character_pivot_y: i32,
    rng: &mut u32,
    rand: &mut dyn FnMut() -> i32,
) -> (Vec<u8>, KartState) {
    crate::init::init_kart(
        &crate::init::KartInit {
            world: w,
            template,
            setup,
            slot: o.slot,
            character: o.character,
            vehicle: o.vehicle,
            grid: o.grid,
            flags: 1,
            character_pivot_y,
        },
        rng,
        rand,
    )
}

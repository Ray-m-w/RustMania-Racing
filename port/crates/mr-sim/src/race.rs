//! Race flow around the kart updates: the countdown and race clock
//! (FUN_8005C2EC), the start boosts at GO, race progress and positions
//! (FUN_800837F8 -> FUN_80081170 mode 0x10, FUN_800834C0), and the race
//! phase / results view the HUD reads.
//!
//! Units: countdown and race clock as the game keeps them. The countdown
//! 0x800B0C70 counts VBlanks down (dt per tick) from 480 at race load; the
//! "Copy the buttons" start game runs while it is below 300 (0x800B07F8); GO
//! is the tick it reaches 0. The race clock 0x800B0A94 (and every kart's
//! race time +0x358, lap times +0x35C) counts 5 per VBlank: 300 = 1 second.

use crate::fixed::s16;
use crate::globals::RaceGlobals;
use crate::kart::KartState;
use crate::tables::square_root0;
use crate::world::World;

/// The countdown at race load: 300 + 180 VBlanks (FUN_8008C4AC @0x8008DA4C).
pub const COUNTDOWN_START: i32 = 300 + 180;
/// Race-clock units per second.
pub const CLOCK_PER_SECOND: i32 = 300;

/// FUN_8005C2EC, the part that runs while a race frame is live: frame
/// counter, race clock, countdown and the start boosts at GO.
/// `rand` is the BIOS rand() (the game draws one every frame).
pub fn frame_clock(g: &mut RaceGlobals, karts: &mut [&mut KartState], best_start: &mut u8, rand: &mut dyn FnMut() -> i32) {
    g.frame = g.frame.wrapping_add(g.dt);
    if g.countdown == 0 {
        g.race_clock = g.race_clock.wrapping_add(g.dt * 5);
    } else {
        crate::sound::countdown(g);
        g.countdown -= g.dt;
        if g.countdown < 1 {
            go(g, karts, best_start, rand);
        }
    }
    // The per-frame draw (60 Hz mode: once per pair, the game's rate).
    if g.hz60.beat() {
        rand();
    }
}

/// 0x8005C4A8: GO. Human karts keep the boost they earned in the start game
/// (+0x446); 0x800E02BA remembers the best; every kart's boost becomes
/// ticks (<< 3) and the HUD flash +0x4FC = 0xB shows it.
fn go(g: &mut RaceGlobals, karts: &mut [&mut KartState], best_start: &mut u8, rand: &mut dyn FnMut() -> i32) {
    // 0x8005C538: AI karts draw their boost as rand() / d, d from the best
    // start so far (0x800E02BA, as it stood before this GO) and the AI
    // difficulty 0x800E039A: d = 32767 / (best * difficulty / 16 + 1) + 1
    // (as an i16: 32768 wraps to -32768, so with no best start yet the AI
    // gets 0).
    let scaled = crate::fixed::div_pow2(*best_start as i32 * g.difficulty as i8 as i32, 4) + 1;
    let d = crate::fixed::s16(32767 / scaled + 1);
    let mut best = 0i32;
    for k in karts.iter_mut() {
        if g.mode == 1 {
            if k.flags & 1 != 0 {
                k.boost_count = (rand() / d) as i16;
            } else if (*best_start as i16) < k.boost_count {
                *best_start = k.boost_count as u8;
            }
        } else {
            k.boost_count = 0;
        }
        best = best.max(k.boost_count as i32);
    }
    // 0x800B0E8C: the karts with the best start (bit per slot).
    g.start_best_mask = 0;
    if best > 0 {
        for (i, k) in karts.iter().enumerate() {
            if k.boost_count as i32 == best {
                g.start_best_mask |= 1 << i;
            }
        }
    }
    g.countdown = 0;
    for k in karts.iter_mut() {
        if g.mode == 1 {
            k.flash_timer = 0xB;
            k.blink_count = 0;
            k.hit_dir = k.boost_count;
        }
        k.boost_count = k.boost_count.wrapping_shl(3);
    }
}

/// FUN_80081170(x >> 5, y >> 5, z >> 5, course, 0x10, from, 0): the course
/// point (DAT chunk 5) nearest to the kart, searched around `from`; returns
/// (index, signed step from `from`, wrapped to the shorter way round).
pub fn course_point(w: &World, k: &KartState, from: i16) -> Option<(i16, i32)> {
    let pts = &w.course.course_points;
    let n = pts.len() as i32;
    if n == 0 {
        return None;
    }
    let (x, y, z) = ((k.x >> 5) >> 5, (k.y >> 5) >> 5, (k.z >> 5) >> 5);
    let d = |i: i32| -> u32 {
        let p = pts[i as i16 as usize];
        let dx = (p[0] >> 5) - x;
        let dy = (p[1] >> 5) - y;
        let dz = (p[2] >> 5) - z;
        dx.wrapping_mul(dx).wrapping_add(dy.wrapping_mul(dy)).wrapping_add(dz.wrapping_mul(dz)) as u32
    };
    let from = from as i32;
    let half = (n - (n >> 31)) >> 1;
    let mut best = d(from);
    let mut idx = from;
    let mut opp = from + half;
    if n <= s16(opp) {
        opp -= n;
    }
    let dd = d(s16(opp));
    if dd < best {
        best = dd;
        idx = opp;
    }
    let mut step = (if n < 0 { n + 3 } else { n }) >> 2;
    while step > 0 {
        let mut a = idx - step;
        if (a << 16) < 0 {
            a += n;
        }
        let da = d(s16(a));
        if da < best {
            best = da;
            idx = a;
        }
        let mut b = idx + step;
        if n <= s16(b) {
            b -= n;
        }
        let db = d(s16(b));
        if db < best {
            best = db;
            idx = b;
        }
        step = (s16(step) - (s16(step) >> 31)) >> 1;
    }
    let _ = square_root0;
    let mut delta = s16(idx) - s16(from);
    let h = s16(half);
    if delta < 1 {
        if delta < 0 && delta < -h {
            delta += n as i16 as i32;
        }
    } else if h < delta {
        delta -= n as i16 as i32;
    }
    Some((s16(idx) as i16, delta))
}

/// FUN_800837F8 for a mode-1 race of type 0: one kart per frame (the
/// round-robin 0x800C3058[n]) advances its race progress +0x388 along the
/// course points, then FUN_800834C0 ranks every kart by progress into
/// +0x4BE (1 = leading). Returns the leader's index (0x800D8F94[0]).
pub fn race_progress(g: &RaceGlobals, w: &World, karts: &mut [&mut KartState], pick: usize) -> Option<usize> {
    if g.mode == 0xB || g.mode == 0x14 {
        return None;
    }
    if g.race_type == 2 || g.race_flags & 1 != 0 || g.race_type == 3 {
        crate::unreachable_in_scope!("race progress outside a plain race (battle / time trial / flag 1)");
    }
    let len = w.course.length? as i32;
    if g.race_clock == 0 {
        return None;
    }
    let laps = g.laps as i32;
    // Modes 10 / 0x11 (after the finish): positions only.
    if g.mode != 10 && g.mode != 0x11 {
        let k = &mut *karts[pick];
        if k.lap as i32 != laps {
            if let Some((idx, delta)) = course_point(w, k, k.course_point) {
                k.course_point = idx;
                if !(k.lap == 0 && k.line_flag == 1) {
                    k.progress = if g.race_flags & 2 == 0 { k.progress + delta } else { k.progress - delta };
                    let cap = s16(len * laps);
                    if cap < k.progress {
                        k.progress = cap;
                    }
                }
            }
        }
    }
    // FUN_800834C0(0): bubble sort by progress, highest first (equal
    // neighbours swap).
    let n = karts.len();
    let mut order: Vec<(i32, usize)> = karts.iter().enumerate().map(|(i, k)| (k.progress, i)).collect();
    for _ in 0..n.saturating_sub(1) {
        for j in 0..n - 1 {
            if order[j].0 <= order[j + 1].0 {
                order.swap(j, j + 1);
            }
        }
    }
    for (rank, &(_, i)) in order.iter().enumerate() {
        karts[i].race_pos = rank as u8 + 1;
    }
    // 0x80083A60: a position change shows for 0x1E VBlanks (+0x4FF), the
    // shown position +0x4CA follows when the flash starts.
    for k in karts.iter_mut() {
        let mut run = k.pos_flash != 0;
        if k.race_pos != k.shown_pos && k.pos_flash == 0 {
            k.shown_pos = k.race_pos;
            k.pos_flash = 0x1E;
            run = true;
        }
        if run {
            let v = k.pos_flash as i32 - g.dt;
            k.pos_flash = if (v as i8) < 0 { 0 } else { v as u8 };
        }
    }
    Some(order[0].1)
}

/// Where the race is, for the HUD.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RacePhase {
    /// Course fly-by before the start game. `countdown`: VBlanks left to GO
    /// (>= 300).
    Intro { countdown: i32 },
    /// The "Copy the buttons" start game: `countdown` VBlanks to GO (< 300;
    /// the game beeps at 90, 60 and 30 and says GO at 0); `button` is the
    /// face button to copy (PS1 pad bit: 0x8000 SQUARE, 0x1000 TRIANGLE,
    /// 0x2000 CIRCLE, 0x4000 CROSS); `boost` the start boost earned so far
    /// (+0x446; it becomes boost ticks x 8 at GO).
    StartGame { countdown: i32, button: u16, boost: i16 },
    /// Racing: `lap` is 1-based (the lap being driven), `laps` the race's.
    Racing { lap: u8, laps: u8 },
    /// The player crossed the line after the last lap (or the race was
    /// ended): `place` 1-based.
    Finished { place: u8 },
}

/// One kart's line in the results.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KartResult {
    /// Kart slot (0 = the player).
    pub slot: usize,
    /// Race position (+0x4BE): final place once finished, else the current.
    pub place: u8,
    pub finished: bool,
    /// Race time (+0x358), clock units (300 = 1 s): the running sum of the
    /// lap times; it stops at the finish, before the last lap time is
    /// rounded, so it can differ by 1 from the sum of `lap_times`.
    pub race_time: i32,
    /// Completed laps' times (+0x35C + 4 * lap), rounded to multiples of 3
    /// at each lap end, as the game shows them.
    pub lap_times: Vec<i32>,
    /// Best lap (+0x384; 0 before the first lap end).
    pub best_lap: i32,
    pub character: u8,
    pub vehicle: u8,
}

pub fn phase(g: &RaceGlobals, k: &KartState, w: &World) -> RacePhase {
    if k.finished != 0 {
        return RacePhase::Finished { place: k.race_pos };
    }
    if g.countdown > 0 {
        if g.countdown >= g.countdown_len as i32 {
            return RacePhase::Intro { countdown: g.countdown };
        }
        let button = w.exe.i16(crate::update::COPY_BUTTONS + 2 * (k.copy_button as u8 & 3) as u32) as u16;
        return RacePhase::StartGame { countdown: g.countdown, button, boost: k.boost_count };
    }
    RacePhase::Racing { lap: (k.lap + 1).min(g.laps as u8), laps: g.laps as u8 }
}

pub fn result(slot: usize, k: &KartState) -> KartResult {
    KartResult {
        slot,
        place: k.race_pos,
        finished: k.finished != 0,
        race_time: k.race_time,
        lap_times: k.lap_time[..(k.lap as usize).min(8)].to_vec(),
        best_lap: k.best_lap,
        character: k.character,
        vehicle: k.vehicle,
    }
}

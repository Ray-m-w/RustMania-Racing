//! Oracle checks for the AI (used by mr-ai and tests/ai.rs). Not part of the
//! simulation: file I/O and the oracle's memory live here.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use mr_sim::collide::{self, ContactRules};
use mr_sim::globals::RaceGlobals;
use mr_sim::kart::{KartState, KART_ARRAY, RECORD_SIZE};
use mr_sim::mem::Mem;
use mr_sim::oracle::{self, Capture};
use mr_sim::world::{TickInputs, World};

/// Inputs from the rest of the frame (water animation normals of this
/// frame, the leader's progress), as mr_sim::harness takes them.
pub fn inputs(ma: &Mem, mb: &Mem) -> TickInputs {
    let mut inp = oracle::tick_inputs(ma);
    let ib = oracle::tick_inputs(mb);
    inp.water_normal = ib.water_normal;
    inp.alt_normal = ib.alt_normal;
    inp.aim = None;
    inp
}

use crate::ai::{self, AiContext, AiState};

/// BIOS rand() seed (A0:2F) in kernel RAM; captured by --physics runs made
/// after 2026-10-12 21:30 (tools/oracle/capture.py PHYSICS_REGIONS).
pub const RAND_SEED: u32 = 0x8000_9010;
/// 0x800C3058 + n: per-frame round-robin counters (mod n); entry 6 picks the
/// kart whose path position FUN_8009004C refreshes this frame.
pub const ROUND_ROBIN: u32 = 0x800C_3058;
pub const DIFFICULTY: u32 = 0x800E_039A;
pub const CUP: u32 = 0x800B_0E38;
pub const KARTS: u32 = 0x800B_0DFE;

pub fn bios_rand(seed: &mut u32) -> i32 {
    *seed = seed.wrapping_mul(0x41C6_4E6D).wrapping_add(0x3039);
    ((*seed >> 16) & 0x7FFF) as i32
}

/// Logic ticks: the VBlanks where the race clock 0x800B0A94 changes.
pub fn ticks(cap: &Capture) -> Vec<u32> {
    let mut out = Vec::new();
    let mut prev = None;
    for &vb in &cap.vbs {
        let c = cap.i32(vb, 0x800B_0A94).unwrap();
        if prev != Some(c) {
            out.push(vb);
            prev = Some(c);
        }
    }
    out
}

fn kaddr(slot: usize) -> u32 {
    KART_ARRAY + (RECORD_SIZE * slot) as u32
}

pub fn context(m: &Mem) -> AiContext<'static> {
    AiContext {
        karts: m.i16(KARTS),
        kart0_flags: m.u16(KART_ARRAY + 0x496),
        kart0_race_pos: m.u8(KART_ARRAY + 0x4BE),
        difficulty: m.u8(DIFFICULTY),
        cup: m.i8(CUP),
        driver_pos: &[],
    }
}

/// Path-position fields FUN_8009004C writes before the kart loop.
const PATH_FIELDS: &[(usize, usize)] = &[(0x498, 2), (0x49A, 2), (0x3F4, 4), (0x3F8, 4), (0x3FC, 4), (0x500, 1), (0x48C, 2)];

#[derive(Default, Debug, Clone)]
pub struct Count {
    pub n: usize,
    pub bad: usize,
    pub first: Option<(usize, usize)>,
}

impl Count {
    fn add(&mut self, ok: bool, tick: usize, kart: usize) {
        self.n += 1;
        if !ok {
            self.bad += 1;
            self.first.get_or_insert((tick, kart));
        }
    }
}

#[derive(Default)]
pub struct TfReport {
    pub calls: usize,
    pub skipped_special: usize,
    pub fields: BTreeMap<&'static str, Count>,
    pub rand_seeded: bool,
    pub lines: Vec<String>,
}

impl TfReport {
    pub fn text(&self) -> String {
        let mut s = String::new();
        let _ = writeln!(s, "AI decisions checked: {} (kart-ticks), {} in a special move (buttons 0, skipped)", self.calls, self.skipped_special);
        let _ = writeln!(s, "rand() seed from the capture: {}", self.rand_seeded);
        for (k, c) in &self.fields {
            let _ = writeln!(s, "  {k:<28} {:>6} / {:<6} mismatching  first {:?}", c.bad, c.n, c.first);
        }
        for l in &self.lines {
            let _ = writeln!(s, "{l}");
        }
        s
    }
}

/// Teacher forcing of FUN_800912F8 on every AI kart and tick.
pub fn teacher_force(cap: &Capture, base: &[u8], w: &World, ticks: &[u32], verbose: bool) -> TfReport {
    let mut rep = TfReport::default();
    let has_seed = cap.read(ticks[0], RAND_SEED, 4).is_some();
    rep.rand_seeded = has_seed;
    for i in 0..ticks.len() - 1 {
        let ma = cap.mem_at(ticks[i], Some(base));
        let mb = cap.mem_at(ticks[i + 1], Some(base));
        let mut g = RaceGlobals::read(&ma);
        g.dt = mb.i32(0x800B_0AEC);
        let ctx = context(&ma);
        let mut seed = ma.u32(RAND_SEED);
        let n = ma.i16(KARTS) as usize;
        for slot in 1..n {
            let ra = ma.slice(kaddr(slot), RECORD_SIZE);
            let rb = mb.slice(kaddr(slot), RECORD_SIZE);
            let mut rec = ra.to_vec();
            for &(o, sz) in PATH_FIELDS {
                rec[o..o + sz].copy_from_slice(&rb[o..o + sz]);
            }
            rec[0x4CB] = rb[0x4CB].wrapping_sub(g.dt as u8);
            let mut k = KartState::from_record(&rec);
            if k.flags & 1 == 0 {
                continue;
            }
            let mut st = AiState::from_record(&rec);
            let want = u16::from_le_bytes([rb[0x44E], rb[0x44F]]);
            if k.special != 0 || rb[0x4D6] != 0 {
                rep.skipped_special += 1;
                continue;
            }
            rep.calls += 1;
            let mut gg = g.clone();
            let mut rand = || bios_rand(&mut seed);
            let got = ai::ai_buttons(&mut k, &mut st, &mut gg, &ctx, w, &mut rand);
            let want_st = AiState::from_record(rb);
            let f = &mut rep.fields;
            let bits = |b: u16, m: u16| b & m;
            f.entry("steer (LEFT/RIGHT)").or_default().add(bits(got, 0xA0) == bits(want, 0xA0), i, slot);
            f.entry("accelerate (CROSS)").or_default().add(bits(got, 0x4000) == bits(want, 0x4000), i, slot);
            f.entry("brake (CIRCLE)").or_default().add(bits(got, 0x2000) == bits(want, 0x2000), i, slot);
            f.entry("turbo (R1)").or_default().add(bits(got, 0x800) == bits(want, 0x800), i, slot);
            f.entry("special (R2)").or_default().add(bits(got, 0x200) == bits(want, 0x200), i, slot);
            f.entry("jump (SQUARE) [rand]").or_default().add(bits(got, 0x8000) == bits(want, 0x8000), i, slot);
            f.entry("weapons (L1/L2) [rand]").or_default().add(bits(got, 0x500) == bits(want, 0x500), i, slot);
            f.entry("all buttons").or_default().add(got == want, i, slot);
            f.entry("target +0x3E8").or_default().add((k.ai_tx, k.ai_ty, k.ai_tz) == (i32::from_le_bytes(rb[0x3E8..0x3EC].try_into().unwrap()), i32::from_le_bytes(rb[0x3EC..0x3F0].try_into().unwrap()), i32::from_le_bytes(rb[0x3F0..0x3F4].try_into().unwrap())), i, slot);
            f.entry("curvature +0x4A0").or_default().add(st.turn == want_st.turn, i, slot);
            f.entry("+0x4CB staleness").or_default().add(k.path_flag_4cb == rb[0x4CB], i, slot);
            if verbose && got != want {
                rep.lines.push(format!(
                    "tick {i} vb {} kart {slot}: got {got:04X} want {want:04X} target {:?} vs {:?} turn {} vs {} seg {} pt {} spd {}",
                    ticks[i + 1], [k.ai_tx, k.ai_ty, k.ai_tz], &rb[0x3E8..0x3F4], st.turn, want_st.turn, k.path_seg, k.path_pt, k.speed_est
                ));
            }
        }
    }
    rep
}

/// Teacher forcing of the kart update on AI karts: each tick, every AI
/// kart from the oracle's record (path fields of this frame), the contact
/// pass among all karts, then `ai_kart_tick` with the oracle's buttons;
/// compare the motion fields with the oracle's next record.
pub fn teacher_force_update(cap: &Capture, base: &[u8], w: &World, ticks: &[u32], verbose: bool) -> TfReport {
    let mut rep = TfReport::default();
    let motion: &[(&'static str, usize, usize)] = &[
        ("x", 0, 4), ("y", 4, 4), ("z", 8, 4), ("pitch", 0xC, 2), ("heading", 0xE, 2), ("roll", 0x10, 2),
        ("vx", 0x2F4, 4), ("vy", 0x2F8, 4), ("vz", 0x2FC, 4), ("fwd_speed", 0x390, 4), ("side_speed", 0x394, 4),
        ("lap", 0x4BF, 1), ("lap_time[0..3]", 0x35C, 12), ("speed_est", 0x400, 4),
    ];
    for i in 0..ticks.len() - 1 {
        let ma = cap.mem_at(ticks[i], Some(base));
        let mb = cap.mem_at(ticks[i + 1], Some(base));
        let mut g = RaceGlobals::read(&ma);
        g.dt = mb.i32(0x800B_0AEC);
        let ctx = context(&ma);
        let inp = inputs(&ma, &mb);
        let n = ma.i16(KARTS) as usize;
        let mut karts: Vec<KartState> = (0..n).map(|s| oracle::load_kart(&ma, s as u32)).collect();
        let rules = ContactRules { karts: n as i32, kart0_finished: ma.u8(0x800C_356F) };
        contacts(&mut karts, &mut g, w, &rules);
        for slot in 1..n {
            let rb = mb.slice(kaddr(slot), RECORD_SIZE);
            let mut k = karts[slot].clone();
            if k.flags & 1 == 0 {
                continue;
            }
            let mut rec = vec![0u8; RECORD_SIZE];
            rec.copy_from_slice(ma.slice(kaddr(slot), RECORD_SIZE));
            k.to_record(&mut rec);
            for &(o, sz) in PATH_FIELDS {
                rec[o..o + sz].copy_from_slice(&rb[o..o + sz]);
            }
            let trail = k.trail;
            k = KartState::from_record(&rec);
            k.trail = trail;
            if k.special != 0 || rb[0x4D6] != 0 {
                rep.skipped_special += 1;
                continue;
            }
            rep.calls += 1;
            let buttons = u16::from_le_bytes([rb[0x44E], rb[0x44F]]);
            let mut gg = g.clone();
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                ai_kart_tick(&mut k, &mut gg, w, &inp, &ctx, buttons);
                k
            }));
            let k = match r {
                Ok(k) => k,
                Err(e) => {
                    let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
                    rep.fields.entry("PANIC").or_default().add(false, i, slot);
                    if verbose {
                        rep.lines.push(format!("tick {i} kart {slot}: panic {msg}"));
                    }
                    continue;
                }
            };
            let mut ours = rb.to_vec();
            k.to_record(&mut ours);
            let mut all = true;
            for &(name, o, sz) in motion {
                let ok = ours[o..o + sz] == rb[o..o + sz];
                all &= ok;
                rep.fields.entry(name).or_default().add(ok, i, slot);
            }
            rep.fields.entry("all motion fields").or_default().add(all, i, slot);
            if verbose && !all {
                rep.lines.push(format!("tick {i} vb {} kart {slot}: {:?}", ticks[i + 1], KartState::from_record(&ours).diff(&KartState::from_record(rb))));
            }
        }
    }
    rep
}

/// FUN_800418D8's pair loop over every kart (the game's order, i outer):
/// mr-sim's `collide::frame_contacts_all`.
pub fn contacts(karts: &mut [KartState], g: &mut RaceGlobals, w: &World, rules: &ContactRules) {
    collide::frame_contacts_all(karts, g, w, rules);
}

// ---------------------------------------------------------------------
// Free run
// ---------------------------------------------------------------------

/// The interim AI kart tick until update.rs calls the AI hooks itself
/// (docs/spec/ai.md "Integration"): mr-sim's FUN_80064B5C port with the
/// AI's buttons and player -1, with the two AI-only paths of the update done
/// here: the catch-up scaling (0x80065850, `ai::catchup_scale`) and the
/// re-roll timer +0x47A expiring (0x80068048, `ai::reroll`). update.rs
/// stubs both (`unreachable_in_scope!`), so the call runs with flag bit 0
/// cleared and the re-roll timer pre-empted. Exact for the kart's motion at
/// difficulty 8: the unscaled catch-up the update uses in-tick is at most 3,
/// and (0x50 - 0x40) * 3 / 0xB4 = 0 (no change to the speed cap).
pub fn ai_kart_tick(k: &mut KartState, g: &mut RaceGlobals, w: &World, inp: &TickInputs, ctx: &AiContext, buttons: u16) {
    let expire = k.respawn_timer != 0 && (k.respawn_timer as i32 - g.dt).wrapping_mul(0x10000) < 1;
    if expire {
        k.respawn_timer = 0;
    }
    let flags = k.flags;
    k.flags &= !1;
    mr_sim::update::kart_update(k, g, w, inp, buttons, -1);
    k.flags = (k.flags & !1) | (flags & 1);
    ai::catchup_scale(k, g, ctx.difficulty, ctx.cup);
    if expire {
        ai::reroll(k, g, w, ctx.karts);
    }
}

pub struct FreeRun {
    /// Per tick: (vblank, per kart: (x, z, lap, lateral, seg, pt, buttons, fwd, width)).
    pub rows: Vec<(u32, Vec<[i32; 9]>)>,
    pub stopped: Option<(usize, usize, String)>,
    /// Per kart: lap end ticks (tick index where +0x4BF increments).
    pub laps: BTreeMap<usize, Vec<usize>>,
    pub lap_times: BTreeMap<usize, Vec<i32>>,
}

impl FreeRun {
    pub fn csv(&self) -> String {
        let mut s = String::from("vb,kart,x,z,lap,lateral,seg,pt,buttons,fwd,width\n");
        for (vb, ks) in &self.rows {
            for (i, r) in ks.iter().enumerate() {
                let _ = writeln!(s, "{vb},{},{},{},{},{},{},{},{},{},{}", i + 1, r[0], r[1], r[2], r[3], r[4], r[5], r[6], r[7], r[8]);
            }
        }
        s
    }

    pub fn summary(&self) -> String {
        let mut s = String::new();
        let _ = writeln!(s, "free run: {} ticks, stopped: {:?}", self.rows.len(), self.stopped);
        for (k, t) in &self.lap_times {
            let _ = writeln!(s, "  kart {k}: lap times {:?}", t);
        }
        s
    }
}

/// Free run of every AI kart from the oracle's state at `ticks[0]`, items
/// off. The player kart (slot 0) is replayed from the oracle each tick (it
/// takes part in contacts). Laps are counted without a race end (laps = 99).
pub fn free_run(cap: &Capture, base: &[u8], w: &World, ticks: &[u32], seed: Option<u32>) -> FreeRun {
    let m0 = cap.mem_at(ticks[0], Some(base));
    let n = m0.i16(KARTS) as usize;
    let mut karts: Vec<KartState> = (0..n).map(|s| oracle::load_kart(&m0, s as u32)).collect();
    let mut st: Vec<AiState> = (0..n).map(|s| AiState::from_record(m0.slice(kaddr(s), RECORD_SIZE))).collect();
    let mut g = RaceGlobals::read(&m0);
    let mut rseed = seed.unwrap_or_else(|| m0.u32(RAND_SEED));
    // Items off: empty weapon slots, no turbo meter.
    for k in karts.iter_mut().skip(1) {
        k.weapons = [0xFF; 6];
        k.start_timer = 0;
    }
    let mut rr = m0.u8(ROUND_ROBIN + 6) as usize;
    let mut fr = FreeRun { rows: Vec::new(), stopped: None, laps: BTreeMap::new(), lap_times: BTreeMap::new() };
    let rules = ContactRules { karts: n as i32, kart0_finished: m0.u8(0x800C_356F) };
    for i in 0..ticks.len() - 1 {
        let mb = cap.mem_at(ticks[i + 1], Some(base));
        let ma = cap.mem_at(ticks[i], Some(base));
        let og = RaceGlobals::read(&ma);
        let rng = g.rng;
        g = og;
        g.rng = rng;
        g.dt = mb.i32(0x800B_0AEC);
        g.laps = 99;
        // The player as the oracle has it at the start of the frame.
        karts[0] = oracle::load_kart(&ma, 0);
        // FUN_800418D8: contacts, every pair (j < i) in order.
        contacts(&mut karts, &mut g, w, &rules);
        // FUN_8009004C for one kart (round robin over 0..n).
        let pk = rr % n;
        if pk != 0 {
            mr_sim::progress::path_position(&mut karts[pk], w);
        }
        rr = (rr + 1) % n;
        let inp = inputs(&ma, &mb);
        let ctx = AiContext { kart0_flags: karts[0].flags, kart0_race_pos: karts[0].race_pos, ..context(&ma) };
        let mut row = Vec::new();
        for s in 1..n {
            let k = &mut karts[s];
            let lap0 = k.lap;
            let mut rand = || bios_rand(&mut rseed);
            let b = ai::items_off(ai::ai_buttons(k, &mut st[s], &mut g, &ctx, w, &mut rand));
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut kk = k.clone();
                let mut gg = g.clone();
                ai_kart_tick(&mut kk, &mut gg, w, &inp, &ctx, b);
                (kk, gg)
            }));
            match r {
                Ok((kk, gg)) => {
                    *k = kk;
                    g = gg;
                }
                Err(e) => {
                    let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
                    fr.stopped = Some((i, s, msg));
                    return fr;
                }
            }
            if k.lap != lap0 {
                fr.laps.entry(s).or_default().push(i);
                fr.lap_times.entry(s).or_default().push(k.lap_time[lap0 as usize]);
            }
            row.push([k.x, k.z, k.lap as i32, k.path_lateral, k.path_seg as i32, k.path_pt as i32, b as i32, k.fwd_speed, k.path_width_r - k.path_width_l]);
        }
        fr.rows.push((ticks[i + 1], row));
    }
    fr
}

/// Vehicle templates against the oracle's table (RAM 0x800D8FD0). Wheel
/// positions only exist for the vehicles the track loader loaded (RAM
/// 0x800D1560 != -1, flag 2 of 0x800AC5E7 set).
pub fn check_vehicles(base: &[u8], w: &World) -> String {
    let m = Mem::new(base[..0x20_0000].to_vec(), base[0x20_0000..0x20_0400].to_vec());
    let mut s = String::new();
    let tpl = ai::vehicle_templates(&w.exe, &w.tables).expect("decode FUN_80087EB4");
    let mut disc = mr_disc::Disc::open_default().expect("disc");
    let mut exact = 0;
    for v in 0..ai::N_VEHICLES {
        let mut b = tpl[v];
        let loaded = m.i8(0x800D_1560 + v as u32) != -1;
        let path = ai::model_path(&w.exe, ai::addr::VEHICLE_PATHS, v as u8, "MDL");
        if loaded {
            let p = path.clone().expect("loaded vehicle has a model path");
            let mdl = mr_assets::mdl::Mdl::parse(&disc.read(&p).expect("read model")).expect("parse model");
            ai::place_wheels(&mut b, &mdl);
        }
        let ram = m.slice(ai::TEMPLATES + (v * ai::TEMPLATE_SIZE) as u32, ai::TEMPLATE_SIZE);
        let diffs: Vec<usize> = (0..ai::TEMPLATE_SIZE).filter(|&i| b[i] != ram[i]).collect();
        if diffs.is_empty() {
            exact += 1;
        }
        let _ = writeln!(s, "vehicle {v:>2} {:<22} loaded {loaded:<5} wheels {} bytes differing {}{}", path.unwrap_or_default(), b[0x15C], diffs.len(),
            if diffs.is_empty() { String::new() } else { format!(" at {:X?}", &diffs[..diffs.len().min(12)]) });
    }
    let _ = writeln!(s, "{exact} of {} templates byte-identical to RAM", ai::N_VEHICLES);
    s
}

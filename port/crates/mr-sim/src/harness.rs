//! Oracle verification harness (used by `mr-verify` and tests/oracle_lap.rs):
//! teacher forcing and free running of the Rust kart against a capture.
//! File I/O happens in `oracle`; nothing here is used by the simulation.
//!
//! Ticks are the VBlanks where the player's race clock (kart +0x358)
//! changes; tick k runs from VBlank V(k) to V(k+1) with the dt and buttons
//! the game used, both read at V(k+1).
//!
//! What the Rust kart takes from the oracle each tick (inputs produced by
//! game code outside the port):
//! - race globals (dt, mode, clock, ...), the buttons, the leader's progress;
//! - the water animation's normals (FUN_8003C5FC, start of the frame: V(k+1));
//! - the look-at target +0x2C0..+0x2CC (FUN_80041A20/80041CC4, start of the
//!   frame: V(k+1)) and the path position +0x498/+0x49A (FUN_8009004C, start
//!   of the frame: V(k+1)) — `PRE_EXTERNAL`;
//! - fields other code writes after the kart update (`EXTERNAL`, V(k));
//! - the other karts' records at V(k), for the kart-kart contact pass.

use std::collections::BTreeMap;

use crate::collide::ContactRules;
use crate::globals::RaceGlobals;
use crate::kart::{KartState, FIELD_OFFSETS, KART_ARRAY, RECORD_SIZE};
use crate::mem::Mem;
use crate::oracle::{self, Capture};
use crate::world::{TickInputs, World};

/// Kart fields written by game code outside the port after the kart
/// update (render side, race position, turbo meter, wrong-way monitor).
pub const EXTERNAL: &[(&str, usize, usize)] = &[
    ("progress", 0x388, 4),
    ("race_pos", 0x4BE, 1),
    ("start_pos", 0x4CA, 1),
    ("start_timer", 0x48A, 2),
    ("wobble", 0x48C, 2),
];

/// Kart fields written before the kart loop of the same frame.
pub const PRE_EXTERNAL: &[(&str, usize, usize)] = &[
    ("path_seg", 0x498, 2),
    ("path_pt", 0x49A, 2),
    ("path_width_l", 0x3F4, 4),
    ("path_width_r", 0x3F8, 4),
    ("path_lateral", 0x3FC, 4),
    ("path_side", 0x500, 1),
    ("path_flag_4cb", 0x4CB, 1),
];

/// Owned fields whose oracle value the harness cannot reproduce from the
/// inputs it has; none of them feeds the kart's motion (checked: the free
/// run is exact on every motion field). See docs/spec/kart-physics-ledger.md.
pub const NOT_COMPARABLE: &[&str] = &[
    // Body-sway outputs: random-driven (the springs take random kicks);
    // exact with `rng_search` except at the AI hit.
    "sway_c", "sway_e",
    // Pad rumble: the pad code (outside the port) decays these between
    // frames.
    "rumble", "rumble_a", "rumble_b",
    // Driven by random bytes; the random pointer at the moment the kart
    // update runs depends on code earlier in the frame (water animation,
    // other karts) that the capture does not reproduce.
    "event_bits", "blink_timer", "skid",
    // Set by later pairs of the contact pass (karts 1..5 among themselves).
    "nearest_kart", "close_kart",
    // One tick: the wrong-way timer read by FUN_80086B7C changes with
    // +0x48C, which the race-position code writes.
    "warn_timer", "track_dir",
];

/// Render-side record ranges the race frame rebuilds after the kart loop
/// (render.rs), compared byte for byte.
pub const RENDER_RANGES: &[(&str, usize, usize)] = &[
    ("body matrix +0x18C", 0x18C, 18),
    ("vehicle matrix +0x1AC", 0x1AC, 18),
    ("driver matrix +0x1CC", 0x1CC, 18),
    ("wheel matrices +0x20C", 0x20C, 0x12 * 4),
    ("driver part matrices +0x29C", 0x29C, 0x12 * 2),
    ("driver position +0x330..+0x347", 0x330, 0x18),
    ("look point +0x324", 0x324, 12),
    ("bounce offset +0x46A", 0x46A, 6),
    ("wheel 0 world position", 0x14, 0x18),
    ("wheel 1 world position", 0x40, 0x18),
    ("wheel 2 world position", 0x6C, 0x18),
    ("wheel 3 world position", 0x98, 0x18),
    ("light points +0x11C", 0x11C, 0x30),
    ("aim +0x2C0..+0x2CC", 0x2C0, 16),
    ("sway springs +0x3A0..+0x3E7", 0x3A0, 72),
    ("weapon slots +0x4D0", 0x4D0, 6),
];

/// Largest random-pointer skip tried by `teacher_forcing`.
pub const RNG_SEARCH: u32 = 12;

pub struct Run<'a> {
    pub cap: &'a Capture,
    pub base: &'a [u8],
    pub world: &'a World,
    pub ticks: Vec<u32>,
}

#[derive(Default, Clone, Debug)]
pub struct Stat {
    pub bad: usize,
    pub first: Option<usize>,
    pub max: i64,
}

#[derive(Default, Debug)]
pub struct TeacherReport {
    pub ticks: usize,
    pub fields: BTreeMap<String, Stat>,
    /// Ticks whose position / heading / velocity mismatch, with the owned
    /// fields that differ there (external events the port does not model).
    pub events: BTreeMap<usize, Vec<(usize, usize)>>,
    pub panics: Vec<(usize, String)>,
    /// RENDER_RANGES: ticks mismatching / first tick.
    pub render: BTreeMap<String, Stat>,
    /// Random-pointer skip that reproduced the random-driven fields, per
    /// tick count (-1: none within the search).
    pub rng_skips: BTreeMap<i32, usize>,
    /// Per tick: the random pointer at the start of the frame that
    /// reproduced the random-driven fields (oracle VBlank value + skip).
    pub rng_at_frame: BTreeMap<usize, u32>,
    /// Ticks where RENDER_RANGES differ even with the right random pointer
    /// (state given by code outside the port: weapon pickups, the AI hit).
    pub render_events: BTreeMap<usize, Vec<(usize, usize)>>,
}

pub struct FreeRun {
    /// Rows: (tick, buttons, kart after the tick, dt, vblank).
    pub rows: Vec<(usize, u16, KartState, i32, u32)>,
    /// First tick whose position, angles or velocity differ from the oracle.
    pub first_diff: Option<usize>,
    /// First tick whose RENDER_RANGES differ, with the range's name.
    pub first_render_diff: Option<(usize, String)>,
    /// The full record after each tick's render pass (for part placement).
    pub records: Vec<Vec<u8>>,
    pub stopped: Option<(usize, String)>,
}

pub fn val(b: &[u8]) -> i64 {
    match b.len() {
        1 => b[0] as i8 as i64,
        2 => i16::from_le_bytes([b[0], b[1]]) as i64,
        _ => i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as i64,
    }
}

impl<'a> Run<'a> {
    pub fn new(cap: &'a Capture, base: &'a [u8], world: &'a World) -> Self {
        let mut ticks = Vec::new();
        let mut prev = None;
        for &vb in &cap.vbs {
            let c = cap.i32(vb, KART_ARRAY + 0x358).unwrap();
            if prev != Some(c) {
                ticks.push(vb);
                prev = Some(c);
            }
        }
        Run { cap, base, world, ticks }
    }

    pub fn n(&self) -> usize {
        self.ticks.len() - 1
    }

    fn rec(&self, vb: u32) -> &[u8] {
        self.cap.read(vb, KART_ARRAY, RECORD_SIZE).unwrap()
    }

    /// Globals, inputs, buttons and memory for tick i.
    fn inputs(&self, i: usize) -> (RaceGlobals, TickInputs, u16, Mem) {
        let (va, vb) = (self.ticks[i], self.ticks[i + 1]);
        let ma = self.cap.mem_at(va, Some(self.base));
        let mb = self.cap.mem_at(vb, Some(self.base));
        let mut g = RaceGlobals::read(&ma);
        g.dt = mb.i32(0x800B_0AEC);
        let mut inp = oracle::tick_inputs(&ma);
        let ib = oracle::tick_inputs(&mb);
        inp.water_normal = ib.water_normal;
        inp.alt_normal = ib.alt_normal;
        inp.aim = None;
        (g, inp, mb.u16(0x800B_0CE8), ma)
    }

    fn others(m: &Mem) -> (Vec<KartState>, ContactRules) {
        let n = m.i16(0x800B_0DFE) as u32 + m.i16(0x800B_0C98) as u32;
        let others = (1..n).map(|s| oracle::load_kart(m, s)).collect();
        (others, ContactRules { karts: m.i16(0x800B_0DFE) as i32, tagged: m.i8(0x800B_07FE) })
    }

    /// Overwrite `fields` of `k` with the record `src`.
    fn patch(k: &mut KartState, src: &[u8], fields: impl Iterator<Item = (usize, usize)>) {
        let mut rec = src.to_vec();
        k.to_record(&mut rec);
        for (off, sz) in fields {
            rec[off..off + sz].copy_from_slice(&src[off..off + sz]);
        }
        let (trail, wheels) = (k.trail, k.wheels);
        *k = KartState::from_record(&rec);
        k.trail = trail;
        k.wheels = wheels;
    }

    fn step(&self, k: &mut KartState, g: &mut RaceGlobals, i: usize) -> Result<u16, String> {
        let (og, inp, buttons, ma) = self.inputs(i);
        let rng = g.rng;
        *g = og;
        g.rng = rng;
        let _ = &ma;
        let (mut others, rules) = Self::others(&ma);
        let mut kk = k.clone();
        let mut gg = g.clone();
        let w = self.world;
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            crate::tick_with_karts(&mut kk, &mut others, &mut gg, w, &inp, &rules, buttons);
            (kk, gg)
        }));
        match r {
            Ok((kk, gg)) => {
                *k = kk;
                *g = gg;
                Ok(buttons)
            }
            Err(e) => Err(e
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_default()),
        }
    }

    /// Kart at the start of tick i as the oracle has it (plus PRE_EXTERNAL).
    fn oracle_kart(&self, i: usize) -> KartState {
        let ma = self.cap.mem_at(self.ticks[i], Some(self.base));
        let mut k = oracle::load_kart(&ma, 0);
        Self::patch(&mut k, self.rec(self.ticks[i + 1]), PRE_EXTERNAL.iter().map(|e| (e.1, e.2)));
        k
    }

    /// One teacher-forced tick from the oracle's state, the random pointer
    /// advanced by `rng_skip` first. Returns the kart and its full record.
    fn forced(&self, i: usize, rng_skip: u32) -> Result<(KartState, Vec<u8>), String> {
        let mut k = self.oracle_kart(i);
        let ma = self.cap.mem_at(self.ticks[i], Some(self.base));
        let mut g = RaceGlobals::read(&ma);
        g.rng = g.rng.wrapping_add(rng_skip);
        self.step(&mut k, &mut g, i)?;
        let mut ours = self.rec(self.ticks[i]).to_vec();
        k.to_record(&mut ours);
        crate::render::after_kart_loop(&mut k, &mut ours, self.world, &self.world.driver);
        k.to_record(&mut ours);
        Ok((k, ours))
    }

    /// Teacher forcing. The random pointer at the moment kart 0 updates is
    /// not in the capture (the water animation and the contact pass draw
    /// from it earlier in the frame); with `rng_search` the harness tries
    /// skips 0..=RNG_SEARCH and keeps the first that reproduces the
    /// random-driven fields (body-sway springs, event bits, blink timer),
    /// reporting the skips in `rng_skips`.
    pub fn teacher_forcing(&self, n: usize) -> TeacherReport {
        self.teacher_forcing_opt(n, true)
    }

    pub fn teacher_forcing_opt(&self, n: usize, rng_search: bool) -> TeacherReport {
        let mut rep = TeacherReport { ticks: n, ..Default::default() };
        for i in 0..n {
            let refr = self.rec(self.ticks[i + 1]);
            let random_ok = |r: &[u8]| {
                r[0x3A0..0x3E8] == refr[0x3A0..0x3E8]
                    && r[0x29C..0x2C0] == refr[0x29C..0x2C0]
                    && r[0x454..0x45E] == refr[0x454..0x45E]
                    && r[0x44C..0x44E] == refr[0x44C..0x44E]
            };
            let mut chosen = None;
            let tries = if rng_search { RNG_SEARCH + 1 } else { 1 };
            for d in 0..tries {
                match self.forced(i, d) {
                    Ok((k, ours)) => {
                        let ok = random_ok(&ours);
                        if chosen.is_none() || ok {
                            chosen = Some((d, k, ours));
                        }
                        if ok {
                            break;
                        }
                    }
                    Err(msg) => {
                        if d == 0 {
                            rep.panics.push((i, msg));
                        }
                        break;
                    }
                }
            }
            let Some((d, _k, ours)) = chosen else { continue };
            *rep.rng_skips.entry(if random_ok(&ours) { d as i32 } else { -1 }).or_default() += 1;
            if random_ok(&ours) {
                let ma = self.cap.mem_at(self.ticks[i], Some(self.base));
                rep.rng_at_frame.insert(i, RaceGlobals::read(&ma).rng.wrapping_add(d));
            }
            let mut motion = false;
            for &(name, off, n) in RENDER_RANGES {
                let st = rep.render.entry(name.to_string()).or_default();
                if ours[off..off + n] != refr[off..off + n] {
                    st.bad += 1;
                    st.first.get_or_insert(i);
                    st.max = 1;
                    rep.render_events.entry(i).or_default().push((off, n));
                }
            }
            for &(name, off, sz) in FIELD_OFFSETS {
                let d = (val(&ours[off..off + sz]) - val(&refr[off..off + sz])).abs();
                let st = rep.fields.entry(name.to_string()).or_default();
                if d != 0 {
                    st.bad += 1;
                    st.first.get_or_insert(i);
                    st.max = st.max.max(d);
                    if ["x", "y", "z", "pitch", "heading", "roll", "vx", "vy", "vz", "fwd_speed", "side_speed"].contains(&name) {
                        motion = true;
                    }
                }
            }
            if motion {
                let fields = FIELD_OFFSETS
                    .iter()
                    .filter(|&&(name, off, sz)| ours[off..off + sz] != refr[off..off + sz] && !EXTERNAL.iter().any(|e| e.0 == name))
                    .map(|&(_, off, sz)| (off, sz))
                    .collect();
                rep.events.insert(i, fields);
            }
        }
        rep
    }

    /// Free run from the oracle's state at tick 0. `events`: at these ticks,
    /// after the Rust tick, copy the listed fields from the oracle (effects
    /// of game code the port does not model, e.g. an AI weapon hit).
    pub fn free_run(&self, n: usize, events: &BTreeMap<usize, Vec<(usize, usize)>>) -> FreeRun {
        self.free_run_rng(n, events, None)
    }

    /// Free run; `rng`: per tick, the random pointer to use at the start of
    /// the frame (from `TeacherReport::rng_at_frame`): the game's random
    /// stream is shared with the AI karts, so this is an input like the
    /// buttons. Without it the run keeps its own pointer.
    pub fn free_run_rng(&self, n: usize, events: &BTreeMap<usize, Vec<(usize, usize)>>, rng: Option<&BTreeMap<usize, u32>>) -> FreeRun {
        let m0 = self.cap.mem_at(self.ticks[0], Some(self.base));
        let mut k = oracle::load_kart(&m0, 0);
        let mut g = RaceGlobals::read(&m0);
        let mut out = FreeRun { rows: Vec::new(), first_diff: None, first_render_diff: None, records: Vec::new(), stopped: None };
        let mut rrec = self.rec(self.ticks[0]).to_vec();
        out.records.push(rrec.clone());
        out.rows.push((0, m0.u16(0x800B_0CE8), k.clone(), m0.i32(0x800B_0AEC), self.ticks[0]));
        for i in 0..n {
            let (va, vb) = (self.ticks[i], self.ticks[i + 1]);
            Self::patch(&mut k, self.rec(va), EXTERNAL.iter().map(|e| (e.1, e.2)));
            Self::patch(&mut k, self.rec(vb), PRE_EXTERNAL.iter().map(|e| (e.1, e.2)));
            if let Some(r) = rng.and_then(|m| m.get(&i)) {
                g.rng = *r;
            }
            let buttons = match self.step(&mut k, &mut g, i) {
                Ok(b) => b,
                Err(msg) => {
                    out.stopped = Some((i + 1, msg));
                    break;
                }
            };
            if let Some(fields) = events.get(&i) {
                Self::patch(&mut k, self.rec(vb), fields.iter().copied());
            }
            k.to_record(&mut rrec);
            crate::render::after_kart_loop(&mut k, &mut rrec, self.world, &self.world.driver);
            if let Some(fields) = events.get(&i) {
                // The injected event's render side too (spin etc.).
                let r = self.rec(vb);
                for &(off, sz) in fields {
                    rrec[off..off + sz].copy_from_slice(&r[off..off + sz]);
                }
            }
            k.to_record(&mut rrec);
            if out.first_render_diff.is_none() {
                let r = self.rec(vb);
                for &(name, off, n) in RENDER_RANGES {
                    if rrec[off..off + n] != r[off..off + n] {
                        out.first_render_diff = Some((i + 1, name.to_string()));
                        break;
                    }
                }
            }
            out.records.push(rrec.clone());
            if out.first_diff.is_none() {
                let r = self.rec(vb);
                let mut mine = r.to_vec();
                k.to_record(&mut mine);
                if mine[..0x12] != r[..0x12] || mine[0x2F4..0x300] != r[0x2F4..0x300] {
                    out.first_diff = Some(i + 1);
                }
            }
            out.rows.push((i + 1, buttons, k.clone(), g.dt, vb));
        }
        out
    }
}

/// The free run as an mr-trace JSONL (fields named as tools/oracle/trace.py).
pub fn trace_jsonl(fr: &FreeRun, route: &str, note: &str) -> String {
    let mut s = format!(
        "{{\"mrm_trace\":1,\"source\":\"rust\",\"route\":\"{route}\",\"note\":\"{note}\",\"fields\":[\"x\",\"y\",\"z\",\"pitch\",\"heading\",\"roll\",\"vx\",\"vy\",\"vz\",\"speed\",\"yaw_rate\",\"clock\",\"dt\",\"vblank\"]}}\n"
    );
    for (t, btn, k, dt, vb) in &fr.rows {
        s += &format!(
            "[{t},{btn},{},{},{},{},{},{},{},{},{},{},{},{},{dt},{vb}]\n",
            k.x, k.y, k.z, k.pitch, k.heading, k.roll, k.vx, k.vy, k.vz, k.fwd_speed, k.side_speed, k.race_time
        );
    }
    s
}

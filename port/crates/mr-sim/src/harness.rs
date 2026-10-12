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
    ("look_x", 0x324, 4),
    ("look_y", 0x328, 4),
    ("look_z", 0x32C, 4),
    ("ref_y", 0x338, 4),
    ("progress", 0x388, 4),
    ("race_pos", 0x4BE, 1),
    ("start_timer", 0x48A, 2),
    ("wobble", 0x48C, 2),
];

/// Kart fields written before the kart loop of the same frame.
pub const PRE_EXTERNAL: &[(&str, usize, usize)] = &[("path_seg", 0x498, 2), ("path_pt", 0x49A, 2)];

/// Owned fields whose oracle value the harness cannot reproduce from the
/// inputs it has; none of them feeds the kart's motion (checked: the free
/// run is exact on every motion field). See docs/spec/kart-physics-ledger.md.
pub const NOT_COMPARABLE: &[&str] = &[
    // Render-side body tilt outputs: the game's own code run on the
    // capture (tools/oracle/refstep.py) differs from the oracle by a few
    // units too; the extra input was not identified.
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
}

pub struct FreeRun {
    /// Rows: (tick, buttons, kart after the tick, dt, vblank).
    pub rows: Vec<(usize, u16, KartState, i32, u32)>,
    /// First tick whose position, angles or velocity differ from the oracle.
    pub first_diff: Option<usize>,
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
        let a = KART_ARRAY;
        inp.aim = Some([mb.i32(a + 0x2C0), mb.i32(a + 0x2C4), mb.i32(a + 0x2C8), mb.i32(a + 0x2CC)]);
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

    pub fn teacher_forcing(&self, n: usize) -> TeacherReport {
        let mut rep = TeacherReport { ticks: n, ..Default::default() };
        for i in 0..n {
            let mut k = self.oracle_kart(i);
            let ma = self.cap.mem_at(self.ticks[i], Some(self.base));
            let mut g = RaceGlobals::read(&ma);
            if let Err(msg) = self.step(&mut k, &mut g, i) {
                rep.panics.push((i, msg));
                continue;
            }
            let mut ours = self.rec(self.ticks[i]).to_vec();
            k.to_record(&mut ours);
            let refr = self.rec(self.ticks[i + 1]);
            let mut motion = false;
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
        let m0 = self.cap.mem_at(self.ticks[0], Some(self.base));
        let mut k = oracle::load_kart(&m0, 0);
        let mut g = RaceGlobals::read(&m0);
        let mut out = FreeRun { rows: Vec::new(), first_diff: None, stopped: None };
        out.rows.push((0, m0.u16(0x800B_0CE8), k.clone(), m0.i32(0x800B_0AEC), self.ticks[0]));
        for i in 0..n {
            let (va, vb) = (self.ticks[i], self.ticks[i + 1]);
            Self::patch(&mut k, self.rec(va), EXTERNAL.iter().map(|e| (e.1, e.2)));
            Self::patch(&mut k, self.rec(vb), PRE_EXTERNAL.iter().map(|e| (e.1, e.2)));
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

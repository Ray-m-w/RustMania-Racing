//! Function-level checks against the game's own code: tools/oracle/
//! fixtures.py runs the original MIPS (tools/oracle/mipsref.py) on every
//! tick of analysis/oracle/lap1P3 and records each call's entry and exit
//! state; each test here runs the Rust port of that function from the entry
//! state and compares with the exit state. Skips when the data is absent.

use mr_sim::globals::RaceGlobals;
use mr_sim::kart::{diff_records, KartState};
use mr_sim::mem::Mem;
use mr_sim::oracle::{self, read_fixtures, Capture, Fixture};
use mr_sim::world::World;

struct Env {
    cap: Capture,
    world: World,
}

fn env() -> Option<Env> {
    let root = oracle::repo_root();
    let dir = root.join("analysis/oracle/lap1P3");
    if !dir.join("fx").exists() {
        eprintln!("skip: {} has no fx/ (run tools/oracle/fixtures.py)", dir.display());
        return None;
    }
    std::env::set_current_dir(&root).unwrap();
    let cap = Capture::open(&dir).unwrap();
    let world = oracle::dock_world(cap.ram0.as_ref().unwrap());
    Some(Env { cap, world })
}

fn fixtures(e: &Env, f: u32) -> Vec<Fixture> {
    read_fixtures(e.cap.dir.join(format!("fx/{f:08X}.bin"))).unwrap()
}

fn mem_entry(e: &Env, fx: &Fixture) -> Mem {
    let mut m = e.cap.mem_at(fx.vb, None);
    fx.entry.apply(&mut m);
    m
}

/// Run `f` on every fixture of `addr`; report per-field first failure.
fn check_kart_fn(addr: u32, f: impl Fn(&mut KartState, &mut RaceGlobals, &World, &Mem)) {
    let Some(e) = env() else { return };
    let fxs = fixtures(&e, addr);
    let mut bad = 0;
    let mut first: Vec<String> = Vec::new();
    for fx in &fxs {
        let m = mem_entry(&e, fx);
        let mut k = oracle::load_kart(&m, 0);
        let mut g = RaceGlobals::read(&m);
        f(&mut k, &mut g, &e.world, &m);
        let mut out = fx.entry.kart.clone();
        k.to_record(&mut out);
        let mut d = diff_records(&out, &fx.exit.kart);
        let rng_exit = u32::from_le_bytes(fx.exit.globals[0x4CC..0x4D0].try_into().unwrap());
        if g.rng != rng_exit {
            d.push(format!("rng {:08X} vs {:08X}", g.rng, rng_exit));
        }
        if !d.is_empty() {
            bad += 1;
            if first.len() < 12 {
                first.push(format!("vb {}: {}", fx.vb, d.join("; ")));
            }
        }
    }
    eprintln!("{addr:08X}: {} / {} calls match", fxs.len() - bad, fxs.len());
    for l in &first {
        eprintln!("  {l}");
    }
    assert_eq!(bad, 0, "{addr:08X} mismatches");
}

#[test]
fn ratan2_matches() {
    let Some(e) = env() else { return };
    let fxs = fixtures(&e, 0x8009_5010);
    for fx in &fxs {
        let r = mr_sim::tables::ratan2(&e.world.tables, fx.args[0] as i32, fx.args[1] as i32);
        assert_eq!(r as u32, fx.ret, "ratan2({}, {})", fx.args[0] as i32, fx.args[1] as i32);
    }
    eprintln!("ratan2: {} calls", fxs.len());
}

#[test]
fn sqrt_matches() {
    let Some(e) = env() else { return };
    for fx in &fixtures(&e, 0x8009_3D70) {
        let r = mr_sim::tables::square_root0(&e.world.tables, fx.args[0] as i32);
        assert_eq!(r as u32, fx.ret, "sqrt({})", fx.args[0] as i32);
    }
}

#[test]
fn ground_matches() {
    check_kart_fn(0x8003_E91C, |k, g, w, m| {
        let inp = oracle::tick_inputs(m);
        mr_sim::ground::kart_ground(k, g, w, &inp);
    });
}

#[test]
fn track_direction_matches() {
    check_kart_fn(0x8008_6B7C, |k, g, w, _m| mr_sim::progress::track_direction(k, g, w, &[]));
}

#[test]
fn lap_timing_matches() {
    check_kart_fn(0x8008_2708, |k, g, w, _m| mr_sim::progress::lap_timing(k, g, w, &mut || 0));
}

#[test]
fn kart_update_matches() {
    check_kart_fn(0x8006_4B5C, |k, g, w, m| {
        let inp = oracle::tick_inputs(m);
        let buttons = m.u16(0x800B_0CE8);
        mr_sim::update::kart_update(k, g, w, &inp, buttons, 0);
    });
}

//! mr-items: the item simulation against an items-on oracle capture
//! (tools/oracle/capture_items.py: kart records, globals, pickups
//! 0x800E1B70, projectiles 0x800DDC50, rand() seed).
//!
//!   mr-items tf CAPTURE [--ticks N] [--from T] [--verbose] [--skip-max D]
//!       Teacher-forced race frames: each tick the Session is loaded with the
//!       game's state at V(k) (`Session::load_mem`) and runs one frame with
//!       the player's pad and dt; the result is compared with V(k+1): every
//!       kart's owned fields, the pickups, the projectiles. The random bytes
//!       FUN_8007F03C draws (kart particles, not ported) are searched: the
//!       skip 0..=D before the item code that reproduces the item tables.
//!   mr-items free CAPTURE [--ticks N] [--verbose]
//!       Free run of the player and the item state: the player kart, the
//!       pickups and the projectiles carry over from the port's own previous
//!       frame; the AI karts, the globals, the random pointer and the rand()
//!       seed are loaded from the oracle each frame (inputs, as in Session
//!       3), the FUN_8007F03C skip from the teacher-forced pass. Nothing is
//!       injected: hits on the player come from the port's projectiles.

use std::collections::BTreeMap;

use mr_sim::harness::Run;
use mr_sim::items::{ItemState, Pickup, Projectile};
use mr_sim::kart::{KartState, FIELD_OFFSETS, KART_ARRAY, RECORD_SIZE};
use mr_sim::mem::Mem;
use mr_sim::oracle::{self, Capture};
use mr_sim::session::Session;

/// Owned fields that the port cannot reproduce from what it runs: random
/// draws of code it does not run between the item code and the end of the
/// frame, the pad's own decay, render-only values (see harness.rs).
const COSMETIC: &[&str] = &[
    "sway_c", "sway_e", "rumble", "rumble_a", "rumble_b", "event_bits", "blink_timer", "skid",
];

#[derive(Default, Debug, Clone)]
struct Stat {
    bad: usize,
    first: Option<usize>,
}

fn diff_pickups(a: &[Pickup], b: &[Pickup]) -> Vec<String> {
    let mut out = Vec::new();
    for (i, (x, y)) in a.iter().zip(b).enumerate() {
        if x != y {
            out.push(format!("pickup {i}: {x:?} vs {y:?}"));
        }
    }
    out
}

fn diff_proj(a: &[Projectile], b: &[Projectile]) -> Vec<String> {
    let mut out = Vec::new();
    for (i, (x, y)) in a.iter().zip(b).enumerate() {
        if !x.active && !y.active {
            continue;
        }
        if x != y {
            out.push(format!("proj {i}:\n   port {x:?}\n   game {y:?}"));
        }
    }
    out
}

fn records(s: &Session) -> Vec<Vec<u8>> {
    let mut m = Mem::new(vec![0u8; 0x20_0000], vec![0u8; 0x400]);
    s.store_mem(&mut m);
    (0..1 + s.others.len()).map(|i| m.slice(KART_ARRAY + 0x504 * i as u32, RECORD_SIZE).to_vec()).collect()
}

/// Every kart's owned fields (but the cosmetic ones) equal the game's.
fn karts_match(s: &Session, mb: &Mem, nk: usize) -> bool {
    let ours = records(s);
    (0..nk).all(|k| {
        let refr = mb.slice(KART_ARRAY + 0x504 * k as u32, RECORD_SIZE);
        FIELD_OFFSETS.iter().all(|&(name, off, sz)| COSMETIC.contains(&name) || ours[k][off..off + sz] == refr[off..off + sz])
    })
}

fn val(b: &[u8]) -> i64 {
    mr_sim::harness::val(b)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    std::panic::set_hook(Box::new(|_| {}));
    let opt = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    std::env::set_current_dir(oracle::repo_root()).unwrap();
    let mode = args.get(1).cloned().unwrap_or_default();
    let dir = args.get(2).cloned().expect("capture dir");
    let verbose = args.iter().any(|a| a == "--verbose");
    let cap = Capture::open(&dir).expect("open capture");
    let base = cap.ram0.clone().expect("capture needs ram0.bin");
    let world = oracle::dock_world(&base);
    let run = Run::new(&cap, &base, &world);
    let n = opt("--ticks").map(|v| v.parse().unwrap()).unwrap_or(run.n()).min(run.n());
    let from: usize = opt("--from").map(|v| v.parse().unwrap()).unwrap_or(0);
    let skip_max: u32 = opt("--skip-max").map(|v| v.parse().unwrap()).unwrap_or(64);
    let mut disc = mr_disc::Disc::open_default().expect("disc");
    let world2 = oracle::dock_world(&base);
    let m0 = cap.mem_at(run.ticks[from], Some(&base));
    let mut s = Session::from_mem(&mut disc, world2, &m0).expect("session from the capture");
    let nk = 1 + s.others.len();
    println!("capture {dir}: ticks {from}..{n}, {nk} karts");
    match mode.as_str() {
        "tf" => teacher(&run, &cap, &base, &mut s, from, n, skip_max, verbose),
        "free" => free(&run, &cap, &base, &mut s, from, n, skip_max, verbose),
        "rows" => {
            let out = opt("--out").expect("--out CSV");
            oracle_rows(&run, &cap, &base, n, &out);
        }
        "race" => {
            let out = opt("--out").expect("--out CSV");
            let seed: u32 = opt("--seed").map(|v| v.parse().unwrap()).unwrap_or(0);
            port_race(&run, &cap, &base, &mut s, n, seed, &out);
        }
        _ => eprintln!("usage: mr-items tf|free|rows|race CAPTURE [--ticks N] [--out CSV] [--seed S]"),
    }
}

/// One teacher-forced frame from V(k). Returns the session after the tick.
fn forced(s: &mut Session, ma: &Mem, mb: &Mem, skip: u32) -> Result<(), String> {
    s.load_mem(ma);
    s.rng_skip_items = Some(skip);
    let buttons = if ma.u16(KART_ARRAY + 0x496) & 1 != 0 { mb.u16(KART_ARRAY + 0x44E) } else { mb.u16(0x800B_0CE8) };
    let dt = mb.i32(0x800B_0AEC);
    s.tick_dt(buttons, dt).map_err(|e| e.to_string())
}

#[allow(clippy::too_many_arguments)]
fn teacher(run: &Run, cap: &Capture, base: &[u8], s: &mut Session, from: usize, n: usize, skip_max: u32, verbose: bool) {
    let nk = 1 + s.others.len();
    let mut fields: BTreeMap<(usize, String), Stat> = BTreeMap::new();
    let mut item_bad = Stat::default();
    let mut skips: BTreeMap<i64, usize> = BTreeMap::new();
    let mut panics = Vec::new();
    let mut events = 0usize;
    let mut rng_end: BTreeMap<i64, usize> = BTreeMap::new();
    let mut seed_bad = 0usize;
    for i in from..n {
        let ma = cap.mem_at(run.ticks[i], Some(base));
        let mb = cap.mem_at(run.ticks[i + 1], Some(base));
        let want = ItemState::read(&mb);
        let mut chosen: Option<(u32, bool, bool)> = None;
        for d in 0..=skip_max {
            match forced(s, &ma, &mb, d) {
                Ok(()) => {
                    let ok = s.items().pickups == want.pickups && s.items().projectiles == want.projectiles && s.items().blink == want.blink;
                    let karts_ok = ok && karts_match(s, &mb, nk);
                    let better = match chosen {
                        None => true,
                        Some((_, cok, ckok)) => (ok && !cok) || (karts_ok && !ckok),
                    };
                    if better {
                        chosen = Some((d, ok, karts_ok));
                    }
                    if karts_ok {
                        break;
                    }
                }
                Err(e) => {
                    panics.push((i, e));
                    break;
                }
            }
        }
        let chosen = chosen.map(|(d, ok, _)| (d, ok));
        let Some((d, ok)) = chosen else { continue };
        if !ok {
            *skips.entry(-1).or_default() += 1;
            forced(s, &ma, &mb, 0).ok();
            item_bad.bad += 1;
            item_bad.first.get_or_insert(i);
            if verbose && item_bad.bad <= 12 {
                println!("tick {i} (vb {}): item tables differ", run.ticks[i + 1]);
                for l in diff_pickups(&s.items().pickups, &want.pickups).iter().chain(diff_proj(&s.items().projectiles, &want.projectiles).iter()).take(8) {
                    println!("   {l}");
                }
            }
        } else {
            *skips.entry(d as i64).or_default() += 1;
            if d > 0 {
                forced(s, &ma, &mb, d).ok();
            }
        }
        events += s.items().events.len();
        {
            let d = (mb.u32(0x800B_0BCC) as i64) - (s.globals.rng as i64);
            *rng_end.entry(d.clamp(-5, 40)).or_default() += 1;
            if s.rand_state != mb.u32(0x8000_9010) {
                seed_bad += 1;
            }
        }
        if verbose && !s.items().events.is_empty() {
            println!("tick {i} (vb {}): events {:?}", run.ticks[i + 1], s.items().events);
        }
        let ours = records(s);
        for k in 0..nk {
            let refr = mb.slice(KART_ARRAY + 0x504 * k as u32, RECORD_SIZE);
            for &(name, off, sz) in FIELD_OFFSETS {
                if COSMETIC.contains(&name) {
                    continue;
                }
                if ours[k][off..off + sz] != refr[off..off + sz] {
                    let st = fields.entry((k, name.to_string())).or_default();
                    st.bad += 1;
                    st.first.get_or_insert(i);
                    if verbose && st.bad <= 3 {
                        println!("tick {i} kart {k} {name}: port {} game {}", val(&ours[k][off..off + sz]), val(&refr[off..off + sz]));
                    }
                }
            }
            if ours[k][0x4D0..0x4D6] != refr[0x4D0..0x4D6] {
                let st = fields.entry((k, "weapons".to_string())).or_default();
                st.bad += 1;
                st.first.get_or_insert(i);
            }
        }
    }
    println!("\n== teacher-forced frames {from}..{n} ==");
    println!("item tables (pickups + projectiles) differing: {} ticks, first {:?}", item_bad.bad, item_bad.first);
    println!("FUN_8007F03C random-byte skip found (skip: ticks; -1 none): {skips:?}");
    println!("item events produced: {events}");
    println!("random pointer at the frame end, game - port (bytes: ticks): {rng_end:?}");
    println!("BIOS rand() seed at the frame end differing: {seed_bad} ticks");
    let mut kinds: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for (t, e) in &panics {
        let en = kinds.entry(e.clone()).or_insert((0, *t));
        en.0 += 1;
    }
    for (e, (c, t)) in &kinds {
        println!("  panic x{c} (first tick {t}): {e}");
    }
    println!("panics: {}", panics.len());
    println!("kart fields differing (kart, field: ticks, first):");
    for ((k, name), st) in &fields {
        println!("  {k} {name:<20} {:>5}  first {}", st.bad, st.first.unwrap());
    }
}

#[allow(clippy::too_many_arguments)]
fn free(run: &Run, cap: &Capture, base: &[u8], s: &mut Session, from: usize, n: usize, skip_max: u32, verbose: bool) {
    // Pass 1: the FUN_8007F03C skips (teacher forced, as in `tf`).
    let nk = 1 + s.others.len();
    let mut skip_at: BTreeMap<usize, u32> = BTreeMap::new();
    for i in from..n {
        let ma = cap.mem_at(run.ticks[i], Some(base));
        let mb = cap.mem_at(run.ticks[i + 1], Some(base));
        let want = ItemState::read(&mb);
        let mut best: Option<(u32, bool)> = None;
        for d in 0..=skip_max {
            if forced(s, &ma, &mb, d).is_err() {
                break;
            }
            let ok = s.items().pickups == want.pickups && s.items().projectiles == want.projectiles && s.items().blink == want.blink;
            if ok && karts_match(s, &mb, nk) {
                best = Some((d, true));
                break;
            }
            if ok && best.is_none() {
                best = Some((d, false));
            }
        }
        if let Some((d, _)) = best {
            skip_at.insert(i, d);
        }
    }
    // Pass 2: free run of the player kart and the item state.
    let m0 = cap.mem_at(run.ticks[from], Some(base));
    s.load_mem(&m0);
    let mut player: KartState = s.kart.clone();
    let mut prec = s.record.clone();
    let mut items = s.globals.items.clone();
    let mut first_diff = None;
    let mut first_item_diff = None;
    let mut first_any: Option<usize> = None;
    let mut hits = 0;
    for i in from..n {
        let ma = cap.mem_at(run.ticks[i], Some(base));
        let mb = cap.mem_at(run.ticks[i + 1], Some(base));
        s.load_mem(&ma);
        s.kart = player.clone();
        s.record = prec.clone();
        let ev = std::mem::take(&mut s.globals.items.events);
        s.globals.items = items.clone();
        drop(ev);
        s.rng_skip_items = Some(*skip_at.get(&i).unwrap_or(&0));
        let buttons = if ma.u16(KART_ARRAY + 0x496) & 1 != 0 { mb.u16(KART_ARRAY + 0x44E) } else { mb.u16(0x800B_0CE8) };
        let dt = mb.i32(0x800B_0AEC);
        if let Err(e) = s.tick_dt(buttons, dt) {
            println!("STOPPED at tick {i}: {e}");
            break;
        }
        for e in &s.items().events {
            if let mr_sim::items::ItemEvent::Hit { victim: 0, .. } = e {
                hits += 1;
                if verbose {
                    println!("tick {i}: player hit {e:?}");
                }
            }
        }
        player = s.kart.clone();
        prec = s.record.clone();
        items = s.globals.items.clone();
        let refr = mb.slice(KART_ARRAY, RECORD_SIZE);
        let mut mine = refr.to_vec();
        player.to_record(&mut mine);
        if first_any.is_none() {
            let d = mr_sim::kart::diff_records(&mine, refr);
            if !d.is_empty() {
                first_any = Some(i + 1);
                println!("first player field difference at tick {}: {:?}", i + 1, d.iter().take(10).collect::<Vec<_>>());
            }
        }
        if first_diff.is_none() && (mine[..0x12] != refr[..0x12] || mine[0x2F4..0x300] != refr[0x2F4..0x300]) {
            first_diff = Some(i + 1);
            println!("first motion difference at tick {}: {:?}", i + 1, mr_sim::kart::diff_records(&mine, refr).iter().take(12).collect::<Vec<_>>());
        }
        let want = ItemState::read(&mb);
        if first_item_diff.is_none() && (items.next_projectile, items.special, items.special_clock, items.blink, &items.hit_bits) != (want.next_projectile, want.special, want.special_clock, want.blink, &want.hit_bits) {
            println!("tick {}: item globals differ: port {:?} game {:?}", i + 1, (items.next_projectile, items.special, items.special_clock, items.blink), (want.next_projectile, want.special, want.special_clock, want.blink));
        }
        if first_item_diff.is_none() && (items.pickups != want.pickups || items.projectiles != want.projectiles) {
            first_item_diff = Some(i + 1);
            println!("first item-state difference at tick {}:", i + 1);
            for l in diff_pickups(&items.pickups, &want.pickups).iter().chain(diff_proj(&items.projectiles, &want.projectiles).iter()).take(6) {
                println!("   {l}");
            }
        }
    }
    println!("\n== free run {from}..{n} (player + items free, AI karts / globals / random pointer from the oracle) ==");
    println!("first player motion difference: {first_diff:?}");
    println!("first item-state difference: {first_item_diff:?}");
    println!("projectile hits on the player produced by the port: {hits}");
}

/// Item-use statistics rows (tools/oracle/item_stats.py): one per tick and
/// kart.
const ROW_HEADER: &str = "tick,slot,charge0,charge1,meter,special,boosting,boost_count,lap,dealt,taken,race_time,finished,gspecial,gowner,clock,character,fires,pickups_box,pickups_token,hits_dealt,hits_taken";

#[allow(clippy::too_many_arguments)]
fn row(out: &mut String, tick: usize, slot: usize, k: &KartState, g: (i8, u8, i32), ev: [usize; 5]) {
    out.push_str(&format!(
        "{tick},{slot},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
        k.weapons[4], k.weapons[5], k.start_timer, k.special, k.boosting, k.boost_count, k.lap, k.contacts_from_above, k.contacts_below,
        k.race_time, k.finished, g.0, g.1, g.2, k.character, ev[0], ev[1], ev[2], ev[3], ev[4]
    ));
}

/// The oracle's rows from the capture (events left at -1: the stats
/// script derives them from the record fields, as for the port).
fn oracle_rows(run: &Run, cap: &Capture, base: &[u8], n: usize, path: &str) {
    let mut out = String::from(ROW_HEADER);
    out.push_str("\n");
    for i in 0..=n {
        let m = cap.mem_at(run.ticks[i], Some(base));
        let nk = (m.i16(0x800B_0DFE) + m.i16(0x800B_0C98)) as u32;
        let it = ItemState::read(&m);
        for slot in 0..nk {
            let k = oracle::load_kart(&m, slot);
            row(&mut out, i, slot as usize, &k, (it.special, it.special_owner, m.i32(0x800B_0A94)), [0; 5]);
        }
    }
    std::fs::write(path, out).unwrap();
    println!("{} ticks -> {path}", n + 1);
}

/// A port race from the capture's first tick: every kart in the port (the
/// AI with items), the player on the capture's pad; `seed` != 0 replaces
/// the BIOS rand() seed (another race from the same start).
fn port_race(run: &Run, cap: &Capture, base: &[u8], s: &mut Session, n: usize, seed: u32, path: &str) {
    let m0 = cap.mem_at(run.ticks[0], Some(base));
    s.load_mem(&m0);
    if seed != 0 {
        let mut x = seed.wrapping_mul(0x9E37_79B9) ^ 0x5bd1_e995;
        for _ in 0..3 {
            x ^= x >> 15;
            x = x.wrapping_mul(0x2C1B_3C6D);
        }
        s.rand_state = x;
        s.globals.rng = 0x8001_F94C + (x >> 8) % 0x5DB;
    }
    let nk = 1 + s.others.len();
    let mut out = String::from(ROW_HEADER);
    out.push_str("\n");
    let mut ev = vec![[0usize; 5]; nk];
    let it = s.items();
    for (slot, k) in std::iter::once(&s.kart).chain(s.others.iter().map(|o| &o.kart)).enumerate() {
        row(&mut out, 0, slot, k, (it.special, it.special_owner, s.globals.race_clock), ev[slot]);
    }
    for i in 0..n {
        let mb = cap.mem_at(run.ticks[i + 1], Some(base));
        let buttons = mb.u16(0x800B_0CE8);
        let dt = mb.i32(0x800B_0AEC);
        if let Err(e) = s.tick_dt(buttons, dt) {
            println!("STOPPED at tick {i}: {e}");
            break;
        }
        for e in &s.items().events {
            use mr_sim::items::{ItemEvent, PickupKind};
            match *e {
                ItemEvent::Fired { kart, .. } => ev[kart as usize][0] += 1,
                ItemEvent::Pickup { kart, kind: PickupKind::Box, .. } => ev[kart as usize][1] += 1,
                ItemEvent::Pickup { kart, kind: PickupKind::Token, .. } => ev[kart as usize][2] += 1,
                ItemEvent::Hit { victim, shooter, .. } => {
                    ev[shooter as usize][3] += 1;
                    ev[victim as usize][4] += 1;
                }
                _ => {}
            }
        }
        let it = s.items();
        let g = (it.special, it.special_owner, s.globals.race_clock);
        for (slot, k) in std::iter::once(&s.kart).chain(s.others.iter().map(|o| &o.kart)).enumerate() {
            row(&mut out, i + 1, slot, k, g, ev[slot]);
        }
    }
    std::fs::write(path, out).unwrap();
    println!("port race seed {seed}: {n} ticks -> {path}");
}

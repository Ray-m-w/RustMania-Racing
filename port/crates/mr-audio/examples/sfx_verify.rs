//! Verify the port's sound against an oracle capture (tools/oracle/
//! capture_sfx.py + sfx_events.py -> keyons.csv).
//!
//! `cargo run --release -p mr-audio --example sfx_verify -- CAPTURE_DIR [--wav OUT.wav]`
//!
//! 1. Teacher forcing: every tick from the oracle's state (mr_sim::harness:
//!    the random pointer that reproduces the kart's random-driven fields),
//!    the player's kart update (engine, scrape, slide, impact, jump, skid,
//!    landing, walls, splash, contacts with kart 0, lap), the countdown, and
//!    FUN_8007A7F4's character voices from the oracle's request bits ->
//!    the port's sound calls per tick.
//! 2. Loops: the port's channel refreshes against the game's channel table
//!    0x800D8AB0 (pitch, volume per owner) at the end of the tick.
//! 3. End to end: the calls through mr-audio's driver (seeded from the RAM at
//!    the first tick), key-ons per VBlank against the oracle's SPU KEYON
//!    events: sample address, VBlank (+-1), pitch, volumes, ADSR, voice.

use std::collections::BTreeMap;

use mr_audio::driver::{Consts, Driver, KeyOn};
use mr_audio::vab::Vab;
use mr_sim::globals::RaceGlobals;
use mr_sim::harness::{Run, PRE_EXTERNAL};
use mr_sim::kart::{KartState, KART_ARRAY, RECORD_SIZE};
use mr_sim::oracle::{self, Capture};
use mr_sim::sound::{SoundCall, SoundFrame};

#[derive(Clone, Copy, Debug)]
struct Ev {
    vb: i64,
    voice: u8,
    addr: u32,
    pitch: u16,
    vol_l: u16,
    vol_r: u16,
    adsr1: u16,
    adsr2: u16,
}

fn read_keyons(path: &std::path::Path) -> Vec<Ev> {
    let t = std::fs::read_to_string(path).expect("keyons.csv (run tools/oracle/sfx_events.py)");
    t.lines()
        .skip(1)
        .filter(|l| l.ends_with("KEYON"))
        .map(|l| {
            let f: Vec<i64> = l.split(',').take(8).map(|x| x.parse().unwrap()).collect();
            Ev { vb: f[0], voice: f[1] as u8, addr: f[2] as u32, pitch: f[3] as u16, vol_l: f[4] as u16, vol_r: f[5] as u16, adsr1: f[6] as u16, adsr2: f[7] as u16 }
        })
        .collect()
}

fn patch(k: &mut KartState, src: &[u8]) {
    let mut rec = src.to_vec();
    k.to_record(&mut rec);
    for &(_, off, sz) in PRE_EXTERNAL {
        rec[off..off + sz].copy_from_slice(&src[off..off + sz]);
    }
    let (trail, wheels) = (k.trail, k.wheels);
    *k = KartState::from_record(&rec);
    k.trail = trail;
    k.wheels = wheels;
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let opt = |k: &str| a.iter().position(|x| x == k).and_then(|i| a.get(i + 1)).cloned();
    std::env::set_current_dir(oracle::repo_root()).unwrap();
    let dir = a.get(1).cloned().unwrap_or_else(|| "analysis/oracle/sfx_race3".into());
    let cap = Capture::open(&dir).expect("capture");
    let base = cap.ram0.clone().expect("ram0.bin");
    let world = oracle::dock_world(&base);
    let run = Run::new(&cap, &base, &world);
    let n = run.n();
    eprintln!("{dir}: {n} ticks; teacher forcing for the random pointer ...");
    let rep = run.teacher_forcing(n);
    let rng_ok = rep.rng_at_frame.len();

    // 1. The port's calls per tick.
    let mut frames: Vec<(u32, u32, SoundFrame)> = Vec::new(); // (vb start, vb end, calls)
    let mut rng_known: Vec<bool> = Vec::new();
    let mut panics = 0;
    for i in 0..n {
        let (va, vb) = (run.ticks[i], run.ticks[i + 1]);
        let ma = cap.mem_at(va, Some(&base));
        let mb = cap.mem_at(vb, Some(&base));
        let mut g = RaceGlobals::read(&ma);
        let prev_dt = g.dt;
        g.dt = mb.i32(0x800B_0AEC);
        if let Some(&r) = rep.rng_at_frame.get(&i) {
            g.rng = r;
        }
        g.sound.frames.clear();
        g.sound.frames.push(SoundFrame { dt: g.dt, calls: Vec::new() });
        g.sound.listeners[0] = [ma.i32(0x800C_4F68), ma.i32(0x800C_4F6C), ma.i32(0x800C_4F70)];
        // FUN_8005C2EC's countdown beeps (with the previous frame's dt).
        if g.countdown != 0 {
            let mut gc = g.clone();
            gc.dt = prev_dt;
            mr_sim::sound::countdown(&mut gc);
            g.sound = gc.sound;
        }
        let mut inp = oracle::tick_inputs(&ma);
        let ib = oracle::tick_inputs(&mb);
        inp.water_normal = ib.water_normal;
        inp.alt_normal = ib.alt_normal;
        inp.aim = None;
        let buttons = if ma.u16(KART_ARRAY + 0x496) & 1 != 0 { mb.u16(KART_ARRAY + 0x44E) } else { mb.u16(0x800B_0CE8) };
        let mut k = oracle::load_kart(&ma, 0);
        patch(&mut k, cap.read(vb, KART_ARRAY, RECORD_SIZE).unwrap());
        let nk = ma.i16(0x800B_0DFE) as u32 + ma.i16(0x800B_0C98) as u32;
        let mut others: Vec<KartState> = (1..nk).map(|s| oracle::load_kart(&ma, s)).collect();
        let rules = mr_sim::collide::ContactRules { karts: ma.i16(0x800B_0DFE) as i32, kart0_finished: ma.u8(0x800C_356F) };
        let w = &world;
        if a.iter().any(|x| x == "--all-karts") {
            // Every kart: the contact pass over all pairs, kart 0's update,
            // then the AI karts' updates (player -1, their buttons +0x44E).
            let mut all = vec![k];
            all.extend(others);
            for s in 1..nk as usize {
                let src = cap.read(vb, KART_ARRAY + 0x504 * s as u32, RECORD_SIZE).unwrap().to_vec();
                patch(&mut all[s], &src);
            }
            let bs: Vec<u16> = (0..nk).map(|s| mb.u16(KART_ARRAY + 0x504 * s + 0x44E)).collect();
            let mut gg = g.clone();
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                mr_sim::collide::frame_contacts_all(&mut all, &mut gg, w, &rules);
                mr_sim::render::aim_at_course(&mut all[0], &mut gg, w);
                mr_sim::tick(&mut all[0], &mut gg, w, &inp, buttons);
            }));
            if r.is_err() {
                panics += 1;
            }
            for s in 1..nk as usize {
                let mut g2 = gg.clone();
                let mut ks = all[s].clone();
                let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    mr_sim::update::kart_update(&mut ks, &mut g2, w, &inp, bs[s], -1);
                    g2
                }));
                if let Ok(g2) = r {
                    gg = g2;
                }
            }
            g = gg;
        } else {
            let mut gg = g.clone();
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                mr_sim::tick_with_karts(&mut k, &mut others, &mut gg, w, &inp, &rules, buttons);
                gg
            }));
            match r {
                Ok(gg) => g = gg,
                Err(_) => panics += 1,
            }
        }
        // FUN_8007A7F4 with the oracle's request bits (set during this frame,
        // cleared at the next frame's start) and voice timers.
        let mut ks: Vec<KartState> = (0..nk).map(|s| oracle::load_kart(&mb, s)).collect();
        for s in 0..nk as usize {
            g.sound.voice_timer[s] = ma.u8(KART_ARRAY + 0x504 * s as u32 + 0x4F5);
        }
        mr_sim::sound::character_voices(&mut ks, &mut g, &world);
        frames.push((va, vb, g.sound.frames.pop().unwrap()));
        rng_known.push(rep.rng_at_frame.contains_key(&i));
    }
    let total: usize = frames.iter().map(|f| f.2.calls.len()).sum();
    println!("ticks {n}, random pointer found for {rng_ok}, kart-update panics {panics}, port calls {total}");

    // 2. Loops vs the channel table.
    let mut loop_stats: BTreeMap<i16, (usize, usize, usize)> = BTreeMap::new(); // owner -> (calls, pitch ok, vol ok)
    let (mut eng_known, mut eng_known_ok) = (0usize, 0usize);
    let mut first_bad: BTreeMap<i16, (u32, String)> = BTreeMap::new();
    for (fi, (_, vb, f)) in frames.iter().enumerate() {
        let mb = cap.mem_at(*vb, Some(&base));
        for c in &f.calls {
            if let SoundCall::Loop { owner, pitch, vol, .. } = *c {
                let e = loop_stats.entry(owner).or_default();
                e.0 += 1;
                let ch = (0..8).map(|c| 0x800D_8AB0 + 0x18 * c).find(|&b| mb.i32(b) == owner as i32);
                if let Some(b) = ch {
                    let (cur, vl) = (mb.i16(b + 6), mb.i16(b + 0x14));
                    let want_vol = (vol as i32).clamp(0, 150) as i16;
                    if cur == pitch {
                        e.1 += 1;
                    }
                    if owner == 0 && rng_known[fi] {
                        eng_known += 1;
                        eng_known_ok += (cur == pitch) as usize;
                    }
                    if vl == want_vol {
                        e.2 += 1;
                    }
                    if (cur != pitch || vl != want_vol) && !first_bad.contains_key(&owner) {
                        first_bad.insert(owner, (*vb, format!("port pitch {pitch} vol {want_vol}, oracle {cur} {vl}")));
                    }
                }
            }
        }
    }
    println!("\nengine pitch on ticks whose random pointer is known: {eng_known_ok} / {eng_known}");
    println!("\n== looping channels (owner: calls, pitch exact, volume exact) ==");
    for (o, (c, p, v)) in &loop_stats {
        println!("  owner {o}: {c} refreshes, pitch {p} ({:.1}%), volume {v} ({:.1}%)  {}", 100.0 * *p as f64 / *c as f64, 100.0 * *v as f64 / *c as f64, first_bad.get(o).map(|x| format!("first diff vb {}: {}", x.0, x.1)).unwrap_or_default());
    }

    // 3. End to end.
    if !std::path::Path::new(&dir).join("keyons.csv").exists() {
        println!("
(no keyons.csv: end-to-end comparison skipped)");
        return;
    }
    let mut disc = mr_disc::Disc::open_default().expect("disc");
    let vab = Vab::parse(&disc.read("SFX/SFX_6.VH").unwrap(), disc.read("SFX/SFX_6.VB").unwrap()).unwrap();
    let consts = Consts::from_exe(&disc.read("SLUS_012.37").unwrap()).unwrap();
    let mut drv = Driver::new(vab, consts);
    let m0 = cap.mem_at(run.ticks[0], Some(&base));
    drv.load_ram(&m0.ram);
    let mut ours: Vec<(i64, KeyOn)> = Vec::new();
    let mut pcm = Vec::new();
    let off: i64 = opt("--vb-offset").map(|v| v.parse().unwrap()).unwrap_or(0);
    for (va, vb, f) in &frames {
        for c in &f.calls {
            match *c {
                SoundCall::Play { id, pitch, vol_l, vol_r } => drv.play(id as i32, pitch as i32, vol_l as i32, vol_r as i32),
                SoundCall::Loop { owner, id, pitch, vol, glide } => {
                    drv.loop_sound(owner as i32, id as i32, pitch as i32, vol as i32, glide as i32, f.dt);
                }
            }
        }
        for v in *va..*vb {
            drv.vblank_render(&mut pcm);
            for k in drv.take_keyons() {
                ours.push((v as i64 + 2 + off, k));
            }
        }
    }
    let theirs: Vec<Ev> = read_keyons(&std::path::Path::new(&dir).join("keyons.csv"))
        .into_iter()
        .filter(|e| e.vb > run.ticks[0] as i64 && e.vb <= *run.ticks.last().unwrap() as i64)
        .collect();
    // Greedy match: same address, VBlank within +-1, in order.
    let mut used = vec![false; ours.len()];
    let (mut matched, mut exact_vb, mut pitch_ok, mut vol_ok, mut adsr_ok, mut voice_ok) = (0, 0, 0, 0, 0, 0);
    let mut missed: BTreeMap<u32, usize> = BTreeMap::new();
    let mut dvb: BTreeMap<i64, usize> = BTreeMap::new();
    let mut examples: Vec<String> = Vec::new();
    for e in &theirs {
        let j = ours.iter().enumerate().position(|(j, (v, k))| !used[j] && k.addr == e.addr && (v - e.vb).abs() <= 1);
        match j {
            Some(j) => {
                used[j] = true;
                matched += 1;
                let (v, k) = ours[j];
                exact_vb += (v == e.vb) as usize;
                *dvb.entry(v - e.vb).or_insert(0usize) += 1;
                pitch_ok += (k.pitch == e.pitch) as usize;
                let vo = k.vol_l == e.vol_l && k.vol_r == e.vol_r;
                vol_ok += vo as usize;
                adsr_ok += (k.adsr1 == e.adsr1 && k.adsr2 == e.adsr2) as usize;
                voice_ok += (k.voice == e.voice) as usize;
                if a.iter().any(|x| x == "--voices") && matched <= 40 {
                    println!("  vb {} addr {:#x} voice port {} oracle {}", e.vb, e.addr, k.voice, e.voice);
                }
                if (k.pitch != e.pitch || !vo) && examples.len() < 12 {
                    examples.push(format!("vb {} addr {:#x}: pitch {:#x}/{:#x} vol {:#x},{:#x}/{:#x},{:#x}", e.vb, e.addr, k.pitch, e.pitch, k.vol_l, k.vol_r, e.vol_l, e.vol_r));
                }
            }
            None => {
                *missed.entry(e.addr).or_default() += 1;
                if opt("--show-missed").map(|x| u32::from_str_radix(x.trim_start_matches("0x"), 16).unwrap()) == Some(e.addr) {
                    println!("  missed {:#x} at vb {} voice {} pitch {:#x} vol {:#x}", e.addr, e.vb, e.voice, e.pitch, e.vol_l);
                }
            }
        }
    }
    let mut extra: BTreeMap<u32, usize> = BTreeMap::new();
    for (j, (_, k)) in ours.iter().enumerate() {
        if !used[j] {
            *extra.entry(k.addr).or_default() += 1;
        }
    }
    let pct = |x: usize, of: usize| 100.0 * x as f64 / of.max(1) as f64;
    println!("\n== SPU key-ons, VBlanks {}..{} ==", run.ticks[0], run.ticks.last().unwrap());
    println!("  oracle {}, port {}", theirs.len(), ours.len());
    println!("  matched (address, VBlank +-1): {matched} = {:.1}% of the oracle's", pct(matched, theirs.len()));
    println!("  of those: same VBlank {:.1}%, pitch {:.1}%, volumes {:.1}%, ADSR {:.1}%, voice {:.1}%", pct(exact_vb, matched), pct(pitch_ok, matched), pct(vol_ok, matched), pct(adsr_ok, matched), pct(voice_ok, matched));
    let name = |addr: u32| -> String {
        let v = drv_vag(&drv, addr);
        format!("{addr:#07x} (vag {v:?}, ids {:?})", ids_for_vag(&drv, v))
    };
    println!("  VBlank (port - oracle) of the matches: {dvb:?}");
    println!("  missed (oracle only), by sample:");
    for (a, c) in &missed {
        println!("    {} x{c}", name(*a));
    }
    println!("  extra (port only), by sample:");
    for (a, c) in &extra {
        println!("    {} x{c}", name(*a));
    }
    for e in &examples {
        println!("  diff: {e}");
    }
    if let Some(p) = opt("--wav") {
        write_wav(&p, &pcm);
        println!("wrote {p} ({} frames from VBlank {})", pcm.len() / 2, run.ticks[0]);
    }
}

fn drv_vag(d: &Driver, addr: u32) -> Option<usize> {
    (1..d.vab.vag_sizes.len()).find(|&v| d.vab.vag_spu_addr(d.c.base, v).map(|a| a as u32 * 8) == Some(addr))
}

fn ids_for_vag(d: &Driver, v: Option<usize>) -> Vec<usize> {
    let Some(v) = v else { return vec![] };
    (0..d.vab.tones.len()).filter(|&t| d.vab.tones[t].vag as usize == v).map(|t| t + 1).collect()
}

fn write_wav(path: &str, pcm: &[i16]) {
    let mut b = Vec::with_capacity(44 + pcm.len() * 2);
    let data = (pcm.len() * 2) as u32;
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&44_100u32.to_le_bytes());
    b.extend_from_slice(&(44_100u32 * 4).to_le_bytes());
    b.extend_from_slice(&4u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data.to_le_bytes());
    for s in pcm {
        b.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::write(path, b).unwrap();
}

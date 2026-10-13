//! mr-hud: lay out the race HUD (mr_assets::hud) and print its primitives,
//! optionally with the state read from the running recomp (read-only
//! read_ram), and render it with the software reference renderer.
//!
//! usage: mr-hud [--oracle] [--kart 0xADDR] [--set key=value ...] [--png OUT]
//!   keys: countdown, countdown_start, mode, lap, laps, race_time, position,
//!         position_flag, finished, button, count
//! Prints one line per primitive in the game's add order:
//!   x y w h u v uw vh tpage clut raw semi abr r g b

use mr_assets::hud::{self, HudState, HudTables, Minigame};
use mr_assets::raw;
use mr_assets::vram::Vram;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;

fn read_ram(port: u16, addr: u32, n: usize) -> Vec<u8> {
    let s = TcpStream::connect(("127.0.0.1", port)).expect("oracle");
    let mut r = BufReader::new(s.try_clone().unwrap());
    let mut w = s;
    writeln!(w, "{{\"id\":1,\"cmd\":\"read_ram\",\"addr\":\"0x{addr:08X}\",\"len\":{n}}}").unwrap();
    let mut line = String::new();
    while line.trim().is_empty() {
        line.clear();
        r.read_line(&mut line).unwrap();
    }
    let key = if line.contains("\"hex\"") { "\"hex\"" } else { "\"data\"" };
    let i = line.find(key).expect("hex") + key.len();
    let rest = &line[i..];
    let q = rest.find('"').unwrap() + 1;
    let hex = &rest[q..q + n * 2];
    (0..n).map(|k| u8::from_str_radix(&hex[k * 2..k * 2 + 2], 16).unwrap()).collect()
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let opt = |k: &str| a.iter().position(|x| x == k).and_then(|i| a.get(i + 1)).cloned();
    let mut disc = mr_disc::Disc::open_default().expect("open disc");
    let exe = disc.read("SLUS_012.37").expect("exe");
    let t = HudTables::from_exe(&exe).expect("hud tables");
    let mut st = HudState::default();
    let port = 4370;
    if a.iter().any(|x| x == "--oracle") {
        let g = |addr: u32, n: usize| read_ram(port, addr, n);
        let i32at = |b: &[u8]| i32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        let kart = opt("--kart").map(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).unwrap()).unwrap_or(0x800C_30A8);
        st.countdown = i32at(&g(0x800B_0C70, 4));
        st.countdown_start = i32at(&g(0x800B_07F8, 4));
        st.mode = g(0x800B_0D46, 2)[0];
        st.laps = g(0x800B_0DA6, 2)[0];
        st.race_time = i32at(&g(0x800B_0A94, 4));
        let rec = g(kart, 0x504);
        st = st.with_kart(&rec);
        if st.countdown != 0 {
            st.minigame = Some(Minigame {
                button: rec[hud::kart::MINIGAME_BUTTON],
                count: i16::from_le_bytes([rec[hud::kart::MINIGAME_COUNT], rec[hud::kart::MINIGAME_COUNT + 1]]),
                flicker: 0,
            });
        }
        if st.finished || st.mode == 10 {
            let leader = i32at(&g(0x800D_8F94, 4)) as u32;
            let lrec = g(0x800C_30A8 + leader * 0x504, 0x504);
            let mut r = hud::Results::from_records(&rec, &lrec, st.laps);
            let e38 = i16::from_le_bytes(g(0x800B_0E38, 2).try_into().unwrap()) as i32;
            let e32 = i16::from_le_bytes(g(0x800B_0E32, 2).try_into().unwrap()) as i32;
            let record = i32at(&g((0x800E_0190i64 + 4 * (e38 * 8 + e32 * 2) as i64) as u32, 4));
            let e2 = u16::from_le_bytes(g(0x800B_07E2, 2).try_into().unwrap());
            r.best_lap_record = e2 & 2 == 0 && r.best_lap == record;
            r.press_start = g(0x800B_0F88, 1)[0] != 0 || g(0x800B_0EBE, 1)[0] != 0;
            r.frame = i32at(&g(0x800B_0D90, 4));
            st.results = Some(r);
        }
        eprintln!("oracle state: {st:?}; kart +0x358 = {}", i32at(&rec[0x358..]));
    }
    for (i, k) in a.iter().enumerate() {
        if k != "--set" {
            continue;
        }
        let (key, val) = a[i + 1].split_once('=').expect("--set key=value");
        let v: i32 = val.parse().expect("number");
        match key {
            "countdown" => st.countdown = v,
            "countdown_start" => st.countdown_start = v,
            "mode" => st.mode = v as u8,
            "lap" => st.lap = v as u8,
            "laps" => st.laps = v as u8,
            "race_time" => st.race_time = v,
            "position" => st.position = v as u8,
            "position_flag" => st.position_flag = v as u8,
            "finished" => st.finished = v != 0,
            "button" => st.minigame.get_or_insert_with(Minigame::default).button = v as u8,
            "count" => st.minigame.get_or_insert_with(Minigame::default).count = v as i16,
            "best_lap" => st.results.get_or_insert_with(hud::Results::default).best_lap = v,
            "total_time" => st.results.get_or_insert_with(hud::Results::default).total_time = v,
            "leader_total" => st.results.get_or_insert_with(hud::Results::default).leader_total = v,
            "place" => st.results.get_or_insert_with(hud::Results::default).place = v as u8,
            "hits_taken" => st.results.get_or_insert_with(hud::Results::default).hits_taken = v as i16,
            "hits_given" => st.results.get_or_insert_with(hud::Results::default).hits_given = v as i16,
            "frame" => st.results.get_or_insert_with(hud::Results::default).frame = v,
            "press_start" => st.results.get_or_insert_with(hud::Results::default).press_start = v != 0,
            _ => panic!("unknown key {key}"),
        }
    }
    let file = disc.read("COURSE_A/DOCK.RAW").expect("RAW");
    let env = raw::LoadEnv::from_exe(&exe, raw::DOCK_COURSE).unwrap();
    let dec = raw::decode(&file, &env).expect("decode");
    let slots = hud::slot_attrs(&dec.slots);
    let sprites = t.layout(&st);
    for s in &sprites {
        let (tp, cl) = s.attrs(&slots);
        println!(
            "{} {} {} {} {} {} {} {} {:#06x} {:#06x} {} {} {} {} {} {}",
            s.x, s.y, s.w, s.h, s.u, s.v, s.uw, s.vh, tp, cl, s.raw as u8, s.semi as u8, s.abr, s.rgb[0], s.rgb[1], s.rgb[2]
        );
    }
    if let Some(p) = opt("--png") {
        let mut v = Vram::default();
        dec.apply(&mut v);
        let (w, h) = (512usize, 240usize);
        let mut canvas = vec![[0u8, 0, 0, 0]; w * h];
        hud::render(&v, &slots, &sprites, &mut canvas, w, h);
        let rgba: Vec<u8> = canvas.iter().flatten().copied().collect();
        std::fs::write(&p, mr_assets::img::png_rgba(w as u32, h as u32, &rgba)).unwrap();
        eprintln!("wrote {p}");
    }
}

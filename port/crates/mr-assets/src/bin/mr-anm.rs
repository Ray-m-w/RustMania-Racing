//! mr-anm: decode `.ANM` files from the owner's disc and dump them as text;
//! optionally compare with the copies the running recomp holds in RAM.
//!
//! usage: mr-anm [DISC_PATH.ANM ...] [--all] [--oracle] [--port P]
//!   default: CHARS_A/KERMIT.ANM VEHIC_A/RACER.ANM; --all = every .ANM.
//! writes analysis/anm/<stem>.txt. --oracle (read-only read_ram) finds each
//! file in the model table DAT_800dd598 and checks it equals the file
//! except the runtime fields the loader writes (+0 = 1, +4/+6 = slot range,
//! block offsets relocated to pointers).

use mr_assets::anm::{Anm, Channel};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::PathBuf;

const ANM_TABLE_ADDR: u32 = 0x800D_D598;

fn repo_root() -> PathBuf {
    let mut d = std::env::current_dir().unwrap();
    loop {
        if d.join("game.toml").exists() {
            return d;
        }
        if !d.pop() {
            return std::env::current_dir().unwrap();
        }
    }
}

fn read_ram(port: u16, addr: u32, n: usize) -> Vec<u8> {
    let s = TcpStream::connect(("127.0.0.1", port)).expect("connect to the recomp debug server");
    s.set_read_timeout(Some(std::time::Duration::from_secs(30))).unwrap();
    let mut w = s.try_clone().unwrap();
    writeln!(w, "{{\"id\":1,\"cmd\":\"read_ram\",\"addr\":\"0x{addr:08X}\",\"len\":{n}}}").unwrap();
    let mut r = BufReader::new(s);
    let mut line = String::new();
    while line.trim().is_empty() {
        line.clear();
        r.read_line(&mut line).unwrap();
    }
    let k = line.find("\"hex\"").expect("read_ram reply");
    let rest = &line[k + 5..];
    let a = rest.find('"').unwrap() + 1;
    let hex = &rest[a..a + n * 2];
    (0..n).map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap()).collect()
}

fn describe(p: &str, a: &Anm, len: usize) -> String {
    let mut s = format!(
        "{p}: {len} bytes; runtime fields {} {} {}; block offsets {:?}\n",
        a.loaded_flag, a.first_slot, a.end_slot, a.block_offsets
    );
    for b in &a.blocks {
        s.push_str(&format!(
            "block @0x{:X}: header {:?} parts {} frame_hw {} frames {} scale_mask 0b{:b}\n",
            b.offset,
            b.header,
            b.part_count(),
            b.frame_len_hw(),
            b.frame_count(),
            b.scale_mask
        ));
        for (i, c) in b.channels.iter().enumerate() {
            match c {
                Channel::Seam => s.push_str(&format!("  part {i}: seam (neck)\n")),
                Channel::Pose(o) => {
                    for (f, fr) in b.frames.iter().enumerate() {
                        let pz = fr[i].unwrap();
                        s.push_str(&format!(
                            "  part {i} (+{o} hw) frame {f}: pos4 {:?} (pivot {:?}) rot {:?} scale {:?}\n",
                            pz.pos4,
                            pz.pivot(),
                            pz.rot,
                            pz.scale
                        ));
                    }
                }
            }
        }
    }
    s
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut paths = Vec::new();
    let (mut all, mut oracle, mut port) = (false, false, 4370u16);
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--all" => all = true,
            "--oracle" => oracle = true,
            "--port" => {
                i += 1;
                port = args[i].parse().expect("--port P");
            }
            p => paths.push(p.to_string()),
        }
        i += 1;
    }
    let mut disc = mr_disc::Disc::open_default().expect("open disc (MRM_DISC or game.toml)");
    if all {
        paths = disc.paths().filter(|p| p.ends_with(".ANM")).map(|s| s.to_string()).collect();
        paths.sort();
    } else if paths.is_empty() {
        paths = vec!["CHARS_A/KERMIT.ANM".into(), "VEHIC_A/RACER.ANM".into()];
    }
    let out = repo_root().join("analysis").join("anm");
    std::fs::create_dir_all(&out).unwrap();
    let table = if oracle { read_ram(port, ANM_TABLE_ADDR, 64 * 4) } else { vec![] };
    let mut ok_all = true;
    for p in &paths {
        let b = disc.read(p).unwrap();
        let a = Anm::parse(&b).unwrap_or_else(|e| panic!("{p}: {e}"));
        let st = p.rsplit('/').next().unwrap().split('.').next().unwrap().to_ascii_lowercase();
        let txt = describe(p, &a, b.len());
        std::fs::write(out.join(format!("{st}.txt")), &txt).unwrap();
        print!("{txt}");
        if oracle {
            let mut found = None;
            for k in 0..64 {
                let ptr = u32::from_le_bytes(table[k * 4..k * 4 + 4].try_into().unwrap());
                if ptr < 0x8000_0000 {
                    continue;
                }
                let ram = read_ram(port, ptr, b.len());
                if ram[16..] == b[16..] && ram[2..4] == b[2..4] {
                    found = Some((k, ptr, ram));
                    break;
                }
            }
            match found {
                Some((k, ptr, ram)) => {
                    let h = |o: usize| u16::from_le_bytes([ram[o], ram[o + 1]]);
                    let mut ok = h(0) == 1 && h(4) < h(6);
                    for (j, &off) in a.block_offsets.iter().enumerate() {
                        let rp = u32::from_le_bytes(ram[8 + 4 * j..12 + 4 * j].try_into().unwrap());
                        ok &= if off == 0 { rp == 0 } else { rp == ptr + off };
                    }
                    ok_all &= ok;
                    println!(
                        "{} {p} == RAM DAT_800dd598[{k}] @0x{ptr:08X}: loaded {} slots {}..{} offsets relocated, rest identical",
                        if ok { "PASS" } else { "FAIL" },
                        h(0),
                        h(4),
                        h(6)
                    );
                }
                None => println!("(not loaded) {p}: no DAT_800dd598 entry holds this file"),
            }
        }
    }
    if oracle {
        println!("{}", if ok_all { "ALL PASS" } else { "SOME CHECKS FAILED" });
    }
}

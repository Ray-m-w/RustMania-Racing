//! mr-axe: dump / export / verify Muppet RaceMania .AXE sky domes.
//!
//!   mr-axe [summary] [DISC_PATH]  table summary (default COURSE_A/DOCK.AXE)
//!   mr-axe export [DISC_PATH]     analysis/axe/<name>_axe.obj (vertex colours) and .svg
//!   mr-axe all                    parse every .AXE on the disc, round-trip check
//!   mr-axe oracle                 read-only: compare Axe::to_ram_image with the bytes at
//!                                 *0x800b0c4c in the running recomp (TCP 4370) and the
//!                                 globals FUN_80042130 sets
//!
//! Output goes only to analysis/axe/ (gitignored).

use mr_assets::axe::Axe;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

fn analysis_dir() -> PathBuf {
    let mut d = std::env::current_dir().unwrap();
    loop {
        if d.join("game.toml").exists() {
            return d.join("analysis").join("axe");
        }
        if !d.pop() {
            return PathBuf::from("analysis").join("axe");
        }
    }
}

fn load(path: &str) -> (Vec<u8>, Axe) {
    let mut disc = mr_disc::Disc::open_default().expect("open disc (MRM_DISC or game.toml)");
    let b = disc.read(path).expect("read file");
    let a = Axe::parse(&b).expect("parse AXE");
    (b, a)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(|s| s.as_str()).unwrap_or("summary");
    let path = args.get(1).map(|s| s.as_str()).unwrap_or("COURSE_A/DOCK.AXE");
    match cmd {
        "summary" => summary(path),
        "export" => export(path),
        "all" => all(),
        "oracle" => oracle(path),
        _ => {
            eprintln!("usage: mr-axe [summary|export|all|oracle] [DISC_PATH]");
            std::process::exit(2);
        }
    }
}

fn summary(path: &str) {
    let (b, a) = load(path);
    println!("{path}: {} bytes, {} segments, {} face ids, tail at {:#x}", b.len(), a.segments.len(), a.n_face_ids, a.tail_offset);
    for (i, s) in a.segments.iter().enumerate() {
        let ids: Vec<u32> = s.faces.iter().map(|f| f.face_id).collect();
        println!(
            "seg {i:2}: verts {:3} @ {:#06x} faces {:2} @ {:#06x} spare {:#06x} corners {:?} unk {} {} ids {}..={}",
            s.vertices.len(), s.vert_offset, s.faces.len(), s.face_offset, s.spare_offset, s.corners,
            s.unk40, s.unk42, ids.iter().min().unwrap_or(&0), ids.iter().max().unwrap_or(&0)
        );
    }
    for (o, u) in &a.unreferenced {
        println!("unreferenced bytes @ {o:#x}: {} bytes", u.len());
    }
    println!("tail: {} bytes after the count, all zero: {}", a.tail_rest.len(), a.tail_rest.iter().all(|&x| x == 0));
    println!("drawn quads (deduplicated): {}", a.drawn_quads().len());
    println!("round trip identical: {}", a.to_bytes() == b);
}

fn stem(path: &str) -> String {
    let n = path.rsplit('/').next().unwrap_or(path);
    n.split('.').next().unwrap_or(n).to_ascii_lowercase()
}

fn export(path: &str) {
    let (_, a) = load(path);
    let dir = analysis_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let name = stem(path);
    let mut o = format!("# {path} sky dome decoded by mr-axe (model units, y flipped up, vertex colours 0..1)\n");
    let mut f = String::new();
    let mut n = 1;
    for q in a.drawn_quads() {
        // G4 order (v1, v0, v2, v3) -> polygon order v1, v0, v3, v2.
        for &k in &[0usize, 1, 3, 2] {
            let ([x, y, z], [r, g, b]) = q[k];
            o.push_str(&format!(
                "v {x} {} {z} {:.3} {:.3} {:.3}\n",
                -(y as i32), r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0
            ));
        }
        f.push_str(&format!("f {} {} {} {}\n", n, n + 1, n + 2, n + 3));
        n += 4;
    }
    o.push_str(&f);
    let objp = dir.join(format!("{name}_axe.obj"));
    std::fs::write(&objp, o).unwrap();

    // SVG: top-down (x right, z down), each quad filled with its mean colour,
    // drawn far-to-near by y so the rim shows over the cap.
    let mut quads = a.drawn_quads();
    quads.sort_by_key(|q| q.iter().map(|(p, _)| p[1] as i32).sum::<i32>());
    let r = 520.0;
    let mut v = format!("<svg xmlns='http://www.w3.org/2000/svg' viewBox='{} {} {} {}' width='800' height='800' style='background:#000'>\n", -r, -r, 2.0 * r, 2.0 * r);
    for q in quads {
        let pts: Vec<String> = [0usize, 1, 3, 2].iter().map(|&k| format!("{},{}", q[k].0[0], q[k].0[2])).collect();
        let c = |i: usize| q.iter().map(|(_, c)| c[i] as u32).sum::<u32>() / 4;
        v.push_str(&format!(
            "<polygon points='{}' fill='rgb({},{},{})' stroke='#000' stroke-width='0.5'/>\n",
            pts.join(" "), c(0), c(1), c(2)
        ));
    }
    v.push_str("</svg>\n");
    let svgp = dir.join(format!("{name}_axe.svg"));
    std::fs::write(&svgp, v).unwrap();
    println!("wrote {} and {}", objp.display(), svgp.display());
}

fn all() {
    let mut disc = mr_disc::Disc::open_default().expect("open disc");
    let mut paths: Vec<String> = disc.paths().filter(|p| p.ends_with(".AXE")).map(String::from).collect();
    paths.sort();
    let mut bad = 0;
    for p in &paths {
        let b = disc.read(p).unwrap();
        match Axe::parse(&b) {
            Ok(a) => {
                let rt = a.to_bytes() == b;
                bad += !rt as u32;
                let nv: usize = a.segments.iter().map(|s| s.vertices.len()).sum();
                let nf: usize = a.segments.iter().map(|s| s.faces.len()).sum();
                let un: usize = a.unreferenced.iter().map(|u| u.1.len()).sum();
                println!(
                    "{p:24} {:6} bytes segs {:2} verts {:4} faces {:4} ids {:4} unreferenced {:4} round-trip {}",
                    b.len(), a.segments.len(), nv, nf, a.n_face_ids, un, if rt { "ok" } else { "DIFF" }
                );
            }
            Err(e) => {
                bad += 1;
                println!("{p:24} ERROR {e}");
            }
        }
    }
    println!("{} files, {bad} problems", paths.len());
}

fn read_ram(addr: u32, len: usize) -> Vec<u8> {
    let port = std::env::var("MRM_DEBUG_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(4370u16);
    let mut c = std::net::TcpStream::connect(("127.0.0.1", port)).expect("connect to the recomp debug server");
    c.write_all(format!("{{\"id\":1,\"cmd\":\"read_ram\",\"addr\":\"0x{addr:08X}\",\"len\":{len}}}\n").as_bytes()).unwrap();
    let mut r = BufReader::with_capacity(1 << 20, c);
    let mut s = String::new();
    while s.trim().is_empty() {
        s.clear();
        r.read_line(&mut s).unwrap();
    }
    let i = s.find("\"hex\":\"").expect("read_ram reply") + 7;
    (0..len).map(|k| u8::from_str_radix(&s[i + 2 * k..i + 2 * k + 2], 16).unwrap()).collect()
}

fn oracle(path: &str) {
    let (b, a) = load(path);
    let g = read_ram(0x800b_0000, 0x1000);
    let w = |addr: u32| mr_assets::le::u32(&g, (addr - 0x800b_0000) as usize);
    let h = |addr: u32| mr_assets::le::i16(&g, (addr - 0x800b_0000) as usize);
    let base = w(0x800b_0c4c);
    println!("AXE at {base:#010x}");
    let ram = read_ram(base, b.len());
    let want = a.to_ram_image(base);
    let diffs: Vec<usize> = (0..b.len()).filter(|&i| ram[i] != want[i]).collect();
    println!("RAM == to_ram_image(base): {} ({} differing bytes)", diffs.is_empty(), diffs.len());
    for &i in diffs.iter().take(16) {
        println!("  +{i:#06x}: ram {:02x} want {:02x} disc {:02x}", ram[i], want[i], b[i]);
    }
    let raw_same = (0..b.len()).filter(|&i| ram[i] == b[i]).count();
    println!("bytes equal to the disc file as loaded: {raw_same}/{}", b.len());
    println!(
        "0x800b0c48 = {:#x} (want {:#x}), 0x800b0c64 = {:#x} (want {:#x}), 0x800b0c82 = {} (want {}), 0x800b0db0 = {} (want {})",
        w(0x800b_0c48), base + a.tail_offset, w(0x800b_0c64), base + 8,
        h(0x800b_0c82), a.segments.len(), h(0x800b_0db0), a.n_face_ids
    );
}

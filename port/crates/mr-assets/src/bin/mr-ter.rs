//! mr-ter: dump / export / verify Muppet RaceMania .TER terrain files.
//!
//!   mr-ter [summary] [DISC_PATH]   header + section summary (default COURSE_A/DOCK.TER)
//!   mr-ter export [DISC_PATH]      analysis/ter/<name>_ter.obj and .svg (top-down)
//!   mr-ter all                     parse every .TER on the disc, round-trip check
//!   mr-ter oracle-dump RAM.bin     same checks on a saved 2 MB read_ram dump
//!   mr-ter oracle                  read-only check against the running recomp
//!                                  (TCP 127.0.0.1:$MRM_DEBUG_PORT or 4370): file bytes in RAM at
//!                                  *0x800b0b3c, the pointers FUN_8003c2d8 derived, and each
//!                                  kart's ground face/height vs Ter::ground.
//!
//! Output goes only to analysis/ter/ (gitignored).

use mr_assets::ter::{Ter, KIND_WATER};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

fn analysis_dir() -> PathBuf {
    let mut d = std::env::current_dir().unwrap();
    loop {
        if d.join("game.toml").exists() {
            return d.join("analysis").join("ter");
        }
        if !d.pop() {
            return PathBuf::from("analysis").join("ter");
        }
    }
}

fn load(path: &str) -> (Vec<u8>, Ter) {
    let mut disc = mr_disc::Disc::open_default().expect("open disc (MRM_DISC or game.toml)");
    let b = disc.read(path).expect("read file");
    let t = Ter::parse(&b).expect("parse TER");
    (b, t)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(|s| s.as_str()).unwrap_or("summary");
    let path = args.get(1).map(|s| s.as_str()).unwrap_or("COURSE_A/DOCK.TER");
    match cmd {
        "summary" => summary(path),
        "export" => export(path),
        "all" => all(),
        "oracle" => oracle("COURSE_A/DOCK.TER", None),
        // A 2 MB main-RAM dump (0x80000000..) taken earlier with read_ram.
        "oracle-dump" => oracle("COURSE_A/DOCK.TER", Some(path)),
        p if p.contains('/') => summary(p),
        _ => {
            eprintln!("usage: mr-ter [summary|export|all|oracle] [DISC_PATH]");
            std::process::exit(2);
        }
    }
}

fn summary(path: &str) {
    let (b, t) = load(path);
    println!("{path}: {} bytes", b.len());
    println!("grid {}x{} cell {}x{} (y span {})", t.grid_w, t.grid_h, t.cell_x, t.cell_z, t.span_y);
    println!("bbox min {:?} max {:?} (TER units = world >> 5)", t.min, t.max);
    println!("no_water_y {}", t.no_water_y);
    println!(
        "vertices {} faces {} wall_points {} walls {} groups {} cell_list {} (+{} pad) path_points {} segments {} links {} (+{} pad)",
        t.vertices.len(), t.faces.len(), t.wall_points.len(), t.walls.len(), t.groups.len(),
        t.cell_list.len(), t.pad_list, t.path_points.len(), t.path_segments.len(), t.links.len(), t.pad_links
    );
    let mut kinds = std::collections::BTreeMap::new();
    for f in &t.faces {
        *kinds.entry((f.kind, f.flags)).or_insert(0) += 1;
    }
    println!("faces by (kind, flags): {kinds:?}");
    for (i, s) in t.path_segments.iter().enumerate() {
        let ls = s.link_start as usize;
        let next = &t.links[ls..ls + s.n_next as usize];
        let prev = &t.links[ls + s.n_next as usize..ls + (s.n_next + s.n_prev) as usize];
        println!(
            "seg {i:2}: points {}..{} next {next:?} prev {prev:?} flags {:#x} bb {:?}..{:?}",
            s.first, s.first + s.count, s.flags, s.bb_min, s.bb_max
        );
    }
    println!("round trip identical: {}", t.to_bytes() == b);
}

fn stem(path: &str) -> String {
    let n = path.rsplit('/').next().unwrap_or(path);
    n.split('.').next().unwrap_or(n).to_ascii_lowercase()
}

fn export(path: &str) {
    let (_, t) = load(path);
    let dir = analysis_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let name = stem(path);
    // OBJ: TER units, y negated so +y is up. Objects: floor, water, alt,
    // walls (vertical quads), path (polyline).
    let mut o = String::new();
    o.push_str(&format!("# {path} decoded by mr-ter (TER units, y flipped up)\n"));
    for v in &t.vertices {
        o.push_str(&format!("v {} {} {}\n", v.x, -v.y, v.z));
    }
    let nv = t.vertices.len();
    for (label, pick) in [
        ("floor", 0u8),
        ("water", 1),
        ("alt", 2),
    ] {
        o.push_str(&format!("o {label}\n"));
        for f in &t.faces {
            let k = if f.kind == KIND_WATER { 1 } else if f.alt() { 2 } else { 0 };
            if k != pick {
                continue;
            }
            o.push('f');
            for &i in f.corners() {
                o.push_str(&format!(" {}", i as usize + 1));
            }
            o.push('\n');
        }
    }
    o.push_str("o walls\n");
    let mut idx = nv + 1;
    let mut wv = String::new();
    let mut wf = String::new();
    for w in &t.walls {
        let (a, b) = (t.wall_points[w.p0 as usize], t.wall_points[w.p1 as usize]);
        let (at, ab, bt, bb) = if a.y_top == a.y_bottom && b.y_top == b.y_bottom {
            (a.y_top - 300, a.y_top, b.y_top - 300, b.y_top) // infinite wall: draw a marker band
        } else {
            (a.y_top, a.y_bottom, b.y_top, b.y_bottom)
        };
        wv.push_str(&format!("v {} {} {}\nv {} {} {}\nv {} {} {}\nv {} {} {}\n", a.x, -ab, a.z, a.x, -at, a.z, b.x, -bt, b.z, b.x, -bb, b.z));
        wf.push_str(&format!("f {} {} {} {}\n", idx, idx + 1, idx + 2, idx + 3));
        idx += 4;
    }
    o.push_str(&wv);
    o.push_str(&wf);
    o.push_str("o path\n");
    for p in &t.path_points {
        o.push_str(&format!("v {} {} {}\n", p.x, -p.y + 50, p.z));
    }
    for s in &t.path_segments {
        o.push('l');
        for k in 0..s.count as usize {
            o.push_str(&format!(" {}", idx + s.first as usize + k));
        }
        o.push('\n');
    }
    let objp = dir.join(format!("{name}_ter.obj"));
    std::fs::write(&objp, o).unwrap();

    // SVG top-down: x right, z down. Floors grey by kind, water blue,
    // walls red (enabled) with a tick on their positive side, path green.
    let (x0, z0) = (t.min[0] as f64, t.min[2] as f64);
    let (w, h) = ((t.max[0] - t.min[0]) as f64, (t.max[2] - t.min[2]) as f64);
    let s = 2000.0 / w.max(h);
    let px = |x: i32| (x as f64 - x0) * s;
    let pz = |z: i32| (z as f64 - z0) * s;
    let mut v = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{:.0}' height='{:.0}' style='background:#111'>\n",
        w * s, h * s
    );
    for c in 0..=t.grid_w {
        let x = px(t.min[0] + c * t.cell_x);
        v.push_str(&format!("<line x1='{x:.1}' y1='0' x2='{x:.1}' y2='{:.0}' stroke='#222'/>\n", h * s));
    }
    for c in 0..=t.grid_h {
        let z = pz(t.min[2] + c * t.cell_z);
        v.push_str(&format!("<line x1='0' y1='{z:.1}' x2='{:.0}' y2='{z:.1}' stroke='#222'/>\n", w * s));
    }
    for f in &t.faces {
        let pts: Vec<String> = f
            .corners()
            .iter()
            .map(|&i| {
                let p = t.vertices[i as usize];
                format!("{:.1},{:.1}", px(p.x), pz(p.z))
            })
            .collect();
        let fill = if f.kind == KIND_WATER {
            "#2a5caa".to_string()
        } else {
            let g = 60 + (f.kind as u32 * 13) % 120;
            format!("rgb({g},{g},{})", g + 20)
        };
        v.push_str(&format!(
            "<polygon points='{}' fill='{fill}' fill-opacity='0.6' stroke='#000' stroke-width='0.3'><title>face kind {} flags {:#x}</title></polygon>\n",
            pts.join(" "), f.kind, f.flags
        ));
    }
    for (i, wl) in t.walls.iter().enumerate() {
        let (b, a) = (t.wall_points[wl.p0 as usize], t.wall_points[wl.p1 as usize]);
        let col = if wl.disabled { "#844" } else { "#f33" };
        v.push_str(&format!(
            "<line x1='{:.1}' y1='{:.1}' x2='{:.1}' y2='{:.1}' stroke='{col}' stroke-width='1.2'><title>wall {i} top {}/{} bottom {}/{}</title></line>\n",
            px(a.x), pz(a.z), px(b.x), pz(b.z), a.y_top, b.y_top, a.y_bottom, b.y_bottom
        ));
    }
    for sg in &t.path_segments {
        let pts: Vec<String> = (0..sg.count as usize)
            .map(|k| {
                let p = t.path_points[sg.first as usize + k];
                format!("{:.1},{:.1}", px(p.x), pz(p.z))
            })
            .collect();
        v.push_str(&format!("<polyline points='{}' fill='none' stroke='#3c3' stroke-width='1'/>\n", pts.join(" ")));
    }
    v.push_str("</svg>\n");
    let svgp = dir.join(format!("{name}_ter.svg"));
    std::fs::write(&svgp, v).unwrap();
    println!("wrote {} and {}", objp.display(), svgp.display());
}

fn all() {
    let mut disc = mr_disc::Disc::open_default().expect("open disc");
    let mut paths: Vec<String> = disc.paths().filter(|p| p.ends_with(".TER")).map(String::from).collect();
    paths.sort();
    let mut bad = 0;
    for p in &paths {
        let b = disc.read(p).unwrap();
        match Ter::parse(&b) {
            Ok(t) => {
                let rt = t.to_bytes() == b;
                if !rt {
                    bad += 1;
                }
                println!(
                    "{p:28} grid {:2}x{:2} verts {:5} faces {:5} walls {:5} groups {:2} path {:4}/{:3} round-trip {}",
                    t.grid_w, t.grid_h, t.vertices.len(), t.faces.len(), t.walls.len(), t.groups.len(),
                    t.path_points.len(), t.path_segments.len(), if rt { "ok" } else { "DIFF" }
                );
            }
            Err(e) => {
                bad += 1;
                println!("{p:28} ERROR {e}");
            }
        }
    }
    println!("{} files, {bad} problems", paths.len());
}

// ---- read-only oracle check ----

pub struct Oracle {
    port: u16,
    id: u32,
}

impl Oracle {
    pub fn connect() -> std::io::Result<Oracle> {
        let port = std::env::var("MRM_DEBUG_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(4370u16);
        std::net::TcpStream::connect(("127.0.0.1", port))?;
        Ok(Oracle { port, id: 0 })
    }
    fn q(&mut self, body: &str) -> String {
        self.id += 1;
        let line = format!("{{\"id\":{},{body}}}\n", self.id);
        // One connection per request (the server drops long-lived idle
        // clients between requests in practice).
        let mut c = std::net::TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        c.write_all(line.as_bytes()).unwrap();
        let mut r = BufReader::with_capacity(1 << 20, c);
        loop {
            let mut s = String::new();
            r.read_line(&mut s).unwrap();
            if !s.trim().is_empty() {
                return s;
            }
        }
    }
    pub fn frame(&mut self) -> i64 {
        let s = self.q("\"cmd\":\"frame\"");
        let i = s.find("\"frame\":").unwrap() + 8;
        s[i..].chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap()
    }
    /// read_ram (read-only). One request = one consistent snapshot.
    pub fn read(&mut self, addr: u32, len: usize) -> Vec<u8> {
        let s = self.q(&format!("\"cmd\":\"read_ram\",\"addr\":\"0x{addr:08X}\",\"len\":{len}"));
        let i = s.find("\"hex\":\"").expect("read_ram reply") + 7;
        let hex = &s[i..i + 2 * len];
        (0..len).map(|k| u8::from_str_radix(&hex[2 * k..2 * k + 2], 16).unwrap()).collect()
    }
}

struct Ram(Vec<u8>);
impl Ram {
    fn u32(&self, a: u32) -> u32 {
        mr_assets::le::u32(&self.0, (a - 0x8000_0000) as usize)
    }
    fn i32(&self, a: u32) -> i32 {
        self.u32(a) as i32
    }
    fn i16(&self, a: u32) -> i16 {
        mr_assets::le::i16(&self.0, (a - 0x8000_0000) as usize)
    }
    fn bytes(&self, a: u32, n: usize) -> &[u8] {
        let o = (a - 0x8000_0000) as usize;
        &self.0[o..o + n]
    }
}

/// Kart records: 6 x 0x504 bytes from 0x800c30a8 (the argument of
/// FUN_8003e91c). Offsets used here are the ones FUN_8003e91c writes.
pub const KART_BASE: u32 = 0x800c_30a8;
pub const KART_STRIDE: u32 = 0x504;

fn oracle(path: &str, dump: Option<&str>) {
    let (b, t) = load(path);
    let mut o = None;
    let ram = match dump {
        Some(p) => {
            println!("RAM dump {p}");
            Ram(std::fs::read(p).expect("read RAM dump"))
        }
        None => {
            let mut c = Oracle::connect().expect("connect to the recomp debug server");
            let f0 = c.frame();
            let r = Ram(c.read(0x8000_0000, 0x20_0000));
            println!("snapshot at frame {f0}");
            o = Some(c);
            r
        }
    };
    let base = ram.u32(0x800b_0b3c);
    let same = ram.bytes(base, b.len()) == &b[..];
    println!("TER at {base:#010x}: RAM == disc bytes: {same}");
    let names: [(u32, usize); 12] = [
        (0x800b0f90, 0), (0x800b0fa4, 1), (0x800b0e7c, 2), (0x800b0f5c, 3), (0x800b0e44, 4), (0x800b0e34, 5),
        (0x800b0f40, 6), (0x800b0fb0, 7), (0x800b0f64, 8), (0x800b0ac4, 9), (0x800b0c50, 10), (0x800b0a84, 11),
    ];
    let mut ptr_ok = true;
    for (g, sec) in names {
        let ok = ram.u32(g) == base + t.offsets[sec];
        ptr_ok &= ok;
        if !ok {
            println!("  pointer {g:#x} = {:#x}, expected {:#x}", ram.u32(g), base + t.offsets[sec]);
        }
    }
    let scal: [(u32, i32); 13] = [
        (0x800b0d78, t.grid_w), (0x800b0d7c, t.grid_h), (0x800b0b6c, t.min[0]), (0x800b0b78, t.min[1]),
        (0x800b0b80, t.min[2]), (0x800b0b74, t.max[0]), (0x800b0b7c, t.max[1]), (0x800b0b88, t.max[2]),
        (0x800b0bb4, t.cell_x), (0x800b0bbc, t.cell_z), (0x800b0d38, t.no_water_y),
        (0x800b0f50, t.groups.len() as i32), (0x800b0d5c, t.path_segments.len() as i32),
    ];
    for (g, v) in scal {
        let ok = ram.i32(g) == v;
        ptr_ok &= ok;
        if !ok {
            println!("  global {g:#x} = {}, expected {v}", ram.i32(g));
        }
    }
    println!("FUN_8003c2d8 pointers/globals match: {ptr_ok}");

    // Last FUN_8003ddb0 call's outputs (whichever kart/caller made it).
    let fptr = |p: u32| if p == 0 { None } else { Some(((p - base - t.offsets[1]) / 16) as usize) };
    println!(
        "last ground call: cell ({}, {}) nearest face {:?} y {} below {:?} y {} water {:?} y {} chosen {}",
        ram.i32(0x800b0d64), ram.i32(0x800b0d6c), fptr(ram.u32(0x800b0b0c)), ram.i32(0x800b0b24),
        fptr(ram.u32(0x800b0ae0)), ram.i32(0x800b0b04), fptr(ram.u32(0x800b0f8c)), ram.i32(0x800b0a54),
        ram.i32(0x800b0c00)
    );
    let mut n_ok = 0;
    for k in 0..6u32 {
        let a = KART_BASE + k * KART_STRIDE;
        let (x, y, z) = (ram.i32(a), ram.i32(a + 4), ram.i32(a + 8));
        let face = fptr(ram.u32(a + 0x2e0));
        let water = fptr(ram.u32(a + 0x2e4));
        let gy = ram.i32(a + 0x31c);
        let qy = (2 * y + ram.i16(a + 0x438) as i32 + ram.i16(a + 0x43a) as i32) / 2;
        let g = t.ground(x, qy, z);
        let mine = g.nearest.map(|h| h.face);
        let ok = mine == face && g.nearest_y == gy && g.water.map(|h| h.face) == water;
        n_ok += ok as u32;
        println!(
            "kart {k}: pos ({x}, {y}, {z}) query y {qy} RAM face {face:?} y {gy} water {water:?} | rust face {mine:?} y {} water {:?} y {} cell ({},{}) {}",
            g.nearest_y, g.water.map(|h| h.face), g.water_y, g.cell_x, g.cell_z, if ok { "MATCH" } else { "DIFF" }
        );
        if !ok {
            // The kart may not have run FUN_8003e91c since it last moved:
            // try its previous position (+0x2e8/+0x2ec/+0x2f0) and the
            // "below" candidate.
            let (ox, oz) = (ram.i32(a + 0x2e8), ram.i32(a + 0x2f0));
            let g2 = t.ground(ox, qy, oz);
            println!(
                "    at previous pos ({ox}, {oz}): nearest {:?} below {:?} alt {:?}; at current pos: below {:?} alt {:?}",
                g2.nearest, g2.below, g2.alt, g.below, g.alt
            );
        }
    }
    println!("karts matching: {n_ok}/6");
    if let Some(mut c) = o {
        println!("frame now {}", c.frame());
    }
}

//! mr-raw: decode a `.RAW` texture container from the owner's disc, write the
//! decoded VRAM as PNG, and (unless --no-oracle) compare every upload with
//! the running recomp's VRAM/RAM over its TCP debug server (read-only
//! commands only: gpu_state, vram_peek, read_ram).
//!
//! usage: mr-raw [DISC_PATH] [--course N] [--no-oracle] [--port P]
//!   DISC_PATH defaults to COURSE_A/DOCK.RAW, course to 3 (DOCK).
//! writes analysis/raw/<stem>_vram.png and <stem>_vram_oracle.png.

use mr_assets::img::png_rgba;
use mr_assets::raw::{self, LoadEnv, UploadKind};
use mr_assets::vram::{self, Vram};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::PathBuf;

struct Oracle {
    r: BufReader<TcpStream>,
    w: TcpStream,
    id: u32,
    port: u16,
}

impl Oracle {
    fn connect(port: u16) -> std::io::Result<Self> {
        let s = TcpStream::connect(("127.0.0.1", port))?;
        s.set_read_timeout(Some(std::time::Duration::from_secs(60)))?;
        Ok(Oracle { r: BufReader::new(s.try_clone()?), w: s, id: 0, port })
    }
    /// One request, one reply line. The debug server serves one client at a
    /// time and drops idle ones, so reconnect and retry (like oracle.py).
    fn q(&mut self, cmd: &str, args: &str) -> String {
        for attempt in 0..5 {
            match self.try_q(cmd, args) {
                Ok(l) => return l,
                Err(e) => {
                    if attempt == 4 {
                        panic!("oracle {cmd}: {e}");
                    }
                    std::thread::sleep(std::time::Duration::from_millis(200));
                    if let Ok(n) = Oracle::connect(self.port) {
                        *self = n;
                    }
                }
            }
        }
        unreachable!()
    }
    fn try_q(&mut self, cmd: &str, args: &str) -> std::io::Result<String> {
        self.id += 1;
        let sep = if args.is_empty() { "" } else { "," };
        writeln!(self.w, "{{\"id\":{},\"cmd\":\"{}\"{}{}}}", self.id, cmd, sep, args)?;
        loop {
            let mut line = String::new();
            self.r.read_line(&mut line)?;
            if line.is_empty() {
                return Err(std::io::Error::new(std::io::ErrorKind::ConnectionAborted, "closed"));
            }
            if !line.trim().is_empty() {
                return Ok(line);
            }
        }
    }
    fn vram(&mut self, x: usize, y: usize, w: usize, h: usize) -> Vec<u16> {
        let r = self.q("vram_peek", &format!("\"x\":{x},\"y\":{y},\"w\":{w},\"h\":{h}"));
        let hex = jstr(&r, "hex").unwrap_or_else(|| panic!("vram_peek: {r}"));
        // one "%04x" group per pixel
        (0..w * h).map(|i| u16::from_str_radix(&hex[i * 4..i * 4 + 4], 16).unwrap()).collect()
    }
    fn ram(&mut self, addr: u32, n: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(n);
        while out.len() < n {
            let k = (n - out.len()).min(4096);
            let a = addr + out.len() as u32;
            let r = self.q("read_ram", &format!("\"addr\":\"0x{a:08X}\",\"len\":{k}"));
            let hex = jstr(&r, "hex").or_else(|| jstr(&r, "data")).unwrap_or_else(|| panic!("read_ram: {r}"));
            out.extend((0..k).map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap()));
        }
        out
    }
}

fn jstr(s: &str, key: &str) -> Option<String> {
    let k = format!("\"{key}\": \"");
    let k2 = format!("\"{key}\":\"");
    let i = s.find(&k).map(|i| i + k.len()).or_else(|| s.find(&k2).map(|i| i + k2.len()))?;
    let e = s[i..].find('"')?;
    Some(s[i..i + e].to_string())
}

fn jnums(s: &str, key: &str) -> Vec<i64> {
    let k = format!("\"{key}\":");
    let Some(i) = s.find(&k) else { return vec![] };
    let rest = s[i + k.len()..].trim_start();
    let body: String = if rest.starts_with('[') {
        rest[1..rest.find(']').unwrap()].to_string()
    } else {
        rest.chars().take_while(|c| c.is_ascii_digit() || *c == '-').collect()
    };
    body.split(',').filter_map(|t| t.trim().parse().ok()).collect()
}

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

fn to_png(v: &Vram) -> Vec<u8> {
    let rgba: Vec<u8> = v.px.iter().flat_map(|&c| vram::rgba(c)).collect();
    png_rgba(vram::W as u32, vram::H as u32, &rgba)
}

#[derive(Clone, Copy)]
struct R {
    x0: i64,
    y0: i64,
    x1: i64, // exclusive
    y1: i64,
}
impl R {
    fn has(&self, x: i64, y: i64) -> bool {
        x >= self.x0 && x < self.x1 && y >= self.y0 && y < self.y1
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut path = "COURSE_A/DOCK.RAW".to_string();
    let mut course = raw::DOCK_COURSE;
    let mut use_oracle = true;
    let mut port = 4370u16;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--course" => {
                i += 1;
                course = args[i].parse().expect("--course N");
            }
            "--no-oracle" => use_oracle = false,
            "--port" => {
                i += 1;
                port = args[i].parse().expect("--port P");
            }
            p => path = p.to_string(),
        }
        i += 1;
    }

    let mut disc = mr_disc::Disc::open_default().expect("open disc (MRM_DISC or game.toml)");
    let file = disc.read(&path).expect("read file");
    let exe = disc.read("SLUS_012.37").expect("read exe");
    let env = LoadEnv::from_exe(&exe, course).expect("course table");
    println!("{path}: {} bytes; course {course} fog {:02x?} flags {:#06x}", file.len(), env.fog_rgb, env.course_flags);

    let rf = raw::parse(&file).expect("parse");
    println!("{} chunks, terminator at {:#x}, {} trailing bytes", rf.chunks.len(), rf.terminator, rf.trailing);
    for (k, c) in rf.chunks.iter().enumerate() {
        println!(
            "  #{k:2} @{:#07x} type {:#05x} packed {:6} unpacked {:6} crc ok",
            c.file_offset, c.kind, c.header.packed_len, c.header.unpacked_len
        );
    }
    let dec = raw::decode_chunks(&rf.chunks, &env).expect("decode");
    let mut v = Vram::default();
    dec.apply(&mut v);

    let stem = std::path::Path::new(&path).file_stem().unwrap().to_string_lossy().to_ascii_lowercase();
    let out = repo_root().join("analysis").join("raw");
    std::fs::create_dir_all(&out).unwrap();
    let p1 = out.join(format!("{stem}_vram.png"));
    std::fs::write(&p1, to_png(&v)).unwrap();
    println!("wrote {}", p1.display());

    println!("slots:");
    for (s, e) in dec.slots.iter().enumerate() {
        if let Some(e) = e {
            println!("  slot {s:#04x}: tpage {:?} clut {:#06x} x {:#x} y {:#x} mode {:?}", e.tpage.map(|t| format!("{t:#06x}")), e.clut, e.x, e.y, e.mode);
        }
    }
    for a in &dec.anims {
        println!("  anim slot {:#04x}: {}x{} x {} frames", a.slot, a.w, a.h, a.frames.len());
    }

    if !use_oracle {
        for u in &dec.uploads {
            println!("  upload type {:#04x} {:?} at ({:#x},{:#x}) {}x{}", u.kind, u.what, u.x, u.y, u.w, u.h);
        }
        return;
    }
    let mut o = match Oracle::connect(port) {
        Ok(o) => o,
        Err(e) => {
            println!("oracle not reachable on {port} ({e}); comparison skipped");
            return;
        }
    };
    let frame = o.q("frame", "");
    println!("oracle {}", frame.trim());
    let gs = o.q("gpu_state", "");
    let (dx, dy) = (jnums(&gs, "display_x")[0], jnums(&gs, "display_y")[0]);
    let (dw, dh) = (jnums(&gs, "width")[0], jnums(&gs, "height")[0]);
    let da = jnums(&gs, "draw_area");
    let mut fbs = vec![R { x0: dx, y0: dy, x1: dx + dw, y1: dy + dh }];
    if da.len() == 4 {
        fbs.push(R { x0: da[0], y0: da[1], x1: da[2] + 1, y1: da[3] + 1 });
    }
    println!("framebuffers excluded: display ({dx},{dy}) {dw}x{dh}, draw area {:?}", da);

    let mut ov = Vram::default();
    for ty in (0..vram::H).step_by(128) {
        for tx in (0..vram::W).step_by(128) {
            let t = o.vram(tx, ty, 128, 128);
            ov.load_image(tx, ty, 128, 128, &t);
        }
    }
    let p2 = out.join(format!("{stem}_vram_oracle.png"));
    std::fs::write(&p2, to_png(&ov)).unwrap();
    println!("wrote {}", p2.display());

    // Animated-texture frames found in the oracle VRAM (the game copies one
    // frame of each 0x22 strip into a page every few frames).
    let mut anim_rects: Vec<(R, u32, usize)> = Vec::new();
    for a in &dec.anims {
        let hw = a.w as usize / 4;
        let h = a.h as usize;
        if hw == 0 || h == 0 {
            continue;
        }
        'frames: for (fi, f) in a.frames.iter().enumerate() {
            let distinct = f.iter().collect::<std::collections::HashSet<_>>().len();
            if distinct < 3 {
                continue;
            }
            for y in 0..=vram::H - h {
                for x in 0..=vram::W - hw {
                    if ov.get(x, y) != f[0] {
                        continue;
                    }
                    if (0..h).all(|r| (0..hw).all(|c| ov.get(x + c, y + r) == f[r * hw + c])) {
                        anim_rects.push((R { x0: x as i64, y0: y as i64, x1: (x + hw) as i64, y1: (y + h) as i64 }, a.slot, fi));
                        continue 'frames;
                    }
                }
            }
        }
    }
    for (r, s, f) in &anim_rects {
        println!("anim slot {s:#04x} frame {f} found in oracle VRAM at ({:#x},{:#x}) {}x{}", r.x0, r.y0, r.x1 - r.x0, r.y1 - r.y0);
    }

    println!("per-upload match (decoded vs oracle VRAM; framebuffer pixels skipped):");
    let (mut tot, mut tot_ok, mut tot_ok_anim, mut tot_anim) = (0usize, 0usize, 0usize, 0usize);
    for u in &dec.uploads {
        let (mut n, mut ok, mut na, mut ok_na) = (0usize, 0usize, 0usize, 0usize);
        for r in 0..u.h as usize {
            for c in 0..u.w as usize {
                let (x, y) = (u.x as usize + c, u.y as usize + r);
                if fbs.iter().any(|f| f.has(x as i64, y as i64)) {
                    continue;
                }
                // later uploads overwrite earlier ones: compare against final decoded VRAM
                let want = u.data[r * u.w as usize + c];
                if v.get(x, y) != want {
                    continue;
                }
                n += 1;
                let m = ov.get(x, y) == want;
                ok += m as usize;
                if anim_rects.iter().any(|(a, _, _)| a.has(x as i64, y as i64)) {
                    na += 1;
                } else {
                    ok_na += m as usize;
                }
            }
        }
        let what = match u.what {
            UploadKind::Page => "page ".to_string(),
            UploadKind::Clut { variant } => format!("clut{variant}"),
            UploadKind::Image => "image".to_string(),
        };
        let pct = |a: usize, b: usize| if b == 0 { 100.0 } else { 100.0 * a as f64 / b as f64 };
        println!(
            "  type {:#04x} {what} ({:#05x},{:#05x}) {:3}x{:3}: {:7.3}% of {:6} px; {:7.3}% excluding {} animated px",
            u.kind, u.x, u.y, u.w, u.h, pct(ok, n), n, pct(ok_na, n - na), na
        );
        tot += n;
        tot_ok += ok;
        tot_anim += na;
        tot_ok_anim += ok_na;
    }
    println!(
        "TOTAL: {}/{} px = {:.3}%; excluding {} animated px: {}/{} = {:.3}%",
        tot_ok, tot, 100.0 * tot_ok as f64 / tot.max(1) as f64, tot_anim, tot_ok_anim, tot - tot_anim,
        100.0 * tot_ok_anim as f64 / (tot - tot_anim).max(1) as f64
    );

    // RAM checks
    let ram_course = u32::from_le_bytes(o.ram(0x800b_0fac, 4).try_into().unwrap());
    println!("RAM DAT_800b0fac (course) = {ram_course} (decoded with {course})");
    let tab = o.ram(0x800d_ff00, 32 * 16);
    let (mut sn, mut sok) = (0, 0);
    for (s, e) in dec.slots.iter().enumerate() {
        let Some(e) = e else { continue };
        let b = &tab[s * 16..s * 16 + 16];
        let g = |o: usize| u16::from_le_bytes([b[o], b[o + 1]]);
        let ok = e.tpage.map_or(true, |t| t == g(0)) && e.clut == g(2) && e.x as u16 == g(4) && e.y as u16 == g(6) && e.mode.map_or(true, |m| m == g(8));
        sn += 1;
        sok += ok as usize;
        if !ok {
            println!("  slot {s:#04x} MISMATCH: RAM tpage {:#06x} clut {:#06x} x {:#x} y {:#x} mode {}", g(0), g(2), g(4), g(6), g(8));
        }
    }
    println!("slot table 0x800dff00: {sok}/{sn} slots match");
    let ptrs = o.ram(0x800b_bb20, 65 * 4);
    for r in dec.retained.iter().filter(|r| r.kind == 0x22) {
        let slot = u32::from_le_bytes(r.data[4..8].try_into().unwrap()) as usize;
        let p = u32::from_le_bytes(ptrs[slot * 4..slot * 4 + 4].try_into().unwrap());
        let want = &r.data[8..];
        let got = o.ram(p, want.len());
        let eq = want.iter().zip(&got).filter(|(a, b)| a == b).count();
        println!("  0x22 slot {slot:#04x} -> DAT_800bbb20[{slot:#x}] = {p:#010x}: {eq}/{} bytes equal (buffer offset {:#x})", want.len(), r.buffer_offset);
    }
}

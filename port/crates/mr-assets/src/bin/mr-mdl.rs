//! mr-mdl: decode `.MDL` models from the owner's disc and export them for
//! checking; optionally prove the decode against the running recomp.
//!
//! usage: mr-mdl [DISC_PATH.MDL ...] [--raw COURSE_A/DOCK.RAW] [--course N]
//!               [--oracle] [--port P] [--frames N]
//!   default models: CHARS_A/KERMIT.MDL VEHIC_A/RACER.MDL
//! writes analysis/mdl/<stem>.txt, <stem>.obj/.mtl, <stem>_slotNN.png
//! (texture pages through the part's CLUTs, from the course .RAW decoded by
//! raw.rs) and <stem>_preview.png (software render of the rest pose).
//! --oracle (read-only TCP commands: read_ram, vram_peek, gte_ring_dump,
//! gpu_frame_dump, frame) checks: polygon bytes vs RAM model slots; every
//! mesh polygon vs GTE RTPT/RTPS input; ring vertices vs seam-renderer
//! RTPS input; decoded texture pages and CLUTs vs VRAM; tpage/CLUT vs GPU
//! packets. Prints a PASS/FAIL line per check.

use mr_assets::img::png_rgba;
use mr_assets::mdl::{self, Mdl, PartData, Polygon, SlotAttr};
use mr_assets::raw::{self, LoadEnv};
use mr_assets::vram::{self, Depth, Vram};
use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::PathBuf;

// ------------------------------------------------------------ oracle ----

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
    fn q(&mut self, cmd: &str, args: &str) -> Json {
        for attempt in 0..5 {
            match self.try_q(cmd, args) {
                Ok(l) => return Json::parse(&l).unwrap_or_else(|| panic!("bad json from {cmd}")),
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
    fn ram(&mut self, addr: u32, n: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(n);
        while out.len() < n {
            let k = (n - out.len()).min(65536);
            let a = addr + out.len() as u32;
            let r = self.q("read_ram", &format!("\"addr\":\"0x{a:08X}\",\"len\":{k}"));
            let hex = r.get("hex").or_else(|| r.get("data")).and_then(|h| h.str()).unwrap_or_else(|| panic!("read_ram failed"));
            out.extend((0..k).map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap()));
        }
        out
    }
    /// vram_peek: one "%04x" group per pixel (as mr-raw reads it).
    fn vram(&mut self, x: usize, y: usize, w: usize, h: usize) -> Vec<u16> {
        let mut out = vec![0u16; w * h];
        let mut ty = 0;
        while ty < h {
            let th = (h - ty).min(128);
            let mut tx = 0;
            while tx < w {
                let tw = (w - tx).min(128);
                let r = self.q("vram_peek", &format!("\"x\":{},\"y\":{},\"w\":{tw},\"h\":{th}", x + tx, y + ty));
                let hex = r.get("hex").and_then(|h| h.str()).unwrap_or_else(|| panic!("vram_peek failed"));
                for j in 0..th {
                    for i in 0..tw {
                        let k = (j * tw + i) * 4;
                        out[(ty + j) * w + tx + i] = u16::from_str_radix(&hex[k..k + 4], 16).unwrap();
                    }
                }
                tx += tw;
            }
            ty += th;
        }
        out
    }
    fn frame(&mut self) -> i64 {
        self.q("frame", "").get("frame").and_then(|v| v.num()).unwrap_or(0.0) as i64
    }
}

/// Minimal JSON value (enough for the debug server's replies).
#[derive(Debug, Clone)]
enum Json {
    Null,
    Bool,
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    fn parse(s: &str) -> Option<Json> {
        let b = s.as_bytes();
        let mut i = 0;
        let v = Self::val(b, &mut i)?;
        Some(v)
    }
    fn ws(b: &[u8], i: &mut usize) {
        while *i < b.len() && (b[*i] as char).is_ascii_whitespace() {
            *i += 1;
        }
    }
    fn val(b: &[u8], i: &mut usize) -> Option<Json> {
        Self::ws(b, i);
        match *b.get(*i)? {
            b'{' => {
                *i += 1;
                let mut o = Vec::new();
                loop {
                    Self::ws(b, i);
                    if b[*i] == b'}' {
                        *i += 1;
                        return Some(Json::Obj(o));
                    }
                    let Json::Str(k) = Self::val(b, i)? else { return None };
                    Self::ws(b, i);
                    *i += 1; // ':'
                    let v = Self::val(b, i)?;
                    o.push((k, v));
                    Self::ws(b, i);
                    if b[*i] == b',' {
                        *i += 1;
                    }
                }
            }
            b'[' => {
                *i += 1;
                let mut a = Vec::new();
                loop {
                    Self::ws(b, i);
                    if b[*i] == b']' {
                        *i += 1;
                        return Some(Json::Arr(a));
                    }
                    a.push(Self::val(b, i)?);
                    Self::ws(b, i);
                    if b[*i] == b',' {
                        *i += 1;
                    }
                }
            }
            b'"' => {
                *i += 1;
                let mut s = String::new();
                while b[*i] != b'"' {
                    if b[*i] == b'\\' {
                        *i += 1;
                    }
                    s.push(b[*i] as char);
                    *i += 1;
                }
                *i += 1;
                Some(Json::Str(s))
            }
            b't' => {
                *i += 4;
                Some(Json::Bool)
            }
            b'f' => {
                *i += 5;
                Some(Json::Bool)
            }
            b'n' => {
                *i += 4;
                Some(Json::Null)
            }
            _ => {
                let st = *i;
                while *i < b.len() && matches!(b[*i], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9') {
                    *i += 1;
                }
                std::str::from_utf8(&b[st..*i]).ok()?.parse().ok().map(Json::Num)
            }
        }
    }
    fn get(&self, k: &str) -> Option<&Json> {
        match self {
            Json::Obj(o) => o.iter().find(|(kk, _)| kk == k).map(|(_, v)| v),
            _ => None,
        }
    }
    fn str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }
    fn num(&self) -> Option<f64> {
        match self {
            Json::Num(n) => Some(*n),
            _ => None,
        }
    }
    fn arr(&self) -> &[Json] {
        match self {
            Json::Arr(a) => a,
            _ => &[],
        }
    }
    fn ivec(&self) -> Vec<i64> {
        self.arr().iter().filter_map(|v| v.num()).map(|n| n as i64).collect()
    }
    fn hexnum(&self) -> Option<u32> {
        let s = self.str()?;
        u32::from_str_radix(s.trim_start_matches("0x").trim_start_matches("0X"), 16).ok()
    }
}

// ----------------------------------------------------------- helpers ----

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

fn stem(path: &str) -> String {
    path.rsplit('/').next().unwrap().split('.').next().unwrap().to_ascii_lowercase()
}

fn page_of(tpage: u16) -> (usize, usize, Depth) {
    vram::tpage(tpage)
}

/// The part's texture page rendered as a 256x256 image, each 64x64 cell
/// through its own CLUT (slot clut + cell index), as the emitter picks it.
fn page_png(v: &Vram, slot: SlotAttr, flag: u8) -> Vec<u8> {
    let (px, py, d) = page_of(slot.tpage.wrapping_add((flag & 0x60) as u16));
    let mut rgba = vec![0u8; 256 * 256 * 4];
    for t in 0..256usize {
        for s in 0..256usize {
            let off = ((t >> 6) * 4 + (s >> 6)) as u16;
            let (cx, cy) = vram::clut(slot.clut.wrapping_add(off));
            let c = v.texel(d, px, py, cx, cy, s as u8, t as u8);
            rgba[(t * 256 + s) * 4..(t * 256 + s) * 4 + 4].copy_from_slice(&vram::rgba(c));
        }
    }
    png_rgba(256, 256, &rgba)
}

// ------------------------------------------------------------ render ----

struct Canvas {
    w: usize,
    h: usize,
    rgba: Vec<u8>,
    z: Vec<f32>,
}

impl Canvas {
    fn new(w: usize, h: usize) -> Self {
        let mut rgba = vec![0u8; w * h * 4];
        for p in rgba.chunks_mut(4) {
            p.copy_from_slice(&[40, 44, 52, 255]);
        }
        Canvas { w, h, rgba, z: vec![f32::INFINITY; w * h] }
    }
}

struct Tri {
    p: [[f32; 3]; 3], // screen x, y, depth
    uv: [[f32; 2]; 3],
    col: [[f32; 3]; 3],
    tex: Option<(Depth, usize, usize, usize, usize)>,
    semi: bool,
}

fn raster(c: &mut Canvas, v: &Vram, t: &Tri) {
    let [a, b, d] = t.p;
    let area = (b[0] - a[0]) * (d[1] - a[1]) - (b[1] - a[1]) * (d[0] - a[0]);
    if area.abs() < 1e-6 {
        return;
    }
    let minx = a[0].min(b[0]).min(d[0]).floor().max(0.0) as usize;
    let maxx = (a[0].max(b[0]).max(d[0]).ceil() as usize).min(c.w - 1);
    let miny = a[1].min(b[1]).min(d[1]).floor().max(0.0) as usize;
    let maxy = (a[1].max(b[1]).max(d[1]).ceil() as usize).min(c.h - 1);
    for y in miny..=maxy {
        for x in minx..=maxx {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let w0 = ((b[0] - fx) * (d[1] - fy) - (b[1] - fy) * (d[0] - fx)) / area;
            let w1 = ((d[0] - fx) * (a[1] - fy) - (d[1] - fy) * (a[0] - fx)) / area;
            let w2 = 1.0 - w0 - w1;
            if w0 < -1e-4 || w1 < -1e-4 || w2 < -1e-4 {
                continue;
            }
            let z = w0 * a[2] + w1 * b[2] + w2 * d[2];
            let i = y * c.w + x;
            if z >= c.z[i] {
                continue;
            }
            let col = [0, 1, 2].map(|k| w0 * t.col[0][k] + w1 * t.col[1][k] + w2 * t.col[2][k]);
            let mut rgb = col.map(|q| q * 2.0);
            if let Some((depth, px, py, cx, cy)) = t.tex {
                let u = (w0 * t.uv[0][0] + w1 * t.uv[1][0] + w2 * t.uv[2][0]).clamp(0.0, 255.0) as u8;
                let vv = (w0 * t.uv[0][1] + w1 * t.uv[1][1] + w2 * t.uv[2][1]).clamp(0.0, 255.0) as u8;
                let texel = v.texel(depth, px, py, cx, cy, u, vv);
                if texel == 0 {
                    continue;
                }
                let tc = vram::rgba(texel);
                rgb = [0, 1, 2].map(|k| tc[k] as f32 * col[k] / 128.0);
            }
            c.z[i] = z;
            let o = i * 4;
            for k in 0..3 {
                let val = rgb[k].clamp(0.0, 255.0);
                c.rgba[o + k] = if t.semi { ((c.rgba[o + k] as f32 + val) / 2.0) as u8 } else { val as u8 };
            }
        }
    }
}

/// Orthographic views of the rest pose (yaw degrees, pitch degrees).
fn render(m: &Mdl, v: &Vram, slots: &[Option<SlotAttr>; 32], views: &[(f32, f32)], cell: usize) -> Vec<u8> {
    let mut canvas = Canvas::new(cell * views.len(), cell);
    // model-space points
    let mut all = Vec::new();
    for (_, part) in m.meshes() {
        for poly in part.polys() {
            for vx in &poly.verts {
                all.push([0, 1, 2].map(|k| (vx.pos[k] as i32 + part.record.pivot[k]) as f32));
            }
        }
    }
    let r = all.iter().map(|p| (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt()).fold(1.0f32, f32::max);
    let c = [0, 1, 2].map(|k| all.iter().map(|p| p[k]).sum::<f32>() / all.len().max(1) as f32);
    let r = r * 1.05;
    for (vi, &(yaw, pitch)) in views.iter().enumerate() {
        let (sy, cy) = yaw.to_radians().sin_cos();
        let (sp, cp) = pitch.to_radians().sin_cos();
        let xf = |p: [f32; 3]| {
            let (x, y, z) = (p[0] - c[0], p[1] - c[1], p[2] - c[2]);
            let (x1, z1) = (x * cy - z * sy, x * sy + z * cy);
            let (y2, z2) = (y * cp - z1 * sp, y * sp + z1 * cp);
            let s = cell as f32 / (2.0 * r);
            [vi as f32 * cell as f32 + cell as f32 / 2.0 + x1 * s, cell as f32 / 2.0 + y2 * s, z2]
        };
        for (_, part) in m.meshes() {
            let flag = part.tex_flag().unwrap_or(0);
            let slot = slots[(flag & 0x1F) as usize];
            for poly in part.polys() {
                let pts: Vec<[f32; 3]> = poly
                    .verts
                    .iter()
                    .map(|vx| xf([0, 1, 2].map(|k| (vx.pos[k] as i32 + part.record.pivot[k]) as f32)))
                    .collect();
                // back-face rule of FUN_8001f040 on the GPU-order triangle 0,1,(2|3)
                let g = poly.gpu_order();
                let (a, b, d) = (pts[g[0]], pts[g[1]], pts[g[2]]);
                let ncl = (b[0] - a[0]) * (d[1] - a[1]) - (b[1] - a[1]) * (d[0] - a[0]);
                if ncl < 0.0 && !poly.double_sided() {
                    continue;
                }
                let tex = slot.map(|s| {
                    let (tp, cl) = mdl::tpage_clut(flag, s, poly);
                    let (px, py, d) = page_of(tp);
                    let (cx, cy) = vram::clut(cl);
                    (d, px, py, cx, cy)
                });
                let tris: &[[usize; 3]] = if poly.is_quad() { &[[0, 1, 3], [1, 2, 3]] } else { &[[0, 1, 2]] };
                for t in tris {
                    let tri = Tri {
                        p: t.map(|k| pts[k]),
                        uv: t.map(|k| [poly.verts[k].uv[0] as f32, poly.verts[k].uv[1] as f32]),
                        col: t.map(|k| poly.colors[k].map(|q| q as f32)),
                        tex,
                        semi: poly.semi_transparent(),
                    };
                    raster(&mut canvas, v, &tri);
                }
            }
        }
    }
    png_rgba(canvas.w as u32, canvas.h as u32, &canvas.rgba)
}

// -------------------------------------------------------------- text ----

fn describe(name: &str, m: &Mdl, file_len: usize) -> String {
    let mut s = format!("{name}: {file_len} bytes, table at 0x{:X} ({} parts)\n", m.table_hw * 2, m.parts.len());
    for (i, p) in m.parts.iter().enumerate() {
        let r = &p.record;
        s.push_str(&format!(
            "part {i}: data 0x{:X}+{} kind 0x{:X} pivot {:?} point_w {:?} points {:?} unk_28 {:?} data_off_hw {}\n",
            p.offset,
            r.size_hw * 2,
            r.kind,
            r.pivot,
            r.point_w,
            r.points,
            r.unk_28,
            r.data_offset_hw
        ));
        match &p.data {
            PartData::Mesh { polys, tail } => {
                let tris = polys.iter().filter(|q| !q.is_quad()).count();
                let mut flags = BTreeMap::new();
                for q in polys {
                    *flags.entry(q.tex_flag()).or_insert(0) += 1;
                }
                s.push_str(&format!(
                    "  mesh: {} polys ({} tris, {} quads), tail {} bytes, tex flags {:?}\n",
                    polys.len(),
                    tris,
                    polys.len() - tris,
                    tail.len(),
                    flags.iter().map(|(k, v)| format!("0x{k:02X}x{v}")).collect::<Vec<_>>()
                ));
                for q in polys {
                    s.push_str(&format!(
                        "    @0x{:04X} code 0x{:02X} flags {:02X?} col {:02X?} v {:?}\n",
                        q.offset,
                        q.code,
                        q.flags,
                        q.colors,
                        q.verts.iter().map(|v| (v.pos, v.uv)).collect::<Vec<_>>()
                    ));
                }
            }
            PartData::Ring(rg) => s.push_str(&format!(
                "  seam ring id {} (part {} ring {}), {} segments: {:?}\n",
                rg.id,
                rg.owner_part(),
                rg.ring_number(),
                rg.segments,
                rg.verts
            )),
            PartData::Other(b) => s.push_str(&format!("  other block {} bytes\n", b.len())),
        }
    }
    s
}

// -------------------------------------------------------------- main ----

fn check(name: &str, ok: bool, detail: String) -> bool {
    println!("{} {name}: {detail}", if ok { "PASS" } else { "FAIL" });
    ok
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut paths: Vec<String> = Vec::new();
    let mut raw_path = "COURSE_A/DOCK.RAW".to_string();
    let mut course = raw::DOCK_COURSE;
    let mut use_oracle = false;
    let mut port = 4370u16;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--raw" => {
                i += 1;
                raw_path = args[i].clone();
            }
            "--course" => {
                i += 1;
                course = args[i].parse().expect("--course N");
            }
            "--oracle" => use_oracle = true,
            "--port" => {
                i += 1;
                port = args[i].parse().expect("--port P");
            }
            p => paths.push(p.to_string()),
        }
        i += 1;
    }
    if paths.is_empty() {
        paths = vec!["CHARS_A/KERMIT.MDL".into(), "VEHIC_A/RACER.MDL".into()];
    }
    let out = repo_root().join("analysis").join("mdl");
    std::fs::create_dir_all(&out).unwrap();
    let mut disc = mr_disc::Disc::open_default().expect("open disc (MRM_DISC or game.toml)");

    // textures: the course .RAW through raw.rs
    let exe = disc.read("SLUS_012.37").expect("read exe");
    let env = LoadEnv::from_exe(&exe, course).expect("course table");
    let rawf = disc.read(&raw_path).expect("read .RAW");
    let dec = raw::decode(&rawf, &env).expect("decode .RAW");
    let mut vr = Vram::default();
    dec.apply(&mut vr);
    let mut slots: [Option<SlotAttr>; 32] = [None; 32];
    for (k, s) in dec.slots.iter().enumerate() {
        if let Some(s) = s {
            if let Some(tp) = s.tpage {
                slots[k] = Some(SlotAttr { tpage: tp, clut: s.clut });
            }
        }
    }

    let mut models = Vec::new();
    for p in &paths {
        let bytes = disc.read(p).unwrap_or_else(|e| panic!("{p}: {e:?}"));
        let m = Mdl::parse(&bytes).unwrap_or_else(|e| panic!("{p}: {e}"));
        let st = stem(p);
        let txt = describe(p, &m, bytes.len());
        std::fs::write(out.join(format!("{st}.txt")), &txt).unwrap();
        // textures and materials
        let mut mtl = String::new();
        let mut used = BTreeMap::new();
        for (_, part) in m.meshes() {
            if let Some(f) = part.tex_flag() {
                used.insert(f, ());
            }
        }
        for &f in used.keys() {
            let mname = format!("slot{:02X}", f);
            if let Some(slot) = slots[(f & 0x1F) as usize] {
                let png = format!("{st}_{mname}.png");
                std::fs::write(out.join(&png), page_png(&vr, slot, f)).unwrap();
                mtl.push_str(&format!("newmtl {mname}\nKd 1 1 1\nmap_Kd {png}\n\n"));
            } else {
                mtl.push_str(&format!("newmtl {mname}\nKd 0.8 0.8 0.8\n\n"));
            }
        }
        std::fs::write(out.join(format!("{st}.mtl")), &mtl).unwrap();
        let tex_flag_of: HashMap<usize, u8> = m.meshes().filter_map(|(i, p)| p.tex_flag().map(|f| (i, f))).collect();
        let obj = m.to_obj(&st, Some(&format!("{st}.mtl")), &|pi, _p: &Polygon| tex_flag_of.get(&pi).map(|f| format!("slot{:02X}", f)));
        std::fs::write(out.join(format!("{st}.obj")), obj).unwrap();
        let png = render(&m, &vr, &slots, &[(0.0, 15.0), (90.0, 15.0), (180.0, 15.0), (215.0, 30.0)], 320);
        std::fs::write(out.join(format!("{st}_preview.png")), png).unwrap();
        let nmesh = m.meshes().count();
        let npoly: usize = m.meshes().map(|(_, p)| p.polys().len()).sum();
        println!("{p}: {} parts ({nmesh} meshes, {} rings), {npoly} polygons -> analysis/mdl/{st}.obj", m.parts.len(), m.rings().count());
        models.push((p.clone(), bytes, m));
    }

    if !use_oracle {
        return;
    }
    let mut o = Oracle::connect(port).expect("connect to the recomp debug server");
    let mut all_ok = true;

    // 1. RAM model slots: find each model's slots by pivot + polygon count,
    //    compare the polygon list bytes at the slot pointer with the file.
    let slot_ram = o.ram(mdl::SLOT_TABLE_ADDR, mdl::SLOT_LEN * 200);
    let ram_slots: Vec<mdl::RamSlot> = (0..200).map(|k| mdl::RamSlot::parse(&slot_ram[k * mdl::SLOT_LEN..])).collect();
    let mut model_first_slot = HashMap::new();
    for (p, bytes, m) in &models {
        let meshes: Vec<_> = m.meshes().collect();
        let n = meshes.len();
        let found = (0..200 - n).find(|&s| {
            meshes.iter().enumerate().all(|(k, (_, part))| {
                let rs = &ram_slots[s + k];
                rs.pivot[0] == part.record.pivot[0] && rs.pivot[2] == part.record.pivot[2] && rs.poly_count as usize == part.polys().len()
            })
        });
        let Some(first) = found else {
            all_ok &= check(&format!("{p} RAM slots"), false, "no slot run matches pivots + counts".into());
            continue;
        };
        model_first_slot.insert(p.clone(), first);
        for (k, (pi, part)) in meshes.iter().enumerate() {
            let rs = ram_slots[first + k];
            let len: usize = part.polys().iter().map(|q| q.len_bytes()).sum::<usize>() + 4;
            let ram = o.ram(rs.polys_ptr, len);
            let file = &bytes[part.offset..part.offset + len];
            let same = ram == file;
            if same {
                all_ok &= check(
                    &format!("{p} part {pi} bytes == RAM slot {}", first + k),
                    true,
                    format!("{} bytes at 0x{:08X}, pivot {:?} kind {}", len, rs.polys_ptr, rs.pivot, rs.kind),
                );
            } else {
                // the seam pass reorders neck quads in place: compare as a multiset of vertex sets
                let ram_polys = mdl::parse_polys(&ram, 0, *pi).map(|x| x.0).unwrap_or_default();
                let key = |q: &Polygon| {
                    let mut v: Vec<[i16; 3]> = q.verts.iter().map(|v| v.pos).collect();
                    v.sort();
                    v
                };
                let mut a: Vec<_> = part.polys().iter().map(key).collect();
                let mut b: Vec<_> = ram_polys.iter().map(key).collect();
                a.sort();
                b.sort();
                let diff = ram.iter().zip(file).filter(|(x, y)| x != y).count();
                all_ok &= check(
                    &format!("{p} part {pi} (neck, kind {} in RAM) == RAM slot {} as a polygon set", rs.kind, first + k),
                    a == b,
                    format!("{diff} of {len} bytes reordered by FUN_800384b4/800398f8; vertex sets equal: {}", a == b),
                );
            }
        }
    }

    // 2. GTE: every mesh polygon of the frame's RTPT inputs; quads then RTPS v2.
    // A 30 Hz game frame spans two VBlanks (world on one, karts on the
    // next), so take the GTE work of the last few VBlanks together.
    let now = o.frame();
    let f = now - 2;
    let frames: Vec<i64> = (now - 7..=now - 2).collect();
    let mut ents: Vec<Json> = Vec::new();
    for &cand in &frames {
        let mut off = 0;
        loop {
            let r = o.q("gte_ring_dump", &format!("\"count\":4096,\"newest\":0,\"frame\":{cand},\"offset\":{off}"));
            let e = r.get("entries").map(|e| e.arr().to_vec()).unwrap_or_default();
            let n = e.len();
            ents.extend(e);
            if n < 4096 {
                break;
            }
            off += 4096;
        }
    }
    println!("GTE ring: {} entries from frames {}..={}", ents.len(), frames[0], f);
    ents.sort_by_key(|e| e.get("seq").and_then(|v| v.num()).unwrap_or(0.0) as i64);
    let v3 = |e: &Json, k: &str| -> [i64; 3] {
        let v = e.get(k).map(|v| v.ivec()).unwrap_or_default();
        [v.first().copied().unwrap_or(0), v.get(1).copied().unwrap_or(0), v.get(2).copied().unwrap_or(0)]
    };
    let cmd = |e: &Json| e.get("cmd").and_then(|c| c.hexnum()).unwrap_or(0);
    for (p, _, m) in &models {
        for (pi, part) in m.meshes() {
            if part.record.point_w[1] != 0 {
                continue; // neck: drawn from the rings
            }
            let mut hit = 0;
            let mut ra = BTreeMap::new();
            for q in part.polys() {
                let pv = |k: usize| q.verts[k].pos.map(|c| c as i64);
                let want = if q.is_quad() { [pv(0), pv(1), pv(3)] } else { [pv(0), pv(1), pv(2)] };
                let found = ents.iter().enumerate().find(|(i, e)| {
                    cmd(e) == 0x0028_0030
                        && [v3(e, "V0"), v3(e, "V1"), v3(e, "V2")] == want
                        && (!q.is_quad() || ents.get(i + 1).map_or(false, |n| cmd(n) == 0x0018_0001 && v3(n, "V0") == pv(2)))
                });
                if let Some((_, e)) = found {
                    hit += 1;
                    *ra.entry(e.get("ra").and_then(|r| r.str()).unwrap_or("?").to_string()).or_insert(0) += 1;
                }
            }
            all_ok &= check(
                &format!("{p} part {pi} polygons == GTE input (frame {f})"),
                hit == part.polys().len(),
                format!("{hit}/{} matched, ra {:?}", part.polys().len(), ra),
            );
        }
        for rg in m.rings() {
            let n = rg.verts.len();
            let mut hit = 0;
            for (k, (pos, _)) in rg.verts.iter().enumerate() {
                let pv = pos.map(|c| c as i64);
                if ents.iter().any(|e| cmd(e) == 0x0018_0001 && v3(e, "V0") == pv) {
                    hit += 1;
                }
                let _ = k;
            }
            all_ok &= check(&format!("{p} ring {} vertices == seam-renderer RTPS input", rg.id), hit == n, format!("{hit}/{n}"));
        }
    }

    // 3. VRAM: the decoded pages + CLUT rows the models use vs the game's VRAM.
    let mut regions = BTreeMap::new();
    for (_, _, m) in &models {
        for (_, part) in m.meshes() {
            let Some(flag) = part.tex_flag() else { continue };
            let Some(slot) = slots[(flag & 0x1F) as usize] else { continue };
            let (px, py, d) = page_of(slot.tpage);
            let w = match d {
                Depth::B4 => 64,
                Depth::B8 => 128,
                Depth::B15 => 256,
            };
            regions.insert((px, py, w, 256usize), "page");
            let (cx, cy) = vram::clut(slot.clut);
            regions.insert((cx, cy, 16 * 16, 1usize), "clut row");
        }
    }
    for ((x, y, w, h), what) in regions {
        let g = o.vram(x, y, w, h);
        let mut diff = 0;
        for j in 0..h {
            for i in 0..w {
                if g[j * w + i] != vr.get(x + i, y + j) {
                    diff += 1;
                }
            }
        }
        all_ok &= check(&format!("VRAM {what} ({x},{y}) {w}x{h} decoded from {raw_path} == game"), diff == 0, format!("{diff} differing pixels"));
    }

    // 4. GPU packets: tpage/clut of textured polys whose UVs match ours.
    let mut packets: Vec<Json> = Vec::new();
    for &cand in &frames {
        let rr = o.q("gpu_frame_dump", &format!("\"frame\":{cand},\"count\":65536"));
        packets.extend(rr.get("entries").map(|e| e.arr().to_vec()).unwrap_or_default());
    }
    println!("GPU ring: {} packets from frames {}..={}", packets.len(), frames[0], f);
    // key: vertex count + the first three UVs in packet order (the ring
    // keeps 12 words, so a GT4's fourth UV is cut off)
    let mut gp: HashMap<(usize, Vec<(u8, u8)>), Vec<(u16, u16)>> = HashMap::new();
    for e in &packets {
        let op = e.get("op").and_then(|v| v.hexnum()).unwrap_or(0);
        let w: Vec<u32> = e.get("w").map(|w| w.arr().iter().filter_map(|x| x.hexnum()).collect()).unwrap_or_default();
        let nv = match op & 0xFC {
            0x34 => 3,
            0x3C => 4,
            _ => continue,
        };
        if w.len() < 9 {
            continue;
        }
        let uv: Vec<(u8, u8)> = (0..3).map(|k| ((w[2 + 3 * k] & 0xFF) as u8, ((w[2 + 3 * k] >> 8) & 0xFF) as u8)).collect();
        gp.entry((nv, uv)).or_default().push(((w[5] >> 16) as u16, (w[2] >> 16) as u16));
    }
    for (p, _, m) in &models {
        let (mut seen, mut agree) = (0, 0);
        for (_, part) in m.meshes() {
            let Some(flag) = part.tex_flag() else { continue };
            let Some(slot) = slots[(flag & 0x1F) as usize] else { continue };
            for q in part.polys() {
                let uv: Vec<(u8, u8)> = q.gpu_order()[..3].iter().map(|&k| (q.verts[k].uv[0], q.verts[k].uv[1])).collect();
                if let Some(c) = gp.get(&(q.verts.len(), uv)) {
                    seen += 1;
                    let (tp, cl) = mdl::tpage_clut(flag, slot, q);
                    // the GPU tpage halfword carries extra bits from the draw environment
                    if c.iter().any(|&(gt, gc)| gc == cl && (gt & 0x1FF) == (tp & 0x1FF)) {
                        agree += 1;
                    }
                }
            }
        }
        all_ok &= check(
            &format!("{p} tpage/CLUT == GPU packets (frame {f})"),
            seen > 0 && agree == seen,
            format!("{agree}/{seen} drawn polygons with matching UVs agree"),
        );
    }
    let _ = model_first_slot;
    println!("{}", if all_ok { "ALL PASS" } else { "SOME CHECKS FAILED" });
}

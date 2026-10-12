//! mr-dat: decode a track `.DAT` from the owner's disc, export its placed
//! scenery as OBJ (+ flat-colour MTL) and a text summary, and (with
//! --oracle) prove the decode against the running recomp over its TCP debug
//! server, read-only commands only (read_ram, gte_ring_dump):
//!   1. reproduce the loader FUN_8002e3dc and compare the fixed-up file and
//!      its output records with the game's RAM byte for byte;
//!   2. check that every RTPS/RTPT vertex of a recent frame is a vertex of
//!      a decoded mesh;
//!   3. check that each scenery draw's GTE rotation and translation are the
//!      decoded object transform seen through one common camera.
//!
//! usage: mr-dat [DISC_PATH] [--oracle] [--port P]
//!   DISC_PATH defaults to COURSE_A/DOCK.DAT.
//! writes analysis/dat/<stem>.obj, <stem>.mtl, <stem>_summary.txt.

use mr_assets::dat::{self, ChunkBody, Dat, Layer, SinTable};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::PathBuf;

const DAT_HEAP_PTR: u32 = 0x800B_0EE8;

// ------------------------------------------------------------------ oracle

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
    fn ram(&mut self, addr: u32, n: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(n);
        while out.len() < n {
            let k = (n - out.len()).min(32768);
            let a = addr + out.len() as u32;
            let r = self.q("read_ram", &format!("\"addr\":\"0x{a:08X}\",\"len\":{k}"));
            let v = Json::parse(&r);
            let hex = v.get("hex").or_else(|| v.get("data")).and_then(|x| x.str()).unwrap_or_else(|| panic!("read_ram: {r}"));
            let hb = hex.as_bytes();
            out.extend((0..k).map(|i| {
                let h = |c: u8| (c as char).to_digit(16).unwrap() as u8;
                h(hb[2 * i]) * 16 + h(hb[2 * i + 1])
            }));
        }
        out
    }
}

// ------------------------------------------------------------ mini JSON

#[derive(Debug, Clone)]
enum Json {
    Null,
    Bool(#[allow(dead_code)] bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    fn parse(s: &str) -> Json {
        let b = s.as_bytes();
        let mut p = 0;
        Self::val(b, &mut p)
    }
    fn ws(b: &[u8], p: &mut usize) {
        while *p < b.len() && (b[*p] as char).is_whitespace() {
            *p += 1;
        }
    }
    fn val(b: &[u8], p: &mut usize) -> Json {
        Self::ws(b, p);
        match b[*p] {
            b'{' => {
                *p += 1;
                let mut kv = Vec::new();
                loop {
                    Self::ws(b, p);
                    if b[*p] == b'}' {
                        *p += 1;
                        break;
                    }
                    let k = match Self::val(b, p) {
                        Json::Str(s) => s,
                        _ => panic!("json key"),
                    };
                    Self::ws(b, p);
                    *p += 1; // ':'
                    let v = Self::val(b, p);
                    kv.push((k, v));
                    Self::ws(b, p);
                    if b[*p] == b',' {
                        *p += 1;
                    }
                }
                Json::Obj(kv)
            }
            b'[' => {
                *p += 1;
                let mut a = Vec::new();
                loop {
                    Self::ws(b, p);
                    if b[*p] == b']' {
                        *p += 1;
                        break;
                    }
                    a.push(Self::val(b, p));
                    Self::ws(b, p);
                    if b[*p] == b',' {
                        *p += 1;
                    }
                }
                Json::Arr(a)
            }
            b'"' => {
                *p += 1;
                let mut s = String::new();
                while b[*p] != b'"' {
                    if b[*p] == b'\\' {
                        *p += 1;
                    }
                    s.push(b[*p] as char);
                    *p += 1;
                }
                *p += 1;
                Json::Str(s)
            }
            b't' => {
                *p += 4;
                Json::Bool(true)
            }
            b'f' => {
                *p += 5;
                Json::Bool(false)
            }
            b'n' => {
                *p += 4;
                Json::Null
            }
            _ => {
                let st = *p;
                while *p < b.len() && matches!(b[*p], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9') {
                    *p += 1;
                }
                Json::Num(std::str::from_utf8(&b[st..*p]).unwrap().parse().unwrap())
            }
        }
    }
    fn get(&self, k: &str) -> Option<&Json> {
        match self {
            Json::Obj(kv) => kv.iter().find(|(a, _)| a == k).map(|(_, v)| v),
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
    fn ints(&self) -> Vec<i64> {
        self.arr().iter().filter_map(|x| x.num()).map(|x| x as i64).collect()
    }
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

// ------------------------------------------------------------ export

fn export_obj(d: &Dat, sin: &SinTable, stem: &str, obj: &mut String, mtl: &mut String) {
    let _ = writeln!(obj, "# {stem}.DAT scenery decoded by mr-dat (port/crates/mr-assets/src/dat.rs)");
    let _ = writeln!(obj, "# world units (layer 1 multiplied by 4); axes (x, -y, -z) so OBJ is y-up and");
    let _ = writeln!(obj, "# record order is counter-clockwise for front faces. Vertex colours are the");
    let _ = writeln!(obj, "# game's per-vertex modulation colour (/255; 0x80 = neutral).");
    let _ = writeln!(obj, "mtllib {stem}.mtl");
    let mut used_mtl: BTreeMap<String, [f32; 3]> = BTreeMap::new();
    let mut vbase = 1usize;
    let mut vtbase = 1usize;
    for (i, o) in d.objects.iter().enumerate() {
        let t = d.transform(o, sin);
        let Some(m) = d.mesh(o) else { continue };
        let _ = writeln!(obj, "o L{}_{:03}_mesh_{:05x}", o.layer as u8, i, m.offset);
        for v in &m.vertices {
            let w = t.apply_f(v.pos);
            let c = v.rgb();
            let _ = writeln!(
                obj,
                "v {:.2} {:.2} {:.2} {:.3} {:.3} {:.3}",
                w[0],
                -w[1],
                -w[2],
                c[0] as f32 / 255.0,
                c[1] as f32 / 255.0,
                c[2] as f32 / 255.0
            );
        }
        for g in &m.groups {
            let name = if g.is_textured() {
                format!("slot{:02}_{}", g.tex_slot(), if g.semi_transparent() { "semi" } else { "opaque" })
            } else {
                format!("untextured_{}", if g.semi_transparent() { "semi" } else { "opaque" })
            };
            let col = if g.is_textured() {
                let s = g.tex_slot() as u32;
                [((s * 97) % 200 + 55) as f32 / 255.0, ((s * 57) % 200 + 55) as f32 / 255.0, ((s * 151) % 200 + 55) as f32 / 255.0]
            } else {
                [0.6, 0.6, 0.6]
            };
            used_mtl.insert(name.clone(), col);
            let _ = writeln!(obj, "usemtl {name}");
            let n = if g.is_triangle() { 3 } else { 4 };
            for p in &g.prims {
                if g.is_textured() {
                    for c in 0..n {
                        let _ = writeln!(obj, "vt {:.5} {:.5}", p.uv[c][0] as f32 / 256.0, 1.0 - p.uv[c][1] as f32 / 256.0);
                    }
                }
                let mut f = String::from("f");
                for c in 0..n {
                    if g.is_textured() {
                        let _ = write!(f, " {}/{}", vbase + p.idx[c] as usize, vtbase + c);
                    } else {
                        let _ = write!(f, " {}", vbase + p.idx[c] as usize);
                    }
                }
                let _ = writeln!(obj, "{f}");
                if g.is_textured() {
                    vtbase += n;
                }
            }
        }
        vbase += m.vertices.len();
    }
    // Point chunks as polylines.
    for c in &d.chunks {
        if let ChunkBody::Points(pts) = &c.body {
            if pts.is_empty() {
                continue;
            }
            let _ = writeln!(obj, "o chunk_{:02x}_points", c.id);
            for p in pts {
                let _ = writeln!(obj, "v {} {} {}", p[0], -p[1], -p[2]);
            }
            let mut l = String::from("l");
            for k in 0..pts.len() {
                let _ = write!(l, " {}", vbase + k);
            }
            let _ = writeln!(obj, "{l}");
            vbase += pts.len();
        }
    }
    for (n, c) in used_mtl {
        let _ = writeln!(mtl, "newmtl {n}\nKd {:.3} {:.3} {:.3}\n", c[0], c[1], c[2]);
    }
}

fn summary(d: &Dat, sin: &SinTable, path: &str, raw: &[u8]) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "{path}: {} bytes", d.len);
    let _ = writeln!(s, "\nSections:");
    for sec in &d.sections {
        let _ = writeln!(s, "  0x{:06x}..0x{:06x} {:7} {}", sec.start, sec.end, sec.end - sec.start, sec.name);
    }
    let gaps = d.gaps();
    let _ = writeln!(s, "  unaccounted ranges: {}", if gaps.is_empty() { "none".to_string() } else { format!("{gaps:x?}") });
    let mesh_pad: usize = d
        .meshes
        .values()
        .zip(d.meshes.values().skip(1))
        .map(|(a, b)| b.offset - a.end)
        .sum();
    let _ = writeln!(s, "  bytes between consecutive meshes: {mesh_pad}");

    let _ = writeln!(s, "\nChunks ({}):", d.chunks.len());
    for c in &d.chunks {
        let what = match &c.body {
            ChunkBody::Points(p) => {
                let mut mn = [i32::MAX; 3];
                let mut mx = [i32::MIN; 3];
                for q in p {
                    for k in 0..3 {
                        mn[k] = mn[k].min(q[k]);
                        mx[k] = mx[k].max(q[k]);
                    }
                }
                format!("{} points, bbox {:?}..{:?}", p.len(), mn, mx)
            }
            ChunkBody::Records16(r) => format!("{} 16-byte records", r.len()),
            ChunkBody::Raw(r) => format!("{} raw bytes", r.len()),
        };
        let _ = writeln!(s, "  @0x{:05x} id 0x{:02x}{}: {what}", c.offset, c.id, if c.id == dat::CHUNK_VIS_PATH { " (vis path)" } else { "" });
    }
    let _ = writeln!(s, "block128 entries: {}", d.block128.len());

    for l in [Layer::World, Layer::Quarter] {
        let objs = d.layer(l);
        let mut fl: BTreeMap<u8, usize> = BTreeMap::new();
        let mut mn = [i64::MAX; 3];
        let mut mx = [i64::MIN; 3];
        for o in objs {
            *fl.entry(o.flags).or_default() += 1;
            let t = d.transform(o, sin);
            for v in d.mesh(o).map(|m| m.vertices.as_slice()).unwrap_or(&[]) {
                let w = t.apply(v.pos);
                for k in 0..3 {
                    mn[k] = mn[k].min(w[k] as i64);
                    mx[k] = mx[k].max(w[k] as i64);
                }
            }
        }
        let _ = writeln!(s, "\nLayer {} ({:?}, x{}): {} objects; flags {:02x?}", l as u8, l, l.world_scale(), objs.len(), fl);
        let _ = writeln!(s, "  world bbox of vertices: {mn:?} .. {mx:?}");
    }
    let sp: Vec<String> = d.special.iter().enumerate().filter_map(|(i, x)| x.map(|k| format!("{i}->obj{k}"))).collect();
    let _ = writeln!(s, "\nSpecial slots: {} ({} used): {}", d.special.len(), sp.len(), sp.join(" "));

    let ndesc_scaled = d.descriptors.values().filter(|x| x.scale.is_some()).count();
    let nrot = d.descriptors.values().filter(|x| x.angles != [0, 0, 0]).count();
    let nscale = d.descriptors.values().filter(|x| x.scale.map_or(false, |s| s != [4096; 3])).count();
    let _ = writeln!(
        s,
        "Descriptors: {} ({} scaled layout, {} with non-unit scale, {} rotated, {} with 2nd mesh)",
        d.descriptors.len(),
        ndesc_scaled,
        nscale,
        nrot,
        d.descriptors.values().filter(|x| x.mesh2.is_some()).count()
    );
    let shared = d.meshes.len();
    let _ = writeln!(s, "Meshes: {shared}; kind-2/3 parts models: {} (objects: {})", d.parts.len(), d.objects.len());
    let nv: usize = d.meshes.values().map(|m| m.vertices.len()).sum();
    let np: usize = d.meshes.values().map(|m| m.prim_count()).sum();
    let neg = d.meshes.values().filter(|m| m.raw_count < 0).count();
    let _ = writeln!(s, "  {nv} vertices, {np} primitives, {neg} meshes with 12-byte vertices");
    let mut kinds: BTreeMap<u8, usize> = BTreeMap::new();
    let mut slots: BTreeMap<u8, usize> = BTreeMap::new();
    let mut hdr_hi: BTreeMap<u16, usize> = BTreeMap::new();
    let mut blend: BTreeMap<u8, usize> = BTreeMap::new();
    for m in d.meshes.values() {
        for g in &m.groups {
            *kinds.entry(g.kind()).or_default() += g.prims.len();
            if g.is_textured() {
                *slots.entry(g.tex_slot()).or_default() += g.prims.len();
            }
            *hdr_hi.entry(g.header & 0xE080).or_default() += 1;
            *blend.entry(g.blend_bits()).or_default() += g.prims.len();
        }
    }
    let _ = writeln!(s, "  primitives by kind (bit0 tri, bit1 2-sided, bit2 cull-both, bit4 untextured):");
    for (k, n) in &kinds {
        let g = dat::PrimGroup { offset: 0, header: *k as u16, prims: vec![] };
        let _ = writeln!(
            s,
            "    kind 0x{k:02x}: {n:5}  {} {}{}",
            if g.is_textured() { if g.is_triangle() { "GT3" } else { "GT4" } } else if g.is_triangle() { "G3" } else { "G4" },
            if g.double_sided() { "double-sided" } else { "back-face culled" },
            if g.cull_both_halves() && !g.double_sided() && !g.is_triangle() { " (both halves)" } else { "" }
        );
    }
    let _ = writeln!(s, "  textured primitives by texture slot: {slots:?}");
    let _ = writeln!(s, "  primitives by blend bits (3 = opaque): {blend:?}");
    let _ = writeln!(s, "  group headers by bits 7/13-15 (0x8000 = keep previous group texture state): {hdr_hi:x?}");
    let mut v_unused = 0;
    let mut tri_b3 = 0;
    for m in d.meshes.values() {
        for g in &m.groups {
            if g.is_triangle() && g.is_textured() {
                for p in &g.prims {
                    if p.raw[3] != 0 || p.raw[4] != 0 || p.raw[5] != 0 {
                        tri_b3 += 1;
                    }
                }
            }
            if !g.is_textured() && g.is_triangle() {
                v_unused += g.prims.iter().filter(|p| p.raw[3] != 0).count();
            }
        }
    }
    let _ = writeln!(s, "  textured triangles with nonzero bytes 3..6: {tri_b3}; untextured triangles with nonzero byte 3: {v_unused}");

    for (zi, z) in d.vis.iter().enumerate() {
        let seps = z.separators();
        let _ = writeln!(
            s,
            "\nVisibility zone {zi}: chunk #{} @0x{:x} len {} bbox {:?}..{:?} size2 {}; {} nodes (separators), stream {} bytes",
            z.chunk,
            z.offset,
            z.len,
            z.bbox_min,
            z.bbox_max,
            z.size2,
            seps.len(),
            z.stream.len()
        );
    }
    let _ = writeln!(s, "\nLoader output size (FUN_8002e3dc return): {} bytes", d.loader_output_size());
    let _ = raw;
    s
}

// ------------------------------------------------------------ oracle checks

fn check_ram(o: &mut Oracle, d: &Dat, raw: &[u8], sin: &SinTable, rep: &mut String) -> bool {
    let base = u32::from_le_bytes(o.ram(DAT_HEAP_PTR, 4).try_into().unwrap());
    let _ = writeln!(rep, "RAM: DAT_800b0ee8 = 0x{base:08x}");
    if !(0x8001_0000..0x8020_0000).contains(&base) || o.ram(base, 64) != raw[..64] {
        let _ = writeln!(rep, "  this DAT is not loaded at that pointer (game not in a race on this track); RAM check skipped");
        return false;
    }
    let img = d.loader_image(raw, base, sin);
    let ram_file = o.ram(base, raw.len());
    let ram_out = o.ram(base + raw.len() as u32, img.out.len());
    // Objects in special slots are driven by game code after loading (their
    // centre and descriptor change); judge them separately.
    let live: BTreeSet<usize> = d.special.iter().flatten().copied().collect();
    let owner = |i: usize| -> Option<usize> {
        d.objects.iter().position(|o| i >= o.offset && i < o.offset + 20).or_else(|| {
            d.objects.iter().position(|o| {
                let x = d.desc(o);
                i >= x.offset && i < x.offset + x.size()
            })
        })
    };
    let disc_diff = (0..raw.len()).filter(|&i| ram_file[i] != raw[i]).count();
    let fdiff: Vec<usize> = (0..raw.len()).filter(|&i| ram_file[i] != img.file[i]).collect();
    let (mut f_live, mut f_bad) = (0usize, Vec::new());
    for &i in &fdiff {
        match owner(i) {
            Some(k) if live.contains(&k) => f_live += 1,
            _ => f_bad.push(i),
        }
    }
    let _ = writeln!(
        rep,
        "  file image: {disc_diff} bytes differ disc vs RAM (loader fix-ups); predicted vs RAM: {} differ, {} of them in the {} special-slot objects (moved at run time), {} elsewhere",
        fdiff.len(),
        f_live,
        live.len(),
        f_bad.len()
    );
    for &i in f_bad.iter().take(10) {
        let _ = writeln!(rep, "    @0x{i:x}: predicted {:02x} ram {:02x}", img.file[i], ram_file[i]);
    }
    // Object records (40 bytes): matrix, descriptor pointer, slot count are
    // load-time values; +0x18..0x1f are zeroed by the loader and then used
    // as run-time state (as are layer-1 +0x26/0x27), so they are not compared.
    let nsp = d.special.len();
    let (mut rec_ok, mut rec_live, mut rec_bad) = (0usize, 0usize, Vec::new());
    let mut cmp_bytes = 0usize;
    for i in 0..d.objects.len() {
        let ro = 0x1C * nsp + 0x28 * i;
        let ks: Vec<usize> = (ro..ro + 0x28).filter(|&k| img.defined[k] && !(0x18..0x20).contains(&(k - ro)) && !(0x26..0x28).contains(&(k - ro))).collect();
        let bad = ks.iter().any(|&k| img.out[k] != ram_out[k]);
        if live.contains(&i) {
            rec_live += 1;
        } else {
            cmp_bytes += ks.len();
            if bad {
                rec_bad.push(i);
            } else {
                rec_ok += 1;
            }
        }
    }
    let _ = writeln!(
        rep,
        "  object records @0x{:08x}: {} of {} static objects exact ({} bytes compared: matrix, descriptor ptr, slot count); {} special-slot objects skipped",
        base + raw.len() as u32 + 0x1C * nsp as u32,
        rec_ok,
        d.objects.len() - rec_live,
        cmp_bytes,
        rec_live
    );
    for &i in rec_bad.iter().take(10) {
        let ro = 0x1C * nsp + 0x28 * i;
        let which: Vec<String> = (ro..ro + 0x28)
            .filter(|&k| img.defined[k] && img.out[k] != ram_out[k])
            .map(|k| format!("+{:02x}", k - ro))
            .collect();
        let _ = writeln!(rep, "    obj {i}: bytes {} differ", which.join(","));
    }
    let mut sp_diff: BTreeSet<usize> = BTreeSet::new();
    for i in 0..nsp {
        let ro = 0x1C * i;
        for k in ro..ro + 0x1C {
            if img.defined[k] && img.out[k] != ram_out[k] {
                sp_diff.insert(k - ro);
            }
        }
    }
    let empty_ok = (0..nsp).filter(|&i| d.special[i].is_none()).all(|i| {
        let _ = i;
        true
    });
    let _ = writeln!(
        rep,
        "  special (pose) records @0x{:08x} ({} slots, {} used): {}",
        base + raw.len() as u32,
        nsp,
        live.len(),
        if sp_diff.is_empty() { "exact".to_string() } else { format!("differing byte offsets {sp_diff:x?}") }
    );
    let _ = empty_ok;
    f_bad.is_empty() && rec_bad.is_empty() && sp_diff.is_empty()
}

struct GteEntry {
    ra: String,
    verts: Vec<[i16; 3]>,
    rt: [i64; 9],
    tr: [i64; 3],
}

fn parse_entries(r: &Json) -> Vec<GteEntry> {
    let mut entries = Vec::new();
    for e in r.get("entries").map(|x| x.arr()).unwrap_or(&[]) {
        let cmd = e.get("cmd").and_then(|x| x.str()).unwrap_or("");
        let cmdv = u32::from_str_radix(cmd.trim_start_matches("0x"), 16).unwrap_or(0);
        let n = if cmdv & 0x3F == 0x30 { 3 } else { 1 };
        let mut verts = Vec::new();
        for k in 0..n {
            let v = e.get(&format!("V{k}")).map(|x| x.ints()).unwrap_or_default();
            if v.len() == 3 {
                verts.push([v[0] as i16, v[1] as i16, v[2] as i16]);
            }
        }
        let rt = e.get("RT").map(|x| x.ints()).unwrap_or_default();
        let tr = e.get("TR").map(|x| x.ints()).unwrap_or_default();
        if rt.len() != 9 || tr.len() != 3 {
            continue;
        }
        entries.push(GteEntry {
            ra: e.get("ra").and_then(|x| x.str()).unwrap_or("").to_string(),
            verts,
            rt: rt.try_into().unwrap(),
            tr: tr.try_into().unwrap(),
        });
    }
    entries
}

fn check_gte(o: &mut Oracle, d: &Dat, sin: &SinTable, rep: &mut String) {
    // Vertex index over all meshes.
    let mut vidx: HashMap<[i16; 3], HashSet<u32>> = HashMap::new();
    for (&off, m) in &d.meshes {
        for v in &m.vertices {
            vidx.entry(v.pos).or_default().insert(off);
        }
    }
    // Frames in the ring, most populated complete ones first; take the first
    // that has scenery draws (vertices found in decoded meshes).
    let r = Json::parse(&o.q("gte_ring_dump", "\"count\":4096"));
    let mut per: BTreeMap<i64, usize> = BTreeMap::new();
    for e in r.get("entries").map(|x| x.arr()).unwrap_or(&[]) {
        *per.entry(e.get("frame").and_then(|x| x.num()).unwrap_or(-1.0) as i64).or_default() += 1;
    }
    let (oldest, newest) = (per.keys().next().copied().unwrap_or(-1), per.keys().last().copied().unwrap_or(-1));
    let mut frames: Vec<(i64, usize)> = per.iter().filter(|(f, _)| **f != newest && **f != oldest).map(|(f, n)| (*f, *n)).collect();
    frames.sort_by_key(|x| std::cmp::Reverse(x.1));
    if frames.is_empty() {
        frames.push((newest, 0));
    }
    let mut chosen = None;
    for &(frame, _) in frames.iter().take(4) {
        let r = Json::parse(&o.q("gte_ring_dump", &format!("\"count\":4096,\"frame\":{frame}")));
        let es = parse_entries(&r);
        let hits = es.iter().filter(|e| e.verts.iter().all(|v| vidx.contains_key(v)) && !e.verts.is_empty()).count();
        if hits * 4 > es.len() {
            chosen = Some((frame, es));
            break;
        }
    }
    let Some((frame, entries)) = chosen else {
        let _ = writeln!(rep, "GTE ring: no recent frame draws DAT scenery (not in a race?)");
        return;
    };
    let cands = |e: &GteEntry| -> HashSet<u32> {
        let mut s: Option<HashSet<u32>> = None;
        for v in &e.verts {
            let c = vidx.get(v).cloned().unwrap_or_default();
            s = Some(match s {
                None => c,
                Some(p) => p.intersection(&c).copied().collect(),
            });
        }
        s.unwrap_or_default()
    };
    let mut by_ra: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for e in &entries {
        let ok = !cands(e).is_empty();
        let x = by_ra.entry(e.ra.clone()).or_default();
        x.0 += 1;
        if ok {
            x.1 += 1;
        }
    }
    let _ = writeln!(rep, "GTE ring, frame {frame}: {} RTPS/RTPT entries", entries.len());
    let _ = writeln!(rep, "  per caller (ra): entries, of which all input vertices lie in one decoded mesh");
    for (ra, (n, k)) in &by_ra {
        let _ = writeln!(rep, "    {ra}: {n:5} {k:5}");
    }
    // Scenery renderers: callers where most entries matched.
    let scen: HashSet<String> = by_ra.iter().filter(|(_, (n, k))| *k * 2 > *n).map(|(r, _)| r.clone()).collect();
    // Group draws by (RT, TR).
    let mut groups: BTreeMap<([i64; 9], [i64; 3]), Vec<&GteEntry>> = BTreeMap::new();
    for e in entries.iter().filter(|e| scen.contains(&e.ra)) {
        groups.entry((e.rt, e.tr)).or_default().push(e);
    }
    let mut rtc: HashMap<[i64; 9], usize> = HashMap::new();
    for (k, v) in &groups {
        *rtc.entry(k.0).or_default() += v.len();
    }
    let Some((&cam, _)) = rtc.iter().max_by_key(|(_, n)| **n) else {
        let _ = writeln!(rep, "  no scenery draws in this frame");
        return;
    };
    let _ = writeln!(rep, "  camera rotation (most common RT, identity objects): {cam:?}");
    let mut by_mesh: HashMap<u32, Vec<usize>> = HashMap::new();
    for (i, ob) in d.objects.iter().enumerate() {
        by_mesh.entry(d.desc(ob).mesh).or_default().push(i);
    }
    // Per GTE entry: objects whose mesh holds all its input vertices and whose
    // matrix gives RT = CAM * M (+-6); each implies a camera term
    // e = scale * (TR - CAM * origin / 4096) in world units. The decode is
    // right when one e explains (nearly) every entry.
    let mut per_entry: Vec<Vec<(usize, [f64; 3])>> = Vec::new();
    let mut tcache: HashMap<usize, dat::Transform> = HashMap::new();
    for e in entries.iter().filter(|e| scen.contains(&e.ra)) {
        let mut out = Vec::new();
        for m in cands(e) {
            for &oi in by_mesh.get(&m).map(|v| v.as_slice()).unwrap_or(&[]) {
                let t = *tcache.entry(oi).or_insert_with(|| d.transform(&d.objects[oi], sin));
                let mut ok = true;
                for r in 0..3 {
                    for col in 0..3 {
                        let p: i64 = (0..3).map(|k| cam[3 * r + k] * t.m[k][col] as i64).sum::<i64>() >> 12;
                        if (p - e.rt[3 * r + col]).abs() > 6 {
                            ok = false;
                        }
                    }
                }
                if !ok {
                    continue;
                }
                let mut e3 = [0f64; 3];
                for r in 0..3 {
                    let co: f64 = (0..3).map(|k| cam[3 * r + k] as f64 * t.origin[k] as f64).sum::<f64>() / 4096.0;
                    e3[r] = (e.tr[r] as f64 - co) * t.scale as f64;
                }
                out.push((oi, e3));
            }
        }
        per_entry.push(out);
    }
    // Reference camera term: the most populated 32-unit bucket.
    let mut buckets: HashMap<[i64; 3], Vec<[f64; 3]>> = HashMap::new();
    for c in &per_entry {
        for (_, e) in c {
            buckets.entry(e.map(|x| (x / 32.0).round() as i64)).or_default().push(*e);
        }
    }
    let Some(best) = buckets.values().max_by_key(|v| v.len()) else {
        let _ = writeln!(rep, "  no candidate objects for scenery draws");
        return;
    };
    let mut eref = [0f64; 3];
    for k in 0..3 {
        eref[k] = best.iter().map(|e| e[k]).sum::<f64>() / best.len() as f64;
    }
    let (mut ok_e, mut maxerr) = (0usize, 0f64);
    let mut layers = [0usize; 2];
    let mut objs_seen: HashSet<usize> = HashSet::new();
    let mut unexplained: BTreeMap<String, usize> = BTreeMap::new();
    let scen_entries: Vec<&GteEntry> = entries.iter().filter(|e| scen.contains(&e.ra)).collect();
    for (c, e) in per_entry.iter().zip(&scen_entries) {
        let best = c
            .iter()
            .map(|(oi, x)| (*oi, (0..3).map(|k| (x[k] - eref[k]).abs()).fold(0f64, f64::max)))
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        match best {
            Some((oi, err)) if err <= 16.0 => {
                ok_e += 1;
                maxerr = maxerr.max(err);
                layers[d.objects[oi].layer as usize] += 1;
                objs_seen.insert(oi);
            }
            _ => *unexplained.entry(e.ra.clone()).or_default() += 1,
        }
    }
    let _ = writeln!(
        rep,
        "  scenery entries explained by decoded transform + one camera: {} of {} (layer0/layer1 {:?}, {} distinct objects)",
        ok_e,
        per_entry.len(),
        layers,
        objs_seen.len()
    );
    let _ = writeln!(rep, "  camera term (world units) {:?}, max deviation {:.1}", eref.map(|x| x.round()), maxerr);
    if !unexplained.is_empty() {
        let _ = writeln!(rep, "  unexplained entries by caller: {unexplained:?} (input vertices not in any DAT mesh: other geometry drawn by the same routine)");
    }
}

/// Expected GP0 words (positions zeroed) of every decoded primitive, built
/// with the game's texture-slot table, then matched against the GP0 packets
/// of one frame: proves vertex order, colour expansion, UV order and the
/// tpage/CLUT rule of FUN_8001a304.
fn check_gpu(o: &mut Oracle, d: &Dat, rep: &mut String) {
    let tab = o.ram(0x800D_FF00, 32 * 16);
    let slot = |i: u8| {
        let b = i as usize * 16;
        dat::TexSlot {
            tpage: u16::from_le_bytes([tab[b], tab[b + 1]]),
            clut: u16::from_le_bytes([tab[b + 2], tab[b + 3]]),
            fixed_clut: u16::from_le_bytes([tab[b + 8], tab[b + 9]]),
        }
    };
    let rgb = |c: u16| -> u32 { ((c as u32 & 0x1F) << 19) | ((c as u32 & 0x3E0) << 6) | ((c as u32 & 0x7C00) >> 7) };
    let mut want: HashMap<Vec<u32>, usize> = HashMap::new();
    let all_meshes = d.meshes.values().chain(d.orphans.iter().filter_map(|x| x.mesh.as_ref()));
    for m in all_meshes {
        for g in &m.groups {
            let code = g.gpu_code() as u32;
            for p in &g.prims {
                let c = |k: usize| rgb(m.vertices[p.idx[k] as usize].color);
                // Layer-0 renderer (FUN_80012620 family) halves or quarters
                // the UVs of slots 0..3 by depth (texture LOD); the CLUT is
                // chosen from the full-size UV. [CODE 0x80016b3c]
                let lods: &[u32] = if g.is_textured() && g.tex_slot() < 4 { &[0, 1, 2] } else { &[0] };
                for &lod in lods {
                    let uv = |k: usize| ((p.uv[k][0] as u32) >> lod) | ((p.uv[k][1] as u32) >> lod) << 8;
                    let w: Vec<u32> = if g.is_textured() {
                        let (tp, cl) = dat::prim_tpage_clut(slot(g.tex_slot()), g, p);
                        if g.is_triangle() {
                            vec![code << 24 | c(0), 0, (cl as u32) << 16 | uv(0), c(1), 0, (tp as u32) << 16 | uv(1), c(2), 0, uv(2)]
                        } else {
                            vec![code << 24 | c(0), 0, (cl as u32) << 16 | uv(0), c(1), 0, (tp as u32) << 16 | uv(1), c(3), 0, uv(3), c(2), 0, uv(2)]
                        }
                    } else if g.is_triangle() {
                        vec![code << 24 | c(0), 0, c(1), 0, c(2), 0]
                    } else {
                        vec![code << 24 | c(0), 0, c(1), 0, c(3), 0, c(2), 0]
                    };
                    *want.entry(w).or_default() += 1;
                }
            }
        }
    }
    // The game renders every other VBlank: take the fullest of 5 recent
    // GP0 frames; its primitives were transformed one or two frames earlier.
    let now = Json::parse(&o.q("frame", "")).get("frame").and_then(|x| x.num()).unwrap_or(0.0) as i64;
    let mut best: Option<(i64, Json)> = None;
    for f in (now - 5)..now {
        let r = Json::parse(&o.q("gpu_frame_dump", &format!("\"frame\":{f},\"count\":16384")));
        let n = r.get("entries").map(|x| x.arr().len()).unwrap_or(0);
        if best.as_ref().map_or(true, |(_, b)| n > b.get("entries").map(|x| x.arr().len()).unwrap_or(0)) {
            best = Some((f, r));
        }
    }
    let (f, r) = best.unwrap();
    // Screen points produced from DAT mesh vertices (GTE ring, frames f-1, f-2).
    let mut vset: HashSet<[i16; 3]> = HashSet::new();
    for m in d.meshes.values() {
        for v in &m.vertices {
            vset.insert(v.pos);
        }
    }
    let mut dat_xy: HashSet<u32> = HashSet::new();
    for gf in [f - 1, f - 2] {
        let g = Json::parse(&o.q("gte_ring_dump", &format!("\"count\":4096,\"frame\":{gf}")));
        for e in g.get("entries").map(|x| x.arr()).unwrap_or(&[]) {
            let cmd = e.get("cmd").and_then(|x| x.str()).unwrap_or("");
            let rtpt = cmd.ends_with("30");
            let ks: &[&str] = if rtpt { &["V0", "V1", "V2"] } else { &["V0"] };
            let all = ks.iter().all(|k| {
                let v = e.get(k).map(|x| x.ints()).unwrap_or_default();
                v.len() == 3 && vset.contains(&[v[0] as i16, v[1] as i16, v[2] as i16])
            });
            if !all {
                continue;
            }
            let ss: &[&str] = if rtpt { &["S0", "S1", "S2"] } else { &["S2"] };
            for k in ss {
                let sv = e.get(k).map(|x| x.ints()).unwrap_or_default();
                if sv.len() == 2 {
                    dat_xy.insert(((sv[1] as u32 & 0xFFFF) << 16) | (sv[0] as u32 & 0xFFFF));
                }
            }
        }
    }
    // (op, from DAT vertices) -> (packets, exact, subdivided, other mismatch)
    let mut by_op: BTreeMap<(u8, bool), [usize; 4]> = BTreeMap::new();
    for e in r.get("entries").map(|x| x.arr()).unwrap_or(&[]) {
        let op = u8::from_str_radix(e.get("op").and_then(|x| x.str()).unwrap_or("0x0").trim_start_matches("0x"), 16).unwrap_or(0);
        let (n, xy, colour_words, last_uvs): (usize, &[usize], &[usize], &[usize]) = match op & 0xFE {
            0x3C => (12, &[1, 4, 7, 10], &[3, 6, 9], &[8, 11]),
            0x34 => (9, &[1, 4, 7], &[3, 6], &[8]),
            0x38 => (8, &[1, 3, 5, 7], &[2, 4, 6], &[]),
            0x30 => (6, &[1, 3, 5], &[2, 4], &[]),
            _ => continue,
        };
        let mut w: Vec<u32> = e
            .get("w")
            .map(|x| x.arr())
            .unwrap_or(&[])
            .iter()
            .filter_map(|x| x.str())
            .map(|x| u32::from_str_radix(x.trim_start_matches("0x"), 16).unwrap_or(0))
            .collect();
        if w.len() < n {
            continue;
        }
        w.truncate(n);
        let from_dat = xy.iter().any(|&k| dat_xy.contains(&w[k]));
        for &k in xy {
            w[k] = 0;
        }
        // Colour words after the first carry junk in their top byte; the
        // last UV words carry renderer state in their top half.
        for &k in colour_words {
            w[k] &= 0xFF_FFFF;
        }
        for &k in last_uvs {
            w[k] &= 0xFFFF;
        }
        // Subdivided/clipped pieces have interpolated colours (components
        // not multiples of 8; the mesh's 5-bit colours always are).
        let interp = std::iter::once(0usize).chain(colour_words.iter().copied()).any(|k| (w[k] & 0x07_0707) != 0);
        let x = by_op.entry((op & 0xFE, from_dat)).or_default();
        x[0] += 1;
        if want.contains_key(&w) {
            x[1] += 1;
        } else if interp {
            x[2] += 1;
        } else {
            x[3] += 1;
        }
    }
    let _ = writeln!(rep, "GPU frame {f}: polygon packets vs decoded DAT primitives (colours, UVs, CLUT, tpage all equal):");
    let _ = writeln!(rep, "    type  source                 packets  exact  interpolated(subdivided)  other");
    for ((op, dat), c) in by_op {
        let name = match op {
            0x3C => "GT4",
            0x34 => "GT3",
            0x38 => "G4",
            _ => "G3",
        };
        let src = if dat { "DAT vertices (GTE)" } else { "other geometry" };
        let _ = writeln!(rep, "    {name:4}  {src:22} {:7} {:6} {:25} {:6}", c[0], c[1], c[2], c[3]);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut path = "COURSE_A/DOCK.DAT".to_string();
    let mut use_oracle = false;
    let mut port = 4370u16;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--oracle" => use_oracle = true,
            "--port" => {
                i += 1;
                port = args[i].parse().expect("--port P");
            }
            p => path = p.to_string(),
        }
        i += 1;
    }
    let mut disc = mr_disc::Disc::open_default().expect("open disc (MRM_DISC or game.toml)");
    let raw = disc.read(&path).expect("read file");
    let exe = disc.read("SLUS_012.37").expect("read exe");
    let sin = SinTable::from_exe(&exe).expect("sine table");
    let d = Dat::parse(&raw).expect("parse");
    let stem = path.rsplit('/').next().unwrap().split('.').next().unwrap().to_ascii_lowercase();
    let out = repo_root().join("analysis").join("dat");
    std::fs::create_dir_all(&out).unwrap();

    let mut sum = summary(&d, &sin, &path, &raw);
    let (mut obj, mut mtl) = (String::new(), String::new());
    export_obj(&d, &sin, &stem, &mut obj, &mut mtl);
    std::fs::write(out.join(format!("{stem}.obj")), &obj).unwrap();
    std::fs::write(out.join(format!("{stem}.mtl")), &mtl).unwrap();

    if use_oracle {
        let mut o = Oracle::connect(port).expect("connect to recomp debug server");
        let mut rep = String::from("\n== Oracle (running recomp) ==\n");
        let ok = check_ram(&mut o, &d, &raw, &sin, &mut rep);
        let _ = writeln!(rep, "  RAM: loader reproduction exact for all static objects: {ok}");
        check_gte(&mut o, &d, &sin, &mut rep);
        check_gpu(&mut o, &d, &mut rep);
        sum.push_str(&rep);
    }
    std::fs::write(out.join(format!("{stem}_summary.txt")), &sum).unwrap();
    print!("{sum}");
    println!("\nwrote {}", out.join(format!("{stem}.obj")).display());
}

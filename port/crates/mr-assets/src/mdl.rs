//! mdl: Muppet RaceMania `.MDL` models (`CHARS_A/*.MDL`, `CHARS_B/*.MDL`,
//! `VEHIC_A/*.MDL`, `VEHIC_B/*.MDL`, `BOSS/*.MDL`).
//!
//! A model is a list of parts. Each part is either a rigid polygon mesh in
//! its own local frame (placed at `pivot` in model space) or a shared
//! "seam ring" block: a closed loop of vertices that a neck part is
//! stretched between (head ring and body ring). Polygons are stored almost
//! as GPU packets: `[r g b code]`, then per vertex `s16 x,y,z, u8 u,v`, then
//! one `[r g b flag]` word per further vertex. Texture page and CLUT are not
//! in the polygon: the high byte of the last colour word of the part's
//! first polygon selects a slot of the course texture table at 0x800dff00
//! (filled by the `.RAW` loader, see `raw.rs`), and each polygon picks one
//! of 16 CLUTs from the 64x64 cell its vertex 0 `(u, v)` lies in.
//!
//! Behaviour reproduced: FUN_8003c270 (load `<name>.mdl`) and FUN_8003bf58
//! (part table walk, slot setup, ring linking), FUN_8001f040 / FUN_8001ec2c
//! (polygon emitters: vertex order, tpage/CLUT derivation, back-face rule).
//! Not reproduced (runtime only): FUN_800384b4 / FUN_800398f8, which
//! reorder the neck part's quads in RAM to line up with the rings.
//!
//! Verified against the running recomp (DOCK, 1-player race): KERMIT and
//! RACER polygon bytes equal RAM at the model slot pointers (0x800bc668 +
//! 0x6c*slot, +0x20); every KERMIT/RACER rigid polygon equals a GTE RTPT
//! input of the frame (quads as v0,v1,v3 then RTPS v2); ring vertices equal
//! the RTPS inputs of the seam renderer; tpage/CLUT equal the GPU packets.
//! Spec and numbers: docs/spec/formats/MDL.md.

use crate::le;

/// File value of the first word of a ring block; the loader replaces it
/// with a link to the previous ring of the same kind [CODE 8003bf58].
pub const RING_MARKER: u32 = 0x1234_5678;
/// Size of one part record in the table.
pub const RECORD_LEN: usize = 76;
/// Part kind of a rigid polygon mesh.
pub const KIND_MESH: u32 = 1;
/// Part kind of the seam rings (DAT_800d74a8[0x11f] = DAT_800d7924).
pub const KIND_SEAM_RING: u32 = 0x11F;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MdlError {
    Truncated { at: usize, need: usize, len: usize },
    /// The part data does not end where the table starts.
    Layout { parts_end: usize, table: usize },
    /// A polygon list ran past its part, or a part had bytes after the list
    /// that the format does not explain.
    PolyList { part: usize, at: usize },
    BadOpcode { part: usize, at: usize, code: u8 },
    Ring { part: usize, reason: &'static str },
}

impl std::fmt::Display for MdlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for MdlError {}

fn need(b: &[u8], at: usize, n: usize) -> Result<(), MdlError> {
    if at.checked_add(n).map_or(true, |e| e > b.len()) {
        Err(MdlError::Truncated { at, need: n, len: b.len() })
    } else {
        Ok(())
    }
}

/// One vertex as stored: local position (model units, y down) and texel
/// coordinates inside the part's texture page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Vertex {
    pub pos: [i16; 3],
    pub uv: [u8; 2],
}

/// A Gouraud-shaded textured triangle or quad (GPU codes 0x34/0x36/0x3C/0x3E).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Polygon {
    /// Byte offset in the file.
    pub offset: usize,
    /// GPU command byte: 0x34 tri, 0x3C quad; bit 1 = semi-transparent.
    pub code: u8,
    /// 3 or 4 vertices in file order; for quads this is perimeter order
    /// (the GPU gets v0, v1, v3, v2).
    pub verts: Vec<Vertex>,
    /// One RGB per vertex (0x80 = unlit texel brightness).
    pub colors: Vec<[u8; 3]>,
    /// High bytes of the colour words of vertices 1..n (2 for a triangle,
    /// 3 for a quad). See `tex_flag`, `material`.
    pub flags: Vec<u8>,
}

impl Polygon {
    pub fn is_quad(&self) -> bool {
        self.verts.len() == 4
    }
    /// GPU semi-transparency bit of the command byte.
    pub fn semi_transparent(&self) -> bool {
        self.code & 2 != 0
    }
    /// High byte of the last colour word: bits 0-4 texture-table slot,
    /// bits 5-6 added to the tpage (semi-transparency mode), bit 7 =
    /// drawn without back-face culling [CODE 8001f040].
    pub fn tex_flag(&self) -> u8 {
        *self.flags.last().unwrap()
    }
    pub fn tex_slot(&self) -> u8 {
        self.tex_flag() & 0x1F
    }
    pub fn double_sided(&self) -> bool {
        self.tex_flag() & 0x80 != 0
    }
    /// High byte of the second-to-last colour word (triangle +0x1F, quad
    /// +0x2B): 2..25 in the data, read by no game code found [INFER:
    /// authoring material id]. The first extra byte of a quad (+0x27) is 0.
    pub fn material(&self) -> u8 {
        self.flags[self.flags.len() - 2]
    }
    /// CLUT index offset the emitter adds to the slot's CLUT attribute:
    /// ((v0 >> 6) & 3) * 4 + ((u0 >> 6) & 3), i.e. the 64x64 cell of the
    /// page that vertex 0 lies in [CODE 8001f040, GPU packets].
    pub fn clut_offset(&self) -> u16 {
        let [u, v] = self.verts[0].uv;
        ((v as u16 >> 6) & 3) * 4 + ((u as u16 >> 6) & 3)
    }
    /// Vertex indices in GPU packet order (quads: 0, 1, 3, 2).
    pub fn gpu_order(&self) -> &'static [usize] {
        if self.is_quad() {
            &[0, 1, 3, 2]
        } else {
            &[0, 1, 2]
        }
    }
    /// Byte length in the file.
    pub fn len_bytes(&self) -> usize {
        if self.is_quad() {
            0x30
        } else {
            0x24
        }
    }
}

/// Texture-table entry (0x800dff00 + 16 * slot): tpage and CLUT attributes
/// as the GPU takes them. The table is built by the `.RAW` loader
/// (`raw::Decoded::slots`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SlotAttr {
    pub tpage: u16,
    pub clut: u16,
}

/// GPU tpage and CLUT attributes of polygon `poly` in a part whose first
/// polygon carries `part_flag` (`Part::tex_flag`), given that part's slot
/// entry [CODE 8001f040: tpage = slot.tpage + (flag & 0x60); clut =
/// slot.clut + clut_offset; RAM/GPU verified for KERMIT and RACER].
pub fn tpage_clut(part_flag: u8, slot: SlotAttr, poly: &Polygon) -> (u16, u16) {
    (
        slot.tpage.wrapping_add((part_flag & 0x60) as u16),
        slot.clut.wrapping_add(poly.clut_offset()),
    )
}

/// The 76-byte part record. Offsets are from the record start (the table
/// is a u32 count followed by the records).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartRecord {
    /// +0x00 part data length in halfwords [CODE].
    pub size_hw: u32,
    /// +0x04 local origin in model space (x, y, z; y down) [CODE, RAM].
    pub pivot: [i32; 3],
    /// +0x10 kind: < 0x100 a mesh (1 in every file; 9 = attachment point,
    /// see `point_w`), >= 0x100 a shared block linked into list
    /// DAT_800d74a8[kind] (0x11F = seam rings) [CODE].
    pub kind: u32,
    /// +0x14..+0x19, always 0 [INFER unused].
    pub zero_14: [u8; 6],
    /// +0x1A: 0 = none; 1 or other = the part has data after its polygon
    /// list (slot kind += 4 or 8, slot +0x0C points after the list) [CODE].
    /// 0 in every file.
    pub tail_mode: i16,
    /// +0x1C..+0x27, always 0 [INFER unused].
    pub zero_1c: [u8; 12],
    /// +0x28 (255 for meshes, 0 for rings) and +0x2A (0) [INFER unused].
    pub unk_28: [i16; 2],
    /// +0x2C three (x, y, z) points; meshes: a bounding volume built by the
    /// export tool [INFER: values bracket the vertices, not read by the
    /// game except as below]. For kind 9 the first point (8 bytes) is
    /// copied to DAT_800b0f80 [CODE].
    pub points: [[i16; 3]; 3],
    /// The `w` halfwords after each point (+0x32, +0x3A, +0x42), copied to
    /// slot +0x28, +0x2C, +0x2A [CODE 8003bf58]. `point_w[1]` of a neck
    /// part = ring numbers it joins (low byte ring A, high byte ring B;
    /// KERMIT = 0x0001 -> rings 101 and 200) [CODE 800384b4, RAM].
    pub point_w: [i16; 3],
    /// +0x44, always 0 [INFER unused].
    pub unk_44: u16,
    /// +0x46 flags: 0x40 -> slot kind += 2 [CODE]. 0 in every file.
    pub flags_46: u16,
    /// +0x48 offset of this part's data in halfwords from file offset 4
    /// (0 for ring blocks) [INFER: matches every mesh part].
    pub data_offset_hw: u32,
}

impl PartRecord {
    fn parse(r: &[u8]) -> PartRecord {
        let s = |o| le::i16(r, o);
        PartRecord {
            size_hw: le::u32(r, 0),
            pivot: [le::i32(r, 4), le::i32(r, 8), le::i32(r, 12)],
            kind: le::u32(r, 0x10),
            zero_14: r[0x14..0x1A].try_into().unwrap(),
            tail_mode: s(0x1A),
            zero_1c: r[0x1C..0x28].try_into().unwrap(),
            unk_28: [s(0x28), s(0x2A)],
            points: [
                [s(0x2C), s(0x2E), s(0x30)],
                [s(0x34), s(0x36), s(0x38)],
                [s(0x3C), s(0x3E), s(0x40)],
            ],
            point_w: [s(0x32), s(0x3A), s(0x42)],
            unk_44: le::u16(r, 0x44),
            flags_46: le::u16(r, 0x46),
            data_offset_hw: le::u32(r, 0x48),
        }
    }

    /// Kind byte the loader stores in the model slot (+0x36) before the
    /// seam pass [CODE 8003bf58]: 9 -> 0xC; tail adds 4/8; flag 0x40 adds 2.
    pub fn slot_kind(&self) -> u8 {
        let mut k = self.kind as u8;
        if k == 9 {
            return 0xC;
        }
        if self.tail_mode != 0 {
            k = k.wrapping_add(if self.tail_mode == 1 { 4 } else { 8 });
        }
        if self.flags_46 & 0x40 != 0 {
            k = k.wrapping_add(2);
        }
        k
    }
}

/// A closed loop of seam vertices in its owner part's local frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeamRing {
    /// First word as stored (RING_MARKER on disc).
    pub link: u32,
    /// Number of segments; the ring has `segments + 1` vertices and the
    /// last repeats the first [CODE 800384b4 loop bound, data].
    pub segments: u16,
    /// owner * 100 + ring number, owner = part index + 1 (101 = ring 1 of
    /// part 0) [CODE 800384b4: id % 100 and id / 100 - 1].
    pub id: u16,
    /// Vertex and its trailing halfword (alternates 0/1 [INFER: side]).
    pub verts: Vec<([i16; 3], u16)>,
}

impl SeamRing {
    pub fn owner_part(&self) -> usize {
        (self.id / 100) as usize - 1
    }
    pub fn ring_number(&self) -> u16 {
        self.id % 100
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PartData {
    Mesh {
        polys: Vec<Polygon>,
        /// Bytes between the terminating zero word and the part end
        /// (`tail_mode` data); empty in every file.
        tail: Vec<u8>,
    },
    Ring(SeamRing),
    /// A kind >= 0x100 block without the ring layout (none on disc).
    Other(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    /// Byte offset of the part data in the file.
    pub offset: usize,
    pub record: PartRecord,
    pub data: PartData,
}

impl Part {
    pub fn polys(&self) -> &[Polygon] {
        match &self.data {
            PartData::Mesh { polys, .. } => polys,
            _ => &[],
        }
    }
    pub fn is_mesh(&self) -> bool {
        matches!(self.data, PartData::Mesh { .. })
    }
    /// The flag that selects the whole part's texture slot and blend mode:
    /// that of the first polygon [CODE 8001f040 reads it once per part].
    pub fn tex_flag(&self) -> Option<u8> {
        self.polys().first().map(|p| p.tex_flag())
    }
    /// For a neck part: (ring A number, ring B number) from `point_w[1]`.
    pub fn seam_rings(&self) -> (u16, u16) {
        let w = self.record.point_w[1] as u16;
        (w & 0xFF, w >> 8)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mdl {
    /// u32 at offset 0: table offset in halfwords.
    pub table_hw: u32,
    pub parts: Vec<Part>,
}

impl Mdl {
    /// Parse a whole `.MDL` file. Every byte is accounted for: the header
    /// word, the part data (tiling [4, table)), the table count and the
    /// records (ending exactly at the end of the file).
    pub fn parse(b: &[u8]) -> Result<Mdl, MdlError> {
        need(b, 0, 4)?;
        let table_hw = le::u32(b, 0);
        let table = table_hw as usize * 2;
        need(b, table, 4)?;
        let count = le::u32(b, table) as usize;
        need(b, table + 4, count * RECORD_LEN)?;
        if table + 4 + count * RECORD_LEN != b.len() {
            return Err(MdlError::Truncated { at: table + 4 + count * RECORD_LEN, need: 0, len: b.len() });
        }
        let mut parts = Vec::with_capacity(count);
        let mut at = 4usize;
        for i in 0..count {
            let r = &b[table + 4 + i * RECORD_LEN..table + 4 + (i + 1) * RECORD_LEN];
            let record = PartRecord::parse(r);
            let size = record.size_hw as usize * 2;
            need(b, at, size)?;
            let body = &b[at..at + size];
            let data = if record.kind < 0x100 {
                let (polys, used) = parse_polys(body, at, i)?;
                PartData::Mesh { polys, tail: body[used..].to_vec() }
            } else {
                parse_block(body, i)?
            };
            parts.push(Part { offset: at, record, data });
            at += size;
        }
        if at != table {
            return Err(MdlError::Layout { parts_end: at, table });
        }
        Ok(Mdl { table_hw, parts })
    }

    pub fn meshes(&self) -> impl Iterator<Item = (usize, &Part)> {
        self.parts.iter().enumerate().filter(|(_, p)| p.is_mesh())
    }

    pub fn rings(&self) -> impl Iterator<Item = &SeamRing> {
        self.parts.iter().filter_map(|p| match &p.data {
            PartData::Ring(r) => Some(r),
            _ => None,
        })
    }

    /// Number of model slots the loader fills (meshes; kind 9 is removed
    /// again by the `.anm` loader) [CODE 8003bf58 return value].
    pub fn slot_count(&self) -> usize {
        self.parts.iter().filter(|p| p.record.kind < 0x100).count()
    }

    /// Wavefront OBJ of the rest pose: each mesh vertex at local + pivot,
    /// converted to y-up by a 180-degree turn about X (x, -y, -z), faces
    /// wound counter-clockwise (the game draws clockwise-on-screen faces).
    /// `material(part_index, poly)` names the `usemtl` for each face (None
    /// = no material lines). UVs are u/256, 1 - v/256 of the 256x256 page.
    pub fn to_obj(&self, name: &str, mtllib: Option<&str>, material: &dyn Fn(usize, &Polygon) -> Option<String>) -> String {
        let mut s = String::new();
        s.push_str(&format!("# {name}: Muppet RaceMania MDL rest pose (mr-mdl)\n"));
        if let Some(m) = mtllib {
            s.push_str(&format!("mtllib {m}\n"));
        }
        let mut vbase = 1usize;
        let mut cur_mtl: Option<String> = None;
        for (pi, part) in self.meshes() {
            s.push_str(&format!("o part{pi}\n"));
            let [px, py, pz] = part.record.pivot;
            for poly in part.polys() {
                for (v, c) in poly.verts.iter().zip(&poly.colors) {
                    let x = v.pos[0] as i32 + px;
                    let y = v.pos[1] as i32 + py;
                    let z = v.pos[2] as i32 + pz;
                    s.push_str(&format!(
                        "v {} {} {} {:.4} {:.4} {:.4}\n",
                        x,
                        -y,
                        -z,
                        c[0] as f32 / 128.0,
                        c[1] as f32 / 128.0,
                        c[2] as f32 / 128.0
                    ));
                }
                for v in &poly.verts {
                    s.push_str(&format!("vt {:.6} {:.6}\n", (v.uv[0] as f32 + 0.5) / 256.0, 1.0 - (v.uv[1] as f32 + 0.5) / 256.0));
                }
            }
            for poly in part.polys() {
                let m = material(pi, poly);
                if m.is_some() && m != cur_mtl {
                    s.push_str(&format!("usemtl {}\n", m.as_ref().unwrap()));
                    cur_mtl = m;
                }
                let n = poly.verts.len();
                let order: &[usize] = if n == 4 { &[0, 3, 2, 1] } else { &[0, 2, 1] };
                s.push('f');
                for &k in order {
                    s.push_str(&format!(" {}/{}", vbase + k, vbase + k));
                }
                s.push('\n');
                vbase += n;
            }
        }
        s
    }
}

/// Parse a polygon list starting at `body[0]` (file offset `base`): records
/// until one whose code byte (+3) is 0, then the zero word. Returns the
/// polygons and the bytes consumed including the terminator.
pub fn parse_polys(body: &[u8], base: usize, part: usize) -> Result<(Vec<Polygon>, usize), MdlError> {
    let mut polys = Vec::new();
    let mut p = 0usize;
    loop {
        if p + 4 > body.len() {
            return Err(MdlError::PolyList { part, at: base + p });
        }
        let code = body[p + 3];
        if code == 0 {
            // FUN_8003bf58 stops on code byte 0; FUN_8001f040 on
            // (word & 0x3C000000) == 0. The word is 0 in every file.
            return Ok((polys, p + 4));
        }
        let nv = match code & 0x7C {
            0x34 => 3,
            0x3C => 4,
            _ => return Err(MdlError::BadOpcode { part, at: base + p, code }),
        };
        let len = 4 + nv * 8 + (nv - 1) * 4;
        if p + len > body.len() {
            return Err(MdlError::PolyList { part, at: base + p });
        }
        let r = &body[p..p + len];
        let verts = (0..nv)
            .map(|k| {
                let o = 4 + k * 8;
                Vertex { pos: [le::i16(r, o), le::i16(r, o + 2), le::i16(r, o + 4)], uv: [r[o + 6], r[o + 7]] }
            })
            .collect();
        let mut colors = vec![[r[0], r[1], r[2]]];
        let mut flags = Vec::with_capacity(nv - 1);
        for k in 0..nv - 1 {
            let o = 4 + nv * 8 + k * 4;
            colors.push([r[o], r[o + 1], r[o + 2]]);
            flags.push(r[o + 3]);
        }
        polys.push(Polygon { offset: base + p, code, verts, colors, flags });
        p += len;
    }
}

fn parse_block(body: &[u8], part: usize) -> Result<PartData, MdlError> {
    if body.len() < 8 || le::u32(body, 0) != RING_MARKER {
        return Ok(PartData::Other(body.to_vec()));
    }
    let segments = le::u16(body, 4);
    let id = le::u16(body, 6);
    let n = segments as usize + 1;
    if 8 + n * 8 != body.len() {
        return Err(MdlError::Ring { part, reason: "length != 8 + 8 * (segments + 1)" });
    }
    if id < 100 {
        return Err(MdlError::Ring { part, reason: "id below 100" });
    }
    let verts = (0..n)
        .map(|k| {
            let o = 8 + k * 8;
            ([le::i16(body, o), le::i16(body, o + 2), le::i16(body, o + 4)], le::u16(body, o + 6))
        })
        .collect();
    Ok(PartData::Ring(SeamRing { link: RING_MARKER, segments, id, verts }))
}

/// Model slot array in RAM: 0x800bc668 + 0x6C * slot [CODE 8003bf58].
pub const SLOT_TABLE_ADDR: u32 = 0x800B_C668;
pub const SLOT_LEN: usize = 0x6C;

/// The fields of a RAM model slot that the loader fills from the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RamSlot {
    pub pivot: [i32; 3],
    /// +0x0C kind as i32 (or the tail pointer).
    pub kind_word: u32,
    /// +0x20 pointer to the polygon list in RAM.
    pub polys_ptr: u32,
    /// +0x24 polygon count.
    pub poly_count: i16,
    /// +0x28, +0x2A, +0x2C (seam pass rewrites them).
    pub w: [i16; 3],
    /// +0x36 kind byte (1 drawn mesh, 2 neck after the seam pass).
    pub kind: u8,
}

impl RamSlot {
    pub fn parse(s: &[u8]) -> RamSlot {
        RamSlot {
            pivot: [le::i32(s, 0), le::i32(s, 4), le::i32(s, 8)],
            kind_word: le::u32(s, 0x0C),
            polys_ptr: le::u32(s, 0x20),
            poly_count: le::i16(s, 0x24),
            w: [le::i16(s, 0x28), le::i16(s, 0x2A), le::i16(s, 0x2C)],
            kind: s[0x36],
        }
    }
}

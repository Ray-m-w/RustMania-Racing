//! dat: a track's `<course>.DAT` (e.g. COURSE_A/DOCK.DAT): the track's
//! scenery as placed, textured meshes, plus point lists (paths) and a
//! per-path-node visibility stream. Spec: docs/spec/formats/DAT.md.
//!
//! Behaviour reproduced: the loader FUN_8002e3dc(track, file_size) that runs
//! on the file in place right after FUN_8004627c reads it to the heap pointer
//! DAT_800b0ee8, the rotation builder FUN_8001f54c, the primitive counter
//! FUN_80025b8c / FUN_8002e13c, the mesh renderer FUN_8001a304 (primitive
//! semantics, colour and texture attribute rules), the scenery pass of
//! FUN_80029f40 (layer 1 is drawn with the camera divided by 4), and the
//! visibility stream walker FUN_8005d908 / FUN_8005dc14.
//!
//! Verified against the running recomp (mr-dat --oracle): the
//! loader-transformed file and the per-object records it builds match the
//! game's RAM byte for byte (except the 11 objects game code moves), the
//! RTPS/RTPT inputs of the scenery renderers are decoded mesh vertices, each
//! scenery draw's GTE rotation/translation is the decoded object transform
//! seen through one common camera, and the GP0 packets built from DAT
//! vertices carry exactly the decoded colours, UVs, CLUT and tpage
//! (except subdivided pieces).

use crate::le;
use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone)]
pub struct Error {
    pub offset: usize,
    pub msg: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "DAT @0x{:x}: {}", self.offset, self.msg)
    }
}
impl std::error::Error for Error {}

fn err<T>(offset: usize, msg: impl Into<String>) -> Result<T, Error> {
    Err(Error { offset, msg: msg.into() })
}

/// Bounds-checked little-endian reader over the file.
struct R<'a>(&'a [u8]);
impl<'a> R<'a> {
    fn need(&self, o: usize, n: usize) -> Result<(), Error> {
        if o.checked_add(n).map_or(true, |e| e > self.0.len()) {
            return err(o, format!("read of {n} bytes past end (len 0x{:x})", self.0.len()));
        }
        Ok(())
    }
    fn u8(&self, o: usize) -> Result<u8, Error> {
        self.need(o, 1)?;
        Ok(self.0[o])
    }
    fn u16(&self, o: usize) -> Result<u16, Error> {
        self.need(o, 2)?;
        Ok(le::u16(self.0, o))
    }
    fn i16(&self, o: usize) -> Result<i16, Error> {
        self.need(o, 2)?;
        Ok(le::i16(self.0, o))
    }
    fn i32(&self, o: usize) -> Result<i32, Error> {
        self.need(o, 4)?;
        Ok(le::i32(self.0, o))
    }
    fn u32(&self, o: usize) -> Result<u32, Error> {
        self.need(o, 4)?;
        Ok(le::u32(self.0, o))
    }
}

// ---------------------------------------------------------------------------
// Sine table (game data in the executable, read from the owner's disc)
// ---------------------------------------------------------------------------

/// The game's 5120-entry sine table at 0x800A88C0 (1.0 = 16384; cos(a) is
/// entry a+1024). It is initialised data of SLUS_012.37 and is not exactly
/// any rounding of sin(), so it is read from the executable, never computed.
pub struct SinTable(pub Vec<i16>);

pub const SIN_TABLE_ADDR: u32 = 0x800A_88C0;

impl SinTable {
    /// Read from the PS-X EXE image (file bytes of SLUS_012.37).
    pub fn from_exe(exe: &[u8]) -> Result<Self, Error> {
        if exe.len() < 0x800 || &exe[0..8] != b"PS-X EXE" {
            return err(0, "not a PS-X EXE");
        }
        let t_addr = le::u32(exe, 0x18);
        let off = (SIN_TABLE_ADDR - t_addr) as usize + 0x800;
        let r = R(exe);
        r.need(off, 5120 * 2)?;
        Ok(SinTable((0..5120).map(|i| le::i16(exe, off + 2 * i)).collect()))
    }
    /// sin(a) in 4.12 (the game's `table[a & 0xfff] >> 2`).
    pub fn sin12(&self, a: u16) -> i32 {
        (self.0[(a & 0xFFF) as usize] as i32) >> 2
    }
    /// cos(a) in 4.12 (`table[(a & 0xfff) + 1024] >> 2`).
    pub fn cos12(&self, a: u16) -> i32 {
        (self.0[(a & 0xFFF) as usize + 1024] as i32) >> 2
    }
}

/// GTE gpf12-style product: (a*b) >> 12, arithmetic.
fn mul12(a: i32, b: i32) -> i32 {
    (a * b) >> 12
}

/// FUN_8001f54c: rotation matrix (4.12, row-major m[row][col]) from three
/// 12-bit angles, M = Rx(ax) * Ry(ay) * Rz(az), with the game's exact
/// truncation order. [CODE 0x8001f54c] [RAM: 832/832 load-time matrices]
pub fn rot_matrix(sin: &SinTable, ang: [u16; 3]) -> [[i16; 3]; 3] {
    let (sa, ca) = (sin.sin12(ang[0]), sin.cos12(ang[0]));
    let (sb, cb) = (sin.sin12(ang[1]), sin.cos12(ang[1]));
    let (sc, cc) = (sin.sin12(ang[2]), sin.cos12(ang[2]));
    let sa_sb = mul12(sa, sb);
    let ca_sb = mul12(ca, sb);
    let m00 = mul12(cc, cb);
    let m01 = -mul12(sc, cb);
    let m02 = sb;
    let m10 = mul12(cc, sa_sb) + mul12(ca, sc);
    let m11 = mul12(ca, cc) - mul12(sc, sa_sb);
    let m12 = -mul12(sa, cb);
    let m20 = mul12(sa, sc) - mul12(cc, ca_sb);
    let m21 = mul12(sa, cc) + mul12(sc, ca_sb);
    let m22 = mul12(ca, cb);
    let s = |v: i32| v as i16;
    [[s(m00), s(m01), s(m02)], [s(m10), s(m11), s(m12)], [s(m20), s(m21), s(m22)]]
}

/// The loader's column scale for scaled descriptors: m[r][c] = m*scale[c]/4096,
/// C division (truncates toward zero). [CODE 0x8002e3dc]
pub fn scale_matrix(m: &mut [[i16; 3]; 3], scale: [i16; 3]) {
    for row in m.iter_mut() {
        for c in 0..3 {
            row[c] = ((row[c] as i32 * scale[c] as i32) / 4096) as i16;
        }
    }
}

// ---------------------------------------------------------------------------
// Point-list chunks (file header)
// ---------------------------------------------------------------------------

/// Chunk id 0x40: the visibility zone path (points 0 and 1 are the zone's
/// bbox corners, points 2.. are the path nodes the visibility stream steps
/// along). [CODE 0x8002e3dc, 0x8005dc14]
pub const CHUNK_VIS_PATH: i16 = 0x40;

#[derive(Debug, Clone)]
pub enum ChunkBody {
    /// ids 0..=0x3e and 0x40: `count` points of three i32 (x, y, z), layer-0
    /// world units. Registered at DAT_800d1c48[id] (0x40: zone table
    /// DAT_800e3494). Meaning per id is not decoded (paths) [INFER].
    Points(Vec<[i32; 3]>),
    /// id 0x3f: `count` records of 16 bytes (not present on DOCK) [CODE size].
    Records16(Vec<[u8; 16]>),
    /// id < 0 (registered at DAT_800d1c48[-id]) or id > 0x40 (converted in
    /// place by the loader to s16/4 points + a computed normal): not present
    /// on DOCK, kept raw (bytes after the 4-byte header) [CODE size].
    Raw(Vec<u8>),
}

#[derive(Debug, Clone)]
pub struct Chunk {
    pub offset: usize,
    pub count: i16,
    pub id: i16,
    pub body: ChunkBody,
}

impl Chunk {
    /// Total size including the 4-byte header, as the loader steps it.
    /// [CODE 0x8002e3dc]
    pub fn size_for(count: i16, id: i16) -> usize {
        let c = count as i32;
        let n = if id < 0 {
            ((c * 3 + 1) / 2) * 4 + 0x10
        } else if id == 0x3F {
            c * 16 + 4
        } else {
            c * 12 + 4
        };
        n.max(4) as usize
    }
}

// ---------------------------------------------------------------------------
// Objects and descriptors
// ---------------------------------------------------------------------------

/// Which of the file's two object lists an object came from. Layer 0
/// (DAT_800b0f00) is in world units; layer 1 (DAT_800b0fb4) is drawn with
/// the camera position divided by 4 [CODE 0x8002A600..0x8002A67C], so its
/// coordinates are quarter world units. [RAM/GTE: both layers resolve to one
/// camera with that factor]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    World = 0,
    Quarter = 1,
}

impl Layer {
    pub fn world_scale(self) -> i32 {
        match self {
            Layer::World => 1,
            Layer::Quarter => 4,
        }
    }
}

/// Object flag bits (instance byte +0x0e). [CODE 0x8002e3dc, 0x8002e13c]
pub mod flag {
    /// Low bits (& 0x7, with 0x40 ignored by the type switch): 1/4 = 8-byte
    /// vertex mesh (FUN_80025b8c path), 2/3 = other model kind, 5 = none.
    pub const KIND_MASK: u8 = 0x07;
    /// Descriptor carries a per-axis scale (32-byte descriptor).
    pub const SCALED: u8 = 0x08;
    pub const B10: u8 = 0x10;
    /// Descriptor has a second mesh pointer (loader then sets 0x40).
    pub const SECOND_MESH: u8 = 0x20;
    pub const B40: u8 = 0x40;
    /// Loader rewrites f -> (f | 0x50) - 0x10 (sets 0x40, clears 0x10) and,
    /// in layer 0, clears bit 15 of the radius halfword.
    pub const B80: u8 = 0x80;
}

/// A placed object: 20 bytes in a list ended by a record whose flag byte
/// (+0x0e) is 0. [CODE 0x8002e3dc]
#[derive(Debug, Clone)]
pub struct Object {
    pub offset: usize,
    pub layer: Layer,
    /// +0x00 i32 x, y, z: centre of the transformed mesh's bounding box, in
    /// the layer's units [RAM/INFER: matches bbox centre to +-1 on DOCK].
    pub centre: [i32; 3],
    /// +0x0c u16: cull radius in layer units (FUN_80010000 compares it with
    /// camera distance >> 4) [CODE 0x80010000]; bit 15 cleared by the loader
    /// when flag 0x80 is set (layer 0).
    pub radius: u16,
    /// +0x0e u8 (see `flag`), as stored on disc.
    pub flags: u8,
    /// +0x0f u8: unknown, 0 on DOCK.
    pub b0f: u8,
    /// +0x10 u32: file offset of the descriptor (relocated, then replaced by
    /// the pointer to the object's 40-byte record by the loader).
    pub desc_offset: u32,
}

impl Object {
    /// Flag byte after the loader's 0x80 rewrite. [CODE 0x8002e3dc]
    pub fn loaded_flags(&self) -> u8 {
        let mut f = self.flags;
        if f & flag::B80 != 0 {
            f = (f | 0x50).wrapping_sub(0x10);
        }
        if f & flag::SECOND_MESH != 0 {
            f |= flag::B40;
        }
        f
    }
}

/// Per-object descriptor: 24 bytes (+4 with a second mesh), or 32 (+4) when
/// the object flag has SCALED. [CODE 0x8002e3dc]
#[derive(Debug, Clone)]
pub struct Descriptor {
    pub offset: usize,
    /// +0x00 i32 x, y, z: translation of the mesh origin, layer units
    /// [RAM/GTE].
    pub origin: [i32; 3],
    /// +0x0c u16 x3: rotation angles (12-bit, 4096 = 360 deg), M = Rx*Ry*Rz.
    pub angles: [u16; 3],
    /// SCALED only: +0x12 i16 x3, 4.12 column scales.
    pub scale: Option<[i16; 3]>,
    /// +0x12 (or +0x18 scaled) u8: the loader keeps only the low nibble,
    /// the layer-0 depth (OT) bias index: bias = 16 * b_lo (`lod_bias`).
    /// [CODE 80028010]
    pub b_lo: u8,
    /// +0x13 (or +0x19) u8: bits 0-2 select a light matrix (0x800BBA00 +
    /// 32n; 0 = default) [CODE 80028010]; bits 3-7 (clamped to 18 by
    /// FUN_8002e13c) scale the value passed as FUN_80012390 param_2
    /// [INFER: draw distance].
    pub b_hi: u8,
    /// SCALED only: +0x1a u16, unknown (0 or small on DOCK).
    pub b_extra: u16,
    /// +0x14 (or +0x1c): file offset of the mesh.
    pub mesh: u32,
    /// +0x18 (or +0x20), only with SECOND_MESH: file offset of a second mesh.
    pub mesh2: Option<u32>,
    /// Model format at `mesh`, from the object flags (FUN_8002e13c switch).
    pub kind: ModelKind,
}

/// What `Descriptor::mesh` points at, by object flags & 0x6f with the scale
/// bit removed: 1/4 (+0x40) = 8-byte-vertex `Mesh`, 2/3 = `PartsModel`,
/// 5 = nothing. [CODE 0x8002e13c]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelKind {
    Mesh,
    Parts,
    None,
}

impl ModelKind {
    pub fn from_flags(loaded_flags: u8) -> Option<ModelKind> {
        match loaded_flags & 0x67 {
            1 | 4 | 0x41 | 0x44 => Some(ModelKind::Mesh),
            2 | 3 | 0x42 | 0x43 => Some(ModelKind::Parts),
            5 | 0x45 => Some(ModelKind::None),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Orphan {
    pub offset: usize,
    pub end: usize,
    pub mesh: Option<Mesh>,
}

/// Model format 2/3 (no instance on DOCK): i32 n, n x 12 bytes (per-part
/// header, three i32 [INFER: pivot]), then per part an i32 k and k x 44-byte
/// records (11 words; [INFER: prebuilt primitives]). Only its size and
/// record count are reproduced (FUN_8002e13c counts the records). Kept raw.
#[derive(Debug, Clone)]
pub struct PartsModel {
    pub offset: usize,
    pub headers: Vec<[i32; 3]>,
    pub parts: Vec<Vec<[u8; 44]>>,
    pub end: usize,
}

impl PartsModel {
    pub fn record_count(&self) -> usize {
        self.parts.iter().map(|p| p.len()).sum()
    }
}

fn parse_parts(r: &R, o: usize) -> Result<PartsModel, Error> {
    let n = r.i32(o)?;
    if !(0..=4096).contains(&n) {
        return err(o, format!("parts model count {n}"));
    }
    let mut headers = Vec::new();
    for i in 0..n as usize {
        let q = o + 4 + 12 * i;
        headers.push([r.i32(q)?, r.i32(q + 4)?, r.i32(q + 8)?]);
    }
    let mut q = o + 4 + 12 * n as usize;
    let mut parts = Vec::new();
    for _ in 0..n {
        let k = r.i32(q)?;
        if !(0..=65536).contains(&k) {
            return err(q, format!("parts record count {k}"));
        }
        q += 4;
        r.need(q, 44 * k as usize)?;
        parts.push((0..k as usize).map(|j| r.0[q + 44 * j..q + 44 * j + 44].try_into().unwrap()).collect());
        q += 44 * k as usize;
    }
    Ok(PartsModel { offset: o, headers, parts, end: q })
}

impl Descriptor {
    pub fn size(&self) -> usize {
        (if self.scale.is_some() { 0x20 } else { 0x18 }) + if self.mesh2.is_some() { 4 } else { 0 }
    }
    /// The 4.12 matrix the loader stores in the object's record.
    pub fn matrix(&self, sin: &SinTable) -> [[i16; 3]; 3] {
        let mut m = rot_matrix(sin, self.angles);
        if let Some(s) = self.scale {
            scale_matrix(&mut m, s);
        }
        m
    }
    fn b_hi_loaded(&self) -> u8 {
        let hi = (self.b_hi >> 3).min(0x12);
        (self.b_hi & 7) + hi * 8
    }
}

// ---------------------------------------------------------------------------
// Meshes
// ---------------------------------------------------------------------------

/// Mesh vertex: 8 bytes, the GTE SVECTOR the renderer loads directly
/// (vx, vy, vz, pad=colour). [CODE 0x8001a304] [GTE: all inputs found]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Vertex {
    pub pos: [i16; 3],
    /// 15-bit colour with RED in bits 10-14, green 5-9, BLUE in 0-4 (the
    /// reverse of the GPU's order); the renderer expands it to the
    /// primitive's 24-bit modulation colour, 0x80 = neutral. [CODE 0x8001a304]
    pub color: u16,
    /// Only for meshes with a negative vertex count (12-byte vertices; not
    /// present in DOCK): the trailing 4 bytes, meaning unknown.
    pub extra: Option<[u8; 4]>,
}

impl Vertex {
    /// Modulation colour as the GPU receives it (R, G, B; 0x80 = 1.0).
    pub fn rgb(&self) -> [u8; 3] {
        let c = self.color;
        [(((c >> 10) & 31) << 3) as u8, (((c >> 5) & 31) << 3) as u8, ((c & 31) << 3) as u8]
    }
}

/// One primitive record. [CODE 0x8001a304]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Prim {
    /// Vertex indices in polygon (cyclic) order; quads use all four
    /// (GPU order is idx0, idx1, idx3, idx2), triangles the first three.
    pub idx: [u8; 4],
    /// Texture coordinates per corner (u, v), same order as `idx`; zero for
    /// untextured primitives.
    pub uv: [[u8; 2]; 4],
    /// The record bytes as stored (12 for textured, 4 for untextured).
    pub raw: [u8; 12],
}

/// A run of primitives sharing one header halfword. [CODE 0x8001a304,
/// 0x80025b8c]
#[derive(Debug, Clone)]
pub struct PrimGroup {
    pub offset: usize,
    pub header: u16,
    pub prims: Vec<Prim>,
}

impl PrimGroup {
    /// Bits 0-4: primitive kind (dispatch of FUN_8001a304).
    pub fn kind(&self) -> u8 {
        (self.header & 0x1F) as u8
    }
    /// Kind bit 0: triangle (GT3/G3) instead of quad (GT4/G4).
    pub fn is_triangle(&self) -> bool {
        self.kind() & 1 != 0
    }
    /// Kind bit 1: drawn without the NCLIP back-face test.
    pub fn double_sided(&self) -> bool {
        self.kind() & 2 != 0
    }
    /// Kind bit 2 (quads): culled only if both halves face away.
    pub fn cull_both_halves(&self) -> bool {
        self.kind() & 4 != 0
    }
    /// Kind bit 3: FUN_80025b8c reserves two packet slots per primitive.
    pub fn double_slot(&self) -> bool {
        self.kind() & 0x18 == 8
    }
    /// Kind bit 4: untextured gouraud (4-byte index-only records).
    pub fn is_textured(&self) -> bool {
        self.kind() & 0x10 == 0
    }
    /// Bits 5-6: added to the tpage's ABR field; 3 = opaque, 0..2 = semi-
    /// transparent with that blend mode.
    pub fn blend_bits(&self) -> u8 {
        ((self.header >> 5) & 3) as u8
    }
    pub fn semi_transparent(&self) -> bool {
        self.blend_bits() != 3
    }
    /// Bits 8-12: texture slot, index into the 16-byte table at 0x800DFF00
    /// that the track's .RAW loader fills (tpage, clut, ...).
    pub fn tex_slot(&self) -> u8 {
        ((self.header >> 8) & 0x1F) as u8
    }
    /// Bytes per record.
    pub fn record_size(kind: u8) -> usize {
        if kind & 0x10 != 0 {
            4
        } else {
            12
        }
    }
    /// Packet slots the loader counts for this group (FUN_80025b8c).
    pub fn slots(&self) -> usize {
        self.prims.len() * if self.double_slot() { 2 } else { 1 }
    }
    /// GPU command byte the renderer emits (before semi-transparency bit).
    pub fn gpu_code(&self) -> u8 {
        let base = match (self.is_textured(), self.is_triangle()) {
            (true, false) => 0x3C,
            (true, true) => 0x34,
            (false, false) => 0x38,
            (false, true) => 0x30,
        };
        base | if self.semi_transparent() { 2 } else { 0 }
    }
}

/// One entry of the texture-slot table at 0x800DFF00 (filled by the .RAW
/// loader, not part of the DAT). [CODE 0x8004627c, 0x8001a304]
#[derive(Debug, Clone, Copy, Default)]
pub struct TexSlot {
    pub tpage: u16,
    pub clut: u16,
    /// +8 u16: when 0 the CLUT is offset per 64x64 texel tile (see
    /// `prim_tpage_clut`); 1 for the 8-bit pages on DOCK.
    pub fixed_clut: u16,
}

/// Effective (tpage, clut) halfwords of a textured primitive, as the
/// renderer builds them: tpage = slot.tpage + (header & 0x60); clut =
/// slot.clut + (u0 >> 6) + 4 * (v0 >> 6) unless the slot is fixed-CLUT,
/// where (u0, v0) is the first corner's UV. [CODE 0x8001a304]
pub fn prim_tpage_clut(slot: TexSlot, g: &PrimGroup, p: &Prim) -> (u16, u16) {
    let tpage = slot.tpage.wrapping_add(g.header & 0x60);
    let mut clut = slot.clut;
    if slot.fixed_clut == 0 {
        let (u, v) = (p.uv[0][0] as u16, p.uv[0][1] as u16);
        clut = clut.wrapping_add((u >> 6) + 4 * (v >> 6));
    }
    (tpage, clut)
}

// ---------------------------------------------------------------------------
// Texture LOD of the layer-0 renderer
// ---------------------------------------------------------------------------

/// Texture LOD of layer-0 scenery (FUN_80012390 / FUN_80012620 family; the
/// layer-1 renderer FUN_8001a304 never applies it). [CODE 80016b3c,
/// 80012390, 80042534, 8002e3dc] [RAM] [GPU]
///
/// The RAW loader (FUN_80044008) overwrites the top-left of each 4-bit page
/// of slots 0..3 with point-sampled reduced copies: texel (u, v) of the full
/// page is also at (u/2, v/2) in the w/2 x h/2 corner and at (u/4, v/4) in
/// the w/4 x h/4 corner (VRAM check on DOCK pages 0..3: 3072/3072 sampled
/// texels each). Stored UVs never point into that corner; they address the
/// full-resolution texels. LOD 1/2 divide each UV byte by 2/4, keeping the
/// CLUT chosen from the full-size UV (`prim_tpage_clut`).
pub fn lod_uvs(slot: u8, level: u8, uv: [[u8; 2]; 4]) -> [[u8; 2]; 4] {
    if slot >= 4 || level == 0 {
        return uv;
    }
    let sh = level.min(2);
    uv.map(|c| c.map(|b| b >> sh))
}

/// Depth limits of the LOD bands in "k" units, k = OTZ + bias where
/// OTZ = (ZSF4 * (sz0+sz1+sz2+sz3)) >> 12 of the quad (ZSF4 = 256 in a race:
/// the average screen z / 4) and bias = `lod_bias`. LOD 0 if k <= lod0_max,
/// LOD 1 if k <= lod1_max, else LOD 2.
///
/// The game compares OT byte offsets: off = T[2k] with T the table
/// FUN_80042534 builds at DAT_800b0ca8 (DAT_800b0cd4 = 400 on DOCK),
/// LOD 0 if off <= DAT_800b0f1c, LOD 1 if off <= DAT_800b0c44. FUN_8002e3dc
/// sets those to 0xC00/0xF80 (1 player, DAT_800b0ca6 == 0), to 0/0xC00 when
/// DAT_800b0ca6 != 0, and to 0x4000/0x4000 (no LOD) for courses 0x1D/0x1E,
/// mode DAT_800b0b9a == 3 or DAT_800b0e38 > 6. With the DOCK table T first
/// exceeds 0xC00 at index 1902 and 0xF80 at index 4901.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LodThresholds {
    pub lod0_max: i32,
    pub lod1_max: i32,
}

impl LodThresholds {
    /// 1-player race (DOCK): thresholds 0xC00/0xF80. [RAM][GPU 80/80 packets]
    pub const ONE_PLAYER: LodThresholds = LodThresholds { lod0_max: 950, lod1_max: 2450 };
    /// DAT_800b0ca6 != 0: thresholds 0/0xC00 [CODE only; k <= 0 never occurs
    /// in practice].
    pub const CA6_SET: LodThresholds = LodThresholds { lod0_max: 0, lod1_max: 950 };
    /// Thresholds 0x4000: the table never exceeds them, always LOD 0. [CODE]
    pub const NONE: LodThresholds = LodThresholds { lod0_max: i32::MAX, lod1_max: i32::MAX };

    pub fn level(&self, k: i32) -> u8 {
        if k <= self.lod0_max {
            0
        } else if k <= self.lod1_max {
            1
        } else {
            2
        }
    }
}

/// Depth bias the layer-0 draw adds to OTZ (FUN_80012390 param_3, from
/// caseD_1 at 0x80028010): the course byte at 0x800ACB3F + 12*course (DOCK,
/// course 3: 96) + 16 * (descriptor b_lo & 0xF) (table 0x800A87C4 =
/// 0, 16, .., 240). DOCK sand objects mostly have b_lo = 4 (bias 160).
pub fn lod_bias(course_bias: u8, b_lo: u8) -> i32 {
    course_bias as i32 + 16 * (b_lo & 0xF) as i32
}

/// The course depth-bias byte from the executable (0x800ACB3F + 12*course).
pub fn course_depth_bias(exe: &[u8], course: usize) -> Result<u8, Error> {
    if exe.len() < 0x800 || &exe[0..8] != b"PS-X EXE" {
        return err(0, "not a PS-X EXE");
    }
    let t_addr = le::u32(exe, 0x18);
    let off = (0x800A_CB3F - t_addr) as usize + 0x800 + 12 * course;
    R(exe).u8(off)
}

/// LOD for a quad from its four screen z (GTE SZ). A renderer without
/// per-polygon depth can pass 4 x the object's view-space depth: k ~= z/4 +
/// bias (view z in layer-0 world units; the camera matrix's z row is unit).
pub fn lod_level_for_sz(sz: [i32; 4], bias: i32, t: LodThresholds) -> u8 {
    let otz = (256 * sz.iter().sum::<i32>()) >> 12;
    t.level(otz + bias)
}

#[derive(Debug, Clone)]
pub struct Mesh {
    pub offset: usize,
    /// i32 at +0: vertex count; negative means 12-byte vertices plus a
    /// 4-byte trailer (not present on DOCK) [CODE 0x80025b8c].
    pub raw_count: i32,
    pub vertices: Vec<Vertex>,
    pub groups: Vec<PrimGroup>,
    /// Offset just past the 0xFFFF terminator, padded to 4 (next mesh).
    pub end: usize,
}

impl Mesh {
    pub fn prim_count(&self) -> usize {
        self.groups.iter().map(|g| g.prims.len()).sum()
    }
    /// FUN_8002e13c / FUN_80025b8c: packet slots (stored at record +0x20).
    pub fn slot_count(&self) -> usize {
        self.groups.iter().map(|g| g.slots()).sum()
    }
}

fn parse_mesh(r: &R, o: usize) -> Result<Mesh, Error> {
    let n = r.i32(o)?;
    let mut vertices = Vec::new();
    let mut q;
    if n >= 0 {
        if n > 256 {
            return err(o, format!("vertex count {n} > 256 (indices are bytes)"));
        }
        for i in 0..n as usize {
            let b = o + 4 + 8 * i;
            vertices.push(Vertex {
                pos: [r.i16(b)?, r.i16(b + 2)?, r.i16(b + 4)?],
                color: r.u16(b + 6)?,
                extra: None,
            });
        }
        q = o + 4 + 8 * n as usize;
    } else {
        let m = (-n) as usize;
        if m > 256 {
            return err(o, format!("vertex count {n} out of range"));
        }
        for i in 0..m {
            let b = o + 4 + 12 * i;
            vertices.push(Vertex {
                pos: [r.i16(b)?, r.i16(b + 2)?, r.i16(b + 4)?],
                color: r.u16(b + 6)?,
                extra: Some([r.u8(b + 8)?, r.u8(b + 9)?, r.u8(b + 10)?, r.u8(b + 11)?]),
            });
        }
        q = o + 4 + 12 * m + 4;
    }
    let nv = vertices.len();
    let mut groups = Vec::new();
    loop {
        let h = r.u16(q)?;
        if h == 0xFFFF {
            break;
        }
        let count = r.i16(q + 2)?;
        let kind = (h & 0x1F) as u8;
        // Kinds FUN_80025b8c does not step over (5,7,0xd,0xf,0x15,0x17,0x1d)
        // would hang the game; reject them.
        if matches!(kind, 5 | 7 | 0xD | 0xF | 0x15 | 0x17 | 0x1D) {
            return err(q, format!("primitive kind 0x{kind:x} has no record size"));
        }
        if count < 0 {
            return err(q, "negative primitive count");
        }
        let rs = PrimGroup::record_size(kind);
        let mut prims = Vec::with_capacity(count as usize);
        for k in 0..count as usize {
            let b = q + 4 + rs * k;
            r.need(b, rs)?;
            let mut raw = [0u8; 12];
            raw[..rs].copy_from_slice(&r.0[b..b + rs]);
            let idx = [raw[0], raw[1], raw[2], raw[3]];
            let used = if kind & 1 != 0 { 3 } else { 4 };
            for &i in &idx[..used] {
                if i as usize >= nv {
                    return err(b, format!("vertex index {i} >= {nv}"));
                }
            }
            let mut uv = [[0u8; 2]; 4];
            if rs == 12 {
                if kind & 1 == 0 {
                    for c in 0..4 {
                        uv[c] = [raw[4 + 2 * c], raw[5 + 2 * c]];
                    }
                } else {
                    // Triangles: bytes 3..6 unused; UVs at 6, 8, 10.
                    for c in 0..3 {
                        uv[c] = [raw[6 + 2 * c], raw[7 + 2 * c]];
                    }
                }
            }
            prims.push(Prim { idx, uv, raw });
        }
        groups.push(PrimGroup { offset: q, header: h, prims });
        q += 4 + rs * count as usize;
    }
    let end = (q + 2 + 3) & !3;
    Ok(Mesh { offset: o, raw_count: n, vertices, groups, end })
}

// ---------------------------------------------------------------------------
// Visibility zones
// ---------------------------------------------------------------------------

/// One visibility zone (one per id-0x40 chunk). Record found by walking
/// back from the file's last i32. [CODE 0x8002e3dc, 0x8005d908, 0x8005dc14]
#[derive(Debug, Clone)]
pub struct VisZone {
    /// Index into `Dat::chunks` of the zone's 0x40 chunk.
    pub chunk: usize,
    /// Zone bbox = min/max over the chunk's first two points (loader table
    /// DAT_800e3478), and its "size" (dx>>5)^2+(dy>>5)^2+(dz>>5)^2.
    pub bbox_min: [i32; 3],
    pub bbox_max: [i32; 3],
    pub size2: i32,
    /// File offset and length (i16 at +0, includes the 4-byte header).
    pub offset: usize,
    pub len: usize,
    /// +0x02 u16: unknown (0 on DOCK).
    pub b02: u16,
    /// +0x04: 256-byte visibility mask at path node 0. Bit order is MSB
    /// first per byte. Layer-0 object i: byte i/8 [CODE 0x80010000];
    /// layer-1 object i: byte 0xFF - i/8 [CODE 0x8001031c].
    pub mask0: Vec<u8>,
    /// +0x104: delta stream of (xor_mask, byte_index) pairs; a pair with
    /// mask 0 separates nodes. Moving from node k to k+1 applies the pairs
    /// after separator k. [RAM: mask at the player's node matches]
    pub stream: Vec<u8>,
}

impl VisZone {
    /// Offsets (within `stream`) of the node separators.
    pub fn separators(&self) -> Vec<usize> {
        (0..self.stream.len() / 2).map(|i| 2 * i).filter(|&i| self.stream[i] == 0).collect()
    }
    /// The 256-byte mask after stepping forward to `node`.
    pub fn mask_at(&self, node: usize) -> Vec<u8> {
        let mut m = self.mask0.clone();
        let seps = self.separators();
        for k in 0..node.min(seps.len()) {
            let mut p = seps[k] + 2;
            while p + 1 < self.stream.len() && self.stream[p] != 0 {
                m[self.stream[p + 1] as usize] ^= self.stream[p];
                p += 2;
            }
        }
        m
    }
}

// ---------------------------------------------------------------------------
// The file
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Section {
    pub name: &'static str,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone)]
pub struct Dat {
    pub len: usize,
    /// +0x00 i32: number of chunks.
    pub chunks: Vec<Chunk>,
    /// i32 count + count * 128 bytes; unknown (empty on DOCK) [CODE size].
    pub block128: Vec<Vec<u8>>,
    /// Layer-0 objects (list at DAT_800b0f00), then layer-1 (DAT_800b0fb4).
    pub objects: Vec<Object>,
    pub layer0_count: usize,
    /// i32 N + (N+1) u32 file offsets (0 = empty) of objects (layer 0 on
    /// DOCK; layer 1 on some arenas) that
    /// get a 28-byte pose record at DAT_800b0c1c (slot i); these objects are
    /// moved by game code at run time (their centre, origin and angles change
    /// in RAM). Resolved to object indices. [CODE 0x8002e3dc] [RAM]
    pub special: Vec<Option<usize>>,
    pub special_offset: usize,
    pub descriptors: BTreeMap<u32, Descriptor>,
    pub meshes: BTreeMap<u32, Mesh>,
    /// Kind 2/3 models (none on DOCK).
    pub parts: BTreeMap<u32, PartsModel>,
    /// Data between referenced models that no descriptor points at (none on
    /// DOCK; some other tracks): parsed as a mesh when it is one.
    pub orphans: Vec<Orphan>,
    pub vis: Vec<VisZone>,
    /// Byte layout, for accounting (sorted, contiguous).
    pub sections: Vec<Section>,
}

fn parse_object_list(r: &R, mut p: usize, layer: Layer, out: &mut Vec<Object>) -> Result<usize, Error> {
    loop {
        let f = r.u8(p + 0xE)?;
        if f == 0 {
            r.need(p, 20)?;
            return Ok(p + 20);
        }
        out.push(Object {
            offset: p,
            layer,
            centre: [r.i32(p)?, r.i32(p + 4)?, r.i32(p + 8)?],
            radius: r.u16(p + 0xC)?,
            flags: f,
            b0f: r.u8(p + 0xF)?,
            desc_offset: r.u32(p + 0x10)?,
        });
        p += 20;
    }
}

impl Dat {
    pub fn parse(b: &[u8]) -> Result<Dat, Error> {
        let r = R(b);
        let mut sections = Vec::new();
        let n = r.i32(0)?;
        if !(0..=4096).contains(&n) {
            return err(0, format!("chunk count {n}"));
        }
        let mut p = 4;
        let mut chunks = Vec::new();
        for _ in 0..n {
            let count = r.i16(p)?;
            let id = r.i16(p + 2)?;
            let size = Chunk::size_for(count, id);
            r.need(p, size)?;
            let body = if id == 0x3F {
                ChunkBody::Records16(
                    (0..count.max(0) as usize)
                        .map(|i| b[p + 4 + 16 * i..p + 20 + 16 * i].try_into().unwrap())
                        .collect(),
                )
            } else if (0..=0x40).contains(&id) {
                let mut pts = Vec::new();
                for i in 0..count.max(0) as usize {
                    let q = p + 4 + 12 * i;
                    pts.push([r.i32(q)?, r.i32(q + 4)?, r.i32(q + 8)?]);
                }
                ChunkBody::Points(pts)
            } else {
                ChunkBody::Raw(b[p + 4..p + size].to_vec())
            };
            chunks.push(Chunk { offset: p, count, id, body });
            p += size;
        }
        sections.push(Section { name: "chunks", start: 0, end: p });

        let nb = r.i32(p)?;
        if !(0..=4096).contains(&nb) {
            return err(p, format!("block128 count {nb}"));
        }
        let mut block128 = Vec::new();
        for i in 0..nb as usize {
            r.need(p + 4 + 128 * i, 128)?;
            block128.push(b[p + 4 + 128 * i..p + 132 + 128 * i].to_vec());
        }
        let s = p;
        p += 4 + 128 * nb as usize;
        sections.push(Section { name: "block128", start: s, end: p });

        let mut objects = Vec::new();
        let s = p;
        p = parse_object_list(&r, p, Layer::World, &mut objects)?;
        let layer0_count = objects.len();
        sections.push(Section { name: "objects layer 0 (+terminator)", start: s, end: p });

        let special_offset = p;
        let ns = r.i32(p)?;
        if !(-1..=4096).contains(&ns) {
            return err(p, format!("special count {ns}"));
        }
        // Slots are resolved after both lists are read (they may point into
        // either list).
        let mut special_raw = Vec::new();
        for i in 0..(ns + 1).max(0) as usize {
            special_raw.push((p + 4 + 4 * i, r.u32(p + 4 + 4 * i)?));
        }
        p += 4 + 4 * (ns + 1).max(0) as usize;
        sections.push(Section { name: "special slots", start: special_offset, end: p });

        let s = p;
        p = parse_object_list(&r, p, Layer::Quarter, &mut objects)?;
        sections.push(Section { name: "objects layer 1 (+terminator)", start: s, end: p });

        let mut special = Vec::new();
        for (at, off) in special_raw {
            if off == 0 {
                special.push(None);
            } else {
                match objects.iter().position(|o| o.offset == off as usize) {
                    Some(k) => special.push(Some(k)),
                    None => return err(at, format!("special slot -> 0x{off:x} is not an object")),
                }
            }
        }

        let mut descriptors = BTreeMap::new();
        for o in &objects {
            let d = o.desc_offset as usize;
            if descriptors.contains_key(&o.desc_offset) {
                continue;
            }
            let lf = o.loaded_flags();
            let scaled = lf & flag::SCALED != 0;
            let kind = ModelKind::from_flags(lf).ok_or(Error { offset: o.offset, msg: format!("object flags 0x{:02x}: unknown model kind", o.flags) })?;
            let second = lf & flag::SECOND_MESH != 0;
            let desc = if scaled {
                Descriptor {
                    offset: d,
                    origin: [r.i32(d)?, r.i32(d + 4)?, r.i32(d + 8)?],
                    angles: [r.u16(d + 0xC)?, r.u16(d + 0xE)?, r.u16(d + 0x10)?],
                    scale: Some([r.i16(d + 0x12)?, r.i16(d + 0x14)?, r.i16(d + 0x16)?]),
                    b_lo: r.u8(d + 0x18)?,
                    b_hi: r.u8(d + 0x19)?,
                    b_extra: r.u16(d + 0x1A)?,
                    mesh: r.u32(d + 0x1C)?,
                    mesh2: if second { Some(r.u32(d + 0x20)?) } else { None },
                    kind,
                }
            } else {
                Descriptor {
                    offset: d,
                    origin: [r.i32(d)?, r.i32(d + 4)?, r.i32(d + 8)?],
                    angles: [r.u16(d + 0xC)?, r.u16(d + 0xE)?, r.u16(d + 0x10)?],
                    scale: None,
                    b_lo: r.u8(d + 0x12)?,
                    b_hi: r.u8(d + 0x13)?,
                    b_extra: 0,
                    mesh: r.u32(d + 0x14)?,
                    mesh2: if second { Some(r.u32(d + 0x18)?) } else { None },
                    kind,
                }
            };
            descriptors.insert(o.desc_offset, desc);
        }
        if let (Some(first), Some(last)) = (descriptors.values().next(), descriptors.values().last()) {
            sections.push(Section { name: "descriptors", start: first.offset, end: last.offset + last.size() });
        }

        let mut meshes = BTreeMap::new();
        let mut parts = BTreeMap::new();
        let mut model_ends = Vec::new();
        for d in descriptors.values() {
            for m in std::iter::once(d.mesh).chain(d.mesh2) {
                match d.kind {
                    ModelKind::Mesh if !meshes.contains_key(&m) => {
                        let x = parse_mesh(&r, m as usize)?;
                        model_ends.push((x.offset, x.end));
                        meshes.insert(m, x);
                    }
                    ModelKind::Parts if !parts.contains_key(&m) => {
                        let x = parse_parts(&r, m as usize)?;
                        model_ends.push((x.offset, x.end));
                        parts.insert(m, x);
                    }
                    _ => {}
                }
            }
        }
        model_ends.sort();
        let mut orphans = Vec::new();
        if let (Some(first), Some(last)) = (model_ends.first(), model_ends.last()) {
            // Bytes between referenced models: parse as unreferenced meshes
            // while that works, else keep raw.
            for w in model_ends.windows(2) {
                let (mut a, b2) = (w[0].1, w[1].0);
                if b2 < a {
                    return err(b2, "models overlap");
                }
                while a < b2 {
                    match parse_mesh(&r, a) {
                        Ok(m) if m.end <= b2 => {
                            let e = m.end;
                            orphans.push(Orphan { offset: a, end: e, mesh: Some(m) });
                            a = e;
                        }
                        _ => {
                            orphans.push(Orphan { offset: a, end: b2, mesh: None });
                            a = b2;
                        }
                    }
                }
            }
            sections.push(Section { name: "models (meshes)", start: first.0, end: last.1 });
        }

        // Visibility zones: one per 0x40 chunk, the first at len - last_i32.
        let mut vis = Vec::new();
        let tail = r.i32(b.len().saturating_sub(4))?;
        let zone_chunks: Vec<usize> = chunks.iter().enumerate().filter(|(_, c)| c.id == CHUNK_VIS_PATH).map(|(i, _)| i).collect();
        if tail != -1 && !zone_chunks.is_empty() {
            let mut vo = b.len() - tail as usize;
            let start = vo;
            for &ci in &zone_chunks {
                let c = &chunks[ci];
                let pts = match &c.body {
                    ChunkBody::Points(p) => p,
                    _ => unreachable!(),
                };
                if pts.len() < 2 {
                    return err(c.offset, "vis path chunk with < 2 points");
                }
                let mut mn = [i32::MAX; 3];
                let mut mx = [i32::MIN; 3];
                for pt in &pts[..2] {
                    for k in 0..3 {
                        mn[k] = mn[k].min(pt[k]);
                        mx[k] = mx[k].max(pt[k]);
                    }
                }
                let d: Vec<i32> = (0..3).map(|k| (mx[k] - mn[k]) >> 5).collect();
                let len = r.i16(vo)? as usize;
                if len < 0x104 {
                    return err(vo, format!("vis record length {len}"));
                }
                r.need(vo, len)?;
                vis.push(VisZone {
                    chunk: ci,
                    bbox_min: mn,
                    bbox_max: mx,
                    size2: d[0] * d[0] + d[1] * d[1] + d[2] * d[2],
                    offset: vo,
                    len,
                    b02: r.u16(vo + 2)?,
                    mask0: b[vo + 4..vo + 0x104].to_vec(),
                    stream: b[vo + 0x104..vo + len].to_vec(),
                });
                vo += len;
            }
            sections.push(Section { name: "visibility zones", start, end: vo });
            sections.push(Section { name: "padding + vis offset (i32)", start: vo, end: b.len() });
        } else {
            sections.push(Section { name: "tail i32 (-1: no vis)", start: b.len().saturating_sub(4), end: b.len() });
        }

        sections.sort_by_key(|s| s.start);
        Ok(Dat {
            len: b.len(),
            chunks,
            block128,
            objects,
            layer0_count,
            special,
            special_offset,
            descriptors,
            meshes,
            parts,
            orphans,
            vis,
            sections,
        })
    }

    /// Layer-0 then layer-1 slices.
    pub fn layer(&self, l: Layer) -> &[Object] {
        match l {
            Layer::World => &self.objects[..self.layer0_count],
            Layer::Quarter => &self.objects[self.layer0_count..],
        }
    }

    pub fn desc(&self, o: &Object) -> &Descriptor {
        &self.descriptors[&o.desc_offset]
    }

    /// The object's 8-byte-vertex mesh (None for kind 2/3/5 objects).
    pub fn mesh(&self, o: &Object) -> Option<&Mesh> {
        self.meshes.get(&self.desc(o).mesh)
    }

    /// Byte ranges not covered by any section (should be empty).
    pub fn gaps(&self) -> Vec<(usize, usize)> {
        let mut g = Vec::new();
        let mut p = 0;
        for s in &self.sections {
            if s.start > p {
                g.push((p, s.start));
            }
            p = p.max(s.end);
        }
        if p < self.len {
            g.push((p, self.len));
        }
        g
    }

    /// Model-to-world transform of an object: world = scale * (origin +
    /// (M * v) >> 12), in layer-0 (world) units.
    pub fn transform(&self, o: &Object, sin: &SinTable) -> Transform {
        let d = self.desc(o);
        Transform { m: d.matrix(sin), origin: d.origin, scale: o.layer.world_scale() }
    }

    /// Visibility zone for a camera eye in DAT layer-0 units (= kart/world
    /// units >> 5, toward zero; scratch 0x1F800384): among zones whose bbox
    /// contains the eye, the one with the smallest size2; else `prev` (the
    /// game keeps last frame's zone, 0 at start). [CODE 8005d908]
    pub fn vis_zone(&self, eye: [i32; 3], prev: usize) -> usize {
        let mut best = (i32::MAX, prev);
        for (i, z) in self.vis.iter().enumerate() {
            let inside = (0..3).all(|k| z.bbox_min[k] <= eye[k] && eye[k] <= z.bbox_max[k]);
            if inside && z.size2 < best.0 {
                best = (z.size2, i);
            }
        }
        best.1
    }

    /// Current path node of a zone: the node (zone chunk points 2..) nearest
    /// to the eye (DAT units), distance sum of ((p - eye) >> 4)^2, first
    /// minimum wins. The game then steps the mask node by node to it, so the
    /// mask in use is `VisZone::mask_at(node)`. [CODE 8005dc14] [RAM]
    pub fn vis_node(&self, zone: usize, eye: [i32; 3]) -> Option<usize> {
        let z = self.vis.get(zone)?;
        let pts = match &self.chunks[z.chunk].body {
            ChunkBody::Points(p) => p,
            _ => return None,
        };
        let mut best = (i64::MAX, None);
        for (k, p) in pts.iter().enumerate().skip(2) {
            let d: i64 = (0..3).map(|a| (((p[a] - eye[a]) >> 4) as i64).pow(2)).sum();
            if d < best.0 {
                best = (d, Some(k - 2));
            }
        }
        best.1
    }

    /// Objects the game draws this frame (FUN_80010000 for layer 0,
    /// FUN_8001031c for layer 1, called from FUN_80029f40) and the mesh each
    /// one draws. `eye`: camera eye in DAT layer-0 units (world >> 5);
    /// `cam`: the camera rotation, 4.12, rows right/down/forward (camera
    /// record +0x00, NOT the 1.6x-scaled GTE matrix); `mask`: the 256-byte
    /// visibility mask (`VisZone::mask_at(node)`; all 0xFF if the track has
    /// no zone). See DAT.md "Per-frame object selection". [CODE] [GTE]
    pub fn drawn(&self, eye: [i32; 3], cam: [[i16; 3]; 3], mask: &[u8], p: DrawParams) -> Vec<Drawn> {
        // RTIR: the offsets go through 16-bit IR registers (mtc2 keeps the
        // low half), then MAC = (R * IR) >> 12.
        let rot = |d: [i32; 3]| -> [i32; 3] {
            let mut o = [0i32; 3];
            for r in 0..3 {
                let s: i64 = (0..3).map(|c| cam[r][c] as i64 * (d[c] as i16) as i64).sum();
                o[r] = (s >> 12) as i32;
            }
            o
        };
        let sq = |v: i32| (v as i64) * (v as i64);
        let mut out = Vec::new();
        // Layer 0: mask bytes 0.. forward, MSB first.
        let far2 = sq((p.cd0 * 5) >> 4);
        let near2 = sq((p.de8 * 5) >> 4);
        for (i, o) in self.layer(Layer::World).iter().enumerate() {
            if mask.get(i / 8).map_or(true, |b| b & (0x80 >> (i % 8)) == 0) {
                continue;
            }
            let fl = o.loaded_flags();
            let r = (if o.flags & flag::B80 != 0 { o.radius & 0x7FFF } else { o.radius }) as i32;
            let d = [o.centre[0] - eye[0], o.centre[1] - eye[1], o.centre[2] - eye[2]];
            let h2 = sq(d[0] >> 4) + sq(d[2] >> 4) - sq(r >> 4);
            let v = rot(d);
            let (x, y, z) = (v[0].abs(), v[1].abs(), v[2]);
            let frustum = z + r > 0 && z + r >= x && (z * 3 >> 2) + r >= y;
            let list = if fl & 0x80 == 0 {
                if !(h2 < far2 && frustum && z - r < 4 * p.cd0 && fl != 5) {
                    continue;
                }
                if fl & 0x40 == 0 {
                    if h2 >= near2 {
                        continue;
                    }
                    DrawList::Near
                } else {
                    DrawList::Ground
                }
            } else {
                if !(h2 < 0x60_0000 && frustum) {
                    continue;
                }
                DrawList::Ground
            };
            out.push(Drawn { object: i, mesh: self.desc(o).mesh, layer: Layer::World, list });
        }
        // Layer 1: quarter units; mask bytes from 0xFF backwards, MSB first.
        let e4 = eye.map(|v| if v < 0 { (v + 3) >> 2 } else { v >> 2 });
        for (i, o) in self.layer(Layer::Quarter).iter().enumerate() {
            if mask.get(255 - i / 8).map_or(true, |b| b & (0x80 >> (i % 8)) == 0) {
                continue;
            }
            let fl = o.loaded_flags();
            let r = o.radius as i32;
            let v = rot([o.centre[0] - e4[0], o.centre[1] - e4[1], o.centre[2] - e4[2]]);
            let (x, y, z) = (v[0].abs(), v[1].abs(), v[2]);
            if fl & 0x80 != 0 {
                if z + r < 0 || z > 16000 {
                    continue;
                }
            } else if z < (p.cd0 >> 2) - r {
                // Near objects are left to layer 0: layer 1 is the far,
                // mostly untextured stand-in set.
                continue;
            }
            if z + r >= x && (z * 3 >> 2) + r >= y && z - r < 4 * p.b00 && fl != 5 {
                out.push(Drawn { object: self.layer0_count + i, mesh: self.desc(o).mesh, layer: Layer::Quarter, list: DrawList::Near });
            }
        }
        out
    }

    /// Size of the loader's output (special records + object records), the
    /// value FUN_8002e3dc returns. [CODE]
    pub fn loader_output_size(&self) -> usize {
        self.objects.len() * 0x28 + self.special.len() * 0x1C
    }

    /// Reproduce FUN_8002e3dc on a copy of the file loaded at `base`:
    /// returns (file image after in-place fix-ups, output block that the
    /// loader writes at base + len: special records then object records)
    /// and a mask of output bytes the loader defines. Chunks with id > 0x40
    /// (in-place s16 conversion + normal) are not reproduced.
    pub fn loader_image(&self, raw: &[u8], base: u32, sin: &SinTable) -> LoaderImage {
        let mut f = raw.to_vec();
        let nsp = self.special.len();
        let out_base = base + raw.len() as u32;
        let rec0 = out_base + 0x1C * nsp as u32;
        let mut out = vec![0u8; self.loader_output_size()];
        let mut defined = vec![false; out.len()];
        let w16 = |v: &mut Vec<u8>, o: usize, x: u16| v[o..o + 2].copy_from_slice(&x.to_le_bytes());
        let w32 = |v: &mut Vec<u8>, o: usize, x: u32| v[o..o + 4].copy_from_slice(&x.to_le_bytes());
        let mut done_desc = std::collections::HashSet::new();
        for (i, o) in self.objects.iter().enumerate() {
            let p = o.offset;
            let fl = o.loaded_flags();
            f[p + 0xE] = fl;
            if o.flags & flag::B80 != 0 && o.layer == Layer::World {
                let r = le::u16(&f, p + 0xC) & 0x7FFF;
                w16(&mut f, p + 0xC, r);
            }
            let d = self.desc(o);
            if done_desc.insert(d.offset) {
                let (mo, m2o, blo, bhi) = if d.scale.is_some() { (0x1C, 0x20, 0x18, 0x19) } else { (0x14, 0x18, 0x12, 0x13) };
                w32(&mut f, d.offset + mo, base + d.mesh);
                if let Some(m2) = d.mesh2 {
                    w32(&mut f, d.offset + m2o, base + m2);
                }
                f[d.offset + blo] = d.b_lo & 0xF;
                if matches!(fl & 0x6F, 1 | 4 | 0x41 | 0x44 | 9 | 0xC | 0x49 | 0x4C) {
                    f[d.offset + bhi] = d.b_hi_loaded();
                }
            }
            let rec = rec0 + 0x28 * i as u32;
            w32(&mut f, p + 0x10, rec);
            let ro = 0x1C * nsp + 0x28 * i;
            let m = d.matrix(sin);
            for r in 0..3 {
                for c in 0..3 {
                    w16(&mut out, ro + 2 * (3 * r + c), m[r][c] as u16);
                }
            }
            defined[ro..ro + 18].iter_mut().for_each(|x| *x = true);
            w32(&mut out, ro + 0x14, base + d.offset as u32);
            defined[ro + 0x14..ro + 0x20].iter_mut().for_each(|x| *x = true);
            let slots = match d.kind {
                ModelKind::Mesh => self.meshes[&d.mesh].slot_count() as u16,
                ModelKind::Parts => self.parts[&d.mesh].record_count() as u16,
                ModelKind::None => 0,
            };
            w16(&mut out, ro + 0x20, slots);
            defined[ro + 0x20..ro + 0x22].iter_mut().for_each(|x| *x = true);
            if o.layer == Layer::Quarter {
                defined[ro + 0x26..ro + 0x28].iter_mut().for_each(|x| *x = true);
            }
        }
        // Special slots: relocate, then fill each 28-byte record with a copy
        // of the object's pose from its descriptor: origin (+0x00, 3 x i32),
        // descriptor +0x12..0x18 (+0x0c, scale on scaled descriptors), the
        // object flags (+0x12, u16), angles (+0x14, 3 x u16); +0x1a..0x1c
        // untouched. [CODE 0x8002e3dc tail loop]
        for (i, s) in self.special.iter().enumerate() {
            let ro = 0x1C * i;
            if let Some(k) = s {
                let o = &self.objects[*k];
                w32(&mut f, self.special_offset + 4 + 4 * i, base + o.offset as u32);
                w16(&mut out, ro + 0x12, o.loaded_flags() as u16);
                let d = self.desc(o).offset;
                let src = |k: usize| le::u16(&f, d + k);
                let mut put = Vec::new();
                for k in 0..6 {
                    put.push((2 * k, src(2 * k)));
                }
                for (dst, s_) in [(0x14, 0xC), (0x16, 0xE), (0x18, 0x10), (0x0C, 0x12), (0x0E, 0x14), (0x10, 0x16)] {
                    put.push((dst, src(s_)));
                }
                for (dst, v) in put {
                    w16(&mut out, ro + dst, v);
                }
                defined[ro..ro + 0x1A].iter_mut().for_each(|x| *x = true);
            }
        }
        LoaderImage { file: f, out, defined }
    }
}

/// Per-frame draw-distance globals. [CODE 8002e2f0, 80029f40] [RAM]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DrawParams {
    /// DAT_800b0cd0: far distance (written each frame by FUN_80029f40).
    pub cd0: i32,
    /// DAT_800b0de8: detail distance for layer-0 objects without flag 0x40
    /// (= DAT_800b0cd4 - 0x180 at load).
    pub de8: i32,
    /// DAT_800b0b00 = 0x4AF - DAT_800a8840: layer-1 far distance.
    pub b00: i32,
}

impl DrawParams {
    /// DOCK, 1-player race (RAM, 2026-10-12): 2386, 2002, 1139.
    pub const DOCK_1P: DrawParams = DrawParams { cd0: 2386, de8: 2002, b00: 1139 };

    /// A 1-player race on a course whose flags (u16 at 0x800ACB3A +
    /// 12 * course) are `flags`: FUN_8002e2f0 sets cd4 = 0x952 with flag 4,
    /// else 5000; cd0 = cd4, de8 = cd4 - 0x180, b00 = 0x4AF - DAT_800a8840.
    /// [CODE 8002e2f0] [RAM: all 18 course captures, cd0 / de8 = 2386 / 2002
    /// or 5000 / 4616]
    pub fn race_1p(flags: u16) -> DrawParams {
        let cd4 = if flags & 4 != 0 { 0x952 } else { 5000 };
        DrawParams { cd0: cd4, de8: cd4 - 0x180, b00: 1139 }
    }

    /// The course flags u16 at 0x800ACB3A + 12 * course from the executable.
    pub fn course_flags(exe: &[u8], course: usize) -> Option<u16> {
        if exe.len() < 0x800 || &exe[0..8] != b"PS-X EXE" {
            return None;
        }
        let t_addr = u32::from_le_bytes(exe[0x18..0x1C].try_into().ok()?);
        let off = (0x800A_CB3A - t_addr) as usize + 0x800 + 12 * course;
        Some(u16::from_le_bytes(exe.get(off..off + 2)?.try_into().ok()?))
    }
}

/// Which draw list FUN_80010000 put an object in: 0x800D1DD0 (objects
/// without flag 0x40, and all of layer 1) or 0x800D6890 (flag 0x40/0x80).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawList {
    Near,
    Ground,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Drawn {
    /// Index into `Dat::objects`.
    pub object: usize,
    /// Mesh file offset (`Descriptor::mesh`).
    pub mesh: u32,
    pub layer: Layer,
    pub list: DrawList,
}

// ---------------------------------------------------------------------------
// Start/finish banner (point chunk id 9)
// ---------------------------------------------------------------------------

/// Chunk id 9: banner end points, two per banner (DOCK: one banner across
/// the start/finish line). [CODE 80034ce0]
pub const CHUNK_BANNER: i16 = 9;
/// Banner texture: slot 0x15, CLUT + 8, tpage + 0x60 (opaque); 4-bit page.
/// [CODE 800351e8] [GPU]
pub const BANNER_TEX_SLOT: u8 = 0x15;
pub const BANNER_CLUT_ADD: u16 = 8;
pub const BANNER_TPAGE_ADD: u16 = 0x60;
/// Phase increments per race frame (FUN_80035004, banner type 1).
pub const BANNER_PHASE_STEP: (u16, u16) = (0x37, 0x3E);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Banner {
    /// The two chunk points (DAT layer-0 units).
    pub ends: [[i32; 3]; 2],
    /// (p0 + p1) / 2 per axis, C division (the draw origin).
    pub mid: [i32; 3],
    /// ratan2(dx, dz) + 0x400, 12-bit (rotation about y of the cloth grid).
    pub angle: u16,
    /// sqrt(dx^2 + dz^2) / (cols - 1): this banner's grid spacing.
    pub spacing: i16,
}

/// The waving cloth banners of a track (FUN_80034ce0 at track start; drawn
/// by FUN_800351e8 -> FUN_800313bc each frame, animated by FUN_80035004).
#[derive(Debug, Clone)]
pub struct Banners {
    pub banners: Vec<Banner>,
    /// Grid size: 11 x 3 vertices (10 x 2 quads).
    pub cols: usize,
    pub rows: usize,
}

/// One cloth vertex after FUN_80035004: local x, y, z (z = wave) and the
/// grey shade (0x80..0xA0) used as its colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClothVertex {
    pub pos: [i16; 3],
    pub shade: u8,
}

/// One banner quad in polygon order (corners (c,r), (c+1,r), (c+1,r+1),
/// (c,r+1)); GPU order 0, 1, 3, 2. World DAT layer-0 units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BannerQuad {
    pub pos: [[i32; 3]; 4],
    pub uv: [[u8; 2]; 4],
    pub shade: [u8; 4],
}

fn isqrt(v: i64) -> i64 {
    if v <= 0 {
        return 0;
    }
    let mut x = (v as f64).sqrt() as i64;
    while x * x > v {
        x -= 1;
    }
    while (x + 1) * (x + 1) <= v {
        x += 1;
    }
    x
}

/// atan2 as a 12-bit angle (PSY-Q ratan2(y, x)) [INFER: the library is
/// table based; this matches DOCK's banner exactly, checked via the GTE].
pub fn ratan2(y: i32, x: i32) -> i32 {
    let a = (y as f64).atan2(x as f64) * 2048.0 / std::f64::consts::PI;
    a.round() as i32
}

impl Dat {
    /// FUN_80034ce0 (all courses but 0x1E): banners from chunk id 9 when its
    /// point count is even and the pair count < 3.
    pub fn banners(&self) -> Option<Banners> {
        let c = self.chunks.iter().find(|c| c.id == CHUNK_BANNER)?;
        let pts = match &c.body {
            ChunkBody::Points(p) => p,
            _ => return None,
        };
        let n = c.count as usize;
        if n == 0 || n & 1 != 0 || n / 2 >= 3 {
            return None;
        }
        let (cols, rows) = (11usize, 3usize);
        let mut banners = Vec::new();
        for i in 0..n / 2 {
            let (p0, p1) = (pts[2 * i], pts[2 * i + 1]);
            let dx = (p1[0] - p0[0]) as i16 as i32;
            let dz = (p1[2] - p0[2]) as i16 as i32;
            let angle = ((ratan2(dx, dz) + 0x400) & 0xFFF) as u16;
            let len = isqrt((dx as i64) * (dx as i64) + (dz as i64) * (dz as i64)) as i32;
            let spacing = (len / (cols as i32 - 1)) as i16;
            let mid = [(p0[0] + p1[0]) / 2, (p0[1] + p1[1]) / 2, (p0[2] + p1[2]) / 2];
            banners.push(Banner { ends: [p0, p1], mid, angle, spacing });
        }
        Some(Banners { banners, cols, rows })
    }
}

impl Banners {
    /// The cloth phase (a, b) after `ticks` race frames from `start`.
    pub fn phase_after(start: (u16, u16), ticks: u32) -> (u16, u16) {
        (
            ((start.0 as u32 + BANNER_PHASE_STEP.0 as u32 * ticks) & 0xFFF) as u16,
            ((start.1 as u32 + BANNER_PHASE_STEP.1 as u32 * ticks) & 0xFFF) as u16,
        )
    }

    /// FUN_80035004 (type 1) on the rest grid built from banner 0's spacing
    /// s: x = (s - cols*s)/2 + c*s, y = (s - rows*s)/2 + r*s, z = wave:
    /// v = (sin(a + 0x179 r) * sin(b + 500 c)) >> 14 (table units, 1.0 =
    /// 16384), z = v >> 6, shade = 0xA0 - |v >> 7|. `phase` = (DAT_800afee4,
    /// DAT_800afee6) after this frame's step. Row-major, cols x rows.
    pub fn cloth(&self, sin: &SinTable, phase: (u16, u16)) -> Vec<ClothVertex> {
        let s = self.banners.first().map_or(0, |b| b.spacing) as i32;
        let x0 = (s - self.cols as i32 * s) / 2;
        let y0 = (s - self.rows as i32 * s) / 2;
        let mut out = Vec::with_capacity(self.cols * self.rows);
        for r in 0..self.rows {
            let u = (phase.0 as u32 + 0x179 * r as u32) & 0xFFF;
            for c in 0..self.cols {
                let w = (phase.1 as u32 + 500 * c as u32) & 0xFFF;
                let p = sin.0[u as usize] as i32 * sin.0[w as usize] as i32;
                let v = ((if p < 0 { p + 0x3FFF } else { p }) >> 14) as i16 as i32;
                let dz = (if v < 0 { v + 0x3F } else { v }) >> 6;
                let sh = ((if v < 0 { v + 0x7F } else { v }) >> 7).abs();
                out.push(ClothVertex {
                    pos: [(x0 + c as i32 * s) as i16, (y0 + r as i32 * s) as i16, dz as i16],
                    shade: (0xA0 - sh) as u8,
                });
            }
        }
        out
    }

    /// FUN_800351e8 (type 1): banner `i`'s quads in world DAT units. Banners
    /// after the first scale the cloth by spacing_i / spacing_0 (C division).
    /// Columns 1, 2, cols-4, cols-3 show the face tile (u 0 on columns 1 and
    /// cols-4, else 0x20; v 0x80 on row 0, else 0xA0); the rest the checker
    /// tile (u 0x40, v 0x10); each cell spans 31 texels.
    pub fn quads(&self, i: usize, cloth: &[ClothVertex], sin: &SinTable) -> Vec<BannerQuad> {
        let b = self.banners[i];
        let s0 = self.banners[0].spacing as i32;
        let (sn, cs) = (sin.sin12(b.angle), sin.cos12(b.angle));
        let tz = |a: i32| if a < 0 { (a + 0xFFF) >> 12 } else { a >> 12 };
        let world = |v: &ClothVertex| -> [i32; 3] {
            let mut p = [v.pos[0] as i32, v.pos[1] as i32, v.pos[2] as i32];
            if i > 0 && s0 != 0 {
                for k in 0..3 {
                    p[k] = p[k] * b.spacing as i32 / s0;
                }
            }
            let x = tz(p[0] * cs + p[2] * sn);
            let z = tz(-p[0] * sn + p[2] * cs);
            [b.mid[0] + x, b.mid[1] + p[1], b.mid[2] + z]
        };
        let mut out = Vec::new();
        let cols = self.cols;
        for r in 0..self.rows - 1 {
            for c in 0..cols - 1 {
                let face = (1..=2).contains(&c) || c == cols - 4 || c == cols - 3;
                let (u0, v0) = if face {
                    (if c == 1 || c == cols - 4 { 0u8 } else { 0x20 }, if r == 0 { 0x80u8 } else { 0xA0 })
                } else {
                    (0x40u8, 0x10u8)
                };
                let idx = [r * cols + c, r * cols + c + 1, (r + 1) * cols + c + 1, (r + 1) * cols + c];
                out.push(BannerQuad {
                    pos: idx.map(|k| world(&cloth[k])),
                    uv: [[u0, v0], [u0 + 31, v0], [u0 + 31, v0 + 31], [u0, v0 + 31]],
                    shade: idx.map(|k| cloth[k].shade),
                });
            }
        }
        out
    }

    /// Drawn this frame: the banner's midpoint within 0x2000 DAT units of the
    /// eye on every axis (eye = camera eye >> 5). [CODE 800351e8, 800313bc]
    pub fn visible(&self, i: usize, eye: [i32; 3]) -> bool {
        let m = self.banners[i].mid;
        (0..3).all(|k| (m[k] as i64 - eye[k] as i64 + 0x2000) as u64 <= 0x4000)
    }
}

pub struct LoaderImage {
    pub file: Vec<u8>,
    pub out: Vec<u8>,
    pub defined: Vec<bool>,
}

/// Object-to-world transform (layer-0 units).
#[derive(Debug, Clone, Copy)]
pub struct Transform {
    pub m: [[i16; 3]; 3],
    pub origin: [i32; 3],
    pub scale: i32,
}

impl Transform {
    /// Integer world position of a mesh vertex.
    pub fn apply(&self, v: [i16; 3]) -> [i32; 3] {
        let mut w = [0i32; 3];
        for r in 0..3 {
            let s: i64 = (0..3).map(|c| self.m[r][c] as i64 * v[c] as i64).sum();
            w[r] = self.scale * (self.origin[r] + (s >> 12) as i32);
        }
        w
    }
    /// Same in f64 without the >> 12 truncation (for export).
    pub fn apply_f(&self, v: [i16; 3]) -> [f64; 3] {
        let mut w = [0f64; 3];
        for r in 0..3 {
            let s: f64 = (0..3).map(|c| self.m[r][c] as f64 * v[c] as f64).sum::<f64>() / 4096.0;
            w[r] = self.scale as f64 * (self.origin[r] as f64 + s);
        }
        w
    }
}

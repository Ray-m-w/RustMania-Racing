//! ter: track terrain / collision data (`<track>.TER`), plus the game's
//! ground-height and wall-crossing queries reimplemented in integer Rust.
//!
//! Loading: FUN_8004627c reads `<track>.ter` (FUN_80042068) to the heap at
//! DAT_800b0b3c and calls FUN_8003c2d8, which only turns the header's section
//! offsets into absolute pointers and copies header words into globals. The
//! file is used in place, unmodified (proven: RAM == disc byte for byte on
//! DOCK, see docs/spec/formats/TER.md "Verification").
//!
//! Queries ported here (behavior read from the MIPS code of the EXE; the
//! recomp's generated C is a translation of the same instructions):
//! - [`Ter::ground`]        = FUN_8003ddb0 (floor / water surface under a point)
//! - [`Ter::face_height`]   = FUN_8003cf14 (height of a triangle at x,z)
//! - [`Ter::wall_cross`]    = FUN_8003d760 (first wall edge a move crosses)
//! - [`Ter::wall_candidates`] = the geometric part of FUN_8003da18 (it then
//!   calls FUN_8003d240 for the wall height test and the slide response,
//!   which depend on kart state and are not ported here)
//! - [`Ter::set_group_enabled`] = FUN_800421c4 / FUN_8004229c,
//!   [`Ter::move_group_y`] = FUN_80042374 (moving / switchable pieces)
//!
//! Units. "World" units are the kart's position units (kart record +0/+4/+8).
//! TER coordinates are world >> 5 (arithmetic). The PS1 Y axis points DOWN:
//! a larger y is lower. Heights returned by the queries are world units.
//! All arithmetic is i32 with wrapping multiply/add and truncating division,
//! exactly as the R3000 code does it (mult/mflo, div/mflo).

use crate::le;

#[derive(Debug)]
pub enum Error {
    Short { need: usize, have: usize },
    Layout(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Short { need, have } => write!(f, "TER too short: need {need} bytes, have {have}"),
            Error::Layout(s) => write!(f, "TER layout: {s}"),
        }
    }
}
impl std::error::Error for Error {}

fn bad<T>(s: impl Into<String>) -> Result<T, Error> {
    Err(Error::Layout(s.into()))
}

/// Header length: 33 little-endian i32 words.
pub const HEADER_LEN: usize = 0x84;

/// Face flag bits (face byte +0x0E).
pub const FACE_QUAD: u8 = 0x80;
pub const FACE_ALT: u8 = 0x40;
pub const FACE_DISABLED: u8 = 0x20;
/// Face kind (byte +0x0F) the ground query treats as a water surface.
pub const KIND_WATER: u8 = 3;

/// One terrain vertex, TER units (12 bytes: i32 x, y, z).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Vertex {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

/// One floor polygon (16 bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Face {
    /// Vertex indices (file stores byte offsets into the vertex section,
    /// i.e. index * 12). `v[3]` is meaningful only for quads.
    pub v: [u16; 4],
    /// Unit normal, 1.0 = 4096, pointing up (-y) for floors. Copied (smoothed)
    /// into the kart's up vector by FUN_8003e91c.
    pub normal: [i16; 3],
    /// FACE_QUAD | FACE_ALT | FACE_DISABLED (low 5 bits always 0 on disc).
    pub flags: u8,
    /// Surface kind; copied to kart +0x4F3. 3 = water surface.
    pub kind: u8,
}

impl Face {
    pub fn is_quad(&self) -> bool {
        self.flags & FACE_QUAD != 0
    }
    pub fn disabled(&self) -> bool {
        self.flags & FACE_DISABLED != 0
    }
    pub fn alt(&self) -> bool {
        self.flags & FACE_ALT != 0
    }
    pub fn corners(&self) -> &[u16] {
        if self.is_quad() { &self.v } else { &self.v[..3] }
    }
}

/// Wall end point (16 bytes, TER units). y_top < y_bottom (y is down).
/// When y_top == y_bottom at both ends of an edge the game skips the
/// height test: the wall is infinitely tall.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WallPoint {
    pub x: i32,
    pub z: i32,
    pub y_top: i32,
    pub y_bottom: i32,
}

/// Wall edge (4 bytes: two u16 byte offsets into the wall point section;
/// bit 0 of the first is the "disabled" flag).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wall {
    /// Wall point index from the first u16 ("B" in the spec).
    pub p0: u16,
    /// Wall point index from the second u16 ("A" in the spec).
    pub p1: u16,
    pub disabled: bool,
}

/// A switchable / movable piece of terrain (20 bytes). Ranges index the
/// vertex, face and wall sections. Zero on DOCK.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Group {
    pub unk0: u32,
    pub vert_start: u16,
    pub face_start: u16,
    pub wall_start: u16,
    pub vert_count: i16,
    pub face_count: i16,
    pub wall_count: i16,
    pub unk16: u32,
}

/// Racing-line point (16 bytes). TER units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PathPoint {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    /// Direction of travel, 4096 = full turn (read by FUN_80090424/80090670).
    pub heading: u16,
    /// Two bytes, unknown use (likely left/right half widths). [INFER]
    pub w: [u8; 2],
}

/// Racing-line segment (36 bytes): a run of path points plus graph links.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PathSegment {
    pub first: i16,
    pub count: i16,
    /// Index into `links`: `n_next` successor segment ids then `n_prev`
    /// predecessor ids.
    pub link_start: i16,
    pub n_next: i16,
    pub n_prev: i16,
    pub flags: i16,
    pub bb_min: [i32; 3],
    pub bb_max: [i32; 3],
}

/// The decoded file. Section order on disc is the field order below.
#[derive(Debug, Clone)]
pub struct Ter {
    pub grid_w: i32,
    pub grid_h: i32,
    /// Section byte offsets from the header (words 2..=13).
    pub offsets: [u32; 12],
    /// Bounding box, TER units (words 14..=19: min x,y,z then max x,y,z).
    pub min: [i32; 3],
    pub max: [i32; 3],
    /// Grid cell size in x, total y span (unused by code), cell size in z.
    pub cell_x: i32,
    pub span_y: i32,
    pub cell_z: i32,
    /// Default "no water" height, TER units (word 29; the query starts its
    /// water minimum at this << 5).
    pub no_water_y: i32,
    pub vertices: Vec<Vertex>,
    pub faces: Vec<Face>,
    pub wall_points: Vec<WallPoint>,
    pub walls: Vec<Wall>,
    pub groups: Vec<Group>,
    /// Per-cell lists, raw u16 entries. A cell's list starts at
    /// `cell_list_start[cell]` and holds `cell_floors[cell]` face byte
    /// offsets followed by `cell_walls[cell]` wall byte offsets.
    pub cell_list: Vec<u16>,
    /// Start of each cell's list, as an index into `cell_list` (file: bytes).
    pub cell_list_start: Vec<u16>,
    pub cell_floors: Vec<u8>,
    pub cell_walls: Vec<u8>,
    pub path_points: Vec<PathPoint>,
    pub path_segments: Vec<PathSegment>,
    pub links: Vec<u8>,
    /// Bytes after the last list entry / last link (alignment padding).
    pub pad_list: usize,
    pub pad_links: usize,
}

/// One surface found by [`Ter::ground`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceHit {
    pub face: usize,
    /// World units.
    pub y: i32,
}

/// Result of [`Ter::ground`] (the globals FUN_8003ddb0 writes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ground {
    /// Cell column / row (0x800b0d64 / 0x800b0d6c); -1 when below the
    /// grid minimum. Not written-through range checked.
    pub cell_x: i32,
    pub cell_z: i32,
    /// True when the point is inside the grid (otherwise every Option is
    /// None and the heights are the defaults).
    pub in_grid: bool,
    /// The chosen height (0x800b0c00): the closest to the query y among
    /// `nearest_y`, `alt` and `water_y`.
    pub height: i32,
    /// Closest normal surface to the query y (0x800b0b0c / 0x800b0b24).
    /// `nearest_y` defaults to max_y << 5 when nothing is found.
    pub nearest: Option<SurfaceHit>,
    pub nearest_y: i32,
    /// Closest normal surface at or below the query y (y >= query)
    /// (0x800b0ae0 / 0x800b0b04); falls back to `nearest` when none.
    pub below: Option<SurfaceHit>,
    /// Closest FACE_ALT surface (0x800b0c84 / 0x800b0c9c).
    pub alt: Option<SurfaceHit>,
    /// Highest water surface (kind 3) (0x800b0f8c / 0x800b0a54).
    pub water: Option<SurfaceHit>,
    pub water_y: i32,
    /// Any normal surface was found (0x800b0ae4).
    pub found: bool,
}

impl Ter {
    pub fn parse(b: &[u8]) -> Result<Ter, Error> {
        if b.len() < HEADER_LEN {
            return Err(Error::Short { need: HEADER_LEN, have: b.len() });
        }
        let h = |i: usize| le::i32(b, 4 * i);
        let grid_w = h(0);
        let grid_h = h(1);
        if !(1..=256).contains(&grid_w) || !(1..=256).contains(&grid_h) {
            return bad(format!("grid {grid_w}x{grid_h}"));
        }
        let mut offsets = [0u32; 12];
        for (i, o) in offsets.iter_mut().enumerate() {
            *o = h(2 + i) as u32;
        }
        if offsets[0] as usize != HEADER_LEN {
            return bad("vertex section does not follow the header");
        }
        for i in 0..11 {
            if offsets[i + 1] < offsets[i] {
                return bad(format!("section {} offset goes backwards", i + 1));
            }
        }
        if offsets[11] as usize > b.len() {
            return Err(Error::Short { need: offsets[11] as usize, have: b.len() });
        }
        let end = |i: usize| if i == 11 { b.len() } else { offsets[i + 1] as usize };
        let start = |i: usize| offsets[i] as usize;
        let count = |i: usize| h(23 + i);
        let cnt = |i: usize, what: &str| -> Result<usize, Error> {
            let c = count(i);
            if c < 0 {
                return bad(format!("negative {what} count"));
            }
            Ok(c as usize)
        };
        let n_vert = cnt(0, "vertex")?;
        let n_face = cnt(1, "face")?;
        let n_wpt = cnt(2, "wall point")?;
        let n_wall = cnt(3, "wall")?;
        let n_group = cnt(4, "group")?;
        let n_list = cnt(5, "cell list")?;
        let no_water_y = count(6);
        let n_path = cnt(7, "path point")?;
        let n_seg = cnt(8, "path segment")?;
        let n_link = cnt(9, "link")?;
        let cells = (grid_w * grid_h) as usize;

        let exact = |i: usize, n: usize, what: &str| -> Result<(), Error> {
            if end(i) - start(i) != n {
                return bad(format!("{what} section is {} bytes, expected {n}", end(i) - start(i)));
            }
            Ok(())
        };
        exact(0, 12 * n_vert, "vertex")?;
        exact(1, 16 * n_face, "face")?;
        exact(2, 16 * n_wpt, "wall point")?;
        exact(3, 4 * n_wall, "wall")?;
        exact(4, 20 * n_group, "group")?;
        let list_bytes = end(5) - start(5);
        if list_bytes < 2 * n_list || list_bytes > 2 * n_list + 3 {
            return bad("cell list section size");
        }
        exact(6, 2 * cells, "cell offset")?;
        exact(7, cells, "cell floor count")?;
        exact(8, cells, "cell wall count")?;
        exact(9, 16 * n_path, "path point")?;
        exact(10, 36 * n_seg, "path segment")?;
        let link_bytes = end(11) - start(11);
        if link_bytes < n_link || link_bytes > n_link + 3 {
            return bad("link section size");
        }

        let vertices: Vec<Vertex> = (0..n_vert)
            .map(|k| {
                let o = start(0) + 12 * k;
                Vertex { x: le::i32(b, o), y: le::i32(b, o + 4), z: le::i32(b, o + 8) }
            })
            .collect();
        let mut faces = Vec::with_capacity(n_face);
        for k in 0..n_face {
            let o = start(1) + 16 * k;
            let flags = b[o + 14];
            let mut v = [0u16; 4];
            for (j, vj) in v.iter_mut().enumerate() {
                let off = le::u16(b, o + 2 * j);
                let used = j < 3 || flags & FACE_QUAD != 0;
                if off % 12 != 0 || (used && off as usize / 12 >= n_vert) {
                    return bad(format!("face {k} vertex {j} offset {off}"));
                }
                *vj = off / 12;
            }
            faces.push(Face {
                v,
                normal: [le::i16(b, o + 8), le::i16(b, o + 10), le::i16(b, o + 12)],
                flags,
                kind: b[o + 15],
            });
        }
        let wall_points: Vec<WallPoint> = (0..n_wpt)
            .map(|k| {
                let o = start(2) + 16 * k;
                WallPoint {
                    x: le::i32(b, o),
                    z: le::i32(b, o + 4),
                    y_top: le::i32(b, o + 8),
                    y_bottom: le::i32(b, o + 12),
                }
            })
            .collect();
        let mut walls = Vec::with_capacity(n_wall);
        for k in 0..n_wall {
            let o = start(3) + 4 * k;
            let a = le::u16(b, o);
            let c = le::u16(b, o + 2);
            let (a0, dis) = (a & !1, a & 1 != 0);
            if a0 % 16 != 0 || c % 16 != 0 || a0 as usize / 16 >= n_wpt || c as usize / 16 >= n_wpt {
                return bad(format!("wall {k} offsets {a} {c}"));
            }
            walls.push(Wall { p0: a0 / 16, p1: c / 16, disabled: dis });
        }
        let mut groups = Vec::with_capacity(n_group);
        for k in 0..n_group {
            let o = start(4) + 20 * k;
            let g = Group {
                unk0: le::u32(b, o),
                vert_start: le::u16(b, o + 4),
                face_start: le::u16(b, o + 6),
                wall_start: le::u16(b, o + 8),
                vert_count: le::i16(b, o + 10),
                face_count: le::i16(b, o + 12),
                wall_count: le::i16(b, o + 14),
                unk16: le::u32(b, o + 16),
            };
            if g.vert_start % 12 != 0 || g.face_start % 16 != 0 || g.wall_start % 4 != 0 {
                return bad(format!("group {k} offsets"));
            }
            groups.push(Group {
                vert_start: g.vert_start / 12,
                face_start: g.face_start / 16,
                wall_start: g.wall_start / 4,
                ..g
            });
        }
        let cell_list: Vec<u16> = (0..n_list).map(|k| le::u16(b, start(5) + 2 * k)).collect();
        let pad_list = list_bytes - 2 * n_list;
        let mut cell_list_start = Vec::with_capacity(cells);
        for k in 0..cells {
            let off = le::u16(b, start(6) + 2 * k);
            if off % 2 != 0 {
                return bad(format!("cell {k} list offset {off} is odd"));
            }
            cell_list_start.push(off / 2);
        }
        let cell_floors = b[start(7)..start(7) + cells].to_vec();
        let cell_walls = b[start(8)..start(8) + cells].to_vec();
        for c in 0..cells {
            let s = cell_list_start[c] as usize;
            let nf = cell_floors[c] as usize;
            let nw = cell_walls[c] as usize;
            if s + nf + nw > n_list {
                return bad(format!("cell {c} list overruns"));
            }
            for &e in &cell_list[s..s + nf] {
                if e % 16 != 0 || e as usize / 16 >= n_face {
                    return bad(format!("cell {c} floor entry {e}"));
                }
            }
            for &e in &cell_list[s + nf..s + nf + nw] {
                if e % 4 != 0 || e as usize / 4 >= n_wall {
                    return bad(format!("cell {c} wall entry {e}"));
                }
            }
        }
        let path_points = (0..n_path)
            .map(|k| {
                let o = start(9) + 16 * k;
                PathPoint {
                    x: le::i32(b, o),
                    y: le::i32(b, o + 4),
                    z: le::i32(b, o + 8),
                    heading: le::u16(b, o + 12),
                    w: [b[o + 14], b[o + 15]],
                }
            })
            .collect();
        let path_segments = (0..n_seg)
            .map(|k| {
                let o = start(10) + 36 * k;
                PathSegment {
                    first: le::i16(b, o),
                    count: le::i16(b, o + 2),
                    link_start: le::i16(b, o + 4),
                    n_next: le::i16(b, o + 6),
                    n_prev: le::i16(b, o + 8),
                    flags: le::i16(b, o + 10),
                    bb_min: [le::i32(b, o + 12), le::i32(b, o + 16), le::i32(b, o + 20)],
                    bb_max: [le::i32(b, o + 24), le::i32(b, o + 28), le::i32(b, o + 32)],
                }
            })
            .collect();
        let links = b[start(11)..start(11) + n_link].to_vec();
        Ok(Ter {
            grid_w,
            grid_h,
            offsets,
            min: [h(14), h(15), h(16)],
            max: [h(17), h(18), h(19)],
            cell_x: h(20),
            span_y: h(21),
            cell_z: h(22),
            no_water_y,
            vertices,
            faces,
            wall_points,
            walls,
            groups,
            cell_list,
            cell_list_start,
            cell_floors,
            cell_walls,
            path_points,
            path_segments,
            links,
            pad_list,
            pad_links: link_bytes - n_link,
        })
    }

    /// Face indices listed in a cell (floor part of its list).
    pub fn cell_faces(&self, cell: usize) -> impl Iterator<Item = usize> + '_ {
        let s = self.cell_list_start[cell] as usize;
        let n = self.cell_floors[cell] as usize;
        self.cell_list[s..s + n].iter().map(|&e| e as usize / 16)
    }

    /// Wall indices listed in a cell (wall part of its list).
    pub fn cell_wall_list(&self, cell: usize) -> impl Iterator<Item = usize> + '_ {
        let s = self.cell_list_start[cell] as usize + self.cell_floors[cell] as usize;
        let n = self.cell_walls[cell] as usize;
        self.cell_list[s..s + n].iter().map(|&e| e as usize / 4)
    }

    /// FUN_8003cf14: height of the triangle (p, q, r) at TER (x, z), returned
    /// in world units ((y << 10) interpolated, then >> 5). The game scans the
    /// triangle by z: it interpolates x and y along the two edges that span
    /// z, then interpolates y across x. Integer steps (slopes are divided
    /// before multiplying) are reproduced exactly.
    pub fn face_height(&self, p: usize, q: usize, r: usize, x: i32, z: i32) -> i32 {
        let (mut a, mut b, mut c) = (self.vertices[p], self.vertices[q], self.vertices[r]);
        // Three compare-swaps by z (offset 8): a <= b <= c afterwards.
        if b.z < a.z {
            std::mem::swap(&mut a, &mut b);
        }
        if c.z < b.z {
            std::mem::swap(&mut b, &mut c);
        }
        if b.z < a.z {
            std::mem::swap(&mut a, &mut b);
        }
        // Base vertex and the two edge ends.
        let (base, e1, e2) = if z < b.z || b.z == c.z { (a, b, c) } else { (c, a, b) };
        let dz = z.wrapping_sub(base.z);
        let bx = base.x << 10;
        let by = base.y << 10;
        let (mut x1, mut y1, mut x2, mut y2) = (bx, by, bx, by);
        if dz != 0 {
            let d1 = e1.z.wrapping_sub(base.z);
            if d1 != 0 {
                let sx = ((e1.x << 10).wrapping_sub(bx)).wrapping_div(d1);
                let sy = ((e1.y << 10).wrapping_sub(by)).wrapping_div(d1);
                x1 = bx.wrapping_add(sx.wrapping_mul(dz));
                y1 = by.wrapping_add(sy.wrapping_mul(dz));
            }
            let d2 = e2.z.wrapping_sub(base.z);
            if d2 != 0 {
                let sx = ((e2.x << 10).wrapping_sub(bx)).wrapping_div(d2);
                let sy = ((e2.y << 10).wrapping_sub(by)).wrapping_div(d2);
                x2 = bx.wrapping_add(sx.wrapping_mul(dz));
                y2 = by.wrapping_add(sy.wrapping_mul(dz));
            }
        }
        let xa = x1 >> 10;
        let xb = x2 >> 10;
        let y = if xa == xb {
            y1.wrapping_add(y2) / 2
        } else if xa < xb {
            let t = x.wrapping_sub(xa);
            if t == 0 {
                y1
            } else {
                y1.wrapping_add((y2.wrapping_sub(y1) / (xb - xa)).wrapping_mul(t))
            }
        } else {
            let t = x.wrapping_sub(xb);
            if t == 0 {
                y2
            } else {
                y2.wrapping_add((y1.wrapping_sub(y2) / (xa - xb)).wrapping_mul(t))
            }
        };
        y >> 5
    }

    /// Edge function the game uses for point-in-polygon tests (x, z plane):
    /// (x - P.x) * (Q.z - P.z) + (z - P.z) * (P.x - Q.x), wrapping i32.
    fn edge(px: i32, pz: i32, qx: i32, qz: i32, x: i32, z: i32) -> i32 {
        x.wrapping_sub(px)
            .wrapping_mul(qz.wrapping_sub(pz))
            .wrapping_add(z.wrapping_sub(pz).wrapping_mul(px.wrapping_sub(qx)))
    }

    fn vedge(&self, p: u16, q: u16, x: i32, z: i32) -> i32 {
        let (p, q) = (self.vertices[p as usize], self.vertices[q as usize]);
        Self::edge(p.x, p.z, q.x, q.z, x, z)
    }

    /// The triangle of `face` containing TER (x, z), as the game picks it,
    /// or None when the point is outside the face.
    pub fn face_triangle(&self, face: usize, x: i32, z: i32) -> Option<[usize; 3]> {
        let f = &self.faces[face];
        let [v0, v1, v2, v3] = f.v;
        let tri = |a: u16, b: u16, c: u16| Some([a as usize, b as usize, c as usize]);
        if f.is_quad() {
            if self.vedge(v0, v2, x, z) >= 0 {
                if self.vedge(v2, v3, x, z) >= 0 && self.vedge(v3, v0, x, z) >= 0 {
                    return tri(v0, v2, v3);
                }
            } else if self.vedge(v0, v1, x, z) >= 0 && self.vedge(v1, v2, x, z) >= 0 {
                return tri(v0, v1, v2);
            }
            None
        } else if self.vedge(v0, v1, x, z) >= 0
            && self.vedge(v1, v2, x, z) >= 0
            && self.vedge(v2, v0, x, z) >= 0
        {
            tri(v0, v1, v2)
        } else {
            None
        }
    }

    /// Grid cell of a TER-unit point the way FUN_8003ddb0 computes it
    /// (-1 when below the minimum; no upper clamp).
    pub fn cell_of(&self, x: i32, z: i32) -> (i32, i32) {
        let cx = if x < self.min[0] { -1 } else { x.wrapping_sub(self.min[0]) / self.cell_x };
        let cz = if z < self.min[2] { -1 } else { z.wrapping_sub(self.min[2]) / self.cell_z };
        (cx, cz)
    }

    /// FUN_8003ddb0(x, y, z): surfaces under/over a world-unit point.
    /// x and z are shifted >> 5 into TER units; y stays in world units and
    /// is only used to rank candidates by |height - y|. Faces are visited in
    /// the cell list order and ties keep the first one, as in the game.
    pub fn ground(&self, x: i32, y: i32, z: i32) -> Ground {
        let x = x >> 5;
        let z = z >> 5;
        let (cx, cz) = self.cell_of(x, z);
        let default_y = self.max[1] << 5;
        let mut g = Ground {
            cell_x: cx,
            cell_z: cz,
            in_grid: false,
            height: default_y,
            nearest: None,
            nearest_y: default_y,
            below: None,
            alt: None,
            water: None,
            water_y: self.no_water_y << 5,
            found: false,
        };
        if cx < 0 || cx >= self.grid_w || cz < 0 || cz >= self.grid_h {
            return g;
        }
        g.in_grid = true;
        let cell = (cz.wrapping_mul(self.grid_w) + cx) as usize;
        let (mut d_near, mut d_below, mut d_alt) = (0u32, 0u32, 0u32);
        for fi in self.cell_faces(cell) {
            let f = &self.faces[fi];
            if f.disabled() {
                continue;
            }
            let Some([p, q, r]) = self.face_triangle(fi, x, z) else { continue };
            let h = self.face_height(p, q, r, x, z);
            if f.kind == KIND_WATER {
                if h < g.water_y {
                    g.water_y = h;
                    g.water = Some(SurfaceHit { face: fi, y: h });
                }
                continue;
            }
            let d = h.wrapping_sub(y).wrapping_abs() as u32;
            if f.alt() {
                if g.alt.is_none() || d < d_alt {
                    d_alt = d;
                    g.alt = Some(SurfaceHit { face: fi, y: h });
                }
                continue;
            }
            if g.nearest.is_none() || d < d_near {
                d_near = d;
                g.nearest = Some(SurfaceHit { face: fi, y: h });
                g.nearest_y = h;
            }
            if h >= y {
                let d = h.wrapping_sub(y) as u32;
                if g.below.is_none() || d < d_below {
                    d_below = d;
                    g.below = Some(SurfaceHit { face: fi, y: h });
                }
            }
            g.found = true;
        }
        // Pick the closest of nearest / alt / water (strictly closer wins).
        let dist = |h: i32| h.wrapping_sub(y).wrapping_abs();
        g.height = g.nearest_y;
        if let Some(a) = g.alt {
            if dist(a.y) < dist(g.height) {
                g.height = a.y;
            }
        }
        if dist(g.water_y) < dist(g.height) {
            g.height = g.water_y;
        }
        if g.below.is_none() {
            g.below = g.nearest;
        }
        g
    }

    /// FUN_8003d760 (x0, z0) -> (x1, z1), world units: the first enabled
    /// wall edge in the destination cell's list that the move crosses from
    /// its negative side, or None. Returns the wall index. The game's own
    /// return also latches (x0, z0) into 0x800b0bfc / 0x800b0c0c.
    pub fn wall_cross(&self, x0: i32, z0: i32, x1: i32, z1: i32) -> Option<usize> {
        self.wall_candidates(x0, z0, x1, z1).next()
    }

    /// Every wall edge FUN_8003da18 would hand to its height test, in game
    /// order (same geometric test as FUN_8003d760).
    pub fn wall_candidates(&self, x0: i32, z0: i32, x1: i32, z1: i32) -> impl Iterator<Item = usize> + '_ {
        let (sx0, sz0, sx1, sz1) = (x0 >> 5, z0 >> 5, x1 >> 5, z1 >> 5);
        let cell = self.wall_cell(sx1, sz1);
        let list: Box<dyn Iterator<Item = usize>> = match cell {
            Some(c) => Box::new(self.cell_wall_list(c)),
            None => Box::new(std::iter::empty()),
        };
        list.filter(move |&wi| {
            let w = self.walls[wi];
            if w.disabled {
                return false;
            }
            let b = self.wall_points[w.p0 as usize];
            let a = self.wall_points[w.p1 as usize];
            Self::edge(a.x, a.z, b.x, b.z, sx0, sz0) < 0
                && Self::edge(a.x, a.z, b.x, b.z, sx1, sz1) >= 0
                && Self::edge(a.x, a.z, sx0, sz0, sx1, sz1) >= 0
                && Self::edge(sx0, sz0, b.x, b.z, sx1, sz1) >= 0
        })
    }

    /// Cell index as FUN_8003d760 computes it (i16 truncations included).
    fn wall_cell(&self, x: i32, z: i32) -> Option<usize> {
        if x < self.min[0] {
            return None;
        }
        let cx = (x.wrapping_sub(self.min[0]) / self.cell_x) as i16 as i32;
        if cx >= self.grid_w {
            return None;
        }
        if z < self.min[2] {
            return None;
        }
        let cz = (z.wrapping_sub(self.min[2]) / self.cell_z) as i16 as i32;
        if cz >= self.grid_h {
            return None;
        }
        let c = (cx.wrapping_add(cz.wrapping_mul(self.grid_w))) as i16;
        if c < 0 {
            return None;
        }
        Some(c as usize)
    }

    /// FUN_800421c4 (enable = true) / FUN_8004229c (enable = false): set or
    /// clear FACE_DISABLED on the group's faces and the disabled bit on its
    /// walls.
    pub fn set_group_enabled(&mut self, group: usize, enable: bool) {
        let Some(g) = self.groups.get(group).copied() else { return };
        for k in 0..g.face_count.max(0) as usize {
            let f = &mut self.faces[g.face_start as usize + k];
            if enable { f.flags &= !FACE_DISABLED } else { f.flags |= FACE_DISABLED }
        }
        for k in 0..g.wall_count.max(0) as usize {
            self.walls[g.wall_start as usize + k].disabled = !enable;
        }
    }

    /// FUN_80042374: add `dy` (TER units) to the y of the group's vertices
    /// and to both end points' y_top/y_bottom of each of its walls (a point
    /// shared by two walls moves twice, as in the game).
    pub fn move_group_y(&mut self, group: usize, dy: i16) {
        let Some(g) = self.groups.get(group).copied() else { return };
        let dy = dy as i32;
        for k in 0..g.vert_count.max(0) as usize {
            let v = &mut self.vertices[g.vert_start as usize + k];
            v.y = v.y.wrapping_add(dy);
        }
        for k in 0..g.wall_count.max(0) as usize {
            let w = self.walls[g.wall_start as usize + k];
            for p in [w.p0, w.p1] {
                let p = &mut self.wall_points[p as usize];
                p.y_top = p.y_top.wrapping_add(dy);
                p.y_bottom = p.y_bottom.wrapping_add(dy);
            }
        }
    }

    /// Re-encode to the on-disc byte layout (used by the tests to prove the
    /// decode is lossless).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut o = Vec::new();
        let p32 = |o: &mut Vec<u8>, v: i32| o.extend_from_slice(&v.to_le_bytes());
        let p16 = |o: &mut Vec<u8>, v: u16| o.extend_from_slice(&v.to_le_bytes());
        p32(&mut o, self.grid_w);
        p32(&mut o, self.grid_h);
        for &x in &self.offsets {
            p32(&mut o, x as i32);
        }
        for v in self.min.iter().chain(self.max.iter()) {
            p32(&mut o, *v);
        }
        for v in [
            self.cell_x,
            self.span_y,
            self.cell_z,
            self.vertices.len() as i32,
            self.faces.len() as i32,
            self.wall_points.len() as i32,
            self.walls.len() as i32,
            self.groups.len() as i32,
            self.cell_list.len() as i32,
            self.no_water_y,
            self.path_points.len() as i32,
            self.path_segments.len() as i32,
            self.links.len() as i32,
        ] {
            p32(&mut o, v);
        }
        for v in &self.vertices {
            for c in [v.x, v.y, v.z] {
                p32(&mut o, c);
            }
        }
        for f in &self.faces {
            for v in f.v {
                p16(&mut o, v * 12);
            }
            for n in f.normal {
                p16(&mut o, n as u16);
            }
            o.push(f.flags);
            o.push(f.kind);
        }
        for w in &self.wall_points {
            for c in [w.x, w.z, w.y_top, w.y_bottom] {
                p32(&mut o, c);
            }
        }
        for w in &self.walls {
            p16(&mut o, w.p0 * 16 | w.disabled as u16);
            p16(&mut o, w.p1 * 16);
        }
        for g in &self.groups {
            o.extend_from_slice(&g.unk0.to_le_bytes());
            p16(&mut o, g.vert_start * 12);
            p16(&mut o, g.face_start * 16);
            p16(&mut o, g.wall_start * 4);
            p16(&mut o, g.vert_count as u16);
            p16(&mut o, g.face_count as u16);
            p16(&mut o, g.wall_count as u16);
            o.extend_from_slice(&g.unk16.to_le_bytes());
        }
        for &e in &self.cell_list {
            p16(&mut o, e);
        }
        o.extend(std::iter::repeat(0).take(self.pad_list));
        for &s in &self.cell_list_start {
            p16(&mut o, s * 2);
        }
        o.extend_from_slice(&self.cell_floors);
        o.extend_from_slice(&self.cell_walls);
        for p in &self.path_points {
            for c in [p.x, p.y, p.z] {
                p32(&mut o, c);
            }
            p16(&mut o, p.heading);
            o.extend_from_slice(&p.w);
        }
        for s in &self.path_segments {
            for v in [s.first, s.count, s.link_start, s.n_next, s.n_prev, s.flags] {
                p16(&mut o, v as u16);
            }
            for c in s.bb_min.iter().chain(s.bb_max.iter()) {
                p32(&mut o, *c);
            }
        }
        o.extend_from_slice(&self.links);
        o.extend(std::iter::repeat(0).take(self.pad_links));
        o
    }
}

//! axe: the sky dome (`<track>.AXE`), a camera-centred shell of
//! Gouraud-shaded quads drawn behind everything.
//!
//! Loading: FUN_8004627c reads `<track>.axe` (FUN_800420cc) to the heap at
//! DAT_800b0c4c only when the track's flag bit 0 is set, then FUN_80042130
//! relocates it in place and calls FUN_80034ba4, which rewrites vertex
//! colours and face indices in place (see [`Axe::to_ram_image`]).
//! Drawing: FUN_80034ae4 loads the camera rotation (0x800c3008) with a zero
//! translation and calls FUN_80011a14(segments, count, n_face_ids), which
//! culls each segment by its 4 corner points, then emits one POLY_G4
//! (code 0x38) per face into a single ordering-table slot, deduplicating
//! faces shared between segments by their `face_id` in a scratchpad byte map
//! at 0x1F800020 (cleared for `n_face_ids` bytes, rounded up to 8).
//!
//! Units: model space i16, rotated (no translation) by the camera matrix.

use crate::le;

#[derive(Debug)]
pub enum Error {
    Short { need: usize, have: usize },
    Layout(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Short { need, have } => write!(f, "AXE too short: need {need} bytes, have {have}"),
            Error::Layout(s) => write!(f, "AXE layout: {s}"),
        }
    }
}
impl std::error::Error for Error {}

/// A dome vertex (12 bytes on disc).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Vertex {
    pub x: i16,
    pub y: i16,
    pub z: i16,
    /// Never read by the game (the GTE load takes x, y, z only). Kept for a
    /// lossless round trip. [CODE 80011a14]
    pub pad: i16,
    /// Colour as stored on disc: bytes B, G, R. The loader swaps bytes 0
    /// and 2 so RAM holds R, G, B as the GPU wants.
    pub bgr: [u8; 3],
    /// Byte 11; the loader zeroes it (it becomes the GP0 code byte slot).
    pub code: u8,
}

impl Vertex {
    pub fn rgb(&self) -> [u8; 3] {
        [self.bgr[2], self.bgr[1], self.bgr[0]]
    }
}

/// A dome quad (12 bytes on disc).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Face {
    /// Vertex indices into the owning segment's vertex list. The loader
    /// takes |v[0]| (a negative first index is accepted) and scales all four
    /// by 12 (byte offsets).
    pub v: [i16; 4],
    /// Global dedupe id (< `n_face_ids`); the loader adds 0x1F800020 so it
    /// points at the face's byte in the scratchpad bitmap.
    pub face_id: u32,
}

/// One dome segment (44-byte table entry).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    /// File offsets of this segment's vertex list and face list.
    pub vert_offset: u32,
    pub face_offset: u32,
    /// Third relocated pointer; never read by the code. On DOCK every
    /// segment points it at the same spare copy of the table. [CODE]
    pub spare_offset: u32,
    /// 4 corner points used to cull the whole segment before its faces.
    pub corners: [[i16; 3]; 4],
    /// +0x28, +0x2A: not read by the code. [INFER: authoring data]
    pub unk40: u16,
    pub unk42: u16,
    pub vertices: Vec<Vertex>,
    pub faces: Vec<Face>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Axe {
    /// Offset of the tail block (header word 0).
    pub tail_offset: u32,
    pub segments: Vec<Segment>,
    /// Number of distinct face ids: the first i16 of the tail block
    /// (stored to 0x800b0db0).
    pub n_face_ids: i16,
    /// Bytes from `tail_offset + 2` to the end of file (never read). [CODE]
    pub tail_rest: Vec<u8>,
    /// Bytes not covered by the table, the vertex/face lists or the tail
    /// (on DOCK: the spare table copy). Offset and bytes, never read.
    pub unreferenced: Vec<(usize, Vec<u8>)>,
}

/// Size of one segment table entry.
pub const SEGMENT_LEN: usize = 44;

impl Axe {
    pub fn parse(b: &[u8]) -> Result<Axe, Error> {
        if b.len() < 8 {
            return Err(Error::Short { need: 8, have: b.len() });
        }
        let tail_offset = le::u32(b, 0);
        let n = le::i32(b, 4);
        if n < 0 || n > 4096 {
            return Err(Error::Layout(format!("segment count {n}")));
        }
        let n = n as usize;
        let table_end = 8 + SEGMENT_LEN * n;
        if table_end > b.len() || tail_offset as usize + 2 > b.len() {
            return Err(Error::Short { need: table_end.max(tail_offset as usize + 2), have: b.len() });
        }
        let mut covered = vec![false; b.len()];
        covered[..table_end].iter_mut().for_each(|c| *c = true);
        covered[tail_offset as usize..].iter_mut().for_each(|c| *c = true);
        let mut segments = Vec::with_capacity(n);
        for s in 0..n {
            let o = 8 + SEGMENT_LEN * s;
            let vert_offset = le::u32(b, o);
            let face_offset = le::u32(b, o + 4);
            let spare_offset = le::u32(b, o + 8);
            let mut corners = [[0i16; 3]; 4];
            for (k, c) in corners.iter_mut().enumerate() {
                for (j, v) in c.iter_mut().enumerate() {
                    *v = le::i16(b, o + 12 + 6 * k + 2 * j);
                }
            }
            let nv = le::i16(b, o + 36);
            let nf = le::i16(b, o + 38);
            if nv < 0 || nf < 0 {
                return Err(Error::Layout(format!("segment {s} counts {nv} {nf}")));
            }
            let (nv, nf) = (nv as usize, nf as usize);
            let (vo, fo) = (vert_offset as usize, face_offset as usize);
            if vo + 12 * nv > b.len() || fo + 12 * nf > b.len() {
                return Err(Error::Layout(format!("segment {s} lists past end")));
            }
            covered[vo..vo + 12 * nv].iter_mut().for_each(|c| *c = true);
            covered[fo..fo + 12 * nf].iter_mut().for_each(|c| *c = true);
            let vertices = (0..nv)
                .map(|k| {
                    let p = vo + 12 * k;
                    Vertex {
                        x: le::i16(b, p),
                        y: le::i16(b, p + 2),
                        z: le::i16(b, p + 4),
                        pad: le::i16(b, p + 6),
                        bgr: [b[p + 8], b[p + 9], b[p + 10]],
                        code: b[p + 11],
                    }
                })
                .collect();
            let mut faces = Vec::with_capacity(nf);
            for k in 0..nf {
                let p = fo + 12 * k;
                let v = [le::i16(b, p), le::i16(b, p + 2), le::i16(b, p + 4), le::i16(b, p + 6)];
                for (j, &i) in v.iter().enumerate() {
                    let i = if j == 0 { i.unsigned_abs() as usize } else { i as usize };
                    if i >= nv {
                        return Err(Error::Layout(format!("segment {s} face {k} vertex {i} >= {nv}")));
                    }
                }
                faces.push(Face { v, face_id: le::u32(b, p + 8) });
            }
            segments.push(Segment {
                vert_offset,
                face_offset,
                spare_offset,
                corners,
                unk40: le::u16(b, o + 40),
                unk42: le::u16(b, o + 42),
                vertices,
                faces,
            });
        }
        let n_face_ids = le::i16(b, tail_offset as usize);
        for s in &segments {
            for f in &s.faces {
                if f.face_id as i64 >= n_face_ids as i64 {
                    return Err(Error::Layout(format!("face id {} >= {n_face_ids}", f.face_id)));
                }
            }
        }
        let mut unreferenced = Vec::new();
        let mut i = 0;
        while i < b.len() {
            if covered[i] {
                i += 1;
                continue;
            }
            let s = i;
            while i < b.len() && !covered[i] {
                i += 1;
            }
            unreferenced.push((s, b[s..i].to_vec()));
        }
        Ok(Axe {
            tail_offset,
            segments,
            n_face_ids,
            tail_rest: b[tail_offset as usize + 2..].to_vec(),
            unreferenced,
        })
    }

    /// Rebuild the file bytes (lossless round trip).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut len = self.tail_offset as usize + 2 + self.tail_rest.len();
        for (o, u) in &self.unreferenced {
            len = len.max(o + u.len());
        }
        let mut b = vec![0u8; len];
        b[0..4].copy_from_slice(&self.tail_offset.to_le_bytes());
        b[4..8].copy_from_slice(&(self.segments.len() as i32).to_le_bytes());
        for (s, seg) in self.segments.iter().enumerate() {
            let o = 8 + SEGMENT_LEN * s;
            b[o..o + 4].copy_from_slice(&seg.vert_offset.to_le_bytes());
            b[o + 4..o + 8].copy_from_slice(&seg.face_offset.to_le_bytes());
            b[o + 8..o + 12].copy_from_slice(&seg.spare_offset.to_le_bytes());
            for k in 0..4 {
                for j in 0..3 {
                    let p = o + 12 + 6 * k + 2 * j;
                    b[p..p + 2].copy_from_slice(&seg.corners[k][j].to_le_bytes());
                }
            }
            b[o + 36..o + 38].copy_from_slice(&(seg.vertices.len() as i16).to_le_bytes());
            b[o + 38..o + 40].copy_from_slice(&(seg.faces.len() as i16).to_le_bytes());
            b[o + 40..o + 42].copy_from_slice(&seg.unk40.to_le_bytes());
            b[o + 42..o + 44].copy_from_slice(&seg.unk42.to_le_bytes());
            for (k, v) in seg.vertices.iter().enumerate() {
                let p = seg.vert_offset as usize + 12 * k;
                for (j, c) in [v.x, v.y, v.z, v.pad].iter().enumerate() {
                    b[p + 2 * j..p + 2 * j + 2].copy_from_slice(&c.to_le_bytes());
                }
                b[p + 8..p + 11].copy_from_slice(&v.bgr);
                b[p + 11] = v.code;
            }
            for (k, f) in seg.faces.iter().enumerate() {
                let p = seg.face_offset as usize + 12 * k;
                for (j, c) in f.v.iter().enumerate() {
                    b[p + 2 * j..p + 2 * j + 2].copy_from_slice(&c.to_le_bytes());
                }
                b[p + 8..p + 12].copy_from_slice(&f.face_id.to_le_bytes());
            }
        }
        for (o, u) in &self.unreferenced {
            b[*o..*o + u.len()].copy_from_slice(u);
        }
        let t = self.tail_offset as usize;
        b[t..t + 2].copy_from_slice(&self.n_face_ids.to_le_bytes());
        b[t + 2..t + 2 + self.tail_rest.len()].copy_from_slice(&self.tail_rest);
        b
    }

    /// The bytes the game holds after FUN_80042130 + FUN_80034ba4 when the
    /// file was loaded at `base`: the three per-segment offsets and word 0
    /// become absolute pointers, vertex colours are swapped to R,G,B with a
    /// zero code byte, face indices become |v0|*12, v1*12, v2*12, v3*12 and
    /// face ids become 0x1F800020 + id. Everything else is unchanged.
    pub fn to_ram_image(&self, base: u32) -> Vec<u8> {
        let mut b = self.to_bytes();
        let w = |b: &mut Vec<u8>, o: usize, v: u32| b[o..o + 4].copy_from_slice(&v.to_le_bytes());
        // Word 0 is not rewritten in RAM (only read into 0x800b0c48).
        for (s, seg) in self.segments.iter().enumerate() {
            let o = 8 + SEGMENT_LEN * s;
            w(&mut b, o, base.wrapping_add(seg.vert_offset));
            w(&mut b, o + 4, base.wrapping_add(seg.face_offset));
            w(&mut b, o + 8, base.wrapping_add(seg.spare_offset));
            for (k, v) in seg.vertices.iter().enumerate() {
                let p = seg.vert_offset as usize + 12 * k;
                b[p + 8] = v.bgr[2];
                b[p + 10] = v.bgr[0];
                b[p + 11] = 0;
            }
            for (k, f) in seg.faces.iter().enumerate() {
                let p = seg.face_offset as usize + 12 * k;
                let v0 = (f.v[0] as i32).abs();
                let vals = [v0 * 12, f.v[1] as i32 * 12, f.v[2] as i32 * 12, f.v[3] as i32 * 12];
                for (j, c) in vals.iter().enumerate() {
                    b[p + 2 * j..p + 2 * j + 2].copy_from_slice(&(*c as i16).to_le_bytes());
                }
                w(&mut b, p + 8, f.face_id.wrapping_add(0x1F80_0020));
            }
        }
        b
    }

    /// Faces as drawn, deduplicated by face id (first segment wins, like the
    /// scratchpad bitmap), each as 4 model-space points + colours in the
    /// GPU's G4 order (v1, v0, v2, v3), i.e. strip triangles
    /// (v1,v0,v2) and (v0,v2,v3).
    pub fn drawn_quads(&self) -> Vec<[([i16; 3], [u8; 3]); 4]> {
        let mut seen = vec![false; self.n_face_ids.max(0) as usize];
        let mut out = Vec::new();
        for seg in &self.segments {
            for f in &seg.faces {
                let id = f.face_id as usize;
                if id < seen.len() {
                    if seen[id] {
                        continue;
                    }
                    seen[id] = true;
                }
                let idx = [f.v[1] as usize, f.v[0].unsigned_abs() as usize, f.v[2] as usize, f.v[3] as usize];
                let q = idx.map(|i| {
                    let v = seg.vertices[i];
                    ([v.x, v.y, v.z], v.rgb())
                });
                out.push(q);
            }
        }
        out
    }
}

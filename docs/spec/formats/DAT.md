# DAT: track scenery (placed meshes), paths, visibility stream

File: `<track>.DAT` (DOCK: `COURSE_A/DOCK.DAT`, 344,624 bytes). 36 files on the
disc; all parse with every byte accounted for.
Decoder: `port/crates/mr-assets/src/dat.rs`. CLI: `mr-dat` (`src/bin/mr-dat.rs`).
Tests: `port/crates/mr-assets/tests/dat.rs`.
Tags: [CODE addr] read from the game's code (Ghidra decomp checked against the
recompiler's generated C where they could differ), [RAM] matched the running
recomp, [GTE]/[GPU] matched the recomp's GTE ring / GP0 packets, [INFER] my
inference, unverified.

## What it is
The track's render scenery: a list of placed objects, each a descriptor
(origin, rotation, optional scale) pointing at a mesh of 8-byte vertices and
primitive groups that the renderer turns into GPU GT4/GT3/G4/G3 packets. Two
object lists ("layers"): layer 0 in world units, layer 1 drawn with the camera
divided by 4 (quarter units, finer precision for small props). Plus a header of
point lists (paths; one is the visibility path) and a per-path-node
visibility (PVS) delta stream. Not in this file: collision/ground (TER),
textures (RAW: fills the texture-slot table this file indexes), sky (AXE).

DOCK: 12 point chunks, 589 + 243 objects, 832 descriptors, 609 meshes
(21,265 vertices, 9,798 primitives), 1 visibility zone of 172 nodes.

## Loading [CODE 8004627c, 8002e3dc] [RAM]
FUN_8004627c reads `<track>.dat` with FUN_8005fdf4 to the heap pointer
DAT_800b0ee8 (DOCK: 0x80137E68), then calls FUN_8002e3dc(track, a1 = file size).
The loader works in place and writes an output block right after the file
(DAT_800b0c1c = base + size); it returns that block's size,
`objects*0x28 + (N+1)*0x1c` (DOCK 34,484), which the caller adds to the heap.
In-place fix-ups (all reproduced by `Dat::loader_image`):
- object +0x10: descriptor offset -> base + offset, then replaced by the
  pointer to the object's 40-byte record in the output block;
- object flags: if bit 0x80: f = (f | 0x50) - 0x10 (set 0x40, clear 0x10);
  layer 0 only: also clear bit 15 of the radius halfword; if bit 0x20: f |= 0x40;
- descriptor mesh pointer(s) += base; low-nibble mask of the "b_lo" byte;
  FUN_8002e13c clamps "b_hi" >> 3 to <= 18;
- special-slot offsets += base;
- chunks with id > 0x40 are converted in place (not on DOCK; not reproduced).

## File layout
| off | size | content | tag |
|---|---|---|---|
| 0 | 4 | i32 chunk count (DOCK 12) | [CODE 8002e3dc] |
| 4 | ... | chunks (below) | [CODE] |
| ... | 4 + 128n | i32 n, n x 128 bytes; unknown, n = 0 on DOCK | [CODE size] |
| ... | 20 x k + 20 | layer-0 object list, ended by a record with flags = 0 (DAT_800b0f00 = base+0x3A04) | [CODE][RAM] |
| ... | 4 + 4(N+1) | i32 N, N+1 u32 object offsets (0 = empty): special slots (DAT_800b0c28 = base+0x681C) | [CODE][RAM] |
| ... | 20 x k + 20 | layer-1 object list (DAT_800b0fb4 = base+0x68CC) | [CODE][RAM] |
| ... | | descriptors (24/28/32/36 bytes, contiguous, one per object on DOCK) | [CODE][RAM] |
| ... | | meshes (and kind-2/3 models), contiguous, each padded to 4 | [CODE][RAM] |
| len - V | | visibility zone records, one per id-0x40 chunk | [CODE 8002e3dc][RAM] |
| ... | | zero padding | |
| len - 4 | 4 | i32 V = distance from file end to the first zone record; -1 = none | [CODE][RAM] |

DOCK sections: chunks 0x0..0x3A00, block128 count 0x3A00, layer 0
0x3A04..0x681C, special slots ..0x68CC, layer 1 ..0x7BDC, descriptors
..0xDB7C, meshes ..0x52554, visibility ..0x54226, padding + V (= 7388) ..0x54230.

### Chunk (header section) [CODE 8002e3dc]
| off | type | meaning |
|---|---|---|
| 0 | i16 | count |
| 2 | i16 | id |
| 4 | | body |

Size by id: id < 0: `((count*3+1)/2)*4 + 0x10` (registered at DAT_800d1c48[-id]);
id 0x3F: `count*16 + 4`; else `count*12 + 4`. Ids 0..0x3E, 0x40: `count` points
of three i32 (x, y, z), world units. id 0x40 is a visibility zone path (table
DAT_800e3494, 36-byte entries); others are registered at DAT_800d1c48[id]
(paths; meaning per id [INFER], DOCK has 1, 3, 5, 7, 9, 0xA, 0xB, 0xC, 0x12,
0x13, 0x15; 0xA/0xB have 290 points each and follow the track, likely racing
lines). Ids > 0x40: converted in place to s16 points (>> 2) with a computed
normal and added to DAT_800d6c90 (not on DOCK; kept raw).

### Object (20 bytes) [CODE 8002e3dc, 80010000] [RAM]
| off | type | meaning | tag |
|---|---|---|---|
| 0 | i32 x3 | centre (layer units): the bbox centre of the transformed mesh, to within 3 units on > 95% of axes | [INFER: checked over all DOCK objects] |
| 0xC | u16 | cull radius (FUN_80010000 compares with camera distance >> 4) | [CODE 80010000] |
| 0xE | u8 | flags; 0 ends the list | [CODE] |
| 0xF | u8 | unknown, 0 on DOCK | |
| 0x10 | u32 | descriptor offset | [CODE][RAM] |

Flags: bits 0-2 model kind (with bit 6 ignored, FUN_8002e13c switch on
flags & 0x6f): 1/4 = mesh, 2/3 = parts model, 5 = none; 0x08 = scaled
descriptor; 0x20 = second mesh pointer (loader sets 0x40); 0x80 = rewritten by
the loader (see above). DOCK flags: layer 0 {0x11, 0x19, 0x41, 0x49, 0x99},
layer 1 {0x01, 0x09, 0x89}. Meaning of 0x10/0x40 [INFER: render path options].

### Descriptor [CODE 8002e3dc, 8001f54c] [RAM]
| off (plain) | off (scaled, flag 0x08) | type | meaning |
|---|---|---|---|
| 0 | 0 | i32 x3 | origin: translation of the mesh (layer units) [GTE] |
| 0xC | 0xC | u16 x3 | angles ax, ay, az (12-bit) |
| - | 0x12 | i16 x3 | column scale (4.12) |
| 0x12 | 0x18 | u8 | "b_lo": loader keeps low nibble [CODE] |
| 0x13 | 0x19 | u8 | "b_hi": high 5 bits clamped to 18 by FUN_8002e13c [INFER: LOD/distance] |
| - | 0x1A | u16 | unknown (0 or small) |
| 0x14 | 0x1C | u32 | model offset |
| 0x18 | 0x20 | u32 | second model offset, only with flag 0x20 (none on DOCK) |

Rotation FUN_8001f54c: M = Rx(ax) * Ry(ay) * Rz(az) in 4.12, built from the
executable's sine table at 0x800A88C0 (5120 x i16, 1.0 = 16384, used >> 2;
cos a = entry a+1024), each product `(a*b) >> 12` in the order of
`dat::rot_matrix`. Scaled: `m[r][c] = m[r][c] * scale[c] / 4096` (C
division). [RAM: 821/821 static object matrices exact]

**World transform** (both [GTE]-verified):
- layer 0: `world = origin + (M * v) >> 12`
- layer 1: `world = 4 * (origin + (M * v) >> 12)`. FUN_80029f40 divides the
  camera position words by 4 (`(x + 3) >> 2` for negatives) before drawing
  layer 1 [CODE 8002A600..8002A67C].

### Output block (written by the loader at base + file size) [CODE][RAM]
- (N+1) x 28-byte special (pose) records, slot i for special slot i: +0x00
  origin (3 x i32), +0x0C descriptor bytes 0x12..0x17 (the scale for scaled
  descriptors), +0x12 u16 object flags, +0x14 angles (3 x u16), +0x1A..0x1B
  untouched. Read by FUN_8002d72c/8002d9e4/8002dc14/8002df88/8002e060.
  DOCK uses slots 32..42 (objects 274, 436, 563, 565, 566, 567, 569, 570, 572,
  574, 269); these objects are moved by game code at run time (centre, origin
  and angles change in RAM; the pose records keep the loaded pose).
- objects x 40-byte records in list order: +0x00 matrix (9 x i16), +0x14 u32
  descriptor pointer, +0x18..0x1F zeroed (run-time state afterwards), +0x20
  i16 packet-slot count (FUN_80025b8c: primitives, x2 for kinds 8-0xF;
  kind-2/3 models: record count), layer 1 also zeroes +0x26/+0x27.

### Mesh [CODE 80025b8c, 8001a304, 80012620] [RAM] [GTE] [GPU]
| off | type | meaning |
|---|---|---|
| 0 | i32 | vertex count n (>= 0 on DOCK; negative = 12-byte vertices + 4-byte trailer, [CODE 80025b8c] size only) |
| 4 | 8 x n | vertices: i16 x, y, z, u16 colour (the GTE SVECTOR, pad = colour) |
| ... | | primitive groups until a u16 0xFFFF; then pad to 4 (0xFFFF bytes) |

Colour: R in bits 10-14, G 5-9, B 0-4 (reverse of the GPU's 15-bit order);
the packet colour is (R<<3, G<<3, B<<3), 0x80 = neutral modulation [GPU].

Group: u16 header, i16 count, count records.
| header bits | meaning | tag |
|---|---|---|
| 0-4 | kind: bit0 triangle; bit1 no back-face cull; bit2 quad culled only if both halves face away (layer-1 renderer); bit3 two packet slots (subdivided path in layer 0); bit4 untextured | [CODE 8001a304, 80012620] |
| 5-6 | added to the tpage ABR bits; 3 = opaque, 0-2 = semi-transparent | [CODE][GPU] |
| 7, 13, 14 | unused (0 on DOCK) | |
| 8-12 | texture slot: index into the 16-byte table at 0x800DFF00 (tpage u16, clut u16, +8 u16 "fixed CLUT"; filled by the RAW loader) | [CODE][GPU] |
| 15 | renderer skips the texture setup and keeps the previous group's state (DOCK: always equal to the previous group's bits 5-12) | [CODE 8001a304] |

Kinds 0x18-0x1E end the renderer's loop and kinds 5/7/0xD/0xF/0x15/0x17/0x1D
hang FUN_80025b8c, so no valid mesh uses them (the decoder rejects them).
DOCK kinds: 0 (4746), 1 (553), 2 (204), 4 (2126), 6 (20), 0x10 (969),
0x11 (449), 0x14 (731).

Records:
| kind | size | layout |
|---|---|---|
| textured quad | 12 | u8 i0, i1, i2, i3; uv(i0), uv(i1), uv(i2), uv(i3) as (u8 u, u8 v) |
| textured triangle | 12 | u8 i0, i1, i2, (unused); 2 unused bytes; uv(i0), uv(i1), uv(i2) at +6, +8, +10 |
| untextured | 4 | u8 i0, i1, i2, i3 (triangles ignore i3) |

Indices are in polygon (cyclic) order; the GPU quad order is i0, i1, i3, i2.
Front faces: drawn when NCLIP(i3, i2, i0) >= 0 (quads) / NCLIP(i2, i1, i0) >= 0
(triangles); in a y-up view the record order is counter-clockwise for front
faces (the OBJ export uses axes x, -y, -z).
Texture attributes (both renderers): tpage = slot.tpage + (header & 0x60);
clut = slot.clut + (u0 >> 6) + 4 * (v0 >> 6) unless slot +8 is nonzero
((u0, v0) = first corner): 4-bit pages are 16 tiles of 64x64 texels, each with
its own CLUT [GPU]. Layer-0 renderer only: for slots 0..3 the UVs are halved or
quartered by depth (texture LOD, thresholds at scratch +0x88/+0x8A), the CLUT
still chosen from the full-size UV [CODE 80016b3c][GPU]; near polygons are
subdivided/clipped with interpolated UVs and colours.

### Kind 2/3 "parts" models [CODE 8002e13c] (none on DOCK; kept raw)
i32 n; n x 12 bytes (three i32 per part [INFER: pivot]); then per part i32 k
and k x 44-byte records [INFER: prebuilt primitives]. FUN_8002e13c counts the
records. Present on other tracks (e.g. STUNT/MOUNTAIN: 280).

### Visibility zone record [CODE 8002e3dc, 8005d908, 8005dc14, 80010000] [RAM]
| off | type | meaning |
|---|---|---|
| 0 | i16 | record length (DOCK 7378); next zone follows |
| 2 | u16 | unknown (0) |
| 4 | 256 | visibility mask at path node 0; MSB first per byte (0x80 = first object of the list FUN_80010000 walks). DOCK: bytes 0..0x49 (74 bytes = 589 layer-0 objects) and 0xE1..0xFF (31 bytes, matching the 243 layer-1 objects) are the only ones mask0 or the stream ever touch [INFER: layer 1 packed at the end] |
| 0x104 | ... | delta stream of (xor mask, byte index) pairs; a pair with mask 0 separates nodes; stepping node k -> k+1 applies the pairs after separator k (backwards: the same pairs) |

Zone selection: zone bbox = min/max of the zone chunk's points 0 and 1
(table DAT_800e3478: min xyz, max xyz, size2 = sum of ((max-min)>>5)^2, chunk
pointer, record pointer); nodes are the chunk's points 2.. (DOCK: 172 nodes =
172 separators); the current node is the nearest to the player (distances >> 4).

## Public Rust API (`mr_assets::dat`)
- `Dat::parse(&[u8]) -> Result<Dat, Error>`; fields `chunks: Vec<Chunk>`,
  `block128`, `objects: Vec<Object>` (layer 0 then layer 1, `layer0_count`),
  `special: Vec<Option<usize>>`, `descriptors: BTreeMap<u32, Descriptor>`,
  `meshes: BTreeMap<u32, Mesh>`, `parts: BTreeMap<u32, PartsModel>`,
  `orphans: Vec<Orphan>`, `vis: Vec<VisZone>`, `sections`; methods `layer`,
  `desc`, `mesh`, `transform(&Object, &SinTable) -> Transform`, `gaps`,
  `loader_output_size`, `loader_image(raw, base, &SinTable) -> LoaderImage`.
- `Object { offset, layer: Layer, centre, radius, flags, b0f, desc_offset }`,
  `loaded_flags()`; `Layer::{World, Quarter}`, `world_scale()`; `flag::*`.
- `Descriptor { offset, origin, angles, scale, b_lo, b_hi, b_extra, mesh,
  mesh2, kind: ModelKind }`, `matrix(&SinTable)`.
- `Mesh { offset, raw_count, vertices: Vec<Vertex>, groups: Vec<PrimGroup>,
  end }`, `prim_count`, `slot_count`; `Vertex { pos, color, extra }`, `rgb()`;
  `PrimGroup { offset, header, prims: Vec<Prim> }` with `kind`, `is_triangle`,
  `double_sided`, `cull_both_halves`, `double_slot`, `is_textured`,
  `blend_bits`, `semi_transparent`, `tex_slot`, `gpu_code`, `slots`;
  `Prim { idx, uv, raw }`.
- `TexSlot { tpage, clut, fixed_clut }`, `prim_tpage_clut(slot, &group, &prim)`.
- `SinTable::from_exe(&exe)` (reads the table from SLUS_012.37 on the disc),
  `rot_matrix`, `scale_matrix`, `Transform { m, origin, scale }` with
  `apply` (integer, as the game) and `apply_f`.
- `VisZone { chunk, bbox_min, bbox_max, size2, offset, len, b02, mask0,
  stream }`, `separators()`, `mask_at(node)`.

## Verification
Recomp: psxrecomp debug build, DOCK 1-player race, DAT at 0x80137E68,
2026-10-12. Read-only commands only (read_ram, gte_ring_dump, gpu_frame_dump,
frame).

    cd port && cargo build && ./target/debug/mr-dat --oracle
    cargo test -p mr-assets --test dat

`mr-dat` writes `analysis/dat/dock.obj`, `dock.mtl`, `dock_summary.txt`.
Numbers from the last run (frames 19499/19540):
1. RAM, loader reproduced on the disc file: 7,391 bytes of the file differ
   disc vs RAM; predicted vs RAM differ in 311 bytes, all 311 in the 11
   special-slot objects (their centre and descriptor pose move at run time),
   0 elsewhere. Object records: 821 of 821 static objects exact (19,704 bytes:
   matrices, descriptor pointers, slot counts). Special pose records: exact.
2. GTE ring (one frame, 1,580 RTPS/RTPT): every entry of the scenery callers
   ra 0x80028140 (402/402, layer 0) and 0x80028CC4 (709/709, layer 1) has all
   input vertices in one decoded mesh; 0x80016F98 354/396 and 0x800165A4
   43/63 (the rest are not DAT vertices).
3. Transform: for each such entry an object whose mesh holds the vertices and
   whose matrix gives RT = CAM * M (within 6) implies a camera term
   scale * (TR - CAM * origin); 1,508 of 1,570 entries agree on one camera term
   within 2.8 world units (layer 0: 799, layer 1: 709 with scale 4; 71
   objects); the 62 others have vertices not in any DAT mesh. Repeated on
   three different camera positions with the same result (max deviation
   2.4-3.1).
4. GPU (GP0 frame 19540, texture slots read from 0x800DFF00): of the packets
   whose screen points come from DAT vertices, G4 72/72 and G3 15/15 match a
   decoded primitive exactly (colours); GT4 52/80 exact (colours, UVs incl.
   LOD, CLUT, tpage), the other 28 are subdivided pieces with interpolated
   colours/UVs; GT3 1/22 exact, 18 interpolated, 3 flat-coloured subdivision
   pieces (UVs at midpoints 0x0E/0x1F/0x5F).
5. Visibility: RAM at node 44 (DAT_800b0d40 = 44): the mask at 0x800C1AC8
   equals mask0 with the deltas of nodes 0..43 applied (157 bits set) and
   DAT_800b0f98 = record + 0x7E2 = 0x104 + separator 44. Zone table
   DAT_800e3478 = {-13099, 0, 5861, -11786, 0, 7118, 3202, base+4,
   base+0x52554}.
6. Matrices hard-coded in tests from RAM: objects 0, 426, 427, 432, 589, 831
   (rotated, scaled, both layers), slot counts and descriptor pointer.
7. All 36 DATs on the disc parse with no unaccounted bytes (BOG, GRAVE, ISLAND,
   DISCO, DINER, MOUNTAIN have unreferenced meshes between models; DISCO has
   256 raw bytes there).

## Open unknowns
- Meaning of each point-list id (paths for AI / camera / items?).
- block128 (empty on DOCK), descriptor b_lo/b_hi/b_extra,
  object byte +0x0F, zone +0x02, flag bits 0x10/0x40.
- Layer-1 part of the visibility mask (bytes 0xE1..0xFF) and which code reads it.
- Kind 2/3 parts-model record layout; 12-byte-vertex meshes (negative count).
- The layer-0 renderer's subdivision/clipping rules and LOD thresholds
  (scratch +0x88/+0x8A), needed for pixel-exact rendering, not for the data.
- The object centre is the bbox centre only approximately (up to 77 units).

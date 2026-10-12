# TER: track terrain, collision and racing line

File: `<track>.TER` (DOCK: `COURSE_A/DOCK.TER`, 70,180 bytes). 36 files on the disc;
all 36 parse and re-encode byte for byte (`mr-ter all`).
Decoder: `port/crates/mr-assets/src/ter.rs`. CLI: `mr-ter`. Tests: `tests/ter.rs`.

Evidence tags: [CODE addr] read from the game's MIPS code (disassembled from
the EXE; Ghidra output used as a reading aid only), [RAM] matched the running
recomp, [INFER] unverified inference.

## Loading [CODE 8004627c, 80042068, 8003c2d8] [RAM]
FUN_8004627c reads `<track>.ter` via FUN_8005fdf4 to the heap pointer
DAT_800b0b3c, then FUN_8003c2d8 converts the header's section offsets into
absolute pointers and copies header words into globals. The file is used in
place and never transformed: on DOCK the 70,180 bytes at *0x800b0b3c
(0x8019474c) equal the disc file [RAM]. Only FUN_800421c4 / FUN_8004229c /
FUN_80042374 (switchable / moving groups) modify it later.

## Units and conventions
- World units = kart position units (kart record +0/+4/+8).
  TER units = world >> 5 (arithmetic shift) [CODE 8003ddb0, 8003d760].
- Y points down (PS1): a larger y is lower. Floors have normals with ny < 0.
- Heights the queries return are world units with sub-TER precision
  (y interpolated at << 10, result >> 5) [CODE 8003cf14].
- All math is i32: wrapping mult/mflo, truncating div (R3000 `div`).

## Header (0x84 bytes, 33 x i32 LE)
| word | off | DOCK | meaning | global | tag |
|---|---|---|---|---|---|
| 0 | 0x00 | 32 | grid width (cells in x) | 0x800b0d78 | [CODE 8003c2d8][RAM] |
| 1 | 0x04 | 32 | grid height (cells in z) | 0x800b0d7c | [CODE][RAM] |
| 2..13 | 0x08 | 0x84 ... | byte offsets of sections S0..S11 (below) | pointer globals | [CODE][RAM] |
| 14,15,16 | 0x38 | -20953, -3303, -29421 | bbox min x, y, z (TER) | 0x800b0b6c, b78, b80 | [CODE][RAM] |
| 17,18,19 | 0x44 | 46668, 4822, 44446 | bbox max x, y, z | 0x800b0b74, b7c, b88 | [CODE][RAM] |
| 20 | 0x50 | 2113 | cell size x = (maxx-minx)/w | 0x800b0bb4 | [CODE] |
| 21 | 0x54 | 8125 | y span = maxy-miny (stored, never read) | 0x800b0bb8 | [CODE] |
| 22 | 0x58 | 2308 | cell size z | 0x800b0bbc | [CODE] |
| 23 | 0x5c | 1368 | vertex count | 0x800b0b68 (unread) | [CODE] |
| 24 | 0x60 | 943 | face count | 0x800b0b84 | [CODE] |
| 25 | 0x64 | 927 | wall point count | 0x800b0fa8 | [CODE] |
| 26 | 0x68 | 672 | wall edge count | 0x800b0b34 | [CODE] |
| 27 | 0x6c | 0 | group count | 0x800b0f50 | [CODE 8003c4d4, 800421c4] |
| 28 | 0x70 | 5097 | cell-list entry count (u16s) | 0x800b0f44 | [CODE] |
| 29 | 0x74 | 21206 | "no water" y (TER); query starts water min at this << 5 | 0x800b0d38 | [CODE 8003ddb0][RAM 678592] |
| 30 | 0x78 | 357 | path point count | 0x800b0c54 | [CODE] |
| 31 | 0x7c | 27 | path segment count | 0x800b0d5c | [CODE 8009004c][RAM] |
| 32 | 0x80 | 48 | link byte count | 0x800b0be8 | [CODE] |

Section pointers: S0 verts 0x800b0f90, S1 faces 0x800b0fa4, S2 wall points
0x800b0e7c, S3 wall edges 0x800b0f5c, S4 groups 0x800b0e44, S5 cell lists
0x800b0e34, S6 cell list offsets 0x800b0f40, S7 floor counts 0x800b0fb0,
S8 wall counts 0x800b0f64, S9 path points 0x800b0ac4, S10 path segments
0x800b0c50, S11 links 0x800b0a84 [CODE 8003c2d8][RAM, all 12 = base + offset].
Sections are contiguous in this order; S5 and S11 are padded to 4 bytes with zeros.

## S0 vertices (12 bytes each)
| off | type | meaning | tag |
|---|---|---|---|
| 0 | i32 | x (TER) | [CODE 8003ddb0] |
| 4 | i32 | y (TER, down) | [CODE 8003cf14] |
| 8 | i32 | z (TER) | [CODE] |

## S1 faces (16 bytes)
| off | type | meaning | tag |
|---|---|---|---|
| 0,2,4,6 | u16 x4 | vertex byte offsets into S0 (index*12); v3 only for quads | [CODE 8003ddb0] |
| 8,10,12 | i16 x3 | unit normal (4096 = 1.0); kart up vector eases toward it | [CODE 8003e91c (kart +0x2d0..)]; matches -(v1-v0)x(v2-v0) on 882/943 DOCK faces, 39 reversed [test] |
| 14 | u8 | flags: 0x80 quad, 0x40 "alt" surface (ranked separately), 0x20 disabled; low 5 bits 0 on disc | [CODE 8003ddb0, 800421c4] |
| 15 | u8 | surface kind, copied to kart +0x4F3. 3 = water surface. Kind 12 + quad = directional surface (kart +0x476 = ratan2 of its edge midpoints) | [CODE 8003ddb0, 8003e91c]; names other than water [INFER] |
DOCK kinds: 0 (23), 3 (33 water), 4 (139), 6 (541), 15 (207).
Some faces on other tracks store a zero normal.

## S2 wall points (16 bytes)
| off | type | meaning | tag |
|---|---|---|---|
| 0 | i32 | x | [CODE 8003d760] |
| 4 | i32 | z | [CODE 8003d760] |
| 8 | i32 | y top (smaller = higher) | [CODE 8003d240] |
| 12 | i32 | y bottom | [CODE 8003d240] |
If top == bottom at both ends of an edge the height test is skipped: the
wall is infinitely tall [CODE 8003d240].

## S3 wall edges (4 bytes)
| off | type | meaning | tag |
|---|---|---|---|
| 0 | u16 | byte offset into S2 of point "B" (`Wall::p0`); bit 0 = disabled | [CODE 8003d760][RAM] |
| 2 | u16 | byte offset into S2 of point "A" (`Wall::p1`) | [CODE][RAM] |
RAM: after a hit, 0x800b0dac = edge, 0x800b0ed0 = A (second u16), 0x800b0ed8 = B
(first u16); four DOCK hits matched the decode (walls 579, 580, 268, 269).

## S4 groups (20 bytes; 0 on DOCK, up to 15 on other tracks)
| off | type | meaning | tag |
|---|---|---|---|
| 0 | u32 | always 0 on disc, unread | [CODE] |
| 4 | u16 | vertex byte offset (S0) | [CODE 80042374] |
| 6 | u16 | face byte offset (S1) | [CODE 8003c4d4, 800421c4] |
| 8 | u16 | wall edge byte offset (S3) | [CODE 8003c568] |
| 10 | i16 | vertex count | [CODE 80042374] |
| 12 | i16 | face count | [CODE] |
| 14 | i16 | wall count | [CODE] |
| 16 | u32 | always 0, unread | [CODE] |
FUN_800421c4 (enable) clears face flag 0x20 and wall bit 0; FUN_8004229c
(disable) sets them; FUN_80042374(group, dy) adds dy to the vertices' y and to
y top/bottom of both points of every wall (shared points move twice).
FUN_8003c4d4 / FUN_8003c568 map a face / wall pointer back to its group.

## S5..S8 the 2D grid (cells over x,z)
- cell = cz * grid_w + cx, cx = (x - minx) / cell_x, cz = (z - minz) / cell_z (TER).
- S6: u16 per cell, byte offset into S5 of the cell's list [CODE].
- S7: u8 per cell, number of floor entries; S8: u8 per cell, number of wall entries [CODE].
- S5: the list = S7[cell] face byte offsets (into S1), then S8[cell] wall-edge
  byte offsets (into S3) [CODE 8003ddb0 (floors), 8003d760 (walls at +S7*2)].

## S9 path points (16 bytes) — racing line
| off | type | meaning | tag |
|---|---|---|---|
| 0,4,8 | i32 x3 | x, y, z (TER) | [CODE 80071210 et al.]; inside segment bbox [test] |
| 12 | u16 | heading, 4096 = full turn | [CODE 80090424 sums deltas] |
| 14,15 | u8 x2 | unknown, 0 at 40 points, else ~55..255 | [INFER: half widths] |

## S10 path segments (36 bytes)
| off | type | meaning | tag |
|---|---|---|---|
| 0 | i16 | first point index (S9) | [CODE 80090424] |
| 2 | i16 | point count | [CODE 80090424] |
| 4 | i16 | index into S11 of this segment's links | [CODE 80090424] |
| 6 | i16 | successor count (first links) | [CODE 80090424, rand() picks a branch] |
| 8 | i16 | predecessor count (links after the successors) | [INFER, layout consistent on all segments] |
| 10 | i16 | flags (1, 2, 0x80 on DOCK) | [INFER] |
| 12..35 | i32 x6 | bbox min x,y,z, max x,y,z (TER) | [INFER; all points inside, test] |
## S11 links: u8 segment ids, zero padded to 4.

## Queries (ported in ter.rs)
### FUN_8003ddb0(x, y, z) -> `Ter::ground` [CODE][RAM]
Inputs world units; x,z >> 5. y is only used to rank by |h - y|. For the cell
of (x,z) (none if outside the grid: all results default), for each floor entry
in order, skipping disabled faces:
1. point-in-face with E(P,Q) = (x-Px)(Qz-Pz) + (z-Pz)(Px-Qx) >= 0. Quads:
   if E(v0,v2) >= 0 test triangle (v0,v2,v3) via E(v2,v3), E(v3,v0); else
   (v0,v1,v2) via E(v0,v1), E(v1,v2). Triangles: E(v0,v1), E(v1,v2), E(v2,v0).
2. h = FUN_8003cf14(x, z, triangle): sort the 3 vertices by z; base = lowest
   if z < mid.z or mid.z == top.z, else the top; interpolate x<<10, y<<10 along
   both base edges (slope divided first, then times dz), then across x
   (equal x: mean, truncating); return >> 5.
3. kind 3: water, keep the smallest h (0x800b0f8c face, 0x800b0a54 y; default
   no_water_y << 5). Flag 0x40: keep closest |h-y| (0x800b0c84 / 0x800b0c9c).
   Otherwise: keep closest |h-y| (0x800b0b0c / 0x800b0b24, default maxy << 5),
   and closest with h >= y (0x800b0ae0 / 0x800b0b04); set 0x800b0ae4 = 1.
   Ties keep the first. Comparisons on |h-y| are unsigned per face.
4. Chosen 0x800b0c00 = nearest; replaced by alt then water if strictly closer
   (signed compare). If no "below" face: below = nearest.
Kart use (FUN_8003e91c): called with y = (2*y + k[+0x438] + k[+0x43a]) / 2;
k[+0x2e0] = chosen floor face, k[+0x2e4] = water face, k[+0x31c] = ground y.

### FUN_8003d760(x0,_,z0,x1,_,z1) -> `Ter::wall_cross` [CODE]
Cell from (x1,z1) >> 5 with i16 truncations; for each enabled wall entry
(A = second u16 point, B = first): hit when E(A,B,p0) < 0, E(A,B,p1) >= 0,
E(A,p0,p1) >= 0 and E(p0,B,p1) >= 0. Walls are one-sided. On a hit the game
latches x0,z0 into 0x800b0bfc / 0x800b0c0c.
FUN_8003da18 runs the same geometric test (`Ter::wall_candidates`) and calls
FUN_8003d240 for each candidate: height test against the wall (A,B top/bottom
interpolated along the edge vs kart y + 0x800b0da4 / + 0x800b0f62 with a
step-size margin) and the slide response (rotation by ratan2 of the edge,
sin/cos tables 0x800a90c0 / 0x800a88c0). That part depends on kart state and
is NOT ported here; it belongs with the kart physics.

## Verification
- `cargo run -p mr-assets --bin mr-ter -- all`: 36 TER files, 0 problems
  (every section size exact, every offset divisible and in range, byte-exact
  round trip).
- `cargo run -p mr-assets --bin mr-ter -- oracle` (read-only read_ram of the
  2 MB, one request = one snapshot), DOCK race:
  `TER at 0x8019474c: RAM == disc bytes: true`;
  `FUN_8003c2d8 pointers/globals match: true`.
  Kart check (6 karts per snapshot, 100 snapshots, frames ~5500-6500 and
  ~11000-16500): 358 of 360 kart records match `Ter::ground` exactly at the
  current position (face pointer, ground y, water face); the other 2 match
  exactly at the kart's previous position (+0x2e8/+0x2f0), i.e. that kart had
  not re-run FUN_8003e91c since moving. Logs: analysis/ter/oracle_runs*.txt.
- `mr-ter oracle-dump analysis/ter/ram_dock_race1.bin`: snapshot with water
  under 4 karts; all 6 match incl. water faces 806/797/797/314, and the last
  call's globals (cell 13,8; nearest 543 y 99669; water 314 y 96000; chosen
  96000) equal `Ter::ground` for kart 5.
- Walls: 4 RAM hits (0x800b0dac/ed0/ed8 with the hitting kart's previous
  position) match the edge decode, the start side and `wall_candidates`.
- `cargo test -p mr-assets --test ter` (values above as constants).
- Eyeball: `mr-ter export` -> analysis/ter/dock_ter.obj (floor/water/alt/walls/path
  objects, y flipped up) and dock_ter.svg (top-down: faces, water blue, walls red,
  racing line green); the SVG shows the DOCK circuit with walls on both edges.

## Open unknowns
- Face kinds other than 3 (and 12-with-quad): game meaning (grip / sound /
  effects) not traced. Path point bytes 14/15. Segment +8 (predecessor count is
  inferred from layout) and +10 flags. Group words +0/+16 (always 0).
- FUN_8003d240 (wall height test + response) not ported.

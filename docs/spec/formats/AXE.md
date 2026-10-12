# AXE: sky dome

File: `<track>.AXE` (DOCK: `COURSE_A/DOCK.AXE`, 20,924 bytes). 12 files on the
disc (tracks whose flag bit 0 is set); all parse and round-trip byte for byte.
Decoder: `port/crates/mr-assets/src/axe.rs`. CLI: `mr-axe`. Tests: `tests/axe.rs`.
Tags: [CODE addr] from the MIPS code, [RAM] matched the running recomp, [INFER].

## What it is
A camera-centred shell of Gouraud-shaded quads: 16 segments around the y axis
(radius ~470-500, y from -443 at the top rim to +132 below the horizon), blue to
purple. FUN_80034ae4 loads the camera rotation (0x800c3008) with a ZERO
translation and calls FUN_80011a14(segments, count, n_face_ids), which
RTPS/RTPT-culls each segment by its 4 corner points against the screen, then for
each face not yet drawn (byte flag at 0x1F800020 + face_id, cleared for
(n_face_ids+7)/8*8 bytes) emits a POLY_G4 (code 0x38) linked into one
ordering-table slot (drawn behind everything). [CODE 80034ae4, 80011a14]

## Loading [CODE 8004627c, 800420cc, 80042130, 80034ba4] [RAM]
Read to DAT_800b0c4c if the track flag bit 0 is set. FUN_80042130:
0x800b0c48 = base + word0; 0x800b0c82 = (i16) word1 (segment count);
0x800b0db0 = first i16 at base + word0 (face id count); 0x800b0c64 = base + 8;
each segment's three offsets += base. FUN_80034ba4 then, per segment:
vertices: swap bytes 8 and 10, zero byte 11; faces: v0 = |v0|*12, v1..v3 *= 12,
face_id += 0x1F800020. Word 0 itself is not rewritten.

## Layout
| off | type | meaning | tag |
|---|---|---|---|
| 0 | u32 | offset of the tail block | [CODE 80042130][RAM] |
| 4 | i32 | segment count (16) | [CODE][RAM] |
| 8 | 44 x count | segment table | [CODE] |

Segment (44 bytes):
| off | type | meaning | tag |
|---|---|---|---|
| 0 | u32 | offset of vertex list | [CODE 80034ba4, 80011a14] |
| 4 | u32 | offset of face list | [CODE] |
| 8 | u32 | relocated but never read; on every file all segments point at one spare copy of words 1.. + table (708 bytes for 16 segments) | [CODE][test] |
| 12..35 | i16 x12 | 4 corner points (x,y,z) for segment culling | [CODE 80011a14] |
| 36 | i16 | vertex count | [CODE 80034ba4] |
| 38 | i16 | face count | [CODE 80034ba4, 80011a14] |
| 40 | u16 | unread (0) | [CODE] |
| 42 | u16 | unread (123) | [CODE] |

Vertex (12 bytes): i16 x, y, z (model units, y down) [CODE: lwc2 0/4];
i16 at +6 unread; +8..+10 colour B,G,R on disc (R,G,B after load); +11 zeroed
by the loader [CODE 80034ba4][RAM].

Face (12 bytes): i16 v0..v3 vertex indices into the segment's list (v0 may be
negative, abs taken); u32 face_id < n_face_ids, shared by faces duplicated in
neighbouring segments [CODE]. Drawn as G4 with xy0..3 = v1, v0, v2, v3 (strip
triangles (v1,v0,v2), (v0,v2,v3)), colour of the same vertex at each corner.
Quads never share vertices (4 per face).

Tail: i16 n_face_ids (DOCK 120), then zero bytes to end of file (unread).

## Verification
- `cargo run -p mr-assets --bin mr-axe -- oracle` (read-only, DOCK race):
  `AXE at 0x801a5970`, `RAM == to_ram_image(base): true (0 differing bytes)`,
  15,173/20,924 bytes unchanged from the file; 0x800b0c48 = 0x801aaab0,
  0x800b0c64 = 0x801a5978, 0x800b0c82 = 16, 0x800b0db0 = 120 all as predicted.
- `mr-axe all`: 12 files, 0 problems. `cargo test -p mr-assets --test axe`.
- DOCK: 16 segments, 1292 vertices, 323 faces, 120 distinct face ids =
  120 drawn quads after dedupe.
- Eyeball: `mr-axe export` -> analysis/axe/dock_axe.obj (vertex colours) and
  dock_axe.svg (top-down ring of quads, cyan to purple).

## Open unknowns
- Segment +40/+42 (0, 123) and vertex +6: unread by the game.
- Not yet checked on screen (VRAM) — RAM image and the draw code are the evidence.

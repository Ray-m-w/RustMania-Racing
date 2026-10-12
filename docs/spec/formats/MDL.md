# MDL: character and vehicle models (`CHARS_A/B/*.MDL`, `VEHIC_A/B/*.MDL`, `BOSS/*.MDL`)

Decoder: `port/crates/mr-assets/src/mdl.rs`. CLI: `mr-mdl` (`src/bin/mr-mdl.rs`).
Tests: `port/crates/mr-assets/tests/mdl.rs`. Companion file: [ANM.md](ANM.md).

Evidence tags: **[CODE addr]** read from the game's code (Ghidra decomp of that
function, cross-checked by the result), **[RAM]** matched the running recomp
(RAM, VRAM, GTE ring or GPU packet ring), **[INFER]** inference, not verified.
"[data]" = holds for all 65 `.MDL` files on the disc (test
`every_mdl_on_disc_parses_fully`).

## Who loads it

- FUN_80092b08 (characters, names from the table at 0x800ad1cc, e.g.
  `chars_a\kermit`) and FUN_80092928 (vehicles, table 0x800ad10c, e.g.
  `vehic_a\racer`) call FUN_800925a8(name, &slot, &heap, model, ...). [CODE]
- FUN_800925a8 first loads `<name>.anm` at the heap pointer (unless the
  course `.RAW` provided one, type 0x101 chunk -> DAT_800d8e90[model]), then
  FUN_8003c270 loads `<name>.mdl` right after it with FUN_8005fdf4 and calls
  FUN_8003bf58 (unless a type 0x102 `.RAW` chunk provided the model,
  DAT_800d8d08[model]). [CODE 800925a8, 8003c270] [RAM: KERMIT.ANM at
  0x800ebcdc (80 bytes), KERMIT.MDL at 0x800ebd2c]
- FUN_8003bf58 walks the part table and fills one 0x6C-byte model slot per
  mesh part at 0x800bc668 + 0x6C*slot; it returns the number of slots, and
  the heap pointer advances only past the part data, so the table at the end
  of the file is overwritten by the next load. [CODE 8003bf58] [RAM: KERMIT
  slots 20..22, RACER slots 108..112; the table bytes are gone from RAM]
- FUN_800384b4 then runs the seam pass over the necks (see Seam rings). [CODE]

## File layout

| Offset | Type | Meaning | Tag |
|---|---|---|---|
| 0 | u32 | `T`: part table offset in halfwords (table at byte `2*T`) | [CODE 8003bf58] [data] |
| 4 | part data | parts back to back, in table order; part *i* is `size_hw*2` bytes | [CODE 8003bf58: `*heap += size*2`] [data] |
| 2T | u32 | part count *n* | [CODE] |
| 2T+4 | record[n] | 76-byte part records; they end exactly at end of file | [CODE: stride 0x13 words] [data] |

All bytes are accounted for: header word, part data tiling [4, 2T), count,
records. KERMIT: T = 0x96E (table 0x12DC), 5 parts (3 meshes, 2 rings).
RACER: T = 0xB64, 5 meshes.

### Part record (76 bytes)

Offsets from the record start (the decomp indexes it from the count word,
i.e. its offsets are these + 4).

| Off | Type | Meaning | Tag |
|---|---|---|---|
| 0x00 | u32 | part data length in halfwords | [CODE 8003bf58] [data] |
| 0x04 | i32[3] | pivot: the part's origin in model space (x, y, z; y down). Copied to slot +0x00..+0x08 | [CODE] [RAM: slots 20 (0,-145,-28), 108 (154,-62,-199) ...] |
| 0x10 | u32 | kind: <0x100 mesh (1 in every file); 9 = attachment point (point 0 copied to DAT_800b0f80, slot kind 0xC, removed again by the ANM loader); >=0x100 shared block linked into list DAT_800d74a8[kind] (0x11F = seam rings, list head DAT_800d7924) | [CODE 8003bf58, 800925a8] [data: only 1 and 0x11F occur] |
| 0x14 | u8[6] | 0 | [INFER unused] [data] |
| 0x1A | i16 | tail mode: !=0 -> part has data after its polygon list; slot kind += 4 (1) or 8 (other), slot +0x0C = address after the list | [CODE] [data: always 0] |
| 0x1C | u8[12] | 0 | [INFER unused] [data] |
| 0x28 | i16[2] | 255, 0 for meshes; 0, 0 for rings | [INFER unused] [data] |
| 0x2C | 3 x (i16 x, y, z, i16 w) | three points; for meshes they bracket the vertices (KERMIT head (-87,-94,-11) (88,-94,-87) (87,22,88)) | [INFER: export-tool bounds; only kind 9 reads point 0] |
| 0x32 | i16 | point 0 `w` -> slot +0x28 | [CODE] [data: 0] |
| 0x3A | i16 | point 1 `w` -> slot +0x2C: for a neck, ring A number (low byte) and ring B number (high byte); KERMIT neck = 0x0001 -> rings 1 and 0 | [CODE 800384b4] [RAM: slot 22 +0x2C = 0x0C0D after the seam pass rebased both bytes] |
| 0x42 | i16 | point 2 `w` -> slot +0x2A (the seam pass copies it to +0x28 and treats <2 as "stitch") | [CODE] [data: 0] |
| 0x44 | u16 | 0 | [INFER unused] [data] |
| 0x46 | u16 | flags: 0x40 -> slot kind += 2 | [CODE] [data: always 0] |
| 0x48 | u32 | the part's data offset in halfwords from file offset 4 (0 for rings) | [INFER] [data: holds for every mesh part] |

### Mesh part data: polygon list

Records back to back until a word whose byte 3 (code) is 0; that zero word
is part of the part (part size = polygons + 4). [CODE 8003bf58 counts
polygons until code 0; FUN_8001f040 stops on (word & 0x3C000000) == 0] [data]

Triangle (code & 0x7C == 0x34), 0x24 bytes:

| Off | Type | Meaning | Tag |
|---|---|---|---|
| 0x00 | u8 r,g,b, u8 code | colour of v0 and GPU command (0x34 GT3, 0x36 semi-transparent) | [CODE 8001f040] [RAM: GPU word0 0x3471867D for KERMIT poly 0] |
| 0x04 + 8k | i16 x,y,z, u8 u,v | vertex k (k = 0..2), local to the part; (u,v) inside the 256x256 page | [CODE] [RAM: GTE RTPT V0..V2] |
| 0x1C | u8 r,g,b, u8 `material` | colour of v1; high byte 2..25, read by no code found | [CODE colour] [INFER material id] |
| 0x20 | u8 r,g,b, u8 `tex` | colour of v2; `tex` see below | [CODE] |

Quad (code & 0x7C == 0x3C), 0x30 bytes: code word, 4 vertices at 0x04+8k,
then colour words of v1 (0x24, high byte always 0 [data]), v2 (0x28, high
byte `material`), v3 (0x2C, high byte `tex`). Vertices are in perimeter
order; the emitter feeds the GTE v0, v1, v3 (RTPT) then v2 (RTPS) and the
GPU packet gets them in that order (GPU quad Z order). [CODE 8001f040]
[RAM: 100% of KERMIT/RACER quads in the GTE ring as RTPT(v0,v1,v3)+RTPS(v2);
GPU packet UVs in that order]

Colours are 0x80 = full texel brightness (modulated by GTE DPCL with the
fog colour). [CODE 8001f040]

`tex` byte (high byte of the last colour word):

| Bits | Meaning | Tag |
|---|---|---|
| 0-4 | slot of the course texture table at 0x800dff00 (16 bytes per slot: u16 tpage, u16 clut, u16 x, u16 y, u16 depth), built by the `.RAW` loader (see RAW.md) | [CODE 8001f040] [RAM] |
| 5-6 | added to the slot's tpage (GPU semi-transparency mode bits) | [CODE] |
| 7 | draw without back-face culling (triangle: byte 0x23 < 0; quad: word 0x2C < 0) | [CODE] |

Only the **first polygon's** `tex` byte selects slot and blend bits for the
whole part (the emitter reads it once); every polygon in every file uses one
slot per part [data]. Per polygon:

- tpage = slot.tpage + (tex & 0x60)
- clut = slot.clut + ((v0 >> 6) & 3) * 4 + ((u0 >> 6) & 3), with (u0, v0)
  the first vertex's texel coordinates: each 64x64 cell of the page has its
  own CLUT. [CODE 8001f040: `((s >> 12) & 0xC) + ((s >> 6) & 3)` of the
  halfword at +0x0A] [RAM: KERMIT tex 0x10, slot 16 = 0x0004/0x3FF0, uv in
  cell (0,1) -> GPU clut 0x3FF4; RACER wheels cell (2,0) -> 0x3FF2; body
  slot 18 = 0x0006/0x3F70, cells -> 0x3F70 and 0x3F7E]

Back faces: a polygon is culled when GTE NCLIP of its first three GPU-order
screen vertices is negative (screen y down, so visible = clockwise) unless
bit 7; a kart-mirror flag DAT_800b0d14 flips the test. [CODE 8001f040]

### Ring part data (kind >= 0x100 with the marker)

| Off | Type | Meaning | Tag |
|---|---|---|---|
| 0 | u32 | 0x12345678 on disc; the loader replaces it with the previous list head and makes this block (+4) the new head of DAT_800d74a8[kind] | [CODE 8003bf58] [RAM: 0x00000000 / 0x800ECF7C] |
| 4 | u16 | `segments` *s*: the ring has *s*+1 vertices, the last repeating the first | [CODE 800384b4] [data] |
| 6 | u16 | id = (owner part index + 1) * 100 + ring number (KERMIT: 101 head ring 1, 200 body ring 0; RIZZO 103, 202). The seam pass rewrites it in RAM to ring + (first slot + owner) * 50 | [CODE 800384b4] [RAM: 101 -> 2001, 200 -> 2100] |
| 8 + 8k | i16 x,y,z, u16 side | vertex k in the owner part's local frame; `side` alternates 0/1 | [RAM: GTE RTPS inputs of the seam renderer, under the owner part's matrix] [INFER side] |

## Seam (neck) parts

A neck part (point-1 `w` != 0) is a mesh whose quads join ring A and ring B.
In the rest pose its vertices equal ring vertex + owner pivot (KERMIT: all
16 ring vertices) [data, test `kermit_matches_ram_and_gpu`]. At load,
FUN_800384b4 / FUN_800398f8 reorder the neck's quads and rotate their
vertices in place so each quad lines up with consecutive ring vertices, set
its slot kind to 2, and per frame FUN_80039d90 transforms both rings (RTPS
under the head and the body matrix) and draws the neck from them.
[CODE 800384b4, 800398f8, 800276e8 -> 80039d90] [RAM: neck bytes in RAM are
a permutation of the file's quads (229 of 340 bytes moved, same vertex
sets); ring vertices found in the GTE ring]. The decoder returns file order;
the reordering is runtime-only and not reproduced.

## Placing the parts (kart record -> part transforms)

FUN_80026F3C (vehicle, model index = kart byte +0x4A4) and FUN_800276E8
(driver, model = kart byte +0x4A5) loop over the model's slot range from the
ANM header (+4..+6) and place every part from the kart record (player kart
at 0x800C30A8, 0x504 bytes). The GTE light matrix L and back-colour BK are
used as scratch for the base matrix and base position (FUN_80010504). Ported
in `port/crates/mr-bevy/src/kartmodel.rs` (`KartModel::part_transforms`,
`neck_vertices`). All matrices 4.12, products `(a*b)>>12` per element.

Vehicle [CODE 80026F3C]:

- base position P = (i32 +0x00 + s16 +0x46A, i32 +0x04 + s16 +0x46C,
  i32 +0x08 + s16 +0x46E) >> 5 (model units = world / 32);
- base matrix B = 9 x s16 at +0x18C (the body matrix);
- for part i (slot order):
  - if i == byte +0x171: B = matrix at +0x1AC from this part on (RACER: the
    body, part 4);
  - if i < byte +0x171 (wheels): pivot.y = s16(+0x2E + 0x2C*i) -
    ((i32 +0x348 * (0x80 - s16 +0x180)) >> 15) (suspension; x, z from the MDL);
  - part matrix Pm = 9 x s16 at +0x20C + 0x12*i if i < byte +0x170, else
    identity (wheel spin about X and steering);
  - R_i = B * Pm; T_i = P + (B * pivot) >> 12 (FUN_800106C0, stored in slot
    +0x4C); a model point p of the part goes to T_i + (R_i * p) >> 12
    (FUN_80010A98, slot +0x38). Slot +0x1E == 1 would select a scaled path
    (FUN_80010DC8 + FUN_800108B8 with three s16 at slot +0x18); 0 for every
    KERMIT/RACER slot [RAM].
  - skipped when byte +0x4F1 == 8 and i >= i32 +0x300.

Driver [CODE 800276E8]: P = (i32 +0x330, +0x338, +0x340) >> 5, B = matrix at
+0x1CC (both written per tick by FUN_80025D8C from the body matrix and the
driver's lean), Pm = matrix at +0x29C + 0x12*i for parts 0, 1 (head, body),
identity otherwise; same R_i, T_i; skipped when +0x4F1 == 8 and i >= i32
+0x304. Slots of kind 2 (the neck) go to FUN_80039D90 instead: each ring
vertex is transformed by its owner part's R/T (ring 1xx: head, 2xx: body),
and the neck quads are drawn between them, so in world space each neck vertex
= T_owner + (R_owner * ring_local) >> 12 [RAM: GTE RTPS of both rings under
the head / body RT].

For the main view the game folds the camera in first: B' = V * B and
P' = V * (P*32 - eye) >> 17 (V, eye from the camera record at 0x800C4F40),
so the GTE gets RT = V*B*Pm and TR = view-space T. The world-space form above
differs only in rounding. The s16 at +0x414/+0x418/+0x41C (vehicle) and
+0x416/+0x41A/+0x41E (driver) are colour factors for the depth-cue (DPCL)
of the emitter, not geometry; when all are 0x1000 FUN_8001EC2C is used
instead of FUN_8001F040.

Verification [RAM, 2026-10-12, live DOCK race]: one read of
0x800BC668..0x800C4F80 (slots, kart record, camera); for all 5 RACER parts
and KERMIT head/body, V * R_i equals the game's slot +0x38 matrix within +-2
in rows 1-2 (row 0 is the widescreen mod's x scale, 1.6x), and V*(T_i*32 -
eye)/32 equals slot +0x4C within +-1 in y, z. Bevy side-by-sides (oracle |
mr-bevy, lap1 trace ticks 299, 319, 450): analysis/bevy/sbs_parts_t*.png.

## Coordinates

Model units, x right, y down, z forward (RACER nose at -z). A vertex's model
position is `local + pivot`. `Mdl::to_obj` writes (x, -y, -z) (a 180-degree
turn about X, y up) and reverses the winding so OBJ front faces are CCW.

## Public Rust API (`mr_assets::mdl`)

- `Mdl::parse(&[u8]) -> Result<Mdl, MdlError>`; `Mdl { table_hw, parts }`,
  `meshes()`, `rings()`, `slot_count()`, `to_obj(name, mtllib, material_fn)`.
- `Part { offset, record: PartRecord, data: PartData }`, `polys()`,
  `tex_flag()`, `seam_rings()`; `PartData::{Mesh { polys, tail }, Ring(SeamRing), Other(Vec<u8>)}`.
- `PartRecord` (all 76 bytes as fields, incl. `pivot`, `kind`, `points`,
  `point_w`, `slot_kind()`).
- `Polygon { offset, code, verts: Vec<Vertex { pos, uv }>, colors, flags }`,
  `tex_flag()`, `tex_slot()`, `double_sided()`, `semi_transparent()`,
  `material()`, `clut_offset()`, `gpu_order()`.
- `SlotAttr { tpage, clut }`, `tpage_clut(part_flag, slot, poly) -> (tpage, clut)`.
- `SeamRing { link, segments, id, verts }`, `owner_part()`, `ring_number()`.
- `parse_polys(bytes, base, part)`, `RamSlot::parse` (slot +0x00..+0x36),
  constants `RING_MARKER`, `RECORD_LEN`, `KIND_MESH`, `KIND_SEAM_RING`,
  `SLOT_TABLE_ADDR`, `SLOT_LEN`.

## Verification

Run on 2026-10-12 against the psxrecomp debug build in a 1-player DOCK race
(TCP 127.0.0.1:4370, read-only commands only):

```
cd port && cargo test -p mr-assets --test mdl      # 4 passed
cd .. && port/target/debug/mr-mdl --oracle         # ALL PASS
```

`mr-mdl --oracle` output (abridged):

- RAM: KERMIT parts 0, 1 == RAM slots 20, 21 byte for byte (1960 bytes at
  0x800EBD30, 2380 at 0x800EC4D8); neck (slot 22, kind 2) a permutation of
  the file's quads; RACER parts 0..4 == slots 108..112 byte for byte (604 x4,
  3412). Slot pivots and counts equal the records.
- GTE ring (6 VBlanks, 7805 entries): KERMIT 50/50 + 55/55 polygons
  (ra 0x80027C84, FUN_800276e8), RACER 13/13 x4 + 78/78 (ra 0x80027604,
  FUN_80026f3c) found as RTPT inputs with exact s16 vertices; quads as
  RTPT(v0,v1,v3) followed by RTPS(v2). KERMIT rings 101 and 200: 8/8
  vertices each as RTPS inputs (ra 0x80039D4C / 0x80039C24) under the head
  and body matrices.
- VRAM: texture pages (256,0) and (384,0) 64x256 and CLUT rows (768,253),
  (768,255) decoded from COURSE_A/DOCK.RAW by raw.rs equal the game's VRAM
  (0 differing pixels).
- GPU packets: KERMIT 71/71 and RACER 98/98 drawn polygons (matched by
  vertex count + first three UVs in packet order) carry exactly the tpage
  and CLUT `tpage_clut` predicts.
- Disc-wide: all 65 `.MDL` parse with every byte accounted for, 8079
  polygons, one texture slot per part, every neck's ring numbers exist.

Exports (gitignored, analysis/mdl/): `kermit.obj/.mtl`, `racer.obj/.mtl`,
`<stem>_slot10.png` / `racer_slot12.png` (the page through its 16 cell
CLUTs), `<stem>_preview.png` (textured software render, 4 views),
`<stem>.txt` (every record and polygon).

## Open unknowns

- Polygon `material` byte (2..25) and record fields 0x28 (255), the three
  bounding points: not read by any game code found.
- Ring vertex `side` halfword (alternating 0/1).
- Kind 9 (attachment point), tail mode and flag 0x40 are implemented from the
  code only; no file on the disc uses them (they may come from `.RAW`
  type-0x102 embedded models).
- The neck reordering (FUN_800384b4/800398f8) is documented, not ported.

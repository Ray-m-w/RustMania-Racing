# ANM: model companion files (`*.ANM` next to every `*.MDL`)

Decoder: `port/crates/mr-assets/src/anm.rs`. CLI: `mr-anm` (`src/bin/mr-anm.rs`).
Tests: `port/crates/mr-assets/tests/anm.rs`. See also [MDL.md](MDL.md).

Evidence tags: **[CODE addr]**, **[RAM]**, **[INFER]** as in MDL.md;
"[data]" = holds for all 65 `.ANM` files on the disc.

## Summary

Despite the name, the game uses an `.ANM` only at load time: its header
becomes the model's record in the model table DAT_800dd598 (slot range of
the model's parts), and its channel table marks which part is a neck (-2).
The per-part pose frames are never read. Wheel spin, steering and the
driver's animation come from per-part matrices in the kart object (MDL.md,
"What drives the parts"). Evidence: the only functions that reference
DAT_800dd598 (80025d8c, 80026a5c, 80026f3c, 800276e8, 80060944, 8006e1d4,
80092ccc, 80092f20, 800925a8) read +0, +4, +6 only; the globals
FUN_80092458 sets from the block (DAT_800b0a78, DAT_800b0c08, DAT_800b0ab2,
DAT_800b0ec6) have no reader in the Ghidra xrefs nor gp-relative loads in
the generated C (gp = 0x800AFEC8). [CODE census] Consistently, 24 of the
208 pose records disagree with the MDL pivots the game actually uses (all
zero, or off by a unit), never for KERMIT or RACER. [data]

## Who loads it

FUN_800925a8(name, &slot, &heap, model, &out, ...): unless the model table
entry is already set, takes DAT_800d8e90[model] (a `.RAW` type 0x101 chunk)
or loads `<name>.anm` at the heap pointer with FUN_8005fdf4 and advances the
heap by the size rounded up to 4; stores the pointer in
DAT_800dd598[model]; writes +0 = 1 and +4 = first slot; calls
FUN_80092458(anm, 1); loads the `.MDL` (MDL.md); writes +6 = first slot +
slot count (minus one if the last part was kind 9). [CODE 800925a8]
[RAM: KERMIT.ANM at DAT_800dd598[7] = 0x800EBCDC with +0 = 1, +4/+6 = 20/23;
RACER.ANM at [33] = 0x8010AA88, 108/113]

## File layout

| Off | Type | Meaning | Tag |
|---|---|---|---|
| 0 | u16 | 0 on disc; set to 1 when loaded (renderers test it) | [CODE 800925a8, 80026f3c] [RAM] |
| 2 | u16 | *n*: number of block offsets (2 in every file) | [CODE 80092458] [data] |
| 4 | u16 | 0 on disc; first model slot | [CODE] [RAM] |
| 6 | u16 | 0 on disc; end model slot (exclusive) | [CODE] [RAM] |
| 8 | u32[n] | block offsets from the file start, 0 = none; relocated to pointers in place | [CODE 80092458] [RAM: 0x10 -> 0x800EBCEC] |
| 8+4n | block | the one block (offset 16; slot 0 in 62 files, slot 1 in PIGGY, BIGTRUCK, BUBBLE) | [data] |

The file ends exactly at the end of the block. [data]

### Block

| Off | Type | Meaning | Tag |
|---|---|---|---|
| 0 | i16 | -(header size) = -16. (A non-negative value would mean a 12-byte header; FUN_80092458 supports it, no file uses it) | [CODE 80092458] [data] |
| 2 | i16 | 3 | [INFER unknown] [data] |
| 4 | i16 | 2 | [INFER unknown] [data] |
| 6 | i16 | part count (same as +10) | [data] |
| 8 | i16 | frame length in halfwords | [data: frames tile exactly] |
| 10 | i16 | part count *p* (the loader reads this one) | [CODE 80092458] |
| 12 | i16 | frame count (2) | [data] |
| 14 | i16 | -1 | [INFER terminator] [data] |
| 16 | i16[p] | channel per model part: >= 0 = offset of the part's pose record in a frame (halfwords); -2 = neck, its slot (first + i) is appended to the seam list DAT_800d3dd0 | [CODE 80092458] [data: -2 exactly on MDL neck parts] |
| 16+2p | u16 | scale mask: bit i set = part i's record has a scale triple | [data: record lengths 5/8 match the mask in all 65 files] |
| 18+2p | i16[frames x frame_len] | pose frames; the two frames are identical in every file | [data] |

### Pose record (5 or 8 halfwords)

| Off | Type | Meaning | Tag |
|---|---|---|---|
| 0 | i16[3] | pivot x4 (KERMIT head (0,-580,-112) = 4 x (0,-145,-28)) | [data: equals 4 x MDL pivot in 184/208 records] [INFER: unused by the game] |
| 6 | i16 | 0 | [data] |
| 8 | i16 | 0, or 1168 on rotating parts of DUMPER, TRIKE, ROLLER | [INFER: angle, 4096 = 360 deg] |
| 10 | i16[3] | scale 4.12 (only with the mask bit): RACER rear wheels 3522, LOVEBUG 4096/3540/3540, SNOW 4505 | [INFER: unused; RAM GTE matrices of RACER rear wheels have unit rows] |

KERMIT.ANM (80 bytes): channels [0, 5, -2], mask 0b100 (bit on the neck),
pose head (0,-580,-112), body 0. RACER.ANM (168 bytes): channels
[0, 5, 10, 18, 26], mask 0b01100, poses = 4 x the MDL wheel pivots, rear
wheels with scale 3522.

## Public Rust API (`mr_assets::anm`)

- `Anm::parse(&[u8]) -> Result<Anm, AnmError>`; `Anm { loaded_flag,
  block_slots, first_slot, end_slot, block_offsets, blocks }`,
  `active_block()` (the first non-zero offset, as FUN_80092458 picks it).
- `Block { offset, header: [i16; 8], channels, scale_mask, frames }`,
  `part_count()`, `frame_len_hw()`, `frame_count()`, `seam_parts()`.
- `Channel::{Pose(u16), Seam}`; `Pose { pos4, rot, scale }`, `pivot()`.

## Verification

```
cd port && cargo test -p mr-assets --test anm       # 2 passed
cd .. && port/target/debug/mr-anm --oracle          # ALL PASS
```

- RAM (DOCK race, 2026-10-12): KERMIT.ANM == DAT_800dd598[7] @0x800EBCDC
  and RACER.ANM == DAT_800dd598[33] @0x8010AA88 byte for byte except +0
  (= 1), +4/+6 (slots 20..23, 108..113, matching the MDL slots found by
  `mr-mdl --oracle`) and the relocated block offset (0x10 -> base + 0x10).
- Disc-wide: all 65 parse with every byte accounted for; one channel per
  MDL mesh part; -2 exactly on the neck parts; 184 of 208 poses / 4 equal
  the MDL pivots (all KERMIT and RACER ones).

## Open unknowns

- Header halfwords +2 (3), +4 (2), +14 (-1); pose halfword 8 (1168).
- Whether any code outside the census reads the poses through a pointer
  copied elsewhere (none found; RAM copies are untouched by the game).

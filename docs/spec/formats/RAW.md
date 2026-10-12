# RAW: texture container (`<course>.RAW`, `FRONTEND/*.RAW`, `GFX/*.RAW`)

Decoder: `port/crates/mr-assets/src/raw.rs`. CLI: `mr-raw` (`src/bin/mr-raw.rs`).
Tests: `port/crates/mr-assets/tests/raw.rs`.

Evidence tags: **[CODE addr]** read from the game's code (Ghidra decomp of that
function, cross-checked by the result), **[RAM]** matched the running recomp
(VRAM or RAM), **[INFER]** inference, not verified.

## Who loads it

- FUN_8004627c (track loader) builds the name from the course-name table at
  0x800acaa0 indexed by DAT_800b0fac (3 = `course_a\dock`, 30 =
  `frontend\options`, 29 = `frontend\the_end`, ...), appends `.raw`, and calls
  FUN_800441d8. For course 10 (frontend) it loads `frontend\mem_0<n>.raw`
  instead. Before that it resets the slot table at 0x800dff00 (tpage
  GetTPage(0,0,0,0), x = y = 0, clut 0x7fe8 for slots 0..0x17). [CODE 8004627c] [RAM: unused slots 6, 7 read back as clut 0x7fe8]
- FUN_8005df78 (loading screens) calls FUN_800441d8 with `r_mania.raw`,
  `midway.raw`, `legal1.raw`, `legal2.raw` or a random `load_<nn>.raw`. [CODE 8005df78]
- FUN_800441d8 reads the whole file to the top of RAM (below 0x801aede0) with
  FUN_8005fdf4 and walks the chunks. With bit 0x800 of DAT_800b0cf8 set it
  instead processes one chunk per call from DAT_800b0d84 (streaming mode, all
  cursors reset per call); `raw::decode` implements whole-file mode only. [CODE 800441d8]

All 70 `.RAW` files on the disc (ARENA, COURSE_A/B/C, STUNT, FRONTEND, GFX)
share the container below: every chunk's RNC length and both CRCs check, and
every file ends with the terminator plus 0..7 zero bytes. [RAM-free check:
test `every_raw_on_disc_decodes`]

## Container

| Offset | Type | Meaning | Tag |
|---|---|---|---|
| 0 | chunk[] | RNC ProPack method-2 chunks, back to back | [CODE 800441d8] [RAM] |
| end | u8[8] | `FF FF FF FF FF FF FF FF` terminator; the loader stops at the first four | [CODE 800441d8] |
| end+8 | u8[0..7] | zero padding | [INFER: padding] |

### Chunk header (14 bytes, big-endian): RNC header without the `RNC\x02` signature

| Offset | Type | Meaning | Tag |
|---|---|---|---|
| 0 | u32 BE | unpacked length (game returns it, does not check it) | [CODE 800203f0] [RAM: unpacked chunks equal RAM] |
| 4 | u32 BE | packed length; next chunk = this + 14 + packed length | [CODE 800441d8] |
| 8 | u16 BE | CRC-16 (poly 0xA001 reflected, init 0) of the unpacked bytes | [INFER: RNC standard; holds for all 70 files] |
| 10 | u16 BE | CRC-16 of the packed bytes | [INFER: same] |
| 12 | u8 | RNC leeway; unused by the game | [INFER] |
| 13 | u8 | RNC pack-chunk count; unused by the game | [INFER] |
| 14 | u8[packed] | RNC2 bit stream (8-bit shift register, MSB first, two leading bits skipped; literal / len 2 short offset / len 3 / len 4..8 / literal run (n+3)*4 / len byte+8; `1111` + byte 0 + bit 0 = end) | [CODE 800203f0] |

### Unpacked chunk: u32 LE type at +0, then per type

Texture page, type < 0x20 [CODE 800441d8] [RAM]

| Offset | Type | Meaning |
|---|---|---|
| 0 | u32 | type = slot index into 0x800dff00 table |
| 4 | i16 | width in texels (256 or 128) |
| 6 | i16 | height (256, 192, 128) |
| 8 | u16 | palette byte length (0x300); texels start at 0xC + (len/2)*2 |
| 10 | u16 | mode & 3: 0 = 4-bit, 1 = 8-bit, else 15-bit |
| 12 | u8[256][3] | palette, R,G,B; (0, >=0xFB, 0) is the transparent key |
| 0x30C | u8[] | texels, width/2 (4-bit) or width (8-bit) bytes per row |

Loader actions for a page (in this order):
1. Types 0..3, unless course == 0x1E: FUN_80044008 reduces the texels in
   place (rows 0..h/2 = every 2nd texel of every 2nd row; then rows 0..h/4 =
   every 4th texel of every 4th reduced row; rest of the page unchanged).
   The reduced data is what is uploaded. [CODE 80044008] [RAM: types 0..3 match VRAM 100% only with it]
2. CLUT rows, each 256x1 at x = 0x300 (one 256-entry row; 4-bit textures
   index 16-entry sub-rows):
   - type 0xD: row 0xC0, plain conversion. [CODE]
   - type > 5: row = counter starting 0xFF, decremented per use. [RAM]
   - type 0..5: rows t+0xC1 and t+0xC7 (plain), then if bit 0x10 of the
     course flags is clear rows t+0xCD/t+0xD3/t+0xD9 blended toward the
     course colour by 0x40/0x80/0xC0 (FUN_80045c48, t' = isqrt(a*(a/4+192)),
     c = (fog*t' + c*(255-t') + 0x7F)/0x7F8 per channel) [CODE 80045c48];
     else rows t+0xCD/t+0xD3 desaturated toward each 16-entry row's mean by
     0x80/0xE0 (FUN_80045f1c) [RAM: DOCK uses this path, 100%].
   - Conversion: c = R>>3 | G>>3<<5 | B>>3<<10; 0 -> 1; key -> 0x0000;
     high byte |= DAT_800d89b0[i] (0x80 for all 256, set by the loader) so
     every opaque entry has the STP bit. [CODE] [RAM: table read back all 0x80]
3. Upload at cursor A (starts (0,0)): width in halfwords = w/4, w/2 or w;
   if cursor x > 0x2C0 and h > 0xC0, h = 0xC0; if cursor x > 0x2FF, h = 0xC0
   (keeps 0x300.. rows 0xC0..0xFF free for CLUTs). Then cursor x =
   (x + w_hw + 0x3F) & ~0x3F. Types 0xD/0xE go to x = 0x3C0 (0xD at y = 0x100;
   0xE with h = 0x80) and do not move the cursor. [RAM for the normal path; CODE for 0xD/0xE]
4. Slot table 0x800dff00 + 16*type: +0 tpage = GetTPage(mode,0,x,y) (only
   mode 0/1), +2 clut = row<<6 | 0x30 (row = t+0xC1 for t < 7, else the row
   just used; for 0xD the quirk counter+1), +4 x, +6 y, +8 mode. [RAM: all 15 DOCK slots]

Type 0x22: animated texture frames, kept in RAM [CODE 800441d8] [RAM]

| Offset | Type | Meaning |
|---|---|---|
| 0 | u32 | 0x22 |
| 4 | u32 | anim slot; DAT_800bbb20[slot] = chunk+8 (DAT_800cb680[slot] = 0) |
| 8 | u16 | width in texels |
| 10 | u16 | height |
| 12 | u8[] | frames of (w/2)*h bytes each (4-bit) [INFER: size; every frame found in VRAM is 4-bit sized and the data length divides exactly] |

Some other routine copies one frame at a time into the pages (DOCK: slots
0xA, 0xC to page 5 at (0x330,0)/(0x320,0); 0x19, 0x27 at (0x270,0x80)/(0x270,0xC0);
0x1E at (0x250,0)). [RAM: frames found there in VRAM]

Kept chunks are laid back to back in the loader's work buffer
(DAT_800b0c14 at entry), each padded to 4 bytes. [RAM: DOCK 0x80120aac + offsets]

Other types (no DOCK instance; [CODE 800441d8] only, unverified at runtime):

| Type | Meaning |
|---|---|
| 0x20 | 15-bit image: +4 u32 w, +8 u32 h, +0xC u32 (to DAT_800abbb0), +0x10 texels. Upload at cursor A (cursor B if DAT_800b0cf8 & 0x400); cursor x += w. FRONTEND/MEM_0x: 512x256 at (0x40,0). |
| 0x21 | sprite descriptor kept in RAM, adjusted in place (+0x10 /= 4; +0x16 = +0x14 + slot.y; +0x14 = +0x10 + slot.x; +6 = +0x1A * (+0x18/2); +0x18 /= 4; slot = i16 at +4). |
| 0x24 | 8/4-bit image with palette: +4 u32, +8 w, +10 h, +12 pal len, +14 mode, +0x10 palette, CLUT at counter row, slot 0xC, cursor A, cursor x += w_hw. |
| 0x25 | 15-bit image like 0x20 at cursor B (starts (0x280,0x100)); GFX/*.RAW are one 320x256 0x25 each. |
| 0x26 | kept; DAT_800cb680[slot at +4] = chunk+8. |
| 0x28..0x2F | kept (pointer table 0x800d4b00+4t); the first one (if no 0x20/0x24 yet) is also uploaded as a 15-bit image at cursor B; cursor B x += 0x140; counter -= 1. |
| 0x30..0x57 | kept (pointer table 0x800d3e10+4t). |
| 0x58..0x5F | kept (0x800ddc10+4t); the first one also uploaded like a page (header as type < 0x20) at cursor B with a CLUT at the counter row, slot 5. |
| 0x101/0x102/0x103 | kept; pointers DAT_800d8e90[i]/DAT_800d8d08[i] (i = i16 at +4) / DAT_800b0b30. |

Course table at 0x800acb34, 12 bytes per course: +0..2 fog R,G,B, +6 u16
flags (bit 0x10 = desaturate). The exe holds the static values; at run time
bit 0x0800 of the flags is set as well. DOCK (3): colour (0x00,0x13,0x5F),
flags 0x801F in the exe, 0x881F in RAM. [RAM]

## Rust API (`mr_assets::raw`)

- `parse(file) -> Result<RawFile, RawError>`: chunks (`Chunk { file_offset, header: RncHeader, kind, data }`), terminator offset, trailing byte count.
- `unpack_rnc2(file, at) -> Result<(Vec<u8>, RncHeader), RawError>`, `rnc_crc16`.
- `LoadEnv { course, fog_rgb, course_flags, image_to_second_cursor, stp }`, `LoadEnv::new`, `LoadEnv::from_exe(exe, course)`; `course_raw_path(exe, course)`; consts `DOCK_COURSE`, `FRONTEND_COURSE`, `COURSE_TABLE_ADDR`, `COURSE_NAME_TABLE_ADDR`.
- `decode(file, &env)` / `decode_chunks(&chunks, &env) -> Result<Decoded, RawError>`:
  `Decoded { uploads: Vec<Upload>, slots: [Option<TexSlot>; 32], anims: Vec<AnimStrip>, retained: Vec<Retained> }`.
  `Upload { chunk_index, kind, what: UploadKind::{Page, Clut{variant}, Image}, x, y, w, h, data: Vec<u16> }` in LoadImage order.
- `Decoded::apply(&mut Vram)` / `apply(&uploads, &mut Vram)`.
- Helpers: `clut_blend`, `clut_desaturate`, `reduce_in_place`, `get_tpage`.

## Verification (2026-10-12, recomp debug build in a 1-player DOCK race)

`cd port && cargo build && cd .. && port/target/debug/mr-raw.exe` (defaults:
COURSE_A/DOCK.RAW, course 3, oracle on 127.0.0.1:4370; read-only commands
`frame`, `gpu_state`, `vram_peek` 128x128 tiles, `read_ram`). Writes
`analysis/raw/dock_vram.png` and `analysis/raw/dock_vram_oracle.png`.

- Framebuffers from gpu_state: display (512,256) 512x238, draw area
  (0,256)-(511,495); no upload overlaps them.
- 48 uploads (15 pages, 33 CLUT rows). 46 match VRAM 100.000%. Page 0x15
  (0x240,0) 95.868% and page 5 (0x300,0, 64x192) 91.406%; every differing
  pixel lies in a rectangle that holds a 0x22 animation frame at the time of
  reading (5 rectangles, 3520 px). Total 252475/254208 px = 99.318%;
  excluding the animated rectangles 250688/250688 = 100.000%.
- RAM DAT_800b0fac = 3. Slot table 0x800dff00: 15/15 slots equal (tpage,
  clut, x, y, mode).
- 0x22 chunks: DAT_800bbb20[0xA,0xC,0x19,0x1E,0x27] = 0x80120ab4, 0x80128ac0,
  0x8012cacc, 0x80133ad8, 0x801342e4; RAM there equals chunk bytes 8.. byte for
  byte (32772, 16388, 28676, 2052, 15236 bytes), at buffer offsets 0x0,
  0x800C, 0xC018, 0x13024, 0x13830 as the decoder predicts.
- `cargo test -p mr-assets --test raw`: 5 tests (CRC check value, tpage, DOCK
  chunk list/slots/retained/VRAM hash 0xbbd406fb40dcb272, all 70 files
  decode, course 0x1E skips the reduction).

## Open unknowns

- Paths not exercised by DOCK (fog-blend CLUTs, types 0xD/0xE/0x20/0x21/0x24/
  0x25/0x26/0x28..0x5F/0x101..0x103, the 8-bit reduction, streaming mode)
  follow the Ghidra decomp only; verify on a course with flag 0x10 clear and
  on the frontend/loading screens when the oracle is there.
- Which routine copies the 0x22 frames to VRAM, and its timing/placement.
- What the 0x22 anim slot numbers and page types refer to in TER/DAT/MDL
  (presumably texture ids).
- Meaning of the reduced (half/quarter) texel blocks of types 0..3 (probably
  low-detail copies used by distant geometry) [INFER].

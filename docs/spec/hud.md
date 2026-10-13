# Race HUD (minimal: countdown, position, lap, timer)

Rust: `port/crates/mr-assets/src/hud.rs` (tables from the exe, layout, software
reference renderer), `port/crates/mr-bevy/src/hud.rs` (overlay), CLI
`mr-hud` (`src/bin/mr-hud.rs`), tests `port/crates/mr-assets/tests/hud.rs`,
checks `tools/oracle/hud_check.py` (GP0 packets) and `tools/oracle/hud_sbs.py`
(side-by-side images).

Tags: **[CODE addr]** read from the game's code, **[RAM]** matched against the
running recomp (RAM or the GP0 packets of a frame), **[INFER]** unverified.

## Drawing primitive: FUN_800474E8 (text with the sprite font)

`FUN_800474E8(str, x, y, align, size, colour, abr)` [CODE 800474e8]:

- **Cells**: size 0 = 16 px high, glyph spacing 3; size 1 = 24 px, spacing 5;
  size 2 = 12 px, spacing 2.
- **Glyph table** 0x800ABC18, 0xC0 entries of (u, v, width, present). Index =
  char + 0x20 (16 px), char + 0x60 (24 px), char - 0x20 (12 px). Lowercase
  t/o/x/s map to the special glyphs 0x5B..0x5E with CLUT offset 4. Other
  lowercase letters are skipped. `#d` / `#a`..`#f` switch colour.
  Characters 0x80 and up are accented capitals (FUN_80047368); the extra
  accent sprite is not ported. [CODE] [RAM]
- **Alignment**:
  - Vertical: (align & 5) == 4 gives bottom (y + 1 - cell); == 1 gives top;
    anything else centres (y - cell/2).
  - Horizontal: (align & 10) == 8 gives right (x + 1 - width); == 2 gives
    left; anything else centres (x - width/2).
  - Width = sum(glyph.w + 1 - spacing) + spacing. [CODE] [RAM]
- **One SPRT per glyph**: code 0x64, |2 (semi) unless abr == 0x60, |1 (raw)
  when the colour is neutral (0x80,0x80,0x80) and abr != 0x20. Fields:
  - x, y;
  - w = glyph width;
  - h = cell height;
  - u, v from the table;
  - CLUT = slot 0xF clut (DAT_800DFFF2) + colour table 0x800ABC0C[colour].
  The pen advances by w + 1 - spacing. Spaces advance without a sprite.
  [CODE] [RAM]
- **One DR_MODE** after the sprites: texpage = slot 0xF tpage (DAT_800DFFF0)
  + abr, dither on, draw-to-display on. On DOCK this is E1000668 = page
  (0x200, 0), 4-bit, abr 3. All of these go to the same ordering-table entry
  and are prepended, so the GPU draws the last-added primitive first.
  [CODE] [RAM]
- **Slot 0xF** is the font page that every course RAW carries (RAW.md). On
  DOCK it is the page at VRAM x 0x200 with CLUT row 0xFB at x 0x300; colour
  offsets 8/9/0xB select 16-entry sub-palettes. [RAM]
- **Colour table** 0x800ABC0C = 00 01 02 03 08 09 0A 0B 0C 0D 0E 00 00 00 05 01.
  Colour 4 → 8 (white), 5 → 9 (green), 6 → 0xA, 7 → 0xB (orange). [RAM]

Textured quads of the HUD use FUN_8002FAF4(x, y, w, h, u, v, uw, vh, r, g, b,
slot, clut_off, abr) [CODE 8002faf4]: a POLY_FT4 (0x2C, |2 unless abr 0x60)
stretching (u, v, uw, vh) over (x, y, w, h), modulated by r, g, b, with
tpage = slot.tpage + abr and clut = slot.clut + clut_off. Only the minigame
uses it here.

## Elements (1 player race, DAT_800B0D46 == 1)

Screen = 512x240 with the draw offset already applied. The viewport struct
(DAT_800B0EB0) gives centre x 0x100 and top 0 in 1P. [RAM: packets]

| Element | When | Call | Reads | Tag |
|---|---|---|---|---|
| Place digit | countdown <= 0, not finished (+0x4C7 == 0) | text(`'0'+place`, 0x100, 0x20, 0, 24px, colour 7, abr) | place = kart +0x4CA (not +0x4BE) | [CODE 8004f688] [RAM] |
| Place suffix | same | text(suffix[place-1], 0x100 + dx[place-1], 0x1B, 0, 12px, colour 4, abr); suffixes at 0x800AB3F4 (stride 8: ST ND RD TH TH TH), dx at 0x800B06A4 = 17 19 19 19 19 17 | | [CODE] [RAM] |
| (abr) | | abr = 0 (semi-transparent 50/50) while lap == 0 and kart +0x4C1 != 0, else 0x60 | +0x4BF, +0x4C1 | [CODE] [RAM: semi bit in packets] |
| "LAP" | same | text(*0x800B0200, 0x30, 0xD2, 4, 12px, colour 5, 0x60) | | [CODE] [RAM] |
| "n/m" | same | text("%i/%i", 0x30, 0xD1, 1, 16px, colour 4, 0x60) with n = +0x4BF + 1, m = DAT_800B0DA6 | | [CODE] [RAM] |
| Timer | countdown == 0 | FUN_80050B2C: text("%i:%i%i.%i", 0x100, 0xD2, 0, 16px, colour 4, 0x60) of DAT_800B0A94 / 3 (hundredths; shows tenths) | DAT_800B0A94 | [CODE 80050b2c] [RAM] |
| "RACE 3 LAPS" | countdown >= DAT_800B07F8 | FUN_80050F64: text(0x800B0454 "RACE %i LAPS" or 0x800B0450, 0x100, 0xD2, 0, 24px, colour 5) | DAT_800B0DA6 | [CODE 80050f64] |
| "COPY THE BUTTONS" | 0x86 < countdown < DAT_800B07F8 | text(*0x800B057C, 0x100, 0xD2, 0, 24px, colour 7) | | [CODE] |
| "READY" | countdown <= 0x77 and countdown % 30 >= 15 | text(*0x800B0340, 0x100, 0xD2, 0, 24px, colour 6) | | [CODE] |
| Minigame icons | 0 < countdown < DAT_800B07F8 | FUN_8004E8E8: four FUN_8002FAF4 32x24 quads from slot 5 at (0xD0,0x30) uv(0,0); (0xF0,0x1B) uv(0x20,0); (0x110,0x30) uv(0,0x18); (0xF0,0x45) uv(0x20,0x18). Brightness 0x40, or 0x80 + rand/8 for the button +0x4DA | +0x4DA, random byte stream | [CODE 8004e8e8] |
| Minigame count | same | text("%i" of +0x446, 0x100, 0x3C, 0, 16px, colour 4, 0x60) | +0x446 | [CODE] |

Countdown DAT_800B0C70 counts down by dt per frame (FUN_8005C2EC). After it
reaches 0, DAT_800B0A94 += 5*dt per frame. DAT_800B07F8 was 300 in the
oracle race. [CODE 8005c2ec] [RAM]

The kart's own clock +0x358 runs 10 (one tick at dt 2) ahead of
DAT_800B0A94. Mode of 60 paired reads: 39 × 10, 12 × 20, 9 × 0, all
non-atomic. `hud::race_clock_from_kart` subtracts 10 for the replay, which
only has the kart record. [RAM]

**Not ported** (all drawn by the same frame, outside this minimal HUD):
- item slot outline and meter (FUN_8004CBC8, FUN_80049440);
- minimap (96x64 SPRT, clut 0x31BA, plus kart PolyF3 markers);
- lap-split popup (FUN_80049FD8, DAT_800B07E2 & 8);
- per-lap effects (FUN_8004A310);
- finish/results (FUN_8004CEA0, "WINNER"/"LOSER");
- minigame sparkles (FUN_8002FD04);
- split-screen layouts and clip windows.

## Rust API

**mr-assets `hud`**
- `HudTables::from_exe(exe)`: glyphs, colours, suffixes, strings.
- `text(bytes, x, y, align, Size, colour, abr) -> Vec<Sprite>`.
- `layout(&HudState) -> Vec<Sprite>`, in game add order.
- `minigame(&Minigame)`.
- `HudState { countdown, countdown_start, mode, lap, laps, race_time, position, position_flag, finished, minigame }`, built with `HudState::with_kart(record)`.
- `Sprite { x, y, w, h, u, v, uw, vh, slot, clut_off, rgb, raw, semi, abr }`, resolved with `Sprite::attrs(&slots) -> (tpage, clut)`.
- `slot_attrs(&raw slots)`, `timer_text`, `race_clock_from_kart`.
- `render(vram, slots, sprites, canvas, w, h)`: software reference renderer.

**mr-bevy `hud`**
- `HudPlugin`: an orthographic Camera3d (order 10, no clear, layer 3, fixed 512x240 stretched to the window).
- Unlit quads: alpha-mask for opaque, blend 0.5 for abr 0, add for abr 1.
- Page textures through `VramTextures`, nearest sampling.
- Sprites are rebuilt only when the layout changes.
- Inputs:
  - `HudInput { state, manual }`: set `manual` to drive it directly, e.g. from the race flow.
  - `HudGlobals { countdown, countdown_start, laps, mode, race_time: Option, flicker }`.
  - By default the state comes from `Player.kart` (replay sidecar or mr-sim record) plus `HudGlobals`.

## Verification (2026-10-12)

1. **GP0 packets.** `python tools/oracle/hud_check.py` reads the HUD state
   with read_ram and the newest frame's GP0 ring (gpu_frame_dump), then
   matches every font/button primitive.
   - Oracle frame 45910 (lap 0 of 3, 6th, +0x4C1 = 1, clock 45300): layout
     15 primitives, game 19 font-page primitives, **15/15 matched exactly**
     on x, y, w, h, u, v, texpage, CLUT, raw and semi.
   - The 4 extra game primitives are not in this HUD: three 256x5 strips
     with CLUT 0, and the 96x64 minimap. The same values are locked in the
     `race_hud_matches_game_frame` test.
2. **Side-by-side images.** Oracle VRAM capture
   (`tools/oracle/shot_at.py tools/oracle/routes/lap1.json VB OUT.png`)
   against `mr-bevy --replay analysis/traces/lap1.oracle.trace.jsonl --shot OUT.png --shot-at T`,
   where T = 1 + the trace row whose `vblank` column is about VB - 3. The
   frame shown at VBlank VB was built 2..4 VBlanks earlier, with ±1 tick of
   capture jitter, and the dt=3 ticks after VB ~1374 make VB/2 drift.
   Composed with `tools/oracle/hud_sbs.py` into `analysis/hud/sbs_*.png`, all
   in `analysis/hud/sbs_all.png`:

   | VB | Tick | Oracle and Bevy both show |
   |---|---|---|
   | 60 | 29 | semi-transparent "6TH", "LAP 1/3", "0:05.6" |
   | 400 | 200 | "6TH", "LAP 1/3", "0:11.3" |
   | 2330 | 1160 | "2ND", "LAP 2/3", "0:43.5" |
   | 2420 | 1204 | "4TH", "LAP 2/3", "0:45.0" |

   The oracle frames also show the minimap, the item slot, the "LAP 2"
   banner and the stars, none of which are ported here.
   - The recomp's widescreen mod squashes HUD sprites horizontally by 3/4
     around per-element anchors on the presented and captured frame:
     - LAP, left: x' = x * 0.75;
     - timer and place, centre: x' = 256 + (x - 256) * 0.75.
   - The GP0 packets are the vanilla coordinates. The Bevy HUD reproduces
     those, the 4:3 game, so widths differ by that factor in the
     side-by-sides. Rows, glyphs and colours agree.
3. `cargo test -p mr-assets --test hud`: 4 tests:
   - timer format;
   - exe tables;
   - packet-matched layout;
   - countdown phases.

Countdown elements are [CODE] only. The grid savestate used by the oracle
routes starts at race clock 1430, after the countdown, so there is no oracle
frame of it yet.

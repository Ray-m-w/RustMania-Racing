# Race HUD (countdown, position, lap, timer, items, minimap)

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

## After the finish: results panel (FUN_8004CEA0) and PRESS START

When the player's kart finishes (+0x4C7 != 0), FUN_8004F688 case 1 calls
FUN_8004CEA0. The game then switches to mode 10, and case 10 calls it too
(when DAT_800B07E6 == 0, DAT_800B07E2 & 0x41 == 0, DAT_800B0B9A not 2 or 3,
DAT_800B0A64 == 0). The timer and the race HUD stop. There is no separate
finish banner: the 3D chequered flag is scenery, and the panel shows from
the first frames after the line (oracle VB 7060, finish ~7043).
[CODE 8004f688, 8004cea0] [RAM]

Centre (cx, cy) = viewport +8/+10 = (0x100, 0x78). W = width of the
previous string (DAT_800B0B22, set by FUN_800474E8).

| Line | Draw | Tag |
|---|---|---|
| Place | text('0'+(+0x4CA), cx, cy-0x18, 0, 24px, 7) + suffix at (cx+dx, cy-0x1D, 0, 12px, 4) | [CODE] [RAM] |
| Best lap | +0x384 as "%i:%i%i.%i%i" (hundredths) at (cx, cy, 0, 16px, colour 4); colour 7 while it equals the course record (0x800E0190[e38*8+e32*2]), DAT_800B07E2 & 2 == 0 and (+0x38C & 0x1F) < 0x10 (flash); then "BEST LAP" at (cx - W/2 - 8, cy, align 8 right, 12px, 5). If +0x384 == 0: "NO LAPS COMPLETED" (cx, cy, 0, 16px, 4) | [CODE] [RAM] |
| Total | if +0x4BF == laps: sum of +0x35C[0..laps) at (cx, cy+0x11, 0, 16px, 4); "TOTAL TIME" at (cx - W/2 - 8, ..., 8, 12px, 5); if +0x4BE >= 2: "+%i:%i%i.%i%i" of (total - the leader's sum; leader = kart DAT_800D8F94) at (cx + W/2 + 8, ..., align 2 left, 12px, 7). Else "DID NOT FINISH" (cx, cy+0x11, 0, 16px, 4) | [CODE] [RAM] |
| Hits | "+%i -%i" of +0x492 / +0x494 at (cx, cy+0x22, 0, 16px, 4); "HITS" at (cx - W/2 - 8, ..., 8, 12px, 5). Battle modes (07E2 & 8) show DAMAGE / OUT instead | [CODE] [RAM] |
| PRESS START | FUN_80051698 / FUN_80050DB8: (0x100, 0xD2, 0, 16px, 4) while DAT_800B0D90 % 60 < 45 and (DAT_800B0F88 or DAT_800B0EBE) | [CODE] [RAM] |

Not ported: "YOU BEAT x" / "x BEAT YOU" (07E2 & 0x10), "NEW COURSE"
unlocks, and the two FUN_8002FD04 icons (controller-dependent).

**Not ported**: the lap-split popup (FUN_80049FD8, DAT_800B07E2 & 8, not
in a 1-player race), "WINNER"/"LOSER", the minigame sparkles
(FUN_8002FD04 in FUN_8004E8E8), split-screen layouts. The item HUD
(Session 4) is at the end of this file.

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

4. **Results** (`hud_check.py --route tools/oracle/routes/race3.json --vb N`).
   This replays the 3-lap race under the oracle lock, captures the frame,
   reads the frame counter, then reads the state.
   - At VB 7300: **59/59 primitives matched**. That covers the place, the
     best lap in the record colour, the total, the "+0:15.51" gap, the hits
     and PRESS START. Locked in the `results_panel_matches_game_frame` test.
   - At VB 8000: the 49 panel primitives matched. The best-lap colour and
     the prompt are blinking and lagged the RAM reads.
   - A probe over VB 7950..8096 (frame dumps every ~10 VB with 0x800B0D90
     and +0x38C) shows PRESS START hidden exactly when (counter - ~10) % 60
     >= 45. That fits the code given the ~10-VBlank capture lag. The record
     flash fits (c & 0x1F) < 0x10 within the same lag, with one outlier
     sample.
   - Side-by-side: `analysis/hud/sbs_results.png` compares oracle VB 7300
     and 8000 (3rd, 0:37.32, 2:02.15, +0:15.51, +0 -6) with
     `mr-bevy --autopilot` ticks 4300 and 4500. The autopilot race is a
     different race: 2nd, 0:38.67, 2:00.47, +0:01.24, +0 -0. Same layout,
     fonts, colours and blinking prompt.
   - In the Bevy app, `results_from_session` sets
     `HudGlobals.results` from mr-sim. Two inputs are inferred:
     - `best_lap_record` = the player's best lap is the fastest of the
       race, because the course-record table is save data;
     - `press_start` = true from the finish.

Countdown elements are [CODE] only. The grid savestate used by the oracle
routes starts at race clock 1430, after the countdown, so there is no oracle
frame of it yet.


## Item HUD (Session 4, track B)

Rust: `port/crates/mr-assets/src/hud_items.rs` (`race_hud` = the whole
1-player HUD in add order), `port/crates/mr-bevy/src/hud.rs` (drawing),
`mr-hudx` (layout from an fx capture), `tools/oracle/hudx_check.py`
(GP0 check), test `port/crates/mr-assets/tests/hud_items.rs`.

### Primitives

- **FUN_8002FD04**(x, y, hw, hh, flags, angle, u, v, uw, vh, r, g, b,
  slot, clut_off, abr): a POLY_FT4 (0x2C, |2 unless abr 0x60). Corner x
  offsets are scaled by 512/320 (`(dx << 9) / 0x140`). Angle 0: an
  axis-aligned quad, culled when x0 > 0x1FF, x1 < 0, y0 > 0xEF or y1 < 0.
  Otherwise the corners rotate with the 1.14 sine table (0x800A88C0,
  cos = +1024 entries) and the quad is culled only when all four corners
  are off one side. UVs (u, v)..(u + uw, v + vh), bytes; a negative uw
  mirrors. tpage = slot tpage + abr; clut = slot clut + clut_off; slot
  table 0x800DFF00 stride 0x10. [CODE 8002fd04] [RAM]
- **FUN_8002F8F4**(x, y, u, v, w, h, r, g, b, slot, clut_off, abr): a SPRT
  (0x64, modulated, |2 unless abr 0x60), then a DR_MODE with the page.
  [CODE 8002f8f4] [RAM]
- **FUN_800474E8 clip windows** (added): the string is skipped when its row
  misses y 0..0xEF (DAT_800B0688 / 068A) or its span misses x (x >= 0x201
  or x + width <= -1); glyphs wholly outside x 0..0x1FF (DAT_800B0E9E /
  0EA4) are skipped; rows cut by the y window are trimmed. Seen with the
  sliding "SPECIAL MOVE". [CODE 800474e8] [RAM]

### Elements (1 player, FUN_8004F688 case 1, in add order)

| Element | Call | Reads | Tag |
|---|---|---|---|
| Status icon | FUN_8002F8F4(0xED, 0x30, uv (0x80, 0x40), 0x25 x 0x18, slot 0x15, clut 6, opaque) while +0x4F9 & 0x7F (drawn even before the minigame; y 0xA8 while counting down). Seen as the bomb above the place. | +0x4F9 | [CODE] [RAM] |
| "SPECIAL MOVE" | DAT_800B07F4 (i8) != -1: the starter (+0x4D7 == DAT_800B0E95) blinks it at (0x100, 0x78), 24 px, colour 5, while DAT_800B07F6 < 0x5A and (07F6 % 30) < 15; another kart with +0x4F1 != -1 slides it across: x = (+0x468 << 10) / 300 - 0x100, colour 7, blinking on +0x468 % 30 < 15, with two 0x18 character icons at x -+ 0xA0 (table 0x800AC218 + c * 0x14: +0x12 u, +0x13 v, +0x14 slot, +0x15 on; the right one mirrored) | 07F4, 07F6, 0E95, +0x4F1, +0x468 | [CODE 80049440] [RAM] |
| Direction arrow | FUN_80032A40(0x3C, +0x48E, player) when +0x4F9 & 0x7F == 0: "WRONG WAY" (0x100, 0x3C, 16 px, colour 7) when +0x503 > 0xE; then an 8-point outline (exe 0x80020F1C: x 0,256,128,128,-128,-128,-256,0; z 256,64,64,-256,-256,64,64,0) rotated by the angle, through the camera matrix (rows 0/1/2, x and z columns), projected x = (X << 16) / ((Z + 0x300) * 0x140), y = ((Y + 0x180) << 7) / (Z + 0x300), minus point 7's y, scaled * 12 / 32; 7 white LineF2 around (0x100, 0x3C). DAT_800B0E38 >= 7 uses a flat variant (k 15). | +0x48E, +0x503, camera 0x800C4F40 | [CODE 80032a40] [RAM] |
| Banner / popup | +0x4FC != -1 (0xFF): text at (0x100, 0xB4), 24 px. Ids 10, 11, 13, 15: sprintf("%i " / "+%i ", +0x444) + the message (15: "HIT" / "HITS" by abs(+0x444) >= 2), colour 5 if +0x444 > 0 else 7 ("-1 HIT"); 12: a rotating 0x14 quad (uv 0x90, slot 9); 14: "1 NEW VEHICLE" or "%i NEW VEHICLES"; others the string of 0x800AB424 (0 "LAP 2" ... 7 "LAP 9", 8 "FINAL LAP", 9 "WRONG WAY", 16 "+1 KNOCKOUT", ...) in colour 5 | +0x4FC, +0x444 | [CODE 80049440] [RAM] |
| Place | as before | | |
| Weapon slots | FUN_8004CBC8(0x2D): for slot 0 / 1 at (0x30, 0x1F / 0x3B): the icon FUN_8004C184, then, when the charge +0x4D4+slot < 8 (signed), a ring quad (uv (0xC0, 0x80) 0x37, slot 0x15, clut 0xB, half size pulse / 2; opaque when the slot is empty, -1). Icon: nothing for charge -1; charge 0x40 or abs < 4: the held weapon +0x4D0+2*slot opaque; else the next one +0x4D1+2*slot then the held one, semi-transparent (type 3 stays opaque). Types: 0 spinning (uv 0x40,0x56, 0x29, slot 0x15 clut 5, half = pulse*42/80); 1 spinning (0xC0,0xC0, 0x13x0x37, clut 0xF, pulse*20/96 x pulse*56/96); 2 the 16-frame animation 0x800A8848 (u, v, uw shorts; vh 0x1F; slot 0x14 clut 7; wobble angle sin/128; half pulse/2); 3 (0x40,0x82, 0x27x0x3D, slot 0x14 clut 9, pulse*40/128 x pulse*62/128). The ring is an animated texture (strip 0x19). | +0x4D0..+0x4D5, pulse | [CODE 8004cbc8, 8004c184] [RAM] |
| LAP n/m | as before | | |
| Turbo meter | FUN_8004A310: two additive sparkle quads at x = 0x1D0 + +0x47E, y 0x22 (uv 0xA0,0xC0, 0x1F, slot 0x15 clut 0xE; sizes pulse * DAT_800B07BE / DAT_800B07C6 * 32 >> 14; colours 0x800B07B8.., spinning both ways); the ring quad at (x, 0x3E); the thermometer at x2 = 0x230 - +0x47E: SPRT (x2-0xC, 0x1D) uv (0, 0xC0) 0x18x0x23 additive and SPRT (x2-0x11, 0x1D) uv (0x18, 0xC0) 0x22x0x25 (slot 0x14 clut 0xC); the fill SPRT (x2-0xF, 0x30-(f-0x11)) uv (0x60, 0x22-f) 0x1E x f (slot 0x15 clut 3), f = +0x48A * 0x22 / 0x1A4. +0x47E is 96 while filling (sparkles off-screen) and 0 when full (thermometer off-screen, sparkles in). | +0x47E, +0x48A, pulse | [CODE 8004a310] [RAM] |
| Minimap | FUN_80048A0C (FUN_80051698, after every player's HUD, countdown 0): DOCK uses the per-course table 0x800AC6DC + course * 0x14 (course flags 0x800ACB3A & 2): u, v, w, h = 128, 128, 96, 64 and bounds x0, z0, x1, z1 (world >> 5) = -20250, 44150, 47400, -28900. Per kart: mx = 0x1E0 - w + w * (x >> 5 - x0) / abs(x1 - x0), my = 0xE1 - h - h * (z >> 5 - z0) / abs(z1 - z0); a PolyF3 arrow FUN_800307D4 (0, -b), (a, a), (-a, a) rotated by +0xE (x / 0x2800, y >> 14), colour 0x800AC227 + c * 0x14 (character +0x4B2); human players (a, b) = (5, 10), hidden while clock % 150 > 74; others (4, 8). +0x496 & 0x60: the character icon instead. Then the map SPRT (0x180, 0xA1) 96x64 uv (128, 128) slot 5, clut + (v >> 6) * 4 + (u >> 6) (drawn under the arrows). | all karts | [CODE 80048a0c, 800307d4] [RAM] |
| Timer | as before | | |

**HUD pulse** DAT_800B0E10 / 0E12 (FUN_8005C2EC, after the frame counter
step): targets (0x10, 0x1C) while frame % 120 < 60, else swapped; each
moves by dt toward its target. Slot k of player p uses pulse[(p & 1) ^ k];
the meter uses pulse[p] for the sparkles and pulse[p ^ 1] for the ring.
[CODE 8005c2ec] [RAM]

### Verification (2026-10-13)

`python tools/oracle/hudx_check.py analysis/fx/race3 --lag 0` (fx capture
of race3 with items on: RAM every VBlank + GP0 dumps of the two frames
around every 50th VBlank):
- **9,714 / 9,714 port primitives matched** exactly (vertices, UVs,
  texpage, CLUT, raw/semi, colour) over **345 frames** (VB 50..8650):
  weapon slots (type-2 icons, the stacked next weapon semi-transparent),
  the turbo meter filling and full (sparkles), "LAP 2", "FINAL LAP",
  "-1 HIT", the sliding "SPECIAL MOVE" with its icons, the direction arrow
  (966 lines), the minimap (782 kart arrows + the map), place, LAP, timer.
- Race frames: the only unmatched game primitives that look like HUD are
  three 256x5 SPRTs at (256, 193) (texpage 0x12C, 15-bit), every frame, not
  drawn by FUN_8004F688 / FUN_80048A0C (unknown, not ported). Frames after
  the finish show the results panel, which mr-hudx does not lay out
  (checked in Session 3).
- State timing: the RAM row of the frame's VBlank, except the frame
  counter, race clock and pulse, which FUN_8005C2EC advances after the
  draw (taken from the row before the counter's last step).
- Test `race_hud_with_items_matches_game_frame` locks the 40 primitives of
  VB 3800 ("-1 HIT", both slots, meter 252/420, arrow, six karts).

In the Bevy window the item HUD comes from the live session
(`items_from_player`): the record, every kart's record, the camera matrix,
the frame counter and the pulse stepped per sim frame; replays use the
sidecar record alone (player marker only, pulse phase from the tick).
Side-by-side: `analysis/fx/shots/sbs_items_2330.png` (oracle VB 2330 vs a
replay of lap1 at tick 1161), `analysis/fx/shots/sbs_fx.png` (effects).

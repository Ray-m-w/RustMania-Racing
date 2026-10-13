# Items, weapons and specials (port/crates/mr-sim/src/items/)

Session 4, track A. Evidence tags as in kart-state.md: [CODE addr] the
game's code (MIPS disassembly of the oracle's RAM, `tools/oracle/mipsdis.py`,
as the spec; Ghidra as the reading aid), [RAM] matched the oracle. Unless
marked, everything here is [CODE] and verified (Verification, end).

## Model in one paragraph
Every kart carries two weapon slots and a turbo meter. Item boxes on the
track fill an empty slot with a random weapon that charges for 64 VBlanks
(its type keeps changing while it charges); L1 / L2 fire the charged slot's
projectile. Turbo tokens add 12 to the meter (0x1A4 = full); R1 burns meter
as turbo; with a full meter, R1 starts the special move (the kart vanishes
and flies along the track path until the meter is spent) and R2 starts the
character's special, an effect on every other kart for 300 VBlanks. The AI
presses the same buttons (ai.rs). Projectiles hit karts (spin, velocity
halved, "-1 HIT" / "+1 HIT"), bounce off walls and floors or explode, home
along the path or hover by type. Pickups shrink when taken and come back.

## Where it runs (FUN_8007916C, the race frame)
1. Kart loop: inside FUN_80064B5C (update.rs): slot recharge (0x80065954),
   shoulder buttons (0x80065A50: R1 turbo latch / special move start, L1 /
   L2 `fire_button` FUN_800645F4 -> FUN_80068C6C, R2 FUN_8006B828), the
   special-move branch (0x800653D0 + FUN_800913F4), turbo (0x80066CD4),
   status-bit branches, the trigger warp FUN_80061FC0 in the contact phase.
2. After the kart loop and the render pass (`items::after_kart_loop`):
   FUN_8006C388 effects, FUN_8007F03C kart particles (kart_fx.rs, below),
   FUN_8007A9DC blink timers, FUN_8007BBF0 pickups, FUN_80069648
   projectiles.
3. After the camera: humans' splash draw, FUN_8008727C / FUN_800327D0
   (track B's particle and spark lists).

## State (RaceGlobals::items: ItemState; RAM layout read/written by
`ItemState::read` / `write`)
| addr | what |
|---|---|
| 0x800E1B70 + 0x20 i | pickup object `Pickup`: +0 x, +8 z (TER units), +4 y, +0xC rest y (world), +0x10 vy, +0x14 state (0 none, 1 present, 2 gone, 3 shrinking, 4 growing), +0x16 scale 0..0x80, +0x18 scale speed, +0x1A respawn timer, +0x1E particle burst, +0x1F token colour 0..3 |
| slots 0..0x2F / 0x30..0x3F / 0x40.. | item boxes (DOCK 44) / challenge objects (hidden in a race) / turbo tokens (DOCK 92) |
| 0x800B0B60 / 0x800B0A52 | box / token counts; 0x800B0EBC/0B98/0A58 challenge counts |
| 0x800DDC50 + 0x48 i (4) | projectile `Projectile`: +0 pos, +0xC vel, +0x18 ground y, +0x1C path target x/z, +0x24 chase x/z, +0x2C owner, +0x2E live, +0x2F/+0x30 floor/ceiling latch, +0x32 path dir, +0x33 grounded, +0x34 flags, +0x36 age, +0x38 life, +0x3A spin, +0x3C spin rate, +0x3E/+0x40 path seg/pt, +0x42 sound timer, +0x44 face ptr |
| 0x800B07E0 | next projectile slot (round robin) |
| 0x800B07F4 / 07F6 / 0E95 | character special in progress (type, -1 none) / its clock / owner |
| 0x800B07B0..B3 | two blink toggles + timers (FUN_8007A9DC) |
| 0x800B0B90 | four token spin angles (FUN_8003C5FC: camera roll + wobble) |
| 0x800E02C0 | save data: "special hit" collection bits, 2 words per character |

Kart record fields (KartState): +0x4D0/+0x4D1 L1 current/next weapon type,
+0x4D2/+0x4D3 L2, +0x4D4/+0x4D5 charge (0x40 ready, 0xFF empty), +0x48A
turbo meter (`start_timer`), +0x4C4 boosting, +0x4D6 special move state,
+0x3E8.. its path target (shared with the AI's steering target), +0x404..
its chase point, +0x49C/+0x49E its path position, +0x4F1 effect (-1 none),
+0x354 status bits, +0x468 effect clock, +0x300..+0x318 effect parameters,
+0x4F4 last token colour, +0x492 / +0x494 hits dealt / taken (projectiles,
specials and landing on another kart), +0x4FC / +0x444 HUD message and
value ("-1 HIT" = 0xF / -1), +0x4E6 / +0x4B5 / +0x4EF / +0x4B3 trigger warp.

## Pickups (pickups.rs)
- FUN_8007B448 (course load): boxes from DAT chunk 0x13, challenge objects
  0x0C, tokens 0x12 (colour rand() >> 13); the race loader FUN_8008C4AC hides
  the challenge objects (state 2). The session draws the colours from the
  rand() seeds before its race-load seed (`bios_rand_rewind`) [INFER: the
  oracle's course load happened before its start_probe].
- FUN_8007BBF0, per kart (not in a special move, warp, carried object or
  effect): the box touched (FUN_80081170 mode 0x20: every axis within
  spark_offset + 0x80, TER units, driver position +0x330/+0x338/+0x340) and
  present: fills slot L1, else L2 (type = 0x800AC178[rand() >> 10], charge
  0), state 3, burst 5 if a human camera is within 10000; both slots full:
  the box bounces (vy -0x180). Token (radius + 0x60) with the meter not full:
  +12 (cap 0x1A4), colour -> +0x4F4, burst 7.
- Per object: bounce on its rest height (gravity 0x60), state machine
  (3 shrink: speed -1/VBlank; 2 gone: 30 VBlanks, a token re-rolls its
  colour from the random pointer; 4 grow; 1 present, scale 0x80), the burst
  particles (8 / 6 random bytes each).

## Weapons (weapons.rs)
- Recharge (update.rs 0x80065954): +dt per tick; every 8 the current type
  takes the next and a new next is drawn (0x800AC178[rng >> 3], not equal);
  at 0x40 ready.
- FUN_800645F4: engine scale >= 0x20; charged: FUN_80068C6C(kart, flags) with
  type 0..3 -> flags 0x1D, 0x280, 0x588, 0x78; slot emptied.
- FUN_80068C6C: the round-robin slot (a live one explodes first), from the
  kart's x, look y, z; velocity = vehicle matrix * (0, 0 / -0x40, 0x80 /
  0x70) >> 7 << 12 / engine scale (flag 0x20: (0, 0, -0x20) plus the kart's
  velocity << 11 / scale); the kart recoils by vel / 16 (/ 4); life 300
  (0x20), 180 (1), 240 (0x100), 120; path target 2 points ahead (backward
  when the kart's wrong-way monitor |+0x48C| >= 0x400); spin from the random
  pointer.
- FUN_80069648, per live projectile: age; random hop (0x40) and whine
  (0x400); dt sub-steps: path chase (0x100: a chase point runs at 0xA0 along
  the TER path, the projectile accelerates toward it / 64), slide on the
  floor normal (0x20), soft clamps (0x1000 / 0x1800), gravity (0x10), hover
  0x2000 above the ground (0x80); wall test FUN_8003DA18 (probe +-0x1000, or
  -0x2000 / +0x2000 for 0x600, -0x2000 / 0 for 0x40; bounce 0x10 / 8):
  explode, or bounce with sparks (8); ground FUN_8003DDB0: ceiling (FACE_ALT)
  and floor contact, reflect with sparks (4) or stop; then the first kart
  (driver position within 0x4000 on every axis, not the owner, not in a
  special move / warp / carried object) is hit: FUN_8007B26C spin (toward
  the track for a human), velocity / 2 + projectile velocity / 2, vy -
  0x4CC, hit counters and the HUD messages (race flags 0 / 2 / 0x10), voice
  request bits, rumble 200; trail particles.

## Specials (specials.rs)
- Special move (0x80065B5C): R1 with a full meter on the path in a race:
  +0x4D6 = 1 (2 with race flag 2: backward), target 2 path points ahead,
  FUN_8009192C re-roll; L1 / L2 / R2 not read that tick. Each tick
  (0x800653D0): scale from the meter (0x1000 - 0x80 m below 0x21, 0, then
  (m - 0x184) * 0x80), spin stopped, meter -= dt (state 3 at 0), then
  FUN_800913F4: the chase point runs at 0xA0 / VBlank toward the target,
  which steps along the path within 0xA0; the kart eases 1/16 toward it;
  state 3 converges 1/4 on the last point and ends within 0x14; velocity
  and heading follow. The physics block (limits, steering, throttle,
  integration, vertical) is skipped; ground contact only (FUN_8003E91C's
  special branch: height kept, no sink, no bounce).
  Two locals the skipped block would set are read anyway (stack 0x58 /
  0x48: steer rate, sideways speed): `RaceGlobals::stack` keeps the last
  ordinary kart update's (exact for AI karts; for the player, first in
  the loop, other code leaves other values: ledger).
- Character special FUN_8006B828 (R2 with a full meter, no special running):
  type = character table +0x11 (0x800AC231 + 0x14 c: 0..24 = the character
  index); every other kart not in a special move / warp / carried object /
  stunned / scaled gets effect = type, its status bits and parameters
  (24 types, 0x8006BBA8.. switch) and a hit message; collection bits in the
  save data (vehicle hit, 0x800AC6C0 filled by the model loader).
- FUN_8006C388 while a special runs: clocks to 300 (a human victim holding
  R2 with meter shakes it off twice as fast, burning meter), end of an
  effect (shape back, status 0, a spin from the yaw rate; a human spins
  toward the track direction), the per-effect animations (table 0x80022FF8
  by effect - 1: scale pulses, shape morphs, spin kicks, float, particles),
  pad rumble from the random pointer. Effect 5's particle loop reuses the
  kart loop's counter: the remaining karts are skipped that frame (a game
  quirk, ported).
- Status bits in the kart update: 4 / 8 no throttle / steering, 0x10 / 0x100
  steer at / around the camera, 0x40000 at +0x300, 0x40 spin, 0x20 pulled
  to +0x300, 0x400 floats (water physics at a bobbing height), 1 frozen
  (heading spins), 0x1000 slippery, 0x2000 drag, 0x8000 / 0x80 / effects
  0x15 / 0x16 sway wobble, 0x200000 no water, 0x100000 no slope push,
  0x10000000 normal from the alt surface, 0x1000000 wheels stop; render:
  0x20000 aim at +0x300, 0x400000 body tilted by +0x30C, 0x8000000 no shadow.

## Kart particles FUN_8007F03C (kart_fx.rs, 2026-10-13)
Runs after FUN_8006C388 on the live kart records (the session writes each
kart's state into its record first). The first loop (track objects,
0x800B0C58 of them) is not ported: 0 on DOCK in every oracle race. Then
for each kart within 10000 TER units of a human camera on every axis
(0x800C4F40 + 0x34, the previous frame's), in this order, each spawn with
its own random bytes from 0x800B0BCC (the AI's random source, so every
draw matters):
- ground sparks (FUN_800326B4, 3 bytes): on the ground, +0x174 & 2 clear,
  and +0x16E <= +0x348 * (0x80 - +0x180) / 0x8000, or no wheels down
  (+0x171 = 0) without status 0x200000;
- unless status 0x1000: on surface 0xB (+0x400 > 0x100, else a byte
  < 0x40) the splash (kind 0x15, 6 bytes, colour 0x800AC1D8 by byte / 0x56);
  otherwise outside a special move with +0x4E7 = -1 and wheels down: a
  random wheel's dust (kind 2, colour +0x4BB..+0x4BD, size wheel +0x36 x
  +0x442): sliding (|+0x390| < |+0x394| or +0x4C9) 4 bytes, else when
  drifting (+0x486 > 0x40) 2 bytes, and only a driven wheel (+0x3A & 1;
  otherwise the rest of the kart is skipped);
- the trail: spray (kind 0xF) while floating (+0x4C8) or when near the
  ground (+0x31C - y < 0x1000 and +0x400 > 0x200, or in a special move)
  with +0x4C5; else dust on surfaces 1 / 2 (kind 0xB) and 4 / 0xF (kind
  0x14, colour 0x800ACD15): 2 bytes, then 4..6 for the spawn;
- rain hits (weather 0x800B0B9A == 2 or 0x800B07E2 & 9; off on DOCK),
  dt tries of 1 or 7 bytes;
- running normally (status & 0x1001 clear, +0x4E7 = -1, +0x442 > 0x20):
  without light points (+0x172 = 0) exhaust puffs from wheel parts flagged
  0x40 (kind 10, MulMatrix0 of +0x1AC and the part matrix +0x20C + 0x12 i);
  with them one random light point (kind by its type +0x12E: 4, 8, 5 with
  the 0x800AC198 colours, 10);
- the turbo / special / skid puff (kind 0x11, 7 bytes);
- the pickup and item sparkles around the driver (kinds 1 and 0x11, 7
  bytes each).
The kart-kart contact sparks of FUN_80040428 (collide.rs, three streaks
midway between the two karts' +0x176 points, at ride height) are spawned
too (they used to be skipped draws).

## Trigger warp FUN_80061FC0 (progress.rs `teleport`)
DOCK's two trigger objects (0x800D1C9C, 3 points each): touched (first
point within 16 units >> 10), the kart shrinks (60 VBlanks), flies to the
second point (120; heading toward the third), grows (60), released.
Ported from the code; no oracle capture has a kart touch one.

## Verification (2026-10-13; `mr-items`, analysis/items/, local)
Captures (tools/oracle/capture_items.py: physics regions + pickups +
projectiles + records), savestate slot 2, 3 laps + finish:
- i_race3 (route race3: the player drives, items not used by the player),
  i_items1 (items1: L1/L2 when charged, R1 turbo at meter 200..300),
  i_items2 (items2: meter saved, R2 = Kermit's special at tick 2324),
  i_items3 (items3: R1 special move at tick 2324). Routes recorded
  closed-loop by tools/oracle/drive_items.py.
- Teacher-forced race frames (`mr-items tf`: Session::load_mem of the
  oracle's state, one session tick, compare): every kart's owned fields,
  the pickup and projectile tables exact on every tick of all four races
  except the restore frame (tick 0) and the finish (the harness merges the
  results-switch frame with the next); the player's special move in
  i_items3 differs only in steer lag / sway / look point (stack slots).
  Written before FUN_8007F03C was ported: its random-byte step was
  searched (0 on ~93 % of ticks).
- Free run (`mr-items free`): the player kart, pickups and projectiles from
  the port's own previous tick; AI karts, globals, the random pointer and
  the rand() seed from the oracle each tick (inputs, as in Session 3);
  nothing injected. The player is exact on every motion field to the
  finish in all four races, with 4 / 4 / 2 / 3 projectile hits on the
  player produced by the port's projectiles.
- All karts free (`mr-items freeall`): every kart, pickups and projectiles
  from the port; only the frame-start random pointer, the rand() seed and
  the searched FUN_8007F03C step from the oracle. Exact on every tick for
  all six karts over 4,815 ticks on s_1, s_11, s_15, s_20.
- Nothing from the oracle after tick 1 (`freeall --own-rng`, with
  FUN_8007F03C and the contact sparks ported, 2026-10-13): s_1, s_11,
  s_15, s_20 exact on every tick to the end (4,815 ticks) for all six
  karts, the pickups, the projectiles, the random pointer 0x800B0BCC, the
  rand() seed, the particle and spark ring indices 0x800B0C60 / 0x800B07EB
  and particle slot 0 (the item captures hold only that slot of
  0x800E0C20; its +0x32 bit 0x200 is the drawer's). i_race3, i_items1/2/3
  (the player driving) the same up to the player's finish (3511, 3674,
  3661, 3605); every kart differs from the next tick, the results-switch
  frame the capture merges (as in `tf`); i_items3's player differs from
  the special move (2325) only in look point / sway (stack slots).
  A port race from tick 1 with the capture's own seed (`race --seed 0`)
  reproduces the oracle's rows (every kart field) to the end on s_1,
  s_11, s_20.
- AI item use (`mr-items rows` / `race`, tools/oracle/item_stats.py): the
  20 independent oracle races s_1..s_20 (tools/oracle/capture_seeds.py:
  player idle, a different poked rand() seed and random pointer each) vs
  20 port races, one per start (`race --seed 100k+1`: another rand() seed
  and random pointer at tick 1, then nothing injected). Per-race totals
  over the five AI karts; tolerance = 2 * sqrt(sd_o^2/n_o + sd_p^2/n_p).

  Race window (each kart until it finishes):

  | stat | oracle mean (sd) [min..max] | port mean (sd) | diff | tol | ok |
  |---|---|---|---|---|---|
  | fires | 40.0 (3.4) [34..48] | 39.8 (3.3) | -0.20 | 2.15 | yes |
  | boxes | 38.6 (3.5) [32..46] | 38.5 (3.5) | -0.15 | 2.22 | yes |
  | tokens | 434.3 (24.0) [397..480] | 419.4 (39.9) | -14.85 | 20.83 | yes |
  | turbo ticks | 615.7 (91.7) [440..812] | 583.2 (84.8) | -32.50 | 55.87 | yes |
  | special moves | 2.3 (0.9) [1..4] | 2.4 (1.1) | +0.05 | 0.64 | yes |
  | character specials | 0 | 0 | 0 | 0 | yes |
  | hits dealt | 13.9 (3.4) [8..21] | 11.9 (3.2) | -1.95 | 2.07 | yes |
  | hits taken | 13.8 (3.3) [8..21] | 11.8 (3.1) | -2.00 | 2.03 | yes |
  | hit rate | 0.35 (0.09) | 0.30 (0.09) | -0.05 | 0.06 | yes |

  Whole run (4,815 ticks): all within tolerance too (tokens 620.0 vs
  599.1, tol 26.4; turbo 1011 vs 938, tol 91.2). Fires by lap: oracle
  0.34/0.33/0.32, port 0.32/0.34/0.34. Character specials are 0 in both:
  the idle player is last, and an AI kart uses R1 / R2 specials only when
  it is behind kart 0 (docs/spec/ai.md, AI items).
  Since the port race with the oracle's own seed is the oracle's race row
  for row, what is left between the two columns is the sampling of seeds.
  History: before FUN_8007F03C was ported (100 port races) tokens were
  416 vs 434 (tol 12.1, NO); the same 20 races started at the restore
  frame tick 0 (`race --from0`, where the port's clock ends 10 VBlanks
  ahead and even the capture's own seed differs from tick 3) gave tokens
  410 vs 434 (tol 18.1, NO) and hit rate 0.42 vs 0.35 (tol 0.06, NO).

# Kart state (this repo; supersedes the quarry's kart-state.md for the recomp)

Evidence tags: [CODE addr] read from the game's code (Ghidra decomp of the
recomp's function list, analysis/ghidra/), [RAM] observed in oracle captures
(tools/oracle/capture.py, DOCK grid savestate), [INFER] unverified inference.

## Where karts live
- Kart array at **0x800C30A8**, stride **0x504** bytes, count
  `*(s16*)0x800B0DFE + *(s16*)0x800B0C98` (6 + 0 in a 1-player DOCK race) [CODE 0x8007916C] [RAM].
  Slot 0 is the human player. The quarry's "player context 0x800C30A8" is this
  record; its "entity record 0x801C4D84" is render-side data.
- Per tick, FUN_8007916C (the race frame) loops over karts and calls
  **FUN_80064B5C(kart, buttons, player_index)** [CODE]. Human karts get
  `buttons = *(u16*)(0x800B0CE8 + 2*player)`; karts with `+0x496 & 1` are AI and
  get buttons from FUN_800912F8(kart), player_index = -1 [CODE]. After each
  human kart: FUN_80086B7C(kart). After the loop: FUN_80086DF0(karts, n)
  (kart-kart), FUN_800837F8(), FUN_80025D8C(karts, n); camera
  FUN_80070418(0x800C4F40, kart0) [CODE].
- Tick = 2 VBlanks: `dt = *(u8*)0x800B0AEC` = 2 [RAM]; timers subtract dt
  [CODE 0x80064B5C]. The record changes only on even VBlanks after a restore [RAM].
- Game mode short 0x800B0D46 = 1 in the race; 0x12 forces buttons to 0 [CODE].

## Kart record fields (offsets from the record base)
| off | type | meaning | evidence |
|---|---|---|---|
| 0x000 | i32 | position x (world units, ~1.9M across DOCK) | [RAM] moves with velocity |
| 0x004 | i32 | position y (up is negative? grid y = 46336) | [RAM] |
| 0x008 | i32 | position z | [RAM] |
| 0x00C | i16 | pitch (4096 = 360 deg) | [RAM] changes on bumps |
| 0x00E | i16 | **heading**, 4096 = 360 deg; forward = (sin h, cos h) in (x, z); LEFT decreases it | [RAM] matches velocity direction to 0.3 deg |
| 0x010 | i16 | roll | [RAM] |
| 0x014.. | 4 x 0x2C | per-wheel blocks (x cur/prev, z cur/prev, y?, ...) at 0x14, 0x40, 0x6C, 0x98 | [RAM] |
| 0x18C | 3x3 i16 | body rotation matrix, 4.12 | [RAM] |
| 0x214.. | i16 | wheel spin matrices (rotate about x) | [RAM] |
| 0x2E8/0x2EC/0x2F0 | i32 | previous position x/y/z | [RAM] |
| 0x2F4/0x2F8/0x2FC | i32 | velocity x/y/z (per tick; pos advances ~ v*dt) | [RAM] |
| 0x390 | i32 | speed = \|(vx, vz)\| | [RAM] 161.6 vs \|(65,-148)\| |
| 0x394 | i32 | yaw rate | [RAM] |
| 0x358 | i32 | race clock (+5 per tick) | [RAM] |
| 0x44E | u16 | buttons this tick (active high) | [CODE 0x80064BAC] |
| 0x496 | u16 | flags: bit0 = AI / no human input | [CODE] |

## Controls (this save)
CROSS accelerates. TRIANGLE held on the grid makes the kart spin in place [RAM,
probe route]. LEFT/RIGHT steer. The "Copy the buttons" start game compares
`buttons & 0xF000` with a per-kart expected button [CODE 0x80064B5C].
NOTE: CLAUDE.md and the modern-controls plugin assume Triangle = accelerate;
on this save that is wrong (see docs/notes/2026-10-12-s2.md).

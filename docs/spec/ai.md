# AI drivers (port/crates/mr-sim/src/ai.rs)

Evidence tags as in kart-state.md: [CODE addr] read from the game's code
(MIPS disassembly of the oracle's RAM, `tools/oracle/mipsdis.py`, as the
spec; Ghidra as reading aid), [RAM] matched the oracle, [INFER] unverified.
Unless marked, everything here is [CODE] and verified against the oracle
(Verification, end of file).

## Model in one paragraph
An AI kart is an ordinary kart record with bit 0 of +0x496 set. The race
frame FUN_8007916C runs the same kart update FUN_80064B5C on it as on the
player, with player index -1 and, instead of a pad, the button word
FUN_800912F8(kart) returns (0 when +0x496 & 0x60). So the AI is a virtual
pad. In a race (race type != 2/3, race flag 1 clear) that pad is
FUN_80090D00: walk 0..4 TER path points ahead of the kart's path position
(further when faster), aim at that point moved across the racing line by
twice the kart's own lateral offset (pulls the kart back to the line), steer
left/right when the aim is more than 4 x steer_rate off the heading, hold
CROSS always (after the countdown), add CIRCLE whenever it steers that hard
(brake-steer), jump at random when in water or stuck, R1 (turbo, uses the
meter) in water or on straights when behind kart 0, L1/L2 (weapons) at
random. There is no explicit rubber band beyond the kart update's catch-up,
which the game scales down for AI karts by the difficulty (8/16 in the
oracle's race: effectively off).

## Where it runs (FUN_8007916C)
1. FUN_800418D8 contact pass (every pair j < i), FUN_80041A20/41CC4.
2. FUN_8009004C path position for ONE kart per frame: kart index =
   round-robin byte 0x800C3058[karts] (0x800C305E for 6 karts: counts
   0..5, +1 per frame) [CODE 0x80079270][RAM]. Writes +0x498/+0x49A
   (segment, point), +0x3F4/+0x3F8 (track half widths), +0x3FC (lateral
   offset), +0x500, +0x48C and zeroes +0x4CB.
3. For each kart in slot order: AI karts `b = flags & 0x60 ? 0 :
   FUN_800912F8(k)`; FUN_80064B5C(k, b, -1). Humans get FUN_80086B7C after.
4. After the loop: matrices, FUN_800837F8 race positions (one kart per frame,
   same round robin), projectiles (FUN_80069648: weapon hits).

## Kart record fields the AI reads / writes
| off | type | meaning | |
|---|---|---|---|
| +0x000/+0x008 | i32 | x, z (world = TER << 5) | read |
| +0x00E | i16 | heading (12-bit) | read |
| +0x17E | i16 | steer_rate (vehicle stat, rebuilt every tick) | read: dead zone 4 x |
| +0x3E8/+0x3EC/+0x3F0 | i32 | AI target point (TER units) | written (`AiState::target`) |
| +0x3FC | i32 | lateral offset from the path point (FUN_8009004C) | read |
| +0x400 | i32 | ground speed estimate | read: look-ahead, "stuck" |
| +0x440 | i16 | impact | battle R1 rule |
| +0x47A | i16 | special re-roll timer | FUN_8009192C, update 0x80068048 |
| +0x48A | i16 | turbo meter (0x1A4 = full) | read; reset to 210 |
| +0x496 | u16 | flags: 1 AI, 2 R1-special pref., 4 R2-special pref., 8/0x10 battle states, 0x20 no catch-up scaling, 0x60 parked | read/written |
| +0x498/+0x49A | i16 | path segment / point | read |
| +0x4A0 | i16 | curvature ahead (sum of heading steps walked) | written (`AiState::turn`) |
| +0x4A2 | i16 | battle timer | `AiState::follow_timer` |
| +0x4BE | u8 | race position | read (behind kart 0?) |
| +0x4C8 | u8 | float state (water) | read: jump, R1 |
| +0x4CB | u8 | dt since the last path-position update | += dt |
| +0x4D6 | u8 | special move in progress | read: no buttons |
| +0x4D7 | i8 | slot (1..5 for AI) | read |
| +0x4DC | i8 | nearest kart | battle chase |
| +0x4DF | u8 | path direction 0 forward / 1 backward | read |
| +0x4E4 | i8 | object touched (-1 none) | read: jump |
| +0x4F3 | i8 | surface kind | read: jump on 1 / 5 |
| +0x4F9 | u8 | & 0x7F: stalled, steer only | read |
| +0x500 | u8 | path side flag (FUN_8009004C) | read: no line offset |
| +0x501 | u8 | another kart close | battle |

Globals: 0x800B0DFE karts (6), 0x800B0AEC dt, 0x800B0C70 countdown,
0x800B0A94 race clock, 0x800B0B9A race type, 0x800B07E2 race flags,
0x800B0CA6 split screen, 0x800C353E / 0x800C3566 kart 0's flags / race
position, 0x800E039A difficulty, 0x800B0E38 cup slot. TER path: points
0x800B0AC4 (16 B: x, y, z i32, heading u16, widths u8 x2), segments
0x800B0C50 (36 B: first, count, link start, n_next, n_prev, ...), links
0x800B0A84 (u8). Walk outputs (globals, locals in Rust): 0x800B0B20 seg,
0x800B0F3C point, 0x800B0CBE curvature, 0x800B0A20 first-link flag.

## Pseudocode
`rand()` = BIOS A0:2F (seed 0x80009010, LCG 0x41C64E6D / 0x3039, 15 bits).
`rng()` = the game's byte pointer 0x800B0BCC (`*p++`).

### FUN_800912F8(k) -> buttons [0x800912F8]
```
if race_type == 3:                       # the stunt trial (stunt-trial.md)
    if k.finished: b0,b1 = rng(),rng(); b = b0 < 0x10 ? SQUARE : 0
                   if b1 == 0: k.4DF ^= 1;  b |= LEFT   # (4DF < 2 always)
    return b
if race_type == 2 or race_flags & 1: return FUN_80090FAC(k)   # battle
return FUN_80090D00(k, k.4DF)
```

### FUN_80090D00(k, mode) -> buttons [0x80090D00]
```
if k.seg == -1 or k.special: return 0
v = k.speed >> 4;  n = v < 12 ? 0 : clamp(v, 64, 192) >> 6          # 0..3
k.4CB += dt;  if (k.4CB & 0xFF) > karts: n += 1                      # stale position
walk = mode == 0 ? walk_forward(k.seg, k.pt, n) : walk_backward(k.seg, k.pt, n + 1)
k.4A0 = walk.turn
p = path_point(walk.seg, walk.pt)
off = (k.500 == 0 and n > 2) ? -2 * k.lateral : 0                    # mirror across the line
a = (p.heading + 0x400) & 0xFFF
t = (p.x + (off*sin[a]) / 2^14,  p.y,  p.z + (off*cos[a]) / 2^14)   # toward zero
k.3E8.. = t
b = FUN_80090914(k, t, k.seg)
if k.4F9 & 0x7F == 0 and rand() < 0x400: b |= rand() > 0x3FFF ? L2 : L1
return b
```

### FUN_80090424(seg, pt, n) walk_forward [0x80090424]
```
turn = 0; if seg == -1: return (-1, -, 0)
repeat n times:
    h0 = heading(seg, pt);  pt += 1
    if pt >= seg.count:
        cand = -1
        for i in 0..seg.n_next:                        # successors
            if cand == -1 or rand() < 0x4000: cand = link[seg.link + i]
            if first_only_flag: break                  # 0x800B0A20: player only
        if cand == -1: pt = seg.count - 1  else: seg = cand; pt = 1
    turn += wrap(heading(seg, pt) - h0)
return (seg, pt, turn); first_only_flag = 0
```
At a fork the AI re-rolls the branch every tick (the walk starts from the
kart's own position each time): it heads between the branches until its
position is past the fork. [CODE][RAM: targets exact, see Verification]

### FUN_80090670(seg, pt, n) walk_backward [0x80090670]
Mirror image: predecessors are links [seg.link + n_next ..+ n_prev], entered
at point count - 2; none: point 0. A segment without predecessors is first
left forward with walk_forward(.., 1) until the segment changes (at most
n + 1 steps), then the backward walk continues with the remaining count.

### FUN_80090914(k, t, seg) -> buttons [0x80090914]
```
d = wrap((ratan2(t.x - k.x>>5, t.z - k.z>>5) & 0xFFF) - k.heading)
thr = s16(steer_rate << 2)
b = d > thr ? RIGHT : d < -thr ? LEFT : 0
if k.4F9 & 0x7F: return b                                  # stalled: steer only
if countdown == 0: b |= CROSS
if |d| > 4*steer_rate or (seg != -1 and seg != k.seg): b |= CIRCLE
jump = k.float or k.surface in {1, 5} or (clock > 150 and k.speed < 0x180) or k.4E4 != -1
if jump and rand() < 0x2000: b |= SQUARE                   # 1/4 per tick
if k.flags & 0x18:  (battle states; see ai.rs) ...
behind = split or kart0.flags & 1 or (k.slot > 0 and kart0.race_pos < k.race_pos)
if k.flags & (2|4):                                        # special preference
    if k.meter != 0x1A4: goto water
    if behind: return b | (flags & 2 ? R1 : R2)            # special move
    k.meter = 210; FUN_8009192C(k, 1); return b            # may not: re-roll
if behind and |k.4A0| < 0x180: return b | R1               # turbo on straights
water: if k.float: b |= R1
return b
```

### FUN_8009192C(k, n) special preference [0x8009192C]
Clears flags 2/4 of the FIRST kart only, then for n karts from k:
`+0x47A = 0; r = rng() >> 6;` (race types 2/3: 0 -> 1; mode 7 and slot >=
karts: 2) `0: flags |= 2; 1: flags |= 4; 2,3: +0x47A = ((rng() >> 3) + 29)
* 60`. Called with all karts at race load (0x8008E9xx, after the karts are
placed: the player gets a preference too, +0x496 = 2 in the oracle race),
with (k, 1) when +0x47A runs out in the kart update (0x80068048) or the AI
may not use a full meter.

### FUN_80090FAC(k) battle [0x80090FAC]
Timer +0x4A2 -= dt; on expiry or in state 0x10 without a close kart: state
9 (follow the path, direction = rng() >> 7, timer (rand()/5462 + 5) * 60);
in state 8 with a close kart: state 0x11 (chase); otherwise 1/1024 chance
to flip the path direction. State without 8 or no path: steer at the
nearest kart's driver position (+0x330.. of kart +0x4DC, >> 5) and roll the
weapons; else FUN_80090D00(k, +0x4DF). Exact against the oracle's battle
captures (phase 4, battle.md: LAGOON free run 1219 frames, battle/ko
1510 frames with three knock-outs).

### Inside FUN_80064B5C (update.rs stubs both; ai.rs has the code)
- 0x80065850 catch-up scaling: after the catch-up step, for an AI kart
  (flags & 0x21 == 1) with no split screen, race not over, cup slot < 7 and
  game state != 0x1F: `+0x4C3 = (+0x4C3 * difficulty) / 16` (toward zero).
  `ai::catchup_scale`. At difficulty 8 the stored value settles at 0..2.
- 0x80068048 timer: `if +0x47A: +0x47A -= dt; if <= 0: +0x47A = 0;
  FUN_8009192C(k, 1)` (`ai::reroll`).

### Opponents at race load (FUN_8008C4AC, 0x8008E49C..0x8008E7C4)
Grid start = rng() % karts (humans first, then AI, +1 mod karts each). Per
AI slot: `do c = rand() / 683 while c in race or char_model[c] == -1 or
!(char_table[c].flags & 1)`; vehicle = char_table[c].vehicle (re-drawn only
if 0xFF / not loaded); FUN_800609F4(kart, c, v, grid, flags 1). Then
FUN_8009192C(&kart0, karts). Tables in the exe: characters 0x800AC220
(0x14 B: +0x0E flags, +0x0F default vehicle), vehicles 0x800AC5E4 (4 B,
+3 flags), model paths 0x800AD10C (vehicles) / 0x800AD1CC (characters).
The oracle race: slots 1..5 = characters 21, 14, 23, 3, 10 on their default
vehicles 26 (TRIKE), 13 (W_BUGGY), 18 (FLAME), 7 (M_BUGGY), 9 (HOTDOG).

### Vehicle templates (FUN_80087EB4 + FUN_80087B6C)
`ai::vehicle_templates` builds all 48 templates the way the game does at
boot: the defaults loop (0x80087EFC), then replays the straight run of
li/lui/sh/sb (+ loads of values just stored) at 0x80088018..0x800899DC over
the table (the per-vehicle values are data written as code), then the
radius / carried-wheels loop (0x800899DC). `ai::place_wheels` is
FUN_80087B6C for a loaded model (wheel i at part i's pivot, x and z
negated, radius |y| >= 0x20). Vehicles have 0..6 wheels
(`KartState::wheels` is now [Wheel; 6]).

## Verification (2026-10-12; `cargo run --release --bin mr-ai -- ...`,
`cargo test --release -p mr-sim --test ai`)
Captures (analysis/ai/, local): savestate slot 2 (DOCK, 1 player, 5 AI),
9,630 VBlanks = 4,815 ticks, all 6 kart records + globals + the rand() seed
per VBlank. Items on: c_idle, c_rev60, c_rev, c_nudge, c_left, c_sel
(player routes in analysis/ai/r_*.json: idle, reverse 1 s / 6.7 s, nudge,
reverse-left, SELECT+brake). Items off (tools/oracle/capture_noitems.py):
n_* with the same routes.
- Decisions, teacher forced (`mr-ai tf analysis/ai/c_rev`): 23,206
  kart-ticks; steering, CROSS, CIRCLE, R1, R2, target point and curvature
  exact on every one; all 16 button bits exact on 23,202 (4 kart-ticks in
  two frames where other game code drew from rand() before the AI).
  Without the rand() seed (c_idle) the deterministic bits are exact except
  at forks (random branch, 165 of 18,999).
- Kart update on AI karts with the oracle's buttons (`mr-ai tfk`): x, y,
  z, angles, speeds, lap, lap times exact on 14,535 kart-ticks of the first
  3,000 ticks; vx/vy/vz differ on 13 (weapon hits applied after the kart
  loop) and 34 panic (weapon fired: out of scope).
- Vehicle templates (`mr-ai vehicles`): 47 of 48 byte-identical to RAM
  0x800D8FD0 (vehicle 22, the boss LOG, carries wheel positions from an
  earlier load).
- Opponent records (`init_opponent`) vs the oracle's AI karts: wheel
  geometry, probe corners, radius, counts, handling, camera mount,
  identity bytes equal for all five.

## Tolerance and comparison (docs/history/notes/SESSION-3-ai.md for the numbers)
Behaviour, not ticks (owner, 2026-10-12). From 6 items-off oracle races
(18 laps per kart): per kart mean lap time within max(3 %, 2 standard
errors) of the oracle's, all karts pooled within 1.5 %, |lateral| / width
median and p90 within 0.10, mean finishing position within 1.5 places.
18 port races (`mr-ai free`, `tools/oracle/ai_stats.py`): per kart lap
means -0.6..+0.8 %, pooled +0.09 %, line within 0.04, positions within
0.22 places: PASS.

## Integration (Session)
docs/history/notes/SESSION-3-ai.md section 4: race-load order (templates, grid,
`pick_opponent` + `init_opponent` per slot, `race_load_roll`), the per-tick
order (all-pairs contacts, round-robin `path_position`, `ai_buttons` ->
`items_off` -> `kart_update(.., -1)`), and the two update.rs hooks
(`catchup_scale` at 0x80065850, `reroll` at 0x80068078).

## Items (Session 4)
The AI's item buttons now act (`SessionConfig::items`, default on; the
item code is docs/spec/items.md): L1 / L2 at random (rand() < 0x400 per
tick, then L2 if a second rand() > 0x3FFF) fire a charged slot; R1 in water
(or on straights when behind kart 0) burns the meter as turbo, and with a
full meter starts the special move; with preference flag 2 / 4 and a full
meter, R1 / R2 (special move / character special) when behind kart 0, else
the meter drops to 210 and the preference is re-rolled (FUN_8009192C). So in
a race where the player is last no AI kart is "behind" and the specials
come only from R1 pressed in water with a full meter. The item code runs
the same for every kart; teacher-forced race frames are exact for all six
karts (items.md, Verification). Item-use statistics against oracle races:
docs/history/notes/2026-10-12.md "Session 4, A items sim".

# Player chase camera and render view

Port: `port/crates/mr-sim/src/camera.rs`; verification: `port/crates/mr-sim/tests/camera.rs`.
Evidence tags as in kart-state.md: [CODE addr] read from the game's code
(disassembly of the exe / generated C), [RAM] oracle captures, [TCP] live
read-only queries of the running recomp (`read_ram`, `gte_state`,
`gte_ring_dump`), [INFER] unverified.

## Where it runs
- Race frame FUN_8007916C, after all karts, kart-kart contact and FUN_80025D8C:
  `FUN_80070418(0x800C4F40, 0x800C30A8)` @0x8007A1B8; in two-player races also
  `FUN_80070418(0x800C4FB0, kart1)` @0x8007A1D4 [CODE].
- Then the frame's render, FUN_80078F38 (called last in FUN_8007916C), sets
  `*(0x800B0E60) = camera` and calls **FUN_8002CEBC(camera)** to build the view,
  then FUN_8005E96C / FUN_80029F40 draw the world with it [CODE].
- The camera therefore runs at the logic rate (one tick = dt VBlanks, normally
  2) and the picture of a tick uses the camera of that same tick.

## Camera record (0x70 bytes; player 0 at 0x800C4F40, player 1 at +0x70)
| off | type | name | meaning | evidence |
|---|---|---|---|---|
| 0x00 | i16[3][3] | matrix | camera rotation, 4.12; rows = camera right, down, forward in world space | [CODE 0x80024AB4] [RAM] |
| 0x12 | i16 | - | MATRIX padding (untouched) | |
| 0x14 | i32[3] | matrix_t | MATRIX translation, never written, 0 in a race | [RAM] |
| 0x20 | i16[3] | forward | forward vector 4.12 (same as matrix row 2 up to rounding) | [CODE 0x800727A8] |
| 0x28 | i32[3] | pos | eye position, world units (kart+0x00 space) | [CODE] [RAM] |
| 0x34 | i32[3] | pos_div32 | pos >> 5 taken before the height pass (y = unclamped goal) | [CODE 0x80071FD0] |
| 0x40 | 8 bytes | - | not touched in mode 0 | |
| 0x48 | i32 | roll_ref | intro roll reference (mode 10): +-s1/2 | [CODE 0x80071CEC] |
| 0x4C | i32 | orbit_angle | orbit angle, & 0x7FFF (modes 1, 11); the menu camera's value survives the race load | [CODE] [RAM] |
| 0x50 | i16 | pitch | 12-bit, kept in 0..4095 | [CODE] |
| 0x52 | i16 | yaw | 12-bit | [CODE] |
| 0x54 | i16 | roll | 12-bit | [CODE] |
| 0x5C | i16[3] | weight | per-axis follow weights, /1024 per VBlank | [CODE 0x80071E0C] |
| 0x64 | i16 | mode | 10 intro, 0 chase, 2 trackside after the finish; -1 set by the race load | [CODE] [RAM] |
| 0x66 | i16 | sub_state | scripted state (4 -> 5 -> 6 -> -1, 600 VBlanks each); mode 2: -1 -> 0 on its first tick | [CODE 0x80070510] |
| 0x68 | i16 | sub_timer | countdown for sub_state; mode 2: VBlanks to the next trackside re-check (120) | [CODE] |
| 0x6B | u8 | rand_a | rand() >> 14 drawn on a mode change | [CODE] |
| 0x6C | u8 | rand_b | (mode 1) | [CODE] |
| 0x6E | u8 | mode_changed | 1 on the tick the mode changed or a trackside cut; the render clears it and redraws (FUN_80078F38 @0x800790F4) | [CODE] |

## Inputs
Kart record (kart0 = 0x800C30A8):
| off | use |
|---|---|
| 0x000 | position (i32 x3) |
| 0x182, 0x184, 0x186, 0x188 | camera mount (i16): Kermit on DOCK (0, -145, 5, 145) [RAM] |
| 0x18C | body rotation matrix 4.12 |
| 0x2DC | pointer to the 12-entry position history (DOCK kart0: 0x800D4098) |
| 0x394 | yaw rate (i32) |
| 0x470 | i16 added to yaw_rate>>3 for the roll lean [INFER: slide/lean] |
| 0x4C7 | nonzero = finished (switches mode) |
| 0x4D6 | nonzero = "airborne" roll rule (never set in lap1P) |
| 0x4FE | -1 = look at self; otherwise another kart / object |

Position history ("trail"), 12 x {i32 x, y, z, count}: the kart update
FUN_80064B5C @0x80065278..0x80065324 shifts the entries down by one and writes
`{kart position, dt}` into entry 0 every tick while the countdown
0x800B0C70 == 0 (the value the kart update sees is the previous frame's end
value: no push in the GO frame itself). The
position pushed is the kart's position at the **start** of the tick (before
it moves) - verified: with that, every camera byte matches over the lap; with
the end-of-tick position it does not [RAM]. Rust: `camera::push_trail`. The
kart sim must own this ring (it is not inside the 0x504 record).

Globals: dt 0x800B0AEC; race-start countdown 0x800B0C70 (480 at race load, 0
from GO); 0x800B07F8 (300); laps 0x800B0DA6; 0x800B07E6 / 0x800B0803 (finish
rule gate, 0); game state 0x800B0D46 (1 racing, 10 results); event 0x800B0FAC (3 on DOCK); 0x800B0BD2; 0x800B0E38 (4);
kart count 0x800B0DFE (6); cut-scene sync *0x800D1C84 == *0x800D1C88;
fallback goal (0x800B0B6C+0x800B0B74)*16 etc. (`CameraGlobals`).

Ground query: the camera calls FUN_8003DDB0(x, y, z) once (the terrain
query, owned by the TER work) and reads its result globals (`CameraGround`):
0x800B0B0C (surface found), 0x800B0B24 (height of the surface closest to y),
0x800B0B04 (nearest surface at/below y), 0x800B0C84/0x800B0C9C (a surface
with flag 0x40 in byte 14 and its height), 0x800B0A54 (min height of
type-3 surfaces, byte 15 == 3; level default 0xA5AC0) [CODE 0x8003DDB0..0x8003E36C].
Heights grow downward (y down).

## Algorithm (mode 0), per tick
All angles 12-bit; `/2^n` below means the compiler's signed division by a
power of two (toward zero); `div` = MIPS div (toward zero); products wrap at
32 bits; `>>` is arithmetic.

```
mode_changed = 0; old = mode; mode = select()          // 0 in scope
if countdown == 0: sub_timer -= dt; if < 0 { sub_timer = 0; 4->5/600, 5->6/600, 6->-1 }
if mode != old: mode_changed = 1

// look-at point (FUN_8007D58C)
lift  = (s16)(kart[0x184] - kart[0x188])               // -290 for Kermit
local = (-kart[0x182], lift, -kart[0x186])
look  = kart.pos + (M_kart * local) >> 7               // each row: sum of 3 products, then >> 7

// goal (0x80071954..0x80071C24)
if mode != old: rand_a = rand() / 2^14
follow = 22400                                         // clearance above the ground
d = SquareRoot0(((kart.x-eye.x)>>5)^2 + ((kart.z-eye.z)>>5)^2)   // old eye
if d <= 1024 and countdown == 0: follow = (2048 - d) * follow / 2^10
walk the trail from entry 0 adding each entry's count until 16 VBlanks:
    entry s, step j (1..count) at which the total reaches 16
goal = trail[s].pos + div((trail[s+1].pos - trail[s].pos) * j, trail[s].count)
goal.y += lift << 5                                    // eye 9280 units above the old kart path
w_target = (128, 96, 128); pitch_div = yaw_div = 8
if d < 1024: w_target.x = w_target.z = 0
elif d < 2048: w_target.x = w_target.z = ((d - 1024) << 7) >> 10
(no trail pointer: goal = fallback, w_target = 32,32,32, divisors 16)

// eye x/z (0x80071D98..0x80071FCC); do-while loops run max(dt,1) times
old_y = eye.y
if mode != old: eye = goal; weight = w_target
else repeat dt: weight.x, weight.z step toward target by 32 (no overshoot)
                eye.x += (goal.x - eye.x) * weight.x / 2^10 ; eye.y = goal.y
                eye.z += (goal.z - eye.z) * weight.z / 2^10
pos_div32 = eye >> 5

// eye y (0x80071FFC..0x80072310)
g = FUN_8003DDB0(eye.x, goal.y, eye.z)
y = ground_clamp(goal.y, follow, lift, g)              // see below
if mode != old: weight.y = w_target.y; eye.y = y
else s = old_y; repeat dt: weight.y steps toward 96; s += (y - s) * weight.y / 2^10
     eye.y = s

// aim (0x80072314..0x80072524)
dx = (look.x-eye.x)>>7; dy = (look.y-eye.y)>>7; dz = (look.z-eye.z)>>7
pitch_t = ratan2(|dy|, SquareRoot0(dx^2+dz^2)), negated if dy < 0, & 0xFFF
yaw_t   = ratan2(dx, dz) & 0xFFF
if mode != old: pitch = pitch_t; yaw = yaw_t
else repeat dt: pitch = (pitch + div(wrap(pitch_t - pitch), 8)) & 0xFFF
                yaw   = (yaw   + div(wrap(yaw_t   - yaw),   8)) & 0xFFF
     wrap(d) = d > 2048 ? d-4096 : d < -2048 ? d+4096 : d

// roll (0x80072604..0x800727A4)
if kart[0x4D6] == 0: s = kart[0x470] + (kart[0x394] >> 3); roll_t = clamp(s/2 toward 0, -512, 512) & 0xFFF
else:                roll_t = (-3 * wrap16(yaw_t - yaw)) & 0xFFF
if mode != old: roll = roll_t
else repeat dt: roll = (roll + wrap((s16)roll_t - roll) / 2^4) & 0xFFF

// outputs
forward = (C(-p)*S(yaw)/2^12, -S(-p), C(-p)*C(yaw)/2^12)    // S, C = table>>2 (4.12)
matrix  = FUN_80024AB4(pitch, -yaw & 0xFFF, roll)
```

ground_clamp (0x80072020..0x80072248), y down, F = follow:
```
if !found:                     return y - (F + (lift << 5))
if alt_found and |y-alt| < |y-nearest|:
    if y >= alt + F:           return y
    y = alt + F
    if alt < t3 and t3 - F < y: return (alt + t3) / 2
    if below - F < y:          return (alt + below) / 2
    return y
y0 = y
y = min(y, nearest - F)
if !(alt_found and alt < nearest and t3 < alt) and t3 - F < y:
    y = t3 - F
    if alt_found and alt < t3 and y < alt + F: return (alt + t3) / 2
    return y
if alt_found and alt < y0 and y < alt + F: return (alt + nearest) / 2
return y                      (t3 = 0x800B0A54, /2 toward zero)
```
In a DOCK lap the camera is held `follow` (22400) units above the nearest
surface: y = min(goal y, nearest - 22400), then eased.

FUN_80024AB4(x, y, z, out) builds, with S/C = 4.12 table values and every
product / 4096 toward zero:
```
A = [[cy, sy*sx, -sy*cx], [0, cx, sx], [sy, -cy*sx, cy*cx]]
M = A with columns 0,1 rotated by z: col0' = col0*cz - col1*sz, col1' = col0*sz + col1*cz
out = transpose(M)
```
Tables (read from the exe at run time, `CameraTables::from_exe`): sine 1.14
at 0x800A88C0 (5120 i16; cosine = DAT_800AFF20 = sine + 1024 entries; the
table is not exactly periodic past 4096, so use it as stored), ratan2 table
0x800AD608 (1025 i16), SquareRoot0 table 0x800AD2E8 (192 i16). SquareRoot0 is
a table approximation (sqrt0(123456789) = 11076, true 11111).

## Mode selection (0x80070418..0x800707F4)
```
event 0x1D / 0x1E: special cameras (not ported)
game state 0, 17 -> 1 (pre-race orbit, before the race load); 18 -> 11
game state 1, 2: 0x800B0BD2 != 0 -> scripted rule (not ported)
                 kart+0x4C7 (finished) != 0 -> finish rule
game state 10 (results)                     -> finish rule
other game states                           -> 0
finish rule: if lap (+0x4BF) != laps (0x800B0DA6) and (0x800B07E6 == 0 or 0x800B0803 != 0):
                 +0x38C < 181 -> 0; else +0x440 >= 1000 -> 1, else 2
             else 2
both 0x800D1C84 / 0x800D1C88 non-null and equal -> 3 (9 in game state 0)
mode 0 and countdown >= 0x800B07F8 (300) -> 10
0x800B0E38 > 6 and mode != 1 -> 12
```
A 1P DOCK race goes: race load (mode -1) -> 10 at the first race frame
(countdown 480) -> 0 once the countdown is below 300 -> 2 when the player finishes, kept in
the results (game state 10, lap == laps).

## Mode 10: intro (0x80071C28..0x80071D30)
```
if changed: rand_a = rand() / 2^14
s0 = max(countdown - 60 - 300, 0); s1 = (s0 << 9) / 120     // 512 .. 0
goal = look + (M_kart * (0, 0, -(s1 + 512))) >> 7            // behind the kart, 32768 -> 16384 units
roll_ref = (rand_a ? s1 : -s1) / 2
w_target = (64, 64, 64); pitch_div = yaw_div = 8
```
Common tail as mode 0 except: y is eased like x/z (weight.y steps to 64), no
height pass, and the roll goal is (roll_ref - (kart[0x470] + kart[0x394] >> 3))
& 0xFFF with no clamp (airborne rule as mode 0). The first intro tick is a
mode change from -1: eye, weights and angles are placed directly.

## Mode 2: trackside cameras (0x800710D4..0x80071570)
```
(seg, pt) = walk(kart[0x498], kart[0x49A], 1)              // FUN_80090424, or FUN_80090670 if kart[0x4DF]
if sub_state == -1: sub_timer = 0
keep = seg == -1 or sub_timer != 0
       or (sub_state != -1 and |(kart.pos - eye) >> 10|^2 < 16384)
goal = eye
if !keep:
    d = SquareRoot0(|P(seg, pt) - P(kart seg, kart pt)|^2)   // P = TER path point, TER units
    n = d == 0 ? 1 : round(4096 / d) (q + 1 if d/2 < remainder; at least 1)
    cand = P(walk(kart seg, kart pt, n)) << 5                // about 4096 TER units ahead
    if sub_state == -1 or |(cand - eye) >> 5|^2 >= 0x1000000:
        cut: mode_changed = 1; goal = cand
        g = FUN_8003DDB0(cand)
        if g.below_found (0x800B0AE0):
            goal.y = below - 8192
            if alt_found and alt < below:
                if goal.y < alt + 8192: goal.y = (below + alt) / 2
                elif *(*0x800B0BCC)++ < 128: goal.y = alt + 8192
        sub_timer = 120
if sub_state == -1: sub_timer = 120; sub_state = 0
pitch_div = yaw_div = (goal != eye) ? 1 : 8                 // a cut turns at once
```
The eye is placed at the goal every tick (no easing); the camera pans after
the kart with /8 steps and levels its roll to 0 by /16 steps. Walks use
rand() at a branch with several successors (0x800B0A20 is clear here).

Out of scope, panicking via `unreachable_in_scope!`: modes 1, 3, 9, 11, 12,
events 0x1D/0x1E/30, 0x800B0BD2 != 0, watching another object (0x4FE != -1)
in mode 0, no karts, trail shorter than 16 VBlanks.

## Verification (teacher forcing)
`cargo test -p mr-sim --test camera -- --nocapture`. For each tick: oracle
kart + globals of the tick, oracle camera of the previous tick, trail rebuilt
from the oracle kart positions (ram0 ring for the start), ground results from
the captured globals; compare with the oracle camera of the tick.

Ticks are found from the VBlanks where the records change, not VBlank 2k: the
oracle drops frames (dt = 3 at lap1P VBlanks 1373 and 1393), after which
"tick k = VBlank 2k" samples the wrong state.

| capture | ticks | result |
|---|---|---|
| lap1P (full lap, physics regions) | 1515 | all fields exact; the whole 0x70-byte record is byte-identical on every tick (matrix, forward, pos, pos_div32, pitch, yaw, roll, weights, mode). Includes 440 VBlanks with a flag-0x40 surface (alt branch) and the two dt=3 ticks. |
| lap1P, trail = end-of-tick positions | 1515 | wrong: pos.x up to 1778, yaw 2, pitch 2, matrix 13 (shows the push uses the start-of-tick position) |
| c_steer (default regions, no ground globals) | 216 | x, z, pos_div32, weights, yaw, roll, mode exact (y, pitch, matrix not compared: the ground results were not captured) |
| start_probe from the race load (6 karts, no input) | 835 | whole record exact: 90 intro ticks (mode 10), the 10 -> 0 switch, start game, GO, race |
| start_game from the race load (solo, start game played) | 586 | whole record exact: 90 intro ticks, start game, GO |
| race3P (3 laps, finish at VBlank 7043, results) | 4324 | whole record exact: 3510 chase ticks, 814 trackside ticks (mode 2, game states 1 and 10), including one random-byte draw and one path walk that needed rand() |
| Session free run vs start_probe (oracle buttons and dt) | 372 frames | whole record exact (except 0x4C, the menu camera's leftover, and the cut flag 0x6E) through the intro, start game, GO and the race until an oracle AI item at frame 373 |

Teacher-forcing details: rand() returns the value whose >> 14 is the
oracle's rand_a; the random byte is read at the captured end-of-frame
pointer - 1 (the camera is the frame's last reader); trackside ground and
path from the disc's TER (World::dock); chase ground from the captured
globals. The session test `session_trackside_camera_after_finish` drives a
whole race on autopilot: > 80 intro ticks, then mode 2 for 600 ticks after
the finish with several cuts.

Not exercised by any capture: the airborne roll rule (kart+0x4D6 != 0),
the roll clamp at +-512, the no-trail fallback, the backward path walk in
mode 2 (+0x4DF).

## Render view (FUN_8002CEBC, then the world draw)
FUN_8002CEBC(cam) [CODE 0x8002CEBC..0x8002D1E0]:
1. Copies the 32-byte MATRIX at cam+0x00 to the view matrix **0x800C3008** and
   scales it in place:
   - row 0: `m' = (hi32((m << 9) * 0x66666667) >> 7) - (sign)` = m * 8/5 toward zero
     (512-pixel-wide frame buffer on a 4:3 screen);
   - row 1: `a = m * (240 - *(i16*)0x800B07D8); m' = ((hi32(a * 0x88888889) + a) >> 7) - sign(a)`
     = a / 240 toward zero (0x800B07D8 = 0 in a race, so row 1 is unchanged);
   - row 2 unchanged; translation stays 0.
2. Scratchpad: 0x1F800374 = eye (cam+0x28, world units); 0x1F800384 = eye/32
   (toward zero); 0x1F800394 = the scaled view MATRIX. Also gp+4160 = pitch,
   0x800B0F0A = (4096 - yaw) & 0xFFF, 0x800B0F0C = roll (for other draws).
3. The world draw does `SetRotMatrix(0x800C3008)` (FUN_80029F40 @0x8002A078)
   and, per object at world position P, `TR = (R * (P - eye)) / 2^17` toward
   zero (FUN_8002ADA8 @0x8002AE84..0x8002AF84, then TransMatrix +
   SetTransMatrix). Model vertices are in 1/32 world units:
   `camera-space (GTE) = R * v / 4096 + TR`.
4. Projection: **H = 160** (SetGeomScreen(160) in FUN_800430F8 @0x8004311C;
   InitGeom's 1000 is overwritten), **OFX, OFY = 256, 120** for one player
   (SetGeomOffset in FUN_80029F40 @0x80029FF8; split screen uses 256,180 and
   256,60); draw area 512 x 240. `sx = OFX + H*X/Z`, `sy = OFY + H*Y/Z`.

Live check [TCP, read-only, during the other agent's race]: gte_state H = 160,
OFX = 256<<16, OFY = 120<<16; 0x800C3008 = cam matrix with row 0 * 1.6
(e.g. 1395, -764, -3774 -> 2232, -1222, -6038) and row 1, 2 unchanged; 2131 of
3000 recent RTPS in gte_ring_dump used exactly that matrix as RT; for 4 of 5
visible 0x800C8B38 track objects, TR computed by the formula above from the
scratchpad eye was found exactly among the ring's TR values.

### Formulas for the Bevy renderer
Let `m = cam.matrix / 4096` (rows r0 = right, r1 = down, r2 = forward, PS1
world axes, y down) and `E = cam.pos` (world units).

- PS1 camera space of a world point P: `X = r0.(P-E)`, `Y = r1.(P-E)`, `Z = r2.(P-E)`.
- PS1 screen: `sx = 256 + 256*X/Z` (= H*1.6), `sy = 120 + 160*Y/Z` on 512x240.
  On a 4:3 display that is a horizontal half-angle tan of 1.0 (90 degrees)
  and a vertical one of 0.75: **vertical FOV = 2*atan(120/160) = 73.74 deg
  (1.2870 rad), aspect 4:3**. For wider windows keep the vertical FOV (Hor+).
- World mapping into Bevy (y up, right-handed, camera looks along -Z):
  `f(x, y, z) = (x, -y, -z) * s` (a 180-degree turn about X, so handedness and
  triangle winding are preserved; s = your unit scale, e.g. 1/32 to match
  model units or 1/4096). Camera `Transform`: translation `f(E)`, rotation
  `Quat::from_mat3(Mat3::from_cols(X, Y, Z))` with
  `X = ( m00, -m01, -m02)`, `Y = (-m10, m11, m12)`, `Z = (-m20, m21, m22)`
  (Bevy looks along -Z = f(r2), the game's forward).
  Roll is already inside m; do not add it again.
- Update the camera transform once per sim tick (30 Hz); the game shows the
  same view for both VBlanks of a tick. Interpolating between ticks is a
  renderer choice the game does not make.
- If you mirror instead (`(x, -y, z)`), flip triangle winding too.

## Open questions
- kart+0x4D6 ("airborne" roll rule) and kart+0x470 (lean term): meaning not
  pinned down; the airborne rule is ported but unverified.
- The six ground-query results must come from the terrain port's version of
  FUN_8003DDB0 (nearest surface, surface below, flag-0x40 surface, type-3
  minimum). In lap1P the end-of-frame globals happened to equal the camera's
  query; in general other callers overwrite them.
- Near/far planes: the GTE has none; the game culls objects at +-0x30000 world
  units per axis around the eye (FUN_8002ADA8) and the OT/Z limits were not
  traced.
- The recomp's widescreen mod (game.toml, default on) can change the
  horizontal projection of the running oracle; H, OFX and OFY above are the
  game's own values (gte_ring_dump showed Hs = H = 160 at the time of the check).
- Two-player and the other camera modes are not ported.

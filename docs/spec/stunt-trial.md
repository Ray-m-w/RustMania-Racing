# The stunt trial (race type 3): the game's time trial

Phase 3 of docs/PLAN-COMPLETE.md asked for "time trial mode, ghosts
(FUN_8006F64C), records". The game has no separate time trial and no
ghost: its only timed solo mode is the **1 PLAYER STUNT TRIAL** on the six
stunt courses, which is race type 3 (game mode 3). FUN_8006F64C is not a
ghost but the stunt strip locator below; the "time-trial ghost" labels in
the earlier ledger / ai.md were guesses from the race type number. The
executable's strings confirm it: "1 PLAYER STUNT TRIAL", "2 PLAYER STUNT
RACE", "FINISH THE COURSE", "COURSE FINISHED", "OUT OF TIME", "BEST TIME";
no "TIME TRIAL" or "GHOST" mode string exists (GHOST TOWN is a course).
So this phase ported the stunt trial, i.e. most of phase 5 ("the 6 STUNT
courses and their mode") as well. Evidence tags as elsewhere: [CODE addr],
[RAM] oracle RAM, [GPU] GP0 packets, [SHOT] screenshots, [INFER].

Port: mr-sim `stunt.rs` (+ progress.rs, race.rs, camera.rs, session.rs,
init.rs), mr-assets `hud.rs` / `hud_items.rs` (`stunt_hud`), mr-bevy
menus.rs (course menu), hud.rs, save.rs.

## Entering it
- The stunt courses are cup slot 3 of worlds 0..5 (0x800AC1E4): FUN
  (stunt\fun, world 0), REST (1), DINER (2), ROOFTOPS (3), MOUNTAIN (4),
  MLABS (5). FUN_8008AB5C gives them race type 3 [CODE 8008abf8]; the race
  loader FUN_8008C4AC game mode 3 and one kart (0x800B0DFE = 1).
- Course select / course list: the column / entry after BATTLE (slot 2 =
  the arena, slot 3 = STUNT; the port's labels had 2 and 3 swapped, fixed:
  0x80058E30). Open when the save's flags bit 0 is set (as the other slots).
- Course menu (FUN_80051698 case 0, race type 3): 1 PLAYER STUNT TRIAL, 2
  PLAYER STUNT RACE, <COURSE> SECRETS, GAME MENU, BACK TO MAIN MENU; bottom
  right "BEST TIME m:ss.hh" (BEST TIME for race types 2 / 3, 0x80052D90:
  the save's record for the slot); no laps. CROSS on row 0: the character
  select, then the trial [SHOT analysis/oracle/stunt/mountain_menu.png].
  The menu camera is mode 9 (below).

## Set-up (FUN_8008C4AC, FUN_800609F4)
- Countdown 0x78 + 180 = 300 (0x800B07F8 = 0x78; 300 + 180 in a race)
  [CODE 8008da2c][RAM]; no start game ("COPY THE BUTTONS" is mode 1 only).
- The grid draw *rng++ % 1 still steps the random pointer; FUN_800609F4
  puts the kart on grid point index grid + 2 (race type 3 with one kart)
  [CODE 80060b3c]; +0x4DF from a random byte (outside mode 1); the
  special-move roll FUN_8009192C runs for the one kart (+0x47A = 3000 in the
  oracle's load). +0x488 = -1 (no course points), +0x490 = FUN_8006F410
  (the strip quad under the kart), +0x4A9 = 0x40 (on every course).
- Oracle: the race load is exact, every named field (MOUNTAIN).

## Course data (DAT chunks, pointer 0x800D1C48 + 4 * id)
| chunk | RAM | what | code | port |
|---|---|---|---|---|
| 0x04 | 0x800D1C58 | gates: point pairs; the trial passes them in order (reversed with race flag 4) | FUN_80081DD8 | `stunt::gates` |
| 0x09 | 0x800D1C6C | start / finish banners: point pairs (at most 2); their strip places mark the progress bar (0x800B0C90, count 0x800B0E94; the banner animation 0x800D6CB0 is render side) | FUN_80034CE0, FUN_80070084 | `stunt::banner_marks` (marks only) |
| 0x0F / 0x10 | 0x800D1C84 / 88 | the strip: two edges of equal count; quad i = (A[i], B[i], B[i+1], A[i+1]); 0x800B0CC8 = quads (0 if a quad's corner turns the wrong way) | FUN_8006F1FC, FUN_8006F410, FUN_8006F64C, FUN_8006FB2C, FUN_800702C4, FUN_80070084 | `stunt::strip_*` |
| 0x14 | 0x800D1C98 | pipes: groups of 5 points | FUN_800628FC | `stunt::pipes` |
| 0x0D | 0x800D1C7C | paired triggers: (point, heading point) pairs, two-way links (MOUNTAIN, DINER) | FUN_80061F08, FUN_80061FC0 | progress.rs `teleport` |
| 0x08 / 0x17 | 0x800D1C68 / 0x800D1CA4 | mirror planes: wall segments / floor triangles (3 points each) | FUN_80062E38 (reflections FUN_8002976C, render side) | `stunt::mirrors` (record fields only) |
| 0x02, 0x0E, 0x11, 0x12, 0x13, 0x15, 0x18 | | hooks, jets, turntables, tokens, boxes, triggers, lifts | as on the race courses | already ported |

## Every tick (FUN_80064B5C, FUN_80082708, FUN_800837F8)
- FUN_8006F64C (race type 3, in the update after the motion): keep +0x490
  on the strip quad holding (x >> 9, z >> 9) (the quad's points >> 4,
  FUN_8003D208 on all four edges); else search quads i - d and i + d for d
  = 1..5 (back first).
- FUN_80082708, race type 3, kart not out: FUN_80081DD8 (the gates), then
  +0x358 += dt * 5, at 0x2BF21 or more 180000. The gate test: the segment
  from the previous position (+0x2E8 / +0x2F0, >> 9) to the current one
  crosses gate g (points 2g and 2g + 1, >> 4) from its positive side
  (four FUN_8003D208 tests; race flags other than 4 skip it). Passing gate
  count / 2 finishes: the time rounded to a multiple of 3 ((t / 3) * 3, +3
  when t % 3 > 1), place 0x800B0CD8, +0x4C7 = 2, +0x4CA = place; with the
  strip valid +0x388 = (points + humans - place + 1) * 0x80; with no race
  flags and a human kart a time below the course's record (0x800E0190 +
  0x20 * world + 8 * slot) becomes the record (character +4, vehicle +5,
  0x800B0816 = the player); 0x800B0CD8 += 1; no split screen and no race
  flags: the won bit 2 of the course's flags (sound 0x24 when new); +0x4F9 =
  0x80, +0x496 |= 1 (the AI takes the kart). Then the frame's dt * 5 is
  still added (oracle: record 171, +0x358 = 181) [CODE 80081dd8][RAM].
- Time limit: game mode 3 with +0x358 above 179999: out (+0x4C7 = 2),
  "OUT OF TIME" [CODE 80067ec0].
- Track direction FUN_80086B7C does nothing in race type 3.
- Race progress FUN_800837F8, race type 3 (0x800B0CC8 and the clock
  nonzero; the round robin's kart not out): +0x388 = +0x490 * 0x80 +
  +0x4A8 (mirrored with race flag 4), then the ranking as a race's.
- The AI (FUN_800912F8 race type 3): an out kart (finished or out of time)
  drives on random buttons: two random bytes, SQUARE when the first < 0x10,
  LEFT and +0x4DF ^= 1 when the second is 0 (ai.md).
- The results: once every human is out the game goes to mode 10
  (FUN_8007916C, as a race); 0x800B0AF0 = 0 then counts VBlanks (the
  "PRESS START" blink).

## Pipes (FUN_800628FC, before the ground contact, kart not on a hook)
Touching a pipe's mouth (point 0 of a group, squared distance < 0x90 in
(point >> 5) - (kart >> 10) units) sets +0x4E7 = pipe, +0x4B6 = 0, +0x4DD
|= 8. Then over 180 frames (+0x4B6 += dt, at most 0xB4): t < 60 shrink at
the mouth (+0x442 = |t - 60| / 60 of 0x1000); 60..119 run along points 0..4
(segment (t - 60) * 4 / 60, fraction ((t - 60) % 15) * 32 / 15; +0x442 = 0;
heading from point 0 to point 4; body flat); 120..179 grow at point 4 on
the ground below it (FUN_8003DDB0); at 180 the kart is out (+0x4E7 = -1,
+0x442 = 0x1000). Each step eases the position 1/16 a VBlank toward the
target and zeroes the velocity. While +0x4E7 is set the trigger lists, the
hooks, turntables, ground contact and jets are skipped [CODE 800628fc,
80067898].

## Paired triggers (FUN_80061FC0's first list)
0x800B0C68 = count / 2 when the count is a multiple of 4. A kart near
entry i's point (squared < 0x100) takes +0x4E6 = i ^ 1 (the partner),
state +0x4EF = 2: it is carried from entry i's point to the partner's
point, heading toward the point after it, as a one-way trigger does (shrink
60, fly 120, grow 60); released only once it has left the partner's point.
The one-way list (0x15) is searched when no pair is touched. On course 12
(MOUNTAIN) the end point lies 1.5 x 0x4000 further along the heading
[CODE 80061fc0].

## Camera (FUN_80070418)
Both strip chunks present with equal counts: mode 3 in a race (every game
state; over the results too), mode 9 in game mode 0 (the course menu)
[CODE 80070750].
- Mode 3 (0x80071574): FUN_8006FB2C(x >> 5, z >> 5, +0x490) projects the
  kart across its strip quad: the points A' on edge A and B' on edge B
  (interpolated along the quad by its distance from the quad's two cross
  lines) and 0x800B0AE8 = the fraction along (0..0x80), written to kart
  +0x4A8; +0x4A9 = d(A') * 128 / (d(A') + d(B')). The eye goal: A' moved
  toward the kart by min(d(A'), 0x1C00) - 0x800B078A * s / 4096, raised
  0x800B078C * s / 128 above the kart; s = 0x1000, drawing in over the
  countdown's first 180 VBlanks ((countdown - 0x78) * 0x800 / 180 off),
  backing off to 0x800 over 180 frames after the finish (+0x38C). Weights
  (128, 64, 128), divisors 16, look at the kart's look point; no roll.
- Mode 9 (0x80071844): a sweep phase (rand() / 8, direction rand() >> 14 on
  the change) +-dt a frame; FUN_800702C4((sin(phase) / 4 + 0x4000) & 0x7FFF)
  gives the points of both edges at that place of the strip: eye = edge A
  - 0x8000 in y, looking at edge B + 0x8000; weights 64, divisors 32.
- Roll: only modes 0 and 10 bank (and the orbit): 0 in modes 3 / 9.
- Oracle: exact (every camera field) in both captures below.

## HUD (FUN_80051698, FUN_8004F688 case 3, FUN_80047F1C, FUN_8004E2A8)
In add order (the GPU draws the last-added first):
- The +0x4F9 icon (as in a race).
- Countdown: "READY" blinking at or below 0x77 (as in a race); above the
  start value + 15 "FINISH THE COURSE" (0x100, 0xD2, large, colour 5); none
  in the 15 frames between (as the overhead-camera courses).
- Racing: the turbo meter FUN_8004A310, the weapon slots FUN_8004CBC8(0x2D)
  (no place, no LAP), then the progress bar FUN_80047F1C (y 0x1E): the
  kart's face (the character's icon 0x2F x 0x2F, half size 8, at 0x80 +
  progress * 0x100 / (quads << 7); shown while clock % 150 <= 0x4A;
  spinning with flags 0x60), the start mark (TILE 4 x 10, black), the
  finish chequer (six TILEs; mirrored with race flag 4), the bar (TILE 0x80,
  0x1C, 0x100 x 4, black); then the timer FUN_80050B2C (0x100, 0xD2,
  "%i:%i%i.%i", the race clock) in game mode 3 even once out.
- Out (+0x4C7) or game mode 10: FUN_8004E2A8: "OUT OF TIME" (0x100, 0x1E,
  large, colour 7) at 180000; else the time "%i:%i%i.%i%i" (0x100, 0x78,
  colour 4, 7 while it equals the course record with race flags 0 and
  +0x38C & 0x1F < 0x10), "TIME" left of it, the gap to the first from 2nd
  place, "COURSE FINISHED" (0x100, 0x1E, large, colour 5). Mode 10: "PRESS
  START" (0x100, 0xD2) while 0x800B0AF0 % 60 < 45 (the same case as a
  race's results: the port's race results now blink with it too).

## Checks (oracle, Session 7)
- Capture analysis/oracle/courses/MOUNTAIN (tools/oracle/goto_course.py
  menu MOUNTAIN; capture_course.py MOUNTAIN --hold cross 230: slot 1 ->
  MOUNTAIN's course menu -> 1 PLAYER STUNT TRIAL, Kermit / RACER, CROSS held
  to VB 2200; the kart passes the start gate and ends in the pool).
  `mr-course-check MOUNTAIN`: race load exact; free run exact over all 1033
  frames (kart, camera, random state); teacher forced exact (motion, camera,
  random) on every frame; every named kart field exact but +0x4EA (the pad
  vibration's decay, I/O side).
- Capture analysis/oracle/stunt/finish (capture_poked.py: MOUNTAIN's grid
  state, the kart poked into strip quad 349 before the finish gate with one
  gate passed, CROSS held, save block captured): `mr-course-check MOUNTAIN
  --dir analysis/oracle/stunt/finish`: teacher forced exact on all 558
  frames (the finish, the AI's random buttons after it, mode 10); free run
  exact through the finish and 115 frames of results (then the samples'
  frame-boundary jitter, below). Test `stunt_finish_matches_oracle`: the
  finish frame and the save block's record (171, Kermit / RACER) and won
  bit (0x103) as the game's RAM.
- Capture analysis/oracle/stunt/fx (fx_capture.py, the same poked route,
  GP0 frames): `hudx_check.py` 120 of 121 HUD primitives equal (FINISH THE
  COURSE, READY, the racing HUD with the progress bar's face, marks and
  bar, the result and PRESS START); the one left is a timer digit from
  mr-hudx's race-clock lag (the next frame matches).
- The mode-3 captures sample each VBlank either side of the frame boundary
  (the trial's load takes 3 VBlanks): mr-course-check counts frames by the
  random pointer and clock steps by the frame counter and tells the session
  where FUN_8005C2EC's step goes (`Session::clock_plan`, analysis only).
- `stunt_trial_on_every_stunt_course`: all six load and run a trial;
  MOUNTAIN's progress-bar marks equal the oracle's (1004, 44960).
- mr-fuzz with the stunt courses (36 runs, 0.98 M ticks, no panic).

## Not ported (yet)
- 2 PLAYER STUNT RACE: ported in phase 7 (two human karts, game mode 3;
  0x800B0C20 = the first finisher's time, the result's "+" gap; the split
  result FUN_8004E2A8: no COURSE FINISHED, the place above the centre);
  not checked against the oracle. <COURSE> SECRETS: phase
  6b (adventure.md; REVERSE and BATTLE on the stunt courses).
- (Phase 14: the mirror walls' reflections are drawn, render-ledger.md
  "Mirror walls"; oracle REST VB 545 / 600. The banners: FUN_80034CE0 sets
  the same cloth (11 x 3, banner 0's spacing, the others scaled, phase step
  0x37 / 0x3E) for every course with chunk 9, stunt courses' two banners
  included; scene.rs draws them by those rules, REST's start / finish in
  the VB 880 side by side.) The pipes / gates have no extra drawing of
  their own beyond the course DAT.
- Race flag 4 (reversed trial): ported from the code (gates, progress, the
  bar's marks, phase 6b the grid + 3); COURSE SECRETS' REVERSE row sets it;
  tests / fuzz only, no oracle check.
- 0x800B0C20 (the first finisher's time): set by the first finish
  (FUN_80081DD8, -1 at race load; mr-sim `first_finish`, phase 7).
- The stunt menu camera (mode 9) is ported from the code, not checked
  against the oracle.

## Phase 13 (Session 8): a second course, the reversed trial, two players
- ROOFTOPS (world 3's stunt course; goto_course.py menu ROOFTOPS,
  capture_course.py --hold cross 230): `mr-course-check ROOFTOPS`: race
  load exact, free run and teacher forced exact over 1033 frames. The
  checker now carries the menu camera's random picks (+0x6B / +0x6E: mode 3
  never redraws them) and the jets' on / off state from RAM.
- The reversed trial (race flag 4): adventure.md "Phase 13".
- 2 PLAYER STUNT RACE (two_mountain_stunt): race load exact (2 karts), free
  run and teacher forced exact over 1116 frames (mr-course-check: a mode-3
  frame whose only clock step is its sampled end steps by the frame
  counter's own change: the end sample can already hold the next frame's
  dt). Its result panels (two_mountain_stunt_results): GP0 174 / 174; its
  racing HUD (two/stunt_hud): the progress bar at the split (y 0x78,
  player 1's face above the bar, player 2's below, FUN_80047F1C) matches.
- FUN_80047F1C draws every kart before 0x800B0DFE: a human's face, and a
  kart with flags 0x60 spinning (ADVENTURE's parked Doozer on a stunt
  course: now drawn), CHICKEN CHASE's chicken at its strip place
  (0x800CB67C).
- Draw distances (FUN_8002E2F0): in a stunt course's race MOUNTAIN and
  REST use 0x952 with one player (the port had 5000), every stunt course
  0x952 with two; the faithful draw distance now follows it.
- The paired triggers (MOUNTAIN, DINER; and the one-way list 0x800D1C9C,
  DOCK's warps) carry markers: FUN_8007E21C -> FUN_800362D4, two turning
  additive quads, blue (0, 0, 0x80) and yellow (0x80, 0x80, 0), at each
  entry's first point within 20000 view units (GP0 mountain_paired_gp0:
  four markers, eight quads; DOCK race3 frames have the warps'). Not drawn
  by the port before; now (effects.rs `trigger_marks`).

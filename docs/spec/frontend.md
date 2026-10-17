# Front end: the title screen

Port: the title runs as the game runs it, in the race machinery: the run's
course is 0x1E (mr-bevy courses.rs), the session `SessionConfig::title`
(mr-sim title.rs: karts, knock-out re-creation, letter bounce; the orbit
camera in mr-sim camera.rs), and `port/crates/mr-bevy/src/title.rs` adds the
fade, PRESS START and START (Session 6, track T). Checks:
`tools/oracle/title_check.py`, `mr-title` (mr-sim bin), the tests in
title.rs, side-by-sides in analysis/s6/title (local). Evidence tags as elsewhere: [CODE addr] the
game's code (Ghidra decomp / generated C as the spec), [RAM] oracle RAM,
[GTE] / [GPU] oracle GTE ring / GPU state, [SHOT] oracle screenshots,
[INFER] not verified.

## What the title is
- Not a picture: the title is the frontend "course" 0x1E (`frontend\options`,
  course-name table 0x800ACAA0) run by the race engine in game mode 0x10
  (0x800B0D46) with the title sub-state 0x800B0E8A = 0. FUN_8008ACE4 (the
  course session) loads it like a race (FUN_8004627C track loader: OPTIONS.RAW,
  OPTIONS.DAT, OPTIONS.TER), then, for course 0x1E with no pending menu jump
  (0x800B07FF = 0), sets mode 0x10, sub-state 0 and FUN_8008F788 (cheat-code
  state) [CODE 8008ace4]. main boots with 0x800B0FAC = 0x1E [CODE 800786b4].
- Reached at boot after the intro movies, and from a course menu by BACK TO
  MAIN MENU: from oracle slot 1 (DOCK's course menu) DOWN x4, CROSS at VB 100
  -> loading screen -> course 30 at VB ~138, mode 0x10 and the frame counter
  0x800B0D90 = 0 at VB ~406 [RAM, analysis/s6/title/cap1].
- After 1800 VBlanks without input (counter 1800) the game fades out and
  starts the attract demo race (course 17 in this run, mode 1 at VB ~2243)
  [RAM]; START during the attract returns to the menus. Ported in phase 8
  ("Phase 8" below).

## Scene
- VRAM / slot table: OPTIONS.RAW, decoded by mr_assets::raw (course 0x1E
  skips the texel reduction); the slot table 0x800DFF00 equals the decoder's
  for all 15 used slots [RAM, test `frontend_slots_match_ram`].
- Geometry: OPTIONS.DAT, 427 layer-0 objects (no layer 1, no visibility
  zones: tail -1), 143 special slots; the frontend's other sets (world
  select, menus) are far from the title's disc at about (0, 0, 50650) DAT
  units. World vertices reconstructed from the oracle's GTE ring equal the
  decoded objects' (dat::transform) [GTE, analysis/s6/title/gte_ring.json].
- Selection: FUN_80010000 as on the race courses (DAT.md "Per-frame object
  selection"), every mask bit set, with the title's globals cd0 =
  0x800B0CD0 = 5000 (FUN_80029F40: 0x800B0EE4 < 0), de8 = 0x800B0DE8 =
  4616, b00 = 1139 [RAM]: `title::TITLE_DRAW`. This hides the world-select
  spirals and the spiked pole that a draw-everything view shows [SHOT].
- Objects move only when karts come near: FUN_80091B8C (mode 0x10) bounces
  the logo's special slots 0x68.. (20 pairs) with -0x600 when a kart or
  projectile is within 0x4000 / 0x8000, gravity 0x30 per VBlank, halved
  rebound [CODE 80091b8c]; in the oracle run 4 objects (27, 28, 30, 31) were
  displaced at VB 1000 [RAM]. Ported: mr-sim title.rs `Letters` (the
  objects' centres are the bounce x, z: RAM 0x800DDD90; heights exact on 895
  oracle ticks), drawn by mr-bevy title.rs moving the objects.
- Frame clear: course row 30 colour (0, 0, 0) (course table 0x800ACB34).
- Animated strips: FUN_8007A9DC -> FUN_800311AC as on the courses: 0xA, 0xB,
  0xD as DOCK, 0xC at (f & 0x3F) >> 4 (course != 3), 0x19 / 0x27 at
  (f & 0x1F) >> 1 (the item-box star, texture slot 0x15) [CODE 8007a9dc]
  (mr-bevy animtex.rs).
- Sprites (FUN_8007E21C) [CODE 8007e21c] [RAM]:
  - turbo tokens: DAT chunk 0x12 (35 points; FUN_8007B448 places x, z TER
    units, y << 5); colour 0..3 = rand() >> 13 each at the load (oracle run:
    2,3,1,1,3,0,0,2,0,1,1,0,3,2,2,3,0,3,0,2,0,1,1,0,0,0,0,1,3,2,0,2,2,0,3);
    sprite row 0x800AC1B8 + 8 * colour (u, v, w, h, slot, CLUT offset,
    divisor): raspberry, carrot, lime, tomato; half size scale (0x80) * w /
    divisor; angle = camera roll (0) + the group spin 0x800B0B90[i % 4]
    (FUN_8003C5FC: sin(phase) / 64) [RAM: spins (212, 3954, 3884, 142) at VB 1000];
  - item boxes: chunk 0x13 (28 points; 20 of them in two rows of stars
    beside the disc, z 46112 and 54122), the star sprite slot 0x15 CLUT +0xB,
    u 0xC0 v 0x80, 0x38 texels, half size 2 * scale, angle = camera roll;
  - both with tpage flags 0x160: drawn within +-20000 TER units of the camera
  (mr-sim pickups, drawn by mr-bevy effects.rs as on the courses).
- 2D: "PRESS START" = FUN_800474E8(0x800B0284 -> 0x80021598, x 0x100,
  y 0xD2, align 0, size 0, colour 4, abr 0x60 = opaque) from FUN_80051698's
  course-0x1E case (sub-state 0, @0x8005A3A8): only when the counter
  0x800B0D90 > 599, shown while counter % 60 < 45, and only when the fade
  is done (0x800B0F88) or fading in (0x800B0EBE) [CODE 80051698].
  Check: the port's glyphs (HUD software renderer) against oracle shots at
  VB 1008 and 1428: 1546 and 1562 of 2053 opaque pixels equal (+-8) at offset
  (0, 0) after the recomp's widescreen squash (ws.squash 3/4 on 2D); the rest
  is that squash's column resampling [SHOT, title_check.py text].

## Camera (mode 1 on course 0x1E)
FUN_80070418 picks camera mode 1 for course 0x1E in game mode 0x10
(@0x800705A0); caseD_1 (@0x80070B64) [CODE]:
- on the mode change: +0x4C t = rand() (& 0x7FFF), +0x6B = 1 (direction:
  t decreases), +0x6C = 0 (look-at ahead);
- speed DAT_800B0D58 = ((0x40000 - (clamp(len, 0x20000, 0x40000) - 0x20000))
  >> 14) << 2 on course 0x1E, with len = FUN_80081DC4(eye path) = 0 (the
  function is a stub returning 0) -> 64 per VBlank; look-ahead DAT_800B0A88
  = 0x40000 >> 8 = 0x400;
- every tick t -= 64 * dt; eye goal = FUN_80081BF4(chunk 0x0B = DAT_800D1C74,
  0, 0, t), look-at = FUN_80081BF4(chunk 0x0A = DAT_800D1C70, 0, 0, t +
  0x400) (both registered at DAT_800D1C48[id] by the DAT loader);
  FUN_80081BF4 on (0, 0): a closed loop over all n points, u = t * n * 64 >>
  15, point i = u >> 6, fraction u & 0x3F, p_i * 32 + ((p_j - p_i) * f) >> 1;
- the common tail: on the change tick the eye is set to the goal and the
  angles to their targets; then per VBlank the eye moves by (goal - eye) *
  0x80 >> 10 (weights 0x80 on all axes, fitted: the race tail's default is
  0x20), pitch / yaw turn by error / 8, roll stays 0; matrix as the race
  camera (mr_sim::camera::rot_matrix).
Port: mr-sim camera.rs mode 1 (event 0x1E, state 0x10; the angles turn as
in the races: event 30 fixes them only outside modes 1 / 7, 0x80072394).
Check: 895 ticks of the oracle's camera record (analysis/s6/title/cap1 by
`title_check.py capture`, and the mr-title runs): t, eye, pitch / yaw / roll
and the matrix exact on every tick. The first tick after the load ran with
dt = 4 in the oracle (the load's long frame).

## Fade, input, sound
- Fade: FUN_8007D174(0x80, 8) at the session start; FUN_8007D1C0 steps
  0x800AFED0 toward 2 * 0x80 by 8 * dt; 0x800B0F88 = 1 once it stays there
  (VB 408 -> 440 in the oracle: 16 ticks) [CODE, RAM]; FUN_8002D1E8 draws a
  full-screen B - F quad of (0x80 - level / 2) * 2 while the level is not
  0x80 [CODE 8002d1e8]. The port draws that quad as a B - F primitive of
  the effects pass (exact on the gamma-encoded frame).
- START: FUN_80073138 (the menu input, per player): on course 0x1E, mode
  0x10, sub-state 0, START (button bit 3, pressed not held: 0x800B079C) is
  the confirm when 0x800B0F88 / 0x800B0CE0 allow it, with the confirm sound
  FUN_8005EECC(0xF, 0x1800, 0x60, 0x60) [CODE 80073138]. The game then
  shows its main menu (sub-state 1, "Menus" below) over the same scene
  [SHOT analysis/s7/menus/oracle/main.png]; so does the port (Session 7).
- Music: course row 30 +3 = track 0x18 = TUNES/MUSIC4.XA channel 0, no
  intro clip; the oracle streamed file 1 / channel 0 from LBA 167913 (the
  start of MUSIC4.XA) during the title [CDROM, title_check.py music].

## Karts [CODE 8008c4ac, 800609f4, 80090fac, 80082708, 800834c0] [RAM]
- FUN_8008C4AC on course 0x1E, mode 0x10: 0x800B0DFE = 3 karts, no human
  (0x800B0E28 = 1 still: kart 0 is camera 0's kart); each slot draws a
  character by rand() / 0x2AB among the loaded racers no earlier slot has
  (0x800C3028[c] != -1: 0..24; character table +0x0E bit 0) and its table
  vehicle (else FUN_8008B1BC's draw among the loaded ones, 0x800D1560[v] !=
  -1: 0..18, 25..30), then FUN_800609F4(kart, c, v, grid, 1) with grid =
  (*rng % 3 + slot) % 3; the grid is DAT chunk 1 like the races (three slots
  around (0, 50000)). No FUN_8009192C special-move roll; no countdown
  (0x800B0C70 = 0); 0x800B0D80 |= 1 (pickups on); 0x800B07E2 = 8 at the end.
- FUN_800609F4 outside mode 1: +0x4DF = *rng >> 7; race type 2 (or race
  flag 1): +0x496 |= 8 (battle AI) and +0x4A2 = (rand() / 5462 + 5) * 60.
  (mr-sim init.rs.)
- The AI is FUN_80090FAC (battle): chase the nearest kart's driver
  position (+0x330/+0x338/+0x340, read live: a kart re-created earlier in
  the same frame is chased at its new place) or follow OPTIONS.TER's path
  (one 49-point loop round the disc) in a random direction, 5..10 s each.
- Ranking (FUN_800837F8 race type 2): FUN_800834C0(1), by +0x440 lowest
  first; no course progress (mr-sim race.rs).
- Race flag 8 on course 0x1E: a kart whose damage +0x440 reaches 0x7D is
  knocked out (FUN_80082708 @0x80082770) and at once re-created
  (@0x800828C4): FUN_8008B258 (a loaded racer no kart has, its own old one
  included), the vehicle as above, grid *rng % 3, FUN_800609F4(..., 1), a
  random byte and the FUN_800870E8 puff; the rest of the frame's kart update
  (lap timing on: race flag 8) runs on the new record. In the oracle run: 12
  re-creations in 1790 VBlanks.
- The logo letters: FUN_80091AA8 / FUN_80091B8C (mr-sim title.rs
  `Letters`).
- Check (mr-sim bin mr-title, from the oracle's title state saved in slot 11
  = analysis/states/title/title_vb430.pst, `title_check.py karts`): 895 of
  895 ticks with all three karts' positions, the camera eye and the 20
  letter heights exact, the random pointer and the rand() seed equal every
  tick (12 re-creations included).
- A new session (`SessionConfig::title`): with the oracle's rand() seed at
  the load (0xF8310C8E, VB 369..405) and the race loader's random pointer
  reset (RNG_START) it draws the oracle's characters (17, 23, 19), grid and
  the 35 token colours; with the oracle's water-animation state too
  (`--title-oracle`) the first 4 ticks are exact, then one extra random byte
  of the first frame (not located) shifts the random stream.
- Phase 14: the extra byte was FUN_8007A9DC's blink timers 0x800B07B0..B3
  (never reset by the loader; the oracle's load has (0, 1, 0, 0x0F): only
  one timer fires in frame 1, 17 bytes; zeros fired both). With them, the
  projectile table 0x800DDC50 and its slot 0x800B07E0 (a launch keeps the
  fields it does not write: +0x33, the random hop's grounded flag) and the
  3-kart round robin 0x800C305B (0) from the oracle's load
  (mr_sim::title::oracle_history, ORACLE_ROUND_ROBIN; `--title-oracle` sets
  them) a new title session runs exact for the whole title: `mr-title
  --fresh-vs analysis/s8/title_fresh` 918 / 918 ticks (three karts + camera
  eye), the random pointer equal up to the attract demo's own draws at VB
  2207 (FUN_80074A94, outside the title session). Found with the oracle's
  write trace of 0x800B0BCC (`wtrace_range`, return addresses per byte).

## Not ported (open)
- The menus listed under "Menus" as not ported (the attract demo and the
  cheat codes: phase 8, below).
- (The first frame's extra random byte: phase 14, above.)


# Menus (Session 7, phase 1)

Port: port/crates/mr-bevy/src/menus.rs (pages, input, drawing), mr-sim
session.rs (`menu_tick`, `camera_menu`, any player character / vehicle),
camera.rs (modes 1 / 5 for the menus). Oracle walk: tools/oracle/menu_walk.py
(screenshots + GP0 frames of each screen, analysis/s7/menus/oracle; port
captures with `--menu-keys` / `--menu-shots` in analysis/s7/menus/port;
local, not committed).

## The game's machinery [CODE]
- FUN_80073138 (menu input, per player): UP / DOWN move the row 0x800B0E24
  within 0..0x800B07AB - 1 (no wrap) with FUN_8005EECC(0xE, 0x1800, 0x40,
  0x40); LEFT / RIGHT the column 0x800B0E25; CROSS (bit 0x4000, pressed)
  sets 0x800B0794 (confirm), TRIANGLE (0x1000) 0x800B0798 (back), both with
  sound 0xF (0x1800, 0x60, 0x60); START is the confirm on the title only.
  In the character select (0x800B07CB) a row change calls FUN_8008B488
  (the kart's character = 0x800AC6A4[row]) and FUN_80072C28(p, 0) (column =
  the new character's own vehicle), a column change FUN_8008B644 (vehicle
  0x800AC6C0[col]).
- FUN_80074A94 (menu logic): per course / game mode / sub-state, what a
  confirm or back does; 0x800B0AF4 (course to load) / 0x800B0F10 (game
  mode to enter) start a fade FUN_8007D174(0, 8).
- FUN_80051698 (2D layer): every list item is FUN_800474E8(text, 0x100, y,
  0, size, colour, abr) with y = 0x78 - (rows - 1) * 8 + 0x10 * k; the
  cursor row size 0 (16 px), colour 0, blend colour (sin[(f & 0x1F) *
  0x80] >> 9) - 0x21 on all three channels (f = 0x800B07D4; sine table
  0x800A88C0, amplitude 0x4000); the others size 2 (12 px), colour 1, or
  colour 0 with a tint 0x800B068C..E (GAME MENU (0x1F, 0xFF, 0x1F), BACK TO
  MAIN MENU / NO (0xFF, 0x1F, 0x1F), YES green, SECRET COURSES (0, 0x7F,
  0xFF); the colour-0 rule comes from the oracle GP0 frames, Ghidra's
  output keeps colour 1 there) [GPU]; abr 0x60 = available, 0 = greyed
  (semi-transparent; e.g. the loaded course in the course list).
- FUN_80049C70 "t BACK" (CANCEL on the sound / controller pages) in (0x10,
  0xA4, 0x60), FUN_80049AC0 "x SELECT" (ACCEPT) in (0x68, 0x84, 0xDC), both
  size 0 colour 0; FUN_800307D4(x, y, angle, r, g, b, w, h): a flat
  triangle (0, -h), (w, h), (-w, h) rotated by angle (x scaled 0x4000 /
  0x2800), w = h = ((sin[(f & 0xF) * 0x100] >> 12) + 9) / 2; red when
  nothing is open that way, green in the character select when something
  is.

## Screens [CODE][SHOT][GPU]
| screen | where (0x800B0FAC / 0x800B0D46 / sub-state) | items | port |
|---|---|---|---|
| main menu | 0x1E / 0x10 / 0x800B0E8A = 1 | MEET THE MUPPETS, GAME STATUS, SELECT COURSE, TOURNAMENT - 1 PLAYER, OPTIONS (+ GAME STATUS CHANGED - SAVE GAME while 0x800B07DC: 6 rows); BACK = title | yes (the port autosaves: no 6th row); MEET THE MUPPETS since phase 11 ("MEET THE MUPPETS" below) |
| options | 0x1E / 0x10 / 3 (5 rows, 4 drawn) | ADJUST SOUND, SET CONTROLLERS, ADJUST SCREEN (sub-state 4), MEMORY CARD | yes (row 12: ADJUST SCREEN, MEMORY CARD; "Row 12" below); 5th row = the port's PORT SETTINGS |
| adjust sound | 0x800B07CD | MUSIC VOLUME - n, SOUND VOLUME - n (0..15; levels 0x800AC118 / 0x800AC138, default 12 = 0x6C / 0x60; FUN_80074850: LEFT / RIGHT, CROSS keeps, TRIANGLE restores) | yes (settings music_volume / sound_volume; bus gains level / level(12)) |
| set controllers | 0x800B07CC | CONTROLLER 1, s JUMP / BRAKE o, CONFIGURATION - A/B, VIBRATION - ON/OFF | yes (row 12: VIBRATION per controller, pad 2's own cursor; "Row 12" below) |
| course select | 0x1E / 0xC | world name (0x100, 0x3C), the slot's type RACE A / RACE B / BATTLE / STUNT (worlds 0..5; RACE / BATTLE / RACE / BATTLE RACE above; 0x80058E30) (0x9F, size 2, colour 1), course name (0xB4, size 1, colour 5) or "?" (colour 7); s MOVIE, x SELECT, o MUSIC (0xD2); camera mode 5 (0x80070E4C): eye (0xA000, (row * 10000 - 25000) * 32, -0x28000) looking along +z, angles fixed at 0 on event 0x1E (0x80072394) | yes; CROSS on an open race, stunt or arena course restarts on it; SQUARE = MOVIE (phase 9, fmv.md: bright and playing once the course's movie object is found, flags & 0x80); row 12: MUSIC, the backdrop and the props ("Row 12" below) |
| course menu | course / 0 / 0x800B0EA2 = 0 | 1 PLAYER RACE - n LAPS, 2 PLAYER RACE - n LAPS, COURSE SECRETS, GAME MENU, BACK TO MAIN MENU; course name (0x1E0, 0x1E, right), BEST LAP m:ss.hh (the save's record / 3 = hundredths), SELECT bottom left; LEFT / RIGHT on the race rows: laps 1..10 (0x800E038C) once the course is won (flags & 3 == 3); camera mode 1 (orbit, DAT chunks 0x0B / 0x0A). A stunt course (race type 3): 1 PLAYER STUNT TRIAL, 2 PLAYER STUNT RACE, COURSE SECRETS, GAME MENU, BACK TO MAIN MENU, "BEST TIME" (race types 2 / 3, 0x80052D90), no laps, camera mode 9 (stunt-trial.md). An arena (race type 2, phase 4): 1 PLAYER BATTLE, 2 PLAYER BATTLE, COURSE SECRETS, GAME MENU, BACK TO MAIN MENU, BEST TIME (cups above 6: sub-state 0xC, 1 PLAYER BATTLE, 2 PLAYER BATTLE, CHICKEN CHASE, BACK TO CREDITS; battle.md) | yes (2, 3 greyed); BACK TO MAIN MENU restarts on the title (or on a course the progression check opened) |
| game menu | course / 0 / (BACK shown) | ADVENTURE, GAME STATUS, CHANGE COURSE, TOURNAMENT - 1 PLAYER | ADVENTURE (phase 6: the character select, then game mode 0xB; adventure.md), CHANGE COURSE |
| world list | course / 0 / 1 | the 7 world names (cursor on the current world) | yes; + world 7 (SPEEDWAY, RALLY, GLACIER) once DISCO is won and world 8 (VOLCANO, HOUSE, WHATEVER) once everything is complete (the game's secret screen 0x1D, not ported, offers them) |
| course list | course / 0 / 2 | the world's 4 courses, the cursor's " - RACE A" etc. (as the course select's types), "?" when closed, the current course greyed | yes (arenas playable since phase 4) |
| character select | course / 0 / 0x800B07CB = 1 | name (0x100, 0x3C, top, size 2), vehicle (0xB4, bottom), up / down arrows (0x32 / 0x50), left / right arrows at the vehicle name's ends +- 0x10 (0xAE); "?" + blinking NOT COLLECTED (0xFF, 0x7F, 0) MUPPET / NOT WON (0, 0xFF, 0xFF) VEHICLE (f % 30 < 15); lists 0x800AC6A4 / 0x800AC6C0 = the 5 x 5 grid 0x800AC208 order and each character's own vehicle (FUN_80092B08 / FUN_80092928); the kart alone on grid slot 0 (FUN_80072D8C -> FUN_800609F4(kart, c, v, 0, 0)); camera mode 1 | yes; the menu session is rebuilt per pick, the camera carried over |
| pause | race / 0x800B0BD0 != 0 / 0x800B0E78 = 0 | CONTINUE, RETIRE, RESTART, ADJUST SOUND; PAUSED (0x1E0, 0x1E, right; 0x800B07D0 % 60 < 45), SELECT (0x20, 0xD2) | yes (virtual time and the XA music paused) |
| retire / restart | 0x800B0E78 = 1 / 0xB | YES, NO (opens on NO); YES = game mode 0 (course menu) / the same race again | yes |

Unlocks: the save block ("Saves" below): characters 0x800E02BC, the
vehicles won per character 0x800E02C0 + 8 * c, courses bit 0 of 0x800E0196 +
0x20 * world + 8 * slot. A new game: characters 0..7, each its own vehicle,
worlds 0..5 open, the secret courses closed (0x160 / 0x120), worlds 7 / 8
0x1FF (= the oracle's save). The port: save.rs; `unlock_all` shows the
game's everything code applied to a copy.

## Checks
- GP0: the port's font sprites (position, UV, CLUT, raw / modulated) equal
  the oracle frame's on the main menu, character select, course menu, game
  menu, world list, course list and pause menu (mr-bevy test
  `menus::tests::gp0_main_menu_and_char_select`; the pulse colour is not
  compared).
- Side by side (analysis/s7/menus/oracle vs port): every screen above;
  differences: the not-ported items greyed, the course select's backdrop
  and 3D prop, the orbit camera's place at a given moment (its random
  start), the title karts' line-up.
- mr-sim tests/menus.rs: the course menu's orbit on all 18 courses, the
  course select camera at its film strips, races with other characters /
  vehicles.

## Not ported (listed)
(MEET THE MUPPETS: phase 11, "MEET THE MUPPETS" below; MEMORY CARD,
ADJUST SCREEN, VIBRATION, MUSIC, the fades and LOADING: row 12, "Row 12"
below.) GAME STATUS CHANGED - SAVE GAME (the port autosaves).


# Saves (Session 7, phase 2)

Port: port/crates/mr-sim/src/save.rs (`SaveBlock`: the block in the game's
layout, new game, progression, records, leagues, files),
port/crates/mr-bevy/src/save.rs (save.bin, autosave, the race's effects).
Check: tools/oracle/save_check.py `unlock` (analysis/s7/saves, local).

## The block [CODE 8008f448, 8008efac, 8008be68, 80082708, 8008c4ac, 80074a94] [RAM]
0x800E0190..0x800E039C (0x20C bytes); offsets from 0x800E0190:
| offset | contents |
|---|---|
| 0x000 + 0x20 w + 8 s | 9 worlds x 4 slots (the world / course table 0x800AC1E4): i32 lap record (300 = 1 s; new game: 0x80023678), +4 the record holder's character, +5 vehicle, +6 u16 flags |
| 0x120 + 2 l | 4 leagues (RACE LEAGUE A, B, BATTLE LEAGUE, RANDOM RACE LEAGUE): flags (1 open, 2 won), +1 best points (0x0F new) |
| 0x128 | 0x800E02B8: leagues + courses complete, 0..31 (the card title's "nn/31" [INFER]) |
| 0x129 | 0x800E02B9: bit 0 DISCO won, bit 1 everything (both lead to course 0x1D, the secret screen: SPEEDWAY, RALLY, GLACIER / the arenas) |
| 0x12A | 0x800E02BA: the best start boost so far |
| 0x12C | u32 characters collected |
| 0x130 + 8 c | 2 x u32: vehicles won with character c (25 characters) |
| 0x1F8 + l | laps per league; 0x1FC / 0x1FD laps of a 1 / 2 player race (1..10, 3 new) |
| 0x1FE / 0x1FF | MEET THE MUPPETS cursor |
| 0x200 / 0x201 | music / sound volume (0..15, 12) |
| 0x202 / 0x203 | controller configuration per pad (1 = B) |
| 0x204 / 0x205 | vibration (1) |
| 0x206 / 0x207 | screen offset (main() sets 8, 2 after the new game) |
| 0x208 / 0x209 | course select cursor; 0x20A AI catch-up 8 (/16) |

Course flags: 0x001 open, 0x002 won (player 1 first in a plain race: race
flags 0, or DISCO's 8; arenas at 0x8007916C), 0x004 / 0x020 / 0x080 the
course's secret pickups taken (FUN_8008C4AC marks them gone on the next
load), 0x008 / 0x010 more per-course secrets, 0x040 no boss here (none, or
beaten in a challenge: race flag 0x10 also collects the character), 0x100
set on every non-arena slot at a new game (an arena's by its game mode 7
win); 0x1FF = complete. Phase 6 (adventure.md) reads them as ADVENTURE's
finds: 0x004 the ten targets, 0x008 every item box, 0x010 every turbo
token, 0x020 the Doozer rescued, 0x040 the "BEAT x" race won, 0x080 the
movie object; 0x100 an arena's boss battle (game mode 7) won.

A new game (FUN_8008F448, at boot): the records and one random racer per
course as holder (rand() / 0x2AB), worlds 0..5 open, world 6 0x20 (+0x40,
+0x100), worlds 7 / 8 0x1FF, leagues 0..2 open, characters 0..7 with their
own vehicles, volumes 12, laps 3, catch-up 8. The oracle's block equals the
port's new game except the random holders (mr-sim test
`new_game_matches_oracle_ram`).

## The memory card [RAM: the card file's block = the oracle's RAM]
One 8 KiB block "BASLUS-01237..." titled "MUPPET RACEMANIA: nn/31:..":
offset 0x200 the u32 sum of the block's 131 words, 0x204 the block. The
"GAME STATUS CHANGED - SAVE GAME" row appears while the first 0x1F8 bytes
(0x800B07DE) differ from the copy taken at the last load / save (0x800D8420;
main() compares between course sessions, the cheat codes too).

## What changes it [CODE]
- Each lap end (FUN_80082708 @0x80082E80): a human kart's lap under the
  course record (no race flag 2) takes the record and holder.
- The finish (@0x800832EC): player 1 first, 1 player, plain race: won bit.
- Specials (FUN_8006B828) hitting a kart: the special user's character wins
  the hit kart's vehicle (0x800E02C0; sound 0x24 when new).
- FUN_8008BE68, run by main() between course sessions and after each
  1-player league race: a won slot opens the next slot of its world (race A
  -> race B -> arena -> stunt); all six worlds' slot k open -> league k
  open; leagues A and B won -> the random league; bit 8 on all 24 courses of
  worlds 0..5 -> world 6 slot 0 (OVAL), bit 0x10 -> slot 1, bit 0x20 ->
  slot 2 and character 24, everything 0x127 + leagues A, B and battle won ->
  slot 3 (DISCO); DISCO won -> 0x800E02B9 bit 0; all 28 courses 0x1FF and
  the leagues won with 30+ (battle 20+) points -> bit 1; 0x800E02B8 counted.
  It returns the course it opened (0x800B0800); outside a league main() then
  loads that course instead of the one asked for.
- FUN_8008ACE4 at a course load: race laps back to 3 unless the course is
  won; leagues' laps likewise unless won.
- The league end (below), the menus (laps, volumes, controller
  configuration), the cheat codes (FUN_8008F884; phase 8, "Cheat codes").

## The port
- save.bin beside the executable (`--save PATH`): "MRMSAVE1", u32 version
  1, u32 checksum (the card's word sum), the 0x20C bytes. A bad file is kept
  as save.bin.bad and a new game starts.
- No save.bin: the game's save from a memory card image is imported read
  only (`saves/card1.mcd` beside the exe, psxrecomp's layout, or
  `--import-card PATH`), else a new game (rand() seeded from the clock).
- Autosave: written whenever the block changes (records and won bits during
  the race, vehicles won, the progression check at each course change, laps,
  the league end); options stay in settings.txt and are mirrored into the
  block when written. No "SAVE GAME" row; row 12: the MEMORY CARD page
  saves / loads save.bin on demand ("Row 12" below).
- Course changes run the progression check first; a newly opened race
  course the port runs is loaded instead (as main() does).
- `unlock_all` (default false): the menus show the everything code applied
  to a copy; save.bin is unchanged.
- Tool runs (routes, replays, shots) and tests never write it.

## Checks
- `save_check.py unlock`: slot 1 (DOCK's course menu) with DOCK's record
  poked to 3000 shows BEST LAP 0:10.00 [SHOT analysis/s7/saves/
  unlock_coursemenu.png]; with every world 0..5 course's flag 8, DOCK won
  and ISLAND closed, BACK TO MAIN MENU: main() ran FUN_8008BE68, ISLAND
  reopened, OVAL opened, the game loaded course 26 (OVAL) instead of the
  title and set 0x800B07DC; the block after equals the port's `progression`
  on the poked block, which returns 0x1A (mr-sim test
  `progression_matches_oracle`).

# Leagues (TOURNAMENT - 1 PLAYER; Session 7, phase 2)

Port: port/crates/mr-bevy/src/league.rs, menus.rs (`Leagues`, `LeagueMenu`,
`LeaguePoints`, `LeagueQuit`, `Standings`), mr-sim save.rs (`race_points`,
`table_order`, `league_end`). Check: save_check.py `league`.

## Phase 7: two players [CODE 8008c4ac, 80074a94, 80073e44, 80073b0c, 80051698, 8004f688, 80050f64, 80050b2c, 80050db8]
- The course menu's row 1 (2 PLAYER RACE - n LAPS with its own laps
  0x800E038D = save +0x1FD, LEFT / RIGHT once the course is won; 2 PLAYER
  BATTLE on an arena; 2 PLAYER STUNT RACE on a stunt course), cup 8's 2
  PLAYER BATTLE / CHICKEN CHASE (rows 1 / 2), world 7's 2 PLAYER RACE /
  CHICKEN CHASE (rows 1 / 5), COURSE SECRETS' CHICKEN CHASE and the stunt
  course's BATTLE set 0x800B07CA = 1 and open the character select for both
  pads (0x800B07CB = 2). The race load (FUN_8008C4AC) then makes the race
  split screen: 0x800B0CA6 = 1, 0x800B0E28 = 2, 2 karts (both human; 3 in
  a 2-player league), no AI.
- The port (menus.rs): the same rows (no longer greyed); the character
  select picks in turn (CONTROLLER 1, then CONTROLLER 2 with pad 2; the
  game shows both pickers at once: not ported); SET CONTROLLERS draws
  controller 2's column (x 0x180) and pad 2's LEFT / RIGHT switches its
  layout (0x800E0393, settings `controller_config_2`); either START pauses;
  (phase 13: both pickers at once, as the game: "Row 13" below)
  the results go back to the course menu once both players are out (mode
  10).
- The split HUD (FUN_8004F688 per human with the half's rectangle
  0x800D7388 / 0x800D7398, mr_assets `hud_items::two_hud`): the place at
  x 0x30, 0x18 inside the split (y 0x5F / 0x90); "LAP n/m" at x 0x68 (y
  0x1F / 0xD0); the weapon slots at y 0x2D / 0xC2; the turbo meter at y
  base 0x30 / 0xC0; SPECIAL MOVE at y 0x3C / 0xB4, the arrow at 0x1C / 0x94
  (scale 8; 10 overhead), the message at 0x58 / 0xD0 (medium font); the
  damage gauge at (0x30, place y); the +0x4F9 icon at 0x1C / 0x94 (0x62 /
  0x8E counting down); the start game's buttons around y 0x3C / 0xB4;
  CHICKEN CHASE's count and result; the result panels FUN_8004CEA0 /
  FUN_8004DAE0 (the time only for the winner) / FUN_8004E2A8 (the place
  above the centre, no COURSE FINISHED) around the half's centre;
  FUN_80050F64's texts ("2 PLAYER BATTLE", "CATCH %i CHICKENS", READY,
  ...), the race clock FUN_80050B2C and "PRESS START" (FUN_80050DB8) at y
  0x78. (Phase 13: the minimap / chicken icon / progress bar at the split:
  hud.md "Phase 13".)
- Check (tools/oracle/capture_two.py --hud, local
  analysis/oracle/two/dock_hud: the DOCK 2 PLAYER RACE, GP0 at VB 560,
  1000, 1600; `hudx_check.py` with mr-hudx's split layout): 140 of 186 port
  primitives found in the game's frames; the rest: the arrow lines a pixel
  off, the weapon rings' pulse and the clock's last digit (the sampled
  row's lag), and at VB 560 the start game's count digit (y 52 / 172, the
  positions above) which mr-hudx does not lay out.

## The game's flow [CODE 80074a94, 8008b9ac, 8008baf8, 8008c4ac, 800597cc, 8004eedc, 80050440] [SHOT][GPU]
- Main menu row 3 -> title sub-state 2 (FUN_80050440): "RACE LEAGUE A - n
  LAPS", "RACE LEAGUE B - n LAPS", "BATTLE LEAGUE", "RANDOM RACE LEAGUE - n
  LAPS", "?" while closed (league flags bit 0); LEFT / RIGHT laps 1..10
  (0x800E0388 + l) once the league is won (not the battle league); CROSS on
  an open league: 0x800B07E4 = 1, 0x800C1E48 = league, FUN_8008B9AC courses
  0x800C1DE8 (leagues 0..2: 0x800AD078 + 6 l = A: BOG, HAPPY, PARK, LONDON,
  DOCK, BASE; B: GHOST, GALLERY, NEW_YORK, GRAVE, ISLAND, CAPE; battle: the
  arenas; random: six distinct of A + B, rand() / 0xAAB), load course 0.
- The character select there; CROSS -> FUN_8008BAF8: the player's kart and
  five opponents drawn as a race draws them (a racer no one has, its own
  vehicle), points 0x800C1E00 cleared; then the league menu (0x800B0EA2 =
  4): "START RACE n" (n = race + 1; START TIE-BREAKER / START BATTLE),
  POINTS (0xEA2 = 3, FUN_8004EEDC: per kart in table order its swinging
  face, its points (size 1, colour 5 the leader, 6 others, 7 zero), "P1"
  blinking under the player's, 0x40 apart from 0x60), QUIT TOURNAMENT
  (0xEA2 = 5, YES / NO opening on NO: 0x800B07E4 = 0, the course menu).
- The race: the league's karts, the grid drawn (*rng % 6, then in order),
  the league's laps.
- Results (game mode 10): CROSS -> game mode 0x11 (FUN_8008C4AC computes
  the points: per kart in finishing order (0x800D8F94) place 3 / 2 / 1 / 0,
  +1 the race's best lap (+0x384 == 0x800B0DD0), +1 the best start
  (0x800B0E8C), +1 the most hits (+0x492, ties to the fewest taken +0x494);
  the table 0x800C1E30 by points, a bubble sort keeping slot order among
  equals). Page 1 RACE POINTS (FUN_80051698 case 0x11): columns 62 px apart
  centred on 0x100 in finishing order: 1ST..6TH (y 0x38, the player's
  blinking in colour 5), the four points (y 0x78 + 15 r, size 2, colour 5,
  a 0 in colour 7 semi-transparent), the total (0xB8, colour 6); the
  heading ". . . POSITION . . ." (then BEST LAP, BEST START, MOST HITS, one
  a second) on its row, hiding the two middle columns' values there; course
  name bottom right; "x NEXT". CROSS -> page 2 TOURNAMENT POINTS: the table
  68 px apart (1ST.. y 0x4C, points "%i%i" y 0xA4 colour 7), "AFTER n
  RACE(S)" / FINAL TABLE; BACK -> page 1; CROSS -> the next course
  (0x800C1DEE + 1) and its league menu. 8007916C also runs FUN_8008BE68
  after each 1-player league race.
- After the sixth: a tie for first with a human -> a tie-break race on
  course 0x1A / 0x1B (START TIE-BREAKER, the tied karts); else course 0x1A
  (0x1B battle) in game mode 0x12, the presentation; there (FUN_8008C4AC
  @0x8008CE98) the player first in the table wins the league (bit 1), more
  points than its best are kept (fanfare 0x24 for a new win or 30+ /
  battle 20+).
- In the oracle run [SHOT analysis/s7/saves]: the league page, BOG's
  character select, the league menu "START RACE 1", POINTS, results, RACE
  POINTS and TOURNAMENT POINTS "AFTER 1 RACE".

## The port
- The league crosses the course changes in league.txt beside save.bin
  (`--league`); RETIRE in a league race or QUIT TOURNAMENT ends it.
- Standings over the course's orbit with the karts lined up in front of
  the camera; after the sixth race FINAL TABLE, the tie-break race when
  the player shares first place, the presentation on OVAL / ARENA BATTLE
  (row 12, "Row 12" below), then that course's menu or the title.
- Not ported: 2 player leagues (row 13). BATTLE LEAGUE: phase 6b
  (battle.md).

## Checks
- `save_check.py league` (one run; the player's kart marked finished 300
  VBlanks after GO): the game's points and table equal `race_points` /
  `table_order` on the karts' ranks, best laps, hits and start mask (mr-sim
  test `league_points_match_oracle`; place points only in that run: no
  laps, no hits, no best start).
- GP0: the font sprites of the league page, the league menu, its POINTS,
  RACE POINTS and TOURNAMENT POINTS equal the oracle frames' (position, UV,
  CLUT, raw; mr-bevy test `menus::tests::gp0_league_pages`, the frame
  counter searched).
- In the port (analysis/s7/saves/port): the league menu, POINTS with the
  faces, a league race with the league's karts.

# Not ported in phase 2 (listed)
- GAME STATUS: ported in phase 6b (below).
- ADVENTURE: ported in phase 6 (docs/spec/adventure.md: game mode 0xB,
  the secrets, the "BEAT x" race of race flag 0x10, "YOU BEAT" / "BEAT
  YOU"); COURSE SECRETS (the boss battles, BATTLE AS BOSS, REVERSE RACE,
  ...): phase 6b (adventure.md).
- The secret pickups' bits (0x4 / 0x20 / 0x80 / 0x8 / 0x10) as races set
  them; the cheat codes (FUN_8008F884: everything, all characters, ...).
- The "new course" announcement after a league race (0x800B0801,
  FUN_8004CEA0); the course movies: ported in phase 9 (fmv.md).

# Phase 6b (Session 7): COURSE SECRETS, GAME STATUS, the secret screen's course menus, BATTLE LEAGUE
- <COURSE> SECRETS (course menu row 2, worlds 0..6; menus.rs `Page::Secrets`):
  docs/spec/adventure.md "COURSE SECRETS" (rows, what they start, checks).
- GAME STATUS (`Page::GameStatus`; main menu row 1 on the first cell, game
  menu row 1 on the course's: 0x800B07FA / 0x800B07FC = the course's slot /
  world) [CODE 8004a878, 80073b0c][GPU]: FUN_8004A878 in add order -
  SELECT COURSE (0x80, 0x30); the cursor's cell blinking with 0x800B07D4 %
  0x1E < 0xF (medium); the 7 x 4 grid (x 0x65 + 0x12 * slot, y 0x3D + 0xD *
  world): flags 0x1FF the small star (slot 0x15 clut 0xB, 0x17 x 0xF shown
  0x12 x 0xC), else "?" closed (colour 6), "_" won + targets + boxes +
  tokens + boss (0x127, colour 5), "*" (colour 7) - the font draws "_" as a
  green tick and "*" as a red cross; the star and "n/31" (0x800E02B8) under
  it; the cursor's course at 0x140: name, WON / BOSS (arenas) / DOOZER
  (worlds 0..5) / TOKENS on the left of x 0x130 and FOOD / STARS / MOVIE /
  MUPPET (a course character, 0x800ACB3C) right of 0x150, each "_" / "*",
  the labels semi-transparent where the course has no such secret; the
  star (opaque once everything is found); BEST LAP / BEST TIME (time
  courses: stunt, arenas, secret courses but STUDIO) with the record, its
  holder and vehicle; BACK; the leagues RACE A / BATTLE / RACE B ("?",
  "#7*" not won, "#5_" won, the star at 30 / 20 / 30 points); a half-black
  full-screen backdrop (FUN_8002F6FC abr 0; new `Prim::Shade`). LEFT /
  RIGHT the slot, UP / DOWN the world, TRIANGLE back. Check (local
  analysis/oracle/secrets/game_status: slot 1, DOCK, GAME MENU -> GAME
  STATUS; GP0 frame + save block): every font sprite equals the game's
  (test menus::tests::gp0_game_status).
- The secret screen's courses (FUN_80074A94 sub-states 0xB / 0xC,
  FUN_80051698 cases 0xB / 0xC): world 7 (SPEEDWAY, RALLY, GLACIER): 1
  PLAYER RACE - n LAPS, 2 PLAYER RACE (greyed), REVERSE RACE, BATTLE,
  BATTLE RACE (race flags 2 / 1 / 8, through the character select), CHICKEN
  CHASE (greyed), BACK TO CREDITS; the cup-8 arenas: 1 PLAYER BATTLE, 2
  PLAYER BATTLE (greyed), CHICKEN CHASE (greyed), BACK TO CREDITS. BACK TO
  CREDITS returns to the secret screen (course 0x1D; phase 8: THE END).
- BATTLE LEAGUE: battle.md "Phase 6b".
- The secret screen itself: phase 8 ("THE END"; reached from OPTIONS, not
  the main menu). Not ported: MEET THE MUPPETS. (The port still lists
  worlds 7 / 8 in the world list too.)


# Phase 8 (Session 7): attract demo, THE END, cheat codes

## Attract demo [CODE][RAM]
- Trigger (FUN_80074A94, course 0x1E, game mode 0x10 = the title and its
  title sub-states: PRESS START, main menu, OPTIONS, the leagues): the
  counter 0x800B0AF0 (FUN_8005C2EC adds dt; FUN_80073138 zeroes it on every
  menu input: a direction, confirm or back) past 0x707: 0x800B0BD2 = 1,
  0x800B0A82 = 1, 0x800B0790 = 1, 0x800B0AF4 = FUN_80089CA8() (fade, load).
- FUN_80089CA8, the course: two bytes of the random stream (0x800B0BCC,
  +2 a try): world = b0 % 7, slot = b1 & 3; worlds 0..5 skip slot 3 (no
  stunt course); world 6 needs its slot open (save flag 1); the course of
  0x800AC1E4[slot + 4 world] not one of the last three demos 0x800B0A1C..1E
  (the exe's initial 0xFF), which it joins. Oracle (slot 1 -> the title,
  idle; tools/oracle/routes/attract_demo.json, analysis/oracle/attract/demo,
  local): trigger at VB 2208 (counter 1800), pointer 0x8001FC92, recent 1,
  15, 21 -> NEW YORK (17); the race load at VB 2749 (mode 0 -> 1).
- The race (FUN_8008C4AC with 0x800B0A82 in game mode 0): game mode = race
  type (1 race, 2 arena), 0x800B0BD2 kept; every kart drawn and placed as
  the AI opponents are (@0x8008E49C: kart 0 too, flags 3), 10 laps, the
  countdown from 0x3C (below 0x800B07F8: no intro camera). The demo flag
  then gates: the camera (FUN_80070418: mode 2, the trackside cameras,
  while the scripted sub-state +0x66 < 5); the HUD (FUN_80051698 cases 1 /
  2: only "DEMO" FUN_800474E8(0x100, 0x1E, 0, 1, 6, 0x60) and "PRESS START"
  (0x100, 0xD2, size 0, colour 4) while the frame counter % 60 < 45 once
  the fade-in is done; no start-game draws); the AI catch-up scaling, the
  item / view buttons of player 1, the specials' vehicles won, the voice
  clips (FUN_80060580 / FUN_800606D0), the countdown beeps, the results.
- The end (FUN_8007916C, before its menu-input calls): the race clock 0x800B0A94 past
  0x4650 (1:00 at 300 a second) or kart 0 finished: 0x800B0AF4 = 0x1E (the
  title), fade. START (FUN_8007916C after LAB_80079A58, while 0x800AFEC8 = 0x80):
  the course's menu (0x800B0F10 = 0) when the course is open (save flag
  1), else the title (oracle: START at VB 3300 -> NEW YORK's course menu,
  0x800B0BD2 cleared at its load).
- Port: mr-sim `SessionConfig.demo` (0x800B0BD2; the field was misnamed
  race_over), `Session::demo_over`, `title::demo_course`; mr-bevy demo.rs:
  the idle counter (title / main menu / OPTIONS / leagues; any new press of
  a direction, CROSS, TRIANGLE or START resets it), the course by the title
  session's random pointer, a restart with `--course NAME --skip-title
  --demo --demo-recent A,B,C`; the DEMO / PRESS START layer replaces the
  HUD; no pause menu, no save, no voice clips; START / the end restart on
  the course menu or the title. `--no-demo` (and tool runs) never start it.
- Checks: the race load of the oracle's demo is exact (6 karts, every
  named field; `mr-course-check --cap analysis/oracle/attract/demo
  NEW_YORK`), free run exact for 290 frames (all karts, the trackside
  camera's cuts, the random stream) up to the START (the game then stops
  the race under the fade); tests/demo.rs (the course draw, the load's
  karts, the run to the 1:00 end); mr-fuzz --demo 168 runs, 0 panics; in
  the window: title -> idle -> THEATRE (a battle demo) -> 1:00 -> title ->
  BASE.
- Not checked: the DEMO text against a GP0 frame (the font path is the
  menus' checked one), an arena demo against the oracle (the fuzz runs
  them).

## THE END (course 0x1D) [CODE][SHOT][GPU]
- Reached from OPTIONS row 4 "THE END" (shown, as the fifth row, once
  0x800E02B9 != 0: FUN_80074A94 sets 0x800B0AF4 = 0x1D) and from the
  secret courses' BACK TO CREDITS. FUN_8008ACE4: game mode 0x15,
  0x800B0F60 = 0, cursor 0x800B0AD8[0]; no karts (0x800B0DFE = 0); the
  orbit camera (FUN_80070418 LAB_80070554: mode 1 in game mode 0x15); the
  music: FUN_80089B98 (two random bytes a try: world b0 % 6, slot b1 & 3,
  the slot open, the track 0x800ACB37 + 0xC * course not -1 and not one of
  the last three 0x800B0A18..1A). The scene: FRONTEND/THE_END.* (Kermit's
  kart on a chequered disc).
- Sub-state 0 (FUN_80051698 case 0x15): the credits table 0x800AB488 (228
  rows {text pointer or 0 (blank), size, -, -, last flag}; "THE END",
  "MUPPET RACEMANIA", "DEVELOPED BY", ...) scrolled while the frame counter
  c < 2 (h + 0xF0) (h = 0x800B0622: each row 0xC / 0x18 / 0x10 + 3 for size
  2 / 1 / 0): row y from 0xF0 - c / 2, FUN_800474E8(text, 0x100, y, 1,
  size, 0, 0x60) with the blend colour v = clamp(y * 256 / 0xF0, 0, 255):
  size 0 (v, 0x60, !v), size 1 (!v, 0x30, v), size 2 (v, 0x90, !v). A "?"
  row is a picture when 0x800E02B9 = 3 (phase 14: the trophy,
  FUN_8002FD04(0x100, y + 6 / 0xC / 8 for size 2 / 1 / 0, 0x1C, 0x1C, ...,
  u 0xC0, v 0x80, 0x37 x 0x37, slot 0x15, CLUT + 0xB, abr 0x60 / 0 behind
  the menu), drawn after the lines; `ending::credits_picture`). Then "PRESS START" (c >= (h + 0x10E) / 30 * 60, c % 60 < 45).
  START (the confirm while 0x800B0F60 = 0) -> sub-state 1 (0x800B07DA =
  0x3C).
- Sub-state 1: "t BACK" (0x100, 0x1E), "x SELECT" (0x100, 0xD2), 7 rows:
  SPEEDWAY, RALLY, GLACIER (0x800E02B9 bit 0, DISCO won), VOLCANO, HAUNTED
  HOUSE, WHATEVER (bit 1, everything) - names 0x800AB3D4, "?" while
  locked, the cursor's with " - RACE" / " - BATTLE" - and BACK TO MAIN MENU
  (tint 0xFF, 0x1F, 0x1F); the credits go on scrolling behind,
  semi-transparent (abr 0). CROSS loads course 0, 0x20, 0x21 / 0x22, 0x23,
  0x24 (their menus: "Phase 6b") or the title (0x1E); TRIANGLE back to
  sub-state 0.
- Port: mr-bevy ending.rs (`--ending`: course 0x1D, the session's camera
  only through `menu_tick`, the session's one parked kart not drawn);
  OPTIONS lists THE END before PORT SETTINGS once unlocked; BACK TO
  CREDITS restarts here.
- Checks: oracle walk (title state, 0x800E02B9 poked to 3 at the restore;
  OPTIONS -> THE END; analysis/s8/end, local): the credits' 118 font
  sprites of a GP0 frame equal the port's (position, UV, CLUT, blend
  colour; test ending::gp0_credits, frame 922); the orbit's speed equals
  the oracle's camera angle steps (16 a VBlank).
- Phase 14 (render): the kart's semi-transparent faces were THE_END.DAT's
  environment-mapped primitives (kinds 8 / 9 / 0xC, the 281 blend-0
  primitives of objects 0, 1, 4, 5): drawn twice by the game, opaque and
  as a reflection on texture slot 0x0E (render-ledger.md
  "Environment-mapped primitives"; GP0 UVs 129 / 129 and 149 / 150). The
  floor (object 15, texture slot 2) draws as the chequered disc in the
  current build (a phase-14 run with and without the new scroll: both
  chequered; the streaks were gone before phase 14, cause not
  re-identified); FUN_8007A9DC's case 0x1D scrolls its (0xE0, 0) block
  every frame, now ported (animtex.rs). Picture: GP0 frame g800 (end_gp0.py)
  trophy quad equal at frame counter 198 (test gp0_credits_picture). The sub-state 1 menu is not checked against the oracle
  (its items and hints are the checked menu drawing). Where CRDIT_01.XA
  plays is not located (not course 0x1D's row, not FUN_8008C4AC).

## Cheat codes [CODE][RAM]
- Entry (FUN_80074A94 title sub-state 0): each new press of one face
  button alone (0x800B0CE8 & 0xF000: SQUARE 0, TRIANGLE 1, CIRCLE 2, CROSS
  3; 0x800B0810 waits for the release) shifts the 10-press buffer 0x800C54A8
  (newest first); FUN_8008F7E8 compares the last ten, oldest first, with
  rows 1..6 of 0x800AD08C. A match: FUN_8008F788 (buffer cleared),
  0x800B0811 = code, FUN_8008F884(code), 0x800B080F = 0x3C, sound 0x24
  (5000, 0x80, 0x80), and GAME STATUS CHANGED when the block changed.
  The name shows while 0x800B080F (- dt a frame): FUN_800474E8(name, 0x100,
  0x1E, 0, 1, colour 5 (RESET 7), 0x60).

| code | buttons (oldest first) | name | FUN_8008F884 |
|---|---|---|---|
| 1 | TRIANGLE CIRCLE TRIANGLE SQUARE TRIANGLE CROSS TRIANGLE TRIANGLE CROSS CIRCLE | MUPPETS AND VEHICLES | FUN_8008F2AC: every character, every vehicle per character, flag 0x40 on all 36 courses |
| 2 | SQUARE SQUARE CIRCLE CIRCLE CROSS CIRCLE TRIANGLE CIRCLE TRIANGLE SQUARE | STUDIO | flag 0x08 on the 24 courses of worlds 0..5, world 6 slot 0 (OVAL) open |
| 3 | SQUARE CIRCLE CROSS CIRCLE SQUARE TRIANGLE CIRCLE CROSS CIRCLE TRIANGLE | ARCHES | flag 0x10 likewise, world 6 slot 1 (ARENA/BATTLE) open |
| 4 | CROSS SQUARE CROSS SQUARE CROSS SQUARE TRIANGLE CIRCLE CROSS SQUARE | FRAGGLE ROCK | flag 0x20 likewise, world 6 slot 2 (ROCK) open, character 24 (the Doozer) |
| 5 | CIRCLE TRIANGLE SQUARE TRIANGLE CROSS TRIANGLE SQUARE CIRCLE TRIANGLE CROSS | END COURSES | 0x800E02B9 = 3 (THE END, the secret courses) |
| 6 | TRIANGLE CROSS CIRCLE CROSS SQUARE SQUARE CROSS CIRCLE CROSS TRIANGLE | RESET | 0x800E02B9 = 0, FUN_8008EFAC (a new game's course flags), the leagues as new, FUN_8008F1F0 (characters 0..7) |

- Then FUN_8008BE68 (progression) and 0x800B0800 = -1. Row 0 (CIRCLE
  TRIANGLE CROSS CIRCLE TRIANGLE CROSS CIRCLE TRIANGLE SQUARE CROSS) and
  cases 0 (worlds 0..5 open / won, leagues 0..2 open) and 7 ("everything":
  `unlock_all`) are never matched.
- Port: mr-sim save.rs `Cheats` (entry) and `SaveBlock::cheat`; mr-bevy
  title.rs (the title's PRESS START screen: the pad word each frame, the
  save changed and written at once, the sound, the name).
- Checks: oracle (tools/oracle/routes/cheat_arches.json: slot 1 -> the
  title, the ARCHES presses one per 12 VBlanks; RAM only, the card is not
  written; analysis/oracle/cheats/arches, local): the code is taken
  (0x800B0811 = 3, 0x800B080F = 0x3C at VB 630) and the save block after
  it equals the port's `cheat(3)` of the block before (tests/cheats.rs).
  Not checked: the name's GP0 frame.

## Voice clips in the menus
- FUN_80060580 in the front end is called only by MEET THE MUPPETS (game
  mode 0xD; phase 11, below). The race's calls are in audio.md "Phase 11".

## Not ported (phase 8, listed)
- (MEET THE MUPPETS: phase 11.)
- (THE END's kart, floor and "?" picture: phase 14, "THE END" above.)
  (CRDIT_01.XA is played by no code: audio.md "Phase 11".)
- (Row 12: the fades and LOADING screen, the course select's backdrop and
  props.)

# Phase 9 (Session 7): movies
docs/spec/fmv.md has the decoder, the player's rules and the triggers. In
the front end:
- power-on: HI_15 then TT_15 before the title (main: 0x800B0804 = 0x1C);
  START / CROSS skips each after its frame 75; `skip_intro_movies` skips them;
- a course's first load since power-on (open courses): its movie (row + 9)
  before the course menu (or the demo / the league's race);
- course select: SQUARE = MOVIE on a course whose movie object was found
  (the label bright then); COURSE SECRETS: PLAY MOVIE (row 3, the same
  bit) plays the course's movie; both return to the same page (the game
  reloads it).

# MEET THE MUPPETS (Session 8, phase 11)

Main menu row 0 -> game mode 0xD on the frontend course 0x1E (FUN_80074A94
title sub-state 1: 0x800B0F10 = 0xD, 0x800B074A = -1, 0x800B074B = 0x3C,
0x800B0BD2 = 0). Port: mr_sim::gallery (records, logic, scratch record),
camera.rs mode 6 / 7, mr-bevy main.rs `gallery_tick` / `test_drive_tick`,
menus.rs `gallery_prims` / `test_drive_prims` (Page::Gallery, TestDrive),
audio.rs (the clips, the music), karts.rs (a driver without a vehicle).

## The gallery [CODE 8008c4ac, 80026920, 80026a50, 80070418, 80074a94, 80073138, 80051698] [RAM][SHOT][GPU]
- Load (FUN_8008C4AC, mode 0xD): no karts (0x800B0DFE = 0); the music
  stops (FUN_8005FA18); the cursor from the save (0x800E038E / F, block
  0x1FE / 0x1FF); FUN_80026920 fills 25 records at 0x800B3B78 (0x64 bytes,
  5 x 5): position ((col * 600 + 50000) * 32, row * 24000 - 24000,
  -0x72C0), sway springs 0, each a look timer = a random byte + 0x3C.
- Every frame FUN_80026A50 (from the scene draw FUN_80029F40, course 0x1E
  and mode 0xD) steps all 25: the timer (- dt); at 0 a new look (random
  byte / 0x56, not the current one, not 1 in column 4 nor 2 in column 0)
  and a new timer; FUN_80064738 on the record's springs (+0xC, target 0 /
  -0xC000 / 0xC000 by the look, gain 0, effect -1); FUN_80064428 /
  FUN_800644EC smooth five outputs (+0x54..+0x5C) toward springs 1, 7, 2,
  14, 8 (/ 64) by a quarter per VBlank. The characters within one row and
  one column of the cursor are drawn: the scratch kart record 0x800C49BC
  gets the position (also the driver position +0x330..), colour factors
  +0x414.. = 0x1000 (0 when the character is not collected: black), the
  outputs at +0x454.., +0x4A4 = -1 (no vehicle), +0x4A5 = the model;
  +0x1CC = FUN_800252A4(out1 / 2, out0 / 2), then FUN_80086DF0 (+0x29C
  from out2..4) and FUN_800276E8 (the driver alone). Oracle RAM (VB 300):
  the 25 positions and the scratch record's matrices equal the port's
  (mr-sim tests/gallery.rs).
- Camera mode 6 (FUN_80070418 @0x80070F14): eye ((col * 600 + 50000) *
  32, (row * 0x2EE - 0x2EE - h) * 32, -49856), looking along +z at
  (x, y, 0x20000); h = half the sum of the character's two heights
  (0x800AC038 + 4 c; the first is |part 0 pivot y| of its model, written
  when the model loads); the eye moves to z -41664 while that character's
  clip plays (0x800B0760 bit 0 and 0x800B074A = it). Angles fixed (event
  0x1E).
- Input (FUN_80073138, game mode 0xD): UP / DOWN rows 0..4 (sound 0xE),
  LEFT / RIGHT the column (clamped 0..4 by FUN_80074A94, sound 0xE when it
  changed against the saved one), CROSS accept / TRIANGLE back (sound 0xF);
  presses zero 0x800B0AF0. The logic waits for the fade-in (0x800B0F88).
- Logic (FUN_80074A94 mode 0xD): 0x800B0AF0 >= 0x708 or TRIANGLE ->
  game mode 0x10 sub-state 0 (the title's PRESS START). CROSS on a
  collected character (0x800E02BC): 0x800B0B28 = it, FUN_8008B36C (its
  vehicle list), TEST DRIVE (0xE); with nothing playing its clip: 1 for
  character 0xD, 0 for 0xE / 0x17, none for the others. Then the idle
  clip: while nothing plays 0x800B074B counts down by dt; at 0 with
  0x800B0AF0 > 0x3C, the character under the cursor (collected, not
  0x800B074A) speaks: FUN_80060580(c, n), n = 0 for 0xD, 1 for 0xE /
  0x17, else a random byte >> 7; 0x800B074B = 0x3C, 0x800B074A = c. In game
  mode 0xD FUN_80060580 queues no course track behind the clip.
- 2D (FUN_80051698 game mode 0xD): "t BACK" (0x100, 0x1E); the full name
  (0x800AB124[2 c]) at (0x100, 0xB4), medium, colour 5, or "?" in colour
  7 when not collected; after the fade "x TEST DRIVE" (0x100, 0xD2) in
  (0x68, 0x84, 0xDC), greyed (abr 0) for a character not collected. No
  arrows.

## TEST DRIVE (game mode 0xE) [CODE 8008c4ac, 800609f4, 8008b36c, 8008b644, 80070418, 80074a94, 80073138, 80051698, 800514ac, 80087d10, 8004f688, 8005fb4c] [SHOT][GPU]
- Load: kart 0 = the character in its own vehicle (0x800AC22F[c * 0x14];
  a random loaded selectable one if none), FUN_800609F4(kart, c, v, 5, 0):
  grid point 5, heading 0x600 or 0xA00 by the sign of a random byte; no
  countdown; race flag 8 (course 0x1E); camera mode 7 (@0x80070FCC):
  looking at the kart (y raised by half its probe span; past grid point 5
  the point's height, lowered by 4 * the z distance short of it), the eye
  3 * 0x1000 above and 3 * 0x4000 behind along -z (1 * on an object).
- The player drives round the frontend room (the title's world). START /
  SELECT: the next / previous vehicle of FUN_8008B36C's list (the own one,
  then those of the select screen's vehicle list it has won), sound 0xF on
  a change, FUN_8008B644 swaps it in where the kart stands (the port
  rebuilds the session and carries position, angles and velocity over).
  TRIANGLE: sound 0xF, back to the gallery (mode 0xD, 0x800B074A = -1,
  0x800B074B = 0x3C). 0x1C20 idle VBlanks (menu presses zero the count) ->
  the title.
- Music (FUN_8005FB4C in mode 0xE): whenever nothing plays, FUN_80089B98's
  track (a random open course of worlds 0..5, not one of the last three)
  once, no loop (oracle: MUSIC2.XA channels 1 / 2 = tracks 9 / 10).
- 2D (FUN_80051698 game mode 0xE): "t BACK"; three ratings: FUN_800514AC
  stars (nearest of five levels between FUN_80087D10's min / max over the
  characters' own vehicles; 1..5 stars 0xC apart, alternate ones green
  0x40) over icons at (0x70 / 0xF0 / 0x16C, 0xA8) from the vehicle row
  0x800ACDB0 + 8 v (+1, +3, +2); the vehicle name (0x100, 0x3C, medium,
  colour 5). The item HUD (FUN_8004F688 case 0xE: icon, FUN_80049440,
  weapons at 0x2D, the meter); no lap / place / clock.

## Checks (phase 11)
- Oracle walks (tools/oracle/menu_walk.py, tools/oracle/xa_probe.py from
  the title state; analysis/s8/meet, local): GP0 font sprites of the
  gallery (Kermit: 28 / 28; character 3, not collected: "?" and the greyed
  TEST DRIVE, 16 / 16) equal the port's; TEST DRIVE's layer (name,
  ratings, icons, BACK: 23 sprites) all in the oracle's frame (mr-bevy
  `menus::tests::gp0_gallery_and_test_drive`). Screenshots side by side:
  the gallery (Kermit, Piggy beside, the zoom while the clip plays), TEST
  DRIVE (Kermit / RACER on the turntable, the room, ratings).
- Clips (xa_probe: per-VBlank queue 0x800B0740.. + cdrom_sector_history):
  entering at VB 139, Kermit's idle clip 0x8050 (CHAR_04.XA channel 0,
  first sector LBA 192238) at VB 234 (the port: 96 VBlanks after the
  entry: 32 of fade-in, then the 0x3C wait and 0x800B0AF0 > 0x3C); RIGHT at
  VB 600 -> Miss Piggy's 0x805C (channel 12, LBA 192250) at VB 669 (port:
  62 VBlanks after the press, plus the frame of the press and the queue's
  frame). The port's track ids name the same files, channels and first
  sectors (mr-audio tests/xa_disc.rs). Not checked: the CROSS clips of
  characters 0xD / 0xE / 0x17 (not collected in the oracle's save).


# Row 12 (Session 8, phase 12): flow, loading, course select, options, league end

## In-process course change [port: boot.rs, loading.rs]
- The game loads a course between two course sessions (main() ->
  FUN_8008ACE4); the port used to restart its program. Now `boot::reboot`
  rebuilds the course inside the running app: the boot arguments
  (settings.rs `args`: `--course NAME --skip-title --menu --character C
  --vehicle V [--league]`, `--ending`, `--demo ...`, `--open-page ...`)
  are replaced, every entity with a transform except `KeepOnBoot` ones is
  despawned (meshes, materials, VRAM page images freed with them), each
  plugin's per-course resources are re-inserted (`boot::on_boot`, the
  same function as at start-up), and the `Boot` schedule (the former
  Startup systems: scene, VRAM textures, sky, movers, effects, HUD tables,
  title, movie quad, cameras) runs again. settings.txt is read again
  (what the menus wrote applies from the next course on, as with the
  relaunch). Kept: the save block, the music thread and audio device, the
  movies seen since power-on (movie.rs `SEEN`, no `MRM_MOVIES_SEEN`).
- Check: `--boot-soak N` (N changes: a race course, the title, ...; one
  line per settled course): over 44 changes the title settles at 1394..1437
  entities, 613..649 meshes, 66..84 images, the materials flat; the
  process's private bytes grow ~3..5 MB a change at first and flatten
  (1.23 GB -> 1.34 GB over 20 changes; no asset or entity growth: driver /
  allocator caches [INFER]).

## Fades and the LOADING picture [CODE 80074a94, 800786b4, 8008ace4, 8005df74, 8008a834, 80046dfc, 80046794, 8007d1c0, 8002d1e8] [SHOT]
- A course change: the menu's FUN_8007D174(0, 8) fades the scene out (8 a
  VBlank, 0x800AFED0 toward 0); main() plays a movie asked for (MOVIE /
  PLAY MOVIE, or the course's first load since power-on); FUN_8008ACE4
  loads GFX\LOAD_nn.RAW (FUN_8005DF74: nn = rand() / 0x5D2, 0..21, never
  the last, 0x800B0814; the file's type-0x25 picture, 320 x 256 to VRAM
  (0x280, 0x100)), fades it in by 16 a VBlank (FUN_8005DEFC(1) passes,
  0x800B0AEC = 2) and 3 more passes; the track loads; FUN_8008A834 fades
  it out by 8 a VBlank (0x800B0AEC = 1), 3 passes, then 0x800B0624 / 28 =
  -0x8000; the new session fades in from 0 (FUN_8007D174(0x80, 8)).
- The picture is the screen backdrop (FUN_80046DFC: MoveImage strips of
  the picture across the 320-pixel screen from x 0x800B0624 = 0, y
  0x800B0628 = -8 (FUN_80047218(0, -8)); FUN_80046794 fills the lines below
  with its last pixel's colour); the fade is FUN_8002D1E8's B - F quad of
  (0x80 - level / 2) * 2.
- In-course game mode changes (race start, back to the course menu,
  TEST DRIVE) fade out with the menu's FUN_8007D174(0, 8) and the new
  session fades in (main.rs: the request waits for `loading::mode_change_ready`).
- Oracle (slot 1, DOCK course menu -> BACK TO MAIN MENU; analysis/s8/p12/
  load, local): LOAD_04's frame exact on every sampled pixel; frames
  during the fades exact at subtract levels 8 and 21 (5-bit); the
  sequence fade out / picture in / hold / picture out / title fade-in as
  the port's. Race start (analysis/s8/p12/race): the screen black ~16
  VBlanks after CROSS, the race fading in.

## Course select backdrop and props [CODE 80034ce0, 80035004, 800351e8, 80091b8c] [GPU][SHOT]
- The backdrop: FUN_800351E8 grid type 2 (course 0x1E, drawn in game mode
  0xC only): a 10 x 10 vertex grid in view space, x = -1080 + 240c, y =
  -864 + 192r, z = 1000 - (v >> 8), v = (sin(p0 + 0x179 r) * sin(p1 + 500
  c)) >> 14, the phase stepping (0x23, 0x2A) a frame (FUN_80035004);
  texture slot 1 (u, v = 28c, 28r, 27 texels a cell), colour (s >> 2, s >>
  1, s), s = 0xA0 - |v >> 7|; ordering table 1400 (under everything). On
  screen sx = 256 + 192 x / z, sy = 120 + 160 y / z. Check: the oracle
  frame's 100 vertices within one pixel and the shades exact (mr-sim test
  `backdrop_matches_oracle_frame`, analysis/s7/menus/oracle/worlds.json).
- FUN_80091B8C game mode 0xC moves DAT special slots: the courses' props
  (pairs 1 + 2(4w + c)) scaled toward 0xE00 on the cursor and 0x300
  elsewhere (0x800AD0D4, eased by 1/16 a VBlank), turned (0x100, a26,
  sin(a28) >> 7), placed at ((0xA000 - a2c) >> 5 + 0x800 c, 10000 w -
  25000, -1988) with a2c easing toward column * 0x10000; a closed course
  shows the next "?" slot (0x39..) instead; the worlds' film strips
  (0xA0 + 2w, + 1) at scale 0x1000 turning about y by (frame & 0x3FF) << 2
  while their row is within 0x4000 of the eye. mr-sim title.rs
  `CourseSelect`, movers.rs (FUN_8002DF88 positions); the posed slots are
  drawn whatever the visibility stream says. Side by side with the oracle
  (worlds.png, analysis/s8/p12/cs/o_w1.png): props, strips and backdrop
  as the game's.
- A DAT finding on the way: meshes with a negative vertex count (the
  statue, the ship) keep their 8-byte positions first, then one 4-byte
  record per vertex (FUN_80025B8C skips 3 words a vertex + 1); read
  interleaved they came out 20x too big (mr-assets dat.rs).

## Options: ADJUST SCREEN, SET CONTROLLERS' VIBRATION, MEMORY CARD [CODE 80074a94, 80073138, 80051698, 8005e260, 800744ec, 8007d304, 8004357c] [GPU][SHOT]
- ADJUST SCREEN (sub-state 4): the D-pad moves the display offset (x / y
  0..32; the save's 0x800E0396 / 97, (8, 2) at power-on) live, CROSS keeps,
  TRIANGLE or 0x708 idle VBlanks restore; the page: magenta corner brackets,
  dark purple borders, x SELECT / t BACK, a red arrow toward each side
  still open. The port (screen.rs): settings `screen_x` / `screen_y`
  (mirrored into the save block); the offset moves the picture (3D view +
  HUD) by (x - 8, y - 2) of 240 lines within the window's free border (a
  window the view fills has none). Oracle GP0 (analysis/s8/p12/opt/o_adj):
  every bracket, border and arrow rectangle equal.
- VIBRATION (SET CONTROLLERS row 1, per controller; save 0x204 + pad,
  on at a new game): toggled with LEFT / RIGHT (pad 2: its own cursor,
  UP / DOWN); switching it on rumbles 0xFF for a = b = 0x1F, and while the
  cursor is on it a random byte < 8 starts (byte >> 1) + 0x80
  (FUN_800744EC). In a race FUN_8007D304 gives pad p (karts 0 / 1) large =
  +0x4B7 * a / b & 0xFE while a (+0x4EA) runs down by dt (now in mr-sim:
  `Session::pad_motor`), FUN_8004357C -> PadSetAct. The port (rumble.rs):
  Bevy `GamepadRumbleRequest`s (strong = large / 254, weak = the small
  bit) to the player's gamepad. This build has no gamepad backend (Bevy's
  gilrs integration is a crate it does not carry): no `Gamepad` entity
  exists, so the rumble (like the gamepad buttons) reaches nothing until
  one is added.
- MEMORY CARD: the game loads course 10 (0x800B0AF4 = 10, back to 0x1E):
  FRONTEND/MEM_0n.RAW (n = a random byte >> 5, + 1, not the last; its
  512 x 256 picture as the screen backdrop from line 8), then its card
  manager (PLEASE WAIT while it probes, then s LOAD / x SAVE / t EXIT at
  y 0x64 / 0x78 / 0x8C and MEMORY CARD SLOT 1 at 0xD2 for a formatted
  card; the card UI's strings at 0x80021A0C..). The port: the page over
  save.bin (no load / course change; the picture under the HUD): LOAD
  re-reads save.bin (CONFIRM LOAD, PRESS x TO CONFIRM / PRESS t TO EXIT,
  then GAME LOADED / FAILED TO LOAD), SAVE writes it (GAME SAVED / FAILED
  TO SAVE), EXIT back to OPTIONS. FORMAT is offered by the game only for
  an unformatted card (save.bin never is one: a bad file is kept as
  save.bin.bad, a new game starts). Oracle (analysis/s8/p12/card): MEM_03
  95.7 % of the sampled pixels exact (the rest the text); the labels' GP0
  positions equal.
- The course select's o MUSIC (CIRCLE, FUN_80074A94 sub-state 0xC): an
  open course with a track (row + 3) plays it at once (FUN_80060468, the
  queue cleared, 0x800B0740 = it) unless it plays already; the label is
  bright on an open course (FUN_80051698).

## League tie-break and presentation; the standings' line-up [CODE 8008c4ac, 80074a94, 80051698, 80070418, 8006b1c8, 80025d8c, 80036228] [GPU][RAM]
- After the sixth race, a tie for first with the player: the tie-break
  race on OVAL (0x1A; the battle league ARENA/BATTLE, 0x1B) with the tied
  karts (0x800C1E49; START TIE-BREAKER), its points added as a race's;
  then (or without a tie) the presentation: that course in game mode 0x12
  (FUN_8008C4AC: a league whose race is 6 and no tie); the load writes
  the league's result (won bit, best points, fanfare 0x24) and queues the
  human champion's clip 3; no karts (they are counted for the load's game
  mode 0: oracle 0x800B0DFE = 0); camera mode 11 (0x80070418 case 0xB:
  the eye along DAT chunk 0x16's path (0x800D1CA0) at 16 a VBlank, random
  start and direction; mr-sim camera.rs); TOURNAMENT CHAMPION (0x100,
  0x1E), the champion's name (0x36, size 1, colour 5), PRESS START, the
  player champion: CAN YOU GET 30 (battle 20) POINTS? while the best is
  lower, else the trophy (FUN_8002FD04, slot 0x15); START / CROSS (or 0x1C20
  idle VBlanks): after a tie-break that course opens (flags | 1); its
  course menu when open, else the title.
- Oracle: the league's end poked into slot 1 (league A, race 6, points,
  0x800B0AF4 = 0x1A and the fade; analysis/s8/p12/pres/pokes.json): game
  mode 0x12 on course 26, no karts; the font sprites equal the port's
  49 / 49 (mr-bevy `menus::tests::gp0_presentation`).
- The standings (game mode 0x11): FUN_8006B1C8 lines the karts up in front
  of the camera, drawn in camera space (FUN_80036228): RACE POINTS in
  finishing order 0x3000 apart at y -0xF00, pitch 0x180, scale 0x600;
  TOURNAMENT POINTS in table order 0x3400 apart at y 0xC00, pitch 0xC0,
  scale 0x700; z 0x8000; turning by the menu VBlanks << 4. The port
  (menus.rs `lineup_records`): records on the session camera's axes
  without the view's 1.6 x stretch (the columns' 62-pixel spacing), the
  scale as distance. Side by side: under their columns as in the game.

# Row 13 (Session 8, phase 13): two players complete

## The two-player character select [CODE 80051698 @0x80051C5C, 80073138, 80073c58, 80073e44, 80072d8c, 8006b1c8, 80036228, 80049c70] [SHOT][GPU]
- 0x800B07CB = 2: both players pick at once, a column each at x 0x80 /
  0x180 (0x100 with one player). Per player p: cursor 0x800B0E24 + 2p
  (row) / 0x800B0E25 + 2p (column), ready flag 0x800B0DCE[p]; FUN_80073138
  moves a player's cursor (and its kart) only while not ready.
- FUN_80073E44, player 1 then player 2: TRIANGLE un-readies a ready player;
  with nobody ready it leaves the character select. CROSS: FUN_80073C58
  (collected and not the character the other, ready player has: else
  ALREADY PICKED), the vehicle won (DOGFIGHT: the PLANE) -> ready; both
  ready -> the race (a 2-player league: its menu after FUN_8008BAF8).
- Drawn per column: "t BACK" unless this player is not ready while the
  other is; CANCEL instead once anyone is ready (FUN_80049C70); a ready
  player "PLAYER n" (x, 0x78, bottom, size 1, colour 6) and "READY" (top,
  colour 5); else the name (0x3C), vehicle (0xB4), arrows, "x SELECT" and
  the blinking warning: NOT COLLECTED (0xFF, 0x7F, 0) / NOT WON (0, 0xFF,
  0xFF) / ALREADY PICKED (0xFF, 0, 0xFF, the name and vehicle greyed, abr 0)
  over MUPPET / VEHICLE. The warning is the font's colour 0 with that tint
  (GP0: CLUT +0, not colour 7; the one-player screen had it wrong too).
- The karts: FUN_8006B1C8 runs every frame of the character select and
  puts the karts not yet ready in front of the camera (FUN_80036228 ->
  FUN_80026F3C(kart, 0), camera space): x 0 (one player) or -0x6000 /
  +0x6000, y 0x1800, z 0x8000, pitch 0, scale 0xC00, turning with the menu
  VBlanks << 4; a character not collected black (+0x414.. and +0x416.. 0),
  a vehicle not won black (+0x414..). The port (menus.rs
  `char_select_records`, `view_record`; player 2's kart from
  `Session::menu_kart_record`) now draws the one-player kart that way too
  (it stood on grid slot 0 in world space before).
- Port: menus.rs (`Front::ready`, `sel2`; the 2-player branch of
  `Page::CharSelect`), the overlay unanchored for the two columns
  (hud.rs `HudOverlay::centred`), `--menu-keys2` (pad 2 scripts).
- Check: the oracle's 2P character select (DOCK, player 1 ready on MISS
  PIGGY, player 2 on MISS PIGGY with a vehicle not won;
  analysis/s8/p13/charsel2.json + .png, local): every font sprite equal,
  55 / 55 (test `menus::tests::gp0_two_player_char_select`); the window
  shot matches the oracle's layout (columns, kart at the right column).

## COURSE SECRETS' "- n PLAYERS" [CODE 80051698 cases 8 / 9 / 0xA, 80074a94 sub-states 8 / 9 / 0xA (80076d64, 80077094, 8007753c)]
- 0x800B07F0..0x800B07F2 (1 or 2; LEFT / RIGHT on an open row, sound 0xE
  on a change) per row: race courses REVERSE RACE / BATTLE / BATTLE RACE
  (DISCO's row 2 CHICKEN CHASE stays two players); ARENA/BATTLE DOGFIGHT /
  DOOZER BATTLE; a stunt course's REVERSE. CROSS: 0x800B07CB = the row's
  count (the character select for both), 0x800B07CA = count - 1; DOOZER
  BATTLE starts at once (split screen with two Doozers, the vehicle turn
  stepped per kart). BOSS CHALLENGE and BATTLE AS BOSS stay one player.
- Port: `Front::secret_players`; mr-sim gives player 2 the Doozer / the
  PLANE in those modes (session.rs). Test `two_player_secret_modes`.

## TOURNAMENT - 2 PLAYERS [CODE 80074a94 sub-states 1 / 2, 8008baf8, 8008c4ac, 800597cc, 8004eedc, 80078538]
- The main menu's row 3 "TOURNAMENT - n PLAYERS" (0x800B07ED, LEFT /
  RIGHT); CROSS: 0x800C1DEF = n, 0x800C1E4B = 6 (one) or 3 (two). The
  league's karts 0..n-1 are the players' picks (0x800B0B28 / 0x800B0C18),
  FUN_8008BAF8 draws opponents for the rest of the six; a race runs
  0x800C1E4B karts (two players: split screen and one AI kart, kart 2).
- The standings (FUN_800597CC) and POINTS (FUN_8004EEDC) show 0x800B0DFE /
  0x800C1E4B karts ("P1" / "P2" over the players'); with two players every
  column hides the value of the heading's row. A tie for first with at
  least one human tied (0x800C1E4A) is played off; the save's league result
  only with one player (FUN_8008C4AC @0x8008CE98); the champion's voice
  clip when a player wins.
- Port: league.rs (`League::players`, `karts_n`, `draw_karts_for`,
  `tie_humans`); courses.rs (the league's players into the session);
  mr-sim session.rs (two players + a league: 2 + opponents karts). A tie
  with player 2 alone (phase 14, corrected from the code): the game runs it
  full screen as a one-player race: FUN_8008C4AC sets 0x800B07CA =
  (0x800C1E4A == 2) and 0x800B0CA6 = 0x800B07CA, so one tied human means no
  split screen; kart 0 is player 2's league kart (the tied karts in order)
  and FUN_8007916C reads pad 2 for it (its kart loop: kart 0 with 0x800C1E39
  set uses 0x800B0CEA); FUN_80078538 sets 0x800B0CBC = -1 so pad 1 neither
  pauses nor drives the menus. The port does the same (pad 2 drives kart 0,
  pad 1 ignored); the earlier "split screen with player 1's half blank"
  note was wrong. Tests `two_player_league` (league.rs),
  `two_player_league_race` (mr-sim).

## Saves with two players [CODE 8007916c, 80082708, 80081dd8, 800823d4]
- Every won bit (race, stunt course, arena) and the battle record need
  0x800B0CA6 == 0; lap and stunt records do not (either player sets them;
  the holder is the record's kart, 0x800B0816 = its +0x4D7:
  `RaceGlobals::record_kart`). Oracle two/battle_win: the save block
  unchanged by a 2 PLAYER BATTLE win.
- FUN_800823D4 (the last human out of a 1-player race, game mode 1, race
  flags 0): the AI catch-up scale 0x800E039A (+3 for a win, at most 16;
  else + 1 - place, at least 0). The port now applies it (save.rs) and the
  sessions take it from the save (it stayed at 8 before).

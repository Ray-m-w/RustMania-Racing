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
  [RAM]; START during the attract returns to the menus. Not ported.

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

## Not ported (open)
- The attract demo after 1800 VBlanks, the cheat codes FUN_80051698 shows
  on the title (0x800B080F); the menus listed under "Menus" as not ported.
- The first frame's extra random byte of a new title session (above).


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
| main menu | 0x1E / 0x10 / 0x800B0E8A = 1 | MEET THE MUPPETS, GAME STATUS, SELECT COURSE, TOURNAMENT - 1 PLAYER, OPTIONS (+ GAME STATUS CHANGED - SAVE GAME while 0x800B07DC: 6 rows); BACK = title | yes (1, 2 greyed; the port autosaves: no 6th row) |
| options | 0x1E / 0x10 / 3 (5 rows, 4 drawn) | ADJUST SOUND, SET CONTROLLERS, ADJUST SCREEN (sub-state 4), MEMORY CARD | yes; 3, 4 greyed; 5th row = the port's PORT SETTINGS |
| adjust sound | 0x800B07CD | MUSIC VOLUME - n, SOUND VOLUME - n (0..15; levels 0x800AC118 / 0x800AC138, default 12 = 0x6C / 0x60; FUN_80074850: LEFT / RIGHT, CROSS keeps, TRIANGLE restores) | yes (settings music_volume / sound_volume; bus gains level / level(12)) |
| set controllers | 0x800B07CC | CONTROLLER 1, s JUMP / BRAKE o, CONFIGURATION - A/B, VIBRATION - ON/OFF | configuration only (settings controller_config -> SessionConfig::alt_controls); one controller |
| course select | 0x1E / 0xC | world name (0x100, 0x3C), the slot's type RACE A / RACE B / BATTLE / STUNT (worlds 0..5; RACE / BATTLE / RACE / BATTLE RACE above; 0x80058E30) (0x9F, size 2, colour 1), course name (0xB4, size 1, colour 5) or "?" (colour 7); s MOVIE, x SELECT, o MUSIC (0xD2); camera mode 5 (0x80070E4C): eye (0xA000, (row * 10000 - 25000) * 32, -0x28000) looking along +z, angles fixed at 0 on event 0x1E (0x80072394) | yes; CROSS on an open race, stunt or arena course restarts on it; MOVIE / MUSIC greyed; the backdrop (sky, ship) and the world's 3D prop not drawn |
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
MEET THE MUPPETS (game mode 0xD, camera mode 6: the 5 x 5 character
gallery, TEST DRIVE, voice clips FUN_80060580), 2 PLAYER RACE / STUNT
RACE (phase 7), 2 PLAYER BATTLE / CHICKEN CHASE (phase 7; battle.md), MEMORY CARD and GAME STATUS CHANGED - SAVE
GAME (the port autosaves), ADJUST SCREEN, VIBRATION, MOVIE / MUSIC in the
course select (phases 9 / 8), the fades and LOADING screen between screens,
the attract demo.


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
  configuration), the cheat codes (FUN_8008F884, not ported).

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
  block when written. No MEMORY CARD page, no "SAVE GAME" row.
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
- The league crosses the per-course restarts in league.txt beside save.bin
  (`--league`); RETIRE in a league race or QUIT TOURNAMENT ends it.
- Standings over the course's orbit (the game lines the karts up in 3D);
  after the sixth race FINAL TABLE, then the result into the save and the
  title.
- Not ported: the karts lined up behind the standings, the tie-break race
  (the port's table keeps the player first among equals), the presentation
  on course 0x1A, 2 player leagues. BATTLE LEAGUE: phase 6b (battle.md).

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
  FUN_8004CEA0) and the course intro movies (phase 9).

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
  CREDITS returns to the secret screen (course 0x1D) in the game; the port
  has no secret screen yet and goes to the title.
- BATTLE LEAGUE: battle.md "Phase 6b".
- Not ported: the secret screen itself (course 0x1D, the scene with its six
  choices, 0x800B0F60, reached from the main menu once 0x800E02B9 is set;
  the port lists worlds 7 / 8 in the world list), MEET THE MUPPETS, the
  two-player rows.

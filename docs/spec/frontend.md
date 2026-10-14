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
  shows its start menu (NEW GAME / MEMORY CARD, Session 1 notes) over the
  same scene; the port opens track P's course pick-list over it, and a pick
  restarts the program on that course with `--skip-title`.
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
- The attract demo after 1800 VBlanks, the start / main menus, the cheat
  codes FUN_80051698 shows on the title (0x800B080F).
- The first frame's extra random byte of a new title session (above).

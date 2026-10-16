# Kart physics: differences ledger (port/crates/mr-sim)

Every place the Rust kart deliberately differs from the game, every stubbed
branch, and every input the port takes from outside. Companion to
kart-physics.md.

## Revisit list (not one-for-one; first to make exact for full fidelity)
AI (Session 3, mr-sim ai.rs; docs/spec/ai.md, docs/notes/SESSION-3-ai.md).
The owner's rule: AI behaves the same within a tolerance; these are the
places it is not one-for-one:
- Items (Session 4, docs/spec/items.md): ported and on in the session
  (`SessionConfig::items`); the AI's L1/L2/R1/R2 take effect. `ai::items_off`
  remains for the Session 3 items-off analysis (mr-ai). Item-use
  statistics against oracle races: docs/notes/2026-10-12.md, Session 4 A.
- Items, not one-for-one (Session 4):
  * FUN_8007F03C (kart particles) is ported (2026-10-13, items/kart_fx.rs)
    except its first loop (track objects; none on DOCK), and the kart-kart
    contact sparks of FUN_80040428 are spawned (collide.rs). With them a
    session's random pointer, rand() seed and particle / spark rings match
    the game's on every tick (items.md, Verification). FUN_8008650C
    (weather; off on DOCK) and FUN_800330D4's splash particles are not
    ported (its random draw is kept).
  * The special move reads two stack slots the skipped physics block would
    set (steer rate 0x58(sp), sideways speed 0x48(sp)); the session keeps
    the previous kart update's (`RaceGlobals::stack`): exact for AI karts;
    for the player (first in the loop) other code leaves other values, so
    in a player's special move steer lag (+0x47C), the sway springs and the
    look point (render side) differ from the game's. The yaw rate +0x448 of
    a kart in a special move is the caller's register fp: 0 [RAM].
  * Effect 17 / 22 particles need the driver matrix +0x1EC (not ported):
    their random draws are kept, the particle is not spawned.
  * 0x800AC6C0 (vehicle per character-grid cell, filled by the model loader
    FUN_80092928): taken as "every racing vehicle loaded" (true on DOCK).
  * Token colours at race load come from the 92 rand() seeds before the
    session's race-load seed (the oracle's course load was before its
    start_probe); cosmetic (+0x4F4, sparkle colours).
  * Trigger warp FUN_80061FC0 is ported but no capture touches a trigger.
  * Session race load: the first camera tick (intro mode change) takes its
    side bit from the next rand() without drawing it (oracle start_probe:
    drawing it puts every AI decision one rand() off) [INFER].
- rand(): the BIOS rand() stream (seed 0x80009010) is shared with other
  game code (camera mode changes, contact effects, race load); a session
  has its own seed, so AI path branches and jumps differ from any recorded
  race. With the oracle's seed the decisions are exact except in frames
  where other code drew first (4 of 23,206 kart-ticks).
- Contacts: the session runs the full pair loop (`collide::frame_contacts_all`,
  one reset, pairs (j, i) j < i in order, one nearest-kart table). The
  mr-ai harness (src/bin/mr-ai) still calls `frame_contacts` once per kart
  and so forgets earlier pairs' nearest / close fields (its own analysis
  only).
- update.rs now calls `ai::catchup_scale` (0x80065850) and `ai::reroll`
  (0x80068078) in place; the mr-ai harness's `ai_kart_tick` (flag bit 0
  cleared, both after the update) is kept for its teacher forcing only.
- FUN_80090FAC (battle AI) and FUN_80090670 (backward path walk, the
  battle AI's path direction) are exact against the oracle's battle
  captures (phase 4: LAGOON, battle/ko, battle/win; battle.md). The race type 3 branch of FUN_800912F8 (once called
  the "time-trial ghost") is the stunt trial's out kart on random buttons:
  exact in the oracle's stunt finish capture (phase 3, stunt-trial.md).
- Opponent choice (`ai::pick_opponent`) uses the exe's racing flag; the
  RAM tables 0x800C3028 (character model loaded) and 0x800D1560 (vehicle
  model loaded) are taken as "all loaded" (true on DOCK: all 25 racers and
  their default vehicles).
- Vehicle 22 (boss LOG) template: wheel positions only once its model is
  loaded (the oracle's RAM kept an earlier load's).

Race flow (Session 3, mr-sim race.rs / session.rs):
- After the finish the AI drives the player's kart (`ai_buttons`, items
  off: no R1 specials / weapons as the oracle's race3P had).
- Camera (camera.rs, docs/spec/camera.md): modes 10 (intro, countdown
  >= 300), 0 (chase) and 2 (trackside cameras after the finish, through the
  results) are ported and run in the session; the intro now drives the
  karts' look-at override (FUN_80041CC4) as in the game. Not ported, each
  `unreachable_in_scope!`: mode 1 (pre-race orbit in game state 0/17 before
  the race load, and the finish orbit when +0x440 >= 1000), 3/9 (cut-scene
  sync), 11 (game state 18), 12 (0x800B0E38 > 6), events 0x1D/0x1E/30,
  0x800B0BD2 != 0, split screen. Thin evidence: in race3P a trackside cut
  drew its random byte (flag-0x40 surface below the point) once, exact; a
  trackside path walk needed rand() at a branch once (the test fed 0 and the
  result still matched); the backward path walk (+0x4DF) never ran. The record's 0x4C (pre-race orbit angle) starts at
  0 in the session; the oracle carries the menu camera's value (untouched by
  modes 10/0/2). The render's clearing of the cut flag +0x6E
  (FUN_80078F38 @0x800790F4, followed by a redraw) is not modelled.
- Water animation state (0x800AFEF0) at race load: it runs on between races
  in the game; the session starts it from the exe's initial values. With
  the oracle's state the load frame is exact (random pointer included).
- Race load: the BIOS rand() seed and the random pointer at load decide
  the opponents and the grid. `SessionConfig::dock_default` uses the
  oracle's (start_probe: seed 0x832B2260, pointer 0x8001FC4E); the
  pointer reset to 0x8001F94C (`RNG_START`, FUN_8008ACE4) did not happen
  at that load, so it is only the solo session's start.
- The frame that switches to the results (mode 10) runs FUN_800606D0 (not
  ported; sound / results set-up) and is a long frame in the game (dt 4
  after it); it then runs the race simulation as any frame (Session 7: the
  session used to skip it; oracle tri/DOCK_FINISH, tri/DOCK_TIMEUP).
- AI start boost at GO (0x8005C538): rand() / (32767 / (best start x
  difficulty / 16 + 1) + 1) as an i16; with no best start yet the divisor
  wraps to -32768 and the AI gets 0. Ported, but not yet seen non-zero
  against the oracle.
- The finish voice (FUN_80060580), the "race won" save bit 0x800E0196 and the
  lap-record table 0x800E0190 bytes beyond the time: not modelled.
- After the player finishes (mode 10) the game stops lap timing for every
  kart, so AI karts still racing never finish (oracle race3P: 0x800B0CD8
  stays); the session does the same. The results screen's ranking of those
  karts (by progress?) is not ported.

## 1. Stubbed branches (`unreachable_in_scope!`, panic when reached)
Paths the port does not run. Address = the game code skipped.

Session 7 (stability) audit, against what the port lets a player do: the
title (course 0x1E, mode 0x10), the 18 race courses, one player vs AI, any
button (SELECT, TRIANGLE, L1/L2/R1/R2, START), any duration, the results,
restarts, 30 and 60 Hz. The port's game modes (0x800B0D46) are 1 (race), 10
(results) and 0x10 (title); race types 0 (race) and 2 (title); race flags 0,
8 (DISCO) and the title's; 0x800B0FAC = the course id or 0x1E; one human;
0x800B07E6 (challenge), 0x800B0BD2 (race over) and 0x800B0CA6 (split screen)
stay 0; +0x4E7 / +0x4EE are never set (no code sets them); +0x4DF (path
mode) changes only in battle / time-trial AI. Reachability below follows
from these (and `mr-fuzz`: 1520 runs, 27.2 M ticks, no panic).

Ported in Session 7 (were stubs a player reached):
| where | branch | how reached | evidence |
|---|---|---|---|
| update.rs / camera.rs | TRIANGLE camera lock FUN_800728C0 (nearest kart; with none near, the nearest look point of DAT chunk 7) and the chase camera watching it (+0x4FE != -1) | any TRIANGLE press after race clock 300 (keyboard A): the owner's "freeze" | camera.md "TRIANGLE: the camera lock" (oracle tri/*) |
| update.rs | time limit (0x80067EC0): race clock > 179999 with laps to go: +0x4C7 = 2, +0x4F9 = 0x80, AI drives, voice byte, FUN_8009192C | idling 10 minutes | tri/DOCK_TIMEUP (clock poked to 179000 from slot 2): free run and teacher forced exact, 709 frames, all six karts out, results |
| session.rs | the results switch frame runs the race simulation (was skipped) | every finish / time-up | tri/DOCK_FINISH, tri/DOCK_TIMEUP teacher forced exact through the switch |
| session.rs | one of two humans finished: the game goes on (0x80079708) | (split screen) | code only |

Ported in Session 7, phase 3 (the stunt trial, race type 3 / game mode 3;
docs/spec/stunt-trial.md; reached from the course menus of the six stunt
courses):
| where | branch | evidence |
|---|---|---|
| update.rs | FUN_8006F64C: the strip quad +0x490 (was the "time-trial ghost" stub) | oracle MOUNTAIN, stunt/finish: free run / teacher forced exact |
| progress.rs | lap timing for race type 3: FUN_80081DD8 gates, the trial clock, the finish and course record | stunt/finish: exact, record and won bit equal the save block |
| progress.rs | FUN_800628FC pipes (chunk 0x14; the +0x4E7 path) | code only (no capture enters a pipe) |
| progress.rs | the paired trigger list 0x800D1C7C (state 2) and the course-12 end offset | stunt/paired: free run / teacher forced exact |
| progress.rs | FUN_80062E38 mirror planes (chunks 0x08 / 0x17; record fields) | code only (MOUNTAIN has none) |
| race.rs | race progress of race type 3 (+0x388 along the strip) | MOUNTAIN / stunt/finish: exact |
| camera.rs | modes 3 (the trial) and 9 (a stunt course's menu): FUN_8006FB2C / FUN_800702C4 (were "cut-scene sync") | mode 3 exact on all three captures; mode 9 code only |
| init.rs / session.rs | the trial's set-up: grid index + 2, one kart, countdown 0x78 + 180, the special-move roll, +0x488 = -1, +0x490 / +0x4A9 | MOUNTAIN: race load exact |
| session.rs | mode 3 goes to the results (mode 10) as mode 1; 0x800B0AF0 (`results_clock`) | stunt/finish: exact through mode 10 |

Ported in Session 7, phase 4 (1 PLAYER BATTLE, race type 2 / game mode 2;
docs/spec/battle.md; reached from the arenas' course menus):
| where | branch | evidence |
|---|---|---|
| progress.rs | lap timing for modes 2 / 7 (0x80082790): the battle clock +0x358, the knock-out's place (+0x4BE, +0x440 = 1000 + place, 0x800B0CD8 - 1) | oracle LAGOON, battle/ko, battle/win: free run / teacher forced exact |
| progress.rs | track direction for modes 2 / 7 (toward the nearest kart +0x4DC, read live) | the same captures (+0x48E exact) |
| session.rs | the battle's set-up (6 / 5 karts, grid draws mod 6, countdown 0x78 + 180, 0x800B0CD8 = karts) and the win (FUN_8007916C @0x800791C8: last kart standing, record, then mode 10) | race load exact; battle/win: the win frame and the save block (test) |
| init.rs | grid placement in a battle (mode 2, race type 2: index = grid) | race load exact |
| race.rs | the damage gauge's random bytes in mode 2 | free runs exact (random state) |
| bin/mr-course-check | `--dir`: the karts' own characters / vehicles; teacher forcing keeps the session's history rings (not captured) | the teacher-forced camera now exact on the race captures too |

Ported in Session 7, phase 6 (ADVENTURE, game mode 0xB, and the "BEAT x"
race, race flag 0x10; docs/spec/adventure.md; reached from GAME MENU ->
ADVENTURE):
| where | branch | evidence |
|---|---|---|
| init.rs | grid points of the parked karts (+0x496 0x40 / 0x20) and ADVENTURE's random point (12 + rand() / 0x1556), the TER-centre start past the grid list | oracle adventure/dock: race load exact (3 karts, random state) |
| session.rs | mode 0xB set-up (no countdown, the Doozer and the course's character parked), FUN_8007916C's ADVENTURE branch (a find puts the player out; a tag sets race flag 0x10), the results | adventure/dock, adventure/token: free run / teacher forced exact |
| items/pickups.rs | ADVENTURE boxes / tokens (counts +0x4AD / +0x4AE, taken with full slots / meter), the challenge objects (targets +0x4AC, movie, Doozer goals), the save bits | adventure/token: exact through the find; flags = the save block |
| collide.rs | the tag on contact (0x800B07FE, kart 0's event bits 8) | adventure/tag: exact through the tag |
| progress.rs | FUN_800627E0 (the rescued Doozer onto its goal); the "BEAT x" win bits (race flag 0x10) | code only |
| camera.rs | TRIANGLE look-at on the challenge objects (0x800729CC); watch targets 10..0x1D | code only |

Remaining stubs (all unreachable in the port today):
| where | branch | why unreachable |
|---|---|---|
| update.rs:107 | BIOS rand() in `kart_update` (the one-kart analysis API) | the session uses `kart_update_at` with a rand source |
| update.rs | nearest-camera search with 2+ humans (0x80064FB0) | one human |
| camera.rs | resolve_watch: target 0x7F (the chicken) | mode 0x14 (CHICKEN CHASE, two players) |
| camera.rs | special event camera (0x800B0FAC = 0x1D / 0x1E outside the title's mode 0x10) | no such event; the title is always mode 0x10 |
| camera.rs | pre-race orbit (game state 0 / 17), mode 11 (game state 0x12) | the port's modes are 1, 10, 0x10 |
| camera.rs | alternate race camera rule (0x800B0BD2 != 0) | race over / pause flag never set |
| camera.rs | camera with no karts; camera mode outside 0/1/2/10/12 | the mode selection cannot return them |
| camera.rs | trail shorter than 16 VBlanks | 24-entry ring (camera::TRAIL_LEN) covers 16 VBlanks at dt 1 |
| camera.rs | event 30 fixed angles outside modes 1 / 7 | event 30 = the title, always mode 1 |
| camera.rs | orbit (mode 1) with only the eye path (chunk 0x0B, no 0x0A): looks at the kart, the game at a stale stack value [INFER] | no course: DISCO / the title have both |
| progress.rs | finish line with race flag 2 | REVERSE RACE (COURSE SECRETS, not ported) |
| progress.rs | track direction in mode 0x14 (CHICKEN CHASE: toward the chicken); FUN_80090670 path mode (+0x4DF != 0) for a human in mode 1 | CHICKEN CHASE is two-player (phase 7); +0x4DF of a mode-1 human is race flag 2's bit |
| race.rs | race progress with race flag 1 | not used |
| render.rs | race flag 0x40: aim at the chicken 0x800CB658 (FUN_80041A20; was listed as "the boss": race flag 0x40 is CHICKEN CHASE's) | two players (phase 7) |
| init.rs | grid placement with race flags 1 / 2 / 4 (BATTLE RACE, REVERSE RACE, the reversed trial) | COURSE SECRETS (not ported) |
| items | mode 20 projectile hits | mode 0x14 |

## 2. Code not ported (no effect on the kart record the port owns)
- Sound calls (FUN_8007A620, FUN_8005EECC, FUN_8005F168): ported as events
  (mr_sim::sound, Session 4 track D; docs/spec/audio.md "SFX + engine"): the
  engine / scrape / slide / impact loops, jump, skid, landing, wall, water,
  contacts, countdown, lap, SELECT, not-ready, character voices (FUN_8007A7F4,
  with FUN_800306D0's per-frame clear of +0x4DD). Their random draws are the
  same as before. Differences: the voice timer +0x4F5 lives in
  `RaceGlobals::sound` (not the kart record); FUN_8007A620 uses each human
  kart's own camera (the game: the camera index +0x4F2, the same in a
  1-player race). Item-code sound calls: track A. The sound driver, libsnd
  and the SPU are mr-audio (driver.rs, spu.rs): not modelled there: reverb,
  noise, pitch modulation, volume sweeps (unused by the game), the SPU's
  Gaussian ROM table (a fitted Gaussian kernel instead), the libsnd lock
  0x800D1C44 (a VBlank flush skipped while the game is inside SsUtKeyOn),
  the exact VBlank phase of the game's calls (taken as 2 VBlanks after the
  frame start, as the oracle shows), the SPU state at a savestate (the
  verifier seeds libsnd's tables from RAM, voice envelopes approximately).
- Shadow polygon FUN_8003E59C + FUN_8003E3A0 (writes 0x800C8B38..), particles
  FUN_800326B4 / FUN_800870E8 (random draws kept): render side.
- The body matrix +0x18C (FUN_80025D8C / FUN_80024594) and wheel matrices
  (FUN_80086DF0): render prep after the kart loop. The oracle trace's m0..m8 come
  from there; not in mr-sim yet.
- 0x800B0A20 / 0x800B0B20 / 0x800B0F3C / 0x800B0CBE (path walk outputs) and the
  ground/wall query globals (0x800B0B0C, 0x800B0C40, ...) are locals in Rust.

## 3. Inputs taken from outside the port (the harness reads them from the oracle)
`mr_sim::harness` documents these; a Bevy game must produce them itself:
- buttons and dt per tick (dt is 3 for 15 ticks of lap1, VBlanks ~1374-1398);
- race globals (mode, race clock, countdown, flags, humans, laps, lap record);
- the race leader's progress (+0x388 of kart 0x800D8F94) for the catch-up;
- the water animation's normals 0x800B0CA0..CA4 and 0x800B0EA8..EAC
  (FUN_8003C5FC; it draws random bytes, so it must be ported with the shared
  random pointer to run without the oracle);
- kart fields other code writes: look point +0x324..+0x32C and +0x338 (render),
  race position +0x388/+0x4BE (FUN_800837F8), turbo meter +0x48A, wrong-way
  monitor +0x48C (after the kart loop: previous capture); path position
  +0x498/+0x49A (FUN_8009004C) and look-at target +0x2C0..+0x2CC
  (FUN_80041A20/80041CC4) (before the kart loop: same frame);
- the other karts' records (for the contact pass), from the previous frame.

## 4. External events injected in the lap free run (Session 2/3; gone in Session 4)
Session 4: the projectiles are the port's (FUN_80069648); `mr-items free`
on items-on captures runs the player with nothing injected (exact to the
finish on all four, docs/spec/items.md). The player-only harness below
(mr-verify, tests/oracle_lap.rs, tests/race.rs) still injects, as it does
not run the other karts.
- Tick 821 (VBlank 1650): an AI projectile hits the player (FUN_80069648 after
  the kart loop: velocity halved and pushed, spin-out, flash). Weapons are out
  of scope and the capture does not include the projectile table (0x800DDC50),
  so the free run copies the fields the hit changed from the oracle at that tick
  (vx, vy, vz, spin, flash, rumble, ...). Without it the free run is exact for
  ticks 1..821 and then diverges.

## 5. Deliberate API differences from the brief
- `tick(k, g, w, inputs, buttons)`: race globals (`RaceGlobals`, includes dt) and
  per-tick outside inputs (`TickInputs`) are explicit arguments instead of
  `dt: i32`; `tick_with_karts` adds the kart-kart contact pass.
- The kart record's face pointers (+0x2E0/+0x2E4) are RAM addresses; the port
  writes them from the face index and the heap address of the faces
  (`World::heap`, taken from the oracle RAM) so records compare byte for byte.
- `KartState::trail` owns the position-history ring (+0x2DC -> 0x800D4098)
  for the camera (camera.rs, `push_trail`).

## 6. Known imprecisions (no effect on motion)
- The random pointer 0x800B0BCC at the moment kart 0 updates depends on code
  earlier in the frame (water animation, AI karts) that the port does not
  run, so random-driven cosmetic bits (event bits +0x4DD, blink timer +0x44C,
  skid +0x4ED, contact tie-breaks) can differ from the game; in the lap this
  touched no motion field. (Function-level fixtures, which start from the same
  pointer, match it exactly.)
- Body-tilt outputs +0x458/+0x45C and pad rumble +0x4B7/+0x4EA/+0x4EB differ
  from the oracle by small amounts (the game's own code run on the captured
  state shows the same difference; the extra input was not found).
- Course data: the finish line (DOCK.DAT +0xA14), course length (+0xA48) and
  trigger list (+0x39B4) are read from the disc at the offsets the game's
  pointers give (pointer - DAT load address, taken from the oracle RAM and
  checked byte for byte). They should come from mr_assets::dat once that
  decoder exposes these records.
- Vehicle stats come from the exe table at 0x800ACDB0 (FUN_80087EB4 copies them
  to 0x800D9130 + 0x174 * vehicle); verified for vehicle 0 (Kermit's RACER).

## 7. Standalone session (session.rs)
What the oracle supplied and how the session produces it now:
| input | standalone source |
|---|---|
| initial kart record | FUN_800609F4 port (init.rs) + vehicle template (vehicle.rs) from the exe and RACER.MDL / KERMIT.MDL |
| race globals | SessionConfig (race settings of the oracle savestate: laps 3, event 3, cup 4, lap record 12186, end counter 1) and the ported clock / countdown FUN_8005C2EC (race.rs) |
| buttons, dt | caller (`tick`: dt 2; `tick_dt` for analysis) |
| path position, wobble | FUN_8009004C port, every frame (one kart) |
| water normals | FUN_8003C5FC port, state from the exe's data section |
| random pointer | reset 0x8001F94C at race load, FUN_8007D304's per-frame step and wrap |
| body matrix | FUN_80024594 port |
| leader progress | the leading kart's (race position 1) progress; alone, the player's own |
| race progress, positions | FUN_800837F8 / FUN_800834C0 port (race.rs) over the session's karts |
| other karts | `Session::others` (empty until the AI port fills it) |

Render-side fields (render.rs, ported since): the look-at target +0x2C0..
(FUN_80041A20 with FUN_80081170's mode-1 nearest-point search over DAT chunk
7; FUN_80041CC4 checked), the wheel / driver part matrices (FUN_80086DF0), the
vehicle and driver matrices, driver position, look point, bounce offset,
wheel and light positions (FUN_80025D8C). Not ported: +0x1EC (driver matrix
times the character model's own part matrix; nothing reads it for placement),
the model / character slot numbers +0x4A4 / +0x4A5 (0 in the session; the
render code only tests them against -1).
Not produced (cosmetic or other subsystems), with the consequence:
- Other karts as look-at targets: in a race with AI the contact pass aims
  the body sway at the nearest kart's look point; alone, only the course
  look points remain, so the sway differs from the oracle's once an AI kart
  comes close (lap1: tick 69).
- Race position (ported since, race.rs): with one kart the player is first,
  so the catch-up is 0 as for a leader (analysis: `race_pos_override`).
- (Ported in Session 4: items.rs.) Turbo-meter tokens FUN_8007BBF0 (+0x48A += 12 per token collected on the
  track): not ported, the meter stays 0, so R1 turbo never fires and the boost
  glow target stays 0x60.
- FUN_80081170 / FUN_8006F410 record fields +0x488 / +0x490 at init (course
  point index, zone), the model/character slot numbers +0x4A4 / +0x4A5, flag
  bit 1 of +0x496 (set elsewhere). (The countdown and the start minigame are
  ported since: the session starts at race load, countdown 480.)
- BIOS rand() seed: unknown at boot; `SessionConfig::rand_seed` drives the
  initial wheel angles (cosmetic) and the camera's random bits on a mode change.
- The water animation's state starts from the exe's initial values; in the
  game it has run since the race (or an earlier one) loaded, and the random
  pointer is shared with the AI, so wave phases differ from any oracle run.

**Catch-up consequence (the main behavioural difference):** the oracle runs
had the player 6th of 6 with the leader far ahead, so FUN_80064B5C's catch-up
(+0x4C3) sat at 180 and raised the forward speed cap from 0x40*32 = 2048 to
0x50*32 = 2560 (cap = 0x40 + 16*catchup/180, times 32). With no AI the player
leads, catch-up is 0 and the cap is 2048: top speed is 20% lower than in the
recorded lap. This is what the game does in a race the player leads.
Measured on the lap1 route (buttons replayed per tick): the standalone run
first differs at tick 43 (the speed reaches the lower cap); with the leader
forced far ahead (`mr-standalone --catchup-as-6th`) it is bit-exact through
tick 686 (dt 3 begins at 687 in the oracle), and with the oracle's dt as well
(`--oracle-dt`) bit-exact on every motion field, the body matrix and the
camera through tick 821, the AI projectile hit.

## 8. Render-side fields: verification and the random pointer
The body-sway springs (+0x3A0..), their outputs (+0x454..+0x45C) and the
driver head matrix (+0x29C) take random kicks from the shared random pointer
0x800B0BCC. The capture holds the pointer at VBlanks, not at the moment kart 0
updates (the water animation and the contact pass draw from it first), so the
harness tries pointer skips 0..12 per tick and keeps the first that reproduces
the random-driven fields (`harness::Run::teacher_forcing`, skips found on every
lap tick: 0 x 874, 1 x 517, 2 x 136, 3 x 8, 4 x 2). With that pointer:
- teacher forcing: every render range (body, vehicle, driver and wheel
  matrices, driver position, look point, bounce offset, wheel and light
  positions, aim, sway springs, weapon slots) is exact on every tick except
  172 and 177 (weapon pickups give weapon slots: code outside the port, items)
  and 821/822 (the AI hit);
- free run with those three events injected and the pointer per frame: all
  render ranges and motion fields exact on all 1537 ticks;
- standalone session with the oracle's catch-up, dt, pointer and events:
  exact until tick 69, where the oracle's look-at target becomes an AI kart.
The pointer is an input like the buttons: in a real race the AI karts and
other code draw from it too, so a standalone session's body sway (cosmetic)
cannot match a recorded race's.

## 9. Race flow (Session 3: race.rs, session.rs, progress.rs)
Ported, game order per frame (`Session::tick`):
1. FUN_8005C2EC (before the race frame): frame counter, countdown 0x800B0C70
   (480 at race load = 300 + 180, FUN_8008C4AC) or race clock 0x800B0A94
   (+5 per VBlank once the countdown is 0), one rand(); GO when the countdown
   drops below 1 (0x8005C4A8: humans' start boost -> best 0x800E02BA, every
   kart +0x4FC = 0xB, +0x444 = +0x446, +0x446 <<= 3). It steps by the
   previous frame's dt (0x800B0AEC is measured after it; oracle: countdown -2
   in a frame whose kart update ran with dt 3). The first frame after the race
   load runs with the countdown held at 480 (oracle start_probe).
2. FUN_8007916C's start: all humans finished -> mode 10 (0x8007975C,
   FUN_800606D0), and the frame goes on to the race simulation in mode 10
   (LAB_80079A58 -> 0x80079D50; until Session 7 the session skipped it).
3. The race frame as before; in the kart update the "Copy the buttons" start
   game (0 < countdown < 300, mode 1): button table exe 0x800B077C (SQUARE,
   TRIANGLE, CIRCLE, CROSS) by +0x4DA; a match adds 1 or 2 (+0x4B8 > 0x1D) to
   the start boost +0x446 and draws the next button from the random pointer
   (not the same one); a wrong button takes 1 off.
4. Lap end FUN_80082708 / 0x80082DA0: lap time rounded to a multiple of 3,
   lap record, best lap +0x384, announcement threshold 0x800B0DD0, lap++,
   banners (+0x4FC = 8 on the last lap, lap - 1 before). Finish (0x80083004)
   when lap == laps or another kart has finished (0x800B0CD8 > 1): +0x4C7 = 1,
   +0x4F9 = 0x80, place = 0x800B0CD8, flags |= 1 (AI drives), an early finish
   counts karts still racing on a later lap and re-sums the race time,
   +0x4FF = 0x10, +0x4CA = place, progress = length * lap + 1 + karts - place
   (keeps the finishing order sorted), 0x800B0CD8++. After it the kart's
   race time stops; +0x38C counts VBlanks since the finish.
5. The spin after a hit (status 0x40, 0x800668A8): heading += ramp * +0x300
   / 32 * dt, ramp rising / falling over 32 VBlanks of the 300-VBlank effect
   timer +0x468 (the timer itself is FUN_8006C388, after the kart loop: an
   input in the harness).
6. FUN_800837F8 (after the kart loop): the round-robin kart's progress along
   DAT chunk 5 (FUN_80081170 mode 0x10; not for lap 0 with +0x4C1 = 1, capped
   at length * laps; none in modes 10 / 0x11), FUN_800834C0 bubble sort
   (equal neighbours swap), race_pos = rank + 1, position flash +0x4FF = 0x1E
   and +0x4CA on a change, -dt per frame.

Verified (analysis/oracle/race3P, route tools/oracle/routes/race3.json, 3
laps, player 3rd; analysis/oracle/start_probe, slot 1 through the race load):
- teacher forcing to the finish (3510 ticks): exact except the external
  events 806, 829, 1884, 3179 (AI projectile hits / contacts), 1344 and 1493
  (start and end of the spin effect those hits cause; FUN_8006C388);
- free run with those events injected: every motion field and render range
  exact on all 3510 ticks; at the finish lap times, best lap, race time,
  place (3rd), shown place, progress and the AI hand-over bit match;
- FUN_800837F8 / FUN_800834C0 on the oracle's six karts: progress, course
  point, positions and flashes reproduced on all 3511 frames;
- standalone session from race load (start_probe, 834 frames, no buttons):
  countdown, race clock and every kart field equal except random-pointer
  driven ones (blink, sway, event bits: the AI karts share the pointer),
  AI contacts (close kart, flag bit 1) and the first copy button (drawn at
  init after the AI karts' draws; injected from the oracle);
- start game (analysis/oracle/start_game, route start_game.json: face
  buttons in turn through the minigame, 70 frames with a press): countdown,
  race clock, start-boost count, holds, engine rev, GO, best start 20 and
  every non-random kart field equal on every frame to GO, with the next
  button to copy taken from the oracle (random pointer; tests/race.rs);
- standalone session on the race3 route (`mr-standalone --capture
  analysis/oracle/race3P --oracle-dt --catchup-as-6th`): motion and camera
  exact through tick 662; at 663 an AI kart bumps the player (contact pass,
  +0x501 / +0x410): the first AI-caused divergence.
Not ported: what the AI does with the player's kart after the finish (see
the revisit list), the results screen / mode-10 timer 0x800B0AF0.

## 10. 60 Hz logic (dt = 1) — first pass
(Session 3's findings, kept as found. Session 5 made the sim dt-correct for
dt = 1 behind a flag: section 12, "60 Hz mode".)
- Position history: the game's ring is 12 entries (12 VBlanks at dt = 1),
  but the chase camera walks 16 VBlanks back, so dt = 1 hit
  `unreachable_in_scope!` in camera.rs. Fixed: the sim's ring is
  `camera::TRAIL_LEN` = 24 entries; `GAME_TRAIL_LEN` = 12 is what oracle.rs
  loads / stores and the camera tests compare (entries 12.. repeat the
  oldest when loaded from RAM). At dt >= 2 the walk ends within 9 entries,
  so dt = 2 / 3 results are bit-identical (all tests green).
- Measured (`cargo run --release -p mr-sim --example dt_compare --
  tools/oracle/routes/lap1.json 1500`: both sessions idled to the oracle
  grid, catch-up as the oracle race, then the route's buttons at dt 2 and
  dt 1 compared at equal game time): beyond the player tolerance (1169
  units / 2 deg) from tick 73; |dpos| ~2300 at tick 100, ~20000 by tick 500,
  ~400000 by 1400 (after wall contacts the runs part ways); mean 68229.
  (mr-bevy/examples/dt1.rs idles 143 ticks, which with the countdown now
  ends inside the countdown; it needs `idle`-to-race-time 1430 alignment.)
- Terms not dt-correct found so far (inference from the per-field diff of the
  two runs; no fix yet, none can change without changing dt = 2):
  - the kart-frame <-> world-frame velocity conversion (0x80067028: fwd /
    side from vx / vz and back, each `>> 12` with truncation) runs once per
    tick around the dt sub-steps: at dt 1 it rounds twice as often (first
    accelerating tick: forward speed 162 at dt 2, 161 after two dt 1 ticks;
    position 1 unit off, growing to ~100 units in 10 ticks);
  - heading and steering (yaw from +0x2D2 steer / 8 per sub-step) are applied
    inside the sub-step loop, but the velocity is rotated to the new heading
    once per tick: turning at dt 1 rotates the velocity more often (dheading
    stays 0 on straights, reaches 1-2 deg in turns);
  - per-tick random kicks to the body-sway springs (+0x3A0..) and per-tick
    random draws (engine pitch, blink): twice as many draws at dt 1, so the
    random stream and the sway differ from tick 0 (cosmetic);
  - the look-at target search (FUN_80041A20) runs per frame: aim found at a
    different frame (cosmetic, feeds the sway);
  - per-tick events: pushes +0x45E / +0x462 are cleared each tick after the
    sub-steps, the history ring gets one entry per tick (count = dt).
  Next: list the remaining per-tick constants in update.rs (timers that step
  by a constant instead of dt, easing `x += (t - x) >> n` once per tick:
  lurch, steer lag, catch-up ramp, boost glow) and measure each one's share
  of the divergence by making it per-VBlank in the dt = 1 run only.

## 11. Six-kart session (AI integrated)
`Session::new` loads the race as FUN_8008C4AC: grid draw (`ai::grid_start`),
the player's FUN_800609F4, then per AI slot the opponent draw
(`ai::pick_opponent`) and its FUN_800609F4 (`ai::init_opponent`, vehicle
template from `ai::vehicle_templates` + the vehicle model's wheels), then
FUN_8009192C over every kart (`ai::race_load_roll`). Per frame: FUN_8005C2EC
(GO now includes the AI start boost), water, contacts over every pair,
FUN_80041A20 (the human) and FUN_80041CC4 (every kart), FUN_8009004C for the
round-robin kart (none while the countdown runs), the kart loop in slot order
(AI and the finished player: `ai_buttons` -> `items_off` -> update with
player -1; humans: the pad; FUN_80086B7C for human slots), then the render
pass and race positions for every kart.

Verified:
- race load against the oracle (analysis/oracle/start_probe, with the
  oracle's seed / pointer / water state): opponents, grid, all six records'
  named fields, random pointer and rand() seed equal; then every kart's
  motion fields and buttons (item bits masked) exact for 372 frames
  through the countdown and GO to 130 race ticks; at 373 an oracle AI kart
  turbos (items on in that capture) (tests/race.rs
  six_kart_race_load_matches_oracle);
- the player's race3 route in the oracle-fed harness: unchanged (exact to
  the finish);
- AI behaviour, 18 standalone races (`examples/full_race.rs --oracle-lineup
  --seed S`, player idle, items off) vs the six items-off oracle races
  (analysis/ai/n_*), tools/oracle/ai_stats.py: lap means +0.3 / -0.5 / -0.5
  / +0.2 / +1.1 % per kart (limit 3 %), pooled +0.11 %, line median /
  p90 deltas <= 0.012 / 0.057 (limit 0.10), mean finishing positions within
  0.5 places (limit 1.5): PASS;
- the finished player driven by the AI past the finish (tests/race.rs).

## 12. 60 Hz mode (Session 5, track G)
`SessionConfig::logic_hz = 60` (mr-bevy: `logic_hz = 60`): every
`Session::tick` is one VBlank (dt = 1, 0x800B0AEC = 1), 60 per second. Two
ticks make a *pair*, one tick of the game's 30 Hz logic; `RaceGlobals::hz60`
holds the mode and the phase (`beat()` = first tick of a pair, `pair_end()` =
second; at 30 Hz both are true on every tick, so every 30 Hz code path is the
game's: proven below). Fixed point throughout; no floats.

Already dt-correct, unchanged (the game supports dt 2 and 3): every sub-step
loop (thrust, drag, grip, side / forward caps, steering, power-turn grip,
gravity, water float, wheel spin, sway springs with their per-sub-step random
kicks, bounce, pitch / roll easing, body lean, smoothed sway, special-move
path, status-0x20 pull, pickups' bounce and scale, particles, sparks,
projectiles' motion and homing); every timer counted in VBlanks (countdown,
race clock, lap / race / finish times, start hold, engine rev, catch-up,
weapon recharge — a new type at every crossing of a multiple of 8 —, spin,
respawn, blink, warn, flash, shape, boost glow, drift, lurch, holds, start
boost, meter drain, effect clocks, voice timers, item blink timers,
projectile age / sound timer, water animation phases and chases); the frame
counter 0x800B0D90 (VBlanks: animation phases are the same functions of game
time); velocities from position differences (/ dt); per-tick pushes that the
next update adds per sub-step (contact push-apart, slope push, drag status
0x2000) and the boost's bounce kick (per sub-step); the camera (reps(dt));
the position-history ring (24 entries, section 10).

Changed for dt = 1 (each only when `hz60.on`):

| term | game (per tick) | 60 Hz mode | why / evidence (examples/feel60.rs) |
|---|---|---|---|
| velocity frame (0x80067028) | converted once per tick with the heading after both sub-steps of steering: the first VBlank already moves in the second's frame | on the first tick of a pair the frame is heading + this tick's yaw (the turn the second tick repeats with the same buttons); the second tick uses its own heading | without it a power slide built 2 % more side speed (2245 vs 2199 after 0.4 s) and the DOCK hairpin cost the AI-driven kart +0.22 s per pass (200 entries from the same states); with it 2196 vs 2199, hairpin 4.724 vs 4.735 s |
| frame-conversion truncation | 2 roundings toward zero per 1/30 s | the kart -> world remainder is carried into the next world -> kart conversion (`KartState::hz60_vel`, used while +0x2F4 / +0x2FC still hold that result): 2 roundings per 1/30 s | top speed -0.22 % without, 0.00 % with |
| power-turn yaw (0x80066500) | `steer * v * dt >> 6` with the steer value as the tick starts (a tick behind the steering) | the steer value at the start of the pair on both ticks (`KartState::hz60_steer`) | with the mid-pair value a power turn turned ~25 % further per tap |
| sway input (0x80068090) | side speed / 2 added per tick | side speed / 4 per tick | a level, not a difference: per-second rate kept (cosmetic) |
| effect wobble (0x80068790) | sway[5] += 0x200, sway[10/11] += sin >> 5 / >> 6 per tick | 0x100, sin >> 6 / >> 7 | per-second rate kept (cosmetic) |
| stacked landing (0x80040378) | bounce kick -1000, added in both sub-steps of the next update | -2000 (one sub-step) | same velocity change |
| random tries per tick | FUN_8005C2EC's rand(); pointer step FUN_8007D304; splash byte after the camera; splash sound try; skid try (coast hold 2, 4, 6); random jump (effects 0x13 / 0x14); projectile hop, whine, trail; status-effect tries (particles, spins, sounds, effect 15's angle step) | once per pair (first tick); the skid window 1..6 (three tries, as 2, 4, 6) | the stream and every random event at the game's rate per second |
| per-tick random values | engine pitch byte, impact pitch byte, hover-chassis bounce kick, trigger-flight end jitter, status-effect rumble | `RaceGlobals::replay_draws`: consumed on the first tick, read without consuming on the second | rate kept, a value every tick |
| kart particles FUN_8007F03C | every tick | once per pair, with dt 2 (its spawn tries and draws per pair); particles still move every tick | rate kept |
| AI FUN_800912F8 (and the autopilot) | every tick | once per pair with dt 2 (its timers, +0x4CB), buttons held for the second tick (`Hz60::ai_hold`) | decisions, rand() draws and weapon rolls at the game's rate |
| path position FUN_8009004C | one kart per tick, round robin | one kart per pair; the round robin advances at the pair's end | each kart refreshed at the game's interval |
| pickups FUN_8007BBF0 (collect) | positions tested every tick | tested on the first tick of a pair (the game's 30 Hz sampling); objects still animate every tick | tested every tick: +8 % tokens, +7 % boxes / fires (beyond 2 sigma) |
| load frame | the first frame holds the countdown | the first pair | countdown, GO, race clock on the same game time (tests/hz60.rs) |

Looked at, left as the game's: the wall-top allowance `[0x800A88AA][dt + 5]`
(96 at dt 1, 288 at dt 2: the drop under gravity in one tick) — the dt 2
value at 60 Hz changed nothing measurable; collision tests (walls, kart pairs,
projectile vs kart) run every tick (finer sampling: a hit can come a VBlank
earlier); race positions every tick.

Proof that 30 Hz is unchanged: `cargo test --workspace` (every oracle test
unchanged; 119 green); `mr-verify` on lap1P3 and race3P: output and traces
byte-identical before / after (the pre-Session-5 build from a worktree);
`examples/hz30_fingerprint.rs`: whole-state hashes every 500 ticks of 7 long
sessions (full races with items, idle and AI players, recorded routes)
identical before / after; mr-bevy oracle-grid lap1 route: trace identical to
the Session 2 trace and exact against the oracle to tick 686 (the original's
dt = 3 frame), with `low_latency` on and off.

Feel tolerance (60 Hz vs the faithful 30 Hz port; `cargo run --release -p
mr-sim --example feel60`: scripted drives, 80 AI-driven player races, 1 s
recorded-input windows, 100 six-kart races per mode, ~30 s CPU, no oracle):
39 of 39 rows within limits — accel / top speed / 3 s distance within 0.5 %,
jump height and airtime equal, length +0.9 %, full-lock yaw rate equal,
radius -0.5 %; player (AI-driven) race time -0.03 %, laps within 1 %, line
median 0.019 track widths, lateral quantiles within 0.012; recorded inputs
median 494 units / 0.18 deg after 1 s; AI lap means per kart within 0.9 %
(limit 3 %), pooled +0.42 % (limit 1 %); item use (fires, boxes, tokens,
turbo, special moves, hits) within 2 * sqrt(sd_a^2/n + sd_b^2/n). Full table:
docs/notes/2026-10-13.md, "Session 5, G 60 Hz".
Open: with items the AI leans slower at 60 Hz (+0.4 to +0.9 % pooled over
runs; items off +0.2 %), cause not isolated (every item rate in tolerance).

## Session 6 (track P): the other race courses
Course table, objects and checks: docs/spec/courses.md. Ported from the code
(each with addresses there): the course table (course.rs), jets, turntables,
GRAVE-style hooks, lifts moving TER groups, breakable scenery, weather, race
flag 8 damage / knock-out (DISCO), camera modes 12 (overhead) and 1 (orbit),
the HUD's random draws.
Changes that also touch DOCK (all DOCK oracle tests unchanged; the 30 Hz
fingerprint was identical after the generalisation commit ea1fa61 / 9f0c750
and changes with these, by design):
- Start-game HUD (FUN_8004E8E8): 7 random-pointer bytes per human per frame
  while 0 < countdown < 300, twice when the camera's cut flag makes
  FUN_80078F38 render the frame again. Explains the countdown-time pointer
  drift Session 3 listed as "random-pointer driven fields" (start_probe).
- The load frame: no FUN_8005C2EC rand() before the kart loop; the intro
  camera's change draws its rand() at its place (the Session 3 "peek" is
  gone: `camera_peek` stays false). Equal streams on DOCK; ROCK / OVAL (first
  AI kart's weapon roll) and SPEEDWAY / RALLY (overhead camera: no draw)
  decide it.
- Race load +0x488: FUN_80081170 mode 1 (whole-list nearest), not mode 0x10.
- `drv_prev_x/y/z` (+0x334/+0x33C/+0x344) are named KartState fields (the
  jets read them); two DOCK tests list them with the other driver fields.
Known differences (not one-for-one):
- Course-load rand() draws (token colours, turntable phase / spin, the
  FUN_80084490 switch's render phases) come from the seeds before the
  session's race-load seed unless `SessionConfig::course_load_seed` names
  the game's (track S: the course load runs when the course menu opens,
  menu frames earlier; mr-course-check finds the seed per capture).
- Lifts and breakables change the TER in place; the session keeps the TER in
  step with its state (`Session::sync_ter`, also after `restore` /
  `load_mem`). A projectile's breakable hit is applied to the TER after the
  frame's projectile step (the game: at once); the breaking projectile
  itself skips its ground step as in the game (track S), so only another
  projectile in the same frame could tell.
- Breakable floor hits (FUN_8003C4D4 paths of FUN_80069648): ported by
  track S (below).
- Hooks: FUN_80061BC4 scans all 24 records; the port scans the course's.
  No race course has hooks.
- Weather: the drops' render-side colours / sizes are kept for drawing; the
  shadow (FUN_800861F0) of hooks is not.
- DISCO: one teacher-forced frame (387, kart 5) hit a wall the port
  missed: the lift was not where the checker had it (resolved, track S).
- Free runs (mr-course-check) part from the oracle 90..850 frames after GO:
  first in hidden state (particle / spark lists, then the random pointer by
  1-2 bytes), as on DOCK (456).
- mr-bevy: moving scenery (lift / turntable / animated DAT objects,
  FUN_800859EC's render part), the per-course VRAM scrolls of FUN_8007A9DC
  (FUN_8005E38C cases), falling weather and knocked-down scenery are not
  drawn yet (render-ledger).
- AI stuck spots: a rare AI kart jumps in place against a wall (NEW_YORK,
  HAPPY, seed dependent). Not a difference: the game does the same at the
  same spots [ORACLE] (track S below).

## Session 6 (track S): course simulation gaps
### AI stuck spots (NEW_YORK, HAPPY) [ORACLE][CODE]
`mr-survey --seed N` (hex) over 0x10..0x260 found the stuck episodes
(`MR_STUCK=1` prints where, `MR_TRACEK=slot:t0-t1` traces a kart):
- NEW_YORK A, TER (-251, -149, -4715), seg 35/6 (seeds 0x66 k3, 0x160 k5,
  30 Hz): an AI kart that leaves seg 68 wide at the 76 -> 68 corner is put on
  seg 35 by FUN_8009004C (nearer), drives along the raised surface-14 ledge
  south of seg 35 into a corner; position held by the walls, it jumps at
  random (speed < 0x180) and never turns away. Stuck to the end of the race.
- NEW_YORK B, TER (-6622, -150, -4459), seg 76/1 (seed 0x129 k4, 0x97 k2):
  same ledge after the seg 79 jump; escapes after a while by a random jump.
- HAPPY C, TER (3865, 0, -7685), seg 17/7 (seed 0x169 k1, 60 Hz): wedged
  below the path's step (aim 1250 units up), jumping.
Oracle: each kart poked onto its spot at the grid restore (A and B: slot 9,
C: slot 10; tools/oracle/capture_poked.py; analysis/oracle/stuck/NEW_YORK_AB,
HAPPY_C): the game's kart stays at A and C for the whole 1400 VB (~33 s
after GO) and leaves B after ~13 s, as in the port. `mr-course-check --dir
analysis/oracle/stuck/NEW_YORK_AB NEW_YORK` (and HAPPY_C HAPPY): the port
from the same memory is exact, free run and teacher forced, every kart,
every frame. The code has no recovery for an AI kart: the only ways off a
spot are the AI's random jump (FUN_80090914) and a full-meter special move
(FUN_800913F4); +0x4C7 hand-over and the 10-minute limit concern humans.
So the port keeps the game's behaviour (no recovery added).
### Free runs to the end of the captures [ORACLE][CODE]
`MR_HIDDEN=1 mr-course-check` now prints the first differing particle,
spark, projectile and jet field by field. What it found, in order:
- Particles flag 0x200 is the drawer's on-screen bit (FUN_80010E90 clears
  it off screen); not simulation state, the comparison masks it.
- The projectile ring slot 0x800B07E0 (FUN_80068C6C's round robin) is not
  reset by the race load: the first weapon of a race takes the slot after
  the menus' last one. The checker sets it from RAM with the other carried
  state (session_at_load). A fresh Session starts it at 0 (no menus).
- A projectile that breaks scenery (FUN_80069334 returns 1, wall path
  0x8006A9xx -> 0x80069EE0): the game skips its ground step and the kart
  hits, and draws one random byte: the owner's +0x4DD |= 8 / 4 / 2 (below
  0x55 / 0xAA / else). The port did the ground step (snapping the spent
  projectile onto the breakable's floor) and drew no byte. Ported
  (weapons.rs `broke`); GRAVE, LONDON, GALLERY.
Result: BASE, BOG, CAPE, GALLERY, GHOST, GLACIER, GRAVE, HAPPY, ISLAND,
LONDON, NEW_YORK, OVAL, PARK, RALLY, ROCK, SPEEDWAY and DOCK (start_probe)
exact, every kart, to the end of the capture (1603-1635 frames; DOCK
834). DISCO parted at 387 (k5's wall contact; resolved below: all 18
exact).
### Breakable floor / ceiling hits [CODE][ORACLE]
FUN_80069648's ground step: a projectile landing on a floor face (not on
the floor the frame before) or pushed under a FACE_ALT face (not under it
before) of a breakable TER group (FUN_8003C4D4(face) at 0x8006A4xx for the
nearest face 0x800B0B0C, 0x8006A278 for the ALT face 0x800B0C84) breaks it
through FUN_80069334 after its y is snapped; the step then ends as for a
wall break (owner +0x4DD byte; ground_y / face / on-floor flags not
updated, no kart hits). weapons.rs. Oracle: a type-0 projectile poked
above LONDON's group-0 crate top at the grid restore (slot 4;
analysis/oracle/stuck/LONDON_floorbreak): breaks at VB 8 (spent, grounded,
y = floor - 0x1000, kart 1's +0x4DD set); `mr-course-check --dir` free run
and teacher forced exact (projectile, random pointer, karts) to frame 303,
where a new AI projectile's launch y differs (the --dir session's AI
look_* fields, +0x324.., are not rebuilt by load_mem [INFER]; not a
course capture issue). The ceiling path is from the code only.
### Course-load draws [CODE][ORACLE]
The course (TER, DAT, tokens, turntables) is loaded when the course menu
opens: O's ram0 (the course menu) already holds the course, the token
colours and the turntable spins; the menu frames after it keep drawing
rand() (1-2 per VBlank) and the race load comes later. The course load's
rand() draws, in order: FUN_8007B448 one per turbo token (colour = rand()
>> 13), FUN_80084490 two per turntable (phase = rand() / 8, spin = rand()
< 0x4000 ? 5 : -5, x3 on 0x1A / 0x1B), then its per-course switch.
`mr-course-check --course-load` rewinds the BIOS rand() from each
capture's menu state and finds the one run of those draws that gives all
of the menu state's token colours and turntable spins (38..118 tokens: 76+
bits each; a unique hit within 4 M calls on all 17 courses): 120..286
calls before the menu state (BASE 156, BOG 274, CAPE 252, DISCO 278,
GALLERY 156, GHOST 153, GLACIER 263, GRAVE 217, HAPPY 175, ISLAND 161,
LONDON 286, NEW_YORK 231, OVAL 141, PARK 186, RALLY 151, ROCK 146,
SPEEDWAY 120). The session draws them itself from
`SessionConfig::course_load_seed` (None: just before the race-load seed, as
before); mr-course-check sets it from that search and no longer copies the
colours or spins from RAM (MR_CLRAM=1: the old copy). Still from RAM, as
state that runs on through the menus: the turntable phases (render side)
and the pickups' grow / bob animation (FUN_8007BBF0: scale, scale rate, vy;
a fresh course load starts them at 0, which SPEEDWAY's free run showed
matters). All 18 free runs unchanged with the model (17 exact to the end).
### DISCO frame 387: the lifts run on through the course menu [ORACLE]
Kart 5 crossed the edge of lift 0's platform (walls 248 / 259, TER group 0)
at frame 387; the game counted the wall, the port did not (the kart 82 TER
units above the walls' top as the port had them). The course captures do
not hold the lift records (0x800DD708) or the TER, so the checker took the
lifts from ram0 (the course menu). A capture from DISCO's course menu with
the lift region (tools/oracle/capture_menu_lifts.py; analysis/oracle/stuck/DISCO_lifts, scratch slot 11) shows
the menu running FUN_800859EC: lift 0's wait timer drops by 2 every 2
VBlanks (the menu's frame counter 0x800B0D90 steps by dt 2 too) and the
lift starts down just before the race load. session_at_load now steps the
lifts once per menu frame (each change of 0x800B0D90, dt 0x800B0AEC) from
the first captured VBlank to the race load; teacher forcing keeps the
session's own lift and breakable-group states across each reload. DISCO:
free run and teacher forced exact to the end (1635 frames); the other lift
courses (BOG, ISLAND, HAPPY, ROCK) unchanged (exact). Not a port change:
the session's lifts were right, the checker's starting state was not.


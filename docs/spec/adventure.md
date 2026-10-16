# ADVENTURE, the "BEAT x" race and the course secrets (Session 7, phase 6)

Sources: [CODE] generated/SLUS_012.37_full_*.c, analysis/ghidra/decomp
(addresses below); [RAM] / [SHOT] / [GPU] the oracle (psxrecomp debug build,
TCP 4370), captures under analysis/oracle/adventure (local, gitignored);
[INFER] marked. Port: mr-sim session.rs / init.rs / items/pickups.rs /
collide.rs / progress.rs / camera.rs / globals.rs (`RaceGlobals::adv`),
mr-assets hud.rs / hud_items.rs (`AdventureHud`, `adventure_hud`), mr-bevy
menus.rs / courses.rs / main.rs / save.rs.

## What ADVENTURE is [CODE][SHOT]
Not a story mode: a per-course solo run against the clock. GAME MENU (course
menu row 3) -> ADVENTURE (row 0, 0x80077958: 0x800B07E6 = 1, the character
select 0x800B07CB = 1) -> CROSS -> FUN_80073E44 case 6: game mode 0xB on the
loaded course [SHOT analysis/s7/adventure/walk: gamemenu, charsel, a500].
The player drives alone (no countdown) with the course's pickups and its
challenge objects (DAT chunk 0x0C) out, two parked karts, and a time limit;
the first secret found ends the run (results), or the clock does. What a run
can find, each once per course (the course's save flags,
0x800E0196 + 8 * slot + 0x20 * world):

| secret | how | 0x800B0803 | flag | result text |
|---|---|---|---|---|
| every item box | the player's boxes (+0x4AD) reach 0x800B0B60 | 2 | 0x008 | STARS COLLECTED |
| every turbo token | the player's tokens (+0x4AE) reach 0x800B0A52 | 4 | 0x010 | FOOD COLLECTED |
| the ten targets | challenge objects 0..9 (+0x4AC reaches 0x800B0EBC) | 1 | 0x004 | TOKENS COLLECTED |
| the movie object | challenge object 0x800B0EBC + 0x800B0B98 | 0x10 | 0x080 | MOVIE COLLECTED |
| the Doozer rescued | the parked Doozer (flag 0x40) pushed onto its goal (challenge objects 10..) | 8 | 0x020 | DOOZER RESCUED |
| the course's character | touching the parked kart with flag 0x20: the "BEAT x" race (below) | - | 0x040 (won) | - |
| (time up) | race clock >= 0x800ACF30[race type] | 0 | - | OUT OF TIME |

Time limits 0x800ACF30 [RAM]: 72000 (4:00) on races A / B, 54000 (3:00) in
an arena, 180000 (10:00) on a stunt course. The HUD names follow the game's
strings: boxes = stars, turbo tokens = food, the ten targets = tokens.
COURSE SECRETS (below) offers what the secrets open.

## Set-up (FUN_8008C4AC, FUN_800609F4) [CODE][RAM]
- Game mode 0xB, 0x800B07E6 = 1; 0x800B0DFE = 1 (the player), then the
  parked karts; no countdown (0x800B0C70 = 0, 0x800B07F8 = 0x78); the race
  clock runs from 0; laps 0x800E038B[humans] (unused: no lap timing,
  FUN_80082708 / FUN_80081DD8 return at once); no ranking (FUN_800837F8).
- The grid draw *rng++ % 1 is still taken; the player's grid point is
  12 + rand() / 0x1556 (arenas 0 +, stunt courses 6 +; FUN_800609F4
  @0x80060B60); past the course's list: the TER bounds' centre, heading 0.
- @0x8008E8A4: worlds 0..5 while the Doozer is not rescued (flag 0x20
  clear): the Doozer (character 24) on 0x800B0A14[0x800B0A10] (6, 25, 27 in
  turn, the turn stepped per placement), flags 0x41, grid point 18 (arenas
  6, stunt 12). Then the course's character (0x800ACB3C + 0xC * course, -1
  none; DOCK LINK 11, ...) while not collected (0x800E02BC): its own vehicle
  (0x800AC22F + 0x14 * c; 0xFF: a random selectable one), flags 0x21, grid
  point 19. Both are made with 0x800B0DFE still counting the karts before
  them: no history ring. Flag 0x40 / 0x20 park them (+0x496 & 0x60: no AI
  buttons, no pickups).
- The challenge objects (0x800E1B70 slots 0x30..): state 1, the found ones
  2 (targets with flag 4, goals with 0x20, the movie with 0x80); in a race
  all 2. Taken objects / boxes / tokens do not come back in ADVENTURE.
- FUN_8009192C(kart 0, karts): the special-move roll as a race's.
- Oracle: analysis/oracle/adventure/dock (DOCK from slot 1's course menu,
  tools/oracle/routes/adventure_dock.json): three karts, Kermit on point
  12 + rand, Doozer (24, vehicle 6, 0x41), LINK (11, 0x21).

## The run [CODE]
- FUN_8007BBF0 (pickups, every kart): item boxes: the player takes one with
  both weapon slots full too; counted in +0x4AD while the player is in and
  flag 8 clear. Turbo tokens: taken by the player at a full meter too;
  counted in +0x4AE (flag 0x10). Challenge objects (radius +0x176 + 0x80):
  targets and the movie only by the player while in (a target draws a
  random byte for a cheer: event bits 2 / 4 and 8); the Doozer goals: the
  kart with flag 0x40 on a goal: rescued (event bit 1, +0x4EE = the goal,
  0x800B0803 = 8, flag 0x20); any other kart there makes the player's kart
  flash (+0x4FC = 0xC). Each find: sound 0x24 (FUN_8005EECC(0x24, 5000)).
- FUN_800627E0 (the rescued Doozer, +0x4EE != -1): eases onto the goal
  point 1/16 a VBlank, velocity 0, +0x442 down by 0x40 a VBlank.
- FUN_80040018 / FUN_80040428 (contacts): while the player is in, a contact
  with the flag-0x20 kart tags its character (0x800B07FE); while tagged
  every contact sets kart 0's event bits to 8; on a stunt course also flag
  0x40 and sound 0x24.
- FUN_8007916C: a find (0x800B0803 != 0) puts the player out (+0x4C7 = 1,
  +0x4F9 = 0x80, AI); the results (mode 10) once every human is out. A tag
  instead: stunt course: the character collected (0x800B07FF = 1) and the
  menus (0x800B0AF4 = 0x1E); else race flag 0x10, 0x800B0F10 = 1 (2 in an
  arena), a fade (FUN_8007D174) and the "BEAT x" race (the run goes on
  under the fade: oracle 35 VBlanks).
- Time limit (0x80067EC0): every kart in at clock >= the limit: out
  (+0x4C7 = 2), the results.
- TRIANGLE (FUN_800728C0): the nearest challenge object still out (targets,
  movie; the goals while the Doozer is in the race), compared at the kart's
  >> 10 and the object's >> 5; the camera looks at it (watch target 10 + i,
  FUN_80070418: the object << 5).
- Camera (0x800706E0): the finish rule with 0x800B07E6 / 0x800B0803
  (`finish_gate_a/b`); mode 0xB keeps the race camera.

## The "BEAT x" race (race flag 0x10) [CODE][RAM]
FUN_8008C4AC: 0x800B0DFE = 2, 3 laps, the grid drawn (*rng % 2), kart 1 =
0x800B07FE on its own vehicle (FUN_800609F4 flags 1); game mode 1 (2 in an
arena: a two-kart battle). Intro: "BEAT <kart 1>" (0x800B031C, (0x100,
0x1E), large, colour 7) over RACE n LAPS. Won by player 1 (place 1 at the
finish, 0x800830xx): the character collected (0x800E02BC |= 1 << c), the
course's flag 0x40 (its boss beaten), sound 0x24; an arena's battle won
(0x8007926C): the character. Results (FUN_8004CEA0): "YOU BEAT x" / "x BEAT
YOU" (medium, colour 6, (0x100, 0x1E), blinking with 0x800B0AF0).
Quirk: the catch-up's leader 0x800D8F94 is only written once the clock runs
(FUN_800837F8): in the countdown it can name a stale slot of the previous
session (oracle: 3, a kart record left from the earlier DOCK race), whose
progress drives the player's catch-up. The port's session uses the leading
kart; mr-course-check feeds the stale progress (`stale_leader`).

## HUD (FUN_8004F688 case 0xB, FUN_8004F1C8, FUN_80050B2C, 0x800501E4) [CODE][GPU]
Racing: the +0x4F9 icon, SPECIAL MOVE / arrow, the weapon slots (y 0x2D),
the turbo meter, then FUN_8004F1C8 bottom left, each row while its secret
is not found: boxes left (colour 7, (0x30, 0xD2), the star 0xC0,0x80 0x37
slot 0x15 clut 0xB above), turbo tokens left (colour 6, the last token's
sprite 0x800AC1B8 + 8 * colour turning with 0x800B0B90), targets left
(colour 5, the course's icon 0x800AC9C0 + 6 * course pulsing with the frame
counter % 0x21); each next row 0x2D up. The minimap (its blinking target
marker FUN_80048A0C / FUN_80047F1C: not ported), the time left
(0x800ACF30[type] - clock, (0x100, 0x1E); stunt (0x1C0, 0xD2)). Out / mode
10: the ending text (large, (0x100, 0x1E), colour 5; OUT OF TIME 7), PRESS
START. GP0 (tools/oracle/hudx_check.py, adventure/fx_token VB 60 racing,
VB 300 FOOD COLLECTED): 88 / 88 at lag 0 / 2.

## Checks (oracle, Session 7)
- `mr-course-check --adv-load analysis/oracle/adventure/dock DOCK`: the
  race load exact (three karts, every named field, the random state).
- `mr-course-check --dir analysis/oracle/adventure/dock DOCK`: free run and
  teacher forced exact over 1035 frames (three karts, camera, random
  state; the player drives into a wall, one token).
- adventure/token (DOCK_adventure state, 0x800B0A52 poked to 1): the first
  token ends the run: free run / teacher forced exact over 512 frames
  through the results; 0x800B0803 = 4, flags 0x101 -> 0x111 = the save
  block (test adventure_token_secret_matches_oracle).
- adventure/tag (LINK poked onto the player's path): the tag at VB 74,
  race flag 0x10 the next frame, the fade, the BEAT LINK race: the ADVENTURE
  part exact (free run, teacher forced, 53 frames incl. +0x4DD = 8 and
  0x800B07FE); the race (`--from 112`): teacher forced exact but for two
  frames where the samples split one slow frame (VB 295/296); free run to
  frame 90 (that split).
- tests/adventure.rs: every course (18 + 6 + 10) runs an ADVENTURE to the
  time limit; the two oracle checks above. mr-fuzz --adventure (420 runs,
  6.8 M ticks), --beat (280, 5.8 M): 0 panics.

## COURSE SECRETS (not ported; the menus show it greyed) [CODE]
Course menu row 2 (0x80076D64 sub-state 8 on race / stunt courses,
0x80077094 sub-state 9 on arenas; 0x800B07F0.. players per row, LEFT /
RIGHT). Rows and what they start:
- sub-state 8: REVERSE RACE (race flag 2; DISCO 10), BATTLE RACE (flag 1:
  a battle on the race course), the damage race (flag 8; on DISCO CHICKEN
  CHASE, flag 0x40), PLAY MOVIE (flag 0x80 found: FMV 0x800ACB3D[course]).
- sub-state 9 (arenas): BOSS CHALLENGE (flags 0x18 found, i.e. the arena's
  ADVENTURE boxes and tokens: 0x800B07E8 = 1, game mode 7: the boss battle
  "BEAT SWEETUMS / BEAUREGAARD / BILL, GILL AND JILL / MARLEY AND MARLEY /
  THE CHIEF / VAN NEUTER" (FUN_80050F64 by course 5 / 0xD / 0xE / 0x13 /
  0x14 / 0x15), bosses 0x800B0C7E.. characters 0x20..0x28 on vehicles
  0x13..0x18 (FUN_8008A9AC), 0x800B0C98 extra karts, a 90000 clock counting
  down; the win sets flag 0x100); BATTLE AS BOSS (flag 0x100: race flag 0x20,
  the player as boss 0x800B07F3 of 0x800B0A61); on ARENA/BATTLE (0x1B)
  DOGFIGHT (flag 8: race flag 0x80) and DOOZER BATTLE (flag 0x10: race flag
  0x100, Doozers); CHICKEN CHASE (race flag 0x40, two players); PLAY MOVIE.
- Race flag 0x40 is CHICKEN CHASE's (the aim at the chicken 0x800CB658,
  FUN_80041A20), not a boss race.

## Not ported (listed)
- COURSE SECRETS and everything it starts (above): the boss battles (game
  mode 7: boss models / vehicles 0x13.., boss AI, the boss camera
  0x800B0DCC, BEAT x texts, flag 0x100), BATTLE AS BOSS, DOGFIGHT, DOOZER
  BATTLE, REVERSE RACE, BATTLE RACE, PLAY MOVIE (phase 9), CHICKEN CHASE
  (phase 7).
- ADVENTURE's minimap target marker and the progress-bar marker
  (FUN_80048A0C / FUN_80047F1C with 0x800B07E6), the Doozer goal marker
  (FUN_8007E21C @0x8007E908, FUN_800362D4). The challenge objects' models
  are drawn (mr-bevy movers.rs: special slots 0x20.. placed on their points
  by FUN_8002DF88, turned by 0x800B0CB0.. and scaled by +0x16 as
  FUN_8002DC14; [SHOT analysis/s7/adventure/port/look_target.png]).
- GAME STATUS (FUN_8004A878), the secret screen 0x1D, BATTLE LEAGUE
  (FUN_80082450): unchanged (frontend.md, battle.md).

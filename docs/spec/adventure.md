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
COURSE SECRETS (below; ported in phase 6b) offers what the secrets open.

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
marker FUN_80048A0C / FUN_80047F1C: phase 6b, below), the time left
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

## COURSE SECRETS (Session 7, phase 6b) [CODE][RAM][GPU]
Port: mr-sim secrets.rs (bosses, boss count, the Doozers' vehicles),
session.rs (`SessionConfig::boss_challenge / boss_pick / doozer_turn /
league`), init.rs, progress.rs, race.rs, camera.rs; mr-bevy menus.rs
(`Page::Secrets`, `Front::secret_rows`), courses.rs (`secret_config`),
hud.rs, save.rs; mr-assets hud_items.rs (`boss_hud`, `small_gauge`).

The page: course menu row 2 "<COURSE> SECRETS" (FUN_80074A94 sub-state 8
on race courses, 0xA on stunt courses, 9 in the arenas; FUN_80051698 cases
8 / 0xA / 9 draw it): four rows, each the mode's text once its secret is
found, "?" before (the course's save flags, 0x800E0196 + 8 * slot + 0x20 *
world). Two-player choices ("- n PLAYERS", LEFT / RIGHT, 0x800B07F0..)
stay at 1 PLAYER in the port. Rows and what CROSS starts (0x800B0F10 =
the game mode FUN_80073E44 then loads):

| course | row | needs | text | starts |
|---|---|---|---|---|
| race | 0 | 8 (every box) | REVERSE RACE - 1 PLAYER (DISCO: REVERSE BATTLE RACE) | race flag 2 (DISCO 10), mode 1, character select |
| race | 1 | 0x10 (every token) | BATTLE - 1 PLAYER | race flag 1: game mode 2 on the race course (a battle: 6 karts, 5 above cup 6) |
| race | 2 | 4 (the targets) | BATTLE RACE - 1 PLAYER (DISCO: CHICKEN CHASE) | race flag 8: the damage race (as DISCO's own race); DISCO: flag 0x40, two players (phase 7, greyed) |
| stunt | 0 | 8 | REVERSE - 1 PLAYER | race flag 4: the reversed trial (mode 3) |
| stunt | 1 | 0x10 | BATTLE | flag 1 with 0x800B07CB = 2: two players (greyed) |
| stunt | 2 | 4 | CHICKEN CHASE | two players (greyed) |
| arena | 0 | 0x18 | BOSS CHALLENGE | 0x800B07E8 = 1: game mode 7 (below) |
| arena | 1 | 0x100 (a boss beaten) | BATTLE AS BOSS - name (0x800AB0DC[3 * world + 0x800B07F3], LEFT / RIGHT over 0x800B0A61 bosses) | race flag 0x20, mode 2 at once (no character select) |
| ARENA/BATTLE | 0 | 8 | DOGFIGHT - 1 PLAYER | race flag 0x80: every kart on vehicle 8 (the PLANE), mode 2 |
| ARENA/BATTLE | 1 | 0x10 | DOOZER BATTLE - 1 PLAYER | race flag 0x100: every kart a Doozer, mode 2 at once |
| arena | 2 | 4 | CHICKEN CHASE | two players (greyed) |
| all | 3 | 0x80 (the movie object) | PLAY MOVIE | the course's FMV 0x800ACB3D[course] (phase 9; greyed) |

Note (correction): race flag 1 is the menu's "BATTLE" (the race course as
an arena, game mode 2) and flag 8 its "BATTLE RACE" (the damage race);
phase 6's notes called flag 1 "BATTLE RACE".

Set-ups (FUN_8008C4AC, FUN_800609F4) [CODE][RAM]:
- Grid (FUN_800609F4 @0x80060B3C): race flag 1 index + 0xC (+ 6 on a stunt
  course), flag 2 + 6, flag 4 + 3 (after the single stunt kart's + 2).
- REVERSE RACE: +0x4DF = 1 (race flag 2's bit in mode 1): the AI walks the
  path backward (FUN_80090670), the track direction too (FUN_80086B7C: n +
  1 points back with 0x800B0A20); +0x4C0 = 7; the finish line counts 6
  after 7 as a lap (FUN_80082708); no lap record, no won bit; the race
  progress subtracts the course-point delta; the results' best lap does not
  flash.
- BATTLE (flag 1): game mode 2 on a race / stunt course: battle AI
  (FUN_80090FAC; on a stunt course the race-type-3 AI), damage, knock-outs,
  the ranking by damage (FUN_800834C0(1)), the battle HUD.
- BOSS CHALLENGE (game mode 7; FUN_8008A9AC at the course load; FUN_8008C4AC
  @0x8008D8xx): 0x800B0DFE = 1 (the player), 0x800B0C98 bosses (one per bit
  0x80 / 0x100 / 0x200 of 0x800ACB3A + 0xC * course): CANYON SWEETUMS (0x20)
  on MAYHEM (0x13), T_SQUARE BEAUREGAARD (0x21) / TAXI (0x14), THEATRE BILL,
  GILL, JILL (0x22..0x24) / SAFEARSE (0x18), CHAMBER STATLER, WALDORF
  (0x25, 0x26) / CLASSIC (0x15), LAGOON THE CHIEF (0x27) / LOG (0x16), LAB
  VAN NEUTER (0x28) / DELOREAN (0x17); grid draw *rng % 6, the bosses on
  the following points (FUN_800609F4 flags 1, no history ring: their slot
  is past 0x800B0DFE); FUN_8009192C(all) with the bosses rolling "no
  preference"; then 0x800B0DCC = *rng % bosses (the boss the intro camera
  shows: FUN_80070418 in mode 10 takes kart 0x800B0DFE + 0x800B0DCC).
  Everywhere the game compares a slot with 0x800B0DFE the bosses count
  apart: no damage between two bosses (FUN_80040018 / FUN_80069648), their
  pickups do not fill the meter (FUN_8007BBF0), the AI's +0x4CB threshold,
  the position flash. The clock: the race clock 0x800B0A94, 90000 minus it
  shown; a kart's +0x358 above 89999 is out. Won (the last kart standing,
  FUN_8007916C @0x800793E0): the course's flag 0x100 (BATTLE AS BOSS opens)
  and sound 0x24; no record, no won bit.
- BATTLE AS BOSS (0x20): kart 0 = 0x800B0C7E[0x800B07F3] on 0x800B0D2C[..]
  (flags 0), the five opponents drawn as a battle's.
- DOGFIGHT (0x80): the player's vehicle and every opponent's 8; the
  opponents' characters drawn (no vehicle draw).
- DOOZER BATTLE (0x100): every kart character 24 (no draws) on
  0x800B0A14[0x800B0A10] (BULLDOZE, DUMPER, ROLLER), the turn stepped per
  kart (six karts: back where it was).
- HUD (FUN_8004F688 case 7, FUN_80050F64 case 6, FUN_80050B2C,
  FUN_8004DAE0 with 0x800B07E8): the battle's racing parts, the player's
  gauge at (0x30, 0xEF - 0x1E), each boss's small gauge FUN_80049FD8(x,
  0xD2, boss, 0) along the bottom (0x30 apart, centred on 0x100; "_" once
  knocked out), the minimap, the clock counting down; intro "BEAT SWEETUMS"
  / "BEAT BEAUREGAARD" / "BEAT BILL, GILL AND JILL" / "BEAT MARLEY AND
  MARLEY" / "BEAT THE CHIEF" / "BEAT VAN NEUTER" (large, colour 5, no
  faces); the result WINNER (colour 5) / LOSER / OUT OF TIME (colour 7),
  large at (0x100, 0x1E), PRESS START. DOOZER BATTLE's intro "DOOZER
  BATTLE" instead of "BATTLE THE MUPPETS".

Checks (oracle, Session 7 phase 6b; captures local under
analysis/oracle/secrets, routes tools/oracle/routes/*.json):
- boss_lagoon (LAGOON_menu.pst, flags poked 0x41 -> 0x59, LAGOON SECRETS ->
  BOSS CHALLENGE, Kermit / RACER, CROSS held): `mr-course-check LAGOON --cap
  analysis/oracle/secrets/boss_lagoon`: race load exact (2 karts, every
  named field, random state), free run and teacher forced exact over 619
  frames incl. the boss intro camera. (mr-course-check: mode-7 loads take
  the random-pointer frame ends; the load frame's dt is the next sample's,
  the sampled clock step uses the frame's own dt.)
- boss_win (THE CHIEF's +0x440 held at 1000 by an every-VBlank poke): the
  knock-out at GO, the win the next frame, mode 10, flags 0x59 -> 0x159 =
  the game's save block (test secrets::boss_win_matches_oracle).
- reverse_dock (slot 1, DOCK flags 0x101 -> 0x109): `mr-course-check DOCK
  --cap analysis/oracle/secrets/reverse_dock`: race load exact (6 karts),
  free run and teacher forced exact over 726 frames.
- doozer_battle (BATTLE_menu.pst from `goto_course.py menu BATTLE`, flags
  0x61 -> 0x79, DOOZER BATTLE): `mr-course-check BATTLE --cap ...`: race
  load exact (six Doozers, vehicles 6 / 25 / 27 in turn), free run and
  teacher forced exact over 636 frames.
- GP0 (fx_capture.py, hudx_check.py): fx_boss (intro, racing with the boss
  gauge and the 90000 clock) 129 / 130 at lag 0 (one timer digit at a
  sample boundary); fx_boss_win (WINNER, PRESS START) 44 / 44.
- tests/secrets.rs: BOSS CHALLENGE on the six arenas (boss lists, counts,
  0x800B0DCC), BATTLE AS BOSS, DOGFIGHT, DOOZER BATTLE, REVERSE RACE and
  BATTLE on four race courses, the reversed trial and BATTLE on MOUNTAIN;
  mr-fuzz --secrets: 272 runs, 5.5 M ticks, 0 panics.
- Not checked against the oracle: DOGFIGHT, BATTLE AS BOSS, BATTLE on a
  race course, the reversed stunt trial, a REVERSE RACE lap completion
  (code + tests + fuzz only).

## ADVENTURE's markers (phase 6b) [CODE][GPU]
- FUN_80048A0C / FUN_80047F1C (drawn by FUN_80051698 once racing: the
  minimap; on a stunt course with a strip the progress bar): while the
  targets are not all found (flag 4), the target 0x800B068F still out is
  marked with the course's target icon (0x800AC9C0 + 6 * course) at its
  point (minimap) or at its strip place 0x800DD698[i] (FUN_80070084 at the
  course load), sized w * sin(frame % 0x21 * 0x40) >> 16; every 0x21 frames
  0x800B068F steps to the next target still out (state != 2). The port
  steps it in race.rs `hud_draws` (`RaceGlobals::adv.marker`). GP0
  (fx_adv_marker, adventure_dock route; FX_REGIONS now hold 0x800B068F):
  95 / 95 at lag 0 with the marker.
- The Doozer goal marker (FUN_8007E21C @0x8007E908 -> FUN_800362D4): with
  the parked Doozer still in the race (kart 1 +0x496 bit 0x40), each goal
  (challenge objects 0x800B0EBC.., 0x800B0B98 of them) within 20000 view
  units of the camera: two additive quads 0x100 x 0x100 (slot 0x15 at 0, 0,
  0x3F x 0x3F), red and blue, turning with the camera roll - (frame & 0x7F)
  * 0x20, 0x200 apart (mr-bevy effects.rs `doozer_goals`; not checked
  against GP0).

## Not ported (listed)
- PLAY MOVIE (phase 9), CHICKEN CHASE and the two-player rows / choices
  (phase 7).
- The secret screen itself (course 0x1D: the credits scene choosing world 7
  / the cup-8 arenas, FUN_80074A94 with 0x800B0F60); its courses' own menus
  are ported (frontend.md "Phase 6b").

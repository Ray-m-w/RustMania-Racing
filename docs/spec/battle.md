# Battle (race type 2, game mode 2): phase 4

The arenas' 1 PLAYER BATTLE: six karts on an arena, damage from contacts
and weapons, a kart at 1000 damage is knocked out, the last kart standing
wins. Evidence tags as elsewhere: [CODE addr], [RAM] oracle RAM, [GPU] GP0
packets, [SHOT] screenshots, [INFER].

Port: mr-sim session.rs (set-up, the win), progress.rs (the battle clock,
knock-outs, track direction), init.rs (grid), race.rs (gauge draws),
course.rs (`CourseInfo::arenas`); mr-assets hud.rs (`BattleHud`,
`battle_results`), hud_items.rs (`battle_hud`); mr-bevy menus.rs (course
menu, world 8), courses.rs, hud.rs, save.rs.

## The arenas and how the game offers them [CODE 8008ab5c, 8008c4ac][SHOT]
Race type 2 (FUN_8008AB5C): slot 2 of worlds 0..5, course 0x1B and cup 8.
| course | id | world / slot | karts | reached by |
|---|---|---|---|---|
| CANYON | 5 | 0 / 2 | 6 | the course select / course list (slot 2 "BATTLE"), open with flag bit 0 (a won race B opens it, FUN_8008BE68) |
| T_SQUARE | 19 | 1 / 2 | 6 | as CANYON |
| THEATRE | 20 | 2 / 2 | 6 | as CANYON |
| CHAMBER | 13 | 3 / 2 | 6 | as CANYON |
| LAGOON | 21 | 4 / 2 | 6 | as CANYON |
| LAB | 14 | 5 / 2 | 6 | as CANYON |
| BATTLE (ARENA/BATTLE) | 27 = 0x1B | 6 / 1 | 6 | SECRET COURSES slot 1 (bit 0x10 on the 24 courses of worlds 0..5); also the battle league's presentation course |
| VOLCANO | 34 | 8 / 0 | 5 | the secret screen (course 0x1D) once everything is complete (0x800E02B9 bit 1); the port lists world 8 then |
| HOUSE | 35 | 8 / 1 | 5 | as VOLCANO |
| WHATEVER | 36 | 8 / 2 | 5 | as VOLCANO |
The course menu (FUN_80051698 case 0, race type 2; worlds 0..6): 1 PLAYER
BATTLE, 2 PLAYER BATTLE, <COURSE> SECRETS, GAME MENU, BACK TO MAIN MENU,
"BEST TIME m:ss.hh" bottom right (the course's record: the fastest win)
[SHOT analysis/oracle/battle/lagoon_menu.png]. Cups above 6 use sub-state
0xC: 1 PLAYER BATTLE, 2 PLAYER BATTLE, CHICKEN CHASE, BACK TO CREDITS.
CROSS on row 0: the character select, then FUN_80073E44 asks for game
mode 2 (0x800B07E8 = 0; 7 with it set, the ADVENTURE boss battles; 0x14
with race flag 0x40, CHICKEN CHASE).

## Set-up (FUN_8008C4AC, FUN_800609F4) [CODE][RAM]
- Karts 6 (5 above cup 6); 0x800B0CD8 = the karts (the karts left
  standing); laps as a race's (unused but by the camera's finish rule).
- Countdown 0x78 + 180 = 300 (0x800B07F8 = 0x78): "BATTLE THE MUPPETS" over
  the opponents' faces while at or above 0x78, then READY.
- Grid: the first draw *rng % 6 (6 in modes 2 / 7 / 0x14, not the kart
  count), the other karts on the following slots mod 6; FUN_800609F4 takes
  index = grid as a race's (race type 2: 6 + 1, then the plain path).
  Opponents drawn as a race's; the special-move roll FUN_8009192C.
- FUN_800609F4 race type 2: +0x496 |= 8 (battle AI FUN_80090FAC) and +0x4A2
  = (rand() / 5462 + 5) * 60, for every kart; +0x4DF = a random byte's top
  bit.
- Oracle: the race load is exact, every named field (LAGOON).

## Every frame [CODE 80082708, 8007916c, 80086b7c, 80067ec0]
- Damage (as DISCO's race flag 8, already ported): kart contacts (landing
  100; side push |difference| >> 5) and projectile hits (100) add to +0x440
  and the gauge +0x43E; +0x43C / +0x478 / +0x4FB on the hitter.
- FUN_80082708 (modes 2 / 7, no race flag 8): a standing kart (+0x4C7 = 0)
  under 1000 damage runs its own clock +0x358 += dt * 5 (at most 180000);
  at 1000 or more it is knocked out: the explosion (one random byte), the
  squash, +0x4BE = 0x800B0CD8 (its place), +0x440 = 1000 + that place,
  0x800B0CD8 -= 1, +0x4C7 = 2, the voice (human) or one random byte (AI),
  +0x4F9 = 0x80, the AI takes it, spin. No laps.
- FUN_8007916C, at the frame's start: with 0x800B0CD8 == 1 each human still
  standing wins: +0x4C7 = 1, +0x4CA = +0x4BE = 1, the voice, its clock
  rounded to a multiple of 3; in a 1-player mode-2 battle with no race
  flags and the kart not AI-driven a time below the course's record
  (0x800E0190 + 0x20 w + 8 s) becomes the record (character +4, vehicle
  +5) and the won bit 2 is set (sound 0x24 when new); mode 7 sets 0x100
  instead. Then +0x4F9 = 0x80, the AI takes it. Once every human is out
  (knocked out or the winner) the game goes to mode 10 (results).
- Time limit (0x80067EC0): modes 2 / 3 with +0x358 above 179999 (10 min;
  mode 7: 89999): out.
- FUN_80086B7C (humans): in modes 2 / 7 +0x48E eases toward the nearest
  kart (+0x4DC), read live (the arrow points at it).
- Ranking FUN_800837F8: race type 2 sorts by +0x440, lowest first.
- The camera: as a race's (chase; after the finish trackside, or the orbit
  for a knocked-out kart); cups above 6 overhead (mode 12).
- The HUD's damage gauge FUN_80049FD8 draws three random bytes while
  +0x43E is 0x321..0x3E7 (a standing kart, racing).
- The AI: FUN_80090FAC (ai.md), chasing the nearest kart or following the
  path in a random direction; exact against the oracle here.

## HUD (FUN_80051698, FUN_8004F688 case 2, FUN_8004DAE0, FUN_80050B2C)
In add order:
- The +0x4F9 icon.
- Standing, racing: FUN_80049440 (SPECIAL MOVE, the arrow, the banner at
  y 0xD2 in mode 2), the weapon slots FUN_8004CBC8(0x2D), the turbo meter,
  the damage gauge FUN_80049FD8(0x30, 0xCF, kart, 1) (viewport bottom 0xEF
  - 0x20), then the minimap (knocked-out karts left out). No place, no LAP.
- Out or mode 10: FUN_8004DAE0 (viewport centre (0x100, 0x78)): by +0x38C %
  180, 60 VBlanks each: "+dealt -taken" (+0x43C / +0x43E) DAMAGE, +0x478
  BEST CRASH, +0x4FB KNOCKOUTS (value size 0 colour 4, label right-aligned
  8 px left of it, size 2 colour 5); the place +0x4CA (y - 0x18, large,
  colour 7, suffix); the time +0x358 (y + 0x11; colour 7 while it is the
  course record, race flags 0, place 1 and +0x38C & 0x1F < 0x10) and
  "TIME", or "OUT OF TIME" (y + 0x12).
- Counting down: "READY" blinking at or below 0x77; at or above 0x78
  "BATTLE THE MUPPETS" (0x100, 0xD2, large, colour 5; "2 PLAYER BATTLE" /
  "DOOZER BATTLE" in those modes) with the opponents' faces (FUN_80050F64;
  above cup 6 none in the last 15 frames).
- Racing: the race clock 0x800B0A94 (0x100, 0x1E, "%i:%i%i.%i") (mode 7
  counts 90000 down).
- Mode 10: "PRESS START" (0x100, 0xD2) while 0x800B0AF0 % 60 < 45.

## Checks (oracle, Session 7)
- analysis/oracle/courses/LAGOON (goto_course.py menu LAGOON; then
  capture_course.py LAGOON --hold cross 230 --end 2600: 1 PLAYER BATTLE,
  Kermit / RACER, CROSS held; damage dealt by contacts and weapons, no
  knock-out): `mr-course-check LAGOON`: race load exact; free run exact over
  all 1219 frames (six karts, camera, random state); teacher forced exact.
- analysis/oracle/battle/ko (capture_poked.py, LAGOON's grid with the five
  opponents' damage poked to 950, CROSS held, routes/battle_ko_pokes.json):
  three knock-outs by the AI's weapons and contacts (places 6, 5, 4);
  `mr-course-check --dir analysis/oracle/battle/ko LAGOON`: free run exact
  over 1510 frames, teacher forced exact.
- analysis/oracle/battle/win (the five at 1000, the player's clock at
  30000, routes/battle_win_pokes.json, save block captured): the five
  knock-outs at GO, the win the next frame, mode 10; free run and teacher
  forced exact (535 frames). Test `battle_win_matches_oracle`: places,
  damage, the clock 30010 -> 30009, the record 30009 by Kermit / RACER and
  the won bit (0x41 -> 0x43) equal the game's RAM / save block.
- mr-course-check now takes the karts' characters / vehicles from the RAM
  for `--dir` captures (the driver models feed the render pass the
  contacts read) and keeps the session's own history rings (the camera
  trail; not in the captures) over teacher-forced reloads: the
  teacher-forced camera is exact now (it was off where a kart moved: the
  race captures' frame 89, the battle's racing frames).
- GP0 (fx_capture.py routes/battle_ko_fx.json / battle_win_fx.json;
  hudx_check.py): the result 116 / 116 primitives (DAMAGE and BEST CRASH
  pages, place, record-flash time, TIME, PRESS START); racing 93 / 98 at the
  best sample lag (timer digit and weapon-icon pulse: mr-hudx's sampling
  lag); the intro's "BATTLE THE MUPPETS" exact and the five faces exact in
  shape, their swing one frame off in mr-hudx (the strip counter's
  sample).
- Tests tests/battle.rs: all ten arenas load and run a battle (mode 2, 6 /
  5 karts, countdown 300); the win frame and save block. mr-fuzz with the
  arenas: 10 arenas x 2 rates x 2 seeds, 0.8 M ticks, 0 panics.
- Window shots (local): analysis/s7/battle/port/lagoon_{60,600}.png.

## CHICKEN CHASE (game mode 0x14; ported in phase 7, see below)
Reached from the cup-8 arenas' menu (sub-state 0xC row 2) and the
secret screen's course menus (sub-state 0xB row 5; DISCO with flag 4): it
sets race flag 0x40 and 0x800B07CB = 2 (both pads pick a character),
0x800B07CA = 1 (split screen): a two-player mode only. Rules from the code
[CODE 8006e564, 8006ec4c, 8004f688, 80080b2c]: one chicken (0x800CB658,
hopping: FUN_8006EC4C, two random bytes a hop) runs from the nearest kart;
a kart within 0x100 ((d >> 10)^2) catches it (+0x4AC += 1, a new chicken
FUN_8006E564), the first to 0x800B0EBD = 10 (5 on a race-type-3 course)
wins and the other player is out; "CATCH %i CHICKENS" in the countdown,
"WINNER / LOSER - n" in the result; the time limit is the race clock at
180000; no lap timing; the camera's watch target 0x7F is the chicken; the
track direction points at it. Phase 7 (split screen) is its home.

## Phase 6b: BOSS CHALLENGE, the arena secrets, BATTLE LEAGUE
- Game mode 7 (BOSS CHALLENGE), BATTLE AS BOSS (race flag 0x20), DOGFIGHT
  (0x80), DOOZER BATTLE (0x100), BATTLE on race courses (flag 1): ported;
  set-ups, rules, HUD and checks in docs/spec/adventure.md "COURSE
  SECRETS" (oracle: boss_lagoon, boss_win, doozer_battle exact).
- BATTLE LEAGUE (league 2: CANYON, T_SQUARE, THEATRE, CHAMBER, LAGOON, LAB;
  mr-bevy league.rs, mr-sim save.rs `battle_points`, progress.rs
  `battle_league_finish`) [CODE][RAM]:
  - TOURNAMENT -> BATTLE LEAGUE (no laps), the league menu "START BATTLE
    n" (0x800C1E4C = 1), the battles with the league's karts.
  - FUN_80082450 (FUN_8007916C @0x80079718, the frame every human is out,
    0x800B07E4 set, mode 2, AI karts left): the standing AI karts fight it
    out at random until one is left: a random standing kart (*rng %
    0x800B0DFE) hits another standing one (again, not itself) for 100
    (next byte < 0x40) or the byte after + 1 (then its best crash +0x478);
    +0x43C / +0x43E / +0x440 grow; at 1000 the other is knocked out
    (+0x440 = 1000 + 0x800B0CD8, its place, 0x800B0CD8 - 1, AI-driven,
    out, spinning) and the hitter's +0x4FB counts it.
  - Points (FUN_8008C4AC entering mode 0x11, 0x800C1E4C = 1): place 3 / 2
    / 1 by 0x800D8F94 (damage, least first), +1 the best crash (the
    biggest +0x478 above 0), +1 the most knock-outs (+0x4FB; nobody when
    shared), +1 the most damage dealt (+0x43C, ties to the fewest taken
    +0x43E, the race league's most-hits loop); RACE POINTS headings
    POSITION, BEST CRASH, MOST KNOCKOUTS, MOST DAMAGE (0x800AB468[4..8]).
  - Check (tools/oracle/save_check.py battle_league, local
    analysis/oracle/secrets/battle_league: CANYON, the player put out
    (+0x4C7 = 2) 300 VBlanks after GO): `mr-course-check --dir
    analysis/oracle/secrets/battle_league/battle CANYON`: teacher forced
    exact over 80 frames incl. FUN_80082450's (four knock-outs, places 6..3,
    random state); free run exact up to the poke. The points and table
    equal the game's 0x800C1E08 / 0x800C1E30 (test
    save::tests::battle_league_points_match_oracle).
  - Not ported: the tie-break battle and the presentation on 0x1B (mode
    0x12), as the race leagues'.

## Phase 7: 2 PLAYER BATTLE and CHICKEN CHASE (two players)
- Set-up (FUN_8008C4AC with 0x800B07CA = 1, split screen 0x800B0CA6 = 1,
  0x800B0E28 = 2): 0x800B0DFE = 2 (3 in a league) in every split-screen
  mode, both karts human (FUN_800609F4 flags 0, characters 0x800B0B28[i],
  vehicles 0x800B0C18[i]), the first grid slot *rng % 6 (modes 2 / 7 /
  0x14 off a stunt course; else % karts), player 2 on the next one; no AI
  karts. 0x800B0CD8 = 2 in an arena. 2 PLAYER BATTLE is game mode 2: the
  last kart standing wins (FUN_8007916C, as one player); the course record
  and won bit only with one player (0x800B0CA6 == 0).
- CHICKEN CHASE (mode 0x14, race flag 0x40) [CODE 8006e564, 8006ec4c,
  8006ea2c, 80069648, 80086b7c, 80041a20, 800728c0, 8004f688, 80050f64]
  (mr-sim chicken.rs):
  - FUN_8006E564 (race load, then after each catch): `near` from 0x41
    down by 1 a try; a random path segment ((rand() / (0x7FFF / n + 1)),
    not flagged 0x80) and point (rand() / (0x7FFF / count + 1)), << 5 (no
    path: near kart *rng >> 7 plus three (rand() - 0x4000) * 0x20); retried
    while within near * 0x1000 of a kart on all three axes, or with no
    ground; on the surface below (y = it - 0x2000), a flag-0x40 floor above
    the ground kept; heading = *rng << 4; on a stunt course its strip place.
  - FUN_8006EC4C (race frame, after the projectiles): with ground under it,
    dt steps of vy += 0x60, y += vy; landing (ground <= y + 0x2000): a new
    hop when *rng < 0x20 (vy = ((rng[1] >> 3) + 0x20) * -0x20, sound 0xB)
    else still. The nearest kart by driver position (+0x330 / +0x338 /
    +0x340, (d >> 10)^2, first minimum); the heading eases 1/8 per VBlank
    toward it (the chicken does not move sideways). Within 0x100: a kart
    still in, after GO, below 0x800B0EBD catches it: +0x4AC + 1, +0x4FC =
    0xD, +0x4B0 = 0, +0x444 = 1; at 0x800B0EBD it wins: it and the other
    player (by player index) out (+0x4C7 = 1, +0x4F9 = 0x80, AI-driven;
    FUN_80060580 voices not modelled), else a voice bit by *rng (8 / 4 / 2).
    Then rumble 200, sounds 0x14 and 0xB, the burst (FUN_8006EA2C: one puff
    kind 0x11 and five feathers kind 9, 47 random bytes), a new chicken, a
    second burst.
  - A projectile hit costs the victim a chicken (FUN_80069648 mode 0x14).
    No damage or knock-outs (FUN_80082708 returns), no positions
    (FUN_800837F8 returns); the time limit is the race clock at 180000; the
    results once both players are out.
  - HUD (FUN_8004F688 case 0x14, split screen): the count at (0x50, 0x60 /
    0x8F), "CATCH 10 CHICKENS" (FUN_80050F64 at y 0x78); the result
    "WINNER - n" / "LOSER - n" at the player's half centre, "OUT OF TIME".
    The chicken is a camera-facing sprite (FUN_8007E21C -> FUN_80030AD8:
    one of 0x800A8848's view frames by its heading against the camera yaw,
    slot 0x14 CLUT + 7, 0xC0 x 0xC0).
  - Reached from cup 8's arena menus (row 2), world 7's course menus (row
    5) and COURSE SECRETS (the chicken row, course flag 4).
- Oracle (tools/oracle/capture_two.py, local analysis/oracle/two/chicken:
  VOLCANO, CHICKEN CHASE from the course menu, pad 2 steered live toward
  the chicken): race load exact (both karts; the random state after the
  chicken's spawn equal), free run and teacher forced exact over 1419
  frames incl. two catches, both cameras, the chicken record exact every
  frame (`mr-course-check --cap analysis/oracle/two/chicken VOLCANO`; the
  first frame's camera 1 rand fields differ in the free run: the menu
  camera's carried values, as on one player).
- (Phase 13: 2 PLAYER BATTLE, a CHICKEN CHASE win and the chicken's
  projectile hit are checked: "Phase 13" below.)

## Not ported (listed)
- A 2-player league (TOURNAMENT - 2 PLAYERS: 3 karts).
- (The secret screen, course 0x1D, is ported in phase 8: frontend.md "THE
  END"; BACK TO CREDITS goes there. The port also keeps worlds 7 and 8 in
  the world list.)
- The league's tie-break and presentation (above).

## Phase 13 (Session 8): oracle checks
Captures local under analysis/oracle/two and analysis/oracle/secrets;
routes tools/oracle/routes/*.json (capture_two.py: "pokes", "save",
"writes" = TCP write_ram, chase2_target kart0 / kart0_then_chicken,
chase2_aim, until_out).
- 2 PLAYER BATTLE (two_lagoon_battle: LAGOON, pad 2 chasing kart 0 and
  firing): race load exact (2 karts), free run and teacher forced exact
  over 1069 frames with damage both ways.
- 2 PLAYER BATTLE won (two_lagoon_battle_win, player 2's damage held at
  1000): the split result panels GP0 329 / 329; the save block unchanged.
- CHICKEN CHASE projectile hit and win (two_volcano_chicken_win: player 2's
  slots charged, counts 3 / 9 written at VB 300, pad 2 fires at kart 0
  until it loses a chicken, then chases): `mr-course-check VOLCANO --dir
  analysis/oracle/two/chicken_win --from 320` (MR_KEY_ENDS=1): free run and
  teacher forced exact over 1419 frames through the hit (kart 0 3 -> 2), the
  tenth catch and the results; the chicken exact every frame. Fixes: the
  chicken runs on in the results (FUN_8007916C calls FUN_8006EC4C whenever
  race flag 0x40; the port stopped it outside mode 0x14: one random byte a
  frame short), each kart's look-at override uses its own camera
  (FUN_80041CC4 indexes camera +0x4F2: player 2's +0x2C0.. took camera 1).
- CHICKEN CHASE result panels (two_volcano_chicken_results: counts and
  "out" written): WINNER / LOSER GP0 104 / 104.
- DOGFIGHT (dogfight_battle, ARENA/BATTLE flags 0x61 -> 0x79): race load
  exact (six karts on vehicle 8), free run and teacher forced exact over
  584 frames.
- BATTLE AS BOSS (boss_as_lagoon, LAGOON flags -> 0x159): race load exact
  (THE CHIEF on the LOG and five drawn), free run and teacher forced exact
  over 636 frames.
- BATTLE on a race course (battle_dock, DOCK flags 0x101 -> 0x111): race
  load exact, free run and teacher forced exact over 706 frames.
- Racing HUD in split screen (two/battle_hud): the minimap at the split
  matches; the rest of the misses are the sampled row's lag (the arrow,
  the weapon rings' pulse, a clock digit, a map marker one pixel): each
  matches at the neighbouring row.

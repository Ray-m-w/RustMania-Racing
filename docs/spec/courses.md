# Courses (Session 6, track P)

Every race course of the game in the port: the game's own course table, the
per-course objects and features the race code drives, and how each course
checks against the oracle. Code: port/crates/mr-sim/src/course.rs (table),
objects.rs (jets, turntables, DAT path helper), hooks.rs, lifts.rs,
breakables.rs, weather.rs; session.rs `SessionConfig::course`;
mr-bevy courses.rs (course choice, pick-list). Tags as elsewhere: [CODE addr],
[RAM], [ORACLE], [INFER].

## The course table [CODE]
- 0x800ACAA0: course-name pointers, 37 entries, index = course (0x800B0FAC,
  "event"): `course_a\dock` -> COURSE_A/DOCK.{RAW,DAT,TER,AXE} (FUN_8004627C).
  10 = the front end ("\"), 29/30 = frontend\the_end / options.
- 0x800AC1E4: 9 cups x 4 slots of course indices (i8, -1 none). FUN_8008AB5C
  finds the course: 0x800B0E38 = cup, 0x800B0E32 = slot, race type 0x800B0B9A:
  cups 0..5 slot 3 = 3 (stunt), slot 1 = 1 ("race B"), slot 2 = 2 (arena);
  cup 8, courses 0x1B / 0x1E = 2; else 0 ("race A").
- The race loader FUN_8008C4AC: mode 0x800B0D46 from the race type (0/1 -> 1
  race, 2 -> 2, 3 -> 3); karts 6, 5 when the cup is above 6 (1 on a stunt
  course); laps 0x800E038B[humans] (3 at boot, FUN_8008F448); the music track
  from the course row (+3) unless course 0x1D / mode 0xE; the race-intro voice
  clip (+4, toggled by +0xA bit 0).
- 0x800ACB34, 12 bytes per course: +0..2 fog colour (= the frame clear, checked
  on DOCK), +3 music track (TUNES/MUSIC(t/8+1).XA channel t%8), +4 intro clip
  base (i16, -1 none), +6 flags (0x10 desaturate; 0x20 snow, 0x40 rain:
  FUN_8008FEA8), +8 the cup's boss character, +0xA clip toggle, +0xB scenery
  LOD depth bias.
- 0x80023678: boot-time lap records, 36 i32 [cup * 4 + slot] (FUN_8008F448
  copies them to 0x800E0190 + 8 * (cup * 4 + slot)).
- DISCO (0x1F): the menu sets race flag 8 (0x800B07E2) for it (0x80073E44 and
  its siblings): a damage race (below).

The race courses are the 18 of race type 0 / 1. VOLCANO, HOUSE and WHATEVER
(COURSE_C, cup 8) are arenas: race type 2, no finish line (DAT chunk 3) and no
course points (chunk 5); the game's VOLCANO course menu offers only battle and
CHICKEN CHASE (track O). The six ARENA/* courses are battle arenas (with
ARENA/BATTLE, course 0x1B, and the three cup-8 courses: the ten arenas of
the 1 PLAYER BATTLE, phase 4, docs/spec/battle.md; oracle LAGOON exact); the six
STUNT/* courses (FUN, REST, DINER, ROOFTOPS, MOUNTAIN, MLABS: cups 0..5 slot
3, race type 3) run the stunt trial (docs/spec/stunt-trial.md).

| course | id | dir | cup/slot | type | karts | music (track) | intro clip | flags | lap record | features beyond DOCK |
|---|---|---|---|---|---|---|---|---|---|---|
| BOG | 8 | A | 0/0 | 0 | 6 | 0 | 0x90 | 805E | 7194 | rain; 2 lifts moving TER groups 0, 1 |
| GHOST | 2 | B | 0/1 | 1 | 6 | 1 | 0x92 | 801F | 13674 | 10 breakable groups; fog (0x40,0,0) |
| HAPPY | 9 | A | 1/0 | 0 | 6 | 4 | 0x98 | 800A | 12252 | 1 lift (group 0); 4 breakables |
| GALLERY | 4 | B | 1/1 | 1 | 6 | 5 | 0x9A | 8002 | 15870 | 17 breakables (group 0 takes 1) |
| PARK | 16 | A | 2/0 | 0 | 6 | 8 | 0xA0 | 801F | 13428 | fog (0x30,0x0B,0x30) |
| NEW_YORK | 17 | B | 2/1 | 1 | 6 | 9 | 0xA2 | 801E | 7722 | 16 jets; 2 breakables |
| LONDON | 1 | A | 3/0 | 0 | 6 | 12 | 0xA8 | 802A | 14190 | snow; 5 jets; 13 breakables |
| GRAVE | 7 | B | 3/1 | 1 | 6 | 13 | 0xAA | 803F | 6882 | snow; gate group 2 (off until group 1 breaks); 2 breakables |
| DOCK | 3 | A | 4/0 | 0 | 6 | 16 | 0xB0 | 801F | 12186 | (the reference) |
| ISLAND | 6 | B | 4/1 | 1 | 6 | 17 | 0xB2 | 801F | 10716 | 1 lift (groups 0, 1); 7 breakables |
| BASE | 15 | A | 5/0 | 0 | 6 | 20 | 0xB8 | 801E | 10614 | 2 jets; 1 turntable; 4 breakables |
| CAPE | 18 | B | 5/1 | 1 | 6 | 21 | 0xBA | 801F | 10296 | 9 jets |
| OVAL | 26 | C | 6/0 | 0 | 6 | 25 | - | 8006 | 1620 | 6 turntables (spin x3) |
| ROCK | 28 | C | 6/2 | 0 | 6 | 28 | - | 8002 | 6264 | 3 lifts (scenery only) |
| DISCO | 31 | C | 6/3 | 0 | 6 | 24 | - | 801F | 15426 | race flag 8 (damage, knock-out); 4 jets; 1 turntable; 6 lifts (groups 0, 1) |
| SPEEDWAY | 0 | C | 7/0 | 0 | 5 | 26 | - | 8002 | 18000 | overhead camera |
| RALLY | 32 | C | 7/1 | 0 | 5 | 1 | - | 8002 | 18000 | overhead camera |
| GLACIER | 33 | C | 7/2 | 0 | 5 | 13 | - | 8022 | 18000 | overhead camera; snow; 5 jets |

(Music, intro clips: track O's captures saw exactly these XA files / channels
on all 17 captured courses.)

## Course objects (DAT point chunks, 0x800D1C48 + 4 * id) [CODE]
| chunk | RAM | what | code | port |
|---|---|---|---|---|
| 0x02 | 0x800D1C50 | hooks: groups of 32 path points, 3 hooks each (0x800B0E00); an airborne kart near a free hook is carried until the hook's phase wraps; a jump / wall drops it | FUN_800616A0, FUN_800617CC, FUN_80061BC4, 0x8003EB0C, 0x80066314 | hooks.rs; no race course has a valid list (GRAVE's 14 points are not a multiple of 32), stunt courses do |
| 0x0E | 0x800D1C80 | air jets, 5 points: on / off times from the point distances, push a kart inside the active nearest one (one signed random byte), puff smoke | FUN_800637B8, FUN_80063A98, FUN_80063C24, FUN_8007F03C loop 1 | objects.rs |
| 0x11 | 0x800D1C8C | turntables, 3 points (centre, inner, outer radius): turn a grounded kart on the ring by spin x dt; spin +-5 (x3 on courses 0x1A/0x1B) from rand() at course load | FUN_80084490, FUN_800632BC | objects.rs |
| 0x18 | 0x800D1CA8 | lifts, 4 points: wait at P0.y, rise 0x39 a frame to P1.y, wait, return; the course's animated objects that name a lift move their TER groups by its change (vertex y, wall heights) | FUN_80063554, FUN_800859EC, FUN_80042374 | lifts.rs |
| 0x0A / 0x0B | 0x800D1C70 / 74 | orbit camera look / eye paths (camera mode 1) | FUN_80070418 case 1 | camera.rs |
| 0x04, 0x09, 0x0F / 0x10, 0x14, 0x08 / 0x17 | | the stunt courses' gates, banners, strip, pipes, mirror planes | FUN_80081DD8, FUN_80034CE0, FUN_8006F64C .., FUN_800628FC, FUN_80062E38 | stunt.rs (docs/spec/stunt-trial.md) |
| 0x0D | 0x800D1C7C | paired triggers (two-way links; MOUNTAIN, DINER) | FUN_80061F08 / 80061FC0 | progress.rs `teleport` (none on race courses) |

Breakable scenery (no chunk; FUN_80083B08's per-course switch, jump table
0x80023318 course - 1): TER groups listed with their DAT slots; a projectile's
wall hit on such a group (FUN_8003C568 -> FUN_80069334), or its landing on
a floor / ceiling face of one (FUN_8003C4D4; track S), disables it (faces and
walls stop colliding, FUN_8004229C), hides the scenery and puffs two
explosions; the projectile is spent and its step ends there. Tables checked against RAM on all 8 courses that have them.

Weather (course flags 0x20 snow / 0x40 rain, 0x800B0ACA): FUN_8008650C, 16
drops at 0x800D75D0; each frame the next free slot is placed at random around
the camera (two rand(), one signed byte for its life, one rand() for its
speed) on the highest surface; rain splashes (particle kind 0x10, or a splash
ring and sound 0x1F on water), snow settles. weather.rs.

## Race flag 8 (DISCO) [CODE]
Kart contacts (0x80040A1C landing: 100; 0x800410A8 side push: |push difference|
>> 5) and projectile hits (0x8006AB18: 100) add damage to +0x440 (and +0x43E,
the HUD gauge; +0x43C / +0x478 / +0x4FB on the hitter). At 1000 the kart is
knocked out (FUN_80082708 0x8008279C): explosion, body squash, +0x4C7 = 2, the
AI takes it, spin. Lap timing goes on. The camera of a knocked-out human
orbits the course (mode 1). The damage gauge draws three random bytes per
frame while +0x43E is 0x321..0x3E7 (FUN_80049FD8).

## Camera
- Mode 12 (cups above 6): 0x1400 TER units above the kart (less during the
  countdown), straight down, no yaw / roll (0x80070D94, 0x800723E8).
- Mode 1 (orbit): eye on chunk 0x0B's loop at 16 / VBlank (FUN_80081DC4, the
  path length, returns 0), looking at chunk 0x0A's loop 0x400 ahead or behind;
  banked against the yaw error.

## Oracle checks (track O's captures, mr-course-check)
One capture per course (O, analysis/oracle/courses/NAME, 30 Hz, course menu ->
1 PLAYER RACE, Kermit / RACER, player idle, ~1630 frames: race load,
countdown, GO at frame ~239, about one lap of the AI field). DOCK: Session 3's
start_probe (same route).
`cargo run --release --offline -p mr-sim --bin mr-course-check -- [NAME...]`
(`--fields`, MR_TRACE=a-b, MR_FREEFIELDS=n, MR_HIDDEN (first differing
particle / spark / projectile / jet, field by field), MR_TFFIELDS=... for
detail; `--dir DIR NAME` for a capture that starts mid-race, e.g. a poked
grid state from tools/oracle/capture_poked.py). The session starts from the
oracle's BIOS seed and random pointer at the first mode-1 VBlank, plus the state the course load / menus leave that a
race load does not reset (water animation, blink timers, weather drops, lift
positions, the turntable phases and pickup animation (the token colours and
turntable spins are the session's own course-load draws from the seed
`--course-load` finds, kart-physics-ledger.md track S), the round-robin counter
0x800C3058, the projectile ring slot 0x800B07E0, the camera position,
particles / projectiles in flight).

| course | race load | teacher forced (motion, every frame) | random state (tf frames off) | free run: first kart divergence (frame; GO = ~239) | before track S |
|---|---|---|---|---|---|
| BASE | exact | exact | 0 | exact to the end (1632) | 774 |
| BOG | exact | exact | 1 (weather) | exact to the end (1603) | 395 |
| CAPE | exact | exact | 0 | exact to the end (1631) | 537 |
| DISCO | exact | exact | 0 | exact to the end (1635) | 328 |
| GALLERY | exact | exact | 0 | exact to the end (1630) | 1003 |
| GHOST | exact | exact | 0 | exact to the end (1631) | 689 |
| GLACIER | exact | exact | 0 | exact to the end (1635) | 632 |
| GRAVE | exact | exact | 1 (weather) | exact to the end (1632) | 556 |
| HAPPY | exact | exact | 0 | exact to the end (1633) | 1092 |
| ISLAND | exact | exact | 0 | exact to the end (1634) | 726 |
| LONDON | exact | exact | 0 | exact to the end (1629) | 529 |
| NEW_YORK | exact | exact | 0 | exact to the end (1632) | 547 |
| OVAL | exact | exact | 0 | exact to the end (1631) | 380 |
| PARK | exact | exact | 0 | exact to the end (1633) | 704 |
| RALLY | exact | exact | 0 | exact to the end (1635) | 502 |
| ROCK | exact | exact | 0 | exact to the end (1635) | 485 |
| SPEEDWAY | exact | exact | 0 | exact to the end (1635) | 446 |
| DOCK (start_probe) | exact | exact | 48 (as before S; start_probe holds 0x40 bytes of the particle list and no sparks [INFER]) | exact to the end (834) | 456 |

Divergences (kart-physics-ledger.md, Session 6 sections P and S): track S
found the free runs' first hidden difference was the projectile ring slot
(0x800B07E0 runs on from the menus like the other carried state; the
checker now sets it from RAM) and, on GRAVE / LONDON / GALLERY, a
projectile breaking scenery: the game skips that projectile's ground step
and kart hits and draws one random byte for the owner's +0x4DD (ported).
DISCO's frame 387 was the lifts: the captures do not hold them, and the
course menu keeps stepping them (one step per menu frame, dt 2) between the
menu state and the race load; the checker now runs them on through those
frames (oracle: analysis/oracle/stuck/DISCO_lifts) and keeps its own lift
and breakable-group states over teacher-forced reloads (neither the lifts
nor the TER are captured). Particles compare without the render pass's
on-screen bit (flags 0x200). All 18 courses run exact to the end of the
capture, random state included; the two teacher-forced random-state frames
left are weather spawns (GRAVE 273, BOG 1056). Phase 14 recheck: both now
teacher-force with 0 random-state frames and the drops (MR_WEATHER) equal
every free-run frame on GRAVE, BOG, LONDON, GLACIER (fixed by phase 7's
weather camera / nearest-camera search; not re-bisected).

Phase 14, GRAVE's "far star" (SESSION-6-visual-check): not a port
difference. The oracle's GP0 frame at VB 3380 (analysis/s8/grave_star)
puts the two nearest pickup stars at native x 26..70 (OT 2800, the HUD's
slot) where the port draws them; the oracle's window shows them further
left because the recomp's widescreen package (auto_ui_squash) pins
front-of-OT primitives as HUD. The far three match in both.

## Render notes (Session 6, track R)
What the courses move or animate and how mr-bevy draws it (render-ledger.md
has the rules; code: mr-sim scenery.rs, mr-bevy movers.rs, animtex.rs,
kartfx.rs, hud_items.rs).
- The DAT special slots outside the pickups (0x20 + the chunk 0x0C items)
  are course scenery: before Session 6 R the full view left all of them out
  (breakables, spinners, lifts' pieces, CAPE's slots 2..0x13 etc. were
  invisible); now each is drawn at its run-time pose.
- Animated objects 0x800D8630 (FUN_80084490 / FUN_800859EC; `mr-scenery`
  checks structure, phases and every moved slot's matrix / origin against
  the course captures' RAM):

| course | moving / animated | evidence |
|---|---|---|
| BOG | lifts 0, 1 move slots 1..3 / 4..6 (and TER groups 0, 1); rain | 6 slots exact; rain streaks vs end.png |
| GHOST | 10 breakable groups (slots 1..10) hidden when hit | slots drawn (grid sbs) |
| HAPPY | lift 0 moves slot 0x1F; slots 0x50..0x53 pulse in x (frame % 48); 4 breakable groups (slots 1..10) | 5 slots exact |
| GALLERY | 15 breakable slots (group 0 takes 1); VRAM scroll slot 5 (0x40, 0x40) 8 x 64 | - |
| PARK | - | - |
| NEW_YORK | 2 breakables; VRAM scroll slot 5 (0xC0, 0xC0) 64 x 64 | - |
| LONDON | snow; 13 breakables; jets' smoke (particles) | snow vs end.png |
| GRAVE | snow; 2 breakables (group 2 is TER only) | - |
| DOCK | (none; its record names slot 1 with no lift) | - |
| ISLAND | lift 0 moves slots 1..4 (and groups 0, 1); 7 breakables; VRAM scroll slot 5 (0xC0, 0xC0) down | 4 slots exact |
| BASE | turntable disc slots 1, 2 (turntable 0's spin); fans slots 3 / 4 spinning up / down with jets 1 / 0; slots 5 / 6 turned by the global spin 0x800A889C; 4 breakables | 6 slots exact, load phases exact |
| CAPE | slot 1 (layer 1) spins +-5 (sign from a random byte); jets' smoke | 1 slot exact |
| OVAL | 6 spinners (slots 1..6) at the turntables' spins | 6 slots exact, load phases exact |
| ROCK | lifts 0..2 move slots 1..3 (scenery only) | 3 slots exact |
| DISCO | 12 spotlight spinners (slots 1..12; speed / sign from two random bytes each), lifts 0..6 move slots 0xD..0x15 (0xE a layer-1 stand-in) and TER groups 0..6, slot 0x16 at turntable 0's spin; two VRAM scrolls; damage gauge (race flag 8) | 18 slots exact, load phases exact; gauge vs end.png |
| SPEEDWAY, RALLY | - | - |
| GLACIER | snow; jets' smoke | snow vs end.png |

- Weather drops, projectile shadows, VRAM scrolls, damage gauge: see the
  render ledger. Side by sides: analysis/s6/courses/sbs_NAME_{grid,end}.png
  (oracle left, port right; the port's free run is not the oracle's, so
  karts / camera differ), look_*/pair_* (dev MRM_LOOK views two ticks apart).
- Closed (R, after the lead's follow-up): the "DISCO palette" was not a
  tint: with the oracle's own camera (eye and view matrix from the capture,
  dev MRM_LOOK) the port's frame has the oracle's colours on every course
  (analysis/s6/courses/samecam_NAME_{grid,end}.png, sheets samecam_sheet*);
  the port's free-run camera / grid line-up simply looks elsewhere. Also
  checked: the course VRAM after the load = mr-raw's decode (DISCO 99.3%,
  the rest the scrolled / animated rectangles; tools/oracle/course_vram.py)
  and 108 of 108 matched oracle GP0 polygons carry the DAT vertex colours.
  The minimap was missing on RALLY (0x20) and GLACIER (0x21): the HUD read
  the course flags and map table for courses 0..0x1F only; now 0..0x24, and
  knocked-out karts (race flag 8, battles) leave the map as FUN_80048A0C
  does. All 17 maps vs end.png: analysis/s6/courses/minimaps_all.png.

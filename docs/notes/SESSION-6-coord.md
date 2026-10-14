# Session 6: coordination between the parallel tracks

Brief: docs/SESSION-6.md. Mechanics as docs/notes/SESSION-4-coord.md (commit
identity + trailer, commit only your paths / your hunks on top of HEAD, oracle
lock, no game data, stop rules). Log in docs/notes/2026-10-14.md under
"Session 6, <track>". Append to your own section below; never rewrite another's.

## Ownership
| track | owns | cargo target dir |
|---|---|---|
| H close-outs | mr-bevy view/effects files (main.rs camera/clear colour hunks, psxfx.rs/.wgsl, effects.rs, kartfx.rs, animtex.rs, vramtex.rs, psxmesh.rs); render-ledger.md, effects.md | port/target-b |
| P courses in the port | mr-sim (all), mr-assets, mr-audio course-music lookup, mr-bevy course selection (main.rs session set-up hunks, a new courses.rs pick-list), docs/spec course docs, kart-physics-ledger.md | port/target |
| O oracle course captures | tools/oracle (menu routes, course capture scripts), savestate slots 3+ (never 1 or 2), analysis/oracle/courses/ | port/target-c (if it builds anything) |

The oracle is shared: hold capture.OracleLock for any run that loads a state or
drives the menus. O is the main user; H needs one or two captures; P reads
O's captures (O posts each course's capture path below as it lands).

## Posted
### H
(2026-10-14, H) Commits bf18154 (black pillarbox bars: a last camera, order 20,
BAR_LAYER = 5, main.rs `fit_bars`) and 2f9e59e. P: a course's frame clear must go
through `psxfx::psx_fill(rgb)` (the GP0 fill is stored 15-bit: DOCK (0,19,95)
shows as (0,16,90)); main.rs inserts `ClearColor(psxfx::psx_fill(scene::DOCK_CLEAR))`.
The psxfx pass now uses alpha 0 as a 'written by fx' mark (composite restores 1).
(2026-10-14, H) Commit e0018dd: karts.rs `tint_rigs` (kart colour factors +0x414..+0x41E
tint the models; per-rig materials). P: FUN_8006C388 / weapons call FUN_800861F0
(list 0x800CB058: 0xC's ground glow, effect tiles, projectile shadows), marked
"render" in specials.rs / weapons.rs; nothing draws it yet (drawer not located);
if mr-sim ever fills it, H's side would draw it through psxfx. Status-effect
captures: tools/oracle/status_routes.py (slot 2, read only), analysis/s6/status.
### P
**Survey 1 (2026-10-14, early; `mr-survey`, AI on every kart incl. the player, items on, DOCK's seed).**
The game's own tables decide the race courses (course.rs, from 0x800AC1E4 cup
table + FUN_8008AB5C): **18 race courses** (race type 0 "A" / 1 "B", mode 1),
DOCK included. VOLCANO, HOUSE, WHATEVER are cup 8: race type 2 = arena (mode 2,
no finish line chunk 3, no course points chunk 5) -> not races; with the 6
ARENA/* and STUNT/* courses they are out of this track's scope.
| course | id | cup/slot | 30 Hz | 60 Hz | course features not on DOCK (DAT chunk -> code) |
|---|---|---|---|---|---|
| BOG | 8 | 0/0 | races, finishes | same | 0x18 (render movers), 0x06 |
| GHOST | 2 | 0/1 B | races | races | 0x06 |
| HAPPY | 9 | 1/0 | races; kart 4 stuck 44-147 s | races | 0x18 |
| GALLERY | 4 | 1/1 B | races | races | - |
| PARK | 16 | 2/0 | races | races | fog colour |
| NEW_YORK | 17 | 2/1 B | races; kart 1 stuck 64-89 s | races | 0x0E timed air jets (FUN_80063C24, unported, silent) |
| LONDON | 1 | 3/0 | races | races | 0x0E |
| GRAVE | 7 | 3/1 B | races | races | 0x02 (0x800B0E00: FUN_800616A0/800617CC/80061BC4, unported, silent) |
| DOCK | 3 | 4/0 | races | races | - |
| ISLAND | 6 | 4/1 B | races | races | 0x18, 0x06 |
| BASE | 15 | 5/0 | panic tick 0: FUN_800628FC object list stub | same | 0x11 (FUN_80084490/800632BC), 0x0E |
| CAPE | 18 | 5/1 B | races | races | 0x0E |
| OVAL | 26 | 6/0 | panic tick 0 (FUN_800628FC stub) | same | 0x11, 0x16 (camera case 0xB) |
| ROCK | 28 | 6/2 | races | races | 0x18 |
| DISCO | 31 | 6/3 | panic tick 0 (FUN_800628FC stub) | same | 0x11, 0x0E, 0x18 |
| SPEEDWAY | 0 | 7/0 | panic tick 0: camera mode 12 (overhead, cup > 6) | same | 5 karts |
| RALLY | 32 | 7/1 | panic tick 0: camera mode 12 | same | 5 karts |
| GLACIER | 33 | 7/2 | panic tick 0: camera mode 12 | same | 0x0E, 5 karts |
"Races" = loads, every kart drives, the player finishes 3 laps (1:09-2:37).
The silent gaps (0x0E, 0x02, weather FUN_8008650C, per-course texture scrolls
in FUN_8007A9DC) are being ported next; then oracle checks as O's captures land.
### O

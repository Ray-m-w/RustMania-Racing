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
(2026-10-14, H) Commit 8ba0c8e: the 0x800CB058 list = shadow-list entries 0xC6..
(drawn by FUN_8002ADA8); mr-bevy builds the specials' entries from kart records
(kartfx::special_decals) and draws them. P: the weapons' FUN_800861F0 calls
(projectile shadows) are not drawn; if mr-sim exposes those entries (pos, face,
size, scale, intensity, flags), effects.rs can draw them with kartfx::decal.
Also: kart models now keep the record matrices' scale and a gamma-space tint.
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
**Survey 2 / P status (2026-10-14, later).** All 18 race courses race at 30
and 60 Hz with no panic (`mr-survey`: every kart drives, player finishes 3
laps; DISCO = race flag 8: karts get knocked out, "K"). Oracle: race load
exact on all 17 of O's captures + DOCK; teacher-forced motion exact on every
frame (one DISCO frame); free runs exact through GO and 90-850 frames past it
(table: docs/spec/courses.md). Ported: jets, turntables, hooks, lifts (TER
groups), breakables, weather, race flag 8, camera modes 12 / 1, HUD random
draws, load-frame rand order. mr-bevy: `--course NAME` / pick-list
(courses.rs; HUD font via `hud::HudOverlay`), music per course,
`play-port.ps1 -Course`.
- T: the pick-list opens by itself when the race screen first shows without
  `--course` (it waits while your title has the race cameras off), so your
  START -> Screen::Race needs nothing more; a different course relaunches the
  exe with `--course X --skip-title` (scene is built at start-up). Your
  hunks in main.rs / settings.rs / audio.rs were left out of my commits.
- H: I replaced `crate::scene::DOCK` by `crate::courses::id()` in animtex.rs
  and effects.rs (one line each, 94f6e0f). Render gaps on other courses
  (yours or later): FUN_8007A9DC's per-course VRAM scrolls (FUN_8005E38C
  cases), moving scenery (FUN_800859EC: lifts, turntables, animated DAT
  objects; state in mr-sim `globals.objects`), weather drops
  (`globals.objects.weather`), knocked-down scenery (`objects.hidden_slots`),
  projectile shadows.
### O
**Course captures (O).** How: `python tools/oracle/goto_course.py menu NAME`
(slot 1 -> the game's own menus -> NAME's course menu, saved as
analysis/states/courses/NAME_menu.pst; route in tools/oracle/routes/menu_<name>.json),
then `python tools/oracle/capture_course.py NAME` (restore the course menu ->
CROSS 30/120/210 = 1 PLAYER RACE, KERMIT, RACER as start_probe.json -> player
idle to VB 3400; grid state saved 60 VB after mode 1). Each capture dir:
frames.bin + meta.json + ram0.bin (capture.load() reads it; ram0 = the course
menu state, course loaded, race not yet: the race load is in the frames),
grid.png, end.png. Regions = capture_course.COURSE_REGIONS (28): PHYSICS_REGIONS
+ capture_items.ITEM_EXTRA, with 0x800E0C20 widened to 0xF00 (sprite particles),
0x800D73D0 to 0x4C0 (side table + ambient list 0x800D75D0 16 x 0x2C), plus spark
streaks 0x800B4540 0x400 and flat quads 0x800C84B8 0x680. Grid states: slot
listed, or `goto_course.py install NAME grid [SLOT]` copies the archive into a
slot (default 11 = scratch; 12 slots only). Music: course table +3 (track t =
TUNES/MUSIC(t/8+1).XA ch t%8) and the XA sectors the oracle delivered
(subheader file/channel). Objects = end-of-capture 0x800D1C68/8C/98/A4,
0x800B0C58/0E00/0C68 (bytes), 0x800B0ACA ambient, rain (0x800B0B9A == 2 or
0x800B07E2 & 9); all of them are in the frames too.
| course | id | menu | grid slot (state) | capture | laps | music (table) | XA seen | objects |
|---|---|---|---|---|---|---|---|---|
| ISLAND | 6 | MUPPET TREASURE ISLAND / ISLAND - RACE B | 3 (analysis/states/courses/ISLAND_grid.pst) | analysis/oracle/courses/ISLAND (VB 1-3430, race mode 1 at VB 157, ram0 = course menu) | 3 | track 17 = MUSIC3.XA ch 1; intro clip 0xB2 | f1/ch2 LBA 201120-201488 (VB 22-206), f1/ch1 LBA 148322-156450 (VB 22-3414) | none |
| LONDON | 1 | THE MUPPET CHRISTMAS CAROL / RACE A | 4 (analysis/states/courses/LONDON_grid.pst) | analysis/oracle/courses/LONDON (VB 1-3432, race mode 1 at VB 157, ram0 = course menu) | 3 | track 12 = MUSIC2.XA ch 4; intro clip 0xA8 | f1/ch9 LBA 199671-200375 (VB 24-333), f1/ch4 LBA 127213-135045 (VB 333-3429) | 0x800B0C58=5; ambient list on (0x800B0ACA) |
| GRAVE | 7 | THE MUPPET CHRISTMAS CAROL / RACE B | 5 (analysis/states/courses/GRAVE_grid.pst) | analysis/oracle/courses/GRAVE (VB 1-3430, race mode 1 at VB 155, ram0 = course menu) | 3 | track 13 = MUSIC2.XA ch 5; intro clip 0xAA | f1/ch10 LBA 199672-200376 (VB 21-327), f1/ch5 LBA 127214-134998 (VB 327-3415) | ambient list on (0x800B0ACA) |
| BASE | 15 | MUPPETS FROM SPACE / RACE A | 6 (analysis/states/courses/BASE_grid.pst) | analysis/oracle/courses/BASE (VB 1-3430, race mode 1 at VB 158, ram0 = course menu) | 3 | track 20 = MUSIC3.XA ch 4; intro clip 0xB8 | f1/ch8 LBA 201110-201622 (VB 24-207), f1/ch4 LBA 148325-156333 (VB 272-3426) | 0x800D1C8C=0x8012FB48; 0x800B0C58=2 |
| CAPE | 18 | MUPPETS FROM SPACE / RACE B | 7 (analysis/states/courses/CAPE_grid.pst) | analysis/oracle/courses/CAPE (VB 1-3430, race mode 1 at VB 156, ram0 = course menu) | 3 | track 21 = MUSIC3.XA ch 5; intro clip 0xBA | f1/ch10 LBA 201112-201672 (VB 24-273), f1/ch5 LBA 148326-156270 (VB 273-3423) | 0x800B0C58=9 |
| PARK | 16 | THE MUPPETS TAKE MANHATTAN / RACE A | 8 (analysis/states/courses/PARK_grid.pst) | analysis/oracle/courses/PARK (VB 1-3430, race mode 1 at VB 155, ram0 = course menu) | 3 | track 8 = MUSIC2.XA ch 0; intro clip 0xA0 | f1/ch0 LBA 127209-200462 (VB 25-3426) | none |
| NEW_YORK | 17 | THE MUPPETS TAKE MANHATTAN / RACE B | 9 (analysis/states/courses/NEW_YORK_grid.pst) | analysis/oracle/courses/NEW_YORK (VB 1-3430, race mode 1 at VB 155, ram0 = course menu) | 3 | track 9 = MUSIC2.XA ch 1; intro clip 0xA2 | f1/ch2 LBA 199680-200320 (VB 22-267), f1/ch1 LBA 127210-135082 (VB 331-3425) | 0x800B0C58=16 |
| HAPPY | 9 | THE GREAT MUPPET CAPER / RACE A | 10 (analysis/states/courses/HAPPY_grid.pst) | analysis/oracle/courses/HAPPY (VB 1-3430, race mode 1 at VB 155, ram0 = course menu) | 3 | track 4 = MUSIC1.XA ch 4; intro clip 0x98 | f1/ch8 LBA 197990-199158 (VB 24-514), f1/ch4 LBA 104709-112061 (VB 514-3423) | none |
| GALLERY | 4 | THE GREAT MUPPET CAPER / RACE B | archive (analysis/states/courses/GALLERY_grid.pst) | analysis/oracle/courses/GALLERY (VB 1-3430, race mode 1 at VB 156, ram0 = course menu) | 3 | track 5 = MUSIC1.XA ch 5; intro clip 0x9A | f1/ch10 LBA 197992-198888 (VB 24-390), f1/ch5 LBA 104710-112334 (VB 390-3424) | none |
| BOG | 8 | THE MUPPET MOVIE / SWAMP - RACE A | archive (analysis/states/courses/BOG_grid.pst) | analysis/oracle/courses/BOG (VB 1-3431, race mode 1 at VB 158, ram0 = course menu) | 3 | track 0 = MUSIC1.XA ch 0; intro clip 0x90 | f1/ch0 LBA 104705-198590 (VB 24-3423) | ambient list on (0x800B0ACA) |
| GHOST | 2 | THE MUPPET MOVIE / GHOST TOWN | archive (analysis/states/courses/GHOST_grid.pst) | analysis/oracle/courses/GHOST (VB 1-3430, race mode 1 at VB 155, ram0 = course menu) | 3 | track 1 = MUSIC1.XA ch 1; intro clip 0x92 | f1/ch2 LBA 197984-198752 (VB 20-328), f1/ch1 LBA 104706-112450 (VB 328-3419) | none |
| OVAL | 26 | SECRET COURSES / STUDIO - RACE | archive (analysis/states/courses/OVAL_grid.pst) | analysis/oracle/courses/OVAL (VB 1-3430, race mode 1 at VB 155, ram0 = course menu) | 3 | track 25 = MUSIC4.XA ch 1; intro clip 0xFFFF | f1/ch1 LBA 167914-176626 (VB 26-3426) | 0x800D1C8C=0x8012CF08 |
| ROCK | 28 | SECRET COURSES / FRAGGLE ROCK | archive (analysis/states/courses/ROCK_grid.pst) | analysis/oracle/courses/ROCK (VB 1-3430, race mode 1 at VB 155, ram0 = course menu) | 3 | track 28 = MUSIC4.XA ch 4; intro clip 0xFFFF | f1/ch4 LBA 168293-176837 (VB 22-3419) | none |
| DISCO | 31 | SECRET COURSES / DISCO | archive (analysis/states/courses/DISCO_grid.pst) | analysis/oracle/courses/DISCO (VB 1-3431, race mode 1 at VB 156, ram0 = course menu) | 3 | track 24 = MUSIC4.XA ch 0; intro clip 0xFFFF | f1/ch0 LBA 168297-176849 (VB 24-3422) | 0x800D1C8C=0x8012FBE0; 0x800B0C58=4; rain |
| SPEEDWAY | 0 | (world 7, poked) / SPEEDWAY - RACE | archive (analysis/states/courses/SPEEDWAY_grid.pst) | analysis/oracle/courses/SPEEDWAY (VB 1-3430, race mode 1 at VB 156, ram0 = course menu) | 3 | track 26 = MUSIC4.XA ch 2; intro clip 0xFFFF | f1/ch2 LBA 168291-176819 (VB 20-3413) | none |
| RALLY | 32 | (world 7, poked) / RALLY | archive (analysis/states/courses/RALLY_grid.pst) | analysis/oracle/courses/RALLY (VB 1-3430, race mode 1 at VB 157, ram0 = course menu) | 3 | track 1 = MUSIC1.XA ch 1; intro clip 0xFFFF | f1/ch1 LBA 105082-113618 (VB 21-3418) | none |
| GLACIER | 33 | (world 7, poked) / GLACIER | archive (analysis/states/courses/GLACIER_grid.pst) | analysis/oracle/courses/GLACIER (VB 1-3430, race mode 1 at VB 156, ram0 = course menu) | 3 | track 13 = MUSIC2.XA ch 5; intro clip 0xFFFF | f1/ch5 LBA 127582-136126 (VB 26-3420) | 0x800B0C58=5; ambient list on (0x800B0ACA) |
O notes: all 17 race courses captured (one run each, missing 0, 3 laps on every
course, Kermit 6th/idle on the grid, AI field ~1 lap by VB 3430; race mode 1
at VB 155-158 on all). Grid slots 3..10 = ISLAND, LONDON, GRAVE, BASE, CAPE,
PARK, NEW_YORK, HAPPY; the other nine are archive-only (install into slot 11).
Worlds 7 (SPEEDWAY, RALLY, GLACIER) race 5 karts with the overhead camera, as
P found. XA "f1/ch0" on PARK and BOG spans the intro clip and the music (same
subheader file/channel), hence the wide LBA range. OVAL/ROCK/DISCO needed the
world-6 unlock bytes poked (save shows "GAME STATUS CHANGED"); worlds 7/8 need
0x800B0F12 held during the menu route (not in the world list). VOLCANO's course
menu offers only 1/2 PLAYER BATTLE + CHICKEN CHASE (no race; screenshot in
O's log): not captured, nor HOUSE / WHATEVER (cup 8, same type per P's table).
### T
(2026-10-14, T) Commit dcc785a: the title screen is the port's first screen
(port/crates/mr-bevy/src/title.rs, Bevy state `title::Screen` Title -> Race).
While it shows: the race's cameras are off (HudCamera too, so P's
`race_visible` keeps the pick-list closed) and Time<Virtual> is paused; START
fades out, waits for START's release, unpauses and turns the race cameras back
on; P's pick-list then opens as on a plain start. `--skip-title` (settings row
`skip_title`) and every tool flag (--oracle-grid, --route, --replay,
--replay-capture, --shot, --fx-shots, --latency-log, --autopilot,
--record-route, --trace-out, --ghost) start in the race; P's relaunch already
passes --skip-title. audio.rs: direct_music runs only on the race screen.
Oracle: the title is reached from slot 1 by DOWN x4 + CROSS (BACK TO MAIN
MENU) -> course 0x1E, mode 0x10 at VB ~406; slot 11 not used.
(2026-10-14, T) Commits 9b42af7, 408cd55: title complete except its karts
(docs/spec/frontend.md). P: the title's six karts are race karts on course
0x1E in game mode 0x10 (AI with no chunk-5 course points; FUN_80082708 /
FUN_8002976C / FUN_80067E08 have 0x1E cases); if mr-sim ever runs that
session, title.rs can draw them and FUN_80091B8C's logo bounce. H: title.rs
keeps its own VramTextures / cameras (render layers 6, 7) and a
reverse-subtract material for ABR 2; it uses animtex::frame_for (please keep
the signature).
### S
(2026-10-14, S) Commits 056ce0c, 16d70d8, 729498c, 8f157d7, 23c2988 (mr-sim
weapons.rs, session.rs `SessionConfig::course_load_seed` (one field, default
None, no behaviour change), mr-course-check, mr-survey, tools/oracle
capture_poked.py / capture_menu_lifts.py; ledger "Session 6 (track S)",
courses.md table). All 18 course free runs now exact to the end of the
captures. For R / H: a projectile that breaks scenery (wall, floor or
ceiling) now ends its step at once (no ground snap), so its last position
is where it hit; `objects.hidden_slots` / `group_enabled` unchanged in
meaning. For P: AI stuck spots on NEW_YORK / HAPPY are the game's own
(oracle pokes), no recovery needed.

### R
(2026-10-14, R) Commit 3a2ef9b: the courses' moving scenery (mr-sim
scenery.rs = 0x800D8630 animated objects; mr-bevy movers.rs), weather drops,
FUN_8007A9DC VRAM scrolls (animtex.rs), projectile shadows
(kartfx::projectile_decals), DISCO damage gauge (hud_items::damage_gauge;
RaceGlobals::hud_gauge_rgb). The full view no longer drops special slots
(only the pickups 0x20..). Checker: `cargo run --release --offline -p mr-sim
--bin mr-scenery -- [NAME] [--slots]`. Dev: MRM_LOOK, MRM_NO_SCROLL,
MRM_MOVERS_DEBUG. For whoever takes it: DISCO's palette differs from the
oracle (load-time CLUTs, course flag 0x10 path); GLACIER's port frame lacks
the minimap.
(2026-10-14, T, follow-up) Commits 5d73c0e, c46dbbf, a4e438b (+ docs): the
title's karts run in mr-sim (title.rs; small hunks in session.rs, init.rs,
camera.rs, race.rs, progress.rs, update.rs, world.rs: all title-gated or the
race type 2 / battle paths; races unchanged, tests green). mr-bevy: on the
title the run's course is 0x1E (courses.rs choice / session_config; the
pick-list opens on START, not at start, and does not pause the title),
main.rs select_objects uses the game's selection on the title, scene.rs
update_banners takes Option<BannerState>. S: lap_timing now takes `rand`
(tests/fixtures.rs updated).
(2026-10-14, R, follow-up) Closed: DISCO "palette" (no tint: same colours
with the oracle's camera on all 17 courses; the port's free run looks
elsewhere), minimap on RALLY / GLACIER (HUD tables were sized for courses
0..0x1F), projectile shadows exact vs the game's shadow list (OVAL). New
oracle helpers (lock-held, slot 11 only): tools/oracle/course_gp0.py,
course_ram.py, course_vram.py, gp0_polys.py.

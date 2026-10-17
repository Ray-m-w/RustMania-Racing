# Muppet RaceMania: final report (100%, Session 8, row 15, 2026-10-17)

## What the port is
A reimplementation of Muppet RaceMania (PS1, SLUS-01237, USA) in Rust with
Bevy 0.19.1 for window, rendering and audio output (port/, a Cargo
workspace: mr-disc, mr-assets, mr-sim, mr-audio, mr-video, mr-trace are
std-only; mr-bevy is the game). It contains no game data: models,
textures, scenery, music, sound, movies and the game's own tables are read
from the owner's disc at run time. The game logic (kart physics, AI, items,
every mode, menus, progression) is ported from the original code and was
verified against the psxrecomp build of the original (the oracle,
docs/notes/SESSION-2-oracle.md): simulation tick for tick, screens by GPU
packets / VRAM / screenshots. The psxrecomp build itself (the recomp,
dist/mrm-<ver>-windows-x64.zip, CLAUDE.md) is a separate product.

Every row of docs/PLAN-COMPLETE.md (0-15) is DONE, and gamepad support is
in (owner approved 2026-10-17).

## Install and run
- Package: `tools/package-port.ps1` (run from Git bash as
  `powershell.exe -NoProfile -ExecutionPolicy Bypass -File tools/package-port.ps1`)
  -> `dist/mrm-port-0.1.0-windows-x64.zip`, 18,150,886 bytes, SHA-256
  `bccc122b35e260a8d08398e58bdfc10560f0ec3440fb4b77b2a5902768983f94`.
  Exactly three files: muppet-racemania.exe (58,207,744 bytes),
  README-port.txt, THIRD-PARTY-LICENSES.txt (243 crates, 125 distinct
  licence texts). No disc data, no decoded assets, no captures (the script
  asserts the file list).
- The exe: Windows GUI subsystem (no console window; attaches to a parent
  console for tool output), C runtime linked statically: its imports are
  Windows system DLLs only (no VCRUNTIME / MSVCP / api-ms-win-crt /
  ucrtbase). Requirements: Windows 10/11 x64, a Vulkan or D3D12 GPU.
- Unzip anywhere and start muppet-racemania.exe. Without a disc a window
  asks for it (row 15, discpick.rs): type or paste (Ctrl+V) the path to the
  .cue (or .bin) and press Enter, or press 1..9 for a .cue found beside the
  exe or in the data folder; the path is opened with mr-disc, written to
  settings.txt as `disc = PATH` (only that line changes) and the game starts
  in the same process; Esc quits. The disc can also come from `--disc`,
  `$MRM_DISC` or (development) game.toml. A .chd must be extracted with
  chdman first. The track file is checked once against Redump disc 16664
  track 1 (SHA-1 1b0fc0b6...; a mismatch warns in the window title, never
  refuses; disc-check.txt).
- Data folder `%APPDATA%\MuppetRaceMania\`: settings.txt, save.bin,
  crash.txt, disc-check.txt; `--data-dir PATH` or a `portable.txt` beside the
  exe change it. An existing save.bin / settings.txt beside the exe is
  COPIED in once; nothing is deleted. A PS1 memory card image can be
  imported once (saves\card1.mcd, `--import-card`).
- Controls: keyboard or gamepads (two pads for two players; rumble). Player 1: arrows,
  X accelerate, S brake, Z jump, A camera lock-on / back, Enter START,
  Q / W L1 / R1, E / R L2 / R2; player 2: J / L / I / K, N, B, M, H,
  Backspace, U / O, Y / P (README-port.txt).
- Development: `tools/play-port.ps1` (builds and runs from the repo).

## Modes and features
- Front end: power-on movies, title (logo scene, PRESS START, cheat-code
  entry), main menu, film-strip course select with backdrop and props,
  world / course lists, course and game menus, character / vehicle select
  (one and two players), pause + YES / NO, OPTIONS (ADJUST SOUND, SET
  CONTROLLERS, ADJUST SCREEN, VIBRATION, MEMORY CARD over save.bin, PORT
  SETTINGS), MUSIC, the game's fades and LOADING pictures, in-process
  course change (no relaunch), GAME STATUS.
- Racing: all 18 race courses, 3 laps against five AI opponents (four on
  overhead-camera courses), items, specials, weather, lap records / BEST
  LAP; TOURNAMENT leagues A / B / random (1 or 2 players), standings with
  the 3D line-up, tie-break race and trophy presentation.
- Stunt: the 1 PLAYER STUNT TRIAL on the six stunt courses (gates, clock,
  course record, mirror walls, banners), 2 PLAYER STUNT RACE.
- Battle: 1 / 2 PLAYER BATTLE on the ten arenas, BATTLE LEAGUE, CHICKEN
  CHASE (two players).
- ADVENTURE (game mode 0xB) and its "BEAT x" race; COURSE SECRETS: BOSS
  CHALLENGE, BATTLE AS BOSS, DOGFIGHT, DOOZER BATTLE, REVERSE RACE, BATTLE /
  BATTLE RACE on race courses, the reversed trial, PLAY MOVIE, the
  "n PLAYERS" choices.
- Two players: split screen, simultaneous character select, each player's
  own camera pass (billboards, pickups, shadows, impostors, draw distance),
  audio listeners, split minimap / progress bar / chicken icon.
- Attract demo, THE END (credits roll, "?" picture, the six secret
  courses), the six cheat codes, MEET THE MUPPETS gallery and TEST DRIVE,
  every menu and race voice clip, all 30 FMV movies (mr-video: STR, MDEC,
  XA), music and sound effects from the disc, save.bin progression and
  unlocks.

## Options (settings.txt / OPTIONS -> PORT SETTINGS / `--set key=value`)
faithful_view (the PlayStation's look in one switch), faithful_draw_distance,
aspect (native widescreen / 4:3), msaa, texture_filter, lighting
(faithful / enhanced), resolution, fullscreen, vsync, fx_blend, fx_depth_test,
logic_hz (60 / 30), render_latency, low_latency, disc, course,
skip_intro_movies, skip_title, music_volume, sound_volume,
controller_config(_2), vibration(_2), screen_x / screen_y, unlock_all.
Every key with its help: settings.rs `KNOWN` and the generated settings.txt.

## Verification summary (numbers from the logs and handoffs of Sessions 2-8)
| area | result |
|---|---|
| Kart physics (S2) | DOCK lap, 1,538 ticks: 0 error on all 34 trace fields fed the oracle's inputs; mr-bevy live 507 ticks exact |
| Race flow + AI (S3) | player exact over a 3-lap race (3,510 ticks); AI decisions match on 23,202 / 23,206 teacher-forced kart-ticks |
| Items (S4) | six karts free-running exact over 4,815 ticks on 4 races; item HUD 9,714 / 9,714 GP0 primitives; item key-ons 308 / 308 (P13) |
| Audio (S4, P9, P11) | music / sfx from the disc; movie audio correlation 1.0000 at CdMix 0x70; XA file / channel / LBA equal for 7 clips |
| Graphics (S5, S6, P14) | PS1 blend modes, widescreen HUD at 4:3 / 16:9 / 21:9; environment-mapped primitives GP0 UVs 129 / 129 + 149 / 150; every course's VRAM scrolls |
| Courses (S6, row 15) | analysis/oracle/courses' 21 captures (17 race courses, arena LAGOON, stunt MOUNTAIN / REST / ROOFTOPS) + DOCK (start_probe) = all 18 race courses: race load exact, free run exact to the end (31,394 + 834 frames), teacher forced motion / camera exact (row-15 re-run) |
| Front end (P1, P12) | GP0 font match on 7 screens; LOADING and fades exact; course-select backdrop within 1 px |
| Saves + leagues (P2, P6b) | progression FUN_8008BE68 and battle points matched; save block = RAM; GP0 5 screens |
| Stunt (P3, P13, P14) | MOUNTAIN, ROOFTOPS reversed, REST mirrors: free run / teacher forced exact; HUD 120 / 121 |
| Battle (P4, P13) | LAGOON 3 captures exact incl. 3 knock-outs; result 116 / 116; BATTLE on DOCK exact |
| Adventure + secrets (P6, P6b, P13) | DOCK adventure exact, GP0 88 / 88; boss_lagoon / reverse_dock / doozer_battle exact (619 / 726 / 636 frames); DOGFIGHT, BATTLE AS BOSS, reverse lap exact |
| Two players (P7, P13) | DOCK 2P 1,024 frames, VOLCANO CHICKEN CHASE 1,419 frames exact for both karts and cameras; result panels GP0 324 / 324, 329 / 329, 174 / 174, 104 / 104; character select 55 / 55 |
| Attract, credits, codes (P8) | demo free run exact 290 frames; credits 118 / 118 glyphs; ARCHES code save block = RAM |
| FMV (P9) | all 30 movies decode; 11 VRAM regions 32,768 / 32,768 bytes each |
| MEET THE MUPPETS (P11) | gallery GP0 28 / 28 and 16 / 16; RAM records exact |
| Fresh title (P14) | 918 / 918 ticks exact with the oracle's power-on history |
| Stability (all) | row-15 sweep: mr-fuzz every mode at 30 and 60 Hz, 1,212 runs, 21,998,356 ticks, 0 panics; earlier rows 1,520 + 840 + 272 + ... runs, 0 panics; crash.txt instead of a freeze |
| Tests | `cargo test --workspace --offline --release`: 64 suites, 201 passed, 0 failed, 1 ignored |
| Package (row 15) | zip unpacked into a temp folder, temp APPDATA, MRM_DISC unset, ONE process: disc picker -> settings.txt written (disc check match) -> title -> main menu -> SELECT COURSE (SWAMP) -> LOADING + the course's first-load movie -> course menu (in-process course change, 160 ms) -> character select -> race (LAP 1/3, 0:01.3). Repo build, one process each: course menu -> GAME MENU -> CHANGE COURSE -> GHOST -> BACK TO MAIN MENU -> title; OVAL race to the 10-minute limit -> results -> START -> course menu -> BACK TO MAIN MENU -> title. Game's own --menu-shots only |

Row-15 sweep details (docs/notes/2026-10-17.md "Session 8, phase 15"):
mr-fuzz --seeds 3: races / stunt / arenas / title 210 runs, --adventure 210,
--beat 210, --secrets 204, --demo 168, --two 204, --test-drive 6. Course
check extra captures: secrets/boss_lagoon, reverse_dock, battle_dock,
reverse_rooftops exact; DOCK start_probe exact but for its known 48
teacher-forced random-state frames (the Session-3 capture holds only 0x40
bytes of the particle list; docs/spec/courses.md, unchanged since before
phase 14).

## Deliberate differences (by design)
- 60 Hz game logic (dt = 1 per tick, kart-physics-ledger.md "60 Hz
  mode") is the default and a feel match of the original's 30 Hz, not a
  tick-exact one; `logic_hz = 30` is the original timing, verified tick
  for tick. Low-latency input sampling with a look-ahead display is on by
  default (pacing.rs).
- Enhancements are options and on by default: native widescreen with the
  HUD pinned to the edges, smooth texture filtering, enhanced lighting,
  MSAA, whole-course draw distance; `faithful_view = true` gives the
  PlayStation's look (4:3, nearest, faithful lighting and draw distance).
- Rendering is per-pixel depth on a GPU, not the PS1's ordering-table
  draw order; PS1 blend modes and the ordering-table pushes are emulated
  where they show (render-ledger.md).
- Saves: save.bin in the data folder instead of memory card prompts
  (autosave; the MEMORY CARD page saves / loads it; card images import
  once, read only).
- The disc is read from a .cue / .bin dump; a .chd is not read (extract
  with chdman). MIDWAY (movie 0x1E) has no file on the disc; CRDIT_01.XA
  is unreferenced by the game's code.
- ADJUST SCREEN moves the picture inside the window's border only.

## Open item
- Gamepads: done 2026-10-17 (bevy_gilrs, BINLIST); input.rs maps pads 1
  and 2, rumble.rs drives VIBRATION. Not yet tried with a physical pad by
  the lead (the owner's first play is the check).

## Development and oracle tooling (not part of the game)
- Oracle: the psxrecomp build-dev with the dev-only bridge plugin, TCP
  4370, tools/oracle/*.py (captures, routes such as goto_course.py), the
  local captures under analysis/oracle/ (never committed).
- mr-sim bins: mr-course-check (race load / free run / teacher forced per
  capture; `--cap`, `--dir`, `--adv-load`), mr-fuzz (headless stability,
  every mode, 30 / 60 Hz), mr-verify, mr-items, mr-particles, mr-title,
  mr-scenery checks, the AI harness; mr-trace's mr-compare.
- mr-bevy tool flags: `--shot`, `--title-shot`, `--menu-keys` /
  `--menu-shots`, `--picker-keys` / `--picker-shot`, `--replay`,
  `--replay-capture`, `--ghost`, `--oracle-grid`, `--title-oracle`,
  `--record-route`, `--trace-out`, `--boot-soak`, `--fps-log`,
  `--latency-log`.
- Run-time code uses no oracle, TCP or analysis/ path outside
  `#[cfg(test)]` (phase-10 retirement check, still true).

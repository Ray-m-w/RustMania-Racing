# Muppet RaceMania: final report (Session 7, phase 10, 2026-10-16)

## What the port is
A reimplementation of Muppet RaceMania (PS1, SLUS-01237, USA) in Rust with
Bevy 0.19.1 for window, rendering and audio output (port/, a Cargo
workspace: mr-disc, mr-assets, mr-sim, mr-audio, mr-video, mr-trace are
std-only; mr-bevy is the game). It contains no game data: models,
textures, scenery, music, sound, movies and the game's own tables are read
from the owner's disc at run time. The game logic (kart physics, AI, items,
modes, menus, progression) is ported from the original code and was
verified tick for tick against the psxrecomp build of the original (the
oracle, docs/notes/SESSION-2-oracle.md). The psxrecomp build itself (the
recomp, dist/mrm-<ver>-windows-x64.zip, CLAUDE.md) is a separate product.

## How to run it
- Package: `.\tools\package-port.ps1` -> dist/mrm-port-0.1.0-windows-x64.zip
  (17.9 MB): muppet-racemania.exe, README-port.txt, THIRD-PARTY-LICENSES.txt
  (243 crates, 125 distinct licence texts). Nothing else: no disc data, no
  decoded assets, no captures (the script checks the file list).
- Unzip anywhere, start muppet-racemania.exe. Without a disc a screen says to
  set `disc = <your .cue>` in `%APPDATA%\MuppetRaceMania\settings.txt` (the
  file is created) and the program exits. The disc can also come from
  `--disc`, `$MRM_DISC` or (development) game.toml. The track file is checked
  once against Redump disc 16664 track 1 (SHA-1 1b0fc0b6...; a mismatch
  warns, never refuses; disc-check.txt).
- Data folder `%APPDATA%\MuppetRaceMania\`: settings.txt, save.bin,
  crash.txt, disc-check.txt; `--data-dir PATH` or a `portable.txt` beside the
  exe change it. The first start COPIES an existing save.bin / settings.txt
  from beside the exe (port/target/release) into it; nothing is deleted.
- Development: `.\tools\play-port.ps1` (builds and runs from the repo).
- Requirements: Windows x64, Vulkan or D3D12 GPU, the MSVC 2015-2022
  runtime (VCRUNTIME140.dll; the exe links it dynamically).

## Oracle retirement check (phase 10)
- Run-time code: no psxrecomp, TCP (4370), oracle bridge or analysis/ path
  is used outside `#[cfg(test)]` (grep of mr-bevy, mr-sim, mr-disc,
  mr-assets, mr-audio, mr-video sources; the only `mr_sim::oracle::repo_root`
  calls are in tests). The exe imports only Windows system DLLs, the MSVC
  runtime and the GPU APIs.
- Run: the zip unpacked into a temp folder outside the repo, MRM_DISC unset,
  no game.toml above it, APPDATA pointed at a temp folder holding only
  `settings.txt` with `disc = F:\Muppet Racemania\extract\muppet-usa.cue`.
  The game's own outputs: `--title-shot` (title at frame 400: logo scene),
  `--menu-keys/--menu-shots` (START -> main menu -> SELECT COURSE: the film
  strip on SWAMP), `--course BOG --menu` + keys (1 PLAYER RACE -> character
  select -> race running, LAP 1/3, 0:01.1). The disc check reported a match;
  save.bin and disc-check.txt were written into the data folder. Migration
  checked with the repo build and `--data-dir` (save.bin copied byte-equal,
  the original kept). No desktop screenshots.
- Still needs the oracle (development and verification only, never the
  game): tools/oracle/*.py and the dev-only bridge plugin in the psxrecomp
  build-dev; captures under analysis/ (local); mr-sim's `oracle` module and
  the verification bins (mr-ai harness, mr-items, mr-verify, mr-course-check,
  mr-particles, mr-title, mr-scenery's checks); mr-trace's mr-compare;
  mr-bevy's tool flags (`--oracle-grid`, `--title-oracle` (embedded seed
  constants only), `--replay`, `--replay-capture`, `--ghost`,
  `--record-route`, `--trace-out`); tests that compare with the local captures; tools/compare-drive.ps1, fmv_check.py.

## Verification summary (numbers from the handoffs and logs)
| area | result |
|---|---|
| Kart physics (S2) | DOCK lap, 1,538 ticks: 0 error on all 34 trace fields fed the oracle's inputs; mr-bevy live 507 ticks exact |
| Race flow + AI (S3) | player exact over a 3-lap race (3,510 ticks); AI decisions match on 23,202 / 23,206 teacher-forced kart-ticks; opponent lap times within 1.1 % (pooled +0.11 %) |
| Items (S4) | six karts free-running exact over 4,815 ticks on 4 races; item HUD 9,714 / 9,714 GP0 primitives |
| Audio (S4, S9) | music / sfx from the disc; movie audio correlates 1.0000 with the decoder at CdMix 0x70 |
| Graphics (S5, S6) | PS1 blend modes in gamma space, water within 0.5-2 %, widescreen HUD at 4:3 / 16:9 / 21:9; 60 Hz logic default (30 Hz exact) |
| Courses (S6) | all 18 race courses: race load exact, free run exact to the end of every capture (~1,630 frames) except two weather frames |
| Stability (P0) | 1,520 fuzz runs, 0 panics; crash.txt instead of a freeze |
| Front end (P1) | menus GP0 font match on 7 screens |
| Saves + leagues (P2) | progression FUN_8008BE68 matched; save block = RAM; GP0 5 screens |
| Stunt trial (P3) | MOUNTAIN, 3 captures: free run / teacher forced exact; HUD 120 / 121 |
| Battle (P4) | LAGOON, 3 captures exact incl. 3 knock-outs; result 116 / 116 |
| Adventure (P6) | DOCK race load / free run / teacher forced exact; GP0 88 / 88 |
| Course secrets (P6b) | boss_lagoon / reverse_dock / doozer_battle exact (619 / 726 / 636 frames); boss HUD 129 / 130 |
| Two players (P7) | DOCK 2P 1,024 frames and VOLCANO CHICKEN CHASE 1,419 frames exact for both karts and cameras; split HUD 140 / 186; 204 fuzz runs 0 panics |
| Attract, credits, codes (P8) | demo free run exact 290 frames; credits 118 / 118 glyphs; ARCHES code save block = RAM |
| FMV (P9) | all 30 movies decode; 11 VRAM regions 32,768 / 32,768 bytes each |
| Ship (P10) | package, data folder, disc handling, oracle-independent run (above); `cargo test --workspace --offline --release` green (62 suites) |

## Remaining differences and leftovers (concise)
From PLAN-COMPLETE.md and each phase's "Left" list:
- Front end: MEET THE MUPPETS (gallery + the menus' only voice clips);
  fades / LOADING between menus; course-select sky backdrop and world props;
  in-process course change (another course relaunches the program); the
  scene reload after an in-menu movie; greyed: ADJUST SCREEN, MEMORY CARD,
  VIBRATION, MOVIE / MUSIC.
- Modes: simultaneous 2P character select, the secrets' "n PLAYERS" choices,
  2-player TOURNAMENT (3 karts), split minimap / chicken icon, player-2
  billboards and draw distance; league tie-break and presentation,
  standings' 3D line-up; MIDWAY movie (no file); CRDIT_01.XA (caller not
  found); race voice clips (lap, battle win, catches, standings); THE END's
  floor render and its "?" picture.
- Unchecked against the oracle: 2P battle / stunt race, DOGFIGHT, BATTLE AS
  BOSS, BATTLE on race courses, reversed trial, reverse lap completion, the
  Doozer goal marker, a second stunt course, movie audio vs the oracle.
- Accuracy loose ends: fresh-title random byte; GRAVE / BOG weather frames;
  DISCO floor animation timing; GRAVE far pickup; item sound key-on; AI R2
  specials; stunt banner animation and mirror reflections.
- Ship: the exe is a console program (a console window opens; useful for
  the disc warning, could become a GUI subsystem build); dynamic MSVC
  runtime; no .chd reading (extract with chdman); version 0.1.0 unchanged.
- Deferred by the owner: DLSS. Optional: AI personalities, control
  remapping.

## To reach 100%
1. MEET THE MUPPETS and the race / menu voice clips.
2. In-process course change with the game's fades and LOADING screen.
3. Two-player completeness (simultaneous select, 2P tournament, split
   minimap, player-2 billboards) and the unchecked modes on the oracle.
4. The accuracy loose ends above (each a short capture).
5. Ship polish: GUI-subsystem exe, static CRT, an in-game disc picker.

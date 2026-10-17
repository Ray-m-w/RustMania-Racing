# Muppet RaceMania: Rust + Bevy port

A reimplementation of Muppet RaceMania (PS1, SLUS-01237) in Rust, using the
assets on the owner's disc and verified tick for tick against the psxrecomp
build of the original (the oracle). Complete (row 15, 2026-10-17): the
whole game - title, menus, every mode (races, tournaments, stunt trial,
battles, adventure, course secrets, two players, MEET THE MUPPETS / TEST
DRIVE, the attract demo, THE END, cheat codes, movies), saves and options.
Keyboard only (gamepad support pending: the build has no gamepad backend).
Final report: ../docs/notes/FINAL-REPORT.md; plan: ../docs/PLAN-COMPLETE.md.

## Play
```
..\tools\play-port.ps1
```
or `cargo run --release -p mr-bevy` from this directory. The disc (a .cue or
its .bin; not a .chd), first match wins: `--disc PATH`, the `disc` key of
settings.txt, `$env:MRM_DISC`, the `disc = "..."` line of ../game.toml
(phase 10, discsel.rs). With none (row 15, discpick.rs) a window asks
for it: type or paste (Ctrl+V) the path and press Enter, or press 1..9
for a .cue found beside the exe or in the data folder; a readable disc is
written to settings.txt as `disc = PATH` and the game starts (no restart);
Esc quits. Tool flags: `--picker-keys "PATH<enter>"` (`<paste>`, `<bs>`,
`<esc>`; no window) and `--picker-shot F`; `--shot F` writes the picker's
picture. The track is checked once against Redump disc 16664
track 1 (SHA-1, BINLIST.toml; disc-check.txt): a mismatch warns, never
refuses.

Data folder (phase 10, paths.rs): settings.txt, save.bin, crash.txt and
disc-check.txt live in `%APPDATA%\MuppetRaceMania\` (`--data-dir PATH`
overrides; an empty `portable.txt` beside the exe keeps them beside the
exe). The first start copies (never moves) an existing save.bin /
settings.txt from beside the exe (e.g. port/target/release) when the
folder has none. "Beside the exe" below means this folder.

Standalone package (phase 10): `..\tools\package-port.ps1` ->
dist/mrm-port-<version>-windows-x64.zip (the exe as muppet-racemania.exe,
README-port.txt from package/, THIRD-PARTY-LICENSES.txt generated from
cargo metadata + the registry's licence files; no game data).
Keyboard: arrows steer, X accelerate, S brake/reverse, Z jump, A
(TRIANGLE) locks the camera on the nearest kart while held (Session 7),
Enter = START (pause).
Gamepad (keyboard only for now: gamepad support pending the owner's approval of Bevy's gilrs backend; mapped in input.rs): right trigger / south accelerate, left trigger / east brake, west jump.

Two players (phase 7; the course menu's 2 PLAYER RACE / BATTLE / STUNT
RACE, CHICKEN CHASE): split screen, player 1 on top. The first gamepad
plays player 1, the second player 2 (with one player every pad plays
player 1). Player 2 on the keyboard (the fallback with one gamepad or
none): J / L steer, I / K up / down, N accelerate (CROSS), B brake
(CIRCLE), M jump (SQUARE), H (TRIANGLE), Backspace = START, U / O = L1 /
R1, Y / P = L2 / R2. Player 1 keeps the keys above. Phase 13: both players
pick at once, as the game does (a column each; CROSS = READY, TRIANGLE
un-readies, with nobody ready it leaves; a character the other player
has already picked shows ALREADY PICKED); COURSE SECRETS' rows take LEFT /
RIGHT for "- 1 PLAYER" / "- 2 PLAYERS" (REVERSE RACE, BATTLE, BATTLE RACE,
the reversed stunt race, DOGFIGHT, DOOZER BATTLE); the main menu's
TOURNAMENT - n PLAYERS (LEFT / RIGHT) plays a league with two players
(three karts: both players and one opponent; no save result). Split
screen draws each view as the game's own pass: billboards, pickups, the
chicken, kart shadows and the far-kart impostors from that player's
camera, and with `faithful_draw_distance` each eye's own scenery
selection at the split screen's draw distances; the minimap (with the
chicken) or a stunt course's progress bar sits at the split. SET
CONTROLLERS sets controller 2's layout with pad 2's LEFT / RIGHT
(`controller_config_2`). Test runs: `mr-bevy --course DOCK --skip-title
--players 2 [--race-flags 64]`; menus: `--menu-keys` / `--menu-keys2`
(pad 1 / pad 2 scripts, "VB:keys,...").

The game's title screen comes first (Session 6, track T; ../docs/spec/frontend.md):
the frontend course run as the game runs it (the logo scene, three AI karts
knocking the letters about, the orbiting camera, the title music, PRESS START).
START (Enter / pad Start) opens the game's menus (Session 7, menus.rs;
frontend.md "Menus"):
- main menu: SELECT COURSE (the film-strip course select: UP / DOWN world,
  LEFT / RIGHT course, CROSS loads it), TOURNAMENT - 1 PLAYER (the leagues,
  below), OPTIONS (ADJUST SOUND: music and sound volume 0..15; SET
  CONTROLLERS: configuration A / B; PORT SETTINGS: the view and logic
  settings below as a list, LEFT / RIGHT changes, CROSS writes settings.txt,
  applied on the next start);
- the course menu (over the course): 1 PLAYER RACE -> character select (UP /
  DOWN character, LEFT / RIGHT vehicle, CROSS races; LEFT / RIGHT on the row
  sets 1..10 laps once the course is won), GAME MENU -> CHANGE COURSE (world
  and course lists), BACK TO MAIN MENU;
- in a race START pauses: CONTINUE, RETIRE (to the course menu), RESTART,
  ADJUST SOUND; CROSS / START on the results goes back to the course menu;
- TOURNAMENT - 1 PLAYER (Session 7 phase 2, league.rs; frontend.md
  "Leagues"): RACE LEAGUE A / B (each world's race A / race B course) and
  RANDOM RACE LEAGUE (opens once A and B are won); CROSS loads the first
  course, the character select draws the five opponents, then each course
  shows START RACE n / POINTS / QUIT TOURNAMENT; after each race RACE POINTS
  (place 3 / 2 / 1, fastest lap, best start, most hits) and TOURNAMENT
  POINTS; after six races the result (won when first in the table, best
  points kept);
- the stunt courses (phase 3: the game's only timed mode, its "time
  trial"; ../docs/spec/stunt-trial.md): a world's STUNT course (course
  select / course list) -> 1 PLAYER STUNT TRIAL -> character select -> the
  trial: one kart through the course's gates against the clock (FINISH THE
  COURSE, the progress bar, the timer; the strip camera), the result
  (COURSE FINISHED and the time, or OUT OF TIME after 10 minutes); a time
  under the course's BEST TIME becomes the record (save.bin);
- the arenas (phase 4, ../docs/spec/battle.md): a world's BATTLE course
  (CANYON, T_SQUARE, THEATRE, CHAMBER, LAGOON, LAB; SECRET COURSES' BATTLE;
  VOLCANO, HOUSE, WHATEVER once everything is complete) -> 1 PLAYER BATTLE
  -> character select -> six karts (five on the last three) battle: damage
  from contacts and weapons (the heart gauge), a kart at 1000 damage is
  knocked out, the last one standing wins (the clock at the top); the
  result cycles DAMAGE / BEST CRASH / KNOCKOUTS with the place and time; a
  win faster than the course's BEST TIME becomes the record, and the win
  opens the world's stunt course;
- ADVENTURE (phase 6, ../docs/spec/adventure.md): course menu -> GAME MENU
  -> ADVENTURE -> character select -> the course alone against the clock
  (4:00; arenas 3:00, stunt courses 10:00) with the Doozer and the course's
  character parked somewhere: collect every item box (stars), every turbo
  token (food), the ten targets (tokens) or the movie object, push the
  Doozer onto its goal, or touch the character for its "BEAT x" race (win:
  the character is yours); the first find ends the run, the HUD counts what
  is left; finds and characters go to save.bin.
- <COURSE> SECRETS (phase 6b, course menu row 2; adventure.md "COURSE
  SECRETS"): what an ADVENTURE's finds open - REVERSE RACE, BATTLE (the
  course as an arena), BATTLE RACE (damage) on race courses; the reversed
  stunt trial; in the arenas BOSS CHALLENGE (the course's boss(es):
  SWEETUMS, BEAUREGAARD, BILL / GILL / JILL, STATLER / WALDORF, THE CHIEF,
  VAN NEUTER on their boss vehicles, 5:00 counting down; a win opens BATTLE
  AS BOSS) and BATTLE AS BOSS (LEFT / RIGHT picks the boss); on
  ARENA/BATTLE DOGFIGHT (everyone in a PLANE) and DOOZER BATTLE; "?" until
  found;
- GAME STATUS (main menu / game menu): the course grid, each course's
  secrets, records and the leagues; BATTLE LEAGUE in TOURNAMENT (the six
  arenas, points for place, best crash, knock-outs and damage); the world-7
  and cup-8 courses' own menus (REVERSE RACE / BATTLE / BATTLE RACE, BACK
  TO CREDITS = THE END).
- Phase 8 (frontend.md "Phase 8"): left idle 1800 VBlanks (30 s) on the
  title or the main menu, the attract demo: a random course with six AI
  karts, trackside cameras, "DEMO" / "PRESS START"; START goes to that
  course's menu (or the title when the course is not open yet), after one
  minute it returns to the title by itself (`--no-demo` turns it off).
  Cheat codes: ten face-button presses on the title (PRESS START screen;
  keyboard Z = SQUARE, A = TRIANGLE, S = CIRCLE, X = CROSS) - the game's six
  codes (MUPPETS AND VEHICLES, STUDIO, ARCHES, FRAGGLE ROCK, END COURSES,
  RESET; frontend.md "Cheat codes" lists the sequences); a code changes
  save.bin at once. OPTIONS -> THE END (once SPEEDWAY / RALLY / GLACIER or
  everything is open): the credits roll over Kermit's kart, START opens the
  secret courses (SPEEDWAY, RALLY, GLACIER, VOLCANO, HAUNTED HOUSE,
  WHATEVER) and BACK TO MAIN MENU (`--ending` starts there).
- Phase 11 (frontend.md "MEET THE MUPPETS", audio.md "Phase 11"): MEET THE
  MUPPETS on the main menu: the 5 x 5 gallery of the characters (those not
  collected black, "?"), each speaking when the cursor rests on it, the
  camera moving in while it does; CROSS = TEST DRIVE: drive the character
  round the frontend room in its vehicle (START / SELECT: the other
  vehicles it has won; the three ratings; TRIANGLE back), random course
  music. Every voice clip the game plays: the finish, knock-outs, the stunt
  finish, a battle won, CHICKEN CHASE's win, a BOSS CHALLENGE's boss, the
  results taunt after a win (Statler and Waldorf's faces on the results).
Menu keys: arrows move, X (CROSS) selects, A (TRIANGLE) goes back; pad: D-pad,
south, north. Row 12: another course (or the title) loads in place behind
the game's fades and LOADING picture (boot.rs / loading.rs: the boot
arguments become `--course NAME --skip-title --menu --character C
--vehicle V`, `--league` within a league; the course's scene is rebuilt;
settings.txt is read again). OPTIONS -> ADJUST SCREEN moves the picture
(`screen_x` / `screen_y`), SET CONTROLLERS has VIBRATION per controller
(`vibration` / `vibration_2`: gamepad rumble requests; this build has no
gamepad backend, see rumble.rs), MEMORY CARD saves / loads save.bin on
demand; the course select's MUSIC plays the course's track and shows the
game's backdrop and props; a league ends with the tie-break race when the
player shares first place and the presentation on OVAL. Test flags:
`--boot-soak N` (N course changes, entity / asset / memory lines),
`--loading-shot PATH`, `--open-page courses:W,C | standings:N`.
Saves (phase 2, save.rs; frontend.md "Saves"): `save.bin` in the data folder (or
`--save PATH`) holds the game's own save block (unlocks, lap records, won
courses, leagues, laps, options) with a version header and the game's
checksum; it is written automatically whenever something changes (no MEMORY
CARD prompts). With no save.bin the game's save on a PS1 memory card image
is imported read-only (`saves/card1.mcd` in the data folder or beside the exe, or `--import-card
PATH`), else a new game starts. `unlock_all = true` shows everything open
without touching save.bin (default false). The menus' settings rows:
`music_volume` / `sound_volume` (0..15, 12 = the game's level),
`controller_config` (A / B), `unlock_all`.
`--skip-title` (or `skip_title = true` in settings.txt) starts in the race
(`--menu` with it: in the course's menu); the tool flags below
(--oracle-grid, --route, --replay, --shot, ...) always start in the race and
never show menus.
Title check flags: `--title-oracle` (the oracle run's title: its rand() seed and
water state), `--title-shot F --title-shot-at N` (window capture of the title
frozen at frame counter N, then exit), `--title-press-start N` (START at N).
Menu check flags: `--menu-keys "VB:buttons,..."` (pad buttons held from menu
VBlank VB, route names: cross, start, down+cross, empty = none) and
`--menu-shots "VB:PATH,..."` (window captures, exit after the last).

mr-bevy options: `--autopilot` (AI drives Kermit), `--oracle-grid` (solo
session aligned to the oracle's grid savestate; `--leader` there = no catch-up), `--record-route F` / `--trace-out F` (oracle loop),
`--route F` (scripted input), `--replay TRACE` (show an oracle trace),
`--ghost TRACE` (oracle kart as a ghost, drift in the title bar),
`--shot F --shot-at TICK` (screenshot and exit),
`--course NAME` with `--skip-title` (or `course = NAME` in settings.txt: DOCK,
BOG, NEW_YORK, ... or the course index), `--character N --vehicle N` (the
exe's indices; default Kermit / RACER).

View options (Session 5; `settings.txt` in the data folder, one `key = value`
per line, or `--set key=value`, or `--key-with-dashes [value]`; the command
line wins; every key with its help: `settings::KNOWN`):
| key | default | values | what |
|---|---|---|---|
| `faithful_draw_distance` | false | true/false | true = the game's per-frame selection (visibility stream, cull radius, layer-1 far stand-ins); false = the whole course every frame (layer-1 stand-ins of layer-0 scenery dropped, the one real backdrop kept) |
| `aspect` | native | native / 4:3 | native = the window's aspect, vertical FOV kept (73.7 deg), wider windows see more to the sides, HUD elements pinned to their left / centre / right; 4:3 = the game's view, pillarboxed |
| `msaa` | 4 | off / 2 / 4 / 8 | multisample anti-aliasing on every camera |
| `texture_filter` | smooth | nearest / smooth | nearest = PS1 point sampling; smooth = bilinear + mipmaps + anisotropic, cut-out edges without fringes (alpha to coverage with MSAA) |
| `lighting` | enhanced | faithful / enhanced | enhanced = haze toward the sky's horizon colour over distance (vertex colours unchanged; default by the owner's decision, 2026-10-13); faithful = the game's vertex colours only |
| `resolution` | 1600x900 | WxH | window size |
| `fullscreen` | false | true/false | borderless fullscreen |
| `faithful_view` | false | true/false | preset: all of the above at the game's look + a 1280x960 window (oracle side-by-sides) |

`--layer-report F` writes the draw-distance classification of every DAT object.

Logic rate and input latency (Session 5, track G; same settings mechanism):
| key | default | values | what |
|---|---|---|---|
| `logic_hz` | 60 | 30 / 60 | 30 = the game's logic (2 VBlanks per tick, exact against the original); 60 = the 60 Hz mode (1 VBlank per tick, the game's per-tick terms kept at their per-second rates; within the feel tolerance of 30: ../docs/spec/kart-physics-ledger.md section 12). Default 60 since 2026-10-14 (owner drove it); `--oracle-grid` runs always use 30 |
| `render_latency` | low | low / normal | low = Bevy's pipelined rendering off (each frame rendered right after its update) and at most 1 queued swapchain frame; normal = Bevy's defaults (render thread one frame behind, 2 queued frames) |
| `low_latency` | true | true/false | the pad is sampled every display frame (a tap between two ticks still counts) and the display shows the coming tick as soon as its buttons are known (it runs that tick on a throw-away copy of the session and interpolates toward it; the simulation is untouched). false = the Session 4 display (interpolated one tick behind) |

`--latency-log SECONDS` measures input-to-display latency (synthetic LEFT
presses; per press: sample -> first frame showing a tick computed with it,
and -> shown at full weight) and exits. 120 Hz display, app-side (the render
and present chain after the frame is the same in every mode):
| logic | low_latency | first frame shows the press (mean / p95 ms) | full weight (mean / p95 ms) |
|---|---|---|---|
| 30 Hz | false (Session 4) | 16.5 / 26.3 | 50.1 / 59.5 |
| 30 Hz | true | 1.5 / 1.9 | 11.2 / 26.2 |
| 60 Hz | false | 5.5 / 9.7 | 22.6 / 26.7 |
| 60 Hz | true | 1.0 / 1.6 | 3.9 / 9.2 |

`render_latency` (Session 5 finishing; 1920x1080, MSAA 4x, full draw distance,
30 Hz logic, low_latency on, 30 s each; the log also times the render world's
present call of the first frame showing the press; the swapchain queue after
it, 1 frame with `low` and 2 with `normal`, is not visible to the app):
| vsync | render_latency | sample -> present call, first frame (mean / p95 ms) | fps | frame p99 ms |
|---|---|---|---|---|
| on (120 Hz) | normal | 15.9 / 16.5 | 120.0 | 14.05 |
| on (120 Hz) | low | 8.2 / 8.4 | 120.0 | 9.96 |
| off | normal | 5.9 / 6.6 (6.2 / 7.2) | 331 (306) | 4.36 (4.87) |
| off | low | 3.6 / 4.7 (3.1 / 4.2) | 264 (300) | 5.79 (5.69) |

With vsync (the default) `low` presents one display frame earlier and drops
one queued frame, at no frame-rate cost; uncapped it trades ~10-20 % of the
throughput (no render thread) for ~2.5 ms. Default: low. `--latency-log` and
`--fps-log` keep the app at full rate without focus (Bevy's game default
updates an unfocused window at 60 Hz, which caps tool-launched runs).

Sound (bevy_audio; the race music and the voice clips stream from TUNES/*.XA
and VOICES/*.XA on the disc, the game's XA queue picks the track): `--no-audio`, `--no-music`,
`--volume V` / `--music-volume V` / `--sfx-volume V` (linear, 1.0 = the
game's levels), `--audio-stats` (stream counters on stderr), `--audio-dump F`
(the mix as a WAV on exit; game audio, keep it under ../analysis/).
Movies (phase 9, ../docs/spec/fmv.md): the HI / Traveller's Tales logos at
power-on, each course's movie on its first load, the course select's
SQUARE (MOVIE) and COURSE SECRETS' PLAY MOVIE once the course's movie
object is found. START / CROSS (Enter / X) skips as the game allows.
`skip_intro_movies = true` (settings.txt or PORT SETTINGS) skips the
power-on and course-load movies (the recomp's skip-fmv package skips all).
`--movie M` plays movie M at start (tests); `cargo run --release -p mr-video
--bin mr-str -- info` lists and decodes every movie (`png`, `wav`, `vram`:
a frame, the audio, a check against an oracle VRAM read).

`cargo run --release -p mr-audio --bin mr-xa -- info` lists the music
channels; `decode` / `compare` / `segments` check the decoder against the
oracle (../docs/spec/audio.md).

## Crates
| crate | what | deps |
|---|---|---|
| mr-disc | ISO 9660 reader for the disc image | std |
| mr-assets | RAW, DAT, TER, AXE, MDL, ANM decoders; VRAM model; specs in ../docs/spec/formats | std |
| mr-sim | the kart update, contacts, race progress, camera, standalone session; oracle harness | std |
| mr-trace | trace format, comparator (`mr-compare`), SVG plots | std |
| mr-audio | CD-XA music decoder and the game's XA queue (`xa`), SPU ADPCM / VAB / voice model | std |
| mr-video | STR movies: sector demux, v2 bitstream, MDEC (IDCT, YCbCr), the movie table and player rules (phase 9, ../docs/spec/fmv.md) | std (+ mr-disc, mr-audio; no BINLIST entry) |
| mr-bevy | window, input, rendering | bevy 0.19.1 |

Only mr-bevy depends on Bevy; the simulation is integer-only and never reads
the clock, files (except through mr-disc at load) or the renderer.

## Verify
```
cargo test --workspace --offline
cargo run --release --bin mr-verify     # needs captures in ../analysis/oracle
cargo run --release -p mr-sim --example feel60            # 60 Hz mode vs 30 Hz: the feel tolerance table (~30 s)
cargo run --release -p mr-sim --example hz30_fingerprint  # whole-state hashes of long 30 Hz races (diff before / after a sim change)
cargo run --release -p mr-sim --bin mr-survey             # AI races on every race course at 30 / 60 Hz: panics, laps, stuck karts
cargo run --release -p mr-sim --bin mr-course-check       # each course capture (../analysis/oracle/courses): race load, free run, teacher forced
cargo run --release -p mr-sim --bin mr-course-check -- --dir ../analysis/oracle/tri/DOCK_A DOCK   # a capture from a grid savestate (tools/oracle/capture_route.py): every kart, the camera, the random state; --seq for one-kart captures
cargo run --release -p mr-sim --bin mr-fuzz -- --seeds 40 --long 6   # button-mashing fuzz: every race course + the title, 30 / 60 Hz, past the finish and the 10-minute limit (~100 s on 12 threads)
cargo run --release -p mr-sim --example courses           # the course table and each course's objects
```

## If it stops on an error
The game never hangs on an error: any panic (in the simulation, e.g. a game
branch the port does not have yet, or anywhere in the app) or a failed
simulation tick closes the window, shows a message box (Windows) and writes
`crash.txt` in the data folder: the message, where it was raised, the
thread, the course, the app and simulation tick, race clock and game mode,
the command line and a backtrace. Exit code 101. Please send `crash.txt`
with a report. `MRM_NO_CRASH_DIALOG=1` (and the unattended tool flags
--shot, --route, --replay, ...) skip the message box. Debug builds only:
`MRM_TEST_PANIC=N` panics in the simulation system at app tick N
(`MRM_TEST_PANIC=session:N`: the failed-tick path), to check it.

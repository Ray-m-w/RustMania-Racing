# Muppet RaceMania: Rust + Bevy port

A reimplementation of Muppet RaceMania (PS1, SLUS-01237) in Rust, using the
assets on the owner's disc and verified tick for tick against the psxrecomp
build of the original (the oracle). Now: the game's title and its one-player
menus (Session 7), then a full 3-lap race on any of the game's 18 race
courses (Session 6; ../docs/spec/courses.md) as any character in any
vehicle against five AI opponents (four on the overhead-camera courses),
countdown and start game to results. Plan and rules: ../docs/SESSION-2.md; results:
../docs/notes/SESSION-2-report.md.

## Play
```
..\tools\play-port.ps1
```
or `cargo run --release -p mr-bevy` from this directory. The disc comes from
the `disc = "..."` line of ../game.toml or `$env:MRM_DISC` (a .cue or .bin).
Keyboard: arrows steer, X accelerate, S brake/reverse, Z jump, A
(TRIANGLE) locks the camera on the nearest kart while held (Session 7),
Enter = START (pause).
Gamepad: right trigger / south accelerate, left trigger / east brake, west jump.

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
  points kept).
Menu keys: arrows move, X (CROSS) selects, A (TRIANGLE) goes back; pad: D-pad,
south, north. Shown greyed and not selectable (later phases): MEET THE
MUPPETS, GAME STATUS, ADVENTURE, 2 PLAYER RACE, SECRETS, ADJUST SCREEN, MEMORY
CARD, VIBRATION, BATTLE LEAGUE, MOVIE / MUSIC, stunt and battle courses.
Another course (or the title) restarts the program on it (`--course NAME
--skip-title --menu --character C --vehicle V`, `--league` within a league;
the scene is built at start-up).
Saves (phase 2, save.rs; frontend.md "Saves"): `save.bin` beside the exe (or
`--save PATH`) holds the game's own save block (unlocks, lap records, won
courses, leagues, laps, options) with a version header and the game's
checksum; it is written automatically whenever something changes (no MEMORY
CARD prompts). With no save.bin the game's save on a PS1 memory card image
is imported read-only (`saves/card1.mcd` beside the exe, or `--import-card
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

View options (Session 5; `settings.txt` beside the exe, one `key = value`
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

Sound (bevy_audio; the race music streams from TUNES/*.XA on the disc, the
game's XA queue picks the track): `--no-audio`, `--no-music`,
`--volume V` / `--music-volume V` / `--sfx-volume V` (linear, 1.0 = the
game's levels), `--audio-stats` (stream counters on stderr), `--audio-dump F`
(the mix as a WAV on exit; game audio, keep it under ../analysis/).
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
`crash.txt` beside the executable: the message, where it was raised, the
thread, the course, the app and simulation tick, race clock and game mode,
the command line and a backtrace. Exit code 101. Please send `crash.txt`
with a report. `MRM_NO_CRASH_DIALOG=1` (and the unattended tool flags
--shot, --route, --replay, ...) skip the message box. Debug builds only:
`MRM_TEST_PANIC=N` panics in the simulation system at app tick N
(`MRM_TEST_PANIC=session:N`: the failed-tick path), to check it.

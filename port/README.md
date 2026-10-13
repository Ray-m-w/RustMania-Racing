# Muppet RaceMania: Rust + Bevy port

A reimplementation of Muppet RaceMania (PS1, SLUS-01237) in Rust, using the
assets on the owner's disc and verified tick for tick against the psxrecomp
build of the original (the oracle). Now: a full 3-lap race on DOCK, Kermit in
the RACER against five AI opponents, countdown and start game to results. Plan and rules: ../docs/SESSION-2.md; results:
../docs/notes/SESSION-2-report.md.

## Play
```
..\tools\play-port.ps1
```
or `cargo run --release -p mr-bevy` from this directory. The disc comes from
the `disc = "..."` line of ../game.toml or `$env:MRM_DISC` (a .cue or .bin).
Keyboard: arrows steer, X accelerate, S brake/reverse, Z jump.
Gamepad: right trigger / south accelerate, left trigger / east brake, west jump.

mr-bevy options: `--autopilot` (AI drives Kermit), `--oracle-grid` (solo
session aligned to the oracle's grid savestate; `--leader` there = no catch-up), `--record-route F` / `--trace-out F` (oracle loop),
`--route F` (scripted input), `--replay TRACE` (show an oracle trace),
`--ghost TRACE` (oracle kart as a ghost, drift in the title bar),
`--shot F --shot-at TICK` (screenshot and exit).

View options (Session 5; `settings.txt` beside the exe, one `key = value`
per line, or `--set key=value`, or `--key-with-dashes [value]`; the command
line wins; every key with its help: `settings::KNOWN`):
| key | default | values | what |
|---|---|---|---|
| `faithful_draw_distance` | false | true/false | true = the game's per-frame selection (visibility stream, cull radius, layer-1 far stand-ins); false = the whole course every frame (layer-1 stand-ins of layer-0 scenery dropped, the one real backdrop kept) |
| `aspect` | native | native / 4:3 | native = the window's aspect, vertical FOV kept (73.7 deg), wider windows see more to the sides, HUD elements pinned to their left / centre / right; 4:3 = the game's view, pillarboxed |
| `msaa` | 4 | off / 2 / 4 / 8 | multisample anti-aliasing on every camera |
| `texture_filter` | smooth | nearest / smooth | nearest = PS1 point sampling; smooth = bilinear + mipmaps + anisotropic, cut-out edges without fringes (alpha to coverage with MSAA) |
| `lighting` | faithful | faithful / enhanced | enhanced = prototype haze toward the sky's horizon colour over distance (vertex colours unchanged); not default until the owner decides |
| `resolution` | 1600x900 | WxH | window size |
| `fullscreen` | false | true/false | borderless fullscreen |
| `faithful_view` | false | true/false | preset: all of the above at the game's look + a 1280x960 window (oracle side-by-sides) |

`--layer-report F` writes the draw-distance classification of every DAT object.

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
```

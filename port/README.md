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

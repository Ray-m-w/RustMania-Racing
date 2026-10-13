# Session 5: coordination between the parallel tracks

Brief: docs/SESSION-5.md. Same mechanics as docs/notes/SESSION-4-coord.md
(commit identity and trailer, commit only your own paths and hunks — for a
shared file stage only your hunks on top of HEAD, oracle lock, no game data,
log in docs/notes/2026-10-13.md under "Session 5, <track>", stop rules).
Append to your own "Posted APIs" section; never rewrite another's.

## Ownership
| track | owns | cargo target dir |
|---|---|---|
| E view: draw distance, widescreen, MSAA, texture filtering, settings, lighting prototype | mr-bevy: settings.rs (new: `Settings` resource, CLI + settings file), scene.rs, psxmesh.rs, vramtex.rs, kartmodel.rs, kartshadow.rs (non-blend parts), hud.rs (widescreen placement only), camera/window setup in main.rs; render-ledger.md (view rows); port/README.md options | port/target |
| F effects: blending, sprite depth, effects gaps | mr-bevy: effects.rs, animtex.rs, kartshadow.rs blend; mr-sim particles.rs / items spawn hooks if a missing effect needs sim state (coordinate with G); docs/spec/effects.md, render-ledger.md (effects rows) | port/target-b |
| G 60 Hz: latency + true 60 Hz mode | mr-sim (update.rs and friends, a dt mode flag), mr-bevy tick scheduling / input sampling / interpolation code in main.rs + input.rs; mr-sim tests; kart-physics-ledger.md section 10 | port/target-c |

Options all go through E's `Settings` resource: F and G post the option names
they need here; until E publishes it, read a CLI flag in your own file and
switch over once E posts the API.

## Posted APIs
### E view
### F effects
### G 60 Hz

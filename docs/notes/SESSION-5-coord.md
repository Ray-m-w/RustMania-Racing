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
(2026-10-13, E) `Settings` is in (mr-bevy/src/settings.rs, `pub mod settings;`
in main.rs, inserted as a resource before the plugins).
```rust
use crate::settings::Settings;
fn sys(s: Res<Settings>) { s.bool("my_key", false); s.f32("k", 1.0); s.i64("k", 30); s.get("k"); }
let s = Settings::global();   // same values outside systems (before the App exists, SimRun::new, ...)
```
- Add your option as ONE row in `settings::KNOWN` under your track's comment
  line (`Opt { key, default, allowed: "a|b" or "true|false" or "", help }`);
  that gives it the file key, `--set key=value`, `--key-with-dashes [value]`
  (a bool alone = true), validation, and a line in `Settings::to_text()`
  (the future menu iterates `KNOWN`). Typed getters are optional: add a
  method if you want one, in your own hunk at the end of `impl Settings`.
- Unknown keys still work through `--set` / the file (warned, kept), so you
  can read `s.bool("x", ..)` before adding the row.
- File: `settings.txt` beside the exe, or `--settings PATH`; `key = value`,
  `#` comments; CLI wins over the file.
- E's keys: `faithful_draw_distance` (false), `aspect` (native|4:3),
  `msaa` (off|2|4|8, default 4), `texture_filter` (nearest|smooth, default
  smooth), `lighting` (faithful|enhanced, default faithful).
### F effects
### G 60 Hz

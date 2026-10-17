# Session 5: coordination between the parallel tracks

Brief: docs/history/SESSION-5.md. Same mechanics as docs/history/notes/SESSION-4-coord.md
(commit identity and trailer, commit only your own paths and hunks — for a
shared file stage only your hunks on top of HEAD, oracle lock, no game data,
log in docs/history/notes/2026-10-13.md under "Session 5, <track>", stop rules).
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
(2026-10-13, E, commit 0cb25bb) View options in, what F and G should know:
- `VramTextures::get` now follows `texture_filter` (default smooth: bilinear,
  5 mip levels, transparent texels bled with neighbour colours, sampler
  clamp + anisotropy 16); `get_nearest` = the old point-sampled page (the HUD
  uses it). Pages rewritten by `write_rect` (animtex) are rebuilt (bleed +
  mips) only when their texels changed. Opaque textured scenery uses
  `psxmesh::cutout_mode()` (AlphaToCoverage when smooth, Mask(0.5) nearest).
- MSAA: `Settings::msaa()` (default 4x) is on the chase camera, the far-pass
  camera and the HUD camera (all cameras on the window must share one sample
  count). A custom Material gets it through Bevy's pipeline key; a custom
  render pass / compositing shader (psxfx_comp?) must read the view's sample
  count (multisampled textures) or resolve first.
- Cameras now carry a `Viewport` (4:3 option = pillarbox); `crate::view_rect`
  gives the 3D rectangle. Default window 1600x900 (`resolution`).
- `--faithful-view` (preset): faithful draw distance + 4:3 + nearest +
  faithful lighting + 1280x960 window: use it for side-by-sides with the
  oracle (tools/oracle/hud_sbs.py / fx_sbs.py assume a 4:3 window shot).
- Full draw distance draws every layer-0 object always: pickups / particles
  are no longer seen through missing scenery, but sprite depth is F's.
### F effects
(2026-10-13, F) Effects pass in (commit 1fd9596): `mr-bevy/src/psxfx.rs`.
- Options (rows in `settings::KNOWN`): `fx_blend` (psx|linear, default psx:
  PS1 semi-transparency on the gamma-encoded frame; linear = same equations
  in linear light), `fx_depth_test` (true|false, default true: sprites,
  particles, projectiles, shadows hidden by walls; ground under a sprite
  never hides it).
- E: the pass needs the chase camera's depth texture readable; psxfx adds
  TEXTURE_BINDING to `Camera3d::depth_texture_usages` and a `PsxFxView`
  marker on the `ChaseCamera` itself (no change in main.rs). It runs after
  the main transparent pass, copies the (sRGB, resolved) main texture,
  draws, and writes back through `ViewTarget::get_color_attachment()` (all
  MSAA samples). It requires an LDR sRGB target (skips itself on HDR).
  Kart shadows now go through it (kartshadow.rs: no entity/material any
  more; `update_shadows` pushes `psxfx::FxPrim`s).
- Anyone can draw PS1-blended primitives: push `psxfx::FxPrim` (4 verts,
  page (tpage, clut), `Blend::Abr(n)` / `Opaque`, centre, ground_eps,
  depth_pull) into `ResMut<psxfx::FxPrims>` during Update.
(2026-10-13, F, closing) Done; commits through fa6aee5. New: kartfx.rs
(status shapes, far-kart impostor with `faithful_draw_distance`: karts.rs
hides the model via `effects::KartLodHidden`), pickupshadow.rs, mr-sim
`particles::Flats` + `Session::flats()` (splash rings / skid quads, spawned
in session.rs step 8 with the existing random byte; G: the 60 Hz path gates
splashes on `hz60.beat()`, skids and the update run every tick with dt).
E: scene semi-transparent polygons could be drawn exactly by pushing
`psxfx::FxPrim`s instead of psxmesh's 50% alpha (the oracle's water is
brighter); not done (your files).
### G 60 Hz
(2026-10-13, G) Options in settings.rs `KNOWN` (G rows): `logic_hz` (30|60,
default 30: 60 = the 60 Hz mode, not default until the owner drives it),
`low_latency` (true|false, default true). `--latency-log SECONDS` (tool flag).
What E / F should know:
- mr-sim: `SessionConfig::logic_hz`, `Session::logic_hz()`,
  `vblanks_per_tick()` (2 or 1), `tick_seconds()`, `set_logic_hz(hz)` (at a
  pair boundary), `snapshot()` / `restore()`. `RaceGlobals::hz60` (`beat()`
  = first tick of a 30 Hz pair, `pair_end()`); a sim term that runs once per
  game tick should use them (ledger section 12). `RaceGlobals::frame` and
  `dt` stay in VBlanks: anything animated from `globals.frame` / `dt` is
  right at both rates; anything counting `session.ticks` or `Player::tick`
  runs twice as fast at 60 Hz.
- mr-bevy (pacing.rs): with `low_latency` the display endpoints are (last
  real tick, the coming tick run on a throw-away snapshot): `Player.prev` /
  `Player.pose` / `prev_kart` / `kart` and `KartFeed.prev` / `now` are
  rewritten every frame the buttons or the tick change (RunFixedMainLoop,
  after the fixed ticks); keep interpolating with `Time<Fixed>`'s overstep
  as before. Item / particle state (`session.items()`, `particles()`) is the
  last real tick's: up to one tick behind the karts on screen (it was one
  tick ahead before). `sim_tick` takes its buttons from `PadLatch`.
### Finishing (lead)
(2026-10-13) `psxfx` is per view: `PsxFxView(VIEW_MAIN | VIEW_FAR | VIEW_HUD)`;
`FxPrim` has `view`, `bucket`, `always_depth`, `smooth`, `tri`, `cull_back`
(build with `..Default::default()`). psxmesh keeps semi-transparent polygons
out of meshes: `psxmesh::semi_prims` -> `FxSemi` component. effects.rs
`FxSource` / `FxLookahead`: effects follow the karts' display endpoints.
New options: `render_latency` (low default), `lighting` default enhanced.


# Session 6 follow-up: visual check of all 18 race courses against the oracle

Trigger: the owner's report "as soon as I spawned on GHOST, the kart's wheels
were in the floor". Brief: one run through each race course, compared with
the original at the same moments.

## Method (apples to apples)
- Same run in both: each course's oracle capture (analysis/oracle/courses/NAME,
  DOCK = analysis/oracle/start_probe: course menu -> CROSS 30/120/210, then no
  input to VB 3400). The oracle re-runs that route from the course-menu
  state (slot 11; DOCK slot 1 read only) and takes native VRAM screenshots
  (`tools/oracle/visual_shots.py NAME DIR VB,...`). The port replays the same
  capture (`mr-bevy --course NAME --replay-capture DIR --faithful-view
  --fx-shots`, all six kart records and the camera from the oracle RAM;
  `tools/oracle/visual_port.py`), shots every 2 VBlanks up to 10 before the
  moment; `tools/oracle/visual_sbs.py` picks the port frame with the least
  difference (the oracle shot lands 2-4 VBlanks after the frame it shows).
- The oracle's 3D is squashed 3/4 in x by the recomp's widescreen package:
  row 1 of each side by side undoes it (3D compare); row 2 shows the raw
  oracle frame (the HUD is not squashed); row 3 a 3x crop of the player's
  kart. Default-view shots (widescreen off, 1280x960, smooth, full draw
  distance, haze) at the grid: `sbs_grid_default.png`.
- Moments: grid (VB 300: countdown, karts at rest), GO + 40, mid (VB 1800;
  DOCK 1200), end (VB 3380; DOCK 1780). Images: analysis/s6/visual/NAME/
  sbs_{grid,go,mid,end,grid_default}.png (oracle left, port right),
  NAME/moments.png (the four moments, 3D rows), sheet_grid.png (all 18
  grids, faithful view), sheet_grid_default.png (default view).
- Numbers: `tools/oracle/visual_burial.py` (kart record y against the drawn
  DAT layer-0 surface under it, every 10 VBlanks of every capture);
  `ot_bias` example (each object's ordering-table bias); oracle GP0 frames
  (visual_shots.py --gp0) for PARK, BOG, OVAL.

## Findings
| course | moments | differences found | cause | fixed? | evidence |
|---|---|---|---|---|---|
| DOCK | 300, 681, 1200, 1780 | intro faces row missing; banner cloth shape / shading | faces: not ported; cloth: phase is power-on history | faces yes; cloth n/a | DOCK/sbs_grid.png |
| BOG | 300, 683, 1800, 3380 | faces; 11 kind-3 objects not drawn; karts under the ground in places (p99 76, max 172 DAT units); no rain in replays; banner cloth | parts models not ported; OT order; replay had no weather | yes (cloth n/a) | BOG/sbs_*.png |
| GHOST | 300, 684, 1800, 3380 | **AI kart (and the player's at times) sunk into the floor**; far canyon textured instead of the flat stand-ins; faces; 8 parts objects | drawn ground 30-140 DAT units above the karts' TER on 25 % of samples (object 76, bias 240); port depth-tested karts against it while the game sorts the ground under them; faithful far cut / draw globals; parts | yes | GHOST/wheels_before_after.png (oracle / before / after), GHOST/sbs_grid.png |
| HAPPY | 300, 680, 1800, 3380 | faces; lamps / glows missing (73 parts objects) | parts not ported | yes | HAPPY/moments.png rows 3-4 |
| GALLERY | 300, 684, 1800, 3380 | faces; karts under drawn floor on 8 % of samples (objects 14-23, flag 0x11) | OT order | yes | GALLERY/sbs_*.png |
| PARK | 300, 681, 1800, 3380 | tree canopies missing (bare trunks); faithful view lacked the hill / tree row; faces | parts (33 objects) | yes | PARK/parts_cmp.png (oracle / faithful / default), PARK/sbs_grid.png |
| NEW_YORK | 300, 682, 1800, 3380 | faces; 4 parts objects | as above | yes | NEW_YORK/sbs_*.png |
| LONDON | 300, 683, 1800, 3380 | faces; 23 parts; snow absent in replays; draw distance was DOCK's (2386 vs the game's 5000) | as above | yes | LONDON/sbs_*.png |
| GRAVE | 300, 682, 1800, 3380 | faces; 31 parts; snow absent in replays; one far pickup star at the left edge (end) not in the port frame | as above; the star: open (culling at the frame edge, not traced) | yes / star open | GRAVE/sbs_end.png |
| ISLAND | 300, 680, 1800, 3380 | faces; 1 parts object | as above | yes | ISLAND/sbs_*.png |
| BASE | 300, 682, 1800, 3380 | faces; banner cloth | as above | yes / n/a | BASE/sbs_*.png |
| CAPE | 300, 685, 1800, 3380 | faces; 9 parts | as above | yes | CAPE/sbs_*.png |
| OVAL | 300, 680, 1800, 3380 | faces; token burst pieces looked smaller at the end moment | burst shrinks every frame: the port frame 4 VBlanks earlier is the same (display lag), not a difference | yes / none | OVAL/token_burst_strip.png |
| ROCK | 300, 679, 1800, 3380 | faces; karts under drawn ground on 10 % (object 218, 0x11); draw distance DOCK's | OT order; draw globals | yes | ROCK/sbs_*.png |
| DISCO | 300, 680, 1800, 3380 | faces; floor / tunnel texture animation offset differs | animated strip / scroll counters approximated by the frame counter (render ledger) | faces yes; phase open | DISCO/sbs_mid.png |
| SPEEDWAY | 300, 681, 1800, 3380 | faces; trees (23 parts) missing; karts under the drawn track on 56 % of samples (median 50 DAT units, objects 18-29, 0x19); banner cloth | as above | yes / cloth n/a | SPEEDWAY/moments.png |
| RALLY | 300, 681, 1800, 3380 | faces; karts under drawn ground 9 % (object 58); banner cloth | OT order | yes / n/a | RALLY/sbs_*.png |
| GLACIER | 300, 681, 1800, 3380 | faces; snow absent in replays; banner cloth; draw distance | as above | yes / n/a | GLACIER/sbs_*.png |

Everything else seen in the pairs matched: scenery placement, textures,
sky / backdrops, fog colours, water, kart models, scale and orientation,
wheel contact (after the fix), moving scenery, particles, HUD elements and
positions (the place digit can differ for a frame around a change, display
lag), minimap markers (the player's blinks).

## The GHOST wheels (cause)
The karts run on the TER collision surface; on GHOST the drawn DAT ground
(object 76, a ground-list object with OT bias 96 + 16 * 9 = 240) lies up to
~140 DAT units above it at the start (kart 1 on the grid: record y 132 DAT
units below the drawn triangle under it). The oracle's karts are at exactly
those heights; the game still draws them whole because FUN_80012390 sorts
each scenery quad at T[OTZ + bias] while FUN_8001EC2C sorts kart polygons at
T[OTZ] (no depth buffer). The port depth-tested the kart against the ground
(its -2000 constant depth bias on the ground list is ~0.02 % of depth), so
the ground covered the wheels and body. Fix (51b0710): every layer-0
material writes its fragment depth pushed 4 * bias view units away from the
eye (scenepush.rs, ot_push.wgsl), the effects pass's scenery semis the same.
Courses with buried karts (visual_burial.py, share of samples > 40 DAT units
under): SPEEDWAY 56 %, GHOST 25 %, ROCK 10 %, RALLY 9 %, GALLERY 8 %, GRAVE
3.5 %, ISLAND 2 %, BOG 1.8 %, DISCO 1.4 %, PARK 1.2 %, DOCK 0.8 %.

## Fixes (commits)
- 6b0a238 tooling; Time<Virtual> pause in --fx-shots was undone by courses.rs
  every frame (shots landed ~5 ticks late).
- 51b0710 OT-bias depth push (GHOST wheels); per-course draw globals
  (FUN_8002e2f0) and the per-polygon far cut in the faithful view.
- 66bc054 the race intro's opponent faces row; replays take the HUD phase
  from the captured RAM.
- ee485d4 DAT kind 2/3 parts models drawn (camera-facing quads).
- 47db09c replays draw the captured weather; contact sheet / burial tools.
cargo test --workspace --offline --release green; mr-bevy release builds
without warnings.

## Left for the owner / open
- Banner cloth phase: the game's never resets (power-on history), so its
  wave and shading differ from the port's in any frame. A look, not a bug;
  could be read from the oracle for replays if wanted.
- DISCO's (and the other courses') animated texture phase: approximated by
  the frame counter (render ledger), visible on DISCO's psychedelic floor.
- The OT push is per fragment, the game's sort per polygon average; a kart
  behind a wall within ~4 * bias units of it now shows through, as in the
  game (the game sorts the same way); the default (full draw distance) view
  keeps it.
- Faithful view: the far cut is per fragment (a polygon crossing the limit
  is cut on a curve, the game drops it whole); GHOST shows a far building
  where the oracle shows sky.
- Parts models: kind 3's near subdivision (affine only) and the per-record
  far cut are not reproduced.
- Replays (`--replay-capture`) still do not pose moving scenery (lifts,
  spinners) from the capture: a tool limit, not the game view.
- GRAVE's leftmost far star pickup at the end moment: not traced.
- The intro faces swing with DAT_800B07D4, which live sessions approximate
  by the frame counter.

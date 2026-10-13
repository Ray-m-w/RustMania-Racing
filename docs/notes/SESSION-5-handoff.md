# Session 5 handoff (2026-10-13): graphics pass and 60 Hz

Brief: docs/SESSION-5.md. Tracks: docs/notes/SESSION-5-coord.md. Logs:
docs/notes/2026-10-13.md ("Session 5, ..."). Details: render-ledger.md,
effects.md, kart-physics-ledger.md section 12 (60 Hz mode), port/README.md
(every option).

## Play it
```
.\tools\play-port.ps1            # defaults below
.\tools\play-port.ps1 -Hz60      # true 60 Hz logic
.\tools\play-port.ps1 --msaa 8 --texture-filter nearest   # any option
```
Options: `settings.txt` beside port\target\release\mr-bevy.exe (`key = value`)
or the command line (`--set key=value` / `--key value`); `--faithful-view` =
the original's look (for side-by-sides).

## Defaults
| option | default | faithful setting |
|---|---|---|
| faithful_draw_distance | false (whole course every frame) | true |
| aspect | native (widescreen, game's vertical FOV) | 4:3 |
| msaa | 4 | off |
| texture_filter | smooth (bilinear + mips, clean cut-outs) | nearest |
| lighting | enhanced (distance haze; OWNER DECISION 2026-10-13) | faithful |
| fx_blend | psx (gamma-space PS1 blend modes, effects + water + HUD) | psx (linear = fallback) |
| fx_depth_test | true | - |
| logic_hz | 30 (60 after the owner drives it) | 30 |
| low_latency | true (sim untouched) | false = Session 4 display |
| render_latency | low (no pipelined rendering, 1 queued frame) | normal |

## Status against "Done when"
1. Draw distance: DONE. 242 of 243 layer-1 objects are stand-ins (dropped),
   the 0x89 backdrop kept; far plane covers the course; before/after in
   analysis/s5/drawdist/.
2. Sprite depth: DONE. Depth-tested against the scene; ground exemption as
   the game's OT order; pickups hidden by walls.
3. Widescreen: DONE. HUD elements pinned; checked countdown, banners, finish,
   results at 4:3 / 16:9 / 21:9.
4. Effects: DONE. PS1 blend modes exact in gamma (explosion 185/140/86 vs the
   oracle's 184/144/97); kart shadows subtractive; water through the same pass
   (~7 % brighter than the oracle, was ~50 % dimmer); splash rings / skid quads
   (FUN_800342B8, exact 126,272 entry-frames); far-kart impostor
   (FUN_8007D660, faithful draw distance only); pickup shadows exact; 5 status
   effects from the code (unverified: no capture with character specials).
5. AA + filtering: DONE (MSAA; DLSS deferred by the owner).
6. Lighting: haze prototype, made default by the owner.
7. 60 Hz: DONE as an option. Latency (faithful mode) first visible response
   16.5 -> 1.5 ms, full 50 -> 11 ms; render path 15.9 -> 8.2 ms at 120 Hz vsync.
   60 Hz mode: 39/39 feel-tolerance rows vs the 30 Hz port; 30 Hz path
   bit-identical (fingerprint, all oracle tests, mr-verify, lap1).
8. Settings: DONE (`Settings`, settings::KNOWN; the future menu iterates it).
9. Report: this file. Owner drive: 60 Hz verdict pending.

## Open
- 60 Hz: AI with items +0.4..0.9 % slower (inside limits, cause not isolated);
  hits can land one VBlank earlier.
- Water ~7 % bright (8-bit vs 15-bit, animated water texture alignment).
- Animated texture decode slightly misaligned vs VRAM (pictures match).
- 4:3 pillarbox bars show the course clear colour, not black.
- slab_allocator startup errors: not reproducible; suspected cause fixed.
- Rotated sprite 1.5 stretch not reproduced; status effects unverified.

## Next
Owner drives `-Hz60`; if it feels right, flip `logic_hz` default to 60. Then
Session 6: the other 20 race courses (docs/notes/SESSION-4-handoff.md "Not
started"), then the front end.

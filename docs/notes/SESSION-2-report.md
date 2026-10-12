# Session 2 comparison report: Rust kart vs the recomp on DOCK

Date 2026-10-12. Oracle = psxrecomp build-dev with the oracle bridge
(docs/notes/SESSION-2-oracle.md); Rust = port/crates/mr-sim (+ mr-bevy).
All numbers reproducible with the commands in section 5.

## 1. Tolerance (STEP 2)
- Run-to-run variation of the oracle: **zero**. Two replays of the same route
  are byte-identical in every captured region (3,090 VBlanks, ~16 KB each, plus
  the 2 MB restore image), also across a rebuild and relaunch. So the tolerance
  cannot come from oracle noise; the brief's proposal is used as the ceiling and
  anything non-zero must be explained.
- Track width on DOCK: median wall-to-wall distance across the TER racing line
  = 3,652 TER units = **116,862 kart units** (port/crates/mr-bevy/examples/
  width.rs; 356 racing-line points). 1% = **1,169 kart units**.
- Tolerances used: position < 1,169 kart units and heading < 2 deg over the
  first 300 ticks (10 s); lap time within 2%.

## 2. Results
Route `tools/oracle/routes/lap1.json`: one full DOCK lap, 1,538 logic ticks
(the game drops to 20 Hz, dt = 3, for 15 ticks in the heavy section).

| Run | First difference | 300-tick window | Whole lap |
|---|---|---|---|
| mr-sim, oracle-fed inputs (`lap1.rust`) | none | PASS, max error 0 | 0 on every field, 1,538 ticks |
| mr-sim, pure free run (`lap1.rust.pure`) | tick 823 | PASS, max error 0 | an AI kart's projectile hits the player at tick 821 (weapons/AI out of scope); identical before it |
| mr-sim standalone (disc only), catch-up as in the oracle race, oracle dt | tick 822 | PASS, 0 | same projectile |
| mr-sim standalone, catch-up as the oracle race, dt = 2 | tick 687 | PASS, 0 | the original drops to dt = 3 there; the port keeps 30 Hz |
| mr-sim standalone, player leads (no AI) | tick 43 | FAIL by design | the game's catch-up raises the speed cap from 0x800 to 0xA00 when the player trails; with no AI the player leads, so top speed is 20% lower |
| mr-bevy live app, lap1 scripted, `--oracle-grid` | none in 507 ticks | PASS, 0 on all 34 fields incl. camera | (app stopped at a screenshot) |
| A drive recorded in mr-bevy, replayed on the recomp (`tools/compare-drive.ps1`) | tick 687 | PASS, 0 | dt = 3 frames, as above |

Lap time: the Rust lap is tick-identical to the oracle's (same race clock
values to the end of the lap), so it is within 0% of the original.

Every trace field compared: position x/y/z, pitch/heading/roll, velocity,
forward/sideways speed, body matrix (9), race clock, dt, camera eye (3) and
camera matrix (9).

Plots (SVG, numbers only): `docs/notes/s2-report/lap1/` (first 300 ticks),
`lap1-full/` (whole lap), `lap1-pure/` (pure run; error appears at tick 823):
`pos_err.svg`, `heading_err.svg`, `trajectory.svg`.

## 3. What is native Rust now
- Disc access (ISO 9660), and every DOCK asset: textures (RAW), scenery (DAT,
  with the game's per-frame object selection and draw-order layering), terrain
  and walls (TER), sky (AXE), Kermit and the RACER (MDL; ANM decoded, unused by
  the game), vehicle and character tables from the executable.
- The player kart's whole per-tick update (FUN_80064B5C and callees: input,
  engine, steering, slopes, ground and walls, water, jump, lap progress,
  kart-kart contact), race-start initialisation, water animation, body matrix,
  chase camera (FUN_80070418) and the game's projection.
- Bevy: window, 30 Hz fixed tick, keyboard/gamepad, PS1-style rendering
  (nearest texels, GPU colour modulation, no lighting/tonemapping), oracle
  ghost and replay modes.
- Nothing from the recomp runs in `mr-bevy` live play.

## 4. Differences (ledgers)
- Physics: docs/spec/kart-physics-ledger.md (stubbed branches = items,
  weapons, AI, multiplayer, other modes, race end; inputs the standalone mode
  produces; catch-up consequence; cosmetic fields).
- Rendering: docs/spec/render-ledger.md.

## 5. Reproduce
```
cd port
cargo test --workspace --offline            # decoders, sim, camera, standalone
cargo run --release --bin mr-verify          # teacher forcing + free run vs captures in analysis/oracle
cargo run --release --bin mr-compare -- ../analysis/traces/lap1.oracle.trace.jsonl ../analysis/traces/lap1.rust.trace.jsonl --scale 1 --pos-tol 1169 --deg-tol 2 --window 300
```
Oracle side: `.\tools\run-dev.ps1 -Start`, then
`python tools/oracle/trace.py tools/oracle/routes/lap1.json analysis/traces/lap1.oracle.trace.jsonl`.

## 6. Owner check still to do (brief: "the owner has driven both")
`.\tools\play-port.ps1 -Record`, drive a lap, close the window, then
`.\tools\compare-drive.ps1 <stamp>` with the debug build running; and play the
recomp (`G:\GameDev\Muppet RaceMania Recompiled\`) on DOCK for the feel
comparison. Agreement that they feel the same is the owner's call.

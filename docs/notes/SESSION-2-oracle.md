# Session 2, STEP 2: the oracle harness

The psxrecomp build (build-dev, RelWithDebInfo, TCP 4370) is the reference.
Everything here runs from the repo root with the game started by
`.\tools\run-dev.ps1 -Start` and the dev package enabled (below).

## 1. Deterministic start
- Savestate **slot 2** (`saves/scph1001/state_80093214_slot02.pst`, local,
  never committed; backup in analysis/states/): 1-PLAYER RACE, DOCK, Kermit in
  the RACER, on the grid during the "Copy the buttons" start game, race clock
  ~4.7 s. Slot 1 = the DOCK course menu just before it.
- Menu route used to get there (recomp, attract after ~27,600 frames): START at
  the DEMO screen -> attract configurator; DOWN x3 to GAME MENU, CROSS;
  DOWN x2 CHANGE COURSE, CROSS; DOWN x4 MUPPET TREASURE ISLAND, CROSS;
  "DOCK - RACE A" is first, CROSS; 1 PLAYER RACE, CROSS; KERMIT / RACER, CROSS.
- Loading a savestate does NOT reset the runtime's frame counter, so TCP
  `input_route_start at_frame` cannot anchor a route to a load. Hence the bridge.

## 2. The oracle bridge (src/mods/mrm_dev_oracle_bridge_plugin.c)
A trusted plugin in a development package (`mods/dev/packages/mrm.dev.oracle-
bridge`, merged into the catalog only for non-Release builds by
CMakeLists.txt; its source is filtered out of Release builds). Enable it in
`build-dev/mods/state.toml` (package + feature `oracle-bridge`). It owns a
shared-memory block `Local\mrm_oracle_bridge_v2`:
- on every savestate restore: bumps `restore_gen`, zeroes the VBlank counter,
  copies main RAM + scratchpad (the exact restored state, `ram0.bin`), and
  starts an armed input route;
- every VBlank: copies up to 32 KB of listed RAM regions into a ring slot
  stamped (restore_gen, VBlanks since restore);
- player 1 pad transform (mod_plugins.h stage 2b): while a route is active
  its output REPLACES the pad, keyed on VBlanks since restore. Digital buttons
  + analog sticks per step. (The first version used a controller source, whose
  buttons are ANDed with the physical pad: one capture, lap1P, got a stray
  host START at VBlanks 4-5 and paused; it is discarded. Dev runs therefore
  disable mrm.enhancement.modern-controls in build-dev/mods/state.toml so the
  bridge owns player 1's transform.)
Client: tools/oracle/bridge.py; TCP helper tools/oracle/oracle.py.

## 3. Routes and captures
- `python tools/oracle/capture.py ROUTE.json OUTDIR [--physics]` -> frames.bin,
  meta.json, ram0.bin. Default regions: player kart record, camera, dt, pads,
  mode, kart 1. `--physics`: all 6 kart records + every mutable address the
  kart update touches (capture.PHYSICS_REGIONS, from the Ghidra xrefs).
- `python tools/oracle/trace.py ROUTE.json OUT.trace.jsonl` -> mr-trace file,
  one row per logic tick (tick k = VBlank 2k).
- Routes: tools/oracle/routes/*.json, `[vb_start, "cross+left"]` steps.
  `lap1.json` = one full DOCK lap (235 steps), recorded closed-loop by
  `tools/oracle/drive_lap.py` (holds CROSS, steers toward an AI kart's line)
  and then replayed open-loop.

## 4. A/A determinism (the tolerance floor)
| Run pair | Compared | Result |
|---|---|---|
| steer route, 2 runs | 432 VBlanks x (kart0 0x504 B, camera, globals, kart1) | byte-identical |
| lap1, runs A and B | 1,545 ticks, all trace fields; raw captures | identical (mr-compare PASS, all max diff 0) |
| lap1 run C after rebuild + relaunch | trace | identical prefix (C one tick longer) |
| lap1P2 vs lap1P3 (physics regions, controller source vs pad transform build, relaunch between) | 3,090 VBlanks x all PHYSICS_REGIONS (~16 KB each) + ram0.bin | byte-identical |
Run-to-run variation is zero, so the tolerance cannot be derived from it; the
brief's proposed tolerance stands as the ceiling and anything above 0 is a
real difference to explain (the ledger).

## 5. Comparator
`port/crates/mr-trace` (std only): the trace format, `compare()` with position
/ heading tolerances over a window, per-field first-diff + max-diff, and SVG
plots (position error, heading error, top-down trajectory overlay).
CLI: `cargo run --bin mr-compare -- REF CAND --window 300 --pos-tol T --deg-tol 2 --out DIR`.

## 6. Facts established on the way
- Kart array 0x800C30A8 stride 0x504; tick = 2 VBlanks; field map in
  docs/spec/kart-state.md.
- This save accelerates with CROSS, not TRIANGLE (Triangle on the grid spins
  the kart). CLAUDE.md and mrm_modern_controls_plugin.c assume Triangle.
- lap1: the player never comes within 10.7k units of an AI kart (no contact).

# Brief: port the player kart's handling to Rust (Session 2, STEP 4)

Project G:\GameDev\Muppets Claude Racemania. Read CLAUDE.md, docs/SESSION-2.md
(STEP 4 and Rules), docs/spec/kart-state.md, docs/spec/formats/AGENT-BRIEF.md
(inputs section: decomp, generated C, oracle protocol) first.

## Goal
`port/crates/mr-sim`: a Rust reimplementation of the per-tick update of the
human player's kart, FUN_80064B5C(kart, buttons, 0) and everything it calls on
the paths a solo player kart takes on DOCK (input -> throttle/brake -> speed ->
steering/heading -> ground contact with the track -> position integration),
plus FUN_80086B7C(kart) which runs after it for human karts. Same integers as
the game, tick for tick.

## Rules
- std only, no floats, no Bevy, no I/O inside the simulation (ADR-0002). Data
  tables the code reads (sin/cos tables, handling constants, vehicle stats) are
  read at run time from the executable on the disc (`mr_disc::Disc::read
  ("SLUS_012.37")`, PS-X EXE: file offset = addr - 0x80010000 + 0x800) or from
  the course files, never pasted into the source. Every constant in Rust says
  where it came from (exe address or file offset).
- Port behavior into clean Rust with named fields; never transliterate the
  generated C wholesale and never commit it. Generated C
  (generated/SLUS_012.37_full_*.c, search `void func_80064B5C(`) is the
  behavioral spec; Ghidra decomp (analysis/ghidra/decomp/) is the reading aid.
- Fixed point exactly as the game: 16.16 / 4.12, arithmetic shifts, MIPS `div`
  truncation, `mult` 64-bit products, GTE ops where used (document each GTE op
  you reimplement with the psx-spx formula).
- Branches the solo player never takes on DOCK (items, weapons, AI, multiplayer,
  boss, boost-start minigame success, damage) may be stubbed ONLY as
  `unreachable_in_scope!("reason")`-style code that panics in tests, and each
  one is listed in docs/spec/kart-physics-ledger.md ("differences ledger").
- Ground/collision: the TER format agent (docs/spec/formats/TER.md,
  port/crates/mr-assets/src/ter.rs, may still be in progress) owns decoding the
  terrain and the game's ground query. Until it lands, verify your layers with
  the oracle's own contact results (teacher forcing, below). Do not edit
  mr-assets; you may add read-only helpers in mr-sim.

## Verification: teacher forcing on oracle captures
Captures already exist (do not re-capture unless you need a new route; if you
do, see "Driving the oracle" below):
- analysis/oracle/lap1P/: a full DOCK lap (route tools/oracle/routes/lap1.json),
  `frames.bin` + `meta.json` (regions list, vb list; per-VBlank concatenation of
  the regions, see tools/oracle/capture.py `load()`), and `ram0.bin` = main RAM
  (2 MB) + scratchpad (1 KB) copied at the savestate restore (VBlank 0).
  Regions = capture.PHYSICS_REGIONS: all 6 kart records (from 0x800C3000),
  cameras, globals 0x800B0700..0x800B1000, scratchpad, and the other mutable
  addresses the kart update touches.
- analysis/oracle/c_steer/, c_cross/, probe/: shorter routes, default region
  set (kart0 0x504 bytes, camera, dt, pads, mode, kart1).
Tick k is VBlank 2k. Build a Rust harness (mr-sim test or bin `mr-verify`) that
for every tick takes the oracle state at VBlank 2k, runs your Rust tick, and
compares every field you claim against VBlank 2k+2: report per field the first
mismatching tick and max error. Then free-run (no teacher forcing) from tick 0
and produce an mr-trace file (port/crates/mr-trace format; fields as
tools/oracle/trace.py FIELDS) and compare with
`cargo run --bin mr-compare -- analysis/traces/lap1.oracle.C.trace.jsonl <yours>`.

Work in layers, each gated by the comparator before the next: (1) input
mapping + throttle/brake -> speed on the straight start (routes c_cross),
(2) steering/heading (c_steer), (3) ground contact / height / slopes / walls,
(4) the whole lap free-running. Report results per layer.

## Driving the oracle (only if you need a new capture)
Only you drive the game (other agents are read-only). `python tools/oracle/
capture.py ROUTE.json OUTDIR --physics` loads savestate slot 2 (DOCK grid) and
replays the route frame-exactly (oracle bridge plugin). A/A runs are
byte-identical. Never save over slots 1 or 2. Put routes you want kept in
tools/oracle/routes/.

## Deliverables
- port/crates/mr-sim/src/**: the simulation (public API: `KartState` with named
  fields + `from_record(&[u8])`/`to_record` for the fields you own, a
  `World`/`Track` input for ground queries, `fn tick(&mut KartState, &World,
  buttons: u16, dt: i32)`, plus whatever global state the update needs).
- port/crates/mr-sim/tests/ and/or src/bin/mr-verify.rs: the teacher-forcing
  harness and free-run trace writer.
- docs/spec/kart-physics.md: the model, in prose + pseudocode, with code
  addresses; docs/spec/kart-physics-ledger.md: every deliberate difference and
  every stubbed branch.
- Update docs/spec/kart-state.md field table with what you learn (add rows, mark
  evidence). Do not commit. Report per-layer numbers.
- Also answer: which button accelerates by default and where the game's
  controller-configuration mapping lives (the save used here accelerates with
  CROSS, but CLAUDE.md says TRIANGLE).

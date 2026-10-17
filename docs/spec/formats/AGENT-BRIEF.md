# Brief for format-decoder agents (Session 2, STEP 3)

Project: G:\GameDev\Muppets Claude Racemania. Read CLAUDE.md and docs/history/SESSION-2.md
first. You decode ONE file format of Muppet RaceMania (PS1, SLUS-01237) into Rust.

## Inputs
- Disc files: `port/target/debug/mr-disc get COURSE_A/DOCK.TER out.bin` (also
  `mr-disc list`). Copies already in analysis/disc/ (gitignored): DOCK.{AXE,DAT,RAW,TER},
  KERMIT.{MDL,ANM}, RACER.{MDL,ANM}. In Rust use `mr_disc::Disc::open_default()?.read("COURSE_A/DOCK.TER")`.
- Ghidra decompiled C of every function: analysis/ghidra/decomp/<addr>_<name>.c,
  call graph analysis/ghidra/functions.tsv, data xrefs analysis/ghidra/xrefs_data.tsv.
  This is a READING AID. The behavioral spec is the recompiler's generated C in
  generated/SLUS_012.37_full_*.c (search `func_8004627C(`); when Ghidra and the
  generated C disagree, the generated C wins. Never copy generated C into the repo;
  port behavior into clean Rust.
- Track loader FUN_8004627c: `<course>.raw` -> FUN_800441d8; `.dat` loaded at
  heap ptr DAT_800b0ee8 then FUN_8002e3dc(track); `.ter` loaded (FUN_80042068) at
  DAT_800b0b3c then FUN_8003c2d8(); `.axe` (FUN_800420cc, only if track flag&1) at
  DAT_800b0c4c then FUN_80042130(). File read helper FUN_8005fdf4(name, dest).
  Models: FUN_8003c270 loads `<name>.mdl` then FUN_8003bf58(param, dest).
  `.anm`: FUN_800925a8. Kermit drives the RACER (VEHIC_A/RACER.*) on DOCK.
- The running game (oracle): psxrecomp debug build, TCP JSON on 127.0.0.1:4370,
  sitting in a 1-player race on DOCK. Protocol: psxrecomp/docs/TCP_COMMANDS.md.
  Python helper: tools/oracle/oracle.py (`Oracle().read(addr,n)`, `.q(cmd,**kw)`).
  YOU MAY ONLY USE READ-ONLY COMMANDS: read_ram, read_vram/vram_peek (max 128x128
  per call), gpu_state, gte_ring_dump, gpu_frame_dump, screenshot (to a path under
  analysis/), frame, fn_stats-style queries. NEVER: savestate, set_input, press,
  input_route_*, turbo, write_ram, quit. Another agent drives the game.
  The game's RAM keeps the loaded (and possibly loader-transformed) files at the
  heap pointers above: compare your decode to RAM to prove it.

## Output (only these files are yours)
- port/crates/mr-assets/src/<fmt>.rs (module already declared in lib.rs; std only,
  no new crates). Shared helpers you may use, not edit: `crate::le`, `crate::vram`
  (VRAM model, tpage/clut decode, texel fetch), `crate::img::png_rgba`.
- port/crates/mr-assets/src/bin/mr-<fmt>.rs: a CLI that dumps/exports for checking
  (PNG, OBJ, text) into analysis/<fmt>/ (gitignored).
- port/crates/mr-assets/tests/<fmt>.rs: tests that read the real disc (skip with a
  message if the disc is absent) and check invariants + values proven against RAM.
- docs/spec/formats/<FMT>.md: the format spec. Every field: offset, type, meaning,
  evidence tag: [CODE addr] (read from the game's code), [RAM] (matched the running
  game), [INFER] (your inference, unverified). Plus a "Verification" section with
  the exact commands and numbers.
- Do not edit lib.rs, Cargo.toml, other agents' modules, or anything outside these.
  Do not commit. Never write game bytes into the repo (only into analysis/).

## Done when
Your decoder parses every DOCK (and for MDL/ANM: KERMIT and RACER) file fully
(all bytes accounted for or explicitly marked unknown), its output is proven
against the running game (RAM structures, VRAM, or GTE input), `cargo test -p
mr-assets` passes, and the spec is written. Report: what the format contains,
the public Rust API (types + functions), verification evidence, open unknowns.
Build with `cd port && cargo build` (cargo serializes concurrent builds; that is fine).

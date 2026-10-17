# Session 1, step 4: first build and first boot (2026-10-11)

## Build
- Toolchain: psxrecomp cmake-clang-v1 v1.0.14 (llvm-mingw, clang 22.1.8). MSVC
  cannot build the pinned framework (PROPOSAL-psxrecomp-msvc.md).
- Framework: RetroPortingToolKit/psxrecomp @ 2a4824fcb897cb635438958f0c8cffb5a27df656.
- Generate: 60 shards, 2,195,311 lines, 1377 functions (1378 after the
  0x80060160 seed), 1442 dispatch entries. 40 "out-of-function jump/branch,
  emitting fallthrough" warnings; no unsupported instructions in the game EXE.
  BIOS: OpenBIOS emitted 651 / skipped 4; SCPH-1001 emitted 1309 / interpreted 1
  / skipped 1 (2 unsupported COP0-class instructions in skipped functions).
- Build warnings: 24 ordinary compiler warnings; none about unsupported
  instructions, unresolved jumps, overlays or stale stamps.

## Boot
- BIOS: retail SCPH-1001 v2.2 (bios_info match:1), chosen by first-run
  discovery of the dump the generate CLI staged; HLE tier on (boot-skip).
  OpenBIOS is the default for a player with no dump.
- Intro movies: MDEC FMV and XA/CD audio play; no audio underruns.
- Attract demo and title reached; START at the title opens the start menu
  (NEW GAME / MEMORY CARD), START in the attract demo opens the attract
  configurator. Screenshots: docs/history/notes/shots/ (local only, see .gitignore).
- Native vs interpreted (runtime phase_profile, 10 s window in FMV): static
  96.9%, interpreted 1.0%; run lifetime 16.2% interpreted, all of it the
  FMV.BIN movie player in RAM. After capture-and-compile: 0 interpreted
  instructions in movies, races, the memory-card screen and the attract demo
  (stall_report windows and dirty_ram_stats).
- Input that worked: TCP pad overrides (PS1 active-low word); later the host
  virtual gamepad path (stick, triggers, buttons) through the Trigger Controls
  transform.
- Owner confirmed by playing: menu works, a race is playable.

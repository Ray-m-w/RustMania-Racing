# Session 2 plan: Bevy port, milestone 1 (STEP 1 inventory and decision)

Date: 2026-10-12. Brief: docs/SESSION-2.md.

## Decision
**Option A** (owner, 2026-10-12, at session start): new Cargo workspace
`port/` in this repo. Facts and code from F:\Muppet Racemania are copied, never
linked, and each copy is recorded in BINLIST.toml and the notes.

The owner also asked to fold in the four speed-ups proposed at the end of the
last session; each is assigned to a step below:
1. Ghidra loaded with the recomp's function list (decompiler output next to
   the generated C) -> STEP 2/3/4 reading aid (`tools/ghidra/`).
2. A shared Rust trace-comparison crate (`port/crates/mr-trace`) used by both
   the recomp traces and the Rust simulation: automatic pass/fail + plots.
3. A Bevy debug overlay drawing the oracle kart as a ghost beside the Rust
   kart (`port/crates/mr-bevy`, replay of an oracle trace).
4. Asset decoders written in parallel, one file format per agent, each
   checked independently against VRAM / GTE captures.

The owner is away for this session and asked me to work on my own. I read
"go with option A" plus "integrate the considerations" (which name a Bevy
overlay and a Rust crate) as approval of the Bevy dependency that Option A
implies, using the quarry's ADR-0009 feature set as the starting point. Any
crate beyond Bevy and its tree still goes through the permission prompt and
is listed in BINLIST.toml.

## Quarry inventory (F:\Muppet Racemania, read-only)

| Item | What it is | Verdict |
|---|---|---|
| crates/mr-assets/src/lib.rs | ISO 9660 reader over the MODE2/2352 track; byte-verified on all 337 files vs a Python extraction | REUSE as-is (copy into port/crates/mr-disc, record) |
| crates/mr-assets/src/fmv.rs | FMV.BIN manifest parser | not needed (out of scope) |
| crates/mr-vram | 1024x512 VRAM model, 15-bit pixel rules, one quad rasterizer | REUSE the pixel/CLUT rules (4/8/15-bit texel decode); the rasterizer is not needed (Bevy draws) |
| crates/mr-sim | cadence + kernel-event tables for boot timing | not needed for kart handling |
| crates/mr-bevy | a 144-line Bevy 0.19 shell showing one quad; ADR-0009 feature set | REUSE the Cargo feature set; add 3D features (bevy_pbr/mesh) for a 3D track |
| crates/mr-harness, xtask | empty / CI gates | not needed |
| docs/spec/symbols.md | 1350 functions with PSYQ names | REUSE (copy names into the Ghidra project); boot-level, game logic is mostly FUN_ |
| docs/spec/kart-state.md | entity array stride 0x34, "player" 0x801C4D84, positions 16.16 written from GTE output every 4 frames; pad chain 0x800B0DA8 -> 0x80079B28 -> 0x8007A02C -> FUN_80064B5C -> ctx 0x800C30A8+0x44E (bit 12 = Triangle) | RE-VERIFY. Measured under PCSX-Redux on Happiness Hotel. The "position" fields are written from GTE output, so they are likely a render-side copy, not the physics state; velocity and the player's orientation were never found. The pad chain is strong (T2 instruction evidence) and is the entry point for STEP 4 |
| docs/spec/kernel-usage.md, timing.md | kernel census, VBlank timing | background only |
| docs/notes SESSION-5 | Redux menu route to a race: START 27,600 / UP 27,700 / CROSS 27,800 / 28,200 / 28,600 | HINT; recomp timing differs, route re-derived here |
| tools/ghidra | headless Ghidra 12.1.4 + ghidra_psx_ldr + JDK 21 (in the quarry's .cache and third_party) | REUSE the installed tools by running them in place (no writes into the quarry; project lives in this repo's gitignored ghidra/projects/) |
| ADR-0002 | sim/render split: sim has no Bevy, no floats, no I/O | ADOPT |
| ADR-0010 | frame is the unit of time; event order within a frame | ADOPT |

## Missing (this session builds it)
- Any decoder for .TER/.DAT/.RAW/.AXE (tracks) and .MDL/.ANM (characters,
  vehicles). The disc stores these as single files per track; DOCK is
  COURSE_A/DOCK.{TER,DAT,RAW,AXE}. Kermit is CHARS_A/KERMIT.{MDL,ANM}; the
  "standard kart" is most likely VEHIC_B/KART.{MDL,ANM} (the string table at
  0x80023884 lists `vehic_b\kart` among the vehicles). To be confirmed with
  cd_read_log.
- The real physics state of the player kart (velocity, heading), its update
  function, and its per-track address.
- A deterministic oracle harness on the recomp (the quarry's goldens are
  Redux runs on another track).

## Order of work
STEP 2 harness first (savestate on the DOCK grid, input routes, per-frame
kart trace, A/A variation). Ghidra export runs in parallel. Then the asset
formats in parallel (agents), then handling layer by layer.

## Crate layout (port/, ADR-0002 boundaries)
- mr-disc: ISO reader (from quarry), std only.
- mr-assets: track/model/texture decoders, std only.
- mr-sim: fixed-point kart simulation, std only, no floats.
- mr-trace: trace format (JSONL), comparator, SVG plots, std only.
- mr-bevy: the window; the only crate that depends on Bevy.

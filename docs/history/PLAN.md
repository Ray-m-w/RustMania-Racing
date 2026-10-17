# Project plan: complete portable Windows build

Owner brief (2026-10-11): take the recomp to "essentially a native Windows
application that launches from a portable EXE", with the quality-of-life a
modern port has (widescreen, controller support and binding, etc.), fully
playable; hand back when there is no more work.

Status keys: DONE, OPEN (with reason).

| # | Item | Status | Notes |
|---|---|---|---|
| 1 | Static recompilation of SLUS_012.37 | DONE | 1378 functions; seed 0x80060160 added (jalr target). |
| 2 | Runtime-loaded code native (FMV.BIN, MEMORY.BIN) | DONE | Capture+autocompile; 103 shards shipped; 0 interpreted insns in movies, races, memcard, attract. Bundled overlay toolchain compiles unseen variants on the player's PC. OPEN (optional): declarative AOT profile from disc. |
| 3 | High resolution defaults | DONE | display x1.5 dynamic, fxaa, perspective textures, mipmap+16x aniso, fmv chroma. geometry_correction off (framework G1.8/G1.9). |
| 4 | Widescreen | DONE | mrm.enhancement.widescreen (default on). Squash + gte_game_mode + auto_ui_squash. Menus/FMV 4:3, split screen OK. OPEN (optional): edge cull sites for 21:9+. |
| 5 | Controller support + rebinding | DONE | DualShock presentation (stick steering, vibration), Trigger Controls (RT/LT), launcher rebinding, keyboard. |
| 6 | Skip intro movies | DONE | mrm.enhancement.skip-fmv (off by default). |
| 7 | Fast loading | DONE | mrm.enhancement.fast-loading 4x (on); deprecated builtin excluded. |
| 8 | Smooth motion | DONE | mrm.enhancement.frame-rate (off by default); game renders at 30 fps (flip_period 2). |
| 9 | Save data | DONE | Memory card save, restart, LOAD offered and loads. |
| 10 | Broad playtest | DONE (sampled) | 1P race DOCK/SWAMP, battle arena, 2P split race, menus, options, memcard. Full tournament not driven. |
| 11 | Portable package | DONE | dist/mrm-0.1.0-windows-x64.zip; extracted copy launches with no toolchain. |
| 12 | Docs, handoff, commit | DONE | README, SESSION-1-boot/handoff, notes. |

## Framework findings to propose upstream (not applied; CLAUDE.md)

- MSVC build broken at pin (docs/history/notes/PROPOSAL-psxrecomp-msvc.md).
- setup_project.ps1 unusable under Windows PowerShell 5.1 (no BOM + em dashes;
  empty-string native args dropped; Set-Content UTF8 BOMs inside CMakeLists).
- TCP `turbo` stops window presents entirely.
- `[widescreen] hud_sprt_squash` transforms SPRT draws into offscreen texture
  pages (games that build textures with SPRT), corrupting textures. Should be
  gated on the draw area being the displayed framebuffer.
- native_wide widens the draw area into VRAM that holds texture pages for
  games whose framebuffers sit beside them.
- Text-page divergence bit is sticky (memory.c) even after bytes match again;
  `text_native_blocked` then climbs without any interpretation.
- POST_BUILD staging of game.toml does not rerun on config-only edits.
- Test drift at the pin (ctest 124/132): see PROPOSAL-psxrecomp-msvc.md.

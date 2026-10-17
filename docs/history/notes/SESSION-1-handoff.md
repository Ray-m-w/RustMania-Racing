# Handoff (2026-10-12)

Session 1 met its brief (native boot to title) and, at the owner's request,
continued to a complete portable build. This is the state at handoff.

## Deliverable
- `dist/mrm-0.1.0-windows-x64.zip` (31 MB) and the same, extracted, at
  `G:\GameDev\Muppet RaceMania Recompiled\` (run `Muppet_RaceMania_Recompiled.exe`).
  Self-contained: the exe imports only Windows system DLLs; it ships OpenBIOS,
  the mod catalog, 103 native overlay modules and the overlay toolchain
  (Python + TinyCC + recompiler) so any movie/memory-card code variant not yet
  seen is compiled on the player's machine.
- Rebuild from scratch: `.\tools\build.ps1 -Disc <cue> -Package`.

## What works (verified this session)
- Boot (retail BIOS or OpenBIOS), intro movies with audio, attract mode, title,
  menus, 1-player race (DOCK, SWAMP), battle arena, 2-player split-screen race,
  memory-card save then load after restart, Options > Set Controllers.
- 0 interpreted instructions in every exercised scene after compile.
- Video defaults: display resolution x1.5 dynamic supersampling, FXAA,
  perspective-correct textures, mipmapped/16x anisotropic textures, FMV chroma
  smoothing. Geometry correction intentionally off (framework: known broken
  outside the PGXP build).
- Widescreen (default on, Fit to Window): projection squash + GTE gameplay
  detection + auto_ui_squash HUD; menus/movies/2D stay 4:3; split screen OK.
- Controllers: DualShock presentation (stick steering, vibration option),
  triggers = accelerate/brake (default on), launcher rebinding; keyboard.
- Fast Loading 4x (default on), Skip Intro Movies (off), Smooth Motion (off).

## What the runtime still interprets
- Nothing in the exercised scenes. A movie-player page variant (page
  0x800E4000 mixes code with rewritten data, ~70 variants seen) that has not
  been compiled yet runs interpreted until autocompile builds it; correct, and
  the game ran at full speed even fully interpreted.

## game.toml changes from the scaffold defaults, and why
- `[video]` modern defaults (owner request); dynamic resolution after
  measuring 15.4 ms GPU at 4K x1.5.
- `[controller] default_mode = "analog"`: the game supports vibration and folds
  the stick into steering (measured).
- `[widescreen]`: `native_wide = false` (native-wide overwrote texture VRAM),
  `gte_game_mode = true` (3D title, no sprite tags), `auto_ui_squash = true`,
  `hud_sprt_squash = false` (corrupts textures built with SPRT); `auto_screen_x`
  finds no sites (harmless).
- `[game] name` and window title: "Muppet RaceMania".
- Seed 0x80060160 (function-pointer target the JAL scan missed).

## Most likely remaining problems
1. Widescreen edge pop-in: world-space culling still uses the 4:3 frustum, so
   some trackside objects may appear late at the extreme edges at 21:9/32:9.
   Fix would be cull-site work ([widescreen.cull] sites) found by RE.
2. Tracks/modes not driven this session (tournament progression, bosses, all
   courses, unlocks). They run the same static EXE, so the risk is low, but a
   new FMV variant or rare path may briefly run interpreted.
3. Accelerate = Triangle was taken from the quarry's verified note, not
   re-measured; if a trigger feels wrong, the mapping is one line in
   src/mods/mrm_modern_controls_plugin.c.

## Framework proposals (not applied)
See docs/history/PLAN.md "Framework findings" and docs/history/notes/PROPOSAL-psxrecomp-msvc.md.

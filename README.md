# Muppet RaceMania Recompiled

A native Windows build of **Muppet RaceMania** (PlayStation, USA, SLUS-01237),
made with the [PSXRecomp](https://github.com/RetroPortingToolKit/psxrecomp)
static recompiler. The original PlayStation program is machine-translated to C
and compiled to native x64 code. It is not an emulator running the game.

This repository holds the project: configuration, seeds, mod packages, build
scripts and notes. **It contains no game data and no generated game code.**
You build the game from your own disc.

## What you get

- The whole game, native: the main program is recompiled ahead of time, and the
  code the game loads from disc at runtime (the movie player and memory-card
  screens) is compiled to native modules on first sight and cached.
- **High resolution**: renders at your monitor's resolution with
  supersampling (dynamic, so it never drops below display resolution), FXAA,
  perspective-correct textures and mipmapped/anisotropic texture filtering.
- **Widescreen**: a genuinely wider view (more track either side), with the HUD
  kept at its original proportions and pinned to the screen edges. Menus,
  movies and loading screens stay 4:3. Fit to Window, 16:9, 21:9 or 32:9.
- **Controllers**: any SDL-supported gamepad (Xbox, PlayStation, Switch Pro, ...)
  works out of the box and can be rebound in the launcher. The game sees a
  DualShock: the left stick steers and vibration works (turn it on in the
  game's Options > Set Controllers). **Right trigger accelerates, left trigger
  brakes.** Keyboard: arrows steer, A = Triangle (accelerate), S = Circle
  (brake), Z = Square (jump), X = Cross, Enter = Start; rebindable.
- **Fast Loading** (on by default, 4x), **Skip Intro Movies** (off by default),
  **Smooth Motion** frame blending (off by default; the game renders at 30 fps).
- Memory-card saves (`saves/` beside the game), save states, fullscreen (F11 or
  Alt+Enter), all from the launcher.

All of these live in the launcher's **Mods** and **Settings** pages.

## Playing (portable build)

Unzip `mrm-<version>-windows-x64.zip` anywhere and run
`Muppet_RaceMania_Recompiled.exe`. On first run the launcher asks for your disc
image (`.cue` + `.bin`). It uses the bundled, open-source OpenBIOS unless you
select your own SCPH-1001 BIOS dump in Settings.

## What you must supply

- Your own dump of the disc: *Muppet RaceMania (USA)*, Redump track 1 MD5
  `ee4160b222e47182bc688c49bd65b2f8`.
- Optional: your own SCPH-1001 (v2.2) BIOS dump. Not required.

## Building

Requirements: Git for Windows, Python 3, and psxrecomp's `cmake-clang-v1`
Windows toolchain pack (see `tools/devshell.ps1` and `BINLIST.toml` for the
exact release and hash). Then, from a clone with submodules:

```powershell
.\tools\build.ps1 -Disc "D:\dumps\Muppet RaceMania (USA).cue" -Package
```

That builds the recompiler tools, generates the game C from your disc into
`generated/` (gitignored), builds `build-release/`, and writes the portable zip
to `dist/`.

## Layout

| Path | What |
|---|---|
| `game.toml` | Game identity, recompiler input, runtime/video/widescreen/controller defaults (commented with evidence) |
| `seeds/` | Function entry seeds for the recompiler |
| `mods/preloaded/packages/` | This title's mod packages (widescreen, controls, fast loading, skip movies, smooth motion) |
| `src/mods/` | Their trusted plugins (compiled into the exe) |
| `tools/` | `build.ps1`, `devshell.ps1`, `run-dev.ps1` + `drive.py` (debug-build test harness) |
| `docs/` | Plan, milestones, session notes and handoff |
| `psxrecomp/`, `recomp-ui/` | Framework and launcher submodules (pinned) |

## Legal

You must own the original game. Disc images, BIOS dumps, generated game C,
overlay captures and caches are never committed. PSXRecomp is licensed
PolyForm Noncommercial 1.0.0; see `psxrecomp/LICENSE`.

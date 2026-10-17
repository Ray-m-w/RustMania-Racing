# Muppet RaceMania: Rust + Bevy reimplementation

WHAT THIS IS (final state, 2026-10-17)
A complete reimplementation of Muppet RaceMania (PS1, SLUS-01237) in Rust +
Bevy, in port/. It reads the owner's own disc at run time and contains no game
data. Every mode, menu, movie and sound of the original is ported and was
checked tick for tick against a recompiled build of the original (the oracle).
Summary and numbers: docs/FINAL-REPORT.md. Specs: docs/spec/. History of how
it was built (session briefs, logs, plans): docs/history/.

LAYOUT
- port/            the game: Cargo workspace (mr-disc, mr-assets, mr-audio,
                   mr-video, mr-sim, mr-trace = std only; mr-bevy = the app).
                   port/README.md: build, run, options, tools.
- docs/spec/       how the game works, per subsystem, with code addresses;
                   *-ledger.md = every deliberate difference from the original.
- tools/oracle/    capture / compare tooling against the oracle (dev only).
- tools/package-port.ps1   the shipped zip (GUI exe, static CRT, no game data).
- game.toml, CMakeLists.txt, src/mods/, mods/, seeds/   the psxrecomp game
                   project that builds the oracle (psxrecomp + recomp-ui are
                   pinned submodules, psxrecomp 2a4824fc).

RULES (still binding for any further work)
- Never commit or package game data: disc images, extracted files, generated
  game C, captures, screenshots of RAM/VRAM dumps, overlay captures, BIOS.
  analysis/, build-*/, generated/, saves/, dist/ and port/target*/ are
  gitignored.
- The original's code is the spec: port behaviour, never copy generated C.
  Integers exactly as the game in mr-sim; constants read from the exe/disc at
  run time with their address cited.
- New crates or tools need the owner's approval and a BINLIST.toml entry.
- Keep `cargo test --workspace --offline --release` green and mr-bevy building
  without warnings. Commit locally; the owner pushes.
- Log sessions to docs/history/notes/YYYY-MM-DD.md (append only).

BUILD AND RUN
- `cd port; cargo run --release -p mr-bevy` (disc from settings.txt `disc =`,
  $MRM_DISC, or game.toml's `disc` line). `tools\play-port.ps1` wraps it.
- Package: `powershell -File tools\package-port.ps1` -> dist\mrm-port-<v>-windows-x64.zip.
- User data: %APPDATA%\MuppetRaceMania (save.bin, settings.txt, crash.txt).

ORACLE (only for verification work)
- The oracle is the psxrecomp debug build of the original: build it from your
  disc (psxrecomp docs; tools\build.ps1), then `.\tools\run-dev.ps1 -Start`
  (TCP 4370, oracle-bridge plugin) and `-Stop`. Captures:
  tools\oracle\capture*.py, compared by mr-sim's check bins (port/README.md).
- Toolchain: psxrecomp's cmake-clang-v1 pack via `. .\tools\devshell.ps1`
  (MSVC cannot build this pin; Windows PowerShell 5.1 breaks its scaffold
  script: docs/history/notes/2026-10-11.md).
- Savestate slots 1 and 2 are the reference states; never overwrite them.
  Never let the oracle write the owner's memory card.

GAME FACTS WORTH KNOWING
- Logic 30 Hz (2 VBlanks per tick; 3 in heavy frames); the port defaults to a
  dt-correct 60 Hz mode (`logic_hz`), 30 Hz stays tick-exact.
- CROSS accelerates, CIRCLE brakes, SQUARE jumps, TRIANGLE locks the camera
  on (layout B swaps; save byte 0x800E0392 + player).

# Muppet RaceMania Recomp

WHAT THIS IS
A game project for PSXRecomp (RetroPortingToolKit/psxrecomp): the original
PS1 executable machine-translated to C and compiled to native x64, linked
against PSXRecomp's runtime. Goal: a standalone Windows build of Muppet
RaceMania that the owner can launch and play. Session 1 ends at the title
screen booting natively. Session 2 ends at a full race being playable.

SOURCES OF TRUTH, IN ORDER
1. PSXRecomp's own documentation in the pinned checkout: README.md,
   CLAUDE.md, CONTRIBUTING.md, docs/GAME_PROJECT_SETUP.md,
   docs/BUILDING.md, docs/EXECUTION_MODEL.md, docs/ARCHITECTURE.md,
   docs/AOT_SHARDING.md, docs/TCP_COMMANDS.md. Read these before running
   anything. Quote commands from them; never invent a flag.
2. The existing game repos psxrecomp lists (mstan/TombaRecomp etc.) as
   worked examples of game.toml, CMake wiring and release layout.
3. The owner's reimplementation repo at F:\Muppet Racemania, READ-ONLY.
   It holds verified facts about this exact disc; see QUARRY below.
4. Your own inference, labelled as such.

RULES
- Game content comes only from the owner's disc dump. Never download game
  content, executables, overlays, or assets. Never commit disc data,
  generated game C, or overlay captures (overlay_captures.json holds
  verbatim game code and stays local; psxrecomp's README says so).
- Every outside artifact gets a line in BINLIST.toml: what it is, where
  it came from (owning organization's own repo or release), the tag or
  commit, the SHA-256 you computed, the published hash if one exists,
  and the date. For git checkouts the pinned commit hash is the identity;
  record it. For winget installs, record the package id, version, and
  that winget verified the manifest hash.
- Never hand-edit generated C. Fix the configuration or the recompiler
  input and regenerate. (This is psxrecomp's rule too.)
- Installing or upgrading any tool, and adding any dependency, is a
  command the owner approves through this tool's permission prompt;
  state in one line what it is and why before you run it.
- Build output, generated/, disc images and captures are gitignored.
- Log every session to docs/notes/YYYY-MM-DD.md: what you did, what
  worked, what failed and why, and the exact next action. Append-only.
- Stop and wait for the owner when: the session brief says to; a step
  fails three times; psxrecomp's docs and its behavior disagree and the
  fix would mean modifying the framework (propose it instead); you need
  the owner to look at the screen.

QUARRY: WHAT TO TAKE FROM F:\Muppet Racemania (copy, never modify)
- BINLIST.toml: the disc's Redump-verified hashes, the PS-X EXE header
  (PC0 0x80093214, t_addr 0x80010000, t_size 0xA1000), SYSTEM.CNF, BIOS
  verification results for scph7001 / scph1001 / scph101, and the
  verified chdman version and command lines.
- extract/ (gitignored there): the bin/cue already extracted from the CHD.
  If present, reuse it after re-verifying its hashes against BINLIST.toml.
  If absent, re-extract from F:\ROMs\Sony - PlayStation\Muppet RaceMania
  (USA).chd with chdman exactly as tools/extract/ documents.
- docs/spec/symbols.md: 1350 functions with PSYQ names and addresses
  (main @0x800786b4, VSync @0x800983c0). Useful later for widescreen cull
  sites and mods; not needed for boot.
- docs/spec/kernel-usage.md and kart-state.md: kernel-call census and
  the kart struct. Not needed for boot; read if a runtime issue points
  at threads, callbacks, or pad handling.
- docs/notes/: the game's boot timeline under PCSX-Redux (entry at frame
  473; intro movies run to ~frame 27,400; START at 27,600 reaches the
  attract configurator; the real menu route is START/UP/CROSS...). Use
  this as the expected behavior of a correct build.
- tools/redux/ and procguard.ps1: the Redux harness, if you need a
  reference emulator run to compare against, and the process-guard
  pattern (no leftover emulator or game processes after a run).

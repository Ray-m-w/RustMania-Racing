# SESSION 1: NATIVE BOOT TO TITLE

Work in this directory. Read psxrecomp's documentation (CLAUDE.md source 1)
before step 2. Do not skip ahead to building the game; the framework must
be built and understood first, in its own checkout, as its docs require.

STEP 1. INVENTORY THE MACHINE  ->  docs/history/notes/SESSION-1-tools.md
   Record what is already installed and its version: Git (and the bash it
   ships), Python 3, CMake, Ninja, Visual Studio Build Tools (the owner has
   17.14.39 with Windows SDK 10.0.26100, owner-installed), rustup (not
   needed here). For anything missing that psxrecomp's BUILDING.md
   requires, propose the install (winget where available) and wait for
   approval. Prefer the owner's existing Build Tools over psxrecomp's
   bundled toolchain zip unless BUILDING.md says the bundled one is
   required; if you do fetch the bundled toolchain, its release page is
   the hash source.

STEP 2. PIN AND BUILD THE FRAMEWORK  ->  BINLIST.toml, docs/history/notes/
   1. Clone RetroPortingToolKit/psxrecomp with submodules into a sibling
      directory, not inside this repo (psxrecomp's README is explicit that
      game projects are not built inside the framework folder). Record the
      commit hash in BINLIST.toml. Confirm the organization name against
      the README's own text and the retroportingtoolkit.com link.
   2. Follow BUILDING.md to build the recompiler tool, generate the OpenBIOS
      backend, and build the runtime. Quote each command from the docs.
      Record every error and how it was resolved.
   3. Retail BIOS: psxrecomp's README says the retail backend is a specific
      SCPH1001.BIN image and a different dump cannot be swapped in. The
      owner's dump is F:\BIOS\scph1001.bin. Hash it, compare with what
      psxrecomp expects (the docs or BIOS_SELECTION.md will say how it
      identifies the image), and record the result. If it matches, generate
      the retail backend too. If not, OpenBIOS is the BIOS for this project
      and the note says so; do not go looking for another image.

STEP 3. SCAFFOLD THE GAME PROJECT  ->  this directory
   1. Disc: locate the bin/cue per QUARRY, re-verify hashes, and keep it
      outside this repo or under a gitignored path. The .cue is what the
      setup script takes.
   2. Run psxrecomp's setup_project.ps1 exactly as GAME_PROJECT_SETUP.md
      documents, with the .cue and (if STEP 2.3 passed) the BIOS, targeting
      this directory. Answer Y to Generate. Record the project name you
      chose and every prompt's answer.
   3. Inspect what it generated: game.toml, submodules (psxrecomp/ and
      recomp-ui/ at the root, pinned to the commits you recorded), the
      build script, mods/preloaded. Commit the scaffold without generated/
      or disc data. Write a README.md that says what this repo is, what
      must be supplied locally (disc, BIOS), and the one command to build.

STEP 4. FIRST BUILD AND FIRST BOOT  ->  docs/history/notes/SESSION-1-boot.md
   1. Build with the generated build script. Record warnings that mention
      unsupported instructions, unresolved jumps, or overlays.
   2. Launch the result. You cannot see the window; use the runtime's
      documented debug interface (docs/TCP_COMMANDS.md) or its logs to
      observe state, and psxrecomp's screenshot facility if it has one.
      Expected behavior from the quarry notes: a BIOS boot, then intro
      movies for several minutes of game time (use the documented turbo or
      fast-loading options only to get through them faster; note that they
      were used), then the attract screen that accepts START. Entry to the
      game's main at 0x80093214 and the first VSync() at 0x800983c0 are
      addresses you can watch for if the debug interface exposes PC.
   3. If it crashes or hangs: capture the runtime's diagnostics, locate the
      guest PC, and look it up in the quarry's symbols.md. Classify: an
      overlay not yet captured (the runtime's own capture-and-compile loop
      handles this; let it), an MDEC/XA/CD path issue (check EXECUTION_MODEL
      and the game-repo examples for the relevant game.toml settings), or
      a recompiler limitation (propose a framework fix; do not patch
      generated C). Three attempts per distinct failure, then stop.
   4. When the START-accepting screen is reached, ask the owner to look and
      confirm; save the screenshot; record BIOS used, commit, native vs
      interpreted ratio as the runtime reports it, and the input mapping
      that worked.

STEP 5. CLOSE  ->  docs/history/notes/YYYY-MM-DD.md
   Append the session log. Write docs/history/notes/SESSION-1-handoff.md: what
   works, what the runtime still interprets, which game.toml settings were
   changed from the scaffold defaults and why, and the three most likely
   problems a full race will hit. Commit. STOP. The owner reviews and
   writes docs/history/SESSION-2.md.

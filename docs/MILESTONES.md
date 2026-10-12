SESSION 1: NATIVE BOOT TO TITLE
Done when: the generated project builds from a clean checkout; the owner
double-clicks the result (or runs the documented launcher); the game boots
on the retail BIOS or OpenBIOS, plays its intro movies (MDEC and XA audio
working), and reaches the screen where START is accepted; a screenshot of
that screen is saved under docs/notes/; the session log records which BIOS,
which psxrecomp commit, and how much of execution was native vs interpreted
as the runtime reports it.

SESSION 2: A FULL RACE
Done when: from the title, the owner can navigate the menus with keyboard
or controller, start a 1 PLAYER RACE, complete all laps, and return to the
menu, with no crash, no hang, and no interpreter fallback that the runtime
reports as a performance problem; overlays reached during the race are
captured and compiled so a second run is native; load times and audio are
acceptable to the owner. Stretch, only if everything above is done:
widescreen per docs/WIDESCREEN.md.

SESSION 1 OUTCOME (2026-10-12)
Session 1 delivered the native boot AND, at the owner's request, the full
Session-2 goal above (complete, playable, portable build). See
docs/notes/SESSION-1-handoff.md.

BEVY PORT (from 2026-10-12)
Eventual goal: 100% Rust + Bevy reimplementation that feels the same, using the
original assets. Hybrid first, one subsystem at a time, verified against the
recomp. Milestone 1 (docs/SESSION-2.md): one kart driving one track in Bevy
with handling matching the recomp within a measured tolerance.


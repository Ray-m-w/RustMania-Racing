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

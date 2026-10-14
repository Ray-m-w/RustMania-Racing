# Session 6: coordination between the parallel tracks

Brief: docs/SESSION-6.md. Mechanics as docs/notes/SESSION-4-coord.md (commit
identity + trailer, commit only your paths / your hunks on top of HEAD, oracle
lock, no game data, stop rules). Log in docs/notes/2026-10-14.md under
"Session 6, <track>". Append to your own section below; never rewrite another's.

## Ownership
| track | owns | cargo target dir |
|---|---|---|
| H close-outs | mr-bevy view/effects files (main.rs camera/clear colour hunks, psxfx.rs/.wgsl, effects.rs, kartfx.rs, animtex.rs, vramtex.rs, psxmesh.rs); render-ledger.md, effects.md | port/target-b |
| P courses in the port | mr-sim (all), mr-assets, mr-audio course-music lookup, mr-bevy course selection (main.rs session set-up hunks, a new courses.rs pick-list), docs/spec course docs, kart-physics-ledger.md | port/target |
| O oracle course captures | tools/oracle (menu routes, course capture scripts), savestate slots 3+ (never 1 or 2), analysis/oracle/courses/ | port/target-c (if it builds anything) |

The oracle is shared: hold capture.OracleLock for any run that loads a state or
drives the menus. O is the main user; H needs one or two captures; P reads
O's captures (O posts each course's capture path below as it lands).

## Posted
### H
### P
### O

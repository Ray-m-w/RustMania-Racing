# Session 3, item 3: the AI (report, 2026-10-12)

Model and pseudocode: docs/spec/ai.md. Code: port/crates/mr-sim/src/ai.rs,
the harness and CLI src/bin/mr-ai/ (main.rs, harness.rs), tests/ai.rs. Not
one-for-one items: the revisit list at the top of
docs/spec/kart-physics-ledger.md.

## 1. Oracle data (analysis/ai/, local)
Savestate slot 2 (DOCK, 1-player race, Kermit + 5 AI karts), 9,630
VBlanks (4,815 ticks, 3 full laps for every AI kart), `capture.py
--physics` regions (all 6 kart records, globals, scratchpad) plus the BIOS
rand() seed 0x80009010 (added to PHYSICS_REGIONS this session). Player
routes keep out of the AI's way (analysis/ai/r_*.json): idle; brake/reverse
for 1 s; reverse for 6.7 s; nudge forward 0.5 s then back; reverse-left
3.3 s; SELECT then a brake tap. The routes change the shared random streams,
so the six races differ (different branches, contacts, finishing orders).

| set | runs | items |
|---|---|---|
| c_idle, c_rev60, c_rev, c_nudge, c_left, c_sel | 6 | on (weapons fired 60+ times, specials 1,724 kart-ticks, turbo 5,680) |
| n_idle9600, n_rev60, n_rev, n_nudge, n_left, n_sel | 6 | off: tools/oracle/capture_noitems.py holds every kart's weapon charge timers at 0xFF and turbo meter at 0 by TCP `write_ram` as soon as a slot shows them (530-680 pokes per race); 0 boost or special ticks in all six |

Item-affected ticks are identified in item-on runs from the record (+0x4D6
special, +0x4C4 boosting; weapon hits = the 13 velocity mismatches of the
update teacher forcing); with items on hardly a lap is clean, so the
tolerance uses the items-off set. Deterministic items-off needs bridge v4
(below).

The AI karts in this race: slot 1 Clifford (21) on the TRIKE (26), 2 Bobo
(14) W_BUGGY (13), 3 Sal (23) FLAME (18), 4 Beaker (3) M_BUGGY (7), 5 Chef
(10) HOTDOG (9).

## 2. Port verification
- Decisions (FUN_800912F8, teacher forced from the oracle's records,
  `mr-ai tf analysis/ai/c_rev`): 23,206 kart-ticks; steering, accelerate,
  brake, turbo, special, target point (+0x3E8) and curvature (+0x4A0) exact
  on all; the complete button word exact on 23,202 (two frames where other
  code drew rand() before the AI).
- Kart update on AI karts with the oracle's buttons (`mr-ai tfk`): position,
  angles, speeds, lap and lap times exact on every kart-tick but item events
  (13 weapon hits, 34 weapon fires out of 14,569).
- Vehicle templates (all 48, decoded from FUN_80087EB4 + models): 47
  byte-identical to RAM, the 48th (boss LOG) differs only by wheel positions
  left from an earlier load.
- Opponent records: static fields equal the oracle's for all five.
- Fixed on the way: init.rs read the character camera-mount table at
  2 x character instead of 4 x (only Kermit, 0, was right);
  KartState::wheels holds 6 wheel blocks (vehicles have 0..6).

## 3. Tolerance (from the oracle's spread) and the comparison
Port runs: `mr-ai free CAPTURE --out CSV [--seed S]`: the five AI karts
from the oracle's state at the savestate, ai.rs driving mr-sim's kart
update, kart-kart contacts among them, the player replayed from the
oracle, items off. 18 runs (each items-off capture with its own seed and
two more seeds). Statistics: `python tools/oracle/ai_stats.py --oracle
analysis/ai/n_* --port analysis/ai/pn_*.csv analysis/ai/pns*.csv`.

Oracle spread (items off, 6 races, 18 laps per kart): lap time sd per kart
434..758 (3.3..5.6 % of the mean); the mean of one race varies by 1.1..2.7 %
(sd over races); the standard error of a 6-race mean is 0.8..1.3 %. Line:
the per-race median |lateral| / width varies 0.071..0.162 (kart 4 widest),
p90 0.27..0.45. Track width at the AI karts' positions: median 3,192 TER
units = 102,144 kart units (the brief's 116,862 is over the path points).

Tolerance (set from that spread, means over >= 6 races):
- lap time: each kart's mean within max(3 %, 2 standard errors of the
  difference) of the oracle's mean (3 % = 2.3..3.8 standard errors of a
  6-race oracle mean); all karts pooled within 1.5 %;
- line: |lateral| / width median and 90th percentile within 0.10 of the
  oracle's (the brief's 10 % of track width; the oracle's own race-to-race
  range is up to 0.09 for the median, 0.17 for p90);
- finishing order: each kart's mean finishing position within 1.5 places.

Result (6 oracle races vs 18 port races):

| kart | oracle lap mean (sd) | port lap mean (sd) | delta | line median o / p | p90 o / p | mean position o / p |
|---|---|---|---|---|---|---|
| 1 Clifford TRIKE | 12,946 (434) | 12,944 (449) | -0.0 % | 0.090 / 0.091 | 0.339 / 0.341 | 1.83 / 1.78 |
| 2 Bobo W_BUGGY | 13,396 (601) | 13,432 (630) | +0.3 % | 0.096 / 0.091 | 0.348 / 0.349 | 3.83 / 3.83 |
| 3 Sal FLAME | 12,988 (669) | 12,907 (547) | -0.6 % | 0.096 / 0.092 | 0.358 / 0.323 | 2.00 / 1.83 |
| 4 Beaker M_BUGGY | 13,575 (758) | 13,580 (865) | +0.0 % | 0.121 / 0.111 | 0.409 / 0.391 | 4.17 / 4.39 |
| 5 Chef HOTDOG | 13,227 (644) | 13,329 (595) | +0.8 % | 0.088 / 0.092 | 0.329 / 0.350 | 3.17 / 3.17 |
| pooled | 13,226 (674, n 90) | 13,239 (687, n 270) | +0.09 % (se 0.62 %) | | | |

All inside the tolerance (PASS). Lap times are race-clock units (+10 per
tick; 300 = 1 s): ~44 s a lap. Finishing orders: oracle winners Clifford 2,
Sal 4 of 6; port Clifford 9, Sal 8, Chef 1 of 18.

With items on (c_*), the oracle AI is faster and far more spread out:
pooled lap mean ~12,020, sd 1,280..1,760 per kart (turbo in water, specials,
weapon hits); the items-off port is 8..13 % slower per kart than that, as expected.

## 4. API for N AI karts (for the Session integration)
`ai.rs` needs `pub mod ai;` in lib.rs (until then the bin and tests include
it with #[path]). Race load, in the game's order (FUN_8008C4AC):
```rust
let tpl = ai::vehicle_templates(&w.exe, &w.tables)?;           // 48 x 0x174
let grid0 = ai::grid_start(&mut g, &w, karts);                  // *rng % karts
// humans: init_kart(slot 0, grid0, flags 0); then per AI slot s in humans..karts:
let (c, v) = ai::pick_opponent(&w.exe, &taken, &|_| true, &|_| true, &mut rand);
let mut t = tpl[v as usize];
ai::place_wheels(&mut t, &Mdl::parse(&disc.read(&ai::model_path(&w.exe, ai::addr::VEHICLE_PATHS, v, "MDL").unwrap())?)?);
let cm = Mdl::parse(&disc.read(&ai::model_path(&w.exe, ai::addr::CHARACTER_PATHS, c, "MDL").unwrap())?)?;
let (rec, k) = ai::init_opponent(&w, &t, setup, ai::Opponent { slot: s, character: c, vehicle: v, grid },
                                 cm.parts[0].record.pivot[1], &mut g.rng, &mut rand);
// after all karts:
ai::race_load_roll(&mut karts, &mut g, &w, karts_n);           // FUN_8009192C(&kart0, n)
```
(`rand` = the BIOS rand() closure, `session::bios_rand(&mut seed)`; the
character models also feed the renderer.) Per tick (FUN_8007916C order):
1. contact pass over every pair (j < i) — today `harness::contacts` (calls
   collide::frame_contacts once per kart); collide.rs should get the full
   pair loop;
2. `progress::path_position(&mut karts[rr])`, `rr = (rr + 1) % karts` (one
   kart per frame; start value = the race's frame counter mod karts);
3. for each kart in slot order: AI (flags & 1):
   ```rust
   let ctx = ai::AiContext { karts, kart0_flags: karts[0].flags, kart0_race_pos: karts[0].race_pos,
                             difficulty: 8, cup: 4, driver_pos: &[] };
   let b = ai::items_off(ai::ai_buttons(&mut k, &mut ai_state[s], &mut g, &ctx, &w, &mut rand));
   update::kart_update(&mut k, &mut g, &w, &inp, b, -1);
   ```
   humans as now (kart_update + track_direction);
4. after the loop: render::after_kart_loop for every kart (the contact pass
   reads the look point +0x324), race positions (+0x388 / +0x4BE) for every
   kart (FUN_800837F8, the race-flow agent's), camera.
Per AI kart keep a `KartState` and an `ai::AiState`. The finished player
(handed to the AI, flags |= 1) uses the same `ai_buttons`.

Two lines in update.rs must change for AI karts (I did not edit it: the
race-flow agent owns it). Until then `harness::ai_kart_tick` wraps the
update (clears flag bit 0 and does both after it):
```rust
// 0x80065850, replace `unreachable_in_scope!("AI catch-up scaling")` (keep the if):
crate::ai::catchup_scale(k, g, DIFFICULTY /* 0x800E039A: 8 */, CUP /* 0x800B0E38: 4 */);
// 0x80068078, replace `unreachable_in_scope!("respawn FUN_8009192C")`:
crate::ai::reroll(k, g, w, g.karts);
```
(`catchup_scale` checks the AI / split-screen / race-over conditions
itself; RaceGlobals may want `difficulty` and `cup` fields.) The countdown
"copy the buttons" branch at 0x80064BA8 is skipped for AI karts by flag
bit 0 already, so with the hooks in, AI karts can run from the countdown.

## 5. Bridge v4 (needs a rebuild + oracle restart; owner coordinates)
src/mods/mrm_dev_oracle_bridge_plugin.c (v4, "Local\mrm_oracle_bridge_v4"):
up to 64 RAM pokes armed with a route, applied in the restore callback
before the RAM snapshot and/or at every VBlank until the next restore
(`Bridge.arm_pokes`, `capture.py --no-items`, route "pokes"). Checked with
clang -fsyntax-only (header size static-asserted); not built or run.
tools/oracle/bridge.py accepts v4 and v3 (v3: no pokes), so the running
oracle keeps working. With v4 the items-off captures become deterministic
(today's TCP pokes land a frame or two after a pickup).

## 6. Next
- Wire the two update.rs hooks, `pub mod ai;`, the all-pairs contact loop,
  per-kart race positions; spawn the five opponents in Session.
- Re-take the six items-off races with bridge v4 (`capture.py ROUTE OUT
  --physics --no-items`) and re-run ai_stats.py.
- Items session: un-mask L1/L2/R1/R2 in `ai::items_off` once weapons,
  tokens and specials are ported.

"""Capture an items-on race route (Session 4, track A): the physics regions,
the race-flow state of capture_race.py, and the item state:
projectiles 0x800DDC50 (4 x 0x48, FUN_80069648), pickup objects 0x800E1B70
(192 x 0x20: item boxes 0..0x2F, challenge objects 0x30..0x3F, turbo tokens
0x40..; FUN_8007BBF0), the R2-special pointer table 0x800DD598.

    python tools/oracle/capture_items.py ROUTE.json OUTDIR [--slot N]

Items stay ON (no pokes). Holds capture.OracleLock for the run.
"""
import json
import os
import sys

sys.path.insert(0, os.path.dirname(__file__))
import capture  # noqa: E402

ITEM_EXTRA = [
    (0x800DDC50, 0x120),   # projectiles (FUN_80068C6C / FUN_80069648)
    (0x800AFEF0, 0x20),    # water animation state
    (0x800E0180, 0x240),   # results / record tables
    (0x800E1B70, 0x1800),  # pickup objects (FUN_8007B448 / FUN_8007BBF0)
]
ITEM_REGIONS = capture.PHYSICS_REGIONS + ITEM_EXTRA

if __name__ == "__main__":
    route = json.load(open(sys.argv[1]))
    slot = int(sys.argv[sys.argv.index("--slot") + 1]) if "--slot" in sys.argv else None
    m = capture.run(route, sys.argv[2], slot=slot, regions=ITEM_REGIONS)
    print("captured vb", m["first_vb"], "..", m["last_vb"], "missing", len(m["missing"]))

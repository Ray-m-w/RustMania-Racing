"""Capture a race route with the physics regions plus the race-flow state
(projectiles 0x800DDC50, water animation state 0x800AFEF0, the BIOS rand seed
0x9010, results / record tables 0x800E0180..).
    python tools/oracle/capture_race.py ROUTE.json OUTDIR [--slot N]
"""
import json
import os
import sys

sys.path.insert(0, os.path.dirname(__file__))
import capture  # noqa: E402

EXTRA = [(0x800DDC50, 0x120), (0x800AFEF0, 0x20), (0x80009010, 0x4), (0x800E0180, 0x240)]

if __name__ == "__main__":
    route = json.load(open(sys.argv[1]))
    slot = int(sys.argv[sys.argv.index("--slot") + 1]) if "--slot" in sys.argv else None
    m = capture.run(route, sys.argv[2], slot=slot, regions=capture.PHYSICS_REGIONS + EXTRA)
    print("captured vb", m["first_vb"], "..", m["last_vb"], "missing", len(m["missing"]))

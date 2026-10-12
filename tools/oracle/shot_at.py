"""Replay a route from the grid savestate and screenshot the game at a VBlank.

    python tools/oracle/shot_at.py ROUTE.json VBLANK OUT.png

Uses the native VRAM capture (`screenshot`, 4:3 display buffer, unaffected by
the widescreen presentation mod). The capture lands within a frame of VBLANK
(TCP latency); the actual VBlank is printed.
"""
import json
import os
import sys
import time

sys.path.insert(0, os.path.dirname(__file__))
from bridge import Bridge  # noqa: E402
from capture import active_high  # noqa: E402
from oracle import Oracle  # noqa: E402

route = json.load(open(sys.argv[1]))
target = int(sys.argv[2])
out = os.path.abspath(sys.argv[3]).replace("\\", "/")
o, b = Oracle(), Bridge()
steps = [(st[0], active_high(st[1]), 0x80, 0x80, 0x80, 0x80) for st in route["steps"]]
b.arm_route(steps, end_vb=max(target + 120, int(route.get("end_vb", 0))))
g = b.restore_gen
o.load_state(route.get("slot", 2))
b.wait_restore(g)
while b.vb_since_restore < target:
    time.sleep(0.001)
r = o.q("screenshot", path=out)
print("vblank", b.vb_since_restore, r)

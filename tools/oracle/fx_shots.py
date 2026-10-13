"""Replay a route once and take native VRAM screenshots at several VBlanks.

    python tools/oracle/fx_shots.py ROUTE.json OUTDIR VB [VB ...]

Like shot_at.py (the `screenshot` display-buffer capture, unaffected by the
widescreen presentation mod) but one run for many frames; writes
OUTDIR/o_<VB>.png and prints the VBlank each capture actually landed on.
Holds capture.OracleLock for the run.
"""
import json
import os
import sys
import time

sys.path.insert(0, os.path.dirname(__file__))
from bridge import Bridge  # noqa: E402
from capture import active_high, OracleLock, warm_up  # noqa: E402
from oracle import Oracle  # noqa: E402

route = json.load(open(sys.argv[1]))
outdir = os.path.abspath(sys.argv[2])
os.makedirs(outdir, exist_ok=True)
targets = sorted(int(x) for x in sys.argv[3:])
with OracleLock():
    o, b = Oracle(), Bridge()
    warm_up(o, b, route.get("slot", 2))
    steps = [(st[0], active_high(st[1]), 0x80, 0x80, 0x80, 0x80) for st in route["steps"]]
    b.arm_route(steps, end_vb=max(targets[-1] + 120, int(route.get("end_vb", 0))))
    g = b.restore_gen
    o.load_state(route.get("slot", 2))
    b.wait_restore(g)
    for t in targets:
        while b.vb_since_restore < t:
            time.sleep(0.001)
        path = os.path.join(outdir, f"o_{t}.png").replace("\\", "/")
        r = o.q("screenshot", path=path)
        print("target", t, "vblank", b.vb_since_restore, r.get("ok"), flush=True)

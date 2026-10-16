"""Run a route from its slot and save the state at a VBlank into the scratch
slot 11, archived as analysis/states/courses/NAME_KIND.pst (Session 7,
phase 6: the ADVENTURE start for poked captures).

    python tools/oracle/save_at.py ROUTE.json VB NAME KIND

Holds the oracle lock; the route's slot is only loaded (never slots 1 / 2
written)."""
import json
import os
import sys
import time

sys.path.insert(0, os.path.dirname(__file__))
import goto_course as gc  # noqa: E402
from bridge import Bridge  # noqa: E402
from capture import OracleLock, active_high, warm_up  # noqa: E402
from oracle import Oracle  # noqa: E402

route, at, name, kind = json.load(open(sys.argv[1])), int(sys.argv[2]), sys.argv[3], sys.argv[4]
with OracleLock():
    o, b = Oracle(), Bridge()
    warm_up(o, b, route["slot"])
    b.arm_route([(s[0], active_high(s[1]), 0x80, 0x80, 0x80, 0x80) for s in route["steps"] if s[0] < at], end_vb=at + 600)
    g = b.restore_gen
    o.load_state(route["slot"])
    b.wait_restore(g)
    while b.vb_since_restore < at:
        time.sleep(0.002)
    print("saving at vb", b.vb_since_restore, "mode", int.from_bytes(o.read(0x800B0D46, 2), "little"))
    gc.save_state(o, gc.SCRATCH)
    print(gc.archive(gc.SCRATCH, name, kind))

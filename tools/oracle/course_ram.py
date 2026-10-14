"""Session 6 R: consistent RAM snapshots of a course run from its grid
savestate: at VB (after the load) read GUARD, the regions, GUARD again,
until the two GUARD reads agree (the regions then belong to one frame).

    python tools/oracle/course_ram.py NAME VB OUT.json GUARD_ADDR:LEN ADDR:LEN...

Scratch slot 11 only, under the oracle lock."""
import json
import os
import sys
import time

sys.path.insert(0, os.path.dirname(__file__))
from bridge import Bridge  # noqa: E402
from capture import OracleLock, warm_up  # noqa: E402
from oracle import Oracle  # noqa: E402
import goto_course  # noqa: E402

name, vb, out = sys.argv[1].upper(), int(sys.argv[2]), sys.argv[3]
spec = [(int(a.split(":")[0], 16), int(a.split(":")[1], 16)) for a in sys.argv[4:]]
with OracleLock():
    o, b = Oracle(), Bridge()
    slot = goto_course.install(name, "grid")
    warm_up(o, b, slot)
    b.arm_route([(0, 0, 0x80, 0x80, 0x80, 0x80)], end_vb=vb + 400)
    gen = b.restore_gen
    o.load_state(slot)
    b.wait_restore(gen)
    while b.vb_since_restore < vb:
        time.sleep(0.001)
    # NONZERO=1: also wait until the second region's first byte is nonzero
    need = os.environ.get("NONZERO") == "1"
    for attempt in range(2000):
        g0 = o.read(*spec[0])
        rest = [o.read(a, n).hex() for a, n in spec[1:]]
        if o.read(*spec[0]) == g0 and (not need or rest[0][:2] != "00"):
            break
    res = {"vb": b.vb_since_restore, "attempts": attempt + 1, "ram": {"%08X:%X" % spec[0]: g0.hex(), **{"%08X:%X" % s: h for s, h in zip(spec[1:], rest)}}}
json.dump(res, open(out, "w"))
print(res["vb"], res["attempts"])

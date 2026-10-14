"""Session 6 R: one GP0 frame of a course at its grid savestate.

    python tools/oracle/course_gp0.py NAME VB [OUT.json]

Installs analysis/states/courses/NAME_grid.pst into the scratch slot (11),
idles to VB under the oracle lock, screenshots (analysis/s6/courses/gp0/
NAME_VB.png) and saves the newest complete frame's decoded GP0 primitives
(psxrecomp/tools/psx_gpu_frame.py) as JSON. Never writes slots 1 / 2."""
import json
import os
import sys
import time

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
sys.path.insert(0, os.path.join(ROOT, "psxrecomp", "tools"))
sys.path.insert(0, os.path.dirname(__file__))
import psx_gpu_frame as g  # noqa: E402
from bridge import Bridge  # noqa: E402
from capture import OracleLock, warm_up  # noqa: E402
from oracle import Oracle  # noqa: E402
import goto_course  # noqa: E402

# --ram ADDR:LEN (hex, repeatable): RAM read right after the GP0 capture
ram_req = [a for i, a in enumerate(sys.argv) if i > 0 and sys.argv[i - 1] == "--ram"]
sys.argv = [a for i, a in enumerate(sys.argv) if a != "--ram" and (i == 0 or sys.argv[i - 1] != "--ram")]
name, vb = sys.argv[1].upper(), int(sys.argv[2])
out_dir = os.path.join(ROOT, "analysis", "s6", "courses", "gp0")
os.makedirs(out_dir, exist_ok=True)
out = sys.argv[3] if len(sys.argv) > 3 else os.path.join(out_dir, "%s_%d.json" % (name, vb))
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
    with g.DebugConn(port=4370) as c:
        d = g.capture(c)
        for back in range(1, 4):
            if len(d["prims"]) >= 50:
                break
            d = g.capture(c, frame=d["frame"] - 1)
    rams = {r: o.read(int(r.split(":")[0], 16), int(r.split(":")[1], 16)).hex() for r in ram_req}
    at = b.vb_since_restore
    shot = os.path.join(out_dir, "%s_%d.png" % (name, vb)).replace("\\", "/")
    o.q("screenshot", path=shot)
json.dump({"name": name, "vb": at, "frame": d["frame"], "prims": d["prims"], "ram": rams}, open(out, "w"))
print("vb", at, "prims", len(d["prims"]), "->", out)

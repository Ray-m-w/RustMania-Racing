"""Session 6 R: a course's grid savestate on the oracle, then mr-raw's
decode-vs-VRAM comparison (read-only commands) and a screenshot.

    python tools/oracle/course_vram.py NAME [VB]

Installs analysis/states/courses/NAME_grid.pst into the scratch slot (11),
loads it, idles VB VBlanks (default 30), screenshots to
analysis/s6/courses/vram/NAME_o.png and runs
`port/target-b/release/mr-raw.exe <RAW> --course ID` while holding the
oracle lock. Never writes slots 1 / 2."""
import os
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(__file__))
from bridge import Bridge  # noqa: E402
from capture import OracleLock, warm_up  # noqa: E402
from oracle import Oracle  # noqa: E402
import goto_course  # noqa: E402

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
name = sys.argv[1].upper()
vb = int(sys.argv[2]) if len(sys.argv) > 2 else 30
raw, cid = sys.argv[3], sys.argv[4]
out = os.path.join(ROOT, "analysis", "s6", "courses", "vram")
os.makedirs(out, exist_ok=True)
with OracleLock():
    o, b = Oracle(), Bridge()
    slot = goto_course.install(name, "grid")
    warm_up(o, b, slot)
    b.arm_route([(0, 0, 0x80, 0x80, 0x80, 0x80)], end_vb=vb + 400)
    g = b.restore_gen
    o.load_state(slot)
    b.wait_restore(g)
    while b.vb_since_restore < vb:
        time.sleep(0.005)
    shot = os.path.join(out, name + "_o.png").replace("\\", "/")
    print("shot", b.vb_since_restore, o.q("screenshot", path=shot))
    exe = os.path.join(ROOT, "port", "target-b", "release", "mr-raw.exe")
    r = subprocess.run([exe, raw, "--course", cid], cwd=ROOT, capture_output=True, text=True)
    open(os.path.join(out, name + "_mr-raw.txt"), "w").write(r.stdout + r.stderr)
    print(r.stdout[-3000:])

"""Phase 14: a course's VRAM scroll phase (FUN_8007A9DC) against the frame
counter 0x800B0D90, read from the oracle while the grid state runs.

    python tools/oracle/scroll_phase.py [COURSE]   (default DISCO: block (0xF0, 0) from (0xE0, 0))

Holds capture.OracleLock; installs analysis/states/courses/COURSE_grid.pst in
the scratch slot 11; read-only otherwise."""
import os, sys, time
ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
sys.path.insert(0, os.path.join(ROOT, "tools", "oracle"))
from bridge import Bridge
from capture import OracleLock, warm_up
from oracle import Oracle
import goto_course


def rect(o, x, y, w, h):
    r = o.q("vram_peek", x=x, y=y, w=w, h=h)
    b = bytes.fromhex(r["hex"])
    return [b[2 * w * k:2 * w * (k + 1)] for k in range(h)]


name = sys.argv[1] if len(sys.argv) > 1 else "DISCO"
dx, dy, sx, sy = 0xF0, 0, 0xE0, 0
with OracleLock():
    o, b = Oracle(), Bridge()
    slot = goto_course.install(name, "grid")
    warm_up(o, b, slot)
    b.arm_route([(0, 0, 0x80, 0x80, 0x80, 0x80)], end_vb=900)
    g = b.restore_gen
    o.load_state(slot)
    b.wait_restore(g)
    src = None
    for vb in range(60, 600, 37):
        while b.vb_since_restore < vb:
            time.sleep(0.002)
        c0 = int.from_bytes(o.read(0x800B0D90, 4), "little")
        dst = rect(o, dx, dy, 16, 64)
        c1 = int.from_bytes(o.read(0x800B0D90, 4), "little")
        dt = int.from_bytes(o.read(0x800B0AEC, 4), "little")
        if src is None:
            src = rect(o, sx, sy, 16, 64)
        rs = [r for r in range(64) if all(dst[k] == src[(k + r) % 64] for k in range(64))]
        print(vb, "counter", c0, c1, "dt", dt, "rows", rs, "counter&0x3f", c0 & 0x3F, flush=True)

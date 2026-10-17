"""Session 6 visual check: oracle screenshots of a course capture's run at
chosen VBlanks (the same run as capture_course.py, so the port can replay
analysis/oracle/courses/NAME at the same moments).

    python tools/oracle/visual_shots.py NAME OUTDIR VB[,VB...] [--gp0 VB] [--hold SPEC VB]

Under capture.OracleLock: installs analysis/states/courses/NAME_menu.pst in
the scratch slot 11 (DOCK: slot 1, read only, = analysis/oracle/start_probe's
route), replays capture_course.ROUTE_STEPS (CROSS 30 / 120 / 210, then no
input) and takes the native VRAM screenshot OUTDIR/o_<VB>.png as soon as the
bridge's VBlank counter reaches each VB. Prints the VBlank seen before and
after each request (the image lands a few VBlanks late: TCP latency + the
game's display lag; the side-by-side tool aligns by search). --gp0 VB also
saves that frame's decoded GP0 primitives (psxrecomp/tools/psx_gpu_frame.py)
as OUTDIR/gp0_<VB>.json. Never writes slots 1 / 2.
"""
import json
import os
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(ROOT, "psxrecomp", "tools"))
import goto_course as gc  # noqa: E402
from bridge import Bridge  # noqa: E402
from capture import OracleLock, active_high, warm_up  # noqa: E402
from capture_course import ROUTE_STEPS  # noqa: E402
from oracle import Oracle  # noqa: E402


def fwd(p):
    return os.path.abspath(p).replace("\\", "/")


def run(name, outdir, vbs, gp0=(), steps=None):
    os.makedirs(outdir, exist_ok=True)
    vbs = sorted(set(vbs) | set(gp0))
    log = []
    with OracleLock():
        o, b = Oracle(), Bridge()
        slot = 1 if name == "DOCK" else gc.install(name, "menu", gc.SCRATCH)
        warm_up(o, b, slot)
        b.arm_route([(s[0], active_high(s[1]), 0x80, 0x80, 0x80, 0x80) for s in (steps or ROUTE_STEPS)],
                    end_vb=max(vbs) + 60)
        g = b.restore_gen
        o.load_state(slot)
        b.wait_restore(g)
        for vb in vbs:
            while b.vb_since_restore < vb:
                time.sleep(0.0005)
            v0 = b.vb_since_restore
            if vb in gp0:
                import psx_gpu_frame as gpf
                with gpf.DebugConn(port=4370) as c:
                    d = gpf.capture(c)
                    for _ in range(3):
                        if len(d["prims"]) >= 50:
                            break
                        d = gpf.capture(c, frame=d["frame"] - 1)
                v1 = b.vb_since_restore
                json.dump({"name": name, "vb_req": vb, "vb": [v0, v1], "frame": d["frame"], "prims": d["prims"]},
                          open(os.path.join(outdir, "gp0_%d.json" % vb), "w"))
            else:
                o.q("screenshot", path=fwd(os.path.join(outdir, "o_%d.png" % vb)))
                v1 = b.vb_since_restore
            log.append({"vb": vb, "seen": [v0, v1], "gp0": vb in gp0})
            print(vb, v0, v1, flush=True)
        while b.vb_since_restore < max(vbs) + 60:
            time.sleep(0.02)
    json.dump(log, open(os.path.join(outdir, "oracle_shots.json"), "w"))
    return log


if __name__ == "__main__":
    a = sys.argv[1:]
    gp = [int(v) for i, x in enumerate(a) if x == "--gp0" for v in a[i + 1].split(",")]
    a = [x for i, x in enumerate(a) if x != "--gp0" and (i == 0 or a[i - 1] != "--gp0")]
    # phase 14: --hold SPEC VB as capture_course.py (the stunt courses' drive)
    steps = None
    if "--hold" in a:
        i = a.index("--hold")
        steps = ROUTE_STEPS + [[int(a[i + 2]), a[i + 1]]]
        a = a[:i] + a[i + 3:]
    run(a[0].upper(), a[1], [int(v) for v in a[2].split(",")] if len(a) > 2 else [], gp, steps)

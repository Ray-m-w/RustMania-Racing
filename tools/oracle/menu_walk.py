"""Session 7 (front end): walk the game's own menus and shoot each screen.

    python tools/oracle/menu_walk.py STATE "VB:buttons,..." "VB:name[+gp0],..." [OUTDIR]

STATE: `slot1` (the DOCK course menu) / `slot2` (DOCK race), read only, `title` (the archived title
state analysis/states/title/title_vb430.pst installed into the scratch slot
11) or COURSE_menu / COURSE_grid (analysis/states/courses, slot 11).
Buttons: `start`, `cross`, `down+cross`, empty = release (each press is held
until the next step). Shots: the oracle's native VRAM screenshot (4:3 display
buffer) at that VBlank as OUTDIR/name.png, with `+gp0` also the newest
complete GP0 frame decoded (psxrecomp/tools/psx_gpu_frame.py) as name.json,
plus the menu globals (game mode 0x800B0D46, sub-state 0x800B0E8A, cursor
0x800B0E24/25, course 0x800B0FAC, rows 0x800B07AB) printed. Holds the oracle
lock; never writes slots 1 / 2. Default OUTDIR analysis/s7/menus/oracle.
"""
import json
import os
import shutil
import sys
import time

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
sys.path.insert(0, os.path.join(ROOT, "psxrecomp", "tools"))
sys.path.insert(0, os.path.dirname(__file__))
from bridge import Bridge  # noqa: E402
from capture import OracleLock, active_high, warm_up  # noqa: E402
from oracle import Oracle  # noqa: E402
import goto_course  # noqa: E402

state = sys.argv[1]
steps = [(0, 0)]
for s in filter(None, sys.argv[2].split(",")):
    vb, _, btn = s.partition(":")
    steps.append((int(vb), active_high(btn)))
shots = []
for s in filter(None, sys.argv[3].split(",")):
    vb, _, name = s.partition(":")
    shots.append((int(vb), name.replace("+gp0", ""), name.endswith("+gp0")))
out_dir = os.path.abspath(sys.argv[4]) if len(sys.argv) > 4 else os.path.join(ROOT, "analysis", "s7", "menus", "oracle")
os.makedirs(out_dir, exist_ok=True)


def globals_(o):
    r = o.read(0x800B0D46, 2) + o.read(0x800B0E8A, 2) + o.read(0x800B0E24, 2) + \
        o.read(0x800B0FAC, 2) + o.read(0x800B07AB, 1) + o.read(0x800B0EA2, 2) +         o.read(0x800B07E6, 2)
    return "mode %#x sub %d cursor %d,%d course %d rows %d csub %d adv %d" % (
        int.from_bytes(r[0:2], "little"), int.from_bytes(r[2:4], "little", signed=True),
        r[4], r[5], int.from_bytes(r[6:8], "little"), r[8],
        int.from_bytes(r[9:11], "little", signed=True), int.from_bytes(r[11:13], "little"))


with OracleLock():
    o, b = Oracle(), Bridge()
    if state in ("slot1", "slot2"):
        slot = int(state[-1])  # read only
    elif state == "title":
        slot = goto_course.SCRATCH
        shutil.copyfile(os.path.join(ROOT, "analysis", "states", "title", "title_vb430.pst"),
                        goto_course.slot_path(slot))
    else:
        name, kind = state.rsplit("_", 1)
        slot = goto_course.install(name, kind)
    warm_up(o, b, slot)
    end = max([s[0] for s in shots] + [st[0] for st in steps]) + 60
    b.arm_route([(vb, m, 0x80, 0x80, 0x80, 0x80) for vb, m in steps], end_vb=end)
    # Session 7 (stunt trial): optional RAM pokes at the restore (5th arg,
    # capture_poked.py's format).
    if len(sys.argv) > 5:
        b.arm_pokes([tuple(p) for p in json.load(open(sys.argv[5]))])
    gen = b.restore_gen
    o.load_state(slot)
    b.wait_restore(gen)
    for vb, name, gp0 in shots:
        while b.vb_since_restore < vb:
            time.sleep(0.001)
        at = b.vb_since_restore
        png = os.path.join(out_dir, name + ".png").replace("\\", "/")
        o.q("screenshot", path=png)
        info = globals_(o)
        if gp0:
            import psx_gpu_frame as g
            with g.DebugConn(port=4370) as c:
                d = g.capture(c)
                for _ in range(3):
                    if len(d["prims"]) >= 20:
                        break
                    d = g.capture(c, frame=d["frame"] - 1)
            json.dump({"name": name, "vb": at, "frame": d["frame"], "prims": d["prims"]},
                      open(os.path.join(out_dir, name + ".json"), "w"))
        print("%5d %-14s %s" % (at, name, info), flush=True)

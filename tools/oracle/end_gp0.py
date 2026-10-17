"""Phase 14: THE END's GP0 frames with the camera record of the same frame.

    python tools/oracle/end_gp0.py OUTDIR "VB:buttons,..." VB[,VB...]

The title state (analysis/states/title/title_vb430.pst in the scratch slot
11), 0x800E02B9 poked to 3 (analysis/s8/end/pokes.json), the route (phase 14:
"60:start,70:,120:down,126:,140:down,146:,160:down,166:,180:down,186:,
210:cross,216:,260:down,266:,280:down,286:,300:down,306:,320:down,326:,
350:cross,356:" = OPTIONS -> THE END at VB ~600); at each VB the newest GP0
frame (psx_gpu_frame) and the camera record 0x800C4F40 (0x70 bytes) of that
frame and its neighbours from the frame ring (set_snapshot slot 0, cleared
after) as OUTDIR/g<VB>.json (mr-bevy envmap test gp0_end_reflections, ending
test gp0_credits_picture). Holds the oracle lock; never writes slots 1 / 2."""
import json, os, shutil, sys, time
ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
sys.path.insert(0, os.path.join(ROOT, "psxrecomp", "tools"))
sys.path.insert(0, os.path.join(ROOT, "tools", "oracle"))
from bridge import Bridge
from capture import OracleLock, active_high, warm_up
from oracle import Oracle
import goto_course
import psx_gpu_frame as g

out = sys.argv[1]
steps_s = sys.argv[2]
shot_vbs = [int(v) for v in sys.argv[3].split(",")]
steps = [(0, 0)]
for s in filter(None, steps_s.split(",")):
    vb, _, btn = s.partition(":")
    steps.append((int(vb), active_high(btn)))
os.makedirs(out, exist_ok=True)
with OracleLock():
    o, b = Oracle(), Bridge()
    slot = goto_course.SCRATCH
    shutil.copyfile(os.path.join(ROOT, "analysis", "states", "title", "title_vb430.pst"), goto_course.slot_path(slot))
    print(o.q("set_snapshot", slot=0, addr=0x800C4F40, size=0x70))
    warm_up(o, b, slot)
    end = max(shot_vbs + [st[0] for st in steps]) + 60
    b.arm_route([(vb, m, 0x80, 0x80, 0x80, 0x80) for vb, m in steps], end_vb=end)
    b.arm_pokes([tuple(p) for p in json.load(open(os.path.join(ROOT, "analysis", "s8", "end", "pokes.json")))])
    gen = b.restore_gen
    o.load_state(slot)
    b.wait_restore(gen)
    for vb in shot_vbs:
        while b.vb_since_restore < vb:
            time.sleep(0.001)
        with g.DebugConn(port=4370) as c:
            d = g.capture(c)
            for _ in range(3):
                if len(d["prims"]) >= 20:
                    break
                d = g.capture(c, frame=d["frame"] - 1)
        f = d["frame"]
        cams = {}
        for k in range(-3, 3):
            r = o.q("read_frame_ram", addr=0x800C4F40, len=0x70, frame=f + k)
            cams[k] = r
        mode = o.read(0x800B0D46, 2)
        json.dump({"vb": vb, "frame": f, "prims": d["prims"], "cams": cams, "mode": int.from_bytes(mode, "little")}, open(os.path.join(out, "g%d.json" % vb), "w"))
        print(vb, f, int.from_bytes(mode, "little"), str(cams[0])[:200], flush=True)
    o.q("set_snapshot", slot=0, addr=0, size=0)

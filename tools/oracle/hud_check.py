"""Compare mr-hud's layout with the HUD primitives the running game drew.

    python tools/oracle/hud_check.py [--kart 0xADDR]
    python tools/oracle/hud_check.py --route ROUTE.json --vb N   (replays from
        the grid under the oracle lock like shot_at.py, then checks at VBlank N)

Pulls the newest complete frame's GP0 packets (gpu_frame_dump), reads the
frame counter, then runs `mr-hud --oracle` (read_ram of the HUD state) and
matches every font/button
sprite (texpage x 0x200 = slot 0xF, 0x300 = slot 5) against the layout:
position, size, UV, CLUT, texpage, raw/semi. Prints matches and leftovers.
"""
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
sys.path.insert(0, os.path.join(ROOT, "psxrecomp", "tools"))
import psx_gpu_frame as g  # noqa: E402
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

argv = sys.argv[1:]
lock = None
if "--route" in argv:
    import json
    import time
    sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
    from bridge import Bridge
    from capture import active_high, OracleLock, warm_up
    from oracle import Oracle
    i = argv.index("--route")
    route = json.load(open(argv[i + 1]))
    target = int(argv[argv.index("--vb") + 1])
    argv = [a for k, a in enumerate(argv) if k not in (i, i + 1, argv.index("--vb"), argv.index("--vb") + 1)]
    lock = OracleLock().__enter__()
    o, b = Oracle(), Bridge()
    warm_up(o, b, route.get("slot", 2))
    steps = [(st[0], active_high(st[1]), 0x80, 0x80, 0x80, 0x80) for st in route["steps"]]
    b.arm_route(steps, end_vb=max(target + 120, int(route.get("end_vb", 0))))
    gen = b.restore_gen
    o.load_state(route.get("slot", 2))
    b.wait_restore(gen)
    while b.vb_since_restore < target:
        time.sleep(0.001)
    print("at vblank", b.vb_since_restore)
with g.DebugConn(port=4370) as c:
    d = g.capture(c)
    # the newest frame can still be filling (a handful of packets): step back
    for back in range(1, 4):
        if len(d["prims"]) >= 50:
            break
        d = g.capture(c, frame=d["frame"] - 1)
# the prompt blinks with the frame counter DAT_800B0D90: read it right
# after the capture (mr-hud's own reads take ~1 s, i.e. ~60 frames)
import struct
from oracle import Oracle as _O
d90 = struct.unpack("<i", _O().read(0x800B0D90, 4))[0]
argv = argv + ["--set", "frame=%d" % d90]
print("frame", d["frame"], "prims", len(d["prims"]))
tp = 0
game = []
for p in d["prims"]:
    w = [int(x, 16) for x in p["words"]]
    if p["op_name"].startswith("draw_mode"):
        tp = w[0] & 0x7FF if w else tp
        continue
    ox, oy = p["state"]["offset"]
    if p["kind"] == "rect" and p["textured"] and ((tp & 0xF) * 64) in (0x200, 0x300):
        game.append(dict(x=p["verts"][0][0] - ox, y=p["verts"][0][1] - oy, w=p["size"][0], h=p["size"][1],
                         u=p["uvs"][0][0], v=p["uvs"][0][1], tpage=tp, clut=p["clut"],
                         raw=int(p["raw_tex"]), semi=int(p["semi"])))
    if p["op_name"].startswith("PolyFT4") and p["tpage"] is not None and ((p["tpage"] & 0xF) * 64) == 0x300 and p["clut"] == 0x31B0 + 0:
        xs = [q[0] - ox for q in p["verts"]]
        ys = [q[1] - oy for q in p["verts"]]
        game.append(dict(x=min(xs), y=min(ys), w=max(xs) - min(xs), h=max(ys) - min(ys), u=p["uvs"][0][0],
                         v=p["uvs"][0][1], tpage=p["tpage"] & 0x7FF, clut=p["clut"], raw=0, semi=int(p["semi"])))

exe = os.path.join(ROOT, "port", "target", "debug", "mr-hud.exe")
args = [exe, "--oracle"] + argv
out = subprocess.run(args, cwd=ROOT, capture_output=True, text=True)
print(out.stderr.strip())
if lock is not None:
    lock.__exit__()
mine = []
for line in out.stdout.split("\n"):
    if line.strip():
        f = line.split()
        x, y, w, h, u, v, uw, vh = map(int, f[:8])
        mine.append(dict(x=x, y=y, w=w, h=h, u=u, v=v, tpage=int(f[8], 16), clut=int(f[9], 16),
                         raw=int(f[10]), semi=int(f[11]), quad=(w, h) != (uw, vh) or int(f[9], 16) >> 6 == 0x31b))

key = lambda s: (s["x"], s["y"], s["w"], s["h"], s["u"], s["v"], s["tpage"] & 0x1FF, s["clut"], s["raw"], s["semi"])
gk = [key(s) for s in game]
mk = [key(s) for s in mine]
ok = [k for k in mk if k in gk]
print(f"layout prims {len(mk)}, game font/button prims {len(gk)}, matched {len(ok)}")
for k in mk:
    if k not in gk:
        print("  only in layout:", k)
for k in gk:
    if k not in mk:
        print("  only in game:  ", k)

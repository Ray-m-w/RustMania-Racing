"""Phase 7 (two players): a split-screen capture from a course menu, pad 1
from the bridge route (VBlank-exact), pad 2 over TCP (`set_input port=2`,
psxrecomp docs/TCP_COMMANDS.md: digital, plugs a pad into port 2 while
driven; timed by polling the bridge's VBlank counter, so a step lands
within a VBlank or two of its mark; the captured 0x800B0CEA holds what
the game read).

    python tools/oracle/capture_two.py ROUTE.json OUTDIR

ROUTE.json: as capture.py's ("slot" or "state": NAME_menu.pst installed in
the scratch slot 11, "steps" for pad 1, "end_vb") plus "pad2": [[vb,
spec], ...]. Regions: capture_course.COURSE_REGIONS + the chicken (0x800CB658, 0x28) and
0x800B0EBD's word. Under capture.OracleLock; slots 1 / 2 are only loaded;
pad 2 is unplugged (clear_input port=2) at the end.

"chase2": VB: from that VBlank on (once the countdown is over), pad 2 is
steered live toward CHICKEN CHASE's chicken (0x800CB658; heading kart
+0xE, 4096 a turn, forward (sin, cos)), CROSS held, LEFT / RIGHT by the
sign of the angle; a plain closed loop over TCP, so the pad words are
whatever it sent (the capture holds them).

Phase 13: "pokes" as capture.py's (armed before the restore); "save":
true adds the save block 0x800E0190 (0x140); "chase2_target": "kart0"
steers pad 2 at kart 0 instead of the chicken (2 PLAYER BATTLE contact),
"chase2_fire": SPEC adds that button (e.g. "square") every other second;
"chase2_aim": true fires only when kart 0 is within 80 / 4096 of a
turn ahead and 0x60000 away; "chase2_target": "kart0_then_chicken" chases kart 0 until its CHICKEN
CHASE count (+0x4AC) drops (a projectile hit), then the chicken.
"writes": [[vb, addr, byte], ...]: TCP `write_ram` (one byte each) once the
run reaches that VBlank (within a VBlank or two; the capture holds what the
game had: check from a later VBlank, mr-course-check --dir --from).

--hud VB,VB,...: a HUD check run instead (HUD_REGIONS, the runtime's
VBlank counter per row as fx_capture.py, and the GP0 packets of the frames
drawn around each VBlank in gpu/<VB>.json) for tools/oracle/hudx_check.py.
"""
import math
import json
import os
import sys
import time

sys.path.insert(0, os.path.join(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))), "psxrecomp", "tools"))

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import capture  # noqa: E402
import goto_course as gc  # noqa: E402
from bridge import Bridge  # noqa: E402
from capture import OracleLock, active_high, warm_up  # noqa: E402
from oracle import Oracle, mask  # noqa: E402

import capture_course  # noqa: E402

REGIONS = capture_course.COURSE_REGIONS + [(0x800CB658, 0x28), (0x800B0EBC, 4)]
HUD_REGIONS = [
    (0x800C3000, 0xA8 + 2 * 0x504),  # kart index table + both players' records
    (0x800C4F40, 0x120),             # cameras
    (0x800B0700, 0x900),             # game globals
    (0x800DFF00, 0x180),             # texture slot table
    (0x800CB658, 0x28),              # the chicken
]


def main():
    route = json.load(open(sys.argv[1]))
    outdir = sys.argv[2]
    os.makedirs(outdir, exist_ok=True)
    end_vb = int(route["end_vb"])
    slot = route.get("slot", gc.SCRATCH)
    pad2 = sorted([tuple(p) for p in route.get("pad2", [])])
    hud = sorted(int(x) for x in sys.argv[sys.argv.index("--hud") + 1].split(",")) if "--hud" in sys.argv else []
    regions = list(HUD_REGIONS if hud else REGIONS)
    if route.get("save"):
        regions.append((0x800E0190, 0x140))
    vbts = {}
    if hud:
        os.makedirs(os.path.join(outdir, "gpu"), exist_ok=True)
    with OracleLock():
        o, b = Oracle(), Bridge()
        warm_up(o, b, 2)
        if "state" in route:
            name, kind = route["state"].rsplit("_", 1)
            gc.install(name, kind.split(".")[0], gc.SCRATCH)
            slot = gc.SCRATCH
        b.set_regions(regions)
        b.arm_route([(s[0], active_high(s[1]), 0x80, 0x80, 0x80, 0x80) for s in route["steps"]], end_vb=end_vb)
        if route.get("pokes"):
            b.arm_pokes([tuple(p) for p in route["pokes"]])
        # pad 2 plugged in, nothing pressed, before the restore
        o.q("set_input", port=2, buttons=0xFFFF)
        gen0, seq0 = b.restore_gen, b.seq
        o.load_state(slot)
        b.wait_restore(gen0)
        gen = b.restore_gen
        t0 = time.time()
        while b.u32(316) != gen and time.time() - t0 < 5:
            time.sleep(0.005)
        with open(os.path.join(outdir, "ram0.bin"), "wb") as f:
            f.write(b.ram_snapshot(gen))
        rows = {}
        sent = []
        i = 0
        chase = route.get("chase2")
        last_cmd = None
        writes = sorted([tuple(w) for w in route.get("writes", [])])
        wi = 0
        hit_seen = False
        k0_count = None
        try:
            while True:
                vb = b.vb_since_restore
                while i < len(pad2) and vb >= pad2[i][0]:
                    o.q("set_input", port=2, buttons=mask(pad2[i][1]))
                    sent.append([pad2[i][0], vb, pad2[i][1]])
                    i += 1
                while wi < len(writes) and vb >= writes[wi][0]:
                    o.q("write_ram", addr=writes[wi][1], val=writes[wi][2])
                    sent.append([writes[wi][0], vb, "write %08X=%02X" % (writes[wi][1], writes[wi][2])])
                    wi += 1
                if chase is not None and vb >= chase and i >= len(pad2):
                    k1 = 0x800C30A8 + 0x504
                    kx, kz = (int.from_bytes(o.read(k1 + a, 4), "little", signed=True) for a in (0, 8))
                    h = int.from_bytes(o.read(k1 + 0xE, 2), "little", signed=True)
                    tgt = 0x800C30A8 if route.get("chase2_target") == "kart0" else 0x800CB658
                    if route.get("chase2_target") == "kart0_then_chicken":
                        c0 = o.read(0x800C30A8 + 0x4AC, 1)[0]
                        if k0_count is not None and c0 < k0_count:
                            hit_seen = True
                        k0_count = c0
                        tgt = 0x800CB658 if hit_seen else 0x800C30A8
                    cx, cz = (int.from_bytes(o.read(tgt + a, 4), "little", signed=True) for a in (0, 8))
                    want = math.atan2(cx - kx, cz - kz) * 4096 / (2 * math.pi)
                    d = (want - h + 2048) % 4096 - 2048
                    cmd = "cross+right" if d > 96 else "cross+left" if d < -96 else "cross"
                    aimed = abs(d) < 80 and math.hypot(cx - kx, cz - kz) < 0x60000
                    if route.get("chase2_fire") and not hit_seen and (aimed if route.get("chase2_aim") else (vb // 60) % 2):
                        cmd += "+" + route["chase2_fire"]
                    if cmd != last_cmd:
                        o.q("set_input", port=2, buttons=mask(cmd))
                        last_cmd = cmd
                for sq, g, vbr, vbt, data in b.slots(seq0):
                    seq0 = sq + 1
                    if g == gen and vbr not in rows:
                        rows[vbr] = data
                        vbts[vbr] = vbt
                while hud and rows and max(rows) >= hud[0] + 4:
                    t = hud.pop(0)
                    base = vbts.get(t)
                    if base is None:
                        continue
                    import psx_gpu_frame as gf
                    from fx_capture import compact
                    dumps = {}
                    with gf.DebugConn(port=4370) as c:
                        for f in range(base - 1, base + 3):
                            try:
                                d = gf.capture(c, frame=f)
                            except gf.DebugError as e:
                                print("frame", f, e)
                                continue
                            if len(d["prims"]) > 50:
                                dumps[f] = compact(d["prims"])
                    json.dump({"vbr": t, "vbt": base, "frames": dumps}, open(os.path.join(outdir, "gpu", "%d.json" % t), "w"))
                    print("vb", t, "frames", {k: len(v) for k, v in dumps.items()}, flush=True)
                # "until_out": N: end N VBlanks after player 2 is out
                # (+0x4C7; a CHICKEN CHASE win), end_vb being the limit
                if route.get("until_out") and vb % 30 == 0 and o.read(0x800C30A8 + 0x504 + 0x4C7, 1)[0] and end_vb > vb + route["until_out"]:
                    end_vb = vb + route["until_out"]
                if rows and max(rows) >= end_vb + 30:
                    break
                time.sleep(0.002)
        finally:
            o.q("clear_input", port=2)
        o.q("screenshot", path=os.path.abspath(os.path.join(outdir, "end.png")).replace("\\", "/"))
    keys = sorted(rows)
    with open(os.path.join(outdir, "frames.bin"), "wb") as f:
        for k in keys:
            f.write(rows[k])
    meta = {"route": route, "regions": [[a, n] for a, n in regions], "vb": keys, "vbt": [vbts.get(k, 0) for k in keys], "restore_gen": gen,
            "first_vb": keys[0], "last_vb": keys[-1], "pad2_sent": sent,
            "missing": [v for v in range(keys[0], keys[-1] + 1) if v not in rows]}
    with open(os.path.join(outdir, "meta.json"), "w") as f:
        json.dump(meta, f)
    print(outdir, "vb", keys[0], "..", keys[-1], "missing", len(meta["missing"]), "pad2", sent, flush=True)


if __name__ == "__main__":
    main()

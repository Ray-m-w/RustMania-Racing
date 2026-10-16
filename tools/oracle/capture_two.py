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
"""
import math
import json
import os
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import capture  # noqa: E402
import goto_course as gc  # noqa: E402
from bridge import Bridge  # noqa: E402
from capture import OracleLock, active_high, warm_up  # noqa: E402
from oracle import Oracle, mask  # noqa: E402

import capture_course  # noqa: E402

REGIONS = capture_course.COURSE_REGIONS + [(0x800CB658, 0x28), (0x800B0EBC, 4)]


def main():
    route = json.load(open(sys.argv[1]))
    outdir = sys.argv[2]
    os.makedirs(outdir, exist_ok=True)
    end_vb = int(route["end_vb"])
    slot = route.get("slot", gc.SCRATCH)
    pad2 = sorted([tuple(p) for p in route.get("pad2", [])])
    with OracleLock():
        o, b = Oracle(), Bridge()
        warm_up(o, b, 2)
        if "state" in route:
            name, kind = route["state"].rsplit("_", 1)
            gc.install(name, kind.split(".")[0], gc.SCRATCH)
            slot = gc.SCRATCH
        b.set_regions(REGIONS)
        b.arm_route([(s[0], active_high(s[1]), 0x80, 0x80, 0x80, 0x80) for s in route["steps"]], end_vb=end_vb)
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
        try:
            while True:
                vb = b.vb_since_restore
                while i < len(pad2) and vb >= pad2[i][0]:
                    o.q("set_input", port=2, buttons=mask(pad2[i][1]))
                    sent.append([pad2[i][0], vb, pad2[i][1]])
                    i += 1
                if chase is not None and vb >= chase and i >= len(pad2):
                    k1 = 0x800C30A8 + 0x504
                    kx, kz = (int.from_bytes(o.read(k1 + a, 4), "little", signed=True) for a in (0, 8))
                    h = int.from_bytes(o.read(k1 + 0xE, 2), "little", signed=True)
                    cx, cz = (int.from_bytes(o.read(0x800CB658 + a, 4), "little", signed=True) for a in (0, 8))
                    want = math.atan2(cx - kx, cz - kz) * 4096 / (2 * math.pi)
                    d = (want - h + 2048) % 4096 - 2048
                    cmd = "cross+right" if d > 96 else "cross+left" if d < -96 else "cross"
                    if cmd != last_cmd:
                        o.q("set_input", port=2, buttons=mask(cmd))
                        last_cmd = cmd
                for sq, g, vbr, vbt, data in b.slots(seq0):
                    seq0 = sq + 1
                    if g == gen and vbr not in rows:
                        rows[vbr] = data
                if rows and max(rows) >= end_vb + 30:
                    break
                time.sleep(0.002)
        finally:
            o.q("clear_input", port=2)
        o.q("screenshot", path=os.path.join(outdir, "end.png").replace("\\", "/"))
    keys = sorted(rows)
    with open(os.path.join(outdir, "frames.bin"), "wb") as f:
        for k in keys:
            f.write(rows[k])
    meta = {"route": route, "regions": [[a, n] for a, n in REGIONS], "vb": keys, "restore_gen": gen,
            "first_vb": keys[0], "last_vb": keys[-1], "pad2_sent": sent,
            "missing": [v for v in range(keys[0], keys[-1] + 1) if v not in rows]}
    with open(os.path.join(outdir, "meta.json"), "w") as f:
        json.dump(meta, f)
    print(outdir, "vb", keys[0], "..", keys[-1], "missing", len(meta["missing"]), "pad2", sent, flush=True)


if __name__ == "__main__":
    main()

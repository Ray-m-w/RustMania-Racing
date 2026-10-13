"""Record a full-lap input route by driving the oracle closed-loop.

    python tools/oracle/drive_lap.py WAYPOINT_CAPTURE_DIR OUT_ROUTE.json [--ticks N]

Waypoints are an AI kart's positions from a capture (region 5 = kart 1).
The controller holds CROSS (accelerate in this save's controller setup) and
steers LEFT/RIGHT toward a look-ahead waypoint. It writes each new command into
the bridge route a few VBlanks ahead of the game, so the run it drives is
exactly the open-loop replay of the route it saves (verified by replaying).
"""
import json
import math
import os
import struct
import sys
import time

sys.path.insert(0, os.path.dirname(__file__))
import capture  # noqa: E402
from bridge import Bridge, O_RLEN, O_STEPS  # noqa: E402
from oracle import Oracle, BITS  # noqa: E402

CROSS = 1 << BITS["cross"]
LEFT = 1 << BITS["left"]
RIGHT = 1 << BITS["right"]


def main():
    wp_dir, out = sys.argv[1], sys.argv[2]
    ticks = int(sys.argv[sys.argv.index("--ticks") + 1]) if "--ticks" in sys.argv else 1500
    meta, frames = capture.load(wp_dir)
    wps = [struct.unpack_from("<3i", frames[v][5], 0) for v in sorted(frames) if v % 2 == 0]
    wps = [(p[0], p[2]) for p in wps[::4]]

    o, b = Oracle(), Bridge()
    regions = [(capture.KART0, 0x400)]
    b.set_regions(regions)
    start_vb = 60
    end_vb = start_vb + 2 * ticks
    steps = [(0, 0), (start_vb, CROSS)]
    b.arm_route([s + (0x80, 0x80, 0x80, 0x80) for s in steps], end_vb=end_vb)
    gen0, seq = b.restore_gen, b.seq
    o.load_state(2)
    b.wait_restore(gen0)
    gen = b.restore_gen
    cur = CROSS
    idx = 0
    last_vb = -1
    while True:
        latest = None
        for sq, g, vbr, vbt, data in b.slots(seq):
            seq = sq + 1
            if g == gen:
                latest = (vbr, data)
        if latest is None:
            time.sleep(0.002)
            continue
        vb, data = latest
        if vb >= end_vb:
            break
        if vb == last_vb or vb < start_vb:
            continue
        last_vb = vb
        x, _, z = struct.unpack_from("<3i", data, 0)
        h = struct.unpack_from("<h", data, 0x0E)[0]
        # nearest waypoint at or after idx (monotone progress)
        best = min(range(idx, min(idx + 60, len(wps))),
                   key=lambda i: (wps[i][0] - x) ** 2 + (wps[i][1] - z) ** 2)
        idx = best
        tx, tz = wps[min(idx + 6, len(wps) - 1)]
        want = math.atan2(tx - x, tz - z)
        have = h / 4096 * 2 * math.pi
        err = (want - have + math.pi) % (2 * math.pi) - math.pi
        cmd = CROSS
        if err > 0.08:
            cmd |= RIGHT
        elif err < -0.08:
            cmd |= LEFT
        if cmd != cur:
            n = len(steps)
            at = vb + 4
            if at <= steps[-1][0]:
                at = steps[-1][0] + 1
            steps.append((at, cmd))
            struct.pack_into("<IIBBBB", b.m, O_STEPS + 12 * n, at, cmd, 0x80, 0x80, 0x80, 0x80)
            b.put_u32(O_RLEN, n + 1)
            cur = cmd
            if n + 1 >= 8190:
                print("route full at vb", vb)
                break
    names = {0: ""}
    def spec(m):
        return "+".join(k for k, bit in BITS.items() if m & (1 << bit))
    route = {"name": os.path.splitext(os.path.basename(out))[0], "slot": 2, "end_vb": end_vb,
             "note": "recorded closed-loop by tools/oracle/drive_lap.py from %s" % wp_dir,
             "steps": [[vb, spec(m)] for vb, m in steps]}
    json.dump(route, open(out, "w"), indent=0)
    print("steps", len(steps), "end_vb", end_vb, "last waypoint idx", idx, "of", len(wps))


if __name__ == "__main__":
    main()

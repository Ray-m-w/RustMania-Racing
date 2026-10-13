"""Record a multi-lap input route by driving the oracle closed-loop around a
loop of waypoints taken from an earlier trace (the player's own lap).

    python tools/oracle/drive_race.py TRACE.jsonl OUT_ROUTE.json --loop A B [--ticks N]

Waypoints = trace positions of ticks A..B (one lap, e.g. lap1.oracle.trace
ticks 40..1134); the index wraps, so the route keeps lapping. Controller as
tools/oracle/drive_lap.py (CROSS held, LEFT/RIGHT toward a look-ahead
waypoint), commands written a few VBlanks ahead so the saved route replays
the same run open-loop. Takes the shared oracle lock.
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
    trace, out = sys.argv[1], sys.argv[2]
    a, bnd = (int(x) for x in sys.argv[sys.argv.index("--loop") + 1:sys.argv.index("--loop") + 3])
    ticks = int(sys.argv[sys.argv.index("--ticks") + 1]) if "--ticks" in sys.argv else 4300
    lines = open(trace).read().splitlines()
    f = json.loads(lines[0])["fields"]
    ix, iz = f.index("x") + 2, f.index("z") + 2
    rows = [json.loads(l) for l in lines[1:]]
    wps = [(r[ix], r[iz]) for r in rows if a <= r[0] <= bnd][::4]
    n = len(wps)
    with capture.OracleLock():
        o, b = Oracle(), Bridge()
        capture.warm_up(o, b, 2)
        b.set_regions([(capture.KART0, 0x400)])
        start_vb = 60
        end_vb = start_vb + 2 * ticks
        steps = [(0, 0), (start_vb, CROSS)]
        b.arm_route([s + (0x80, 0x80, 0x80, 0x80) for s in steps], end_vb=end_vb)
        gen0, seq = b.restore_gen, b.seq
        o.load_state(2)
        b.wait_restore(gen0)
        gen = b.restore_gen
        cur, k, last_vb = CROSS, 0, -1  # k: unwrapped waypoint index
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
            k = min(range(k, k + 60), key=lambda j: (wps[j % n][0] - x) ** 2 + (wps[j % n][1] - z) ** 2)
            tx, tz = wps[(k + 6) % n]
            want = math.atan2(tx - x, tz - z)
            have = h / 4096 * 2 * math.pi
            err = (want - have + math.pi) % (2 * math.pi) - math.pi
            cmd = CROSS
            if err > 0.08:
                cmd |= RIGHT
            elif err < -0.08:
                cmd |= LEFT
            if cmd != cur:
                m = len(steps)
                at = vb + 4
                if at <= steps[-1][0]:
                    at = steps[-1][0] + 1
                steps.append((at, cmd))
                struct.pack_into("<IIBBBB", b.m, O_STEPS + 12 * m, at, cmd, 0x80, 0x80, 0x80, 0x80)
                b.put_u32(O_RLEN, m + 1)
                cur = cmd
                if m + 1 >= 8190:
                    print("route full at vb", vb)
                    break

    def spec(m):
        return "+".join(name for name, bit in BITS.items() if m & (1 << bit))
    route = {"name": os.path.splitext(os.path.basename(out))[0], "slot": 2, "end_vb": end_vb,
             "note": "recorded closed-loop by tools/oracle/drive_race.py around %s ticks %d..%d" % (trace, a, bnd),
             "steps": [[v, spec(m)] for v, m in steps]}
    json.dump(route, open(out, "w"), indent=0)
    print("steps", len(steps), "end_vb", end_vb, "laps of waypoints", k / n)


if __name__ == "__main__":
    main()

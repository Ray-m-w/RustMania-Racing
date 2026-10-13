"""Record a multi-lap items-on input route by driving the oracle closed-loop
(Session 4, track A): steering as tools/oracle/drive_race.py (CROSS held,
LEFT/RIGHT toward a look-ahead waypoint of an earlier lap trace), plus an
item policy read from the player's kart record every VBlank:

- L1 / L2: tap (4 VBlanks) when that weapon slot is charged (+0x4D4 / +0x4D5
  == 0x40);
- R1 turbo: hold while the turbo meter (+0x48A) is between --turbo-lo and
  --turbo-hi and not full (a full meter + R1 is the special move);
- full meter (0x1A4): the specials in turn, --specials "r2,r1,..." (R2 =
  character special FUN_8006B828, R1 = special move +0x4D6).

    python tools/oracle/drive_items.py TRACE.jsonl OUT_ROUTE.json --loop A B
        [--ticks N] [--specials r2,r1] [--turbo-lo 200] [--turbo-hi 300]
        [--no-weapons] [--seed-skip VB]

Commands are written a few VBlanks ahead so the saved route replays the same
run open-loop (capture with tools/oracle/capture_items.py). Takes the shared
oracle lock.
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

B = {k: 1 << v for k, v in BITS.items()}
FULL = 0x1A4


def arg(name, default, conv=int):
    if name in sys.argv:
        return conv(sys.argv[sys.argv.index(name) + 1])
    return default


def main():
    trace, out = sys.argv[1], sys.argv[2]
    a, bnd = (int(x) for x in sys.argv[sys.argv.index("--loop") + 1:sys.argv.index("--loop") + 3])
    ticks = arg("--ticks", 4300)
    specials = arg("--specials", "r2,r1", str).split(",")
    tlo, thi = arg("--turbo-lo", 200), arg("--turbo-hi", 300)
    weapons = "--no-weapons" not in sys.argv
    lines = open(trace).read().splitlines()
    f = json.loads(lines[0])["fields"]
    ix, iz = f.index("x") + 2, f.index("z") + 2
    rows = [json.loads(l) for l in lines[1:]]
    wps = [(r[ix], r[iz]) for r in rows if a <= r[0] <= bnd][::4]
    n = len(wps)
    log = []
    with capture.OracleLock():
        o, b = Oracle(), Bridge()
        capture.warm_up(o, b, 2)
        b.set_regions([(capture.KART0, capture.KART_SIZE)])
        start_vb = 60
        end_vb = start_vb + 2 * ticks
        steps = [(0, 0), (start_vb, B["cross"])]
        b.arm_route([s + (0x80, 0x80, 0x80, 0x80) for s in steps], end_vb=end_vb)
        gen0, seq = b.restore_gen, b.seq
        o.load_state(2)
        b.wait_restore(gen0)
        gen = b.restore_gen
        cur, k, last_vb = B["cross"], 0, -1
        tap = {"l1": -100, "l2": -100}   # VBlank of the last tap start
        special_i, special_until, turbo = 0, -1, False
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
            meter = struct.unpack_from("<h", data, 0x48A)[0]
            charge = data[0x4D4], data[0x4D5]
            special = data[0x4D6]
            k = min(range(k, k + 60), key=lambda j: (wps[j % n][0] - x) ** 2 + (wps[j % n][1] - z) ** 2)
            tx, tz = wps[(k + 6) % n]
            want = math.atan2(tx - x, tz - z)
            have = h / 4096 * 2 * math.pi
            err = (want - have + math.pi) % (2 * math.pi) - math.pi
            cmd = B["cross"]
            if err > 0.08:
                cmd |= B["right"]
            elif err < -0.08:
                cmd |= B["left"]
            # Weapons: tap when charged (hold 4 VBlanks, then at least 8 off).
            if weapons:
                for i, nm in enumerate(("l1", "l2")):
                    if vb - tap[nm] < 4:
                        cmd |= B[nm]
                    elif charge[i] == 0x40 and vb - tap[nm] > 12:
                        tap[nm] = vb
                        cmd |= B[nm]
                        log.append((vb, nm))
            # Specials on a full meter, in turn; turbo bursts otherwise.
            if vb < special_until:
                cmd |= B[specials[(special_i - 1) % len(specials)]]
            elif meter == FULL and not special and specials and specials[0]:
                nm = specials[special_i % len(specials)]
                special_i += 1
                special_until = vb + 6
                cmd |= B[nm]
                log.append((vb, nm + "-special"))
            elif not special:
                if turbo and (meter < tlo - 100 or meter >= FULL - 4):
                    turbo = False
                elif not turbo and tlo <= meter < min(thi, FULL - 4):
                    turbo = True
                    log.append((vb, "r1-turbo"))
                if turbo:
                    cmd |= B["r1"]
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
             "note": "recorded closed-loop by tools/oracle/drive_items.py around %s ticks %d..%d; "
                     "items on; specials %s, turbo %d..%d" % (trace, a, bnd, ",".join(specials), tlo, thi),
             "steps": [[v, spec(m)] for v, m in steps]}
    json.dump(route, open(out, "w"), indent=0)
    print("steps", len(steps), "end_vb", end_vb, "laps of waypoints", k / n)
    for e in log:
        print("  vb %5d %s" % e)


if __name__ == "__main__":
    main()

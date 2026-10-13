"""Capture a route with items switched off by TCP pokes (interim, bridge v3).

    python tools/oracle/capture_noitems.py ROUTE.json OUTDIR

Same as `capture.py ROUTE OUT --physics`, but while the run is captured the
script watches every VBlank slot of the oracle bridge and, as soon as a kart
holds a weapon (+0x4D4/+0x4D5 charge timer != 0xFF) or turbo meter
(+0x48A != 0), writes them back to empty / 0 with TCP `write_ram` (one byte
per call). Session 3 AI tolerance runs: the port's AI has no weapons, turbo
meter or specials, so the oracle runs it is compared with must not either.

NOT deterministic: the write lands a frame or two after the pickup (TCP
timing), so a kart can boost for a tick on a token it collected. The
analysis counts those ticks (+0x4C4 boosting) and reports them. Bridge v4
(`capture.py --no-items`) does the same inside the VBlank callback,
deterministically, once the oracle runs a v4 build.
"""
import json
import os
import sys
import time

sys.path.insert(0, os.path.dirname(__file__))
import capture  # noqa: E402
from capture import KART0, KART_SIZE, PHYSICS_REGIONS, OracleLock, warm_up  # noqa: E402
from oracle import Oracle  # noqa: E402
from bridge import Bridge  # noqa: E402


def _kart_offsets(regions):
    """Byte offset of kart slot 0 inside a slot payload."""
    o = 0
    for a, n in regions:
        if a <= KART0 < a + n:
            return o + (KART0 - a)
        o += n
    raise KeyError("kart array not captured")


def run(route, outdir, slot=None, regions=PHYSICS_REGIONS):
    with OracleLock():
        o = Oracle()
        po = Oracle()  # second connection for the pokes
        b = Bridge()
        warm_up(o, b, slot if slot is not None else route.get("slot", 2))
        b.set_regions(regions)
        steps = []
        for st in route["steps"]:
            vb, spec = st[0], st[1]
            lx = st[2] if len(st) > 2 else 0x80
            ly = st[3] if len(st) > 3 else 0x80
            steps.append((vb, capture.active_high(spec), lx, ly, 0x80, 0x80))
        end_vb = int(route.get("end_vb", 1200))
        b.arm_route(steps, end_vb=end_vb)
        gen0 = b.restore_gen
        seq0 = b.seq
        o.load_state(slot if slot is not None else route.get("slot", 2))
        b.wait_restore(gen0)
        gen = b.restore_gen
        os.makedirs(outdir, exist_ok=True)
        t0 = time.time()
        while b.u32(316) != gen and time.time() - t0 < 5:
            time.sleep(0.005)
        try:
            with open(os.path.join(outdir, "ram0.bin"), "wb") as f:
                f.write(b.ram_snapshot(gen))
        except RuntimeError:
            pass
        k0 = _kart_offsets(regions)
        rows = {}
        pokes = []
        last_poke = {}
        while True:
            latest = None
            for sq, g, vbr, vbt, data in b.slots(seq0):
                seq0 = sq + 1
                if g == gen and vbr not in rows:
                    rows[vbr] = data
                    latest = (vbr, data)
            if latest:
                vbr, data = latest
                for s in range(6):
                    r = k0 + s * KART_SIZE
                    want = []
                    for off in (0x4D4, 0x4D5):
                        if data[r + off] != 0xFF:
                            want.append((off, 0xFF))
                    for off in (0x48A, 0x48B):
                        if data[r + off] != 0:
                            want.append((off, 0))
                    for off, val in want:
                        a = KART0 + s * KART_SIZE + off
                        # do not re-send the same write within 3 VBlanks
                        if last_poke.get(a, -10) + 3 > vbr:
                            continue
                        po.q("write_ram", addr=f"0x{a:08X}", val=val)
                        last_poke[a] = vbr
                        pokes.append([vbr, s, off, data[r + off]])
            if rows and max(rows) >= end_vb + 30:
                break
            time.sleep(0.002)
    keys = sorted(rows)
    with open(os.path.join(outdir, "frames.bin"), "wb") as f:
        for k in keys:
            f.write(rows[k])
    meta = {"route": route, "regions": [[a, n] for a, n in regions], "vb": keys,
            "restore_gen": gen, "first_vb": keys[0], "last_vb": keys[-1],
            "missing": [v for v in range(keys[0], keys[-1] + 1) if v not in rows],
            "items_off": "tcp-pokes", "pokes": pokes}
    with open(os.path.join(outdir, "meta.json"), "w") as f:
        json.dump(meta, f)
    return meta


if __name__ == "__main__":
    route = json.load(open(sys.argv[1]))
    m = run(route, sys.argv[2])
    print("captured vb", m["first_vb"], "..", m["last_vb"], "missing", len(m["missing"]), "pokes", len(m["pokes"]))

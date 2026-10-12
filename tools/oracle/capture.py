"""Run one input route from the DOCK grid savestate and capture per-VBlank RAM.

    python tools/oracle/capture.py ROUTE.json OUT.npz-ish-dir [--slot 2] [--vb N]

ROUTE.json: {"name": "...", "slot": 2, "end_vb": 1200,
             "steps": [[vb_start, "triangle+left", lx, ly], ...]}
Writes OUT/frames.bin (raw per-VBlank region bytes), OUT/meta.json and
OUT/ram0.bin (main RAM + scratchpad copied inside the restore callback).
--physics captures PHYSICS_REGIONS instead of the default kart set.
Anchor: VBlank 0 = the savestate restore (oracle bridge plugin).
"""
import json
import os
import sys
import time

sys.path.insert(0, os.path.dirname(__file__))
from oracle import Oracle, BITS  # noqa: E402
from bridge import Bridge  # noqa: E402

KART0 = 0x800C30A8
KART_SIZE = 0x504
# Regions captured every VBlank. Keep total under 8168 bytes.
REGIONS = [
    (KART0, KART_SIZE),            # player kart record
    (0x800C4F40, 0x70),            # player camera (FUN_80070418 arg 0)
    (0x800B0AE8, 0x08),            # tick dt (0x800B0AEC) and neighbour
    (0x800B0CE8, 0x08),            # per-player button words fed to the kart update
    (0x800B0D44, 0x04),            # game mode short (0x800B0D46)
    (KART0 + KART_SIZE, KART_SIZE),  # kart 1 (first AI), for contact checks
]


# Everything the kart update (FUN_80064B5C range + callees, 115 functions)
# writes, plus the mutable state it reads (analysis/ghidra/xrefs_data.tsv),
# for per-tick single-step verification. Constant tables come from ram0.bin.
PHYSICS_REGIONS = [
    (0x800C3000, 0xA8 + 6 * KART_SIZE),  # kart index table + 6 kart records
    (0x800C4F40, 0x120),           # cameras
    (0x800C5460, 0x20),
    (0x800B0700, 0x900),           # game globals (dt, mode, pads, rng ptr, ...)
    (0x800B4940, 0x60),
    (0x800BC100, 0x560),
    (0x800D1570, 0x740),
    (0x800D4090, 0x4), (0x800D4218, 0x4), (0x800D4B80, 0x64),
    (0x800D73D0, 0x8), (0x800D7FF0, 0x20), (0x800D8AB0, 0x4F0),
    (0x800DFD88, 0xC), (0x800E02BC, 0x4), (0x800E0398, 0x4), (0x800E21A4, 0x4),
    (0x800E0C20, 0x40), (0x800E3D78, 0x60),
    (0x1F800000, 0x400),           # scratchpad
]


def active_high(spec):
    m = 0
    if spec:
        for b in spec.split("+"):
            m |= 1 << BITS[b]
    return m


def run(route, outdir, slot=None, regions=REGIONS, settle=0.3):
    o = Oracle()
    b = Bridge()
    b.set_regions(regions)
    steps = []
    for st in route["steps"]:
        vb, spec = st[0], st[1]
        lx = st[2] if len(st) > 2 else 0x80
        ly = st[3] if len(st) > 3 else 0x80
        steps.append((vb, active_high(spec), lx, ly, 0x80, 0x80))
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
        ram0 = b.ram_snapshot(gen)
        with open(os.path.join(outdir, "ram0.bin"), "wb") as f:
            f.write(ram0)
    except RuntimeError:
        pass
    rows = {}
    while True:
        for sq, g, vbr, vbt, data in b.slots(seq0):
            seq0 = sq + 1
            if g == gen and vbr not in rows:
                rows[vbr] = data
        if rows and max(rows) >= end_vb + 30:
            break
        time.sleep(0.05)
    os.makedirs(outdir, exist_ok=True)
    keys = sorted(rows)
    with open(os.path.join(outdir, "frames.bin"), "wb") as f:
        for k in keys:
            f.write(rows[k])
    meta = {"route": route, "regions": [[a, n] for a, n in regions], "vb": keys,
            "restore_gen": gen, "first_vb": keys[0], "last_vb": keys[-1],
            "missing": [v for v in range(keys[0], keys[-1] + 1) if v not in rows]}
    with open(os.path.join(outdir, "meta.json"), "w") as f:
        json.dump(meta, f)
    return meta


def load(outdir):
    meta = json.load(open(os.path.join(outdir, "meta.json")))
    size = sum(n for _, n in meta["regions"])
    raw = open(os.path.join(outdir, "frames.bin"), "rb").read()
    frames = {}
    for i, vb in enumerate(meta["vb"]):
        d = raw[i * size:(i + 1) * size]
        parts, o = [], 0
        for _, n in meta["regions"]:
            parts.append(d[o:o + n])
            o += n
        frames[vb] = parts
    return meta, frames


if __name__ == "__main__":
    route = json.load(open(sys.argv[1]))
    regs = PHYSICS_REGIONS if "--physics" in sys.argv else REGIONS
    m = run(route, sys.argv[2], regions=regs)
    print("captured vb", m["first_vb"], "..", m["last_vb"], "missing", len(m["missing"]))

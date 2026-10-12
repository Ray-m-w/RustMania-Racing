"""Oracle trace: run a route on the recomp and write an mr-trace JSONL.

    python tools/oracle/trace.py ROUTE.json OUT.trace.jsonl [--keep DIR | --from CAPDIR]

Tick k = VBlank 2k after the savestate restore (the kart record changes only
on even VBlanks; dt at 0x800B0AEC is 2). Fields are raw game integers from the
player kart record at 0x800C30A8 (layout: docs/spec/kart-state.md).
"""
import json
import os
import struct
import sys
import tempfile

sys.path.insert(0, os.path.dirname(__file__))
import capture  # noqa: E402

# name -> (offset in kart record, struct format)
FIELDS = [
    ("x", 0x000, "<i"), ("y", 0x004, "<i"), ("z", 0x008, "<i"),
    ("pitch", 0x00C, "<h"), ("heading", 0x00E, "<h"), ("roll", 0x010, "<h"),
    ("vx", 0x2F4, "<i"), ("vy", 0x2F8, "<i"), ("vz", 0x2FC, "<i"),
    ("speed", 0x390, "<i"), ("yaw_rate", 0x394, "<i"),
] + [("m%d" % i, 0x18C + 2 * i, "<h") for i in range(9)]  # body matrix, 4.12, row-major


def reader(meta, parts):
    """addr -> bytes accessor over one VBlank's captured regions."""
    regs = meta["regions"]

    def rd(addr, n):
        for (a, ln), data in zip(regs, parts):
            if a <= addr and addr + n <= a + ln:
                return data[addr - a: addr - a + n]
        raise KeyError(hex(addr))
    return rd


def rows_from_capture(meta, frames):
    out = []
    kart0 = capture.KART0
    for vb in sorted(frames):
        if vb % 2:
            continue
        rd = reader(meta, frames[vb])
        btn = struct.unpack("<H", rd(0x800B0CE8, 2))[0]
        vals = [struct.unpack(fmt, rd(kart0 + off, struct.calcsize(fmt)))[0] for _, off, fmt in FIELDS]
        out.append((vb // 2, btn, vals))
    return out


def write_trace(path, route_name, rows, source="oracle", note=""):
    with open(path, "w") as f:
        f.write(json.dumps({"mrm_trace": 1, "source": source, "route": route_name,
                            "note": note, "fields": [n for n, _, _ in FIELDS]}) + "\n")
        for t, btn, vals in rows:
            f.write(json.dumps([t, btn] + vals, separators=(",", ":")) + "\n")


def main():
    route = json.load(open(sys.argv[1]))
    out = sys.argv[2]
    keep = sys.argv[sys.argv.index("--keep") + 1] if "--keep" in sys.argv else None
    src = sys.argv[sys.argv.index("--from") + 1] if "--from" in sys.argv else None
    d = src or keep or tempfile.mkdtemp(prefix="mrm-cap-")
    if not src:
        capture.run(route, d, regions=capture.PHYSICS_REGIONS)
    meta, frames = capture.load(d)
    rows = rows_from_capture(meta, frames)
    write_trace(out, route["name"], rows,
                note="psxrecomp build-dev + oracle bridge; DOCK grid savestate slot %d" % route.get("slot", 2))
    print("wrote", out, len(rows), "ticks")


if __name__ == "__main__":
    main()

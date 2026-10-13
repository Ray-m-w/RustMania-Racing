"""SPU key-on events of a capture_sfx.py run on the VBlanks-since-restore clock.

    python tools/oracle/sfx_events.py CAPTURE_DIR

Reads OUT/spu_events.json + OUT/clock.json and writes OUT/keyons.csv:
`vb,voice,addr,pitch,vol_l,vol_r,adsr1,adsr2,kind` (kind KEYON / KEYOFF),
vb = runtime frame - offset, the offset taken from the (frame, VBlanks since
restore) pairs sampled while the run was polled (pairs whose bridge count
did not move during the TCP query only). Prints the offset spread and a
summary of key-ons by sample address.
"""
import collections
import json
import os
import statistics
import sys


def main(d):
    ev = json.load(open(os.path.join(d, "spu_events.json")))
    clock = json.load(open(os.path.join(d, "clock.json")))
    offs = [c["frame"] - c["vb1"] for c in clock if c["vb0"] == c["vb1"] and c["vb1"] > 0]
    off = int(statistics.median(offs))
    print("offset frame - vb: median", off, "min", min(offs), "max", max(offs), "n", len(offs))
    rows = []
    for e in ev:
        if e["kind"] not in ("KEYON", "KEYOFF"):
            continue
        vb = e["frame"] - off
        rows.append((vb, e["v"], int(e["addr"], 16), int(e["pitch"], 16), int(e["vol_l"], 16), int(e["vol_r"], 16),
                     int(e["adsr_lo"], 16), int(e["adsr_hi"], 16), e["kind"]))
    with open(os.path.join(d, "keyons.csv"), "w") as f:
        f.write("vb,voice,addr,pitch,vol_l,vol_r,adsr1,adsr2,kind\n")
        for r in rows:
            f.write(",".join(str(x) for x in r) + "\n")
    kon = [r for r in rows if r[8] == "KEYON" and r[0] >= 0]
    print("key-ons after the restore:", len(kon), "first vb", kon[0][0] if kon else None, "last", kon[-1][0] if kon else None)
    c = collections.Counter((hex(r[2]), r[4] == 0 and r[5] == 0) for r in kon)
    for (a, silent), n in c.most_common(40):
        print(f"  addr {a:>8} {'vol 0' if silent else '     '} x{n}")


if __name__ == "__main__":
    main(sys.argv[1])

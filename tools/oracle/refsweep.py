"""Run refstep over every tick of a capture; per kart-record halfword, count
ticks where the reference step (FUN_80064B5C + FUN_80086B7C under mipsref)
disagrees with the oracle's next tick.

    python tools/oracle/refsweep.py analysis/oracle/lap1P [vb0 vb1]
"""
import os
import struct
import sys
import time

sys.path.insert(0, os.path.dirname(__file__))
from refstep import Cap, kart_update, KART0, KSZ  # noqa: E402

c = Cap(sys.argv[1])
vbs = sorted(c.frames)
lo = int(sys.argv[2]) if len(sys.argv) > 2 else 2
hi = int(sys.argv[3]) if len(sys.argv) > 3 else vbs[-1] - 2
cnt = {}
first = {}
t0 = time.time()
n = 0
for vb in range(lo + (lo & 1), hi + 1, 2):
    if vb + 2 not in c.frames:
        break
    m = c.machine(vb)
    kart_update(m)
    o = KART0 & 0x1FFFFF
    after = bytes(m.ram[o:o + KSZ])
    ref = c.read(vb + 2, KART0, KSZ)
    n += 1
    for off in range(0, KSZ, 2):
        if after[off:off + 2] != ref[off:off + 2]:
            cnt[off] = cnt.get(off, 0) + 1
            first.setdefault(off, vb)
print("ticks", n, "time %.1fs" % (time.time() - t0))
for off in sorted(cnt):
    print("+%03X  bad %5d  first vb %d" % (off, cnt[off], first[off]))

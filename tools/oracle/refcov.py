"""Coverage of the kart update over a capture: which instructions the solo
player kart executes (reading aid for deciding what mr-sim must port).
    python tools/oracle/refcov.py analysis/oracle/lap1P OUT.txt
"""
import os, struct, sys
sys.path.insert(0, os.path.dirname(__file__))
from refstep import Cap, kart_update, KART0
c = Cap(sys.argv[1])
cov = {}
calls = {}
for vb in range(2, max(c.frames) - 2, 2):
    m = c.machine(vb)
    m.w16(0x800B0CE8, struct.unpack_from('<H', c.read(vb + 2, 0x800B0CE8, 2))[0])
    m.cov = cov
    kart_update(m)
    for k, v in m.calls.items():
        calls[k] = calls.get(k, 0) + v
with open(sys.argv[2], "w") as f:
    for pc in sorted(cov):
        f.write("%08X %d\n" % (pc, cov[pc]))
for k in sorted(calls):
    print("%08X %d" % (k, calls[k]))

"""Single-step the game's kart update (FUN_80064B5C + FUN_80086B7C) with the
MIPS reference interpreter on an oracle capture and diff against the next tick.

    python tools/oracle/refstep.py analysis/oracle/lap1P VB [--full]

Reading aid for the mr-sim port: shows which kart-record bytes the two
functions explain and which change because of other code in the race frame.
"""
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
import capture  # noqa: E402
from mipsref import Machine, build_memory, overlay  # noqa: E402

KART0 = 0x800C30A8
KSZ = 0x504
SOUND_STUBS = [0x8007A620, 0x8005EECC]


class Cap:
    def __init__(self, d):
        self.meta, self.frames = capture.load(d)
        self.regions = [(a, n) for a, n in self.meta["regions"]]
        self.ram0 = os.path.join(d, "ram0.bin")

    def machine(self, vb):
        ram, sp = build_memory(self.ram0, self.regions, self.frames[vb])
        m = Machine(ram, sp)
        for a in SOUND_STUBS:
            m.stubs[a] = lambda mm: 0
        return m

    def read(self, vb, addr, n):
        for (a, ln), d in zip(self.regions, self.frames[vb]):
            if a <= addr and addr + n <= a + ln:
                return d[addr - a:addr - a + n]
        raise KeyError(hex(addr))


def kart_update(m, kart=KART0, player=0):
    btn = m.r16(0x800B0CE8 + 2 * player)
    m.call(0x80064B5C, kart, s16(btn), player)
    m.call(0x80086B7C, kart)


def s16(v):
    return v - 0x10000 if v & 0x8000 else v


def diff_kart(after, ref):
    out = []
    for o in range(0, KSZ, 2):
        a = struct.unpack_from("<H", after, o)[0]
        b = struct.unpack_from("<H", ref, o)[0]
        if a != b:
            out.append((o, a, b))
    return out


if __name__ == "__main__":
    c = Cap(sys.argv[1])
    vb = int(sys.argv[2])
    m = c.machine(vb)
    m.writes = set()
    kart_update(m)
    after = bytes(m.ram[KART0 & 0x1FFFFF:(KART0 & 0x1FFFFF) + KSZ])
    ref = c.read(vb + 2, KART0, KSZ)
    before = c.read(vb, KART0, KSZ)
    d = diff_kart(after, ref)
    print("instructions", m.icount, "calls", {hex(k): v for k, v in sorted(m.calls.items())})
    print("kart halfwords differing after ref step:", len(d))
    for o, a, b in d:
        pb = struct.unpack_from("<H", before, o)[0]
        print("  +%03X  before %04x  ours %04x  oracle %04x" % (o, pb, a, b))

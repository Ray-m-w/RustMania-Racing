"""Per-function fixtures for the mr-sim port, from the game's own code.

Runs the kart update (FUN_80064B5C + FUN_80086B7C) with mipsref on every tick
of a capture and, for each call of the listed functions, records the state at
entry and at return. Rust tests (port/crates/mr-sim/tests/) replay them.

    python tools/oracle/fixtures.py analysis/oracle/lap1P [--every N]

Output: <cap>/fx/<ADDR>.bin, a sequence of records:
  u32 func, u32 vb, u32 args[4], u32 v0_exit,
  u32 sp_entry,
  entry: kart[0x504] globals[0x900 @0x800B0700] scratch[0x400] stack[0x200 @sp]
  exit:  same layout
"""
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
from refstep import Cap, kart_update, KART0, KSZ  # noqa: E402

FUNCS = [0x80064B5C, 0x80086B7C, 0x8003E91C, 0x80061BC4, 0x80061FC0, 0x800628FC,
         0x80062E38, 0x800632BC, 0x80063C24, 0x80064738, 0x80082708, 0x80090424,
         0x80093D70, 0x80095010, 0x8003CD7C, 0x8003CF14, 0x8003D240, 0x8003D760,
         0x8003DA18, 0x8003DDB0, 0x8003E3A0, 0x8003E59C, 0x80064384, 0x800434C8,
         0x800326B4]
G0, GN = 0x800B0700, 0x900


def snap(m, sp):
    k = KART0 & 0x1FFFFF
    g = G0 & 0x1FFFFF
    s = sp & 0x1FFFFF
    return (bytes(m.ram[k:k + KSZ]) + bytes(m.ram[g:g + GN]) + bytes(m.sp)
            + bytes(m.ram[s:s + 0x200]))


def main():
    capdir = sys.argv[1]
    every = int(sys.argv[sys.argv.index("--every") + 1]) if "--every" in sys.argv else 1
    c = Cap(capdir)
    out = os.path.join(capdir, "fx")
    os.makedirs(out, exist_ok=True)
    files = {f: open(os.path.join(out, "%08X.bin" % f), "wb") for f in FUNCS}
    counts = {f: 0 for f in FUNCS}
    last = max(c.frames) - 2
    for vb in range(2, last, 2 * every):
        m = c.machine(vb)
        m.w16(0x800B0CE8, struct.unpack_from("<H", c.read(vb + 2, 0x800B0CE8, 2))[0])
        pending = []

        def mk_entry(f):
            def bp(mm):
                sp = mm.r[29]
                rec = [f, vb, mm.r[4], mm.r[5], mm.r[6], mm.r[7], sp, snap(mm, sp)]
                ra = mm.r[31]
                pending.append((ra, sp, rec))

                def ex(m2, ra=ra, rec=rec, sp=sp):
                    # only the matching return (same sp after the epilogue)
                    if m2.r[29] != sp:
                        return
                    rec.append(m2.r[2])
                    rec.append(snap(m2, sp))
                    fh = files[rec[0]]
                    fh.write(struct.pack("<7I", rec[0], rec[1], rec[2], rec[3], rec[4], rec[5],
                                         rec[8]))
                    fh.write(struct.pack("<I", rec[6]))
                    fh.write(rec[7])
                    fh.write(rec[9])
                    counts[rec[0]] += 1
                    lst = m2.bps_ret.get(ra, [])
                    if lst:
                        lst.pop()
                    if not lst:
                        m2.bps_ret.pop(ra, None)
                        m2.bps.pop(ra, None)
                m_ra = mm.bps_ret.setdefault(ra, [])
                m_ra.append(ex)
                mm.bps[ra] = lambda m3, ra=ra: m3.bps_ret[ra][-1](m3)
            return bp

        m.bps_ret = {}
        for f in FUNCS:
            m.bps[f] = mk_entry(f)
        kart_update(m)
    for f in FUNCS:
        files[f].close()
    for f in FUNCS:
        print("%08X %d" % (f, counts[f]))


if __name__ == "__main__":
    main()

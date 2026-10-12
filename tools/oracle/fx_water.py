"""Fixtures for FUN_8003C5FC (water animation): run the game's code under
mipsref from random states and record state/rng in and out plus the two
normals. Output: analysis/oracle/fx_water.bin, records of
  u32 dt, u32 rng_in, state_in[0x1C], u32 rng_out, state_out[0x1C], i16 ca0..ca4, ea8..eac
    python tools/oracle/fx_water.py analysis/oracle/lap1P3/ram0.bin [N]
"""
import os, random, struct, sys
sys.path.insert(0, os.path.dirname(__file__))
from mipsref import Machine

raw = open(sys.argv[1], 'rb').read()
n = int(sys.argv[2]) if len(sys.argv) > 2 else 400
rnd = random.Random(1234)
out = open(os.path.join(os.path.dirname(sys.argv[1]), '..', 'fx_water.bin'), 'wb')
S = 0x0AFEF0
for i in range(n):
    ram = bytearray(raw[:0x200000]); sp = bytearray(raw[0x200000:0x200400])
    st = bytearray(0x1C)
    for j in range(0, 0x1C, 2):
        v = rnd.randrange(0, 0x1000) if rnd.random() < 0.8 else rnd.randrange(0, 0x400)
        struct.pack_into('<H', st, j, v)
    # make some angles sit on their targets so new targets are drawn
    for a, t in ((0xC, 0xE), (0x10, 0x12), (0x14, 0x16), (0x18, 0x1A)):
        if rnd.random() < 0.3:
            st[a:a+2] = st[t:t+2]
        elif rnd.random() < 0.3:
            d = rnd.randrange(-0x40, 0x40)
            struct.pack_into('<h', st, a, struct.unpack_from('<h', st, t)[0] + d)
    ram[S:S+0x1C] = st
    dt = rnd.choice([2, 2, 2, 3])
    rng = 0x8001F94C + rnd.randrange(0, 0x5D0)
    struct.pack_into('<I', ram, 0x0B0AEC, dt)
    struct.pack_into('<I', ram, 0x0B0BCC, rng)
    m = Machine(ram, sp)
    m.call(0x8003C5FC)
    rng2 = m.r32(0x800B0BCC)
    st2 = bytes(m.ram[S:S+0x1C])
    nrm = [m.r16(a) for a in (0x800B0CA0, 0x800B0CA2, 0x800B0CA4, 0x800B0EA8, 0x800B0EAA, 0x800B0EAC)]
    out.write(struct.pack('<II', dt, rng) + bytes(st) + struct.pack('<I', rng2) + st2 + struct.pack('<6H', *nrm))
out.close()
print('wrote', n)

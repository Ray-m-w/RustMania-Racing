"""Minimal MIPS R3000A interpreter over an oracle RAM snapshot (reading aid).

Runs the game's own machine code for single functions (e.g. the kart update
FUN_80064B5C) on a memory image built from analysis/oracle/<cap>/ram0.bin plus
the per-VBlank captured regions, so the Rust port in port/crates/mr-sim can be
checked function by function against the real code with intermediate values.
It never touches the running game. GTE commands are implemented only as far as
the kart path needs them (psx-spx formulas); unknown ones raise.

    from mipsref import Machine
    m = Machine(ram_bytes, scratch_bytes)
    m.call(0x80064B5C, kart_addr, buttons, 0)
"""
import struct

MAGIC_RA = 0x8FFFFFF0


def s32(v):
    v &= 0xFFFFFFFF
    return v - 0x100000000 if v & 0x80000000 else v


def s16(v):
    v &= 0xFFFF
    return v - 0x10000 if v & 0x8000 else v


def s8(v):
    v &= 0xFF
    return v - 0x100 if v & 0x80 else v


class Unimpl(Exception):
    pass


class Machine:
    def __init__(self, ram, scratch=None):
        self.ram = bytearray(ram[:0x200000])
        self.sp = bytearray(scratch if scratch is not None else bytes(0x400))
        self.r = [0] * 32
        self.hi = 0
        self.lo = 0
        self.stubs = {}       # addr -> fn(machine) -> return value or None
        self.calls = {}       # jal target -> count
        self.writes = None    # set() to log written addresses (phys)
        self.on_call = None   # fn(target, machine)
        self.gte_d = [0] * 32
        self.gte_c = [0] * 32
        self.icount = 0
        self.reads = None     # set() of (phys, size, pc) when logging reads
        self.cur = 0
        self.cov = None       # dict pc -> count when coverage is wanted
        self.bps = {}         # pc -> fn(machine), called before executing pc

    # ---- memory ----
    def _loc(self, a):
        a &= 0x1FFFFFFF
        if a < 0x800000:
            return self.ram, a & 0x1FFFFF
        if 0x1F800000 <= a < 0x1F800400:
            return self.sp, a - 0x1F800000
        return None, a

    def r8(self, a):
        m, o = self._loc(a)
        if self.reads is not None:
            self.reads.add(((a & 0x1FFFFFFF), 1, self.cur))
        return m[o] if m is not None else 0

    def r16(self, a):
        m, o = self._loc(a)
        if self.reads is not None:
            self.reads.add(((a & 0x1FFFFFFF), 2, self.cur))
        return m[o] | (m[o + 1] << 8) if m is not None else 0

    def r32(self, a):
        m, o = self._loc(a)
        if self.reads is not None:
            self.reads.add(((a & 0x1FFFFFFF), 4, self.cur))
        if m is None:
            if (a & 0x1FFFFFFF) == 0x1F801814:
                return 0x1C000000  # GPUSTAT: ready (no GPU modelled)
            return 0
        return m[o] | (m[o + 1] << 8) | (m[o + 2] << 16) | (m[o + 3] << 24)

    def w8(self, a, v):
        m, o = self._loc(a)
        if m is not None:
            m[o] = v & 0xFF
            if self.writes is not None:
                self.writes.add(((a & 0x1FFFFFFF), 1))

    def w16(self, a, v):
        m, o = self._loc(a)
        if m is not None:
            m[o] = v & 0xFF
            m[o + 1] = (v >> 8) & 0xFF
            if self.writes is not None:
                self.writes.add(((a & 0x1FFFFFFF), 2))

    def w32(self, a, v):
        m, o = self._loc(a)
        if m is not None:
            struct.pack_into("<I", m, o, v & 0xFFFFFFFF)
            if self.writes is not None:
                self.writes.add(((a & 0x1FFFFFFF), 4))

    # ---- GTE (subset) ----
    gte_lenient = False

    def gte_cmd(self, op):
        if self.gte_lenient:
            return  # render-side exploration only: results are garbage
        raise Unimpl("GTE cmd %08x" % op)

    # ---- execution ----
    def call(self, addr, *args, sp=0x801FF000, limit=50_000_000):
        r = self.r
        for i, v in enumerate(args):
            if i < 4:
                r[4 + i] = v & 0xFFFFFFFF
            else:
                self.w32(sp + 16 + 4 * (i - 4), v)
        r[29] = sp
        r[28] = 0x800AFEC8  # $gp (gp-relative loads: 0x8003EC48 lw 0xC44(gp) = 0x800B0B0C)
        r[31] = MAGIC_RA
        self.run(addr, limit)
        return r[2]

    def run(self, pc, limit):
        r = self.r
        npc = pc + 4
        ram = self.ram
        n = 0
        stubs = self.stubs
        cov = self.cov
        bps = self.bps
        while True:
            if pc in bps:
                bps[pc](self)
            if pc == MAGIC_RA:
                break
            if pc in stubs:
                ret = stubs[pc](self)
                if ret is not None:
                    r[2] = ret & 0xFFFFFFFF
                pc = r[31]
                npc = pc + 4
                continue
            if cov is not None:
                cov[pc] = cov.get(pc, 0) + 1
            n += 1
            if n > limit:
                raise RuntimeError("instruction limit at %08x" % pc)
            o = pc & 0x1FFFFF
            ins = ram[o] | (ram[o + 1] << 8) | (ram[o + 2] << 16) | (ram[o + 3] << 24)
            cur = pc
            self.cur = cur
            pc = npc
            npc = pc + 4
            op = ins >> 26
            rs = (ins >> 21) & 31
            rt = (ins >> 16) & 31
            if op == 0:
                rd = (ins >> 11) & 31
                fn = ins & 63
                sa = (ins >> 6) & 31
                if fn == 0x21 or fn == 0x20:
                    v = (r[rs] + r[rt]) & 0xFFFFFFFF
                elif fn == 0x23 or fn == 0x22:
                    v = (r[rs] - r[rt]) & 0xFFFFFFFF
                elif fn == 0x00:
                    v = (r[rt] << sa) & 0xFFFFFFFF
                elif fn == 0x02:
                    v = r[rt] >> sa
                elif fn == 0x03:
                    v = (s32(r[rt]) >> sa) & 0xFFFFFFFF
                elif fn == 0x04:
                    v = (r[rt] << (r[rs] & 31)) & 0xFFFFFFFF
                elif fn == 0x06:
                    v = r[rt] >> (r[rs] & 31)
                elif fn == 0x07:
                    v = (s32(r[rt]) >> (r[rs] & 31)) & 0xFFFFFFFF
                elif fn == 0x24:
                    v = r[rs] & r[rt]
                elif fn == 0x25:
                    v = r[rs] | r[rt]
                elif fn == 0x26:
                    v = r[rs] ^ r[rt]
                elif fn == 0x27:
                    v = (~(r[rs] | r[rt])) & 0xFFFFFFFF
                elif fn == 0x2A:
                    v = 1 if s32(r[rs]) < s32(r[rt]) else 0
                elif fn == 0x2B:
                    v = 1 if r[rs] < r[rt] else 0
                elif fn == 0x08:  # jr
                    npc = r[rs]
                    continue
                elif fn == 0x09:  # jalr
                    t = r[rs]
                    if rd:
                        r[rd] = (cur + 8) & 0xFFFFFFFF
                    npc = t
                    self.calls[t] = self.calls.get(t, 0) + 1
                    if self.on_call:
                        self.on_call(t, self)
                    continue
                elif fn == 0x10:
                    v = self.hi
                elif fn == 0x12:
                    v = self.lo
                elif fn == 0x11:
                    self.hi = r[rs]
                    continue
                elif fn == 0x13:
                    self.lo = r[rs]
                    continue
                elif fn == 0x18:
                    p = s32(r[rs]) * s32(r[rt])
                    self.lo = p & 0xFFFFFFFF
                    self.hi = (p >> 32) & 0xFFFFFFFF
                    continue
                elif fn == 0x19:
                    p = r[rs] * r[rt]
                    self.lo = p & 0xFFFFFFFF
                    self.hi = (p >> 32) & 0xFFFFFFFF
                    continue
                elif fn == 0x1A:
                    a, b = s32(r[rs]), s32(r[rt])
                    if b == 0:
                        self.lo = 1 if a < 0 else 0xFFFFFFFF
                        self.hi = a & 0xFFFFFFFF
                    elif a == -0x80000000 and b == -1:
                        self.lo = 0x80000000
                        self.hi = 0
                    else:
                        q = abs(a) // abs(b)
                        if (a < 0) != (b < 0):
                            q = -q
                        self.lo = q & 0xFFFFFFFF
                        self.hi = (a - q * b) & 0xFFFFFFFF
                    continue
                elif fn == 0x1B:
                    a, b = r[rs], r[rt]
                    if b == 0:
                        self.lo = 0xFFFFFFFF
                        self.hi = a
                    else:
                        self.lo = a // b
                        self.hi = a % b
                    continue
                elif fn == 0x0C or fn == 0x0D:
                    continue  # syscall/break: ignored
                else:
                    raise Unimpl("special fn %02x at %08x" % (fn, cur))
                if rd:
                    r[rd] = v
                continue
            if op == 0x09 or op == 0x08:
                if rt:
                    r[rt] = (r[rs] + s16(ins)) & 0xFFFFFFFF
                continue
            if op == 0x0F:
                if rt:
                    r[rt] = (ins & 0xFFFF) << 16
                continue
            if op == 0x0D:
                if rt:
                    r[rt] = r[rs] | (ins & 0xFFFF)
                continue
            if op == 0x0C:
                if rt:
                    r[rt] = r[rs] & (ins & 0xFFFF)
                continue
            if op == 0x0E:
                if rt:
                    r[rt] = r[rs] ^ (ins & 0xFFFF)
                continue
            if op == 0x0A:
                if rt:
                    r[rt] = 1 if s32(r[rs]) < s16(ins) else 0
                continue
            if op == 0x0B:
                if rt:
                    r[rt] = 1 if r[rs] < (s16(ins) & 0xFFFFFFFF) else 0
                continue
            if 0x20 <= op <= 0x26:
                a = (r[rs] + s16(ins)) & 0xFFFFFFFF
                if op == 0x23:
                    v = self.r32(a)
                elif op == 0x21:
                    v = s16(self.r16(a)) & 0xFFFFFFFF
                elif op == 0x25:
                    v = self.r16(a)
                elif op == 0x20:
                    v = s8(self.r8(a)) & 0xFFFFFFFF
                elif op == 0x24:
                    v = self.r8(a)
                elif op == 0x22:  # lwl
                    w = self.r32(a & ~3)
                    sh = (a & 3)
                    keep = (0x00FFFFFF >> (sh * 8))
                    v = (r[rt] & keep) | ((w << (24 - sh * 8)) & 0xFFFFFFFF)
                elif op == 0x26:  # lwr
                    w = self.r32(a & ~3)
                    sh = (a & 3)
                    keep = (0xFFFFFF00 << (24 - sh * 8)) & 0xFFFFFFFF
                    v = (r[rt] & keep) | (w >> (sh * 8))
                if rt:
                    r[rt] = v
                continue
            if 0x28 <= op <= 0x2E:
                a = (r[rs] + s16(ins)) & 0xFFFFFFFF
                if op == 0x2B:
                    self.w32(a, r[rt])
                elif op == 0x29:
                    self.w16(a, r[rt])
                elif op == 0x28:
                    self.w8(a, r[rt])
                elif op == 0x2A:  # swl
                    w = self.r32(a & ~3)
                    sh = a & 3
                    mask = (0xFFFFFF00 << (sh * 8)) & 0xFFFFFFFF
                    v = (w & mask) | (r[rt] >> (24 - sh * 8))
                    self.w32(a & ~3, v)
                elif op == 0x2E:  # swr
                    w = self.r32(a & ~3)
                    sh = a & 3
                    mask = 0x00FFFFFF >> (24 - sh * 8)
                    v = (w & mask) | ((r[rt] << (sh * 8)) & 0xFFFFFFFF)
                    self.w32(a & ~3, v)
                else:
                    raise Unimpl("store op %02x" % op)
                continue
            if op == 0x04:  # beq
                if r[rs] == r[rt]:
                    npc = (pc + (s16(ins) << 2)) & 0xFFFFFFFF
                continue
            if op == 0x05:
                if r[rs] != r[rt]:
                    npc = (pc + (s16(ins) << 2)) & 0xFFFFFFFF
                continue
            if op == 0x06:
                if s32(r[rs]) <= 0:
                    npc = (pc + (s16(ins) << 2)) & 0xFFFFFFFF
                continue
            if op == 0x07:
                if s32(r[rs]) > 0:
                    npc = (pc + (s16(ins) << 2)) & 0xFFFFFFFF
                continue
            if op == 0x01:
                v = s32(r[rs])
                take = (v < 0) if (rt & 1) == 0 else (v >= 0)
                if rt & 0x10:
                    r[31] = (cur + 8) & 0xFFFFFFFF
                if take:
                    npc = (pc + (s16(ins) << 2)) & 0xFFFFFFFF
                continue
            if op == 0x02 or op == 0x03:
                t = (pc & 0xF0000000) | ((ins & 0x3FFFFFF) << 2)
                if op == 0x03:
                    r[31] = (cur + 8) & 0xFFFFFFFF
                    self.calls[t] = self.calls.get(t, 0) + 1
                    if self.on_call:
                        self.on_call(t, self)
                npc = t
                continue
            if op == 0x12:  # COP2
                if ins & (1 << 25):
                    self.gte_cmd(ins & 0x1FFFFFF)
                    continue
                sub = rs
                if sub == 0:  # mfc2
                    if rt:
                        r[rt] = self.gte_read(((ins >> 11) & 31))
                elif sub == 2:  # cfc2
                    if rt:
                        r[rt] = self.gte_c[(ins >> 11) & 31]
                elif sub == 4:
                    self.gte_write((ins >> 11) & 31, r[rt])
                elif sub == 6:
                    self.gte_c[(ins >> 11) & 31] = r[rt]
                else:
                    raise Unimpl("cop2 sub %d" % sub)
                continue
            if op == 0x32:  # lwc2
                a = (r[rs] + s16(ins)) & 0xFFFFFFFF
                self.gte_write(rt, self.r32(a))
                continue
            if op == 0x3A:  # swc2
                a = (r[rs] + s16(ins)) & 0xFFFFFFFF
                self.w32(a, self.gte_read(rt))
                continue
            if op == 0x10:  # cop0
                if rs == 0:
                    if rt:
                        r[rt] = 0
                continue
            raise Unimpl("op %02x at %08x ins %08x" % (op, cur, ins))
        self.icount += n

    def gte_read(self, i):
        return self.gte_d[i] & 0xFFFFFFFF

    def gte_write(self, i, v):
        v &= 0xFFFFFFFF
        self.gte_d[i] = v
        if i == 30:  # LZCS -> LZCR: count of leading bits equal to bit 31 (psx-spx)
            x = v if not (v & 0x80000000) else (~v) & 0xFFFFFFFF
            n = 0
            while n < 32 and not (x & (0x80000000 >> n)):
                n += 1
            self.gte_d[31] = n


def build_memory(ram0_path, regions=None, frame_parts=None):
    """ram0.bin (2 MB main + 1 KB scratchpad) overlaid with captured regions."""
    raw = open(ram0_path, "rb").read()
    ram = bytearray(raw[:0x200000])
    sp = bytearray(raw[0x200000:0x200400])
    if regions and frame_parts:
        overlay(ram, sp, regions, frame_parts)
    return ram, sp


def overlay(ram, sp, regions, parts):
    for (a, n), d in zip(regions, parts):
        p = a & 0x1FFFFFFF
        if 0x1F800000 <= p < 0x1F800400:
            o = p - 0x1F800000
            sp[o:o + n] = d
        else:
            o = p & 0x1FFFFF
            ram[o:o + n] = d

"""Client for the oracle bridge plugin (src/mods/mrm_dev_oracle_bridge_plugin.c).

Shared memory "Local\\mrm_oracle_bridge_v4" (v3 accepted while an older build
runs): per-VBlank captures of chosen RAM regions, savestate-anchored input
routes, and (v4) RAM pokes armed with a run. Layout mirrors the C header.
"""
import mmap
import struct
import time

NAME = "Local\\mrm_oracle_bridge_v4"
NAMES = {4: "Local\\mrm_oracle_bridge_v4", 3: "Local\\mrm_oracle_bridge_v3"}
MAGIC = 0x424D524D
VERSION = 4
HEADER = 4096
RAMSNAP = 0x200000 + 0x400
NSLOTS = 4096
SLOT = 32768
SLOT_HDR = 24
MAX_REGIONS = 32
MAX_STEPS = 8192
STEPS = MAX_STEPS * 12

O_SEQ, O_GEN, O_NREG, O_REG = 16, 24, 28, 32
O_ARMED, O_RLEN, O_RACTIVE, O_REND, O_VBSR, O_VBT, O_SRC, O_SNAPGEN = 288, 292, 296, 300, 304, 308, 312, 316
O_STEPS = 4096  # step table follows the header (v3)
O_POKES_ARMED, O_POKES_ACTIVE, O_POKES = 320, 324, 512  # v4
MAX_POKES = 64
POKE_AT_RESTORE, POKE_EVERY_VBLANK = 1, 2


class Bridge:
    def __init__(self):
        # Newest layout first; v3 (no pokes) has the same offsets otherwise.
        for ver in (4, 3):
            self.m = mmap.mmap(-1, HEADER + STEPS + RAMSNAP + NSLOTS * SLOT, tagname=NAMES[ver])
            magic, v = struct.unpack_from("<II", self.m, 0)
            if magic == MAGIC and v == ver:
                self.version = ver
                break
            self.m.close()
        else:
            raise RuntimeError("bridge not initialised: is mrm.dev.oracle-bridge enabled and the game running?")
        self.regions = []

    def u32(self, off):
        return struct.unpack_from("<I", self.m, off)[0]

    def put_u32(self, off, v):
        struct.pack_into("<I", self.m, off, v)

    @property
    def seq(self):
        return struct.unpack_from("<Q", self.m, O_SEQ)[0]

    @property
    def restore_gen(self):
        return self.u32(O_GEN)

    @property
    def vb_since_restore(self):
        return self.u32(O_VBSR)

    def set_regions(self, regions):
        """regions: [(addr, len)], total <= SLOT - SLOT_HDR bytes."""
        assert len(regions) <= MAX_REGIONS
        assert sum(n for _, n in regions) <= SLOT - SLOT_HDR
        self.put_u32(O_NREG, 0)
        for i, (a, n) in enumerate(regions):
            struct.pack_into("<II", self.m, O_REG + 8 * i, a & 0xFFFFFFFF, n)
        self.put_u32(O_NREG, len(regions))
        self.regions = list(regions)

    def arm_route(self, steps, end_vb=0):
        """steps: [(vb_start, buttons_active_high, lx, ly, rx, ry)] sorted by vb_start.
        Starts at the next savestate restore; vb counts VBlanks since restore."""
        assert len(steps) <= MAX_STEPS
        self.put_u32(O_ARMED, 0)
        for i, st in enumerate(steps):
            vb, b, lx, ly, rx, ry = (list(st) + [0x80] * 4)[:6]
            struct.pack_into("<IIBBBB", self.m, O_STEPS + 12 * i, vb, b, lx, ly, rx, ry)
        self.put_u32(O_RLEN, len(steps))
        self.put_u32(O_REND, end_vb)
        self.put_u32(O_ARMED, 1)

    def arm_pokes(self, pokes):
        """pokes: [(addr, value, size, when)], when = POKE_AT_RESTORE and/or
        POKE_EVERY_VBLANK. Taken at the next savestate restore (with the
        route); a restore without armed pokes clears them. Needs bridge v4."""
        if self.version < 4:
            raise RuntimeError("RAM pokes need oracle bridge v4 (rebuild build-dev, restart the game)")
        assert len(pokes) <= MAX_POKES
        self.put_u32(O_POKES_ARMED, 0)
        for i, (a, v, n, when) in enumerate(pokes):
            assert n in (1, 2, 4)
            struct.pack_into("<IIBBH", self.m, O_POKES + 12 * i, a & 0xFFFFFFFF, v & 0xFFFFFFFF, n, when, 0)
        self.put_u32(O_POKES_ARMED, len(pokes))

    def slots(self, since_seq):
        """Yield (seq, restore_gen, vb_since_restore, vb_total, bytes) for slots
        written after since_seq that are still in the ring."""
        top = self.seq
        lo = max(since_seq, top - NSLOTS + 16)
        for s in range(lo, top):
            off = HEADER + STEPS + RAMSNAP + (s % NSLOTS) * SLOT
            sq, gen, vbr, vbt = struct.unpack_from("<QIII", self.m, off)
            if sq != s:
                continue
            n = sum(l for _, l in self.regions)
            yield sq, gen, vbr, vbt, bytes(self.m[off + SLOT_HDR: off + SLOT_HDR + n])

    def split(self, data):
        """Cut a slot payload back into per-region byte strings."""
        out, o = [], 0
        for _, n in self.regions:
            out.append(data[o:o + n])
            o += n
        return out

    def wait_restore(self, gen0, timeout=30):
        t = time.time()
        while self.restore_gen == gen0:
            if time.time() - t > timeout:
                raise TimeoutError("no savestate restore seen")
            time.sleep(0.01)

    def ram_snapshot(self, gen):
        """Main RAM (2 MB) + scratchpad (1 KB) as copied at restore `gen`."""
        if self.u32(O_SNAPGEN) != gen:
            raise RuntimeError("no RAM snapshot for restore %d" % gen)
        return bytes(self.m[HEADER + STEPS:HEADER + STEPS + RAMSNAP])

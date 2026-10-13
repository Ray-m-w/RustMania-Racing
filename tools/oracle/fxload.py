"""Load an fx_capture.py directory: per-VBlank rows -> address reads.

    from fxload import Cap; c = Cap("analysis/fx/race3"); c.u16(vb, 0x800C30A8 + 0x48A)
"""
import json
import os
import struct


class Cap:
    def __init__(self, d):
        self.d = d
        self.meta = json.load(open(os.path.join(d, "meta.json")))
        self.regions = self.meta["regions"]
        self.size = sum(n for _, n in self.regions)
        self.data = open(os.path.join(d, "frames.bin"), "rb").read()
        self.index = {vb: i for i, vb in enumerate(self.meta["vb"])}
        self.vbt = dict(zip(self.meta["vb"], self.meta["vbt"]))
        self.byvbt = {t: vb for vb, t in self.vbt.items()}

    def row(self, vb):
        i = self.index[vb]
        return self.data[i * self.size:(i + 1) * self.size]

    def off(self, addr):
        o = 0
        for a, n in self.regions:
            if a <= addr < a + n:
                return o + addr - a
            o += n
        raise KeyError(hex(addr))

    def read(self, vb, addr, n):
        o = self.off(addr)
        return self.row(vb)[o:o + n]

    def u8(self, vb, a): return self.read(vb, a, 1)[0]
    def s8(self, vb, a): return struct.unpack("<b", self.read(vb, a, 1))[0]
    def u16(self, vb, a): return struct.unpack("<H", self.read(vb, a, 2))[0]
    def s16(self, vb, a): return struct.unpack("<h", self.read(vb, a, 2))[0]
    def s32(self, vb, a): return struct.unpack("<i", self.read(vb, a, 4))[0]

    def gpu(self, vb):
        p = os.path.join(self.d, "gpu", f"{vb}.json")
        return json.load(open(p)) if os.path.exists(p) else None

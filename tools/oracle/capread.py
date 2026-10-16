"""Read addresses from a capture: capread.py DIR [VB...] (Session 7 phase 6 helper).
    import capread; c = capread.Cap(DIR); c.u(vb, addr, size, signed=False)"""
import os
import sys

sys.path.insert(0, os.path.dirname(__file__))
import capture  # noqa: E402


class Cap:
    def __init__(self, d):
        self.meta, self.frames = capture.load(d)
        self.vbs = sorted(self.frames)

    def raw(self, vb, a, n):
        for (base, size), part in zip(self.meta["regions"], self.frames[vb]):
            if base <= a and a + n <= base + size:
                return part[a - base:a - base + n]
        return None

    def u(self, vb, a, n=4, signed=False):
        r = self.raw(vb, a, n)
        return None if r is None else int.from_bytes(r, "little", signed=signed)

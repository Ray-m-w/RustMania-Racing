"""Oracle client: the psxrecomp debug build as the reference for the Rust port.

Wraps psxrecomp's TCP debug server (psxrecomp/docs/TCP_COMMANDS.md) with the
few operations the port's harness needs: load a savestate, replay an input
route frame-exactly, and read RAM. Library + CLI; see SESSION-2-oracle.md.
"""
import json
import os
import socket
import time

PORT = int(os.environ.get("MRM_DEBUG_PORT", "4370"))

# PS1 pad bits (active low on the wire; psxrecomp set_input takes the inverted mask).
BITS = {"select": 0, "start": 3, "up": 4, "right": 5, "down": 6, "left": 7,
        "l2": 8, "r2": 9, "l1": 10, "r1": 11, "triangle": 12, "circle": 13,
        "cross": 14, "square": 15}


class Oracle:
    def __init__(self, port=PORT, timeout=60):
        self.port = port
        self.timeout = timeout
        self.sock = None
        self.buf = b""
        self.nid = 0

    def _connect(self):
        self.sock = socket.create_connection(("127.0.0.1", self.port), timeout=self.timeout)
        self.buf = b""

    def q(self, cmd, **kw):
        """One request, one response line. Reconnects once on a dropped socket."""
        for attempt in (0, 1):
            try:
                if self.sock is None:
                    self._connect()
                self.nid += 1
                kw2 = dict(kw, id=self.nid, cmd=cmd)
                self.sock.sendall((json.dumps(kw2) + "\n").encode())
                while True:
                    # Some handlers end their reply with an extra newline;
                    # skip blank lines (one reply per request otherwise).
                    while b"\n" not in self.buf:
                        chunk = self.sock.recv(1 << 20)
                        if not chunk:
                            raise ConnectionError("closed")
                        self.buf += chunk
                    line, _, self.buf = self.buf.partition(b"\n")
                    if line.strip():
                        return json.loads(line)
            except (ConnectionError, OSError):
                self.sock = None
                if attempt:
                    raise
        raise RuntimeError("unreachable")

    def frame(self):
        return self.q("frame")["frame"]

    def wait_frame(self, target, poll=0.02):
        while True:
            f = self.frame()
            if f >= target:
                return f
            time.sleep(poll)

    def read(self, addr, n):
        r = self.q("read_ram", addr=f"0x{addr:08X}", len=n)
        if not r.get("ok"):
            raise RuntimeError(r)
        return bytes.fromhex(r["hex"] if "hex" in r else r["data"])

    def load_state(self, slot, timeout=30):
        gen0 = self.q("savestate_status")["generation"]
        r = self.q("savestate", op="load", slot=slot)
        if not r.get("ok"):
            raise RuntimeError(r)
        t = time.time()
        while time.time() - t < timeout:
            s = self.q("savestate_status")
            if s["generation"] != gen0 and not s["pending"]:
                if not s["last_ok"]:
                    raise RuntimeError(f"load failed: {s}")
                return s
            time.sleep(0.05)
        raise TimeoutError("savestate load")

    def route(self, steps, at_frame=None):
        """steps: [(mask_spec, frames)], mask_spec '' = no buttons, 'triangle+left'.
        Frames are counted per pad poll (TCP_COMMANDS.md input_route_start)."""
        self.q("input_route_clear")
        for spec, n in steps:
            r = self.q("input_route_append", buttons=mask(spec), frames=int(n))
            if not r.get("ok"):
                raise RuntimeError(r)
        kw = {} if at_frame is None else {"at_frame": int(at_frame)}
        r = self.q("input_route_start", **kw)
        if not r.get("ok"):
            raise RuntimeError(r)
        return r


def mask(spec):
    m = 0xFFFF
    if spec:
        for b in spec.split("+"):
            m &= ~(1 << BITS[b])
    return m

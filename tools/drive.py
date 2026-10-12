"""Drive a running debug-tools build over psxrecomp's TCP debug server
(psxrecomp/docs/TCP_COMMANDS.md). Development tooling for this project.

    python tools/drive.py STEP [STEP ...]

Steps (run in order):
    frame                       print the guest frame
    until:N                     turbo until guest frame >= N, then turbo off
    wait:S                      sleep S seconds (real time)
    press:BTN[+BTN][*frames]    press buttons, e.g. press:cross, press:up*10
    hold:BTN[+BTN]              hold buttons until release
    release                     clear held input
    aspect:W:H                  display_aspect (4:3 restores the original)
    shot:NAME                   window capture (present_shot) -> shots dir
    vram:NAME                   native VRAM capture (screenshot) -> shots dir
    cmd:NAME[,k=v...]           any server command, prints the reply

Buttons: select start up right down left l2 r2 l1 r1 triangle circle cross square.
Note: while turbo is on the window gets no presents (session 1 finding), so
`until` always turns it off before returning.
"""
import json
import os
import socket
import sys
import time

PORT = int(os.environ.get("MRM_DEBUG_PORT", "4370"))
SHOTS = os.environ.get("MRM_SHOTS", os.path.join(os.environ.get("TEMP", "."), "mrm-shots"))
BITS = {"select": 0, "start": 3, "up": 4, "right": 5, "down": 6, "left": 7,
        "l2": 8, "r2": 9, "l1": 10, "r1": 11, "triangle": 12, "circle": 13,
        "cross": 14, "square": 15}


def q(cmd, **kw):
    kw.update({"id": 1, "cmd": cmd})
    with socket.create_connection(("127.0.0.1", PORT), timeout=30) as s:
        s.sendall((json.dumps(kw) + "\n").encode())
        buf = b""
        while not buf.endswith(b"\n"):
            chunk = s.recv(1 << 20)
            if not chunk:
                break
            buf += chunk
    return json.loads(buf)


def mask(spec):
    m = 0xFFFF
    for b in spec.split("+"):
        m &= ~(1 << BITS[b])
    return m


def frame():
    return q("frame")["frame"]


def run(step):
    kind, _, arg = step.partition(":")
    if kind == "frame":
        print("frame", frame())
    elif kind == "until":
        target = int(arg)
        q("turbo", enabled=1)
        try:
            while frame() < target:
                time.sleep(1)
        finally:
            q("turbo", enabled=0)
        print("frame", frame())
    elif kind == "wait":
        time.sleep(float(arg))
    elif kind in ("press", "press2"):
        spec, _, n = arg.partition("*")
        extra = {"port": 2} if kind == "press2" else {}
        q("press", buttons=mask(spec), frames=int(n or 6), **extra)
        time.sleep(max(0.25, int(n or 6) / 60 + 0.15))
    elif kind == "hold":
        q("set_input", buttons=mask(arg))
    elif kind == "release":
        q("clear_input")
    elif kind == "aspect":
        w, h = arg.split(":")
        print(q("display_aspect", num=int(w), den=int(h)))
    elif kind in ("shot", "vram"):
        os.makedirs(SHOTS, exist_ok=True)
        path = os.path.join(SHOTS, f"{arg}-{frame()}.png").replace("\\", "/")
        if kind == "vram":
            r = q("screenshot", path=path)
        else:
            seq = q("present_shot_seq")["seq"]
            r = q("present_shot", path=path)
            for _ in range(100):
                s = q("present_shot_seq")
                if s["seq"] != seq:
                    r = dict(r, wrote=s.get("wrote"))
                    break
                time.sleep(0.05)
        print(path if r.get("ok") else r)
    elif kind == "ws":
        ws = q("gpu_state")["ws"]
        print({k: v for k, v in ws.items() if not isinstance(v, (dict, list))})
    elif kind == "cmd":
        name, *pairs = arg.split(",")
        kw = {}
        for p in pairs:
            k, _, v = p.partition("=")
            try:
                kw[k] = int(v, 0)
            except ValueError:
                kw[k] = v
        print(json.dumps(q(name, **kw)))
    else:
        raise SystemExit(f"unknown step {step}")


for s in sys.argv[1:]:
    run(s)

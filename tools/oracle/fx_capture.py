"""Capture a route with the item HUD / effects state and chosen GPU frames.

    python tools/oracle/fx_capture.py ROUTE.json OUTDIR [--frames VB,VB,...]
        [--every N] [--end VB] [--shots VB,VB,...] [--vram X,Y,W,H]

Session 4 track B (item HUD + effects). Under capture.OracleLock: replays
ROUTE from its savestate (bridge v4), records FX_REGIONS every VBlank
(frames.bin + meta.json like capture.py, plus vbt = the runtime's VBlank
counter for each row) and, while the run goes on, dumps the GP0 packets of
the frames drawn around each requested VBlank (gpu/<vbr>.json, compact:
op, kind, verts, uvs, clut, tpage, semi, raw, colours, size, func).
The GP0 ring holds ~2,000 frames, so the dumps are taken during the run.

Session 5 track F: --shots takes a native `screenshot` (shots/o_<VB>.png) as
soon as the run reaches each VBlank (the VBlank it landed on is printed and
kept in shots/shots.json), and with --vram reads that VRAM rectangle
(`vram_peek`, at most 128 x 128) at the same moment (texture animation
timing). Regions added: the flat-quad list 0x800C84B8 and the shadow list
0x800C8B38 (karts, item boxes, the first 32 tokens); dropped (bridge slot
size): the billboard batch 0x800DF688 and the course ambient list
0x800D75D0 (off on DOCK).
"""
import json
import os
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(ROOT, "psxrecomp", "tools"))
import psx_gpu_frame as g  # noqa: E402
import capture  # noqa: E402
from capture import OracleLock, warm_up, active_high  # noqa: E402
from oracle import Oracle  # noqa: E402
from bridge import Bridge  # noqa: E402

KART0 = 0x800C30A8
FX_REGIONS = [
    (0x800C3000, 0xA8 + 6 * 0x504),  # kart index table + 6 kart records
    (0x800C4F40, 0x120),             # cameras
    (0x800B0700, 0x900),             # game globals (HUD scales, frame counter, sparkle globals, ...)
    (0x800B4540, 0x400),             # spark streaks: 32 x 0x20 (FUN_800326b4 / FUN_800327d0)
    (0x800DDC50, 0x120),             # projectiles: 4 x 0x48 (FUN_80030d78 draws)
    (0x800E1B70, 0x1400),            # pickups: item boxes 0..0x3F, turbo tokens 0x40.. (0x800E2370)
    (0x800DFF00, 0x180),             # texture slot table (tpage, clut) + 0x800E0040.. slots
    (0x800D1C70, 0x40),              # list pointers d1c78 / d1c7c / d1c94 / d1c9c
    (0x800D8F90, 0x10),              # race leader
    (0x1F800380, 0x40),              # scratchpad camera translation used by the billboards
    (0x80009010, 0x4),               # BIOS rand seed
    (0x800E0C20, 0xF00),             # sprite particles: 64 x 0x3C (FUN_800870E8 / FUN_8008727C)
    (0x800D73D0, 0x200),             # their side table: 64 x 8
    (0x800C84B8, 0x680),             # flat quads: 32 x 0x34 (FUN_800330D4 / FUN_80033440 / FUN_800342B8)
    (0x800C8B38, 0xD20),             # shadow list 0..0x45: karts 0..5, item boxes 6.. (x 0x30)
    (0x800C9858, 0x600),             # shadow list 0x46..0x65: turbo tokens 0..31
    (0x800B0688, 0x8),               # phase 6b: 0x800B068F (ADVENTURE's target marker)
]


def compact(prims):
    out = []
    for p in prims:
        if p["kind"] in ("env", "fill", "copy"):
            out.append({"k": p["kind"], "op": p["op"], "w": p["words"][:2]})
            continue
        out.append({"k": p["kind"], "op": p["op"], "n": p["op_name"], "v": p["verts"], "uv": p["uvs"],
                    "clut": p["clut"], "tp": p["tpage"], "semi": int(p["semi"]), "raw": int(p["raw_tex"]),
                    "c": p["colors"], "sz": p["size"], "f": p["func"], "stp": p["stp"],
                    "off": p["state"]["offset"]})
    return out


def main():
    a = sys.argv[1:]
    route = json.load(open(a[0]))
    out = a[1]
    opt = lambda k, d=None: a[a.index(k) + 1] if k in a else d
    frames = sorted(int(x) for x in opt("--frames", "").split(",") if x)
    every = int(opt("--every", "0"))
    end_vb = int(opt("--end", route.get("end_vb", 1200)))
    if every:
        frames = sorted(set(frames) | set(range(every, end_vb, every)))
    shots = sorted(int(x) for x in opt("--shots", "").split(",") if x)
    vram = [int(x) for x in opt("--vram", "").split(",") if x]
    shot_log = []
    os.makedirs(os.path.join(out, "gpu"), exist_ok=True)
    if shots:
        os.makedirs(os.path.join(out, "shots"), exist_ok=True)
    with OracleLock():
        o, b = Oracle(), Bridge()
        warm_up(o, b, route.get("slot", 2))
        b.set_regions(FX_REGIONS)
        steps = []
        for st in route["steps"]:
            lx = st[2] if len(st) > 2 else 0x80
            ly = st[3] if len(st) > 3 else 0x80
            steps.append((st[0], active_high(st[1]), lx, ly, 0x80, 0x80))
        b.arm_route(steps, end_vb=max(end_vb, int(route.get("end_vb", 0))))
        pokes = [tuple(p) for p in route.get("pokes", [])]
        if pokes:
            b.arm_pokes(pokes)
        gen0, seq0 = b.restore_gen, b.seq
        o.load_state(route.get("slot", 2))
        b.wait_restore(gen0)
        gen = b.restore_gen
        rows, vbt = {}, {}
        todo = list(frames)
        while True:
            while shots and b.vb_since_restore >= shots[0]:
                t = shots.pop(0)
                rec = {"target": t}
                if len(vram) == 4:
                    r = o.q("vram_peek", x=vram[0], y=vram[1], w=vram[2], h=vram[3])
                    rec["vram_vb"] = b.vb_since_restore
                    rec["vram"] = r
                path = os.path.join(os.path.abspath(out), "shots", f"o_{t}.png").replace("\\", "/")
                rec["shot_vb"] = b.vb_since_restore
                rec["shot"] = o.q("screenshot", path=path)
                rec["after_vb"] = b.vb_since_restore
                shot_log.append(rec)
                print("shot", t, "vblank", rec["shot_vb"], flush=True)
            for sq, gg, vbr, t, data in b.slots(seq0):
                seq0 = sq + 1
                if gg == gen and vbr not in rows:
                    rows[vbr] = data
                    vbt[vbr] = t
            # dump GP0 frames once the run is 4 VBlanks past a target
            while todo and rows and max(rows) >= todo[0] + 4:
                t = todo.pop(0)
                base = vbt.get(t)
                if base is None:
                    continue
                dumps = {}
                with g.DebugConn(port=4370) as c:
                    for f in range(base - 1, base + 3):
                        try:
                            d = g.capture(c, frame=f)
                        except g.DebugError as e:
                            print("frame", f, e)
                            continue
                        if len(d["prims"]) > 50:
                            dumps[f] = compact(d["prims"])
                json.dump({"vbr": t, "vbt": base, "frames": dumps}, open(os.path.join(out, "gpu", f"{t}.json"), "w"))
                print("vb", t, "frames", {k: len(v) for k, v in dumps.items()}, flush=True)
            if rows and max(rows) >= end_vb + 30 and not todo:
                break
            time.sleep(0.002 if shots else 0.02)
    if shot_log:
        json.dump(shot_log, open(os.path.join(out, "shots", "shots.json"), "w"))
    keys = sorted(rows)
    with open(os.path.join(out, "frames.bin"), "wb") as f:
        for k in keys:
            f.write(rows[k])
    meta = {"route": route, "regions": [[x, n] for x, n in FX_REGIONS], "vb": keys,
            "vbt": [vbt[k] for k in keys], "restore_gen": gen,
            "missing": [v for v in range(keys[0], keys[-1] + 1) if v not in rows]}
    json.dump(meta, open(os.path.join(out, "meta.json"), "w"))
    print("rows", len(keys), "missing", len(meta["missing"]))


if __name__ == "__main__":
    main()

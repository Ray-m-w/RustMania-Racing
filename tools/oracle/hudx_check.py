"""Check the port's full race HUD (mr-hudx) against the game's GP0 packets.

    python tools/oracle/hudx_check.py CAPDIR [VB ...] [--verbose] [--lag a,b]

CAPDIR comes from fx_capture.py (RAM rows every VBlank + gpu/<VB>.json GP0
dumps of the frames drawn around VB). For each dumped frame the HUD state is
taken from the captured RAM row LAG VBlanks before the frame's VBlank (the
frame shown at VBlank t was built from the state of an earlier VBlank; the
lag that matches best is reported) and laid out by `mr-hudx`; every port
primitive is looked up among the game's primitives of the same kind:
SPRT (x, y, w, h, u, v, texpage, clut, raw, semi, colour), POLY_FT4 (4
vertices, 4 UVs, texpage, clut, semi, colour), PolyF3 (3 vertices, colour),
LineF2 (2 vertices, colour). Prints matched / port totals per frame and the
leftovers with --verbose. Game primitives that look like HUD (screen-space
font / HUD texture slots, LineF, PolyF3) and match nothing are listed too.
"""
import json
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, HERE)
from fxload import Cap  # noqa: E402

EXE = os.path.join(ROOT, "port", os.environ.get("MR_TARGET", "target-b"), "debug", "mr-hudx.exe")


def game_prims(frame):
    """Game primitives keyed like mr-hudx's lines."""
    tp = 0
    out = []
    for p in frame:
        if p["k"] == "env":
            w0 = int(p["w"][0], 16) if p["w"] else 0
            if (w0 >> 24) == 0xE1:
                tp = w0 & 0x7FF
            continue
        if p["k"] in ("fill", "copy") or "v" not in p:
            continue
        ox, oy = p["off"]
        v = [(x - ox, y - oy) for x, y in p["v"]]
        c = tuple(p["c"][0]) if p["c"] else (0, 0, 0)
        n = p["n"]
        if p["k"] == "rect" and p.get("uv"):
            w, h = p["sz"]
            out.append(("S", v[0][0], v[0][1], w, h, p["uv"][0][0], p["uv"][0][1], tp & 0x1FF, p["clut"],
                        p["raw"], p["semi"], c if not p["raw"] else (128, 128, 128)))
        elif n.startswith("PolyFT4"):
            tp = p["tp"] & 0x7FF
            out.append(("Q",) + tuple(x for q in v for x in q) + tuple(x for q in p["uv"] for x in q)
                       + (tp & 0x1FF, p["clut"], p["semi"], c))
        elif n.startswith("PolyF3"):
            out.append(("T",) + tuple(x for q in v for x in q) + (c,))
        elif n.startswith("LineF"):
            out.append(("L",) + tuple(x for q in v for x in q) + (c,))
        elif p.get("tp") is not None:
            tp = p["tp"] & 0x7FF
    return out


def port_prims(cap, vbs):
    out = subprocess.run([EXE, cap] + [str(v) for v in vbs], capture_output=True, text=True, cwd=ROOT)
    if out.returncode:
        sys.exit(out.stderr)
    res, cur = {}, None
    for line in out.stdout.splitlines():
        if line.startswith("# vb"):
            cur = int(line.split()[2])
            res[cur] = []
            continue
        f = line.split()
        k = f[0]
        if k == "S":
            x, y, w, h, u, v = map(int, f[1:7])
            res[cur].append(("S", x, y, w, h, u, v, int(f[7], 16) & 0x1FF, int(f[8], 16), int(f[9]), int(f[10]),
                             tuple(map(int, f[11:14])) if f[9] == "0" else (128, 128, 128)))
        elif k == "Q":
            nums = list(map(int, f[1:17]))
            res[cur].append(("Q",) + tuple(nums) + (int(f[17], 16) & 0x1FF, int(f[18], 16), int(f[19]),
                                                    tuple(map(int, f[20:23]))))
        elif k == "T":
            res[cur].append(("T",) + tuple(map(int, f[1:7])) + (tuple(map(int, f[7:10])),))
        elif k == "L":
            res[cur].append(("L",) + tuple(map(int, f[1:5])) + (tuple(map(int, f[5:8])),))
    return res


def hudlike(g):
    if g[0] in ("T", "L"):
        return True
    if g[0] == "S":
        return True
    if g[0] == "Q":
        # screen-space HUD quads use the HUD texture pages (slot 0x14/0x15/5/9 pages at x >= 0x200)
        return False
    return False


def main():
    a = sys.argv[1:]
    verbose = "--verbose" in a
    lags = [int(x) for x in (a[a.index("--lag") + 1] if "--lag" in a else "2,3,4,5,6").split(",")]
    a = [x for i, x in enumerate(a) if x != "--verbose" and not (x == "--lag" or (i > 0 and a[i - 1] == "--lag"))]
    capdir = a[0]
    cap = Cap(capdir)
    gdir = os.path.join(capdir, "gpu")
    vbs = [int(x) for x in a[1:]] or sorted(int(f[:-5]) for f in os.listdir(gdir))
    tot_m = tot_p = 0
    for vb in vbs:
        d = json.load(open(os.path.join(gdir, f"{vb}.json")))
        for fr, prims in sorted(d["frames"].items()):
            fr = int(fr)
            fvb = cap.byvbt.get(fr)
            if fvb is None:
                continue
            g = game_prims(prims)
            gset = {}
            for k in g:
                gset[k] = gset.get(k, 0) + 1
            cands = [fvb - l for l in lags if fvb - l in cap.index]
            mine = port_prims(capdir, cands)
            best = None
            for cv in cands:
                left = dict(gset)
                ok = 0
                for k in mine[cv]:
                    if left.get(k, 0) > 0:
                        left[k] -= 1
                        ok += 1
                if best is None or ok > best[1] or (ok == best[1] and len(mine[cv]) < len(mine[best[0]])):
                    best = (cv, ok, left)
            cv, ok, left = best
            n = len(mine[cv])
            tot_m += ok
            tot_p += n
            print(f"frame {fr} (vb {fvb}) state vb {cv} (lag {fvb - cv}): matched {ok}/{n}")
            if verbose or ok < n:
                tmp = dict(gset)
                for k in mine[cv]:
                    if tmp.get(k, 0) > 0:
                        tmp[k] -= 1
                    else:
                        print("   only port:", k)
                if verbose:
                    for k, c in left.items():
                        if c > 0 and hudlike(k):
                            print("   only game:", k)
    print(f"TOTAL matched {tot_m}/{tot_p}")


if __name__ == "__main__":
    main()

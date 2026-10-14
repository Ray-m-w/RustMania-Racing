"""Session 6 visual check: oracle | port side by sides at the same moment.

    python tools/oracle/visual_sbs.py DIR VB[,VB...] [--crop x0,y0,x1,y1] [--no-unsquash]

DIR holds the oracle shots o_<VB>.png (tools/oracle/visual_shots.py) and the
port's window shots b_<V>.png (mr-bevy --replay-capture ... --fx-shots DIR
V,V,...; take several V at and before each VB). The oracle's VRAM frame has
the 3D squashed 3/4 in x toward the centre by the recomp's widescreen package
(gpu_state ws.squash): undone here (x stretched 4/3 about x = 256, the
central 512 columns kept; the HUD, which the package does not squash, then
looks 4/3 too wide in the oracle half). The port shot is box-scaled to
512 x 240. For each VB the port shot with the smallest mean squared
difference (3D area, rows 40..200) among b_<V>.png, V in [VB-10, VB+2], is
used (the oracle shows a frame a few VBlanks old). Writes DIR/sbs_<VB>.png:
row 1 oracle (unsquashed: compare the 3D) | port at 2x; row 2 the raw oracle
frame (compare the HUD, which the package leaves unsquashed) | port; then a
3x crop of both (default: the bottom-centre box where the player's kart
sits). Prints the chosen V and the difference.
"""
import glob
import os
import re
import sys

from PIL import Image, ImageChops, ImageStat


def unsquash(o):
    o = o.convert("RGB")
    w, h = o.size
    big = o.resize((w * 4 // 3, h), Image.BICUBIC)
    x0 = (big.width - w) // 2
    out = Image.new("RGB", (512, 240))
    out.paste(big.crop((x0, 0, x0 + w, h)), (0, 1))
    return out


def port_img(p):
    return Image.open(p).convert("RGB").resize((512, 240), Image.BOX)


def mse(a, b, box=(0, 40, 512, 200)):
    d = ImageChops.difference(a.crop(box), b.crop(box))
    return sum(v * v for v in ImageStat.Stat(d).rms) / 3


def main():
    a = sys.argv[1:]
    d = a[0]
    vbs = [int(v) for v in a[1].split(",")]
    crop = (176, 120, 336, 238)
    for i, x in enumerate(a):
        if x == "--crop":
            crop = tuple(int(v) for v in a[i + 1].split(","))
    raw = "--no-unsquash" in a
    pre = "d" if "--default" in a else "p"
    have = {}
    for p in glob.glob(os.path.join(d, pre + "_*.png")):
        m = re.search(pre + r"_(\d+)\.png$", p)
        if m:
            have[int(m.group(1))] = p
    for vb in vbs:
        o = Image.open(os.path.join(d, "o_%d.png" % vb))
        o = o.convert("RGB").resize((512, 240)) if raw else unsquash(o)
        cands = [v for v in have if vb - 10 <= v <= vb + 2]
        best = min(((mse(o, port_img(have[v])), v) for v in cands), default=None)
        if best is None:
            print(vb, "no port shot")
            continue
        err, v = best
        b = port_img(have[v])
        S = 2
        top = Image.new("RGB", (512 * S * 2 + 8, 240 * S), (40, 40, 40))
        top.paste(o.resize((512 * S, 240 * S), Image.NEAREST), (0, 0))
        top.paste(b.resize((512 * S, 240 * S), Image.NEAREST), (512 * S + 8, 0))
        x0, y0, x1, y1 = crop
        C = 3
        co = o.crop(crop).resize(((x1 - x0) * C, (y1 - y0) * C), Image.NEAREST)
        cb = b.crop(crop).resize(((x1 - x0) * C, (y1 - y0) * C), Image.NEAREST)
        # HUD row: the raw oracle frame (2D is not squashed) | port
        r = Image.open(os.path.join(d, "o_%d.png" % vb)).convert("RGB")
        raw_o = Image.new("RGB", (512, 240))
        raw_o.paste(r, (0, 1))
        hud = Image.new("RGB", top.size, (40, 40, 40))
        hud.paste(raw_o.resize((512 * S, 240 * S), Image.NEAREST), (0, 0))
        hud.paste(b.resize((512 * S, 240 * S), Image.NEAREST), (512 * S + 8, 0))
        out = Image.new("RGB", (top.width, 2 * top.height + co.height + 16), (40, 40, 40))
        out.paste(top, (0, 0))
        out.paste(hud, (0, top.height + 8))
        out.paste(co, (0, 2 * top.height + 16))
        out.paste(cb, (512 * S + 8, 2 * top.height + 16))
        name = os.path.join(d, ("sbsd_%d.png" if pre == "d" else "sbs_%d.png") % vb)
        out.save(name)
        print(vb, "port VB", v, "rms^2 %.0f" % err, name)


if __name__ == "__main__":
    main()

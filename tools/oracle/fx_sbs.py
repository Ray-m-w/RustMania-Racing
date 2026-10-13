"""Side-by-side images: oracle VRAM captures (fx_shots.py, o_<VB>.png) and
mr-bevy window shots (b_<VB>.png), with 2x crops of chosen boxes.

    python tools/oracle/fx_sbs.py DIR OUT.png VB [VB ...] [--crop x0,y0,x1,y1 ...]

Each row: oracle | Bevy at 512x240 (the Bevy shot box-scaled), then the
crops of both at 2x.
"""
import os
import sys
from PIL import Image

a = sys.argv[1:]
d, out = a[0], a[1]
vbs, crops, i = [], [], 2
while i < len(a):
    if a[i] == "--crop":
        crops.append(tuple(int(v) for v in a[i + 1].split(",")))
        i += 2
    else:
        vbs.append(int(a[i]))
        i += 1
rows = []
for vb in vbs:
    o = Image.open(os.path.join(d, f"o_{vb}.png")).convert("RGB").resize((512, 240), Image.BOX)
    b = Image.open(os.path.join(d, f"b_{vb}.png")).convert("RGB").resize((512, 240), Image.BOX)
    parts = [o, b]
    for (x0, y0, x1, y1) in crops:
        parts.append(o.crop((x0, y0, x1, y1)).resize(((x1 - x0) * 2, (y1 - y0) * 2), Image.NEAREST))
        parts.append(b.crop((x0, y0, x1, y1)).resize(((x1 - x0) * 2, (y1 - y0) * 2), Image.NEAREST))
    w = sum(p.width for p in parts) + 6 * len(parts)
    h = max(p.height for p in parts)
    r = Image.new("RGB", (w, h), (40, 40, 40))
    x = 0
    for p in parts:
        r.paste(p, (x, 0))
        x += p.width + 6
    rows.append(r)
W = max(r.width for r in rows)
img = Image.new("RGB", (W, sum(r.height + 6 for r in rows)), (40, 40, 40))
y = 0
for r in rows:
    img.paste(r, (0, y))
    y += r.height + 6
img.save(out)
print(out, img.size)

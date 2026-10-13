"""Side-by-side of the oracle's VRAM capture and an mr-bevy window shot, with
3x crops of the HUD elements (position, lap, timer).

    python tools/oracle/hud_sbs.py ORACLE.png BEVY.png OUT.png

The oracle capture is the 512-wide display buffer (3D squashed by the
widescreen mod, HUD sprites not); the Bevy shot is scaled to 512x240.
"""
import sys
from PIL import Image

o = Image.open(sys.argv[1]).convert("RGB")
b = Image.open(sys.argv[2]).convert("RGB").resize((512, 240), Image.BOX)
crops = [(200, 10, 320, 50), (10, 190, 90, 230), (200, 195, 312, 225)]
W = 512 * 2 + 12
rows = [Image.new("RGB", (W, 240), (40, 40, 40))]
rows[0].paste(o, (0, 0))
rows[0].paste(b, (524, 0))
for (x0, y0, x1, y1) in crops:
    co = o.crop((x0, y0, x1, y1)).resize(((x1 - x0) * 3, (y1 - y0) * 3), Image.NEAREST)
    cb = b.crop((x0, y0, x1, y1)).resize(((x1 - x0) * 3, (y1 - y0) * 3), Image.NEAREST)
    r = Image.new("RGB", (W, co.height + 6), (40, 40, 40))
    r.paste(co, (0, 0))
    r.paste(cb, (524, 0))
    rows.append(r)
out = Image.new("RGB", (W, sum(r.height for r in rows)), (40, 40, 40))
y = 0
for r in rows:
    out.paste(r, (0, y))
    y += r.height
out.save(sys.argv[3])
print("wrote", sys.argv[3])

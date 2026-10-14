"""Contact sheet of the visual check's grid moment: per course oracle
(unsquashed) | port, VB 300 (countdown, karts at rest), labelled.
    python tools/oracle/visual_sheet.py OUT.png [VB] [--default]"""
import glob, os, re, sys
from PIL import Image, ImageDraw
sys.path.insert(0, os.path.dirname(__file__))
from visual_sbs import unsquash, port_img, mse
D = 'analysis/s6/visual'
COURSES = ['DOCK', 'BOG', 'GHOST', 'HAPPY', 'GALLERY', 'PARK', 'NEW_YORK', 'LONDON', 'GRAVE',
           'ISLAND', 'BASE', 'CAPE', 'OVAL', 'ROCK', 'DISCO', 'SPEEDWAY', 'RALLY', 'GLACIER']
a = sys.argv[1:]
out = a[0]
vb = int(a[1]) if len(a) > 1 and a[1].isdigit() else 300
pre = 'd' if '--default' in a else 'p'
W, H, L = 1030, 240, 18
cols = 3
sheet = Image.new('RGB', (cols * W + (cols - 1) * 10, ((len(COURSES) + cols - 1) // cols) * (H + L + 8)), (30, 30, 30))
dr = ImageDraw.Draw(sheet)
for i, c in enumerate(COURSES):
    o = unsquash(Image.open(os.path.join(D, c, 'o_%d.png' % vb)))
    cands = {}
    for p in glob.glob(os.path.join(D, c, pre + '_*.png')):
        v = int(re.search(r'_(\d+)\.png$', p).group(1))
        if vb - 10 <= v <= vb + 2:
            cands[v] = p
    b = min((port_img(p) for p in cands.values()), key=lambda im: mse(o, im))
    x, y = (i % cols) * (W + 10), (i // cols) * (H + L + 8)
    dr.text((x + 4, y + 2), '%s  VB %d   oracle (3D unsquashed)  |  port (%s)' % (c, vb, 'default view' if pre == 'd' else 'faithful view'), fill=(255, 255, 255))
    sheet.paste(o, (x, y + L))
    sheet.paste(b, (x + 518, y + L))
sheet.save(out)
print(out, sheet.size)

"""Kart record y vs the DAT ground drawn under it (layer-0 objects), every
STEP VBlanks of each course capture: how far a kart origin sits below (+) or
above (-) the visible surface. DAT y grows downward; kart pos >> 5 = DAT units.
    python tools/oracle/visual_burial.py [NAME ...] [--step N]
Needs analysis/dat/<name>.obj (mr-dat) and analysis/s6/visual/_dat/NAME.layers.txt
(mr-bevy --layer-report)."""
import json, os, struct, sys, collections
A = sys.argv[1:]
step = int(A[A.index('--step') + 1]) if '--step' in A else 10
names = [a for a in A if not a.isdigit() and a != '--step'] or sorted(os.listdir('analysis/oracle/courses')) + ['DOCK']
def load_obj(path, flags):
    vs = []; cur = None; tris = []
    for l in open(path):
        if l.startswith('o '):
            cur = l.split()[1]
        elif l.startswith('v '):
            x, y, z = map(float, l.split()[1:4]); vs.append((x, -y, -z))
        elif l.startswith('f '):
            if not cur.startswith('L0_'): continue
            oi = int(cur.split('_')[1])
            idx = [int(t.split('/')[0]) - 1 for t in l.split()[1:]]
            for k in range(1, len(idx) - 1):
                tris.append((oi, flags.get(oi, 0), vs[idx[0]], vs[idx[k]], vs[idx[k + 1]]))
    grid = collections.defaultdict(list)
    for t in tris:
        xs = [p[0] for p in t[2:]]; zs = [p[2] for p in t[2:]]
        for gx in range(int(min(xs)) // 512, int(max(xs)) // 512 + 1):
            for gz in range(int(min(zs)) // 512, int(max(zs)) // 512 + 1):
                grid[(gx, gz)].append(t)
    return grid
def surf(grid, x, z):
    out = []
    for oi, fl, a, b, c in grid.get((int(x) // 512, int(z) // 512), []):
        det = (b[2] - c[2]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[2] - c[2])
        if det == 0: continue
        l1 = ((b[2] - c[2]) * (x - c[0]) + (c[0] - b[0]) * (z - c[2])) / det
        l2 = ((c[2] - a[2]) * (x - c[0]) + (a[0] - c[0]) * (z - c[2])) / det
        l3 = 1 - l1 - l2
        if min(l1, l2, l3) < -1e-6: continue
        # skip steep faces (walls)
        ux, uy, uz = b[0]-a[0], b[1]-a[1], b[2]-a[2]; vx, vy, vz = c[0]-a[0], c[1]-a[1], c[2]-a[2]
        ny = uz*vx - ux*vz; n = ((uy*vz-uz*vy)**2 + ny**2 + (ux*vy-uy*vx)**2) ** .5
        if n == 0 or abs(ny) / n < 0.5: continue
        out.append((l1 * a[1] + l2 * b[1] + l3 * c[1], oi, fl))
    return out
res = {}
for name in names:
    d = 'analysis/oracle/start_probe' if name == 'DOCK' else 'analysis/oracle/courses/' + name
    m = json.load(open(d + '/meta.json'))
    flags = {}
    for l in open('analysis/s6/visual/_dat/%s.layers.txt' % name):
        if l.startswith('#'): continue
        p = l.split(); flags[int(p[0])] = int(p[2], 16)
    grid = load_obj('analysis/dat/%s.obj' % name.lower(), flags)
    size = sum(n for _, n in m['regions']); off = 0
    for a, n in m['regions']:
        if a == 0x800C3000: break
        off += n
    f = open(d + '/frames.bin', 'rb')
    race = m.get('race_vb') or 155
    nk = 6
    vals = []
    for i, vb in enumerate(m['vb']):
        if vb < race + 20 or vb % step: continue
        f.seek(i * size + off); blob = f.read(0xA8 + 6 * 0x504)
        for s in range(nk):
            x, y, z = struct.unpack_from('<iii', blob, 0xA8 + 0x504 * s)
            if x == 0 and z == 0: continue
            X, Y, Z = x / 32, y / 32, z / 32
            ss = surf(grid, X, Z)
            if not ss: continue
            gy, oi, fl = min(ss, key=lambda t: abs(Y - t[0]))
            if abs(Y - gy) > 600: continue
            vals.append((Y - gy, vb, s, oi, fl))
    vals.sort()
    n = len(vals)
    if not n:
        print(name, 'no samples'); continue
    q = lambda p: vals[min(n - 1, int(p * n))][0]
    deep = [v for v in vals if v[0] > 40]
    objs = collections.Counter((v[3], hex(v[4])) for v in deep)
    res[name] = dict(n=n, p50=q(.5), p90=q(.9), p99=q(.99), max=vals[-1][:3], frac_over40=len(deep) / n, objs=objs.most_common(4))
    print('%-9s n=%5d median %6.1f p90 %6.1f p99 %6.1f max %s  >40: %.1f%%  objs %s' % (name, n, q(.5), q(.9), q(.99), vals[-1][:3], 100 * len(deep) / n, objs.most_common(4)), flush=True)
json.dump(res, open('analysis/s6/visual/burial.json', 'w'), default=str, indent=1)

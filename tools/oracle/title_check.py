"""Title screen checks against the oracle (Session 6, T).

    python tools/oracle/title_check.py shots OUTDIR VB[,VB...]
        slot 1 -> BACK TO MAIN MENU (DOWN x4, CROSS) -> the title (course
        0x1E, mode 0x10 from VB ~406); native VRAM screenshot at each VB.
    python tools/oracle/title_check.py text PORT_PNG ORACLE_PNG
        "PRESS START": every opaque pixel of the port's software render
        (mr-bevy test press_start_render, MR_TITLE_DUMP) against the oracle
        screenshot at the best (dx, dy) within +-8 / +-3: exact-match count.
    python tools/oracle/title_check.py capture OUTDIR [END_VB]
        per-VBlank capture of the kart heads, camera record, mode, frame
        counter, fade flags, dt (the orbit test in title.rs reads it).

Holds capture.OracleLock; uses savestate slot 1 read-only (no saves).
"""
import os
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from bridge import Bridge  # noqa: E402
from capture import OracleLock, active_high, warm_up, _run  # noqa: E402
from oracle import Oracle  # noqa: E402

# slot 1 = DOCK's course menu, cursor on 1 PLAYER RACE; BACK TO MAIN MENU is
# the fifth row
STEPS = [[0, ""], [20, "down"], [26, ""], [40, "down"], [46, ""], [60, "down"], [66, ""],
         [80, "down"], [86, ""], [100, "cross"], [106, ""]]
REGIONS = [(0x800C30A8 + k * 0x504, 0x40) for k in range(6)] + [
    (0x800C4F40, 0x70), (0x800B0D44, 4), (0x800B0D90, 4), (0x800B0E88, 4),
    (0x800B0F88, 4), (0x800B0EBC, 4), (0x800B0AEC, 4), (0x800B0FAC, 4), (0x800B0BD0, 4)]


def fwd(p):
    return os.path.abspath(p).replace("\\", "/")


def shots(outdir, vbs):
    os.makedirs(outdir, exist_ok=True)
    with OracleLock():
        o, b = Oracle(), Bridge()
        warm_up(o, b, 1)
        b.arm_route([(s[0], active_high(s[1]), 0x80, 0x80, 0x80, 0x80) for s in STEPS], end_vb=max(vbs) + 20)
        g = b.restore_gen
        o.load_state(1)
        b.wait_restore(g)
        for vb in vbs:
            while b.vb_since_restore < vb:
                time.sleep(0.002)
            o.q("screenshot", path=fwd(os.path.join(outdir, "vb%05d.png" % vb)))
            print(vb, b.vb_since_restore)
        while b.vb_since_restore < max(vbs) + 20:
            time.sleep(0.05)


def text(port_png, oracle_png, squash=0.75):
    """The recomp's widescreen package squashes 2D primitives toward the
    screen centre (gpu_state ws.squash = [3, 4]): port x maps to
    256 + (x - 256) * squash in the oracle shot (1.0 when off)."""
    from PIL import Image
    p = Image.open(port_png).convert("RGBA")
    o = Image.open(oracle_png).convert("RGB")
    pp = p.load()
    op = o.load()
    pts = [(x, y, pp[x, y][:3]) for y in range(p.size[1]) for x in range(p.size[0]) if pp[x, y][3] > 0]
    best = None
    for dy in range(-3, 4):
        for dx in range(-8, 9):
            n = 0
            for x, y, c in pts:
                X, Y = int(round(256 + (x - 256) * squash)) + dx, y + dy
                if 0 <= X < o.size[0] and 0 <= Y < o.size[1]:
                    q = op[X, Y]
                    if all(abs(q[k] - c[k]) <= 8 for k in range(3)):
                        n += 1
            if best is None or n > best[0]:
                best = (n, dx, dy)
    print("PRESS START: %d / %d opaque pixels match (+-8 per channel) at dx %d dy %d" % (best[0], len(pts), best[1], best[2]))


def music(end_vb=1200):
    """The XA sectors the title streams (cdrom_sector_history, subheader
    file / channel, LBA range) from the title's start to END_VB."""
    xa = {}
    with OracleLock():
        o, b = Oracle(), Bridge()
        warm_up(o, b, 1)
        b.arm_route([(s[0], active_high(s[1]), 0x80, 0x80, 0x80, 0x80) for s in STEPS], end_vb=end_vb + 20)
        g = b.restore_gen
        o.load_state(1)
        b.wait_restore(g)
        while b.vb_since_restore < 380:
            time.sleep(0.01)
        o.q("cdrom_sector_history_clear")
        seen = set()
        while b.vb_since_restore < end_vb:
            h = o.q("cdrom_sector_history", count=300)
            for e in h.get("entries", []):
                key = (e.get("seq"), e.get("lba"))
                if key in seen or not e.get("xa_audio_delivered"):
                    continue
                seen.add(key)
                k = "%d/%d" % (e["xa_file"], e["xa_channel"])
                r = xa.setdefault(k, [e["lba"], e["lba"], 0])
                r[0], r[1], r[2] = min(r[0], e["lba"]), max(r[1], e["lba"]), r[2] + 1
            time.sleep(0.2)
    for k, r in sorted(xa.items()):
        print("XA file/channel %s: LBA %d..%d, %d sectors" % (k, r[0], r[1], r[2]))


def capture(outdir, end_vb):
    route = {"name": "title_from_slot1", "slot": 1, "end_vb": end_vb, "steps": STEPS}
    with OracleLock():
        m = _run(route, outdir, regions=REGIONS)
    print("captured vb", m["first_vb"], "..", m["last_vb"], "missing", len(m["missing"]))


if __name__ == "__main__":
    cmd = sys.argv[1]
    if cmd == "shots":
        shots(sys.argv[2], [int(v) for v in sys.argv[3].split(",")])
    elif cmd == "text":
        text(sys.argv[2], sys.argv[3], float(sys.argv[4]) if len(sys.argv) > 4 else 0.75)
    elif cmd == "music":
        music(int(sys.argv[2]) if len(sys.argv) > 2 else 1200)
    elif cmd == "capture":
        capture(sys.argv[2], int(sys.argv[3]) if len(sys.argv) > 3 else 2300)

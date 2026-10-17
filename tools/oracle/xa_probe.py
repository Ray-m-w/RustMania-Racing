"""Session 8 (phase 11): which XA clip plays when, on any menu route.

    python tools/oracle/xa_probe.py STATE "VB:buttons,..." END_VB OUTDIR
        [--ram VB:ADDR:LEN,...] [--shot VB:name,...] [--pokes FILE]

STATE as menu_walk.py (`title`, `slot1`/`slot2` read only, COURSE_menu /
COURSE_grid into scratch slot 11). Per VBlank (bridge regions) it records the
game's XA player block 0x800B0730..0x800B0770 (current track 0x740, queue
0x744/46/48, gallery timer / last character 0x74A/4B, playing bit 0x760),
game mode 0x800B0D46, course 0x800B0FAC, the MEET THE MUPPETS cursor
0x800B0E24/25 and the idle counter 0x800B0AF0; it polls cdrom_sector_history
for the XA subheaders (file / channel / LBA) the drive delivered. Output in
OUTDIR (analysis/, local): frames.bin + meta.json (rows), sectors.json,
ram_<vb>_<addr>.bin, <name>.png (native VRAM screenshot), and a printed
summary: every change of the current XA entry with its VBlank and the first
audio sector's LBA / file / channel. Holds the oracle lock; never writes
slots 1 / 2 or the memory card.
"""
import json
import os
import shutil
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
sys.path.insert(0, HERE)
from bridge import Bridge  # noqa: E402
from capture import OracleLock, active_high, warm_up  # noqa: E402
from oracle import Oracle  # noqa: E402
import goto_course  # noqa: E402

REGIONS = [(0x800B0730, 0x40), (0x800B0D44, 4), (0x800B0FAC, 4), (0x800B0E24, 4), (0x800B0AF0, 4)]


def opt(name):
    return sys.argv[sys.argv.index(name) + 1] if name in sys.argv else ""


def main():
    state, route, end_vb, out = sys.argv[1], sys.argv[2], int(sys.argv[3]), os.path.abspath(sys.argv[4])
    os.makedirs(out, exist_ok=True)
    steps = [(0, 0)]
    for s in filter(None, route.split(",")):
        vb, _, btn = s.partition(":")
        steps.append((int(vb), active_high(btn)))
    rams = []
    for s in filter(None, opt("--ram").split(",")):
        vb, a, n = s.split(":")
        rams.append((int(vb), int(a, 16), int(n, 0)))
    shots = []
    for s in filter(None, opt("--shot").split(",")):
        vb, _, name = s.partition(":")
        shots.append((int(vb), name))
    todo = sorted([(vb, "ram", (a, n)) for vb, a, n in rams] + [(vb, "shot", nm) for vb, nm in shots])
    with OracleLock():
        o, b = Oracle(), Bridge()
        if state in ("slot1", "slot2"):
            slot = int(state[-1])
        elif state == "title":
            slot = goto_course.SCRATCH
            shutil.copyfile(os.path.join(ROOT, "analysis", "states", "title", "title_vb430.pst"),
                            goto_course.slot_path(slot))
        else:
            name, kind = state.rsplit("_", 1)
            slot = goto_course.install(name, kind)
        warm_up(o, b, slot)
        b.set_regions(REGIONS)
        b.arm_route([(vb, m, 0x80, 0x80, 0x80, 0x80) for vb, m in steps], end_vb=end_vb + 30)
        if opt("--pokes"):
            b.arm_pokes([tuple(p) for p in json.load(open(opt("--pokes")))])
        o.q("cdrom_sector_history_clear")
        gen0, seq0 = b.restore_gen, b.seq
        o.load_state(slot)
        b.wait_restore(gen0)
        gen = b.restore_gen
        rows, sectors, hist_seq = {}, {}, -1
        while True:
            for sq, g, vbr, vbt, data in b.slots(seq0):
                seq0 = sq + 1
                if g == gen and vbr not in rows:
                    rows[vbr] = data
            vb = b.vb_since_restore
            while todo and todo[0][0] <= vb:
                at, kind, arg = todo.pop(0)
                if kind == "ram":
                    a, n = arg
                    open(os.path.join(out, "ram_%05d_%08X.bin" % (at, a)), "wb").write(o.read(a, n))
                else:
                    o.q("screenshot", path=os.path.join(out, arg + ".png").replace("\\", "/"))
                print("%5d (%d) %s %s" % (at, vb, kind, arg), flush=True)
            h = o.q("cdrom_sector_history", count=400)
            for e in h.get("entries", []):
                if e["seq"] not in sectors:
                    sectors[e["seq"]] = {k: e.get(k) for k in ("seq", "lba", "frame", "xa_file", "xa_channel",
                                                               "xa_submode", "xa_coding", "xa_audio_delivered")}
                    sectors[e["seq"]]["vb"] = vb
            if vb >= end_vb:
                break
            time.sleep(0.1)
    keys = sorted(rows)
    with open(os.path.join(out, "frames.bin"), "wb") as f:
        for k in keys:
            f.write(rows[k])
    json.dump({"regions": REGIONS, "vb": keys}, open(os.path.join(out, "meta.json"), "w"))
    secs = [sectors[k] for k in sorted(sectors)]
    json.dump(secs, open(os.path.join(out, "sectors.json"), "w"))
    # summary: current-entry changes (0x740) and the delivered audio after them
    size = sum(n for _, n in REGIONS)
    last = None
    for k in keys:
        r = rows[k]
        cur = int.from_bytes(r[0x10:0x12], "little")
        q = [int.from_bytes(r[0x14 + 2 * i:0x16 + 2 * i], "little") for i in range(3)]
        mode = int.from_bytes(r[0x42:0x44], "little")
        key = (cur, r[0x30] & 1)
        if key != last:
            print("vb %5d cur %04X q %s playing %d mode %#x tmr %d last %d cursor %d,%d" % (
                k, cur, " ".join("%04X" % v for v in q), r[0x30] & 1, mode, r[0x1B], r[0x1A],
                r[size - 8], r[size - 7]))
            last = key
    runs = []
    for s in secs:
        if s.get("xa_audio_delivered"):
            if runs and runs[-1]["file"] == s["xa_file"] and runs[-1]["ch"] == s["xa_channel"] and s["lba"] - runs[-1]["last"] < 64:
                runs[-1]["last"] = s["lba"]
                runs[-1]["n"] += 1
                runs[-1]["vb_end"] = s["vb"]
            else:
                runs.append({"file": s["xa_file"], "ch": s["xa_channel"], "first": s["lba"], "last": s["lba"],
                             "n": 1, "vb": s["vb"], "vb_end": s["vb"]})
    for r in runs:
        print("audio file %s ch %s lba %d..%d (%d sectors) poll vb %d..%d" % (
            r["file"], r["ch"], r["first"], r["last"], r["n"], r["vb"], r["vb_end"]))


if __name__ == "__main__":
    main()

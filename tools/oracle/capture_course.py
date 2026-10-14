"""One oracle capture per race course (Session 6, O): race load + ~1 AI lap.

    python tools/oracle/capture_course.py NAME [--end VB] [--no-grid]

Needs NAME's course-menu state (goto_course.py menu NAME). Under
capture.OracleLock: installs analysis/states/courses/NAME_menu.pst in the
scratch slot, restores it (ram0.bin = the course menu, mode 0: the course
loaded, the race not yet), replays CROSS 30 / 120 / 210 (1 PLAYER RACE,
KERMIT, RACER: start_probe.json's route; the race load is the game's own),
then no input to END (player idle on the grid; the AI field races), and
records COURSE_REGIONS every VBlank -> analysis/oracle/courses/NAME/
{frames.bin, meta.json, ram0.bin, grid.png, end.png} (capture.load() reads it).
On the grid (60 VBlanks after the game mode turns 1) it saves the grid
savestate to NAME's slot (goto_course.GRID_SLOTS, else the scratch slot)
and archives it as analysis/states/courses/NAME_grid.pst. meta.json also
gets: race_vb (mode 1), grid_vb, the delivered XA sectors (cdrom_sector_history:
lba, file, channel; first / last VBlank seen), course facts read from RAM.
"""
import json
import os
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, HERE)
import capture  # noqa: E402
import capture_items  # noqa: E402
import goto_course as gc  # noqa: E402
from bridge import Bridge  # noqa: E402
from capture import OracleLock, active_high, warm_up  # noqa: E402
from oracle import Oracle  # noqa: E402


def course_regions():
    """capture.PHYSICS_REGIONS + capture_items' item regions + the particle
    lists of fx_capture (sprite particles 0x800E0C20 64 x 0x3C with their side
    table 0x800D73D0 64 x 8, spark streaks, flat quads) + the course ambient
    list 0x800D75D0 (16 x 0x2C, FUN_8008650C). 28 regions, ~31 KB."""
    regs = []
    for a, n in capture_items.ITEM_REGIONS:
        if a == 0x800E0C20:
            n = 0xF00
        elif a == 0x800D73D0:
            n = 0x200 + 0x2C0
        regs.append((a, n))
    regs += [(0x800B4540, 0x400), (0x800C84B8, 0x680)]
    return regs


COURSE_REGIONS = course_regions()
ROUTE_STEPS = [[0, ""], [30, "cross"], [36, ""], [120, "cross"], [126, ""], [210, "cross"], [216, ""]]
FACTS = {  # address: (size, what)
    0x800B0FAC: (4, "course id / event"),
    0x800B0F12: (1, "world (menu)"),
    0x800B0E38: (1, "cup / world (FUN_8008AB5C)"),
    0x800B0E32: (1, "race type / slot in cup"),
    0x800B0B9A: (1, "weather / race kind (FUN_8008AB5C)"),
    0x800B0DA6: (2, "laps"),
    0x800B0D46: (2, "game mode"),
    0x800B0740: (2, "XA current track"),
    0x800B0738: (2, "XA file index"),
    0x800B0ACA: (1, "ambient list on"),
    0x800B0C58: (1, "track objects (FUN_8007F03C first loop)"),
    0x800B0E00: (1, "0x800B0E00 objects (FUN_800616A0)"),
    0x800B0C68: (1, "0x800B0C68 (FUN_80061F08)"),
    0x800D1C68: (4, "object list ptr"),
    0x800D1C8C: (4, "object list ptr"),
    0x800D1C98: (4, "object list ptr"),
    0x800D1CA4: (4, "object list ptr"),
    0x800B0E28: (2, "karts / players"),
    0x800B07E2: (1, "race flags (& 9: rain)"),
}


def read_facts(o):
    out = {}
    for a, (n, what) in FACTS.items():
        v = int.from_bytes(o.read(a, n), "little")
        out["0x%08X" % a] = [v, what]
    return out


def run(name, end_vb=3400, grid=True):
    outdir = os.path.join(ROOT, "analysis", "oracle", "courses", name)
    os.makedirs(outdir, exist_ok=True)
    course = gc.COURSES[name][0]
    grid_slot = gc.GRID_SLOTS.get(name, gc.SCRATCH)
    route = {"name": "course_" + name.lower(), "slot": gc.SCRATCH, "state": "%s_menu.pst" % name,
             "end_vb": end_vb, "steps": ROUTE_STEPS}
    with OracleLock():
        o, b = Oracle(), Bridge()
        warm_up(o, b, 2)
        gc.install(name, "menu", gc.SCRATCH)
        b.set_regions(COURSE_REGIONS)
        b.arm_route([(s[0], active_high(s[1]), 0x80, 0x80, 0x80, 0x80) for s in ROUTE_STEPS], end_vb=end_vb)
        gen0, seq0 = b.restore_gen, b.seq
        o.q("cdrom_sector_history_clear")
        o.load_state(gc.SCRATCH)
        b.wait_restore(gen0)
        gen = b.restore_gen
        t0 = time.time()
        while b.u32(316) != gen and time.time() - t0 < 5:
            time.sleep(0.005)
        with open(os.path.join(outdir, "ram0.bin"), "wb") as f:
            f.write(b.ram_snapshot(gen))
        facts0 = read_facts(o)
        rows = {}
        race_vb = grid_vb = None
        facts_race = None
        xa = {}
        hist_seq = -1
        last_hist = 0
        while True:
            for sq, g, vbr, vbt, data in b.slots(seq0):
                seq0 = sq + 1
                if g == gen and vbr not in rows:
                    rows[vbr] = data
            vb = b.vb_since_restore
            if race_vb is None and vb > 130 and int.from_bytes(o.read(0x800B0D46, 2), "little") == 1:
                race_vb = vb
            if grid and race_vb is not None and grid_vb is None and vb >= race_vb + 60:
                grid_vb = b.vb_since_restore
                gc.save_state(o, grid_slot)
                gc.archive(grid_slot, name, "grid")
                facts_race = read_facts(o)
                o.q("screenshot", path=os.path.join(outdir, "grid.png").replace("\\", "/"))
            if time.time() - last_hist > 1.0:
                last_hist = time.time()
                h = o.q("cdrom_sector_history", count=300)
                for e in h.get("entries", []):
                    if e["seq"] <= hist_seq or not e.get("xa_audio_delivered"):
                        continue
                    k = "%d/%d" % (e["xa_file"], e["xa_channel"])
                    r = xa.setdefault(k, {"file": e["xa_file"], "channel": e["xa_channel"],
                                          "lba_min": e["lba"], "lba_max": e["lba"], "n": 0,
                                          "frame_first": e["frame"], "vb_first_seen": vb})
                    r["lba_min"] = min(r["lba_min"], e["lba"])
                    r["lba_max"] = max(r["lba_max"], e["lba"])
                    r["n"] += 1
                    r["vb_last_seen"] = vb
                if h.get("entries"):
                    hist_seq = max(hist_seq, max(e["seq"] for e in h["entries"]))
            if rows and max(rows) >= end_vb + 30:
                break
            time.sleep(0.02)
        facts_end = read_facts(o)
        o.q("screenshot", path=os.path.join(outdir, "end.png").replace("\\", "/"))
    keys = sorted(rows)
    with open(os.path.join(outdir, "frames.bin"), "wb") as f:
        for k in keys:
            f.write(rows[k])
    meta = {"route": route, "course": name, "course_id": course,
            "regions": [[a, n] for a, n in COURSE_REGIONS], "vb": keys,
            "restore_gen": gen, "first_vb": keys[0], "last_vb": keys[-1],
            "missing": [v for v in range(keys[0], keys[-1] + 1) if v not in rows],
            "race_vb": race_vb, "grid_vb": grid_vb, "grid_slot": grid_slot if grid else None,
            "grid_state": "analysis/states/courses/%s_grid.pst" % name if grid else None,
            "xa_delivered": sorted(xa.values(), key=lambda r: r["vb_first_seen"]),
            "facts_menu": facts0, "facts_grid": facts_race, "facts_end": facts_end}
    with open(os.path.join(outdir, "meta.json"), "w") as f:
        json.dump(meta, f)
    return meta


if __name__ == "__main__":
    a = sys.argv[1:]
    end = int(a[a.index("--end") + 1]) if "--end" in a else 3400
    m = run(a[0].upper(), end_vb=end, grid="--no-grid" not in a)
    print(json.dumps({k: m[k] for k in ("course", "first_vb", "last_vb", "race_vb", "grid_vb", "grid_slot")}),
          "missing", len(m["missing"]))
    print("xa", json.dumps(m["xa_delivered"]))
    print("facts_end", json.dumps(m["facts_end"]))

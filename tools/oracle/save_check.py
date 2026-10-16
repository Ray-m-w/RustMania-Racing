"""Phase 2 (saves + progression): two short oracle checks.

    python tools/oracle/save_check.py unlock [OUTDIR]
    python tools/oracle/save_check.py league [OUTDIR]

unlock: slot 1 (DOCK's course menu, read only) restored with RAM pokes on the
save block 0x800E0190: DOCK's lap record 3000 (BEST LAP 0:10.00 on the
course menu), every world 0..5 course's flag 8 (the first secret course's
condition), ISLAND (world 4 slot 1) closed and DOCK won; then BACK TO MAIN
MENU (DOWN x4, CROSS). main() runs FUN_8008BE68 between the sessions; the
script writes the poked block (before.bin) and the block once the next course
is loaded (after.bin), the course the game loaded, and shots. The port's
check: mr-sim save::tests::progression_matches_oracle.

league: the archived title state (slot 11) -> START -> TOURNAMENT - 1 PLAYER
-> RACE LEAGUE A (shot) -> BOG's character select (shot) -> CROSS -> the
league menu (START RACE 1 / POINTS / QUIT TOURNAMENT; shots of it and of
POINTS) -> START RACE 1 -> the race;
some 300 VBlanks after GO the player's kart is marked finished (+0x4C7 = 1,
one TCP byte write) -> results -> CROSS -> RACE POINTS (shot + GP0 frame) ->
CROSS -> TOURNAMENT POINTS (shot + GP0 frame). Writes league.txt: per kart its
finishing rank (0x800D8F94), best lap (+0x384), hits (+0x492 / +0x494), the
race's best lap 0x800B0DD0, the best-start mask 0x800B0E8C and the game's
points 0x800C1E08 + 5 * kart (place, lap, start, hits, total) and table
0x800C1E30; the port's check: mr-sim save::tests::league_points_match_oracle.

Holds the oracle lock; never writes slots 1 / 2 (slot 11 is the scratch);
never opens the MEMORY CARD page (the game only writes the card there).
Default OUTDIR analysis/s7/saves (local).
"""
import json
import os
import shutil
import sys
import time

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
sys.path.insert(0, os.path.join(ROOT, "psxrecomp", "tools"))
sys.path.insert(0, os.path.dirname(__file__))
from bridge import Bridge  # noqa: E402
from capture import OracleLock, active_high, warm_up  # noqa: E402
from oracle import Oracle  # noqa: E402
import goto_course  # noqa: E402

BLOCK, LEN = 0x800E0190, 0x20C
KART0, KSIZE = 0x800C30A8, 0x504


def u16(o, a):
    return int.from_bytes(o.read(a, 2), "little")


def shot(o, out, name, gp0=False):
    png = os.path.join(out, name + ".png").replace("\\", "/")
    o.q("screenshot", path=png)
    if gp0:
        import psx_gpu_frame as g
        with g.DebugConn(port=4370) as c:
            d = g.capture(c)
            for _ in range(3):
                if len(d["prims"]) >= 20:
                    break
                d = g.capture(c, frame=d["frame"] - 1)
        json.dump({"name": name, "frame": d["frame"], "prims": d["prims"]}, open(os.path.join(out, name + ".json"), "w"))
    print("shot", name, flush=True)


def wait_vb(b, vb):
    while b.vb_since_restore < vb:
        time.sleep(0.005)


def wait_until(o, b, cond, timeout_vb, what):
    t0 = b.vb_since_restore
    while not cond():
        if b.vb_since_restore - t0 > timeout_vb:
            raise RuntimeError("timeout waiting for " + what)
        time.sleep(0.02)
    return b.vb_since_restore


def press(o, spec, frames=4):
    o.route([(spec, frames), ("", 4)])


def unlock(out):
    with OracleLock():
        o, b = Oracle(), Bridge()
        warm_up(o, b, 1)
        # the block as slot 1 has it
        b.arm_route([(0, 0, 0x80, 0x80, 0x80, 0x80)], end_vb=10)
        g = b.restore_gen
        o.load_state(1)
        b.wait_restore(g)
        before = bytearray(o.read(BLOCK, LEN))
        pokes = []
        def poke(off, v, n):
            pokes.append((BLOCK + off, v, n, 1))
            before[off:off + n] = v.to_bytes(n, "little")
        poke(0x20 * 4 + 0, 3000, 4)  # DOCK (world 4 slot 0) record
        for w in range(6):
            for t in range(4):
                off = 0x20 * w + 8 * t + 6
                v = before[off] | 8
                if (w, t) == (4, 0):
                    v |= 2  # DOCK won
                if (w, t) == (4, 1):
                    v &= ~1  # ISLAND closed
                poke(off, v, 1)
        steps = [(0, 0)]
        vb = 20
        for _ in range(4):
            steps += [(vb, active_high("down")), (vb + 6, 0)]
            vb += 20
        steps += [(100, active_high("cross")), (106, 0)]
        b.arm_route([(v, m, 0x80, 0x80, 0x80, 0x80) for v, m in steps], end_vb=130)
        b.arm_pokes(pokes)
        g = b.restore_gen
        o.load_state(1)
        b.wait_restore(g)
        wait_vb(b, 15)
        shot(o, out, "unlock_coursemenu", gp0=True)
        fac0 = int.from_bytes(o.read(0x800B0FAC, 4), "little")
        at = wait_until(o, b, lambda: int.from_bytes(o.read(0x800B0FAC, 4), "little") != fac0 and u16(o, 0x800B0AF4) == 0xFFFF, 4000, "the next course")
        after = o.read(BLOCK, LEN)
        course = int.from_bytes(o.read(0x800B0FAC, 4), "little")
        changed = o.read(0x800B07DC, 1)[0]
        open(os.path.join(out, "unlock_before.bin"), "wb").write(bytes(before))
        open(os.path.join(out, "unlock_after.bin"), "wb").write(after)
        wait_vb(b, at + 200)
        shot(o, out, "unlock_next")
        diff = [(hex(BLOCK + i), before[i], after[i]) for i in range(LEN) if before[i] != after[i]]
        print("next course %d (0x%X) at VB %d; status-changed flag %d; block diffs %s" % (course, course, at, changed, diff), flush=True)


def league(out):
    with OracleLock():
        o, b = Oracle(), Bridge()
        slot = goto_course.SCRATCH
        shutil.copyfile(os.path.join(ROOT, "analysis", "states", "title", "title_vb430.pst"), goto_course.slot_path(slot))
        warm_up(o, b, slot)
        steps = [(0, 0)]
        def st(vb, spec):
            steps.extend([(vb, active_high(spec)), (vb + 6, 0)])
        st(20, "start")
        for k in range(3):
            st(60 + 20 * k, "down")
        st(130, "cross")
        st(200, "cross")
        b.arm_route([(v, m, 0x80, 0x80, 0x80, 0x80) for v, m in steps], end_vb=230)
        g = b.restore_gen
        o.load_state(slot)
        b.wait_restore(g)
        wait_vb(b, 170)
        shot(o, out, "league_page", gp0=True)
        print("league page: sub-state %d cursor %d" % (u16(o, 0x800B0E8A), o.read(0x800B0E24, 1)[0]), flush=True)
        at = wait_until(o, b, lambda: int.from_bytes(o.read(0x800B0FAC, 4), "little") == 8 and u16(o, 0x800B0AF4) == 0xFFFF, 4000, "BOG")
        wait_vb(b, at + 200)
        print("BOG at VB %d: mode %d, char select %d, league %d" % (at, u16(o, 0x800B0D46), o.read(0x800B07CB, 1)[0], u16(o, 0x800B07E4)), flush=True)
        shot(o, out, "league_charsel")
        press(o, "cross")
        # the league menu (0x800B0EA2 = 4): START RACE n, POINTS, QUIT TOURNAMENT
        at = wait_until(o, b, lambda: u16(o, 0x800B0EA2) == 4 and o.read(0x800B07CB, 1)[0] == 0, 3000, "the league menu")
        wait_vb(b, at + 60)
        shot(o, out, "league_menu", gp0=True)
        press(o, "down")
        wait_vb(b, b.vb_since_restore + 20)
        press(o, "cross")
        at = wait_until(o, b, lambda: u16(o, 0x800B0EA2) == 3, 600, "POINTS")
        wait_vb(b, at + 40)
        shot(o, out, "league_menu_points", gp0=True)
        press(o, "triangle")
        wait_until(o, b, lambda: u16(o, 0x800B0EA2) == 4, 600, "the league menu again")
        wait_vb(b, b.vb_since_restore + 20)
        press(o, "up")
        wait_vb(b, b.vb_since_restore + 20)
        press(o, "cross")
        at = wait_until(o, b, lambda: u16(o, 0x800B0D46) == 1 and int.from_bytes(o.read(0x800B0C70, 4), "little") == 0, 6000, "GO")
        wait_vb(b, at + 300)
        o.q("write_ram", addr="0x%08X" % (KART0 + 0x4C7), val=1)
        at = wait_until(o, b, lambda: u16(o, 0x800B0D46) == 10, 2000, "results")
        wait_vb(b, at + 90)
        shot(o, out, "league_results")
        press(o, "cross")
        at = wait_until(o, b, lambda: u16(o, 0x800B0D46) == 0x11 and u16(o, 0x800B0A64) == 1, 3000, "RACE POINTS")
        wait_vb(b, at + 30)
        shot(o, out, "league_points1", gp0=True)
        n = u16(o, 0x800B0DFE)
        order = [int.from_bytes(o.read(0x800D8F94 + 8 * i, 4), "little") for i in range(n)]
        pts = o.read(0x800C1E08, 5 * 8)
        tot = o.read(0x800C1E00, 8)
        table = o.read(0x800C1E30, 8)
        lines = ["fastest %d" % int.from_bytes(o.read(0x800B0DD0, 4), "little", signed=True),
                 "start_mask %d" % o.read(0x800B0E8C, 1)[0]]
        for k in range(n):
            r = o.read(KART0 + KSIZE * k, KSIZE)
            rank = order.index(k) if k in order else 9
            best = int.from_bytes(r[0x384:0x388], "little", signed=True)
            hits = int.from_bytes(r[0x492:0x494], "little", signed=True)
            taken = int.from_bytes(r[0x494:0x496], "little", signed=True)
            lines.append("kart %d rank %d best %d hits %d taken %d game %s total %d" % (k, rank, best, hits, taken, ",".join(str(x) for x in pts[5 * k:5 * k + 5]), tot[k]))
        lines.append("table %s" % ",".join(str(x) for x in table[:n]))
        lines.append("save " + o.read(BLOCK, LEN).hex())
        open(os.path.join(out, "league.txt"), "w").write("\n".join(lines) + "\n")
        print("\n".join(lines[:-1]), flush=True)
        press(o, "cross")
        at = wait_until(o, b, lambda: u16(o, 0x800B0A64) == 2, 600, "TOURNAMENT POINTS")
        wait_vb(b, at + 30)
        shot(o, out, "league_points2", gp0=True)


if __name__ == "__main__":
    what = sys.argv[1]
    out = sys.argv[2] if len(sys.argv) > 2 else os.path.join(ROOT, "analysis", "s7", "saves")
    os.makedirs(out, exist_ok=True)
    {"unlock": unlock, "league": league}[what](out)

"""AI item use: oracle races vs port races (Session 4, track A).

    python tools/oracle/item_stats.py --oracle O1.csv ... --port P1.csv ...
                                      [--json OUT.json] [--slots 1-5]

Inputs are per-tick, per-kart rows written by `mr-items rows CAPTURE --out`
(the oracle's kart records) and `mr-items race CAPTURE --seed S --out` (a
port race from the same start). Every statistic is derived from the record
fields the same way for both, per AI kart (slots 1..5) over the whole run:

  fires     weapon slot charge 0x40 -> 0xFF (+0x4D4 / +0x4D5)
  boxes     charge 0xFF -> 0..0x3F (an item box filled the slot)
  tokens    turbo meter +0x48A rises outside a special move
  turbo     ticks boosting (+0x4C4) on the meter (no start boost, no special)
  moves     special moves started (+0x4D6 0 -> non-zero, full-meter R1)
  specials  character specials started (0x800B07F4 -1 -> type, owner = kart)
  dealt     hits dealt (+0x492 increments, not at the kart's own special)
  taken     hits taken (+0x494 increments, not when any special starts)
  hit rate  dealt / fires (per race, all AI karts)
  --until-finish: only each kart's race (until +0x4C7 is set).
  "when":   share of fires on each lap; tick of the first special move /
            character special.

Tolerance (from the oracle's own spread): for each statistic the per-race
total over the AI karts; |port mean - oracle mean| <= 2 * sqrt(sd_o^2/n_o +
sd_p^2/n_p) (two standard errors of the difference).
"""
import csv
import json
import math
import sys

FIELDS = ["fires", "boxes", "tokens", "turbo", "moves", "specials", "dealt", "taken"]


def load(path):
    rows = {}
    with open(path) as f:
        for r in csv.DictReader(f):
            rows.setdefault(int(r["slot"]), []).append({k: int(v) for k, v in r.items()})
    for s in rows:
        rows[s].sort(key=lambda r: r["tick"])
    return rows


def race_stats(path, slots, until_finish=False):
    rows = load(path)
    # Special starts per tick (any kart): gspecial -1 -> x.
    g = rows[0]
    special_start = {}
    for a, b in zip(g, g[1:]):
        if a["gspecial"] == -1 and b["gspecial"] != -1:
            special_start[b["tick"]] = b["gowner"]
    out = {}
    for s in slots:
        rs = rows[s]
        st = {k: 0 for k in FIELDS}
        fire_laps = [0, 0, 0, 0]
        first_move = first_special = None
        for a, b in zip(rs, rs[1:]):
            t = b["tick"]
            if until_finish and a["finished"]:
                break
            for c in ("charge0", "charge1"):
                if a[c] == 0x40 and b[c] == 0xFF:
                    st["fires"] += 1
                    fire_laps[min(a["lap"], 3)] += 1
                if a[c] == 0xFF and b[c] < 0x40:
                    st["boxes"] += 1
            if b["meter"] > a["meter"] and b["special"] == 0 and a["special"] == 0:
                st["tokens"] += 1
            if b["boosting"] and b["boost_count"] == 0 and b["special"] == 0 and b["meter"] < a["meter"]:
                st["turbo"] += 1
            if a["special"] == 0 and b["special"] != 0:
                st["moves"] += 1
                if first_move is None:
                    first_move = t
            if special_start.get(t) == s:
                st["specials"] += 1
                if first_special is None:
                    first_special = t
            if b["dealt"] > a["dealt"] and special_start.get(t) != s:
                st["dealt"] += b["dealt"] - a["dealt"]
            if b["taken"] > a["taken"] and t not in special_start:
                st["taken"] += b["taken"] - a["taken"]
        st["fire_laps"] = fire_laps
        st["first_move"] = first_move
        st["first_special"] = first_special
        out[s] = st
    return out


def mean_sd(xs):
    n = len(xs)
    m = sum(xs) / n
    sd = math.sqrt(sum((x - m) ** 2 for x in xs) / (n - 1)) if n > 1 else 0.0
    return m, sd


def summarize(races, slots):
    tot = {k: [sum(r[s][k] for s in slots) for r in races] for k in FIELDS}
    rate = [sum(r[s]["dealt"] for s in slots) / max(1, sum(r[s]["fires"] for s in slots)) for r in races]
    laps = [0, 0, 0, 0]
    for r in races:
        for s in slots:
            for i in range(4):
                laps[i] += r[s]["fire_laps"][i]
    nf = max(1, sum(laps))
    first_move = [r[s]["first_move"] for r in races for s in slots if r[s]["first_move"] is not None]
    first_spec = [r[s]["first_special"] for r in races for s in slots if r[s]["first_special"] is not None]
    per_kart = {s: {k: mean_sd([r[s][k] for r in races]) for k in FIELDS} for s in slots}
    return tot, rate, [x / nf for x in laps], first_move, first_spec, per_kart


def main():
    a = sys.argv
    oi, pi = a.index("--oracle"), a.index("--port")
    ends = [i for i in (oi, pi, a.index("--json") if "--json" in a else len(a), a.index("--until-finish") if "--until-finish" in a else len(a)) if i > 0] + [len(a)]

    def files(i):
        nxt = min(j for j in ends if j > i)
        return [x for x in a[i + 1:nxt] if not x.startswith("--")]

    slots = [1, 2, 3, 4, 5]
    uf = "--until-finish" in a
    orc = [race_stats(p, slots, uf) for p in files(oi)]
    prt = [race_stats(p, slots, uf) for p in files(pi)]
    print("window:", "each kart's race (until it finishes)" if uf else "the whole run (4,815 ticks, past the AI's finish)")
    ot, orate, olaps, omove, ospec, okart = summarize(orc, slots)
    pt, prate, plaps, pmove, pspec, pkart = summarize(prt, slots)
    no, np_ = len(orc), len(prt)
    print(f"oracle races {no}, port races {np_}; per race, all five AI karts together")
    print(f"{'stat':<10} {'oracle mean (sd) [min..max]':>30} {'port mean (sd)':>18} {'diff':>8} {'tol':>7}  ok")
    res = {}
    allok = True
    rows = [(k, ot[k], pt[k]) for k in FIELDS] + [("hit rate", orate, prate)]
    for k, o, p in rows:
        mo, so = mean_sd(o)
        mp, sp = mean_sd(p)
        tol = 2 * math.sqrt(so ** 2 / no + sp ** 2 / np_)
        ok = abs(mp - mo) <= tol
        allok &= ok
        fmt = "{:.2f}" if k == "hit rate" else "{:.1f}"
        print(f"{k:<10} {fmt.format(mo):>10} ({fmt.format(so)}) [{fmt.format(min(o))}..{fmt.format(max(o))}]".ljust(41)
              + f" {fmt.format(mp):>8} ({fmt.format(sp)}) {mp - mo:>+8.2f} {tol:>7.2f}  {'yes' if ok else 'NO'}")
        res[k] = {"oracle": [mo, so, min(o), max(o)], "port": [mp, sp], "diff": mp - mo, "tol": tol, "ok": ok}
    print("fires by lap (share, laps 0..3): oracle", [round(x, 2) for x in olaps], "port", [round(x, 2) for x in plaps])

    def med(xs):
        xs = sorted(xs)
        return xs[len(xs) // 2] if xs else None
    print(f"first special move (tick): oracle median {med(omove)} of {len(omove)} karts, port {med(pmove)} of {len(pmove)}")
    print(f"first character special (tick): oracle median {med(ospec)} of {len(ospec)}, port {med(pspec)} of {len(pspec)}")
    print("per kart (mean per race): oracle / port")
    for s in slots:
        print(f"  kart {s}: " + ", ".join(f"{k} {okart[s][k][0]:.1f}/{pkart[s][k][0]:.1f}" for k in FIELDS))
    print("ALL WITHIN TOLERANCE" if allok else "SOME OUTSIDE TOLERANCE")
    if "--json" in a:
        json.dump({"stats": res, "fire_laps": {"oracle": olaps, "port": plaps}}, open(a[a.index("--json") + 1], "w"), indent=1)


if __name__ == "__main__":
    main()

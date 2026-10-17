"""AI behaviour statistics: oracle captures vs mr-ai free runs (Session 3).

    python tools/oracle/ai_stats.py --oracle DIR [DIR ...] --port CSV [CSV ...]
                                    [--json OUT.json]

Per AI kart (slots 1..5, the race's opponents): lap times (lap 1 includes
the time before the savestate, the same in both), lateral offset from the
TER racing line (+0x3FC, the game's own FUN_8009004C measure, sampled every
tick; as a fraction of the local track width (+0x3F8 - +0x3F4) and in kart
units = TER units * 32), and the finishing order (sum of the three laps).

Oracle captures: tools/oracle/capture.py --physics output (items on) or
capture_noitems.py output (items off by TCP pokes). Port: `mr-ai free
CAPTURE --out CSV` (items off).

Item effects in oracle runs: ticks in a special move (+0x4D6 != 0) or
boosting (+0x4C4 != 0, turbo from the meter) are counted per lap; laps with
any are reported as "item-affected".
"""
import json
import os
import sys

import numpy as np

KART0 = 0x800C30A8
KS = 0x504
LAPS = 3


class Cap:
    def __init__(s, d):
        s.dir = d
        s.meta = json.load(open(os.path.join(d, "meta.json")))
        s.regions = s.meta["regions"]
        s.size = sum(n for _, n in s.regions)
        raw = np.fromfile(os.path.join(d, "frames.bin"), dtype=np.uint8)
        s.vb = np.array(s.meta["vb"])
        s.f = raw.reshape(len(s.vb), s.size)

    def off(s, addr):
        o = 0
        for a, n in s.regions:
            if a <= addr < a + n:
                return o + addr - a
            o += n
        raise KeyError(hex(addr))

    def col(s, addr, n=4, signed=True):
        o = s.off(addr)
        dt = {1: "i1", 2: "<i2", 4: "<i4"}[n]
        if not signed:
            dt = dt.replace("i", "u")
        return s.f[:, o:o + n].copy().view(dt).reshape(-1)

    def k(s, i, off, n=4, signed=True):
        return s.col(KART0 + i * KS + off, n, signed)


def oracle_stats(d):
    c = Cap(d)
    clk = c.col(0x800B0A94)
    ticks = np.array([0] + [j for j in range(1, len(clk)) if clk[j] != clk[j - 1]])
    out = {}
    for i in range(1, 6):
        lap = c.k(i, 0x4BF, 1)[ticks]
        special = c.k(i, 0x4D6, 1)[ticks] != 0
        boost = c.k(i, 0x4C4, 1)[ticks] != 0
        lt = [int(c.k(i, 0x35C + 4 * j)[-1]) for j in range(LAPS)]
        ends = [int(np.argmax(lap >= j + 1)) if (lap >= j + 1).any() else None for j in range(LAPS)]
        affected = []
        start = 0
        for j in range(LAPS):
            e = ends[j]
            if e is None:
                affected.append(None)
                continue
            affected.append({"special": int(special[start:e].sum()), "boost": int(boost[start:e].sum())})
            start = e
        lat = c.k(i, 0x3FC)[ticks].astype(float)
        wl = c.k(i, 0x3F4)[ticks].astype(float)
        wr = c.k(i, 0x3F8)[ticks].astype(float)
        width = wr - wl
        ok = width > 0
        out[i] = {
            "character": int(c.k(i, 0x4B2, 1)[0]), "vehicle": int(c.k(i, 0x4B1, 1)[0]),
            "lap_times": lt if all(e is not None for e in ends) else lt[:sum(e is not None for e in ends)],
            "lap_end_tick": ends, "item_ticks_per_lap": affected,
            "lateral": lat[ok], "lat_frac": lat[ok] / width[ok],
        }
    return out


def port_stats(csv):
    a = np.genfromtxt(csv, delimiter=",", names=True, dtype=np.int64)
    out = {}
    for i in range(1, 6):
        r = a[a["kart"] == i]
        lap = r["lap"]
        ends = [int(np.argmax(lap >= j + 1)) if (lap >= j + 1).any() else None for j in range(LAPS)]
        width = r["width"].astype(float)
        ok = width > 0
        out[i] = {"lap_end_tick": ends, "lateral": r["lateral"][ok].astype(float), "lat_frac": r["lateral"][ok] / width[ok]}
    # lap times from the summary file next to the csv (mr-ai prints them)
    summ = csv.replace(".csv", ".txt")
    if os.path.exists(summ):
        for line in open(summ):
            line = line.strip()
            if line.startswith("kart "):
                k = int(line.split()[1].rstrip(":"))
                out[k]["lap_times"] = json.loads(line.split("lap times ")[1])
    return out


def describe(vals):
    vals = np.asarray(vals, dtype=float)
    if len(vals) == 0:
        return {}
    return {"n": int(len(vals)), "mean": float(vals.mean()), "sd": float(vals.std()), "min": float(vals.min()), "max": float(vals.max())}


def order(stats):
    tot = {k: sum(v["lap_times"]) for k, v in stats.items() if len(v.get("lap_times", [])) == LAPS}
    return [k for k, _ in sorted(tot.items(), key=lambda kv: kv[1])]


def main():
    args = sys.argv[1:]
    ors, ps, js = [], [], None
    cur = None
    for a in args:
        if a == "--oracle":
            cur = ors
        elif a == "--port":
            cur = ps
        elif a == "--json":
            cur = "json"
        elif cur == "json":
            js = a
        else:
            cur.append(a)
    O = [oracle_stats(d) for d in ors]
    P = [port_stats(c) for c in ps]
    report = {"oracle_runs": ors, "port_runs": ps, "karts": {}}
    print("kart  char veh | oracle laps (mean/sd/min/max, n)              | port laps                     | lat|frac| median o/p  p90 o/p")
    for i in range(1, 6):
        ol = [t for s in O for t in s[i]["lap_times"]]
        ol_clean = [t for s in O for t, a in zip(s[i]["lap_times"], s[i]["item_ticks_per_lap"]) if a and a["special"] == 0 and a["boost"] == 0]
        pl = [t for s in P for t in s[i].get("lap_times", [])[:LAPS]]
        olat = np.concatenate([s[i]["lat_frac"] for s in O]) if O else np.array([])
        plat = np.concatenate([s[i]["lat_frac"] for s in P]) if P else np.array([])
        oabs = np.concatenate([s[i]["lateral"] for s in O]) * 32 if O else np.array([])
        pabs = np.concatenate([s[i]["lateral"] for s in P]) * 32 if P else np.array([])
        q = lambda v, p: float(np.percentile(np.abs(v), p)) if len(v) else float("nan")
        d = {
            "character": O[0][i]["character"] if O else None, "vehicle": O[0][i]["vehicle"] if O else None,
            "oracle_laps": describe(ol), "oracle_laps_clean": describe(ol_clean), "port_laps": describe(pl),
            "oracle_lat_frac": {"median_abs": q(olat, 50), "p90_abs": q(olat, 90), "mean": float(olat.mean()) if len(olat) else None},
            "port_lat_frac": {"median_abs": q(plat, 50), "p90_abs": q(plat, 90), "mean": float(plat.mean()) if len(plat) else None},
            "oracle_lat_kart_units": {"median_abs": q(oabs, 50), "p90_abs": q(oabs, 90)},
            "port_lat_kart_units": {"median_abs": q(pabs, 50), "p90_abs": q(pabs, 90)},
            "oracle_items_per_lap": [s[i]["item_ticks_per_lap"] for s in O],
        }
        report["karts"][i] = d
        ol_, pl_ = d["oracle_laps"], d["port_laps"]
        print(f"{i:>4}  {d['character']:>4} {d['vehicle']:>3} | {ol_.get('mean', 0):8.0f} {ol_.get('sd', 0):6.0f} {ol_.get('min', 0):6.0f} {ol_.get('max', 0):6.0f} n{ol_.get('n', 0):<3}"
              f" | {pl_.get('mean', 0):8.0f} {pl_.get('sd', 0):6.0f} n{pl_.get('n', 0):<3}"
              f" | {d['oracle_lat_frac']['median_abs']:.3f}/{d['port_lat_frac']['median_abs']:.3f}  {d['oracle_lat_frac']['p90_abs']:.3f}/{d['port_lat_frac']['p90_abs']:.3f}")
    report["oracle_orders"] = [order(s) for s in O]
    report["port_orders"] = [order(s) for s in P]
    print("finishing order (oracle):", report["oracle_orders"])
    print("finishing order (port):  ", report["port_orders"])

    # Tolerance check (docs/history/notes/SESSION-3-ai.md): per kart mean lap time
    # within max(3 %, 2 standard errors of the difference); all karts pooled
    # within 1.5 %; lateral |offset| / width median and p90 within 0.10;
    # mean finishing position within 1.5 places.
    def mean_pos(orders, k):
        ps = [o.index(k) + 1 for o in orders if k in o]
        return (float(np.mean(ps)), float(np.std(ps)), len(ps)) if ps else (float("nan"), 0.0, 0)
    ok_all = True
    print("tolerance check:")
    for i in range(1, 6):
        d = report["karts"][i]
        o, p = d["oracle_laps"], d["port_laps"]
        if not o or not p:
            continue
        se = (o["sd"] ** 2 / o["n"] + p["sd"] ** 2 / p["n"]) ** 0.5
        lim = max(0.03 * o["mean"], 2 * se)
        dl = p["mean"] - o["mean"]
        dmed = abs(d["port_lat_frac"]["median_abs"] - d["oracle_lat_frac"]["median_abs"])
        dp90 = abs(d["port_lat_frac"]["p90_abs"] - d["oracle_lat_frac"]["p90_abs"])
        mo, mp = mean_pos(report["oracle_orders"], i), mean_pos(report["port_orders"], i)
        ok = abs(dl) <= lim and dmed <= 0.10 and dp90 <= 0.10 and abs(mo[0] - mp[0]) <= 1.5
        ok_all &= ok
        d["check"] = {"lap_delta": dl, "lap_delta_pct": 100 * dl / o["mean"], "lap_limit": lim, "lap_se": se,
                      "lat_median_delta": dmed, "lat_p90_delta": dp90, "pos_oracle": mo, "pos_port": mp, "ok": bool(ok)}
        print(f"  kart {i}: lap {dl:+7.0f} ({100 * dl / o['mean']:+.1f} %, limit {lim:.0f} = {100 * lim / o['mean']:.1f} %)"
              f"  line median {dmed:.3f} p90 {dp90:.3f}  position {mo[0]:.2f} vs {mp[0]:.2f}  {'OK' if ok else 'FAIL'}")
    ol = [t for s in O for i in range(1, 6) for t in s[i]["lap_times"]]
    pl = [t for s in P for i in range(1, 6) for t in s[i].get("lap_times", [])[:LAPS]]
    if ol and pl:
        dpool = 100 * (np.mean(pl) - np.mean(ol)) / np.mean(ol)
        se = 100 * (np.var(ol) / len(ol) + np.var(pl) / len(pl)) ** 0.5 / np.mean(ol)
        report["pooled"] = {"oracle": describe(ol), "port": describe(pl), "delta_pct": float(dpool), "se_pct": float(se)}
        ok_all &= abs(dpool) <= 1.5
        print(f"  pooled: oracle {np.mean(ol):.0f} (sd {np.std(ol):.0f}, n {len(ol)})  port {np.mean(pl):.0f} (sd {np.std(pl):.0f}, n {len(pl)})"
              f"  delta {dpool:+.2f} % (se {se:.2f} %)  {'OK' if abs(dpool) <= 1.5 else 'FAIL'}")
    report["ok"] = bool(ok_all)
    print("overall:", "PASS" if ok_all else "FAIL")
    if js:
        with open(js, "w") as f:
            json.dump(report, f, indent=1)


if __name__ == "__main__":
    main()

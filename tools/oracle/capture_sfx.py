"""Capture a route's sound: SPU key-on events, per-VBlank sound RAM, SPU output.

    python tools/oracle/capture_sfx.py ROUTE.json OUTDIR [--wav-at VB]

Runs ROUTE from its savestate (slot 2 = DOCK grid) like capture.py --physics,
holding capture.OracleLock for the whole run, and additionally:
- resets the SPU event ring (TCP spu_events_reset) before the load and polls
  spu_events during the run -> OUT/spu_events.json (every KEYON / KEYOFF /
  END_* with the runtime frame and the voice registers at that moment);
- samples (runtime frame, bridge VBlanks since restore, spu_out tap frames)
  pairs while polling -> OUT/clock.json, to map runtime frames and SPU output
  samples onto VBlanks since the restore;
- at VBlank --wav-at (default 5000) dumps the whole spu_out ring (~95 s) to
  OUT/spu_out.wav (TCP audio_wav) and records the tap head at that moment.
Regions: capture.PHYSICS_REGIONS (they hold the game's sound channel table
0x800D8AB0, libsnd's voice table 0x800BC100 and voice shadow 0x800D8B70) plus
libsnd's per-voice dirty flags 0x800C1E50.
Decoded audio and captures stay in analysis/ (never committed).
"""
import json
import os
import sys
import time

sys.path.insert(0, os.path.dirname(__file__))
import capture  # noqa: E402
from capture import PHYSICS_REGIONS, OracleLock, warm_up  # noqa: E402
from oracle import Oracle  # noqa: E402
from bridge import Bridge  # noqa: E402

SFX_REGIONS = PHYSICS_REGIONS + [(0x800C1E50, 0x20)]


def run(route, outdir, wav_at=5000):
    os.makedirs(outdir, exist_ok=True)
    with OracleLock():
        o = Oracle()
        b = Bridge()
        slot = route.get("slot", 2)
        warm_up(o, b, slot)
        b.set_regions(SFX_REGIONS)
        steps = []
        for st in route["steps"]:
            vb, spec = st[0], st[1]
            lx = st[2] if len(st) > 2 else 0x80
            ly = st[3] if len(st) > 3 else 0x80
            steps.append((vb, capture.active_high(spec), lx, ly, 0x80, 0x80))
        end_vb = int(route.get("end_vb", 1200))
        b.arm_route(steps, end_vb=end_vb)
        pokes = [tuple(p) for p in route.get("pokes", [])]
        if pokes:
            b.arm_pokes(pokes)
        gen0 = b.restore_gen
        seq0 = b.seq
        o.q("spu_events_reset")
        o.load_state(slot)
        b.wait_restore(gen0)
        gen = b.restore_gen
        t0 = time.time()
        while b.u32(316) != gen and time.time() - t0 < 5:
            time.sleep(0.005)
        try:
            with open(os.path.join(outdir, "ram0.bin"), "wb") as f:
                f.write(b.ram_snapshot(gen))
        except RuntimeError:
            pass
        rows, events, clock = {}, {}, []
        wav = None
        last_poll = 0.0
        while True:
            for sq, g, vbr, vbt, data in b.slots(seq0):
                seq0 = sq + 1
                if g == gen and vbr not in rows:
                    rows[vbr] = data
            now = time.time()
            if now - last_poll > 0.25:
                last_poll = now
                v0 = b.vb_since_restore
                fr = o.frame()
                st = o.q("audio_stats")
                v1 = b.vb_since_restore
                clock.append({"vb0": v0, "vb1": v1, "frame": fr,
                              "spu_frames": st["taps"][0]["frames"]})
                r = o.q("spu_events", count=4096)
                for e in r["events"]:
                    events[e["seq"]] = e
                if wav is None and v1 >= wav_at:
                    p = os.path.abspath(os.path.join(outdir, "spu_out.wav"))
                    w = o.q("audio_wav", tap=0, path=p, start=-1, count=0)
                    st2 = o.q("audio_stats")
                    wav = {"reply": w, "vb": b.vb_since_restore,
                           "head_after": st2["taps"][0]["frames"]}
            if rows and max(rows) >= end_vb + 30:
                break
            time.sleep(0.01)
        r = o.q("spu_events", count=4096)
        for e in r["events"]:
            events[e["seq"]] = e
    keys = sorted(rows)
    with open(os.path.join(outdir, "frames.bin"), "wb") as f:
        for k in keys:
            f.write(rows[k])
    meta = {"route": route, "regions": [[a, n] for a, n in SFX_REGIONS], "vb": keys,
            "restore_gen": gen, "first_vb": keys[0], "last_vb": keys[-1],
            "missing": [v for v in range(keys[0], keys[-1] + 1) if v not in rows],
            "wav": wav}
    with open(os.path.join(outdir, "meta.json"), "w") as f:
        json.dump(meta, f)
    with open(os.path.join(outdir, "clock.json"), "w") as f:
        json.dump(clock, f)
    ev = [events[k] for k in sorted(events)]
    with open(os.path.join(outdir, "spu_events.json"), "w") as f:
        json.dump(ev, f)
    seqs = sorted(events)
    gaps = [(a, b2) for a, b2 in zip(seqs, seqs[1:]) if b2 != a + 1]
    return meta, len(ev), gaps


if __name__ == "__main__":
    route = json.load(open(sys.argv[1]))
    wav_at = 5000
    if "--wav-at" in sys.argv:
        wav_at = int(sys.argv[sys.argv.index("--wav-at") + 1])
    m, n, gaps = run(route, sys.argv[2], wav_at)
    print("captured vb", m["first_vb"], "..", m["last_vb"], "missing", len(m["missing"]),
          "events", n, "seq gaps", gaps[:5], "wav", m["wav"])

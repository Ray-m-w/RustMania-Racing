"""Probe the oracle's race music: which XA file/channel/LBA plays when.

    python tools/audio/music_probe.py OUTDIR [--phase load|race|both]

Holds capture.OracleLock for the whole run. Phase "load": savestate slot 1
(DOCK course menu) + routes/start_game.json (race load, countdown, GO).
Phase "race": slot 2 (DOCK grid) + routes/race3.json (3 laps, finish ~VB 7043,
results), then START at VB 9000 and CROSS presses to see what the results /
restart do to the music. Per VBlank (bridge regions) it records the game's XA
player state (0x800B0730..0x800B0770: current LBA 0x734, file index 0x738,
current track 0x740, queue 0x744/46/48, track byte 0x74D, loop flag 0x758,
playing bit 0x760), game mode 0x800B0D46, course 0x800B0FAC and the CdMix
levels 0x800B0A50 / 0x800B0E70. It polls cdrom_sector_history (raw XA
subheaders) and dumps the cd_in audio tap (decoded XA at 44.1 kHz after CdMix,
before the SPU CD volume) to WAV. Outputs stay in OUTDIR (analysis/, local).
"""
import json
import os
import struct
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "oracle"))
import capture  # noqa: E402
from bridge import Bridge  # noqa: E402
from oracle import Oracle  # noqa: E402

REGIONS = [(0x800B0730, 0x40), (0x800B0D44, 4), (0x800B0FAC, 4), (0x800B0A50, 4), (0x800B0E70, 4)]
ROUTES = os.path.join(HERE, "..", "oracle", "routes")


def steps_of(route):
    out = []
    for st in route["steps"]:
        vb, spec = st[0], st[1]
        lx = st[2] if len(st) > 2 else 0x80
        ly = st[3] if len(st) > 3 else 0x80
        out.append((vb, capture.active_high(spec), lx, ly, 0x80, 0x80))
    return out


def run_phase(o, b, name, slot, route, end_vb, outdir, wav_every_vb=4800):
    b.set_regions(REGIONS)
    steps = steps_of(route)
    b.arm_route(steps, end_vb=end_vb)
    o.q("cdrom_sector_history_clear")
    gen0 = b.restore_gen
    seq0 = b.seq
    tap0 = o.q("audio_stats")["taps"][1]["frames"]
    o.load_state(slot)
    b.wait_restore(gen0)
    gen = b.restore_gen
    rows = {}
    sectors = {}
    taps = []
    wavs = []
    last_wav_tap = tap0
    last_wav_vb = 0
    hist_seq = -1
    while True:
        for sq, g, vbr, vbt, data in b.slots(seq0):
            seq0 = sq + 1
            if g == gen and vbr not in rows:
                rows[vbr] = data
        vb = b.vb_since_restore
        h = o.q("cdrom_sector_history", count=400)
        for e in h.get("entries", []):
            if e["seq"] > hist_seq - 400 and e["seq"] not in sectors:
                e2 = {k: e[k] for k in ("seq", "lba", "frame", "mode", "xa_file", "xa_channel",
                                         "xa_submode", "xa_coding", "data_delivered",
                                         "xa_audio_delivered", "skip_reason")}
                e2["head"] = e["hex"][:16]
                e2["vb"] = vb
                sectors[e["seq"]] = e2
        if h.get("entries"):
            hist_seq = max(hist_seq, h["entries"][0]["seq"])
        st = o.q("audio_stats")
        tap = st["taps"][1]["frames"]
        taps.append((vb, tap))
        if vb - last_wav_vb >= wav_every_vb or vb >= end_vb + 30:
            p = os.path.abspath(os.path.join(outdir, "%s_cdin_%05d.wav" % (name, vb)))
            r = o.q("audio_wav", tap=1, path=p, start=str(last_wav_tap), count=str(tap - last_wav_tap))
            wavs.append({"path": p, "vb_from": last_wav_vb, "vb_to": vb, "tap_from": last_wav_tap,
                         "tap_to": tap, "reply": r})
            last_wav_tap, last_wav_vb = tap, vb
        if vb >= end_vb + 30:
            break
        time.sleep(0.25)
    keys = sorted(rows)
    with open(os.path.join(outdir, name + "_frames.bin"), "wb") as f:
        for k in keys:
            f.write(rows[k])
    meta = {"name": name, "slot": slot, "regions": REGIONS, "vb": keys, "taps": taps,
            "tap0": tap0, "wavs": wavs, "spu": o.q("spu_status")}
    json.dump(meta, open(os.path.join(outdir, name + "_meta.json"), "w"))
    json.dump([sectors[k] for k in sorted(sectors)], open(os.path.join(outdir, name + "_sectors.json"), "w"))
    print(name, "vb", keys[0], "..", keys[-1], "sectors", len(sectors), "wavs", len(wavs))


def main():
    outdir = sys.argv[1]
    phase = sys.argv[sys.argv.index("--phase") + 1] if "--phase" in sys.argv else "both"
    os.makedirs(outdir, exist_ok=True)
    with capture.OracleLock():
        o = Oracle()
        b = Bridge()
        capture.warm_up(o, b, 2)
        if phase in ("load", "both"):
            r = json.load(open(os.path.join(ROUTES, "start_game.json")))
            run_phase(o, b, "load", 1, r, 1300, outdir)
        if phase in ("race", "both"):
            r = json.load(open(os.path.join(ROUTES, "race3.json")))
            extra = [[9000, "start"], [9006, ""], [9600, "cross"], [9606, ""],
                     [10200, "cross"], [10206, ""], [10800, "start"], [10806, ""]]
            r = dict(r, steps=r["steps"] + extra)
            run_phase(o, b, "race", 2, r, 11600, outdir)


if __name__ == "__main__":
    main()

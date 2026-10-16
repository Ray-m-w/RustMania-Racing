"""Phase 9: one movie in the oracle, its picture read from VRAM (fmv.md).

    python tools/oracle/fmv_check.py OUTDIR [MOVIE] [COURSE_STATE] [VRAM_X]

MOVIE is only recorded (the course's own movie plays: BOG = 0, MM1_15).

Installs analysis/states/courses/<COURSE_STATE>_menu.pst (default BOG)
into the scratch slot 11 (never 1 or 2) under the oracle lock, loads it,
reads the course rows' "movie shown" bits (0x800ACB3A + 0xC * c & 0x800),
sets the course's movie-object bit (0x80) in RAM and plays the movie with
COURSE SECRETS -> PLAY MOVIE (the game's own input) and, while it plays,
reads the frame number 0x800E7BAC and the picture (vram_peek 128 x 128 at
(VRAM_X, 0) and (VRAM_X, 240): 85 pixels of the 24-bit picture (480
halfwords a line) in both buffers)
a few times. The oracle runs with the skip-fmv package on (build-dev
mods/state.toml): the runtime presses the game's skip input as soon as it
sees a movie, so only its first ~13 frames play, most of them in the skip
fade (0x800F8DB0, recorded per read). Writes OUTDIR/vram_<n>_<y>.bin (raw halfwords) + log.json for
`mr-str vram`. Nothing is saved (no savestate, no memory card write).
"""
import json
import os
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import goto_course as gc  # noqa: E402
from capture import OracleLock  # noqa: E402
from oracle import Oracle  # noqa: E402


def main():
    out = os.path.abspath(sys.argv[1])
    movie = int(sys.argv[2], 0) if len(sys.argv) > 2 else 0
    state = sys.argv[3].upper() if len(sys.argv) > 3 else "BOG"
    vx = int(sys.argv[4], 0) if len(sys.argv) > 4 else 0
    os.makedirs(out, exist_ok=True)
    log = {"movie": movie, "state": state, "reads": []}
    with OracleLock():
        o = Oracle()
        gc.install(state, "menu", gc.SCRATCH)
        o.load_state(gc.SCRATCH)
        time.sleep(0.5)
        course = int.from_bytes(o.read(0x800B0FAC, 4), "little")
        rows = o.read(0x800ACB34, 12 * 37)
        log["course"] = course
        log["shown_bits"] = [c for c in range(37) if int.from_bytes(rows[12 * c + 6:12 * c + 8], "little") & 0x800]
        print("course", course, "rows with 0x800:", log["shown_bits"])
        # the course's save flags (cup 0 slot 0 = BOG: 0x800E0196) get the
        # movie object's bit 0x80; COURSE SECRETS then offers PLAY MOVIE
        # (its only open row: the cursor starts on it); CROSS plays it.
        _, world, slot, _ = gc.COURSES[state]
        fl = 0x800E0194 + 2 + 8 * slot + 0x20 * world
        o.q("write_ram", addr="0x%08X" % fl, val=o.read(fl, 1)[0] | 0x80)
        # (DOWN x3 on the secrets page reaches PLAY MOVIE past the "?" rows)
        o.route([("", 10), ("down", 4), ("", 10), ("down", 4), ("", 10), ("cross", 4), ("", 60),
                 ("down", 4), ("", 10), ("down", 4), ("", 10), ("down", 4), ("", 10), ("cross", 4), ("", 30)])
        t0 = time.time()
        got = 0
        last = None
        while time.time() - t0 < 20 and got < 8:
            f = int.from_bytes(o.read(0x800E7BAC, 4), "little", signed=True)
            if 1 <= f < 400 and f != last:
                last = f
                rec = {"frame_before": f, "level": int.from_bytes(o.read(0x800F8DB0, 4), "little")}
                for y in (0, 240):
                    r = o.q("vram_peek", x=vx, y=y, w=128, h=128)
                    hx = r.get("hex") or r.get("data")
                    if hx is None:
                        rec["error"] = {k: v for k, v in r.items() if k != "pixels"}
                        px = r.get("pixels")
                        if px is not None:
                            data = b"".join(int(p).to_bytes(2, "little") for p in px)
                        else:
                            print("vram_peek reply keys:", list(r.keys()))
                            continue
                    else:
                        # the reply's hex is one big-endian 4-digit value per
                        # pixel; store little-endian halfwords (VRAM order)
                        be = bytes.fromhex(hx)
                        data = b"".join(be[i + 1:i + 2] + be[i:i + 1] for i in range(0, len(be), 2))
                    p = os.path.join(out, "vram_%d_%d.bin" % (got, y))
                    open(p, "wb").write(data)
                rec["frame_after"] = int.from_bytes(o.read(0x800E7BAC, 4), "little", signed=True)
                log["reads"].append(rec)
                print(rec, flush=True)
                got += 1
            time.sleep(0.005)
    json.dump(log, open(os.path.join(out, "log.json"), "w"), indent=1)


if __name__ == "__main__":
    main()

"""Independent items-on oracle races for the AI item-use tolerance (Session 4,
track A): the DOCK grid savestate (slot 2) with the player idle, the BIOS
rand() seed (0x80009010) and the random byte pointer (0x800B0BCC) poked to
new values at the restore (bridge v4 pokes), so every race draws its own
random streams. Item regions of capture_items.py.

    python tools/oracle/capture_seeds.py OUTDIR_PREFIX N [--first K] [--vb 9600]

Writes OUTDIR_PREFIX<k> for k = first .. first + N - 1. The pointer stays in
the game's own window 0x8001F94C..0x8001FF28 (FUN_8007D304's wrap).
"""
import os
import sys

sys.path.insert(0, os.path.dirname(__file__))
import capture  # noqa: E402
import capture_items  # noqa: E402

RNG_LO, RNG_HI = 0x8001F94C, 0x8001FF28


def seed_for(k):
    x = (k * 0x9E3779B9 + 0x7F4A7C15) & 0xFFFFFFFF
    for _ in range(3):
        x ^= x >> 15
        x = (x * 0x2C1B3C6D) & 0xFFFFFFFF
    return x


def main():
    prefix, n = sys.argv[1], int(sys.argv[2])
    first = int(sys.argv[sys.argv.index("--first") + 1]) if "--first" in sys.argv else 1
    vb = int(sys.argv[sys.argv.index("--vb") + 1]) if "--vb" in sys.argv else 9600
    for k in range(first, first + n):
        s = seed_for(k)
        ptr = RNG_LO + (s >> 8) % (RNG_HI - RNG_LO)
        route = {"name": "items-seed-%d" % k, "slot": 2, "end_vb": vb, "steps": [[0, ""]],
                 "pokes": [[0x80009010, s, 4, 1], [0x800B0BCC, ptr, 4, 1]],
                 "note": "player idle; rand seed %08X, pointer %08X poked at the restore" % (s, ptr)}
        m = capture.run(route, prefix + str(k), regions=capture_items.ITEM_REGIONS)
        print(prefix + str(k), "seed %08X ptr %08X" % (s, ptr), "vb", m["first_vb"], "..", m["last_vb"], "missing", len(m["missing"]), flush=True)


if __name__ == "__main__":
    main()

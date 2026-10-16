"""A course grid savestate with RAM pokes at the restore, player idle, the
course regions every VBlank (Session 6, track S: put an AI kart where a port
run got stuck and see what the game does there).

    python tools/oracle/capture_poked.py SLOT OUTDIR END_VB POKES.json

POKES.json: [[addr, value, size, when], ...] (when 1 = at the restore; bridge
v4 pokes, at most 64). The slot is only loaded (never saved). OUTDIR gets
frames.bin / meta.json / ram0.bin as capture.py (ram0 = the restored state).
"""
import json
import os
import sys

sys.path.insert(0, os.path.dirname(__file__))
import capture  # noqa: E402
import capture_course  # noqa: E402


def main():
    slot, outdir, end_vb, pokes = int(sys.argv[1]), sys.argv[2], int(sys.argv[3]), json.load(open(sys.argv[4]))
    if slot in (1, 2):
        pass  # loading is fine; only saving is forbidden (this script never saves)
    # Session 7 (stunt trial): --hold SPEC holds SPEC (e.g. "cross") from
    # VBlank 0; --save adds the save block 0x800E0190 (0x140 bytes).
    hold = sys.argv[sys.argv.index("--hold") + 1] if "--hold" in sys.argv else ""
    regions = list(capture_course.COURSE_REGIONS)
    if "--save" in sys.argv:
        regions.append((0x800E0190, 0x140))
    route = {"name": "poked-slot-%d" % slot, "slot": slot, "end_vb": end_vb, "steps": [[0, hold]],
             "pokes": pokes, "note": ("holding %s" % hold if hold else "player idle") + "; pokes at the restore: " + sys.argv[4]}
    m = capture.run(route, outdir, regions=regions)
    print(outdir, "vb", m["first_vb"], "..", m["last_vb"], "missing", len(m["missing"]), flush=True)


if __name__ == "__main__":
    main()

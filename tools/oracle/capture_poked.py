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
    route = {"name": "poked-slot-%d" % slot, "slot": slot, "end_vb": end_vb, "steps": [[0, ""]],
             "pokes": pokes, "note": "player idle; pokes at the restore: " + sys.argv[4]}
    m = capture.run(route, outdir, regions=capture_course.COURSE_REGIONS)
    print(outdir, "vb", m["first_vb"], "..", m["last_vb"], "missing", len(m["missing"]), flush=True)


if __name__ == "__main__":
    main()

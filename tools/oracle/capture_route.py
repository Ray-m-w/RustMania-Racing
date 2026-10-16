"""Run one input route from a savestate slot and capture the course regions
(capture_course.COURSE_REGIONS + the karts' history rings: all kart records, the cameras, the globals,
the item / particle / object state) every VBlank, for
`mr-course-check --dir OUTDIR COURSE` (free run and teacher forced, all
karts and the camera).

    python tools/oracle/capture_route.py ROUTE.json OUTDIR

ROUTE.json as capture.py ({"slot": N, "end_vb": ..., "steps": [...]}); the
slot is only loaded, never saved. OUTDIR gets frames.bin / meta.json /
ram0.bin (ram0 = the restored state).
"""
import json
import os
import sys

sys.path.insert(0, os.path.dirname(__file__))
import capture  # noqa: E402
import capture_course  # noqa: E402


# The karts' position history rings (kart +0x2DC: 0x800D4098 + 0xC0 * slot,
# 12 x 16 bytes; the chase camera walks them).
HISTORY = (0x800D4098, 6 * 0xC0)
REGIONS = [r for r in capture_course.COURSE_REGIONS if not (HISTORY[0] <= r[0] and r[0] + r[1] <= HISTORY[0] + HISTORY[1])] + [HISTORY]


def main():
    route, outdir = json.load(open(sys.argv[1])), sys.argv[2]
    m = capture.run(route, outdir, regions=REGIONS)
    print(outdir, "vb", m["first_vb"], "..", m["last_vb"], "missing", len(m["missing"]), flush=True)


if __name__ == "__main__":
    main()

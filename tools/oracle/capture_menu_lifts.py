"""A course's course-menu state -> the race load, with the lift records and
the frame counter every VBlank (Session 6, track S: the course captures do
not hold the lifts; this shows the menu stepping them).

    python tools/oracle/capture_menu_lifts.py NAME OUTDIR [END_VB]

Installs analysis/states/courses/NAME_menu.pst into the scratch slot 11
(never 1 or 2) under the oracle lock, then replays capture_course's route
(1 PLAYER RACE, KERMIT, RACER; player idle). Regions: lifts 0x800DD708
(6 x 20 bytes), mode, countdown, race clock, turntables, rand() seed,
random pointer, frame counter / dt.
"""
import os
import sys

sys.path.insert(0, os.path.dirname(__file__))
import capture  # noqa: E402
import capture_course as cc  # noqa: E402
import goto_course as gc  # noqa: E402

REGIONS = [(0x800DD708, 0x78), (0x800B0D44, 4), (0x800B0C70, 4), (0x800B0A94, 4), (0x800BC648, 0x20),
           (0x80009010, 4), (0x800B0BCC, 4), (0x800B0D90, 4), (0x800B0AEC, 4)]


def main():
    name, out = sys.argv[1].upper(), sys.argv[2]
    end = int(sys.argv[3]) if len(sys.argv) > 3 else 400
    with capture.OracleLock():
        gc.install(name, "menu", gc.SCRATCH)
        route = {"name": "menu_lifts_" + name.lower(), "slot": gc.SCRATCH, "end_vb": end, "steps": cc.ROUTE_STEPS}
        m = capture._run(route, out, regions=REGIONS)
    print(out, "vb", m["first_vb"], "..", m["last_vb"], "missing", len(m["missing"]))


if __name__ == "__main__":
    main()

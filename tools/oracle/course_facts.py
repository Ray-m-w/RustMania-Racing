"""One coordination-table row per course capture (Session 6, O).

    python tools/oracle/course_facts.py NAME [NAME ...]

Reads analysis/oracle/courses/NAME/meta.json (capture_course.py) and the
exe's course table (analysis/disc/SLUS_012.37, 0x800ACB34 rows: +3 music
track, +4 intro clip). Music file/channel: track t = TUNES/MUSIC(t / 8 + 1).XA
channel t % 8 (DOCK 16 = MUSIC3 ch 0 [ORACLE, Session 4]); the delivered XA
sectors seen during the capture (subheader file / channel, LBA range) are
printed beside it as the oracle's evidence.
"""
import json
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, HERE)
import goto_course as gc  # noqa: E402

EXE = os.path.join(ROOT, "analysis", "disc", "SLUS_012.37")


def row(name):
    d = os.path.join(ROOT, "analysis", "oracle", "courses", name)
    m = json.load(open(os.path.join(d, "meta.json")))
    e = open(EXE, "rb").read()
    cid = m["course_id"]
    off = 0x800ACB34 + 12 * cid - 0x80010000 + 0x800
    track = struct.unpack_from("b", e, off + 3)[0]
    clip = struct.unpack_from("<h", e, off + 4)[0]
    f = m["facts_end"]
    g = lambda a: f["0x%08X" % a][0]
    music = "track %d = MUSIC%d.XA ch %d" % (track, track // 8 + 1, track % 8) if track >= 0 else "none"
    xa = ", ".join("f%d/ch%d LBA %d-%d (VB %d-%d)" % (x["file"], x["channel"], x["lba_min"], x["lba_max"],
                                                      x["vb_first_seen"], x.get("vb_last_seen", 0))
                   for x in m["xa_delivered"])
    objs = []
    for a in (0x800D1C68, 0x800D1C8C, 0x800D1C98, 0x800D1CA4):
        if g(a):
            objs.append("0x%08X=0x%08X" % (a, g(a)))
    for a in (0x800B0C58, 0x800B0E00, 0x800B0C68):
        if g(a):
            objs.append("0x%08X=%d" % (a, g(a)))
    if g(0x800B0ACA):
        objs.append("ambient list on (0x800B0ACA)")
    if g(0x800B0B9A) == 2 or (f.get("0x800B07E2", [0])[0] & 9):
        objs.append("rain")
    slot = m["grid_slot"] if m["grid_slot"] != gc.SCRATCH else "archive"
    return ("| %s | %d | %s | %s | analysis/oracle/courses/%s (VB %d-%d, race mode 1 at VB %d, ram0 = course menu) "
            "| %d | %s; intro clip 0x%X | %s | %s |" % (
                name, cid, gc.COURSES[name][3], "%s (%s)" % (slot, m["grid_state"]), name,
                m["first_vb"], m["last_vb"], m["race_vb"] or -1, g(0x800B0DA6), music, clip & 0xFFFF, xa,
                "; ".join(objs) or "none"))


if __name__ == "__main__":
    print("| course | id | menu | grid slot (state) | capture | laps | music (table) | XA seen | objects |")
    print("|---|---|---|---|---|---|---|---|---|")
    for n in sys.argv[1:]:
        print(row(n.upper()))

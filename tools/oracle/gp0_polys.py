"""Session 6 R: a course_gp0.py JSON as plain lines for mr-scenery --gp0:
one textured polygon per line: tpage clut n u0 v0 .. r0 g0 b0 .. x0 y0 .."""
import json
import sys

d = json.load(open(sys.argv[1]))
with open(sys.argv[2], "w") as f:
    for q in d["prims"]:
        if q.get("kind") != "poly" or not q.get("textured") or q.get("tpage") is None:
            continue
        ox, oy = q["state"]["offset"]
        n = len(q["verts"])
        a = [q["tpage"] & 0x7FF, q["clut"], n]
        a += [c for uv in q["uvs"] for c in uv]
        a += [c for col in q["colors"] for c in col]
        a += [c for (x, y) in q["verts"] for c in (x - ox, y - oy)]
        f.write(" ".join(str(int(x)) for x in a) + "\n")

"""Session 6 visual check, port side: replay a course capture in mr-bevy and
take window shots at and before each moment.

    python tools/oracle/visual_port.py NAME OUTDIR VB[,VB...] [--default] [-- extra mr-bevy args]

Runs port/target/release/mr-bevy.exe --course NAME --replay-capture
analysis/oracle/courses/NAME (DOCK: analysis/oracle/start_probe)
--faithful-view (or, with --default, the default view at 1280x960) --no-audio
--fx-shots with targets VB-10, VB-8, ..., VB; each shot is renamed to the
VBlank it landed on: OUTDIR/p_<V>.png (--default: d_<V>.png).
"""
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
EXE = os.path.join(ROOT, "port", "target", "release", "mr-bevy.exe")


def run(name, outdir, vbs, default=False, extra=()):
    os.makedirs(outdir, exist_ok=True)
    cap = "analysis/oracle/start_probe" if name == "DOCK" else "analysis/oracle/courses/" + name
    targets = sorted({v - k for v in vbs for k in range(0, 11, 2)})
    view = ["--resolution", "1280x960"] if default else ["--faithful-view"]
    cmd = [EXE, "--course", name, "--replay-capture", cap, *view, "--no-audio",
           "--fx-shots", outdir, ",".join(map(str, targets)), "--fx-speed", "8", *extra]
    r = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True, timeout=900)
    pre = "d" if default else "p"
    landed = []
    for t, v in re.findall(r"fx-shot target (\d+) vblank (\d+)", r.stderr):
        src = os.path.join(outdir, "b_%s.png" % t)
        dst = os.path.join(outdir, "%s_%s.png" % (pre, v))
        if os.path.exists(src):
            os.replace(src, dst)
            landed.append(int(v))
    print(name, "landed", landed)
    if not landed:
        print(r.stderr[-2000:])
    return landed


if __name__ == "__main__":
    a = sys.argv[1:]
    extra = a[a.index("--") + 1:] if "--" in a else []
    a = a[:a.index("--")] if "--" in a else a
    run(a[0].upper(), a[1], [int(v) for v in a[2].split(",")], "--default" in a, extra)

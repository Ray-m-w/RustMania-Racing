"""Summarize a music_probe.py phase: XA player state changes per VBlank and
the XA audio sectors the drive delivered (file/channel/LBA runs).
    python tools/audio/probe_report.py OUTDIR PHASE
"""
import json
import struct
import sys

d, name = sys.argv[1], sys.argv[2]
m = json.load(open("%s/%s_meta.json" % (d, name)))
fr = open("%s/%s_frames.bin" % (d, name), "rb").read()
n = sum(r[1] for r in m["regions"])
prev = None
print("VB: lba file_idx cur_track queue[3] trk loop play mode course levels")
for i, vb in enumerate(m["vb"]):
    row = fr[i * n:(i + 1) * n]
    s = row[:0x40]
    lba, fidx = struct.unpack_from("<iI", s, 4)
    cur, q0, q1, q2 = struct.unpack_from("<hhhh", s, 0x10)
    key = (lba, fidx, cur & 0xFFFF, q0 & 0xFFFF, q1 & 0xFFFF, q2 & 0xFFFF, s[0x1D], s[0x28],
           struct.unpack_from("<H", s, 0x30)[0], struct.unpack_from("<h", row, 0x42)[0], row[0x44], row[0x48], row[0x4C])
    if key != prev:
        print(vb, "lba %d fidx %d cur %04x q %04x %04x %04x trk %d loop %d play %x mode %d course %d lvl %d %d" % key)
    prev = key
sec = json.load(open("%s/%s_sectors.json" % (d, name)))
print("XA audio delivered runs (poll VB, frame, first lba .. last lba, file, ch, coding):")
run = None
for e in sec:
    if not e["xa_audio_delivered"]:
        continue
    k = (e["xa_file"], e["xa_channel"], e["xa_coding"])
    if run and run[0] == k and e["lba"] - run[3] <= 16 and e["lba"] > run[3]:
        run[3] = e["lba"]
        run[4] += 1
        continue
    if run:
        print("  vb %5d frame %6d lba %d..%d %s sectors %d" % (run[1], run[5], run[2], run[3], run[0], run[4]))
    run = [k, e["vb"], e["lba"], e["lba"], 1, e["frame"]]
if run:
    print("  vb %5d frame %6d lba %d..%d %s sectors %d" % (run[1], run[5], run[2], run[3], run[0], run[4]))
print("spu", {k: m["spu"].get(k) for k in ("main_l", "main_r", "cd_l", "cd_r", "reverb_on")})
for w in m["wavs"]:
    print("wav", w["path"], w["vb_from"], w["vb_to"], w["reply"].get("frames"))

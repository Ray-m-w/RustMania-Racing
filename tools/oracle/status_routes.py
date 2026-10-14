"""Routes that make the five kart status shapes appear on the player's kart
(Session 6, track H).

    python tools/oracle/status_routes.py OUTDIR [END_VB]

FUN_80037B80 draws shapes for effect types 0, 0xB, 0xC, 0xE, 0x15 (+0x4F1
of a victim kart). A character special (R2 with a full meter, FUN_8006B828)
gives every other kart effect = the shooter's character table byte
0x800AC231 + 0x14 * c, which is c for c = 0..24. On the slot-2 DOCK grid
the AI field is out of sight ahead of the player, so the player is made the
victim (always in view): AI kart 1 (character 21 on this grid) shoots.
Pokes (bridge v4, at restore and every VBlank):
- kart 1: turbo meter +0x48A = 0x1A4 (full), flags +0x496 = 5 (AI + R2
  preference, FUN_80090914), character +0x4B2 = the wanted type (only the
  R2 type and cosmetic reads use it: voice, impostor colours, HUD icons;
  the model is loaded at race load);
- the player: flags +0x496 = 3 (bit 0 makes every AI kart "behind kart 0"
  in FUN_80090914, the condition for an AI special).
The AI fires R2 at once and again whenever the previous special ends (300
VBlanks). The drive is race3's (slot 2). First attempt (the player as
shooter, R2 at VB 150): the five AI victims were out of view; kept as
--player-shoots for the record.
Writes OUTDIR/status_<type>.json for tools/oracle/fx_capture.py.
"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
KART0 = 0x800C30A8
KART = 0x504
args = [a for a in sys.argv[1:] if not a.startswith("--")]
out = args[0]
end_vb = int(args[1]) if len(args) > 1 else 330
player_shoots = "--player-shoots" in sys.argv
base = json.load(open(os.path.join(HERE, "routes", "race3.json")))
os.makedirs(out, exist_ok=True)


def with_r2(steps, at, n=4):
    """Insert R2 into the held buttons for [at, at + n)."""
    def held(vb):
        cur = ""
        for s in steps:
            if s[0] <= vb:
                cur = s[1]
        return cur
    keep = [s for s in steps if s[0] < at or s[0] >= at + n]
    add = [[at, "+".join(x for x in (held(at), "r2") if x)], [at + n, held(at + n)]]
    inner = [[s[0], "+".join(x for x in (s[1], "r2") if x)] for s in steps if at < s[0] < at + n]
    return sorted(keep + add + inner, key=lambda s: s[0])


for typ in (0, 0xB, 0xC, 0xE, 0x15):
    steps = [s for s in base["steps"] if s[0] <= end_vb + 60]
    if player_shoots:
        pokes = [[KART0 + 0x48A, 0x1A4, 2, 3]] + ([[KART0 + 0x4B2, typ, 1, 3]] if typ else [])
        steps = with_r2(steps, 150)
        note = f"player shoots at VB 150 as character {typ}"
    else:
        k1 = KART0 + KART
        pokes = [[k1 + 0x48A, 0x1A4, 2, 3], [k1 + 0x496, 5, 2, 3], [k1 + 0x4B2, typ, 1, 3],
                 [KART0 + 0x496, 3, 2, 3]]
        note = f"AI kart 1 shoots as character {typ}; the player is the victim"
    r = {"name": f"status_{typ:x}", "slot": 2, "end_vb": end_vb,
         "note": f"race3 drive; {note} (effect type 0x{typ:X})", "steps": steps, "pokes": pokes}
    p = os.path.join(out, f"status_{typ:x}.json")
    json.dump(r, open(p, "w"))
    print(p)

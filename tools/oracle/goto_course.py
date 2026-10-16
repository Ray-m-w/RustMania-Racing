"""Reach any race course in the oracle by the game's own menus (Session 6, O).

    python tools/oracle/goto_course.py menu NAME      # slot 1 -> NAME's course menu,
                                                      # saved to analysis/states/courses/NAME_menu.pst
    python tools/oracle/goto_course.py install NAME menu|grid [SLOT]
                                                      # copy an archived state into a slot (default 11)
    python tools/oracle/goto_course.py list

Route (from savestate slot 1 = DOCK's course menu, cursor on 1 PLAYER RACE;
VBlanks counted from the restore by the oracle bridge):
  DOWN x3 (20/40/60) -> GAME MENU, CROSS 80 -> CHANGE COURSE (remembered
  cursor), CROSS 120 -> the world list (cursor on MUPPET TREASURE ISLAND = 4),
  UP/DOWN to the world from 160 every 20 VBlanks, CROSS, DOWN x type in the
  course list, CROSS -> the game loads the course (0x800B0AF4 -> 0x800B0FAC
  in main) and shows its course menu (1 PLAYER RACE - n LAPS).
The menu's confirm (FUN_80074A94 case 2) takes the course from the cup table
0x800AC1E4[cursor + world * 4] and needs bit 0 of the unlock word
0x800E0194 + world * 0x20 + type * 8 + 2. Pokes (bridge v4, documented per
course in COURSES):
  - SECRET COURSES (world 6, "?" entries on this save): the unlock bytes of
    world 6 are set to 0x61 (0x60 | 1) at the restore; the game then shows
    "GAME STATUS CHANGED - SAVE GAME" in the course menu.
  - worlds 7 and 8 are not in the world list (it ends at SECRET COURSES):
    0x800B0F12 (the selected world, written by the world list confirm) is
    held at 7 / 8 every VBlank of this route; their unlock words are 0xFF
    already. The course list then lists that world and the confirm is the
    game's own.
The race load itself (1 PLAYER RACE, Kermit / RACER) is never poked: see
capture_course.py (CROSS 30 / 120 / 210 from the course menu, as
start_probe.json from slot 1).

Savestates: the runtime has 12 slots (0..11). Slots 1 and 2 are never
written. Grid states of the first eight courses live in slots 3..10
(GRID_SLOTS); every state is also archived in analysis/states/courses/
(local, never committed) and `install` copies one into a slot (11 =
scratch) to load it.
"""
import json
import os
import shutil
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, HERE)
from bridge import Bridge  # noqa: E402
from capture import OracleLock, active_high, warm_up  # noqa: E402
from oracle import Oracle  # noqa: E402

SAVES = os.path.join(ROOT, "saves", "scph1001")
ARCHIVE = os.path.join(ROOT, "analysis", "states", "courses")
ROUTES = os.path.join(HERE, "routes")
SCRATCH = 11
PROTECTED = (1, 2)

# name: (course id 0x800B0FAC, world 0x800B0F12, type = course-list cursor,
#        menu label). Order = menu distance from DOCK (world 4).
COURSES = {
    "ISLAND":   (6, 4, 1, "MUPPET TREASURE ISLAND / ISLAND - RACE B"),
    "LONDON":   (1, 3, 0, "THE MUPPET CHRISTMAS CAROL / RACE A"),
    "GRAVE":    (7, 3, 1, "THE MUPPET CHRISTMAS CAROL / RACE B"),
    "BASE":     (15, 5, 0, "MUPPETS FROM SPACE / RACE A"),
    "CAPE":     (18, 5, 1, "MUPPETS FROM SPACE / RACE B"),
    "PARK":     (16, 2, 0, "THE MUPPETS TAKE MANHATTAN / RACE A"),
    "NEW_YORK": (17, 2, 1, "THE MUPPETS TAKE MANHATTAN / RACE B"),
    "HAPPY":    (9, 1, 0, "THE GREAT MUPPET CAPER / RACE A"),
    "GALLERY":  (4, 1, 1, "THE GREAT MUPPET CAPER / RACE B"),
    "BOG":      (8, 0, 0, "THE MUPPET MOVIE / SWAMP - RACE A"),
    "GHOST":    (2, 0, 1, "THE MUPPET MOVIE / GHOST TOWN"),
    "OVAL":     (26, 6, 0, "SECRET COURSES / STUDIO - RACE"),
    "ROCK":     (28, 6, 2, "SECRET COURSES / FRAGGLE ROCK"),
    "DISCO":    (31, 6, 3, "SECRET COURSES / DISCO"),
    "SPEEDWAY": (0, 7, 0, "(world 7, poked) / SPEEDWAY - RACE"),
    "RALLY":    (32, 7, 1, "(world 7, poked) / RALLY"),
    "GLACIER":  (33, 7, 2, "(world 7, poked) / GLACIER"),
    "VOLCANO":  (34, 8, 0, "(world 8, poked) / VOLCANO"),
    "HOUSE":    (35, 8, 1, "(world 8, poked) / HAUNTED HOUSE"),
    "WHATEVER": (36, 8, 2, "(world 8, poked) / WHATEVER"),
    # Session 7, stunt trial (race type 3): world 4's stunt course.
    "MOUNTAIN": (12, 4, 3, "MUPPET TREASURE ISLAND / STUNT"),
}
GRID_SLOTS = {n: 3 + i for i, n in enumerate(list(COURSES)[:8])}

A_WORLD = 0x800B0F12
A_UNLOCK = 0x800E0194


def press(steps, vb, spec, n=6):
    steps.append([vb, spec])
    steps.append([vb + n, ""])


def menu_route(name):
    """The route from slot 1 to NAME's course menu: {"steps", "pokes", "end_vb"}."""
    course, world, typ, _ = COURSES[name]
    steps = [[0, ""]]
    for vb in (20, 40, 60):
        press(steps, vb, "down")
    press(steps, 80, "cross")
    press(steps, 120, "cross")
    vb = 160
    pokes = []
    if world <= 6:
        for _ in range(abs(world - 4)):
            press(steps, vb, "down" if world > 4 else "up")
            vb += 20
    else:
        pokes.append([A_WORLD, world, 1, 2])
    if world == 6:
        pokes += [[A_UNLOCK + 6 * 0x20 + t * 8 + 2, 0x61, 1, 1] for t in range(4)]
    vb += 10
    press(steps, vb, "cross")
    vb += 50
    for _ in range(typ):
        press(steps, vb, "down")
        vb += 20
    vb += 10
    press(steps, vb, "cross")
    return {"name": "menu_" + name.lower(), "slot": 1, "course": course, "world": world,
            "type": typ, "end_vb": vb + 20, "steps": steps, "pokes": pokes}


def slot_path(slot, ext="pst"):
    return os.path.join(SAVES, "state_80093214_slot%02d.%s" % (slot, ext))


def save_state(o, slot, timeout=30):
    if slot in PROTECTED:
        raise RuntimeError("never write slots 1 or 2")
    gen0 = o.q("savestate_status")["generation"]
    r = o.q("savestate", op="save", slot=slot)
    if not r.get("ok"):
        raise RuntimeError(r)
    t = time.time()
    while time.time() - t < timeout:
        s = o.q("savestate_status")
        if s["generation"] != gen0 and not s["pending"]:
            if not s["last_ok"]:
                raise RuntimeError("save failed: %s" % s)
            return s
        time.sleep(0.05)
    raise TimeoutError("savestate save")


def archive(slot, name, kind):
    os.makedirs(ARCHIVE, exist_ok=True)
    dst = os.path.join(ARCHIVE, "%s_%s.pst" % (name, kind))
    shutil.copyfile(slot_path(slot), dst)
    return dst


def install(name, kind, slot=SCRATCH):
    if slot in PROTECTED:
        raise RuntimeError("never write slots 1 or 2")
    src = os.path.join(ARCHIVE, "%s_%s.pst" % (name, kind))
    shutil.copyfile(src, slot_path(slot))
    for ext in ("thumb", "hdres"):
        p = slot_path(slot, ext)
        if os.path.exists(p):
            os.remove(p)
    return slot


def read16(o, a):
    return int.from_bytes(o.read(a, 2), "little")


def goto_menu(name, shot=None, settle=150, timeout_vb=4000):
    """Slot 1 -> NAME's course menu (the game's own load), then save it to
    the scratch slot and archive it. Holds the oracle lock."""
    route = menu_route(name)
    course = route["course"]
    with OracleLock():
        o, b = Oracle(), Bridge()
        warm_up(o, b, 1)
        steps = [(s[0], active_high(s[1]), 0x80, 0x80, 0x80, 0x80) for s in route["steps"]]
        b.arm_route(steps, end_vb=route["end_vb"])
        b.arm_pokes([tuple(p) for p in route["pokes"]])
        g = b.restore_gen
        o.load_state(1)
        b.wait_restore(g)
        loaded = None
        while True:
            vb = b.vb_since_restore
            if vb > timeout_vb:
                raise RuntimeError("%s: course %d not loaded by VBlank %d" % (name, course, vb))
            if vb > route["end_vb"]:
                fac = int.from_bytes(o.read(0x800B0FAC, 4), "little")
                af4 = read16(o, 0x800B0AF4)
                if fac == course and af4 == 0xFFFF:
                    loaded = vb
                    break
            time.sleep(0.05)
        while b.vb_since_restore < loaded + settle:
            time.sleep(0.02)
        if shot:
            o.q("screenshot", path=os.path.abspath(shot).replace("\\", "/"))
        save_vb = b.vb_since_restore
        save_state(o, SCRATCH)
        path = archive(SCRATCH, name, "menu")
        mode = read16(o, 0x800B0D46)
    rec = dict(route, loaded_vb=loaded, saved_vb=save_vb, mode=mode, state=os.path.relpath(path, ROOT))
    os.makedirs(ROUTES, exist_ok=True)
    with open(os.path.join(ROUTES, "menu_%s.json" % name.lower()), "w") as f:
        json.dump({k: rec[k] for k in ("name", "slot", "course", "world", "type", "end_vb",
                                       "steps", "pokes", "loaded_vb", "saved_vb")}
                  | {"note": COURSES[name][3] + "; from slot 1 (DOCK course menu); "
                     "course loaded (0x800B0FAC == course, 0x800B0AF4 == -1) at loaded_vb, "
                     "state saved ~150 VBlanks later (tools/oracle/goto_course.py)"}, f, indent=1)
    return rec


if __name__ == "__main__":
    a = sys.argv[1:]
    if not a or a[0] == "list":
        for n, (c, w, t, lab) in COURSES.items():
            print("%-9s course %2d world %d type %d slot %s  %s" % (n, c, w, t, GRID_SLOTS.get(n, "-"), lab))
    elif a[0] == "menu":
        r = goto_menu(a[1].upper(), shot=a[2] if len(a) > 2 else None)
        print(json.dumps({k: r[k] for k in ("course", "loaded_vb", "saved_vb", "mode", "state")}))
    elif a[0] == "install":
        print("installed in slot", install(a[1].upper(), a[2], int(a[3]) if len(a) > 3 else SCRATCH))

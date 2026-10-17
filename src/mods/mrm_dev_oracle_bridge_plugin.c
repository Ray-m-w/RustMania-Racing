/* Oracle bridge (development only): the Rust port's window into the recomp.
 *
 * docs/history/notes/SESSION-2-oracle.md. Disabled unless the dev package
 * mrm.dev.oracle-bridge is enabled (mods/state.toml beside the exe); release
 * packaging excludes it (tools/build.ps1).
 *
 * One named shared-memory block, "Local\mrm_oracle_bridge_v4":
 *   - a copy of main RAM + scratchpad taken in the restore callback;
 *   - capture: every VBlank, the RAM regions listed in the header are copied
 *     into a ring slot stamped with the VBlank count since the last savestate
 *     restore (the anchor that makes runs comparable after a load);
 *   - replay: a route written into the header and armed by the client starts
 *     at the next savestate restore and REPLACES player 1's pad (pad
 *     transform) while active, keyed on the same VBlank count. No TCP timing is
 *     involved, so a route lands on the same guest frames in every run;
 *   - pokes (v4): up to MAX_POKES RAM writes armed by the client with the
 *     route. Each is applied in the restore callback (before the RAM
 *     snapshot) and/or held at every VBlank until the next restore. Used to
 *     switch game features off for a capture (e.g. AI weapons and the turbo
 *     meter: hold the karts' +0x4D4 charge timers at 0xFFFF and +0x48A at 0).
 * Layout is mirrored in tools/oracle/bridge.py; bump BRIDGE_VERSION on change.
 */
#include "mod_plugins.h"

#include <stddef.h>
#include <string.h>
#include <windows.h>

#define BRIDGE_NAME "Local\\mrm_oracle_bridge_v4"
#define BRIDGE_MAGIC 0x424D524Du /* 'MRMB' */
#define BRIDGE_VERSION 4u
#define HEADER_SIZE 4096u
#define RAMSNAP_SIZE (0x200000u + 0x400u) /* main RAM + scratchpad */
#define NSLOTS 4096u
#define SLOT_SIZE 32768u
#define SLOT_HDR 24u
#define MAX_REGIONS 32u
#define MAX_STEPS 8192u
#define STEPS_SIZE (MAX_STEPS * 12u) /* step table after the header */
#define MAX_POKES 64u
#define POKE_AT_RESTORE 1u
#define POKE_EVERY_VBLANK 2u

#pragma pack(push, 1)
typedef struct {
    uint32_t addr, len;
} BridgeRegion;
typedef struct {
    uint32_t vb_start;        /* VBlanks since restore at which this step begins */
    uint32_t buttons;         /* active-high PS1 button mask */
    uint8_t lx, ly, rx, ry;   /* 0x80 = centred */
} BridgeStep;
typedef struct {
    uint32_t addr;            /* guest address */
    uint32_t value;           /* little-endian value, low `size` bytes */
    uint8_t size;             /* 1, 2 or 4 */
    uint8_t when;             /* POKE_AT_RESTORE | POKE_EVERY_VBLANK */
    uint16_t pad;
} BridgePoke;
typedef struct {
    uint32_t magic, version, nslots, slot_size; /* 0 */
    uint64_t write_seq;                         /* 16: slots written so far */
    uint32_t restore_gen;                       /* 24 */
    uint32_t nregions;                          /* 28 */
    BridgeRegion regions[MAX_REGIONS];          /* 32..288 */
    uint32_t route_armed;                       /* 288: client sets 1 */
    uint32_t route_len;                         /* 292 */
    uint32_t route_active;                      /* 296 */
    uint32_t route_end_vb;                      /* 300: route releases here */
    uint32_t vb_since_restore;                  /* 304 */
    uint32_t vb_total;                          /* 308 */
    uint32_t source_calls;                      /* 312 */
    uint32_t ramsnap_gen;                       /* 316: restore_gen of the RAM snapshot */
    uint32_t pokes_armed;                       /* 320: client sets the count; taken at restore */
    uint32_t pokes_active;                      /* 324: count in force since the last restore */
    uint8_t reserved[512 - 328];
    BridgePoke pokes[MAX_POKES];                /* 512..1280 */
    uint8_t reserved2[HEADER_SIZE - 512 - MAX_POKES * 12u];
} BridgeHeader;
#pragma pack(pop)
/* The client (tools/oracle/bridge.py) hard-codes these offsets. */
typedef char bridge_header_is_4096[sizeof(BridgeHeader) == HEADER_SIZE ? 1 : -1];
typedef char bridge_pokes_at_512[offsetof(BridgeHeader, pokes) == 512 ? 1 : -1];

static BridgeHeader* g_hdr;
static uint8_t* g_slots;
static uint8_t* g_ramsnap;
static BridgeStep* g_steps;

static void bridge_open(void) {
    HANDLE h;
    void* p;
    if (g_hdr) return;
    h = CreateFileMappingA(INVALID_HANDLE_VALUE, NULL, PAGE_READWRITE, 0,
                           HEADER_SIZE + STEPS_SIZE + RAMSNAP_SIZE + NSLOTS * SLOT_SIZE, BRIDGE_NAME);
    if (!h) return;
    p = MapViewOfFile(h, FILE_MAP_ALL_ACCESS, 0, 0, 0);
    if (!p) return;
    g_hdr = (BridgeHeader*)p;
    g_steps = (BridgeStep*)((uint8_t*)p + HEADER_SIZE);
    g_ramsnap = (uint8_t*)p + HEADER_SIZE + STEPS_SIZE;
    g_slots = g_ramsnap + RAMSNAP_SIZE;
    if (g_hdr->magic != BRIDGE_MAGIC || g_hdr->version != BRIDGE_VERSION) {
        memset(g_hdr, 0, HEADER_SIZE);
        g_hdr->nslots = NSLOTS;
        g_hdr->slot_size = SLOT_SIZE;
        g_hdr->version = BRIDGE_VERSION;
        g_hdr->magic = BRIDGE_MAGIC;
    }
}

static void bridge_capture(void) {
    uint8_t* slot = g_slots + (size_t)(g_hdr->write_seq % NSLOTS) * SLOT_SIZE;
    uint32_t n = g_hdr->nregions < MAX_REGIONS ? g_hdr->nregions : MAX_REGIONS;
    uint32_t off = SLOT_HDR;
    uint32_t i, k;
    memcpy(slot, (const void*)&g_hdr->write_seq, 8);
    memcpy(slot + 8, &g_hdr->restore_gen, 4);
    memcpy(slot + 12, &g_hdr->vb_since_restore, 4);
    memcpy(slot + 16, &g_hdr->vb_total, 4);
    memset(slot + 20, 0, 4);
    for (i = 0; i < n; i++) {
        uint32_t a = g_hdr->regions[i].addr, len = g_hdr->regions[i].len;
        if (off + len > SLOT_SIZE) break;
        for (k = 0; k + 4 <= len; k += 4) {
            uint32_t w = psx_mod_read_word(a + k);
            memcpy(slot + off + k, &w, 4);
        }
        for (; k < len; k++) slot[off + k] = psx_mod_read_byte(a + k);
        off += len;
    }
    /* publish after the slot is complete */
    MemoryBarrier();
    g_hdr->write_seq++;
}

static void bridge_apply_pokes(uint32_t when) {
    uint32_t i, n = g_hdr->pokes_active < MAX_POKES ? g_hdr->pokes_active : MAX_POKES;
    for (i = 0; i < n; i++) {
        const BridgePoke* p = &g_hdr->pokes[i];
        if (!(p->when & when)) continue;
        if (p->size == 1) psx_mod_write_byte(p->addr, (uint8_t)p->value);
        else if (p->size == 2) psx_mod_write_half(p->addr, (uint16_t)p->value);
        else if (p->size == 4) psx_mod_write_word(p->addr, p->value);
    }
}

static void bridge_vblank(void) {
    bridge_open();
    if (!g_hdr) return;
    g_hdr->vb_total++;
    g_hdr->vb_since_restore++;
    bridge_apply_pokes(POKE_EVERY_VBLANK);
    bridge_capture();
}

static void bridge_restored(void) {
    bridge_open();
    if (!g_hdr) return;
    g_hdr->restore_gen++;
    g_hdr->vb_since_restore = 0;
    /* Pokes armed with this run take effect now (before the snapshot, so
     * ram0 shows them); a load without armed pokes clears them. */
    g_hdr->pokes_active = g_hdr->pokes_armed;
    g_hdr->pokes_armed = 0;
    bridge_apply_pokes(POKE_AT_RESTORE);
    /* Exact guest state at the restore point: the Rust simulation's initial
     * state and the loaded (and loader-processed) track data. */
    {
        uint32_t a;
        for (a = 0; a < 0x200000u; a += 4) {
            uint32_t w = psx_mod_read_word(0x80000000u + a);
            memcpy(g_ramsnap + a, &w, 4);
        }
        for (a = 0; a < 0x400u; a += 4) {
            uint32_t w = psx_mod_read_word(0x1F800000u + a);
            memcpy(g_ramsnap + 0x200000u + a, &w, 4);
        }
        g_hdr->ramsnap_gen = g_hdr->restore_gen;
    }
    if (g_hdr->route_armed) {
        g_hdr->route_armed = 0;
        g_hdr->route_active = 1;
    } else {
        g_hdr->route_active = 0;
    }
}

/* Player-1 pad transform (mod_plugins.h stage 2b): while a route is active
 * its output REPLACES the resolved pad, so host input (keyboard, a stray key
 * press) cannot leak into an oracle run. With no active route the stock pad
 * passes through. Keyed on VBlanks since restore, so repeated sampling within
 * a frame returns the same value. */
static int bridge_transform(const PSXModPadFrame* frame, PSXModPadOutput* out) {
    uint32_t vb, i, n;
    const BridgeStep* cur = NULL;
    (void)frame;
    if (!g_hdr) return 0;
    g_hdr->source_calls++;
    if (!g_hdr->route_active) return 0;
    vb = g_hdr->vb_since_restore;
    if (g_hdr->route_end_vb && vb >= g_hdr->route_end_vb) {
        g_hdr->route_active = 0;
        return 0;
    }
    n = g_hdr->route_len < MAX_STEPS ? g_hdr->route_len : MAX_STEPS;
    for (i = 0; i < n; i++) {
        if (g_steps[i].vb_start <= vb) cur = &g_steps[i];
        else break;
    }
    out->buttons = 0xFFFFu;
    out->lx = out->ly = out->rx = out->ry = 0x80;
    if (cur) {
        out->buttons = ~cur->buttons & 0xFFFFu;
        out->lx = cur->lx; out->ly = cur->ly; out->rx = cur->rx; out->ry = cur->ry;
    }
    return 1;
}

static void bridge_activate(void) {
    bridge_open();
    static const PSXModPadTransform transform = {
        sizeof(PSXModPadTransform),
        PSX_MOD_PAD_TYPE_BIT(PSX_MOD_PAD_DIGITAL) | PSX_MOD_PAD_TYPE_BIT(PSX_MOD_PAD_DUALSHOCK),
        PSX_MOD_PAD_DUALSHOCK,
        bridge_transform,
    };
    /* Dev runs keep mrm.enhancement.modern-controls disabled: one transform
     * per player, and this one must own player 1 (SESSION-2-oracle.md). */
    (void)psx_mod_set_pad_transform(0u, &transform);
}

PSX_MOD_CONSTRUCTOR(mrm_register_oracle_bridge_plugin) {
    (void)psx_mod_register_activation_plugin("mrm.dev.oracle-bridge", bridge_activate);
    (void)psx_mod_register_vblank_plugin("mrm.dev.oracle-bridge", bridge_vblank);
    (void)psx_mod_register_savestate_plugin("mrm.dev.oracle-bridge", bridge_restored);
}

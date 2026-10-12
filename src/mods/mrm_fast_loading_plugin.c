/* Fast Loading: host wall-clock pacing of sustained non-XA data loads
 * (psx_mod_set_load_acceleration). Every guest VBlank, CD deadline, interrupt
 * and callback is preserved; release_frames = 0 stops acceleration on the
 * first frame the load predicate clears, so it never spills into gameplay. */
#include "mod_plugins.h"

#include <stdlib.h>
#include <string.h>

#define PKG "mrm.enhancement.fast-loading"
#define FEATURE "fast-loading"

static void mrm_fast_loading_activate(void) {
    char speed[16];
    uint32_t multiplier = 4u;

    if (psx_mod_option_value(PKG, FEATURE, "speed", speed, sizeof speed)) {
        if (strcmp(speed, "uncapped") == 0) {
            multiplier = 0u;
        } else {
            char* end = speed;
            const unsigned long parsed = strtoul(speed, &end, 10);
            if (end != speed && *end == '\0')
                multiplier = (uint32_t)parsed;
        }
    }
    (void)psx_mod_set_load_acceleration(multiplier, 0u);
}

PSX_MOD_CONSTRUCTOR(mrm_register_fast_loading_plugin) {
    (void)psx_mod_register_activation_plugin("mrm.fast-loading",
                                             mrm_fast_loading_activate);
}

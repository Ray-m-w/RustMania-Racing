/* Smooth Motion: presentation-only frame blending (psxrecomp
 * docs/FRAME_RATE.md). Guest VBlanks, timers, CD, SPU and game logic keep
 * their stock cadence; only what the OpenGL presenter shows between them
 * changes. FLIP source: a VBlank is a new source frame only when the game
 * really flipped, so a game that renders below 60 Hz blends across its whole
 * frame instead of blending one VBlank and holding the next. */
#include "mod_plugins.h"

#include <stdlib.h>
#include <string.h>

#define PKG "mrm.enhancement.frame-rate"
#define FEATURE "frame-rate"

static void mrm_frame_rate_activate(void) {
    char rate[16];
    uint32_t fps = 0u; /* 0 = follow the display refresh */

    if (psx_mod_option_value(PKG, FEATURE, "rate", rate, sizeof rate) &&
        strcmp(rate, "display") != 0) {
        char* end = rate;
        const unsigned long parsed = strtoul(rate, &end, 10);
        if (end != rate && *end == '\0')
            fps = (uint32_t)parsed;
    }
    (void)psx_mod_set_frame_interpolation_source(PSX_MOD_FRAME_SOURCE_FLIP);
    (void)psx_mod_set_frame_interpolation_blend(
        PSX_MOD_FRAME_INTERPOLATION_MOTION_ADAPTIVE);
    (void)psx_mod_set_frame_interpolation(fps);
}

PSX_MOD_CONSTRUCTOR(mrm_register_frame_rate_plugin) {
    (void)psx_mod_register_activation_plugin("mrm.framerate",
                                             mrm_frame_rate_activate);
}

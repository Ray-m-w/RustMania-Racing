/* Muppet RaceMania widescreen: activation belongs to the mod catalog
 * (psxrecomp docs/WIDESCREEN.md). The renderer is psxrecomp's native-wide
 * path ([widescreen] native_wide in game.toml): it grows the render surface and
 * cull margin instead of stretching the 320px framebuffer. */
#include "mod_plugins.h"

#include <string.h>

#define PKG "mrm.enhancement.widescreen"
#define FEATURE "widescreen"
#define PLUGIN "mrm.widescreen"

static void mrm_widescreen_activate(void) {
    char aspect[16];
    unsigned num = 16, den = 9;
    int fit = 1;

    if (!psx_mod_option_value(PKG, FEATURE, "aspect", aspect, sizeof aspect))
        strcpy(aspect, "Fit");
    if (strcmp(aspect, "16:9") == 0) {
        fit = 0;
    } else if (strcmp(aspect, "21:9") == 0) {
        num = 21;
        fit = 0;
    } else if (strcmp(aspect, "32:9") == 0) {
        num = 32;
        fit = 0;
    }

    /* Fit starts from a 16:9 window, then follows every resize with no upper
     * limit; the fixed choices pin the view. */
    (void)psx_mod_set_fixed_display_aspect(num, den);
    if (fit)
        (void)psx_mod_set_adaptive_display_aspect(0u, 0u);
}

PSX_MOD_CONSTRUCTOR(mrm_register_widescreen_plugin) {
    (void)psx_mod_register_activation_plugin(PLUGIN, mrm_widescreen_activate);
}

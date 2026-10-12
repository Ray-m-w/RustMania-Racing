/* Skip Intro Movies: the player-facing switch lives in the mod catalog (as in
 * psxrecomp's Tomba example) rather than the generic launcher row. With no
 * per-movie frame table configured, the runtime ends a detected movie (MDEC +
 * XA streaming) through the game's own skip input (runtime/src/main.cpp). */
#include "mod_plugins.h"

static void mrm_skip_fmv_activate(void) {
    (void)psx_mod_set_auto_skip_fmv(1);
}

PSX_MOD_CONSTRUCTOR(mrm_register_skip_fmv_plugin) {
    (void)psx_mod_register_activation_plugin("mrm.skip-fmv",
                                             mrm_skip_fmv_activate);
}

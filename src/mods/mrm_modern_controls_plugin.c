/* Modern Controls: racing-style triggers on a modern gamepad.
 *
 * Right trigger -> the game's accelerate button (TRIANGLE; verified in the
 * quarry's Session 4 notes), left trigger -> BRAKE (CIRCLE, the game's own
 * Set Controllers screen, configuration A). While a trigger drives one of
 * those, its stock L2/R2 fold is removed so it does not also fire L2/R2.
 * Keyboard play, the D-pad, the stick (the game folds it into steering when
 * presented as a DualShock) and every other button are unchanged.
 *
 * psxrecomp pad transform (runtime/include/mod_plugins.h, stage 2b): runs only
 * in offline local play, never under netplay/rollback. */
#include "mod_plugins.h"

#define TRIGGER_ON 64u   /* 0..255; a quarter pull counts as pressed */

#define BTN_L2 (1u << 8)
#define BTN_R2 (1u << 9)
#define BTN_TRIANGLE (1u << 12)
#define BTN_CIRCLE (1u << 13)

static int mrm_modern_controls(const PSXModPadFrame* frame, PSXModPadOutput* out) {
    uint32_t pressed;
    if (!(frame->host_flags & PSX_MOD_PAD_HOST_GAMEPAD))
        return 0;                       /* keyboard only: stock pad */
    pressed = ~out->buttons & 0xFFFFu;  /* active-low -> active-high */
    if ((frame->host_flags & PSX_MOD_PAD_HOST_RT) && frame->host_rt >= TRIGGER_ON)
        pressed = (pressed & ~BTN_R2) | BTN_TRIANGLE;
    if ((frame->host_flags & PSX_MOD_PAD_HOST_LT) && frame->host_lt >= TRIGGER_ON)
        pressed = (pressed & ~BTN_L2) | BTN_CIRCLE;
    out->buttons = ~pressed & 0xFFFFu;
    return 1;
}

static void mrm_modern_controls_activate(void) {
    static const PSXModPadTransform transform = {
        sizeof(PSXModPadTransform),
        PSX_MOD_PAD_TYPE_BIT(PSX_MOD_PAD_DIGITAL) |
            PSX_MOD_PAD_TYPE_BIT(PSX_MOD_PAD_DUALSHOCK),
        PSX_MOD_PAD_DUALSHOCK,
        mrm_modern_controls,
    };
    (void)psx_mod_set_pad_transform(0u, &transform);
    (void)psx_mod_set_pad_transform(1u, &transform);
}

PSX_MOD_CONSTRUCTOR(mrm_register_modern_controls_plugin) {
    (void)psx_mod_register_activation_plugin("mrm.modern-controls",
                                             mrm_modern_controls_activate);
}

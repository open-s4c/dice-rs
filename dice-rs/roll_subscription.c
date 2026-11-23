/*
 * Rust's Dice Roll subscription entry point.
 * Compiled with the same generated dice.h as the configured runtime.
 */
#include <dice/roll.h>

int dice_rs_subscribe(chain_id chain, type_id type, ps_callback_f callback,
                      int slot)
{
    return dice_subscribe_abi(DICE_CONFIG_ID, chain, type, callback, slot);
}

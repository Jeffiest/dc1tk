// dc2tk SDK helpers for arg scripts. Each helper stands for the native calls in its body; the
// compiler expands it in place, so a helper call produces the same bytecode as writing the calls
// out, and the decompiler folds every run it matches. Included by arg.h.
#ifndef DC2TK_HELPERS_ARG_H
#define DC2TK_HELPERS_ARG_H

// Adds the two records that place an actor: its position (actor, x, y, z), then its yaw as (actor,
// 0.0, yaw, 0.0).
static inline void add_actor_place_records(int actor, float x, float y, float z, float yaw)
{
    add_arg_record(actor, x, y, z);
    add_arg_record(actor, 0.0, yaw, 0.0);
}

#endif

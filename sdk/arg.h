// dc2tk SDK. Natives for event argument files loaded with load_arg.
//
// Natives are built-in engine functions declared with NATIVE(...). Argument
// files run once when `load_arg` loads them and build the scene's integer and
// float argument lists.

#ifndef DC2TK_ARG_H
#define DC2TK_ARG_H

#include <dc2tk.h>
#pragma natives arg

enum program {
    // The only program the engine runs in an argument script: _LOAD_ARG loads the file and
    // CEventScriptArg::BuildArgData runs program 100 once to build the event's argument lists
    // (event_func.cpp script_run_id).
    build_args = 100,
};

// Stores all its arguments (ints, floats and strings, in order) as one argument record under the
// next record id, then adds 1 to that id. Event natives called with a single record id read their
// arguments from the matching record.
// Original name: _DATA.
extern void add_arg_record VM_ARGS(any value, ...) NATIVE(0);

// Sets the id that the next add_arg_record call stores its record under; ids start at 0.
// Original name: _ID_OFFSET.
extern void set_arg_record_id(int next_id) NATIVE(1);

#include <helpers/arg.h>

#endif

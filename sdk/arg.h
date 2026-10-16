// dc2tk SDK. Natives for event argument files loaded with load_arg.
//
// Natives are engine functions, declared extern with their number in
// NATIVE(...). A native with one result returns it (count = get_counter(3););
// natives with several results take pointers (get_actor_pos(0, &x, &y, &z);).
// Natives push nothing on the VM stack: a native in an expression is called
// first, into a hidden local, and the expression reads that.
// OPTIONAL(...) lists arguments a call may leave out. Natives never wait; scripts
// wait with `wait;`, which yields one frame (25 a second on PAL). While the player
// skips a cutscene the engine does not run natives until the script reaches
// `skip_end;`.
//
// NATIVE(B, N) means the game's scripts call the native as B + N. A call can
// pick another form with an AT(B) or DIRECT prefix.

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
extern void add_arg_record(any value, ...) NATIVE(0);

// Sets the id that the next add_arg_record call stores its record under; ids start at 0.
// Original name: _ID_OFFSET.
extern void set_arg_record_id(int next_id) NATIVE(1);

#include <helpers/arg.h>

#endif

# The c2s language

c2s is written as C. A script includes its SDK header and reads like a C
file; a C compiler or IDE can parse it with the SDK's prelude (below), and
`c2 compile` turns it into the exact bytecode.

```c
// A made-up event script for a town square: two characters stand in a camera shot, and talking to
// them gets a greeting that changes on the second visit.
//
// How an event script runs: the engine starts one of the script's programs (listed below with
// PROGRAM) when something happens on the map: the map is entered, the player talks to someone,
// steps on a trigger. The program's function runs until it returns. `wait;` hands control back to
// the engine for one frame (the game runs at 50 frames a second on a PAL console), so a loop with
// `wait;` in it is how a script waits for something without freezing the game.

// The SDK header for event scripts: it declares the engine functions (natives) a map script can
// call, helpers built from them, and constants such as PI and the program names.
#include <event.h>
// modkit is the SDK's library for mods. Its text helpers show lines you write yourself in a
// message window: modkit_say opens window 0 on the text, waits for the player to press (X) or (O)
// and closes it. (The game's own lines come from text files by number, with show_message.)
#include "modkit/text.c"

// Motion and face names. The engine looks animations up by name in the character's model file,
// and the game's model files name them in Japanese, so these strings have to be exactly those
// bytes. Giving them English names here keeps the rest of the script readable.
#define MOTION_STAND "立ち"
#define MOTION_WAVE "手を振る"
#define FACE_SMILE "笑顔"
#define EYES_BLINK "まばたき"

// A global keeps its value between programs for as long as the map's script stays loaded. It
// starts at 0 each time the map loads; anything that must survive a save goes in a save flag or
// counter instead (see shopkeeper.c).
int visits;

// The program table: which function the engine runs for each event. map_start runs when the player
// enters the map; talk_or_dungeon_start runs when the player presses the confirm button next to a
// character. The names come from enum program in event.h.
PROGRAM(map_start, on_map_start);
PROGRAM(talk_or_dungeon_start, on_talk);
// The main function the script's header names. Every script has one; event scripts usually name
// their start program.
MAIN(on_map_start);

// Waits the given number of frames. `wait;` ends this frame and carries on from here on the next.
void wait_frames(int frames)
{
    while (frames > 0) {
        wait;
        frames = frames - 1;
    }
}

// Sets the scene up when the player enters the map.
void on_map_start(void)
{
    // Clears what an earlier event may have left behind (queued actor commands, sprites, effects)
    // and stops the camera following the player. Call it at the start of every event.
    init_event();

    // Actors are the event system's puppets: each one drives an object (here, a character) through
    // a queue of commands, and runs them over the following frames while the script goes on.
    // Characters live in numbered scene slots. In a town the engine fills slots 8 and up with the
    // map's villagers; a scene can also load its own character into a slot first, with
    // load_scene_chara(slot, "path/to/model.chr"). Here actor 0 drives the character in slot 8 and
    // actor 1 the one in slot 9.
    init_chara_actor(0, 8);
    init_chara_actor(1, 9);

    // Camera eye at (10, 20, 30), looking at (0, 15, 0). Positions are in world units; y is up.
    set_camera_view(10.0, 20.0, 30.0, 0.0, 15.0, 0.0);
    // Actor 0 at the origin, turned half a circle (angles are in radians: PI is 180 degrees).
    place_actor(0, 0.0, 0.0, 0.0, PI);
    place_actor(1, 40.0, 0.0, 0.0, 0.0);
    // Actor 0 smiles and blinks (face and eye animations on the model's textures).
    set_actor_face(0, FACE_SMILE, EYES_BLINK);
    // Actor 1 plays its standing animation on a loop.
    loop_actor_motion(1, MOTION_STAND);
}

// Greets the player; the first visit and later ones get different lines.
void on_talk(void)
{
    visits = visits + 1;
    // Waves once and holds the last frame: CHARA_MOTION_RESTART starts the motion from its first
    // frame, CHARA_MOTION_HOLD stops it on its last frame instead of looping.
    set_actor_motion(0, MOTION_WAVE, CHARA_MOTION_HOLD | CHARA_MOTION_RESTART);
    // Lets the wave play for 30 frames (0.6 seconds) before the message opens.
    wait_frames(30);
    if (visits == 1) {
        modkit_say("Hello! First time in the square?");
    } else {
        modkit_say("Back again? Welcome!");
    }
}
```

This is a made-up script. For a whole mod, see the
[calculator example](examples/calculator/calculator.c).

- Types are `int`, `float` and `string`, with `int x[4]` for a local array
  and `int x[]` or `int *x` for an array or variable passed by pointer
  (`void *x` for one the function never reads through: the decompiler types
  such a parameter from what its callers pass, and writes `void *` where
  they pass both ints and floats). A
  variable can also be declared with a value kind or enum from the headers
  (`bgm_file track;`, `enum se_bank bank;`): it is the kind's base type,
  and the definitions remember it, so the decompiler prints the numbers it
  holds as names.
  The decompiler declares locals at the top of a function, in slot order.
- Functions have C return types. The decompiler makes a function `void` when
  it never returns a value and nothing uses its result, `float` when it
  returns a float, and `int` otherwise. Every function still ends with the
  compiler's implicit `return 0`; `return;` compiles the same way.
- Globals are file-scope `int g;` declarations, in slot order.
  `PROGRAM(number_or_name, function);` lines are the program table in
  order, and `MAIN(function);` names the header's main function.
- Statements and expressions are C: `if`/`else`, `while`, `break`,
  `continue`, `return`, assignment as an expression, `&&`, `||`, `!`, `&`,
  `|`, comparisons, `+ - * / %`, `&x`, `x[i]`, casts `(int)x` and `(float)x`,
  and calls. `(int *)&x`, `(float *)&x` and `(enum flags)&x` pass the
  address as it is: they compile to nothing and only tell C tools that a
  retail call passes a float's address where an int pointer is read, or an
  address where a native takes an enum. `wait;`, `end;` and `skip_end;` are statements; `print(...)`,
  `sin(x)` and `cos(x)` are built in. `-1` compiles to the constant 1
  followed by `NEG`, which is what the original compiler emitted for all
  231,222 negative literals on the disc.
- Natives are declared `extern` in the SDK headers and called like
  functions. A native whose only result is its last argument returns it
  (`count = get_counter(3);`); one with several results takes pointers
  (`get_chara_pos(0, &x, &y, &z);`). Natives push nothing on the VM stack,
  so the decompiler always shows a native call as a statement or an
  assignment of its result. In your own code a native with a result can
  stand in an expression (`if (get_flag(5) && count > 2)`): the compiler
  calls it first, into a hidden local, and the expression reads that. The
  original Level-5 names (`_GET_FLAG(12, &flag);`) and unregistered numbers
  (`_EXT_190(...)`) still compile.
- `AT(n)` or `DIRECT` before a native call picks a group form other than the
  header's default (see the SDK). 471 of the 1.4 million calls on the disc
  need one.
- `#include "lib/..."` pulls in a library's functions at that point; include
  paths are looked up from the including file's folder and then each folder
  above it, then in `sdk/` (where the `modkit` libraries are). A library's
  `#define`s, enums, typedefs and helpers become the including script's, as
  with a header.
- `#pragma` lines carry what the bytes need and the language does not say:
  `magic`, `unk_14`, `header_padding`, `globals N` and `string_pool` (the
  file's string buffer was never reset between functions) for the file;
  `strings` before a function and `padding_of NAME HEX` after the code for
  string-block bytes no rule predicts (`padding HEX` before a function
  still works). None of the PAL disc's scripts need the last two. C
  compilers ignore unknown pragmas.
- A function the decompiler cannot structure prints as
  `asm f_0388 arg 0 { ... }` around its text assembly, and a file it cannot
  read at all as `#pragma assembly` followed by the whole disassembly. Neither
  happens on the retail disc, and neither is C.

## Writing your own code

The decompiler writes only the forms above, so retail scripts compile back
byte for byte. Code you write can use C89 (ANSI C) almost whole, without
`double`; [the standard](c-standard.md) lists every feature, what the
compiler does with it, and what is still to come (structs, unions, pointer
arithmetic, global arrays, function pointers and varargs). Each form compiles
to the VM's own instructions:

- Declarations anywhere in a function, with initializers: `int count = 0;`,
  `for (int i = 0; ...)`, `int table[] = {1, 2, 4};`, `int grid[3][4];`,
  `int *p = &x;`, `char name[] = "Max";`. An array's initializer sets the
  elements it leaves out to 0. Every local gets one slot for the whole
  function: a name declared again in a block beside another reuses the slot,
  and one that hides a name of an enclosing block gets a slot of its own.
- `static` locals, which keep their value between calls, and globals with
  values (`int lives = 3;`), set once when the script loads, before the first
  program that starts runs its own code. Global and static arrays and pointers
  are not supported yet: the VM cannot index a global.
- `char`, `short`, `long`, `signed` and `unsigned`, with C's widths (char 8
  bits and signed, short 16, int and long 32) and wrap-around, `const`
  (checked), `volatile`, `register`, `auto`, `typedef`, `enum` and `sizeof`
  (1 for a char, 2 for a short, 4 for the rest). `<limits.h>`, `<float.h>` and
  `<stddef.h>` are in the SDK.
- `for`, `do ... while`, `switch` with `case`, `default`, fall-through and
  `break`, and `goto` with labels. `continue` in a `for` runs the step first.
- `+=`, `-=`, `*=`, `/=`, `%=`, `&=`, `|=`, `^=`, `<<=`, `>>=`, `++` and
  `--`, before or after, `?:` and the comma operator.
- `^`, `~`, `<<` and `>>`, which the VM has no instruction for: `a ^ b` is
  worked out as `(a | b) - (a & b)`, `~a` as `-a - 1`, and shifts as
  multiplying or dividing by a power of 2 (a loop when the count is not a
  constant). `>>` of a negative `int` rounds toward zero, not down; of an
  `unsigned` value it shifts zeros in.
- Function prototypes and old-style (K&R) definitions; bodies without braces
  (`if (x) wait;`), nested `{ }` blocks and empty statements.
- `string` variables, `int *p` parameters and locals, `*p` for `p[0]`, and an
  array name standing for its first element's address (`fill(table, 3)`).
- C's literals: octal and hexadecimal numbers, suffixes (`2.5f`, `10u`),
  character constants with every escape, adjacent strings joined.
- C's preprocessor: macros with parameters, `#` and `##`, `#if` with C's
  arithmetic and `defined`, `#ifdef`, `#elif`, `#undef`, `#error`, `#line`,
  `__FILE__`, `__LINE__`, `__DATE__`, `__TIME__` and `__STDC__`. A
  `#define` whose value is a constant stays a named constant, which is how
  the decompiler's trees and a header's constants reach code. `#include <...>`
  reads the header as text for its macros too, so they reach the including
  file's `#if`.
- Errors name the line.

`double` is refused (the VM's floats are 32 bits); use `float`.

The VM differs from C in ways that matter when writing code, unless the code
asks for C's own semantics with `#pragma std c89` (below):

- **Values keep their type.** Every value on the VM's stack and in a
  variable carries its type, and storing copies it: after `int x; x = 1.5;`,
  `x` holds the float 1.5, where C would truncate it to 1. Mixed arithmetic
  gives a float, as in C. Write `x = (int)y;` when you mean to truncate.
- **`&&` and `||` give the deciding value, not 0 or 1:** `x = a || 5;` sets
  `x` to 5 when `a` is 0. Conditions behave as in C.
- A native called inside a condition runs before the condition is tested,
  even on the right of `&&` or `||` or in a branch of `?:`, since natives
  cannot run in the middle of an expression. Write an `if` when a native
  should only run some of the time.

`#pragma std c89` (or `c90`) turns on strict mode for the functions and
globals after it in the file: a value converts to the type it is stored in,
passed as or returned as (`int x = 2.5;` gives 2, by `FTOI`; an int stored in
a float gets `ITOF`), `&&` and `||` give 0 or 1 where their value is used, and
a native in an operand C may not evaluate (the right of `&&` or `||`, a branch
of `?:`, the right of a comma) runs only when C would evaluate it, in an `if`
on a hidden local. Strict code costs a few instructions more where these
apply; the decompiler never writes the pragma, so retail scripts keep their
bytes. A library or mod file that wants C's semantics starts with the pragma;
the conformance suite (see [the standard](c-standard.md)) runs in strict mode.

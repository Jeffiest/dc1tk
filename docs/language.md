# The c2s language

c2s is written as C. A script includes its SDK header and reads like a C
file; a C compiler or IDE can parse it with the SDK's prelude (below), and
`cli.py compile` turns it into the exact bytecode.

```c
// A made-up event script: two actors in a town square, and a greeting that changes on the second
// visit.

#include <event.h>

int visits;

PROGRAM(map_start, on_map_start);
PROGRAM(talk_or_dungeon_start, on_talk);
MAIN(on_map_start);

// Waits the given number of frames.
void wait_frames(int frames)
{
    while (frames > 0) {
        wait;
        frames = frames - 1;
    }
}

// Shows a message and waits until the player closes it.
void say(int line)
{
    int closed;

    show_message(0, line);
    closed = 0;
    while (!closed) {
        wait;
        closed = is_message_closed(0);
    }
}

// Points the camera at the square and puts the two actors in place.
void on_map_start(void)
{
    init_event();
    set_camera_view(10.0, 20.0, 30.0, 0.0, 15.0, 0.0);
    place_actor(0, 0.0, 0.0, 0.0, PI);
    place_actor(1, 40.0, 0.0, 0.0, 0.0);
    set_actor_face(0, "笑顔", "まばたき");  // smile; blink
    loop_actor_motion(1, "立ち");  // stand
}

// Greets the player; the second visit gets a different line.
void on_talk(void)
{
    visits = visits + 1;
    set_actor_motion(0, "手を振る", CHARA_MOTION_HOLD | CHARA_MOTION_RESTART);  // wave
    wait_frames(30);
    if (visits == 1) {
        say(100);
    } else {
        say(110);
    }
}
```

This is [`docs/examples/greeting/greeting.c`](examples/greeting/greeting.c), a made-up script. The
[examples](examples/README.md) show the same script at each stage of decompiling.

- Types are `int`, `float` and `string`, with `int x[4]` for a local array
  and `int x[]` or `int *x` for an array or variable passed by pointer. A
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
  and calls. `wait;`, `end;` and `skip_end;` are statements; `print(...)`,
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
  `magic`, `unk_14`, `header_padding` and `globals N` for the file;
  `strings` before a function; `padding_of NAME HEX` for the leftover bytes
  after a function, printed together at the end of the file so they stay
  out of the code (`padding HEX` before a function still works). C
  compilers ignore unknown pragmas.
- A function the decompiler cannot structure prints as
  `asm f_0388 arg 0 { ... }` around its text assembly, and a file it cannot
  read at all as `#pragma assembly` followed by the whole disassembly. Neither
  happens on the retail disc, and neither is C.

## Writing your own code

The decompiler writes only the forms above, so retail scripts compile back
byte for byte. Code you write can also use more of C; each form compiles to
the VM's own instructions:

- Declarations anywhere in a function, with initializers: `int count = 0;`,
  `for (int i = 0; ...)`, `int table[] = {1, 2, 4};`, `int *p = &x;`. Every
  local still gets one slot for the whole function, so a name declared again
  in another block reuses the slot, and must have the same type.
- `for`, `do ... while`, and `switch` with `case`, `default`, fall-through
  and `break`. `continue` in a `for` runs the step first.
- `+=`, `-=`, `*=`, `/=`, `%=`, `&=`, `|=`, `^=`, `<<=`, `>>=`, `++` and
  `--`, before or after, and `?:`.
- `^`, `~`, `<<` and `>>`, which the VM has no instruction for: `a ^ b` is
  worked out as `(a | b) - (a & b)`, `~a` as `-a - 1`, and shifts as
  multiplying or dividing by a power of 2 (a loop when the count is not a
  constant). `>>` of a negative number rounds toward zero, not down.
- Bodies without braces (`if (x) wait;`), nested `{ }` blocks and empty
  statements.
- `string` variables, `int *p` parameters and locals, `*p` for `p[0]`, and an
  array name standing for its first element's address (`fill(table, 3)`).
- Character literals (`'A'`, `'\n'`) and literal suffixes (`2.5f`, `10u`).
- `#if`, `#ifdef`, `#ifndef`, `#elif`, `#else`, `#endif` and `#undef`, with
  `defined(NAME)`; conditions see the file's own `#define`s.
- Errors name the line.

Not available, because the VM cannot do them: global arrays and global
pointers (the VM has no indexed access to globals), initialized globals
(globals start at 0 when the script loads; set them in a program), structs,
unions, function pointers, `double`, 64-bit and unsigned arithmetic, and
macros with parameters (write a function). `goto` and multi-dimensional
arrays are not supported either.

The VM differs from C in ways that matter when writing code:

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

# The script language

Game scripts use the C programming language. Each script includes an SDK header and uses standard C syntax. Standard C tools (such as `clang` and `clangd`) can parse scripts using the SDK prelude, and `dc2tk compile` compiles them into `.stb` bytecode.

## Example event script

```c
// Example event script for a town square: two characters stand in a camera shot.
// Talking to the first character displays a greeting that changes on the second interaction.
//
// How an event script runs:
// The engine starts one of the script's programs (declared with PROGRAM) when an event occurs on
// the map (for example, when the player enters the map, talks to an NPC, or steps on a trigger).
// The program function runs until it returns. The `wait;` statement yields control back to the
// engine for one frame (25 frames per second on PAL consoles). A loop containing `wait;` lets a
// script wait for an animation or timer without freezing the engine.

// Include the SDK header for map event scripts. This declares all event natives, inline helpers,
// and constants such as PI and program names.
#include <event.h>
// Include the modkit text library. modkit_say opens message window 0, displays a custom string,
// waits for the player to press Confirm (X or O), and closes the window.
#include "modkit/text.c"

// Animation and facial expression names. The engine looks up animations by their Shift-JIS name
// inside the character's .chr model file. Defining English macros keeps the script readable.
#define MOTION_STAND "立ち"
#define MOTION_WAVE "手を振る"
#define FACE_SMILE "笑顔"
#define EYES_BLINK "まばたき"

// Global variables retain their values between program runs while the map script remains loaded.
// Globals reset to 0 each time the map loads. Use save flags or save counters for state that must
// persist across map changes and save files.
int visits;

// Program table: maps engine event numbers to script functions.
// map_start runs when the player enters the map.
// talk_or_dungeon_start runs when the player presses Confirm next to an NPC.
PROGRAM(map_start, on_map_start);
PROGRAM(talk_or_dungeon_start, on_talk);
// Main entry function recorded in the .stb file header.
MAIN(on_map_start);

// Waits for the specified number of frames. Each `wait;` yields for one frame.
void wait_frames(int frames)
{
    while (frames > 0) {
        wait;
        frames = frames - 1;
    }
}

// Initializes the scene when the player enters the map.
void on_map_start(void)
{
    // Reset event state (clear queued actor commands, sprites, and effects) and detach the camera
    // from the player. Call init_event() at the start of every event.
    init_event();

    // Bind actor 0 to scene character slot 8, and actor 1 to scene character slot 9.
    // In towns, the engine populates slots 8 and above with the map's villagers.
    // A scene can also load a custom model into a slot with load_scene_chara(slot, "path.chr").
    init_chara_actor(0, 8);
    init_chara_actor(1, 9);

    // Set camera eye to (10, 20, 30) looking at (0, 15, 0) in world units (y is up).
    set_camera_view(10.0, 20.0, 30.0, 0.0, 15.0, 0.0);
    // Place actor 0 at the origin facing 180 degrees (PI radians), and actor 1 at x = 40.
    place_actor(0, 0.0, 0.0, 0.0, PI);
    place_actor(1, 40.0, 0.0, 0.0, 0.0);
    // Set facial and eye texture animations on actor 0.
    set_actor_face(0, FACE_SMILE, EYES_BLINK);
    // Loop the standing animation on actor 1.
    loop_actor_motion(1, MOTION_STAND);
}

// Greets the player with a different message on the first interaction versus later interactions.
void on_talk(void)
{
    visits = visits + 1;
    // Play the wave animation once from the first frame and hold the final frame.
    set_actor_motion(0, MOTION_WAVE, CHARA_MOTION_HOLD | CHARA_MOTION_RESTART);
    // Wait 30 frames (1.2 seconds at 25 FPS) before opening the message window.
    wait_frames(30);
    if (visits == 1) {
        modkit_say("Hello! First time in the square?");
    } else {
        modkit_say("Back again? Welcome!");
    }
}
```

For a complete working mod, see the [calculator example](examples/calculator/calculator.c) and the [Spinning Cult example](examples/spinning-cult/README.md).

## Core language and decompiler output

Decompiled retail scripts use a direct subset of C that maps one-to-one to `.stb` bytecode instructions:

- **Types**: The base VM types are `int`, `float`, and `string`. Local arrays use `int x[4]`. Array or pointer parameters use `int x[]` or `int *x` (or `void *x` when a parameter is never dereferenced in the function and callers pass both `int` and `float` pointers). Variables can also use SDK value kinds or enums (such as `bgm_file track;` or `enum se_bank bank;`). These compile as their underlying base type and tell the decompiler to format numeric constants as symbolic names. The decompiler declares all local variables at the top of each function in stack-slot order.
- **Function return types**: The decompiler emits `void` when a function never returns a value and callers ignore its result, `float` when it returns a floating-point value, and `int` otherwise. In bytecode, every function ends with an implicit `return 0`, and `return;` compiles to the same instruction.
- **Globals and entry points**: File-scope declarations (`int g;`) allocate global slots in declaration order. `PROGRAM(number_or_name, function);` entries build the script's program table in order, and `MAIN(function);` sets the header's main entry point.
- **Statements and expressions**:
  - Control flow: `if`/`else`, `while`, `break`, `continue`, and `return`.
  - Operators: assignment (`=`), logical (`&&`, `||`, `!`), bitwise (`&`, `|`), comparison (`==`, `!=`, `<`, `<=`, `>`, `>=`), arithmetic (`+`, `-`, `*`, `/`, `%`), address-of (`&x`), array indexing (`x[i]`), and type casts (`(int)x`, `(float)x`).
  - Pointer casts: `(int *)&x`, `(float *)&x`, and `(enum flags)&x` compile to no extra instructions and pass the address unchanged. They allow standard C type checkers to accept retail calls that pass a `float` address to an `int *` parameter or pass an address to an enum-typed parameter.
  - VM statements and built-ins: `wait;`, `end;`, and `skip_end;` are built-in statements. `print(...)`, `sin(x)`, and `cos(x)` are built-in VM operations.
  - Negative literals: `-1` compiles to pushing constant `1` followed by the `NEG` instruction, matching how the original compiler emitted all 231,222 negative literals on the retail disc.
- **Native calls**: SDK headers declare engine functions with `extern`.
  - A native whose only output is its last parameter is declared with a return type (`count = get_counter(3);`).
  - A native with multiple outputs takes pointer arguments (`get_chara_pos(0, &x, &y, &z);`).
  - Because natives do not push values onto the VM expression stack, the decompiler always writes native calls as standalone statements or direct variable assignments.
  - In custom code, you can call a value-returning native directly inside an expression (such as `if (get_flag(5) && count > 2)`). The compiler automatically evaluates the native into a hidden local variable before the expression.
  - Original Level-5 native names (`_GET_FLAG(12, &flag);`) and unregistered native names (`_EXT_190(...)`) also compile.
- **Native group prefixes (`AT` and `DIRECT`)**: Placing `AT(n)` or `DIRECT` before a native call selects a non-default native encoding group (used by 471 of the 1.4 million native calls on the retail disc; see [sdk.md](sdk.md)).
- **File inclusion (`#include`)**: `#include "lib/..."` inserts a shared library's functions and declarations at that point in the file. The compiler searches for quoted includes in the script's directory, each parent directory up to the root, and finally `sdk/` (which contains `modkit/`). All `#define` macros, enums, `typedef`s, and inline helpers in an included library become available to the including script.
- **Pragmas (`#pragma`)**: Pragmas record file metadata required for byte-for-byte round-tripping:
  - File-level pragmas: `magic`, `unk_14`, `header_padding`, `globals N`, and `string_pool` (used when the original compiler did not reset the string buffer between functions).
  - String-block pragmas: `strings` before a function and `padding_of NAME HEX` after a function (or legacy `padding HEX` before a function) specify unpredicted string-block bytes. No PAL disc script requires `strings` or `padding_of`.
  - Native table pragma: `#pragma natives TABLE` selects the native function table. Each SDK header includes this pragma automatically, so scripts that `#include` an SDK header do not need to write it.
  - `#pragma once` is supported (libraries are included only once by default).
  - Mod patch pragmas (`target`, `override`, `every_frame`) are documented in [mods.md](../guide/mods.md#patches).
  - Unrecognized `#pragma` directives are ignored.
- **Inline assembly fallback**: If the decompiler encounters a function with irreducible control flow, it emits `asm f_0388 arg 0 { ... }` containing text assembly. If a file cannot be parsed at all, it emits `#pragma assembly` followed by a full disassembly. Neither fallback occurs on any retail disc script.
- **Floating-point formatting and line endings**: Float constants print as the shortest decimal representation that round-trips to the exact 32-bit IEEE 754 value (`0.5`, `1.0`, `3.1415927`, or scientific notation such as `1e20` and `1.5e-7`). Decompiled files use LF line endings, and the compiler accepts both LF and CRLF.

## Writing custom C89 code

While the decompiler emits only the forms above so retail scripts recompile byte-for-byte, custom scripts and mods can use the full C89 (ANSI C) language except `double` and numeric pointer comparisons/subtraction. See [c-standard.md](c-standard.md) for the complete C89 support table.

The compiler lowers these C89 features into standard VM instructions:

- **Mixed declarations and initializers**: Declare local variables anywhere inside a function with initializers (`int count = 0;`, `for (int i = 0; ...)`, `int table[] = {1, 2, 4};`, `int grid[3][4];`, `int *p = &x;`, `char name[] = "Max";`). Omitted array elements initialize to `0`. Each local variable occupies a function stack slot: variables in sibling blocks share slots, while inner variables that shadow outer variables receive distinct slots.
- **`static` locals and initialized globals**: `static` local variables and initialized globals (`int lives = 3;`, `int primes[] = {2, 3, 5};`, `int *at = &lives;`) persist across function calls. Their initializers execute once when the `.stb` loads, before the first entry program runs. Global and `static` arrays and pointers support both constant indexing (direct global slot access) and variable indexing (via the array's stack address).
- **Structs, unions, and bit-fields**: Full support for `struct` and `union` types (nested structs, arrays of structs, pointer member access `p->next->value`, value assignment, passing and returning structs by value, brace-enclosed initializer lists, ternary `?:` on structs, bit-fields packed into `int` slots, block-scoped struct tags, `typedef`, and `offsetof` in `<stddef.h>`).
- **Pointers and pointer arithmetic**: Support for multi-level pointers (`int **pp`), pointer arithmetic (`p + 2`, `p++`, `*(a + i)`, `i[a]`), functions returning pointers, and comparisons against `0` or `NULL`. Because the VM represents pointers as abstract slot references rather than numeric memory addresses, subtracting two pointers (`p - q`) or comparing two pointers (`p < q`, `p == q`) is not supported; track array indices instead.
- **Function pointers**: Support for function pointers (`int (*f)(int) = twice; f(2);`), including inside arrays and structs, and pointing to functions that take or return structs or variadic arguments (`...`). The compiler represents function pointers as integer IDs and dispatches calls through a generated switch helper.
- **Variadic functions and wide strings**: Support for variadic functions (`int sum(int count, ...)`) using `<stdarg.h>`, and wide string literals (`L"..."`, arrays of `wchar_t`). Standard string literals pass as VM `string` values to natives, but when indexed or assigned to a `char *` (`char *p = "abc";`), the compiler lowers them to null-terminated `char` arrays.
- **Integer widths, qualifiers, and `sizeof`**: Support for `char` (8-bit signed), `short` (16-bit signed), `int` and `long` (32-bit signed), `signed`, `unsigned` (with modular wrap-around), `const` (enforced at compile time), `volatile`, `register`, `auto`, `typedef`, `enum`, and `sizeof` (`1` for `char`, `2` for `short`, `4` for all other scalar and pointer types). The SDK includes `<limits.h>`, `<float.h>`, `<stddef.h>`, and `<stdarg.h>`.
- **Control flow statements**: Support for `for`, `do ... while`, `switch` (`case`, `default`, fall-through, and `break`), and `goto` with labels. In a `for` loop, `continue` executes the increment step before re-testing the condition.
- **Compound assignments and operators**: Support for `+=`, `-=`, `*=`, `/=`, `%=`, `&=`, `|=`, `^=`, `<<=`, `>>=`, prefix and postfix `++` and `--`, the conditional operator `?:`, and the comma operator `,`.
- **Synthesized bitwise operations**: Because the VM has no native instructions for `^`, `~`, `<<`, or `>>`, the compiler lowers `a ^ b` to `(a | b) - (a & b)`, `~a` to `-a - 1`, and shifts (`<<`, `>>`) to multiplication or division by powers of `2` (or a runtime loop for non-constant shift counts). Right-shifting a negative signed `int` rounds toward zero; right-shifting an `unsigned` value shifts in zero bits.
- **Function syntax**: Support for function prototypes, K&R-style definitions, single-statement bodies without braces (`if (x) wait;`), nested `{ }` blocks, empty statements, and array-to-pointer decay (`fill(table, 3)`).
- **Literals**: Support for octal (`077`), hexadecimal (`0xFF`), type suffixes (`2.5f`, `10u`, `10L`), character constants with standard escape sequences, and adjacent string literal concatenation.
- **C preprocessor**: Full support for object-like and function-like macros, stringification (`#`), token pasting (`##`), `#if` with constant integer arithmetic and `defined(...)`, `#ifdef`, `#ifndef`, `#elif`, `#else`, `#endif`, `#undef`, `#error`, `#line`, and predefined macros (`__FILE__`, `__LINE__`, `__DATE__`, `__TIME__`, `__STDC__`). Simple constant `#define` macros also remain visible to the compiler's symbolic constant table.
- **`double` restriction**: The compiler rejects `double` and `long double` because the PS2 VM supports only 32-bit `float` values. Use `float` instead.

## VM differences from standard C and `#pragma std c89`

By default, compiled code preserves the game VM's native semantics so decompiled retail scripts compile back to identical bytes. Three VM behaviors differ from standard C unless you enable strict C89 mode:

1. **Dynamic slot types on assignment**: Every VM stack value and variable slot carries a runtime type tag, and assignment copies both the value and its type tag. In default mode, `int x; x = 1.5;` stores the `float` value `1.5` into `x` without truncating it to `1`. Write `x = (int)y;` to truncate explicitly.
2. **Logical operators return the deciding operand**: In default mode, `&&` and `||` yield the operand value that determined the result rather than `0` or `1` (for example, `x = a || 5;` sets `x` to `5` when `a` is `0`). Conditional branches (`if`, `while`) treat any non-zero value as true.
3. **Natives inside complex conditions evaluate eagerly**: Because native calls cannot execute in the middle of a VM expression stack evaluation, the compiler evaluates native calls before testing the enclosing condition in default mode, even when the call appears on the right-hand side of `&&`, `||`, or `?:`. Use nested `if` statements if a native call must not execute unless the first condition holds.

### Enabling strict C89 mode (`#pragma std c89`)

Place `#pragma std c89` (or `#pragma std c90`) at the top of a custom script, patch, or library file to enable strict C89 semantics for all subsequent functions and globals in that file:

- **Automatic type conversion on store, argument pass, and return**: Storing, passing, or returning a `float` as an `int` automatically emits `FTOI` (`int x = 2.5;` yields `2`), and storing an `int` as a `float` emits `ITOF`.
- **Normalized boolean results (`0` or `1`)**: Logical `&&` and `||` normalize their result to `0` or `1` whenever the result value is used.
- **Short-circuit native evaluation**: A native call on the right-hand side of `&&`, `||`, `?:`, or `,` is wrapped in a conditional branch so it executes only when standard C short-circuit rules evaluate that operand.
- **Strict pointer type diagnostics**: The compiler rejects implicit conversions between integers and pointers (except null pointer constant `0`) and between incompatible pointer types.

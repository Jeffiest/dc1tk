# Standard C

The aim is a conforming C89/C90 freestanding implementation (ANSI X3.159-1989, ISO/IEC
9899:1990) for code people write, on a VM that was made for a smaller language (see
[the format](format.md)). One rule bounds it: every script on the disc still compiles to its
own bytes. Only forms the decompiler never writes take the new paths, and `c2 verify`
checks the whole disc.

Progress is measured by a conformance suite: a small program in
`src/crates/c2-script/tests/c89/` for each feature, compiled with the SDK and run in the
test VM (`cargo test -p c2-script --test c89`). Each program's second line says what it shows,
and its row below says the same (a test checks that):

- **supported**: it compiles and gives C's result, or, where C requires a diagnostic, the
  compiler gives one;
- **differs**: it compiles and gives another result, named in the program, until the step of
  [PLAN.md](../PLAN.md)'s "Next: standard C" given;
- **planned**: it does not compile yet (or fails when run); the step given makes it work;
- **limit**: refused by design.

Today: 95 programs supported (83 giving C's result, 12 giving C's diagnostic), 3 differ, 15
planned and 2 at the limit.

## The departure: double

`double` and `long double` are refused: "double is not supported: the VM has 32-bit floats
only; use float". C89 asks a double for 10 significant digits, which the VM's 32-bit floats
cannot give, and software doubles would make every floating operation a call. A plain
constant such as `1.5` is a float, as it always was in c2s; `1.5L` and the word `double`
anywhere in code are errors, naming the line. `<float.h>` describes `float` only.

## What the implementation defines

C89 leaves these to the implementation and asks it to say what it does.

- **Types.** `char` is 8 bits and signed (as on the PS2); `short` is 16 bits; `int` and `long`
  are 32 bits, two's complement, the VM's own ints. `sizeof` gives 1, 2 and 4, 4 for `float`
  and pointers, and is a `size_t`, which is `unsigned int`; `ptrdiff_t` is `int`. A `char` or
  `short` takes a whole VM slot and is wrapped to its width when stored.
- **Arithmetic.** Signed arithmetic wraps on overflow, as the game's VM does. `/` and `%`
  truncate toward zero. `>>` of a negative `int` divides by a power of 2, rounding toward
  zero (`-7 >> 1` is -3); of an unsigned value it shifts zeros in. A value out of range for a
  narrow signed type wraps (`(signed char)200` is -56). A shift by 32 or more of a constant
  count is refused.
- **Floats.** `float` is an IEEE 754 single held by the PS2's FPU, which rounds toward zero,
  flushes values below `FLT_MIN` to zero, and has no infinities or NaNs. The compiler reads
  constants rounded to nearest. See `sdk/float.h`.
- **Characters.** Source files are UTF-8; strings are stored as Shift-JIS, the game's text
  encoding, so `MB_LEN_MAX` is 2 and `L'x'` (a `wchar_t`, `unsigned short`) is the
  character's Shift-JIS code. `\x` takes two hexadecimal digits.
- **Startup.** A freestanding program: the engine starts the functions the `PROGRAM` table
  names, and `MAIN` names the header's main function; there is no `main()`. Objects with
  static storage (globals and static locals) are set to their initializers once each time the
  script is loaded, before the first function the engine starts runs any of its own code (see
  below).
- **Preprocessor.** `#include <...>` reads the SDK's headers (`sdk/`), `#include "..."` the
  script's folder, the folders above it, then the SDK. Pragmas are c2s's own (see
  [the language](language.md)); others are ignored, as C says. `__DATE__` and `__TIME__` are
  UTC, from `SOURCE_DATE_EPOCH` when it is set. `__STDC__` is 1; `__STDC_VERSION__` is not
  defined (it came with C95).
- **Limits.** Identifiers have no length limit and are case-sensitive, external ones too. A
  function can take at least 31 parameters (the VM keeps a type for the first 10 only, which
  the engine never reads). Parentheses nest about 190 deep, blocks and loops at least 15.
- **register** asks nothing of the VM (every variable is a slot); taking a register
  variable's address is refused, as C requires.

## The features

### 3.1 Lexical elements

| Feature | Status | Program | Notes |
|---|---|---|---|
| 3.1.1 Keywords | supported | `lexical_keywords` | all 32 but `struct`, `union` (planned) and `double` (limit) |
| 3.1.2 Identifiers | supported | `lexical_identifiers` | |
| 3.1.3.1 Floating constants | supported | `lexical_floating_constants` | `.5`, `2.`, exponents, `f` |
| 3.1.3.1 The `l` suffix | limit | `lexical_long_double_constants` | a `long double` |
| 3.1.3.2 Integer constants | supported | `lexical_integer_constants` | octal, hexadecimal, `u` and `l` |
| 3.1.3.2 Their types | supported | `lexical_unsigned_constants` | too large for `int`, or `u`: unsigned |
| 3.1.3.4 Character constants | supported | `lexical_character_constants` | every escape, octal and hex |
| 3.1.3.4 Wide characters | supported | `lexical_wide_characters` | Shift-JIS codes |
| 3.1.4 String literals | supported | `lexical_string_literals` | adjacent literals are one |
| 3.1.4 Wide strings | planned (step 5) | `lexical_wide_strings` | arrays of `wchar_t` from `L"..."` initializers |
| 3.1.9 Comments | supported | `lexical_comments` | `//` too, as an extension |
| 5.2.1.1 Trigraphs | supported | `lexical_trigraphs` | not in comments, whose text the definitions keep |

### 3.2 Conversions

| Feature | Status | Program | Notes |
|---|---|---|---|
| 3.2.1.1 Integer promotion | supported | `conversions_integer_promotion` | |
| 3.2.1.2 Signed and unsigned | supported | `conversions_signed_unsigned` | |
| 3.2.1.3 Float to integer | supported | `conversions_float_to_int` | |
| 3.2.1.3 Integer to float | supported | `conversions_int_to_float` | |
| 3.2.1.3 Unsigned and float | supported | `conversions_unsigned_float` | by value, through the runtime |
| 3.2.1.5 Usual arithmetic conversions | supported | `conversions_usual_arithmetic` | |
| 3.2.1.3 Conversion on assignment | differs (step 4) | `conversions_assignment` | `int x = 2.5;` keeps 2.5: the VM stores the value's own type |
| 3.2.2.2 void | supported | `conversions_void` | |
| 3.2.2.3 Null pointer constants | planned (step 5) | `conversions_null_pointer` | pointers compared as values |

### 3.3 Expressions, 3.4 Constant expressions

| Feature | Status | Program | Notes |
|---|---|---|---|
| 3.3.1 Primary expressions | supported | `expressions_primary` | |
| 3.3.2.1 Array subscripting | supported | `expressions_array_subscript` | |
| 3.3.2.1 `i[a]` and `*(a + i)` | planned (step 5) | `expressions_subscript_commutes` | pointer arithmetic |
| 3.3.2.2 Function calls | supported | `expressions_function_calls` | recursion, arguments by value |
| 3.3.2.3 Structure and union members | planned (step 5) | `expressions_struct_members` | |
| 3.3.2.4 Postfix `++` and `--` | supported | `expressions_postfix_increment` | |
| 3.3.3.1 Prefix `++` and `--` | supported | `expressions_prefix_increment` | |
| 3.3.3.2 `&` and `*` | supported | `expressions_address_indirection` | |
| 3.3.3.3 Unary `+ - ~ !` | supported | `expressions_unary_arithmetic` | |
| 3.3.3.4 sizeof | supported | `expressions_sizeof` | of types and expressions, not evaluated |
| 3.3.4 Casts | supported | `expressions_casts` | |
| 3.3.5 `* / %` | supported | `expressions_multiplicative` | |
| 3.3.6 `+ -` | supported | `expressions_additive` | |
| 3.3.6 Pointer arithmetic | planned (step 5) | `expressions_pointer_arithmetic` | |
| 3.3.7 Shifts | supported | `expressions_shift` | |
| 3.3.8 Relational operators | supported | `expressions_relational` | |
| 3.3.9 Equality operators | supported | `expressions_equality` | |
| 3.3.10-12 `& ^ \|` | supported | `expressions_bitwise` | |
| 3.3.13-14 `&&` and `\|\|` | supported | `expressions_logical` | the right side runs only when needed |
| 3.3.13-14 Their value | differs (step 4) | `expressions_logical_value` | the deciding value, not 0 or 1 |
| 3.3.15 `?:` | supported | `expressions_conditional` | |
| 3.3.16.1 Assignment | supported | `expressions_assignment` | |
| 3.3.16.2 Compound assignment | supported | `expressions_compound_assignment` | |
| 3.3.17 Comma operator | supported | `expressions_comma` | |
| 3.4 Constant expressions | supported | `expressions_constant` | array sizes, enums, cases, `#if` |
| Natives in conditions | differs (step 4) | | a native in a condition runs before it is tested |

### 3.5 Declarations

| Feature | Status | Program | Notes |
|---|---|---|---|
| 3.5.1 auto and register | supported | `declarations_auto_register` | |
| 3.5.1 `&` of a register variable | supported | `declarations_register_address` | refused |
| 3.5.1 static locals | supported | `declarations_static_locals` | hidden globals |
| 3.5.1 Static arrays | planned (step 5) | `declarations_static_arrays` | |
| 3.5.1, 3.7.2 extern, tentative definitions | supported | `declarations_extern` | |
| 3.5.7 Initialised globals | supported | `declarations_initialised_globals` | set once per load |
| 3.5.7 Their constant expressions | supported | `declarations_global_initialiser_constant` | refused otherwise |
| 3.5.2 char, short, long, signed, unsigned | supported | `declarations_integer_types` | wrapped to width |
| 3.5.2 long long | supported | `declarations_long_long` | refused: C99 |
| 3.5.2 Implicit int | supported | `declarations_implicit_int` | |
| 3.5.2 double | limit | `declarations_double` | |
| 3.5.2.1 Structures | planned (step 5) | `declarations_struct` | |
| 3.5.2.1 Unions | planned (step 5) | `declarations_union` | |
| 3.5.2.2 Enumerations | supported | `declarations_enums` | in functions too |
| 3.5.3 const | supported | `declarations_const` | |
| 3.5.3 Writing a const object | supported | `declarations_const_checked` | refused |
| 3.5.3 Writing through a pointer to const | supported | `declarations_const_pointee` | refused |
| 3.5.3 volatile | supported | `declarations_volatile` | every access is made anyway |
| 3.5.4.1 Pointers | supported | `declarations_pointers` | to locals and parameters |
| 3.5.4.1 Pointers to pointers | planned (step 5) | `declarations_pointer_to_pointer` | |
| 3.5.4.1 Global pointers | planned (step 5) | `declarations_global_pointers` | |
| 3.5.4.2 Arrays, initializers | supported | `declarations_arrays` | the rest set to 0 |
| 3.5.4.2 Size at least 1 | supported | `declarations_array_size` | refused otherwise |
| 3.5.4.2 Arrays of arrays | supported | `declarations_multidimensional_arrays` | rows passed as pointers |
| 3.5.4.2 Global arrays | planned (step 5) | `declarations_global_arrays` | |
| 3.5.4.3 Function pointers | planned (step 5) | `declarations_function_pointers` | |
| 3.5.6 typedef | supported | `declarations_typedef` | in functions too |
| 3.5.7 char arrays from strings | supported | `declarations_char_arrays` | a character per element |
| 3.1.2.1 Block scope | supported | `declarations_block_scope` | an inner name hides an outer one |
| 3.5 One declaration in a block | supported | `declarations_redeclared` | refused |

### 3.6 Statements

| Feature | Status | Program | Notes |
|---|---|---|---|
| 3.6.1, 3.6.6.1 Labels and goto | supported | `statements_labels_goto` | |
| 3.6.6.1 goto to no label | supported | `statements_goto_undefined_label` | refused |
| 3.6.2 Blocks | supported | `statements_compound` | |
| 3.6.3 Expression and null statements | supported | `statements_expression_null` | |
| 3.6.4.1 if | supported | `statements_if` | |
| 3.6.4.2 switch | supported | `statements_switch` | |
| 3.6.4.2 A case twice | supported | `statements_switch_duplicate_case` | refused |
| 3.6.5.1 while | supported | `statements_while` | |
| 3.6.5.2 do | supported | `statements_do` | |
| 3.6.5.3 for | supported | `statements_for` | |
| 3.6.6.2-3 continue and break | supported | `statements_continue_break` | |
| 3.6.6.4 return | supported | `statements_return` | |

### 3.7 External definitions

| Feature | Status | Program | Notes |
|---|---|---|---|
| 3.5.4.3, 3.7.1 Prototypes | supported | `functions_prototypes` | unnamed parameters, `()` |
| 3.7.1 Identifier lists (K&R) | supported | `functions_kr_definitions` | |
| 3.7.1 Array parameters | supported | `functions_array_parameters` | |
| 3.1.2.2 static functions | supported | `functions_static` | |
| 2.2.4.1 31 parameters | supported | `functions_many_parameters` | |
| 4.8 Variable arguments | planned (step 5) | `functions_varargs` | |
| 3.5.4.3 Returning pointers | planned (step 5) | `functions_return_pointer` | |

### 3.8 Preprocessing directives

| Feature | Status | Program | Notes |
|---|---|---|---|
| 3.8.1 `#if` and kin | supported | `preprocessor_conditionals` | |
| 3.8.1 `#if` arithmetic | supported | `preprocessor_if_arithmetic` | C's precedence, 32-bit long |
| 3.8.1 One `#else`, last | supported | `preprocessor_else_twice` | refused otherwise |
| 3.8.2 `#include` | supported | `preprocessor_include` | |
| 3.8.2 A header's macros in `#if` | supported | `preprocessor_header_macros` | the header read as text for its macros; its constants stay names in code |
| 3.8.3 Object-like macros | supported | `preprocessor_object_macros` | |
| 3.8.3 Function-like macros | supported | `preprocessor_function_macros` | |
| 3.8.3 Defined again differently | supported | `preprocessor_redefinition` | refused |
| 3.8.3.2 `#` | supported | `preprocessor_stringify` | |
| 3.8.3.3 `##` | supported | `preprocessor_paste` | |
| 3.8.3.4 Rescanning | supported | `preprocessor_rescanning` | |
| 3.8.3.5 `#undef` | supported | `preprocessor_undef` | |
| 3.8.4 `#line` | supported | `preprocessor_line` | |
| 3.8.5 `#error` | supported | `preprocessor_error` | |
| 3.8.6-7 `#pragma`, `#` | supported | `preprocessor_pragma_null` | |
| 3.8.8 Predefined macros | supported | `preprocessor_predefined` | |

### 4.1 The freestanding headers

| Feature | Status | Program | Notes |
|---|---|---|---|
| `<limits.h>` | supported | `library_limits` | |
| `<float.h>` | supported | `library_float` | `float` only |
| `<stddef.h>` | supported | `library_stddef` | `NULL`, `size_t`, `ptrdiff_t`, `wchar_t` |
| `offsetof` | planned (step 5) | `library_offsetof` | needs structs |
| `<stdarg.h>` | planned (step 5) | `functions_varargs` | |

## How the forms the VM lacks are lowered

Done (see `src/crates/c2-script/src/lower.rs`, `parse.rs` and `cpp.rs`):

- **Macros** are expanded by the tokenizer, but a `#define` whose value is a constant (a
  number, a string, a negative number, or a parenthesised integer expression) stays a named
  constant the compiler substitutes, as the decompiler's trees need. Anything else, and every
  function-like macro, is text, expanded and rescanned as C says. Arguments are expanded in
  full, constants too, so `#` and `##` of a macro that passes one on see its value.
- **goto** is a jump to a label of the compiler's own kind (a `NOP` with its number).
- **sizeof** is worked out by the parser, from the declarations it has read.
- **Static locals** are hidden globals named after the function and the local. **Initialised
  globals** (and static locals with initializers) are set by a hidden function,
  `__initialise`, behind a hidden flag; every function the engine can start (each program's
  and the main one) calls it first. The flag makes it run once per load, and a reloaded script
  starts again from 0 as its globals do. The alternative, assignments at the top of each entry
  function without a flag, would set them again each time a program starts. The cost is one
  global, a call and a test in each entry function, in scripts that use initialised globals.
- **Arrays of arrays** are one run of slots, indexed `i * columns + j`; a row is its first
  element's address. Initializers fill them with C's brace elision, and the elements a list
  leaves out are set to 0.
- **char and short** are stored masked to their width (`x & 255`, and for signed ones offset by
  128 first and back after), on assignment, steps, compound assignment, arguments and returns.
- **unsigned int** keeps its bits in an int. `+ - * & | ^ << ==` are the same; `<` and its kin
  compare both sides plus `-2^31`; `/`, `%` and `>>` are calls of a small runtime written in
  c2s and added to a script that uses them; conversions to and from `float` too.
- **Block scope**: each local has one slot for the function; a name that hides another gets a
  slot of its own under another name.

To come, by step of the plan:

- **Step 4, C's semantics in a strict mode.** Assignment converts: the compiler knows each
  expression's type (as `lower.rs` does for the integer types), so a store of a float to an
  `int` adds `FTOI`, and of an int to a `float` `ITOF`. `&&` and `||` give 0 or 1: the
  deciding value is followed by `NOT NOT`. A native in a condition becomes a branch: `a &&
  native()` compiles as `if (a) { native(&t); result = t; }` so it runs only when C would
  evaluate it.
- **Step 5, composite types.** A struct local is a run of slots, a member a fixed offset into
  it, and a pointer to a struct a pointer to its first slot (`p->m` is `p[offset]`). A union's
  members share slots; since the VM's values carry their type, reading a union through
  another member gives the value, not its bits, which C allows to differ. Pointer arithmetic
  `p + n` is `&p[n]`, which the VM's indexed `LOAD_ADDR` gives; `p - q` and comparisons of
  pointers need a pointer's slot as a number, which the VM does not expose, so pointers into
  one array would be kept as a base and an index. Pointers to pointers copy the inner pointer
  into a hidden local and index through it. Global arrays and pointers, which the VM cannot
  index, become accessor functions over hidden globals (a `switch` on the index), or live in a
  hidden array in the frame of a function that stays running. Function pointers are numbers,
  called through a generated function that switches on them. Varargs pass the extra
  arguments in a hidden local array and its count, which `va_arg` reads. `offsetof` and wide
  string literals follow.
- **Step 6, double**: refused, by decision (above).
- **Not in a step yet**: a header's macros for `#if` and as text in the including file, which
  needs `#include` read as text rather than as declarations.

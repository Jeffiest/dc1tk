# Standard C

The aim is a conforming C89/C90 freestanding implementation (ANSI X3.159-1989, ISO/IEC
9899:1990) for code people write, on a VM that was made for a smaller language (see
[the format](../game/scripts.md)). One rule bounds it: every script on the disc still compiles to its
own bytes. Only forms the decompiler never writes take the new paths, and `dc2tk verify`
checks the whole disc.

Where the VM's semantics differ from C's (a value stored keeps its own type, `&&` gives the
deciding value, a native in a condition runs before it is tested), the decompiled scripts need the
VM's, so C's apply in **strict mode**: `#pragma std c89` before the code that wants them (see
[the language](language.md)). Everything else below holds in either mode.

Progress is measured by a conformance suite: a small program in
`src/crates/dc2tk-script/tests/c89/` for each feature, compiled in strict mode with the SDK and run
in the test VM (`cargo test -p dc2tk-script --test c89`). The test VM (`vm.rs`, which `dc2tk mod
test` uses too) runs the script's own logic as the game's VM does, but not natives (a native's
result stays 0), and it holds floats as doubles rather than the PS2's singles. It stops a run past
`vm::STEPS` (10 million) instructions ("ran more than ... steps; does it loop for ever?") or
`vm::CALLS` (10,000) nested calls ("calls nested deeper than ..."), and its errors name the
function they stopped in. Each program's second line says what it shows, and its row below says
the same (a test checks that):

- **supported**: it compiles and gives C's result, or, where C requires a diagnostic, the
  compiler gives one;
- **differs**: it compiles and gives another result, named in the program, until the step of
  [PLAN.md](../../PLAN.md)'s "Next: standard C" given;
- **planned**: it does not compile yet (or fails when run); the step given makes it work;
- **limit**: refused by design.

Today: 130 programs, 126 supported (110 giving C's result, 16 giving C's diagnostic) and 4 at
the limit (`double` twice, and comparing or subtracting two pointers); none differ or are
planned. `src/crates/dc2tk-script/tests/composite.rs` runs larger programs that use the composite
types together.

## The departures

### double

`double` and `long double` are refused: "double is not supported: the VM has 32-bit floats
only; use float". C89 asks a double for 10 significant digits, which the VM's 32-bit floats
cannot give, and software doubles would make every floating operation a call. A plain
constant such as `1.5` is a float, as it always was in the scripts; `1.5L` and the word `double`
anywhere in code are errors, naming the line. `<float.h>` describes `float` only.

### Pointers are not numbers

The VM's pointer is an address `LOAD_ADDR` gives, a value of its own that the VM indexes
through; it gives no address as a number. So two pointers neither compare (`p < q`, `p == q`)
nor subtract (`q - p`), and a pointer does not convert to an integer or back: these are refused,
naming the line. A pointer compares with a null pointer constant (`p == 0` is `!p`, true for 0 and
false for an address, as the VM tests values), so lists and trees that end in a null pointer
work. Code that needs an element's position keeps its index.

## What the implementation defines

C89 leaves these to the implementation and asks it to say what it does.

- **Types.** `char` is 8 bits and signed (as on the PS2); `short` is 16 bits; `int` and `long`
  are 32 bits, two's complement, the VM's own ints. `sizeof` gives 1, 2 and 4, 4 for `float`
  and pointers, and is a `size_t`, which is `unsigned int`; `ptrdiff_t` is `int`. A `char` or
  `short` takes a whole VM slot and is wrapped to its width when stored. An integer constant
  too large for 32 bits is refused.
- **Arithmetic.** Signed arithmetic wraps on overflow, as the game's VM does. `/` and `%`
  truncate toward zero. `>>` of a negative `int` divides by a power of 2, rounding toward
  zero (`-7 >> 1` is -3); of an unsigned value it shifts zeros in. A value out of range for a
  narrow signed type wraps (`(signed char)200` is -56). A shift by 32 or more of a constant
  count is refused.
- **Floats.** `float` is an IEEE 754 single held by the PS2's FPU, which rounds toward zero,
  flushes values below `FLT_MIN` to zero, and has no infinities or NaNs. The compiler reads a
  constant as a double and rounds that to the nearest float; one too large for a float is
  refused. See `sdk/float.h`.
- **Characters.** Source files are UTF-8; strings are stored as Shift-JIS, the game's text
  encoding, so `MB_LEN_MAX` is 2 and `L'x'` (a `wchar_t`, `unsigned short`) is the
  character's Shift-JIS code. `\x` takes two hexadecimal digits. Names are ASCII letters,
  digits and `_`, and numbers ASCII digits; any other character outside a string, a character
  constant or a comment is an error. White space between tokens is any Unicode white space (a
  full-width space too), and a line ends at LF, CRLF reading the same.
- **Startup.** A freestanding program: the engine starts the functions the `PROGRAM` table
  names, and `MAIN` names the header's main function; there is no `main()`. Objects with
  static storage (globals and static locals) are set to their initializers once each time the
  script is loaded, before the first function the engine starts runs any of its own code (see
  below).
- **Preprocessor.** `#include <...>` reads the SDK's headers (`sdk/`), `#include "..."` the
  script's folder, the folders above it, then the SDK. Pragmas are dc2tk's own (see
  [the language](language.md)); others are ignored, as C says. `__DATE__` and `__TIME__` are
  UTC, from `SOURCE_DATE_EPOCH` when it is set. `__STDC__` is 1; `__STDC_VERSION__` is not
  defined (it came with C95).
- **Limits.** Identifiers have no length limit and are case-sensitive, external ones too. A
  function can take at least 31 parameters (the VM keeps a type for the first 10 only, which
  the engine never reads). The parser counts nesting against `parse::NESTING` (200), sized so
  that it, the lowering and the compiler fit a 1 MB stack (a Windows main thread's) even in a
  debug build: parentheses take three levels each and a block, if, loop or switch two, so
  parentheses nest 64 deep, blocks, ifs, loops and switches 97, an expression's operators chain
  194 long (`a + b + ...`, `!!!...x`), a declarator nests 199 deep and a struct defined in a
  struct's members 98. Deeper is refused ("line N: nested too deeply") rather than overflowing
  the stack; `tests/nesting.rs` compiles the deepest expressions and statements on a 1 MB
  thread, in both modes.
- **register** asks nothing of the VM (every variable is a slot); taking a register
  variable's address is refused, as C requires.
- **Pointers** are the VM's addresses, all of one size, and a null pointer is the int 0. A
  cast between pointer types passes the address as it is. Pointer arithmetic counts slots: an
  element of any scalar type, a pointer and a function pointer take one, a struct its members'.
- **Structs and unions** hold their members in order in whole slots; a union's members share
  them. `sizeof` and `offsetof` count C's bytes as the types above define them, each member
  aligned to its size (to 4 at most) and a struct to its largest member's, so `sizeof(struct
  { char c; int i; })` is 8 although the struct takes two slots. A struct's tag declared in a
  block is the block's, hiding one outside until the block ends.
- **Bit-fields** are `int` (signed), `signed int` or `unsigned int`. They share an int slot,
  allocated from its lowest bit; one that does not fit in the bits the slot has left starts the
  next slot (none straddles two), as does one after a member that is not a bit-field, or after
  `:0`. A slot of bit-fields counts 4 bytes for `sizeof`, and a 32-bit field is an ordinary int
  member. Reading one is `(slot & mask) >> offset` (and spreading its sign); storing one keeps the
  slot's other bits, so a value wraps to the field's width.
- **Function pointers** hold the function's number, from 1 in the order the program takes the
  functions' addresses, for every function: one that takes `...` or a struct, or returns one,
  too.
- **Variable arguments** after `...` are passed as they are: a `char` or `short` as an int, a
  `float` as a float (there is no `double`). `va_arg` of `char` or `short` reads the int and
  converts it back, as the default argument promotions have it.
- **String literals** are the VM's strings, the values natives and `string` variables take. A
  literal that is subscripted or dereferenced, or stored in, passed as or returned as a `char`
  pointer, is an array of its characters and a 0 instead, as C has it, kept for as long as the
  script runs (`char *p = "abc";`); `char s[] = "abc";` fills an array of its own.
- **Wide strings** are arrays of `wchar_t` holding each character's Shift-JIS code, then 0.

## The features

### 3.1 Lexical elements

| Feature | Status | Program | Notes |
|---|---|---|---|
| 3.1.1 Keywords | supported | `lexical_keywords` | all 32 but `double` (limit) |
| 3.1.2 Identifiers | supported | `lexical_identifiers` | |
| 3.1.3.1 Floating constants | supported | `lexical_floating_constants` | `.5`, `2.`, exponents, `f` |
| 3.1.3.1 The `l` suffix | limit | `lexical_long_double_constants` | a `long double` |
| 3.1.3.2 Integer constants | supported | `lexical_integer_constants` | octal, hexadecimal, `u` and `l` |
| 3.1.3.2 Their types | supported | `lexical_unsigned_constants` | too large for `int`, or `u`: unsigned |
| 3.1.3.4 Character constants | supported | `lexical_character_constants` | every escape, octal and hex |
| 3.1.3.4 Wide characters | supported | `lexical_wide_characters` | Shift-JIS codes |
| 3.1.4 String literals | supported | `lexical_string_literals` | adjacent literals are one |
| 3.1.4 String literals as arrays | supported | `lexical_string_literal_arrays` | the VM's string, but an array of `char` (a hidden global) where C needs one |
| 3.1.4 Wide strings | supported | `lexical_wide_strings` | a hidden local array of the codes, set before the statement |
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
| 3.2.1.3 Conversion on assignment | supported | `conversions_assignment` | strict mode: `FTOI` or `ITOF` where the types differ; arguments, returns and `op=` too |
| 3.2.2.2 void | supported | `conversions_void` | |
| 3.2.2.3 Null pointer constants | supported | `conversions_null_pointer` | `p == 0` is `!p`, `p != 0` is `!!p` |
| 3.3.16.1 An integer as a pointer | supported | `conversions_pointer_integer` | strict mode: refused |
| 3.3.16.1 Incompatible pointers | supported | `conversions_incompatible_pointers` | strict mode: refused |

### 3.3 Expressions, 3.4 Constant expressions

| Feature | Status | Program | Notes |
|---|---|---|---|
| 3.3.1 Primary expressions | supported | `expressions_primary` | |
| 3.3.2.1 Array subscripting | supported | `expressions_array_subscript` | |
| 3.3.2.1 `i[a]` and `*(a + i)` | supported | `expressions_subscript_commutes` | `p + n` is `&p[n]` |
| 3.3.2.2 Function calls | supported | `expressions_function_calls` | recursion, arguments by value |
| 3.3.2.3 Structure and union members | supported | `expressions_struct_members` | a member is a constant offset in slots |
| 3.3.2.2, 3.6.6.4 Structures as arguments and results | supported | `functions_struct_arguments` | by address, copied by the callee; returned through a hidden pointer |
| 3.3.2.4 Postfix `++` and `--` | supported | `expressions_postfix_increment` | |
| 3.3.3.1 Prefix `++` and `--` | supported | `expressions_prefix_increment` | |
| 3.3.3.2 `&` and `*` | supported | `expressions_address_indirection` | |
| 3.3.3.3 Unary `+ - ~ !` | supported | `expressions_unary_arithmetic` | |
| 3.3.3.4 sizeof | supported | `expressions_sizeof` | of types and expressions, not evaluated |
| 3.3.4 Casts | supported | `expressions_casts` | |
| 3.3.5 `* / %` | supported | `expressions_multiplicative` | |
| 3.3.6 `+ -` | supported | `expressions_additive` | |
| 3.3.6 Pointer arithmetic | supported | `expressions_pointer_arithmetic` | counts slots; `p++` keeps the old value in a hidden local |
| 3.3.6 The difference of two pointers | limit | `expressions_pointer_difference` | the VM gives no address as a number |
| 3.3.8-9 Comparing two pointers | limit | `expressions_pointer_comparison` | the same; a pointer compares with a null pointer only |
| 3.3.7 Shifts | supported | `expressions_shift` | |
| 3.3.8 Relational operators | supported | `expressions_relational` | |
| 3.3.9 Equality operators | supported | `expressions_equality` | |
| 3.3.10-12 `& ^ \|` | supported | `expressions_bitwise` | |
| 3.3.13-14 `&&` and `\|\|` | supported | `expressions_logical` | the right side runs only when needed |
| 3.3.13-14 Their value | supported | `expressions_logical_value` | strict mode: `NOT NOT` where the value is used |
| 3.3.15 `?:` | supported | `expressions_conditional` | |
| 3.3.15 `?:` of structs | supported | `expressions_conditional_structs` | one of the two structs' addresses |
| 3.3.16.1 Assignment | supported | `expressions_assignment` | |
| 3.3.16.2 Compound assignment | supported | `expressions_compound_assignment` | |
| 3.3.17 Comma operator | supported | `expressions_comma` | |
| 3.4 Constant expressions | supported | `expressions_constant` | array sizes, enums, cases, `#if` |
| Natives in conditions | supported | | strict mode: a native C may not evaluate runs in an `if` (tested in `c_features.rs`) |

### 3.5 Declarations

| Feature | Status | Program | Notes |
|---|---|---|---|
| 3.5.1 auto and register | supported | `declarations_auto_register` | |
| 3.5.1 `&` of a register variable | supported | `declarations_register_address` | refused |
| 3.5.1 static locals | supported | `declarations_static_locals` | hidden globals |
| 3.5.1 Static arrays | supported | `declarations_static_arrays` | hidden global arrays |
| 3.5.1, 3.7.2 extern, tentative definitions | supported | `declarations_extern` | |
| 3.5.7 Initialised globals | supported | `declarations_initialised_globals` | set once per load |
| 3.5.7 Their constant expressions | supported | `declarations_global_initialiser_constant` | refused otherwise |
| 3.5.2 char, short, long, signed, unsigned | supported | `declarations_integer_types` | wrapped to width |
| 3.5.2 long long | supported | `declarations_long_long` | refused: C99 |
| 3.5.2 Implicit int | supported | `declarations_implicit_int` | |
| 3.5.2 double | limit | `declarations_double` | |
| 3.5.2.1 Structures | supported | `declarations_struct` | their members' slots, copied slot by slot |
| 3.5.2.1 Bit-fields | supported | `declarations_bit_fields` | masks on an int slot they share |
| 3.5.2.1 A bit-field's width | supported | `declarations_bit_field_width` | more than an int's 32 bits: refused |
| 3.3.3.2 `&` of a bit-field | supported | `declarations_bit_field_address` | refused |
| 3.5.2.1 Unions | supported | `declarations_union` | the members share slots; another member reads the value, not its bits |
| 3.5.2.2 Enumerations | supported | `declarations_enums` | in functions too |
| 3.5.3 const | supported | `declarations_const` | |
| 3.5.3 Writing a const object | supported | `declarations_const_checked` | refused |
| 3.5.3 Writing through a pointer to const | supported | `declarations_const_pointee` | refused |
| 3.5.3 volatile | supported | `declarations_volatile` | every access is made anyway |
| 3.5.4.1 Pointers | supported | `declarations_pointers` | to locals and parameters |
| 3.5.4.1 Pointers to pointers | supported | `declarations_pointer_to_pointer` | through a hidden local each level |
| 3.5.4.1 Global pointers | supported | `declarations_global_pointers` | address constants as initializers |
| 3.5.4.2 Arrays, initializers | supported | `declarations_arrays` | the rest set to 0 |
| 3.5.4.2 Size at least 1 | supported | `declarations_array_size` | refused otherwise |
| 3.5.4.2 Arrays of arrays | supported | `declarations_multidimensional_arrays` | rows passed as pointers |
| 3.5.4.2 Global arrays | supported | `declarations_global_arrays` | a constant index is the global slot itself, another goes through the array's address |
| 3.5.4.3 Function pointers | supported | `declarations_function_pointers` | the function's number, called through `__call_N` |
| 3.5.6 typedef | supported | `declarations_typedef` | in functions too |
| 3.5.7 char arrays from strings | supported | `declarations_char_arrays` | a character per element |
| 3.1.2.1 Block scope | supported | `declarations_block_scope` | an inner name hides an outer one |
| 3.1.2.1 Tags in blocks | supported | `declarations_struct_tag_scope` | a struct of the block's, under another tag |
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
| 4.8 Variable arguments | supported | `functions_varargs` | a hidden array in the caller's frame |
| 4.8.1.2 `va_arg` of char and short | supported | `functions_varargs_promoted` | the int read and converted |
| 3.5.4.3 Pointers to any function | supported | `functions_pointers_composite` | structs and `...` passed as a direct call passes them |
| 3.5.4.3 Returning pointers | supported | `functions_return_pointer` | |

### 3.8 Preprocessing directives

| Feature | Status | Program | Notes |
|---|---|---|---|
| 3.8.1 `#if` and kin | supported | `preprocessor_conditionals` | |
| 3.8.1 `#if` arithmetic | supported | `preprocessor_if_arithmetic` | C's precedence, 32-bit long |
| 3.8.1 One `#else`, last | supported | `preprocessor_else_twice` | refused otherwise |
| 3.8.2 `#include` | supported | `preprocessor_include` | |
| 3.8.2 A header's macros in `#if` | supported | `preprocessor_header_macros` | the header read as text for its macros; its constants stay names in code |
| 3.8.2 A header's macros in constant expressions | supported | `preprocessor_header_constants` | array sizes, enumerators, case labels, initialisers: an SDK header's, a library's or a mod's own header included with quotes |
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
| A header's typedefs as type names | supported | `library_header_typedefs` | where a declaration or cast has them |
| `offsetof` | supported | `library_offsetof` | C's bytes |
| `<stdarg.h>` | supported | `functions_varargs` | `va_list` points into the caller's array |

## How the forms the VM lacks are lowered

Done (see `src/crates/dc2tk-script/src/lower.rs`, `parse/` and `cpp.rs`):

- **Macros** are expanded by the tokenizer, but a `#define` whose value is a constant (a
  number, a string, a negative number, or a parenthesised integer expression) stays a named
  constant the compiler substitutes, as the decompiler's trees need. Anything else, and every
  function-like macro, is text, expanded and rescanned as C says. Arguments are expanded in
  full, constants too, so `#` and `##` of a macro that passes one on see its value. A
  constant a header or library defines (an SDK header, a library, or a mod's own header
  included with quotes) has its value wherever C needs one while parsing (array sizes,
  enumerators, case labels, `#if`), read with the macros in force where it is included. In
  code it stays a name, given the value the header or library has read on its own (a library
  is read once, as if it had `#pragma once`): the two differ only for a constant the header
  defines under an `#if` of a macro the including file defines first.
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
  script C and added to a script that uses them; conversions to and from `float` too.
- **Block scope**: each local has one slot for the function; a name that hides another gets a
  slot of its own under another name.

- **A header's macros** (`#include <...>`) are read from the header's text from the macros
  before it, so they reach `#if`; its declarations are read apart as before, and its constants
  stay names in code. Its typedefs are type words (`size_t n;`, `size_t f(size_t)`,
  `(size_t)x`, `int f(size_t, size_t);`) where a name, `*` or `)` follows (or `,` in a
  parameter list), unless the file declares the name as a variable, enumerator or constant:
  the SDK's value kinds (`flag`, `message`) are names scripts use for variables too.
- **Strict mode** (`lower.rs`, `sdk/resolve.rs`): a store, argument or return converts with `FTOI` or
  `ITOF` (or the unsigned runtime) where the types differ, constants folded; `&&` and `||`
  whose value is used are followed by `NOT NOT`; and in `a && f()`, `a || f()`, `c ? f() : x`
  and `a, f()` a native `f` is called in an `if` on a hidden local (`t = !!a; if (t) { f(&v);
  t = !!v; }`), so it runs only when C would evaluate it.

### Pointers and composite types

`composite.rs` types every expression and lowers C's pointers and composite types to the VM's
slots, after the initialised globals and before the integer types; the forms the compiler had
before come out as they went in, so retail scripts keep their bytes.

- **A place** (an lvalue) is a variable, a slot of a variable's storage at a constant offset
  (the VM's plain local or global mode, with no index), a slot of a local's storage or through
  a pointer local at an index (the indexed modes), or a slot through a pointer an expression
  gives, which is stored in a hidden local first since the VM indexes through a local only.
  Each carries its slot's type, for the float modes.
- **Pointer arithmetic**: `p + n` is `&p[n]`, `p[i]` and `*(p + i)` are one place, `i[a]` is
  `a[i]`; `p++` keeps the old pointer in a hidden local and stores `&p[1]`.
- **Global arrays, static arrays and global pointers** use the globals' place in the VM's
  value stack: the globals are its bottom slots and the frames sit above them, so the address
  of a global indexes like a local's. An element at a constant index is the global slot itself
  (`load global 5`, as cheap as a scalar global); at a variable index it is reached through the
  array's address in a hidden local (`addr local t; addr global 2; store; pop; <index>; load
  ptr_idx t`, five instructions besides the index's). Accessor functions over hidden globals,
  the other way the plan named, would cost a call and on average a compare and a jump per
  element before the wanted one (about 40 instructions run per read of a 16-element array, and
  a function of that size per array), and could not give an element's address, which passing a
  global array to a function needs. A global's initializer may be an address constant
  (`int *p = &g;`) or a function's name.
- **Pointers to pointers** read each level into a hidden local and index through it.
- **Structs and unions** are their members' slots, a member a constant offset. A struct
  assignment copies slot by slot, each slot as it is (its address worked out once). A struct
  argument is passed by address and the function copies it into a hidden local first, so the
  caller's stays as it was; a struct result is written through a hidden first parameter,
  `__result`, that points at a hidden local of the caller's. Initializers follow C's brace
  elision, and set what they leave out to 0.
- **Function pointers** are numbers. Each call through one calls `__call_N` for the number of
  arguments the function takes in the VM, a function the compiler adds that switches on the
  number to the function, passing the arguments on as they are. A call through a pointer passes
  what a direct call passes: a struct argument's address, the address of the caller's hidden
  local for a struct result first, and the hidden array of the arguments after `...` last.
- **Bit-fields** are bits of an int slot: a read is `(slot & mask) >> offset`, then
  `(v ^ top) - top` for a signed field (the bits below the field are 0, so the VM's `>>`, a
  division, is exact); a store is `slot = (slot & ~mask) | ((value << offset) & mask)`, with the slot's
  address worked out once, and gives the field read back. `f.b++` keeps the old value in a hidden
  local. An initializer puts the fields that share a slot together in one value.
- **`?:` of structs** gives the address of one of the two, which is copied or reached through
  as any struct's value is.
- **Tags in blocks**: a struct declared in a block gets a tag of its own (`T#...`), which the
  block's uses of `T` name until the block ends; messages show it as `T`. An unnamed struct's
  made-up tag carries a hash of its file's text, so two files' (two mods' patches, a script and
  its library) never share one.
- **Variadic functions** take a hidden last parameter, `__va`, pointing at a hidden array in the
  caller's frame that holds the arguments after the named ones; `<stdarg.h>`'s `va_list` is a
  pointer into it, `va_start` sets it from `__va` and `va_arg` steps it.
- **String literals as arrays and wide strings** become a hidden global array of their codes,
  set before the statement that uses it (the literal's storage lasts the program, so a function
  may return it).
- **In strict mode** C's diagnostics for assigning an integer to a pointer or a pointer to an
  integer, and for pointers to different types, are given (for assignment, initializers,
  arguments and returns).

The globals-are-addressable route rests on how the game's VM keeps pointers (an address in the
value stack, as its natives write through pointers to globals and locals alike); the test VM
models it, and it is the one assumption here not yet tried in the game.

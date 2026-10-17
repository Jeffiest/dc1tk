# C89 standard support

The `dc2tk` script compiler provides a conforming C89/C90 freestanding implementation (ANSI X3.159-1989, ISO/IEC 9899:1990) for custom script code, targeting the game's bytecode VM (see [script bytecode format](../../docs/internals/scripts.md)).

One design rule governs the compiler: every decompiled script from the retail disc must recompile to its exact original bytes. Running `dc2tk verify` checks byte-for-byte round-tripping across the entire retail disc.

Every script is compiled with C's semantics. Where a retail script relies on the VM's own (a slot keeps the type of the value stored in it, and `&&` and `||` leave the operand that decided them), the decompiler writes `(any)`, which keeps the value as the VM has it, so the script still compiles to its own bytes (`lower::kept_as_is`; see [language.md](../../docs/scripting/language.md)). A native's `int` and `float` arguments are passed as they are: the engine's `GetStackInt` and `GetStackFloat` convert them as C would.

## Conformance test suite

An automated conformance test suite in `src/crates/dc2tk-script/tests/c89/` verifies each C89 feature (`cargo test -p dc2tk-script --test c89`). Each test file is compiled with the SDK headers and executed in the test VM (`vm.rs`, also used by `dc2tk mod test`).

The test VM executes all script bytecode instructions identically to the game VM, with two differences:

- Native calls do not execute engine code (native return values remain `0`).
- Floating-point slots use host `f64` values rather than the PS2 FPU's 32-bit floats.

To catch infinite loops and unbounded recursion, the test VM halts execution after `vm::STEPS` (`10,000,000` instructions) or `vm::CALLS` (`10,000` nested calls) and reports the active function name.

Each test program declares its expected status on line 2, matching the tables below:

- **supported**: The test compiles and returns the expected C89 result, or produces a required compiler diagnostic.
- **differs**: The test compiles and returns a different result than standard C.
- **planned**: The feature is not yet implemented.
- **limit**: The feature is intentionally rejected due to VM architecture constraints.

Out of 130 test programs in the suite, 126 are **supported** (110 runtime result checks and 16 diagnostic checks) and 4 are **limit** checks (`double` and numeric pointer subtraction/comparison). No features have status **differs** or **planned**. Additional multi-feature integration tests live in `src/crates/dc2tk-script/tests/composite.rs`.

## Intentional departures from C89

### No `double` or `long double`

The compiler rejects `double` and `long double` with the error: `"double is not supported: the VM has 32-bit floats only; use float"`. C89 requires `double` to provide at least 10 decimal digits of precision, which 32-bit floats cannot represent, and emulating 64-bit floats in software would turn every arithmetic operation into a function call.

Unsuffixed floating-point literals such as `1.5` have type `float`. Using the `L` suffix on a floating-point literal (`1.5L`) or using the `double` keyword produces a compile error with the source line number. `<float.h>` defines limits for `float` only.

### Pointers are not numeric addresses

In the game VM, a pointer is an abstract slot reference created by the `LOAD_ADDR` instruction, not a numeric byte address. Consequently:

- Two pointers cannot be compared with relational or equality operators (`p < q`, `p == q`).
- Two pointers cannot be subtracted (`q - p`).
- Pointers cannot be cast to integers or constructed from arbitrary integers.

The compiler rejects all of these operations with the source line number.

Pointers can be compared against a null pointer constant (`p == 0` compiles to `!p`, which evaluates to true for integer `0` and false for a slot address), allowing null-terminated linked lists and trees. To track relative positions in an array, store an integer index alongside or instead of a pointer.

## Implementation-defined behavior

C89 requires a freestanding implementation to document the following behaviors:

- **Integer types and sizes**:
  - `char` is 8-bit signed (`signed char`), matching the PS2 ABI.
  - `short` is 16-bit signed.
  - `int` and `long` are 32-bit signed two's-complement integers (the VM's native integer slot type).
  - `sizeof` returns `1` for `char`, `2` for `short`, and `4` for `int`, `long`, `float`, and all pointer types.
  - `size_t` is `unsigned int`, and `ptrdiff_t` is `int`.
  - Each `char` or `short` variable occupies one VM slot and wraps modulo its bit width when stored.
  - Integer literals that exceed 32 bits are rejected at compile time.
- **Integer arithmetic and bitwise shifts**:
  - Signed integer overflow wraps modulo $2^{32}$, matching the game VM.
  - Division (`/`) and remainder (`%`) truncate toward zero.
  - Right-shifting (`>>`) a negative signed `int` divides by a power of `2` and rounds toward zero (for example, `-7 >> 1` yields `-3`).
  - Right-shifting an `unsigned` integer shifts in zero bits.
  - Converting an out-of-range integer to a narrower signed type wraps modulo the target width (for example, `(signed char)200` yields `-56`).
  - Constant shift counts of `32` or greater are rejected at compile time.
- **Floating-point representation**:
  - `float` is a 32-bit IEEE 754 single-precision value executed on the PS2 FPU, which rounds toward zero, flushes subnormal numbers below `FLT_MIN` to zero, and does not support hardware infinity or `NaN`.
  - The compiler parses floating-point literals as IEEE 754 double-precision and rounds them to the nearest 32-bit `float`. Literals exceeding `FLT_MAX` are rejected. See `sdk/float.h`.
- **Source and execution character sets**:
  - Source files are UTF-8.
  - String literals compile to Shift-JIS (the game's text encoding). Therefore, `MB_LEN_MAX` is `2`, and wide character constants (`L'x'`, of type `wchar_t` / `unsigned short`) evaluate to their Shift-JIS code value.
  - Hexadecimal character escapes (`\xNN`) read up to two hexadecimal digits.
  - Identifiers consist of ASCII letters, digits, and `_`. Any non-ASCII character outside a string literal, character constant, or comment is a compile error.
  - Any Unicode whitespace character (including full-width space) is accepted as token whitespace. Both LF and CRLF line endings are supported.
- **Program startup**:
  - Scripts run in a freestanding environment without `main()`. The engine starts entry points listed in the `PROGRAM` table, and `MAIN` specifies the `.stb` header's default entry point.
  - Static storage objects (initialized globals and `static` locals) initialize once per script load, before the first started entry function runs its body.
- **Preprocessor**:
  - `#include <...>` searches `sdk/`.
  - `#include "..."` searches the current script's directory, parent directories up to the root, and then `sdk/`.
  - Recognized `#pragma` directives are documented in [language.md](../../docs/scripting/language.md); unknown pragmas are ignored.
  - `__DATE__` and `__TIME__` use UTC (from `SOURCE_DATE_EPOCH` when set).
  - `__STDC__` is defined as `1`. `__STDC_VERSION__` is undefined (introduced in C95).
- **Translation limits**:
  - Identifiers have no length limit and are case-sensitive.
  - Functions accept at least 31 parameters (the `.stb` header records types for only the first 10 parameters, which the engine ignores).
  - The parser enforces a maximum nesting depth of `parse::NESTING` (`200`) so parsing, lowering, and compilation fit within a 1 MB thread stack even in debug builds. Specifically, parentheses can nest 64 levels deep, blocks/conditionals/loops/switches 97 levels deep, binary/unary operator chains 194 levels deep, declarators 199 levels deep, and nested struct definitions 98 levels deep. Exceeding this limit produces a `"line N: nested too deeply"` error rather than a stack overflow.
- **`register` storage class**:
  - All local variables reside in VM slots; `register` is accepted and prevents taking the variable's address with `&`.
- **Pointers**:
  - All pointers occupy one VM slot, and a null pointer is represented by integer `0`.
  - Casting between pointer types preserves the underlying VM slot address.
  - Pointer arithmetic advances by VM slot counts: scalars, pointers, and function pointers occupy 1 slot, and structs occupy the sum of their member slots.
- **Structures and unions**:
  - Struct members occupy sequential VM slots; union members share the same starting slots.
  - `sizeof` and `offsetof` compute standard C byte sizes and alignments (each member aligned to its byte size up to 4 bytes, and a struct aligned to its largest member), so `sizeof(struct { char c; int i; })` evaluates to `8` even though the struct occupies 2 VM slots.
  - A struct tag declared inside a block is scoped to that block and shadows outer tags until the block ends.
- **Bit-fields**:
  - Supported bit-field types are `int` (signed), `signed int`, and `unsigned int`.
  - Adjacent bit-fields pack into a shared 32-bit `int` slot from the least significant bit upward. A bit-field that does not fit in the remaining bits of the current slot, or that follows `:0` or a non-bit-field member, starts a new slot.
  - Each packed bit-field slot counts as 4 bytes for `sizeof`.
  - Reading a bit-field extracts `(slot & mask) >> offset` (with sign extension for signed bit-fields); writing updates only the masked bits in the slot.
- **Function pointers**:
  - Function pointers store a 1-based integer function ID assigned in the order function addresses are taken. Function pointers work with all function signatures, including variadic functions and functions that take or return structs.
- **Variadic arguments (`<stdarg.h>`)**:
  - Arguments passed to `...` undergo default argument promotions (`char` and `short` pass as `int`; `float` passes as `float`). `va_arg` reads the slot and narrows `char` or `short` back to its width.
- **String literals and wide string literals**:
  - String literals passed to natives or `string` variables compile to native VM strings.
  - String literals that are indexed, dereferenced, or assigned/passed/returned as `char *` (`char *p = "abc";`) compile to null-terminated static `char` arrays in global storage. Initializing `char s[] = "abc";` copies the character bytes into the local or global array.
  - Wide string literals (`L"..."`) compile to `wchar_t` arrays containing Shift-JIS character codes followed by `0`.

## C89 feature support tables

### 3.1 Lexical elements

| Feature | Status | Program | Notes |
|---|---|---|---|
| 3.1.1 Keywords | supported | `lexical_keywords` | All 32 keywords except `double` (limit) |
| 3.1.2 Identifiers | supported | `lexical_identifiers` | |
| 3.1.3.1 Floating constants | supported | `lexical_floating_constants` | `.5`, `2.`, exponents, `f`/`F` suffix |
| 3.1.3.1 The `l` suffix | limit | `lexical_long_double_constants` | Rejects `long double` constants |
| 3.1.3.2 Integer constants | supported | `lexical_integer_constants` | Decimal, octal, hexadecimal, `u` and `l` suffixes |
| 3.1.3.2 Their types | supported | `lexical_unsigned_constants` | Constants exceeding `INT_MAX` or with `u` suffix are unsigned |
| 3.1.3.4 Character constants | supported | `lexical_character_constants` | Standard, octal, and hexadecimal escape sequences |
| 3.1.3.4 Wide characters | supported | `lexical_wide_characters` | Shift-JIS character codes |
| 3.1.4 String literals | supported | `lexical_string_literals` | Adjacent string literals concatenate |
| 3.1.4 String literals as arrays | supported | `lexical_string_literal_arrays` | Lowered to a hidden global `char` array when indexed or used as `char *` |
| 3.1.4 Wide strings | supported | `lexical_wide_strings` | Lowered to a hidden `wchar_t` array of Shift-JIS codes |
| 3.1.9 Comments | supported | `lexical_comments` | Supports `/* ... */` and `//` line comments |
| 5.2.1.1 Trigraphs | supported | `lexical_trigraphs` | Expanded outside comments |

### 3.2 Conversions

| Feature | Status | Program | Notes |
|---|---|---|---|
| 3.2.1.1 Integer promotion | supported | `conversions_integer_promotion` | |
| 3.2.1.2 Signed and unsigned | supported | `conversions_signed_unsigned` | |
| 3.2.1.3 Float to integer | supported | `conversions_float_to_int` | |
| 3.2.1.3 Integer to float | supported | `conversions_int_to_float` | |
| 3.2.1.3 Unsigned and float | supported | `conversions_unsigned_float` | Converted via helper runtime functions |
| 3.2.1.5 Usual arithmetic conversions | supported | `conversions_usual_arithmetic` | |
| 3.2.1.3 Conversion on assignment | supported | `conversions_assignment` | `FTOI` or `ITOF` on assignment, argument pass (to the script's functions), return, and `op=`; `(any)` keeps a value as it is |
| 3.2.2.2 void | supported | `conversions_void` | |
| 3.2.2.3 Null pointer constants | supported | `conversions_null_pointer` | `p == 0` compiles to `!p`; `p != 0` compiles to `!!p` |
| 3.3.16.1 An integer as a pointer | supported | `conversions_pointer_integer` | Rejected |
| 3.3.16.1 Incompatible pointers | supported | `conversions_incompatible_pointers` | Rejected |

### 3.3 Expressions, 3.4 Constant expressions

| Feature | Status | Program | Notes |
|---|---|---|---|
| 3.3.1 Primary expressions | supported | `expressions_primary` | |
| 3.3.2.1 Array subscripting | supported | `expressions_array_subscript` | |
| 3.3.2.1 `i[a]` and `*(a + i)` | supported | `expressions_subscript_commutes` | `p + n` lowers to `&p[n]` |
| 3.3.2.2 Function calls | supported | `expressions_function_calls` | Supports recursion and pass-by-value arguments |
| 3.3.2.3 Structure and union members | supported | `expressions_struct_members` | Member access resolves to a constant slot offset |
| 3.3.2.2, 3.6.6.4 Structures as arguments and results | supported | `functions_struct_arguments` | Passed by address and copied by callee; returned via hidden pointer parameter |
| 3.3.2.4 Postfix `++` and `--` | supported | `expressions_postfix_increment` | |
| 3.3.3.1 Prefix `++` and `--` | supported | `expressions_prefix_increment` | |
| 3.3.3.2 `&` and `*` | supported | `expressions_address_indirection` | |
| 3.3.3.3 Unary `+ - ~ !` | supported | `expressions_unary_arithmetic` | |
| 3.3.3.4 sizeof | supported | `expressions_sizeof` | Computed at compile time without evaluating the operand |
| 3.3.4 Casts | supported | `expressions_casts` | |
| 3.3.5 `* / %` | supported | `expressions_multiplicative` | |
| 3.3.6 `+ -` | supported | `expressions_additive` | |
| 3.3.6 Pointer arithmetic | supported | `expressions_pointer_arithmetic` | Advances by slot count; `p++` saves the prior pointer in a hidden local |
| 3.3.6 The difference of two pointers | limit | `expressions_pointer_difference` | Rejects pointer subtraction because VM pointers are not numeric |
| 3.3.8-9 Comparing two pointers | limit | `expressions_pointer_comparison` | Rejects pointer comparisons except against null pointer constants |
| 3.3.7 Shifts | supported | `expressions_shift` | |
| 3.3.8 Relational operators | supported | `expressions_relational` | |
| 3.3.9 Equality operators | supported | `expressions_equality` | |
| 3.3.10-12 `& ^ \|` | supported | `expressions_bitwise` | |
| 3.3.13-14 `&&` and `\|\|` | supported | `expressions_logical` | Short-circuit evaluation |
| 3.3.13-14 Their value | supported | `expressions_logical_value` | A used value is normalized with `NOT NOT`; `(any)(a && b)` keeps the VM's |
| 3.3.15 `?:` | supported | `expressions_conditional` | |
| 3.3.15 `?:` of structs | supported | `expressions_conditional_structs` | Selects the address of the chosen struct |
| 3.3.16.1 Assignment | supported | `expressions_assignment` | |
| 3.3.16.2 Compound assignment | supported | `expressions_compound_assignment` | |
| 3.3.17 Comma operator | supported | `expressions_comma` | |
| 3.4 Constant expressions | supported | `expressions_constant` | Evaluated for array sizes, enums, `case` labels, and `#if` |
| Natives in conditions | supported | | Short-circuited native calls run in an `if` (tested in `c_features.rs`) |

### 3.5 Declarations

| Feature | Status | Program | Notes |
|---|---|---|---|
| 3.5.1 auto and register | supported | `declarations_auto_register` | |
| 3.5.1 `&` of a register variable | supported | `declarations_register_address` | Rejected at compile time |
| 3.5.1 static locals | supported | `declarations_static_locals` | Lowered to hidden globals |
| 3.5.1 Static arrays | supported | `declarations_static_arrays` | Lowered to hidden global arrays |
| 3.5.1, 3.7.2 extern, tentative definitions | supported | `declarations_extern` | |
| 3.5.7 Initialised globals | supported | `declarations_initialised_globals` | Initialized once per script load |
| 3.5.7 Their constant expressions | supported | `declarations_global_initialiser_constant` | Rejects non-constant global initializers |
| 3.5.2 char, short, long, signed, unsigned | supported | `declarations_integer_types` | Masked and wrapped to target bit width |
| 3.5.2 long long | supported | `declarations_long_long` | Rejected (C99 feature) |
| 3.5.2 Implicit int | supported | `declarations_implicit_int` | |
| 3.5.2 double | limit | `declarations_double` | Rejected (32-bit floats only) |
| 3.5.2.1 Structures | supported | `declarations_struct` | Stored in consecutive slots and copied slot by slot |
| 3.5.2.1 Bit-fields | supported | `declarations_bit_fields` | Packed using bitmasks on shared `int` slots |
| 3.5.2.1 A bit-field's width | supported | `declarations_bit_field_width` | Rejects bit-fields wider than 32 bits |
| 3.3.3.2 `&` of a bit-field | supported | `declarations_bit_field_address` | Rejected at compile time |
| 3.5.2.1 Unions | supported | `declarations_union` | Members share slots |
| 3.5.2.2 Enumerations | supported | `declarations_enums` | Supported at file scope and block scope |
| 3.5.3 const | supported | `declarations_const` | |
| 3.5.3 Writing a const object | supported | `declarations_const_checked` | Rejected at compile time |
| 3.5.3 Writing through a pointer to const | supported | `declarations_const_pointee` | Rejected at compile time |
| 3.5.3 volatile | supported | `declarations_volatile` | All variable reads and writes access the VM slot directly |
| 3.5.4.1 Pointers | supported | `declarations_pointers` | Pointers to locals, globals, and parameters |
| 3.5.4.1 Pointers to pointers | supported | `declarations_pointer_to_pointer` | Dereferenced through a hidden local per indirection level |
| 3.5.4.1 Global pointers | supported | `declarations_global_pointers` | Supports address constants as initializers |
| 3.5.4.2 Arrays, initializers | supported | `declarations_arrays` | Trailing omitted elements initialized to `0` |
| 3.5.4.2 Size at least 1 | supported | `declarations_array_size` | Rejects zero or negative array sizes |
| 3.5.4.2 Arrays of arrays | supported | `declarations_multidimensional_arrays` | Stored flat; rows decay to pointers |
| 3.5.4.2 Global arrays | supported | `declarations_global_arrays` | Constant indices access global slots directly; variable indices use the array's address |
| 3.5.4.3 Function pointers | supported | `declarations_function_pointers` | Stored as function IDs and called via `__call_N` |
| 3.5.6 typedef | supported | `declarations_typedef` | Supported at file scope and block scope |
| 3.5.7 char arrays from strings | supported | `declarations_char_arrays` | One character code per element |
| 3.1.2.1 Block scope | supported | `declarations_block_scope` | Inner declarations shadow outer declarations |
| 3.1.2.1 Tags in blocks | supported | `declarations_struct_tag_scope` | Block-scoped struct tags receive unique internal names |
| 3.5 One declaration in a block | supported | `declarations_redeclared` | Rejects duplicate declarations in the same block |

### 3.6 Statements

| Feature | Status | Program | Notes |
|---|---|---|---|
| 3.6.1, 3.6.6.1 Labels and goto | supported | `statements_labels_goto` | |
| 3.6.6.1 goto to no label | supported | `statements_goto_undefined_label` | Rejected at compile time |
| 3.6.2 Blocks | supported | `statements_compound` | |
| 3.6.3 Expression and null statements | supported | `statements_expression_null` | |
| 3.6.4.1 if | supported | `statements_if` | |
| 3.6.4.2 switch | supported | `statements_switch` | |
| 3.6.4.2 A case twice | supported | `statements_switch_duplicate_case` | Rejects duplicate `case` values |
| 3.6.5.1 while | supported | `statements_while` | |
| 3.6.5.2 do | supported | `statements_do` | |
| 3.6.5.3 for | supported | `statements_for` | |
| 3.6.6.2-3 continue and break | supported | `statements_continue_break` | |
| 3.6.6.4 return | supported | `statements_return` | |

### 3.7 External definitions

| Feature | Status | Program | Notes |
|---|---|---|---|
| 3.5.4.3, 3.7.1 Prototypes | supported | `functions_prototypes` | Supports unnamed parameters and `()` |
| 3.7.1 Identifier lists (K&R) | supported | `functions_kr_definitions` | |
| 3.7.1 Array parameters | supported | `functions_array_parameters` | |
| 3.1.2.2 static functions | supported | `functions_static` | |
| 2.2.4.1 31 parameters | supported | `functions_many_parameters` | |
| 4.8 Variable arguments | supported | `functions_varargs` | Passed via a hidden array in the caller's stack frame |
| 4.8.1.2 `va_arg` of char and short | supported | `functions_varargs_promoted` | Reads promoted `int` and narrows to `char` or `short` |
| 3.5.4.3 Pointers to any function | supported | `functions_pointers_composite` | Supports struct and variadic parameters |
| 3.5.4.3 Returning pointers | supported | `functions_return_pointer` | |

### 3.8 Preprocessing directives

| Feature | Status | Program | Notes |
|---|---|---|---|
| 3.8.1 `#if` and kin | supported | `preprocessor_conditionals` | |
| 3.8.1 `#if` arithmetic | supported | `preprocessor_if_arithmetic` | Standard C operator precedence with 32-bit `long` |
| 3.8.1 One `#else`, last | supported | `preprocessor_else_twice` | Rejects duplicate `#else` or `#elif` after `#else` |
| 3.8.2 `#include` | supported | `preprocessor_include` | |
| 3.8.2 A header's macros in `#if` | supported | `preprocessor_header_macros` | Header macros are available to `#if` while header constants remain symbolic in code |
| 3.8.2 A header's macros in constant expressions | supported | `preprocessor_header_constants` | Usable in array sizes, enums, `case` labels, and initializers |
| 3.8.3 Object-like macros | supported | `preprocessor_object_macros` | |
| 3.8.3 Function-like macros | supported | `preprocessor_function_macros` | |
| 3.8.3 Defined again differently | supported | `preprocessor_redefinition` | Rejects incompatible macro redefinitions |
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
| `<float.h>` | supported | `library_float` | Defines 32-bit `float` limits |
| `<stddef.h>` | supported | `library_stddef` | Defines `NULL`, `size_t`, `ptrdiff_t`, and `wchar_t` |
| A header's typedefs as type names | supported | `library_header_typedefs` | Usable in declarations and casts |
| `offsetof` | supported | `library_offsetof` | Computes byte offsets matching `sizeof` |
| `<stdarg.h>` | supported | `functions_varargs` | `va_list` points into the caller's hidden argument array |

## Compiler lowering rules

The parser, preprocessor (`cpp.rs`), and lowering passes (`lower.rs` and `composite.rs`) translate C89 constructs that have no direct VM instruction into standard `.stb` bytecode:

- **Macros and header constants**: The tokenizer expands function-like and complex macros according to C89 rescanning rules. However, a `#define` whose body is a simple constant (a number, string, negative literal, or parenthesized integer constant expression) is retained as a named constant so decompiled scripts preserve symbolic constant names. Constants defined in SDK headers, shared libraries, or local quoted headers can also be used in compile-time constant expressions (array sizes, enum values, `case` labels, and `#if` conditions).
- **`goto` and labels**: Lowered to an unconditional jump targeting a compiler-internal label marker (`NOP`).
- **`sizeof`**: Evaluated at parse time from the declared types.
- **`static` locals and initialized globals**: `static` local variables lower to hidden global variables named after the enclosing function and variable. When a script defines initialized globals or initialized `static` locals, the compiler generates a hidden `__initialise` function guarded by a hidden global flag. Every `MAIN` and `PROGRAM` entry point calls `__initialise` at the start of the function so initializers run exactly once per script load.
- **Multidimensional arrays**: Flattened into a contiguous run of slots indexed by `i * columns + j`. Sub-arrays decay to the address of their first element. Initializer lists support standard C brace elision and zero-fill omitted elements.
- **`char` and `short` narrowing**: Masked to their bit width on assignment, increment/decrement, compound assignment, argument passing, and return (`x & 255` for unsigned; offset by `128`, masked, and offset back for signed).
- **`unsigned int` operations**: Stored in standard 32-bit integer slots. Addition, subtraction, multiplication, bitwise logic, left shift, and equality work identically to signed integers. Relational comparisons (`<`, `<=`, `>`, `>=`) bias both operands by $-2^{31}$. Unsigned division (`/`), remainder (`%`), right shift (`>>`), and `float` conversions call small helper functions injected into the compiled script.
- **Block-scoped variables**: Each local variable receives one slot in the function frame. If an inner block declares a variable that shadows an outer variable, the compiler renames the inner variable to give it a separate slot.
- **Header typedefs vs. variable names**: Standard typedefs from included headers (such as `size_t`) act as type specifiers unless shadowed by a variable, enumerator, or constant of the same name (allowing retail scripts to name local variables `flag` or `message`).
- **C's conversions and logical values**: Inserts `FTOI` or `ITOF` instructions (or unsigned conversion calls) when storing, passing to a script function, or returning across `int` and `float` types, except for a value cast to `any` and the int `0` stored in a `float` (the same bits); appends `NOT NOT` when the value of `&&` or `||` is used, except under `(any)`; and rewrites native calls inside short-circuited operands (`a && f()`, `a || f()`, `c ? f() : x`, `a, f()`) into guarded `if` blocks using a temporary variable. A native's arguments are not converted between `int` and `float` (the engine does it as it reads them). `x = (any)f();` with `f` a native that gives a result still writes it straight into `x`.

### Lowering pointers and composite types (`composite.rs`)

The `composite.rs` pass type-checks expressions and lowers pointers, structs, unions, bit-fields, and variadic functions into VM slot operations:

- **Lvalue places**: Every lvalue resolves to one of four forms:
  1. A direct local or global slot.
  2. A slot at a constant offset within a local or global aggregate.
  3. An indexed slot within a local array or pointer parameter.
  4. An indexed slot through a computed pointer expression (stored into a temporary local variable first because the VM only indexes through local pointer slots).
- **Pointer arithmetic**: `p + n` lowers to `&p[n]`, `*(p + i)` and `i[a]` lower to `p[i]` and `a[i]`, and `p++` saves the original pointer in a hidden local before storing `&p[1]`.
- **Global arrays, `static` arrays, and global pointers**: Because the VM places global slots at the bottom of the value stack below function call frames, global slot addresses support the same indexed pointer instructions as local slots. Constant indices access the global slot directly (`load global N`); variable indices load the base address of the global array into a hidden local pointer and index through it (`load ptr_idx t`).
- **Multi-level pointers (`int **pp`)**: Dereferencing copies each intermediate pointer level into a hidden local variable before indexing the next level.
- **Structs and unions**: Assigned slot-by-slot. Struct arguments pass by address and the callee copies the slots into a hidden local variable to preserve pass-by-value semantics. Functions returning a struct write into a hidden first pointer parameter (`__result`) pointing to a temporary struct in the caller's frame.
- **Function pointers**: Indirect calls invoke a generated dispatcher function `__call_N` (one per distinct VM argument count) that switches on the function ID and forwards all arguments, including hidden struct and variadic parameters.
- **Bit-fields**: Reads mask and shift the shared `int` slot (`(slot & mask) >> offset`, followed by sign extension `(v ^ top) - top` for signed bit-fields). Writes compute `slot = (slot & ~mask) | ((value << offset) & mask)` and return the stored field value.
- **Variadic functions**: Callers pack trailing variadic arguments into a hidden local array and pass its address as a hidden final parameter (`__va`). `va_start` initializes `va_list` from `__va`, and `va_arg` advances the pointer.
- **String literal arrays and wide strings**: Lowered to hidden global arrays initialized before the statement that references them, ensuring their storage remains valid for the lifetime of the script.

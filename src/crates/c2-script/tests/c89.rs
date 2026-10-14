//! The C89 conformance suite: a small program in tests/c89/ for each feature of the language
//! (ANSI X3.159-1989, by section of the standard), compiled with the SDK's headers in strict mode
//! (#pragma std c89) and run in the test VM. A program's second line says what it shows:
//!
//! - `// expect: V`: it compiles, and test() returns V (an int, or a float when V has a point);
//! - `// rejects: TEXT`: C requires a diagnostic, and the compiler gives one with TEXT in it;
//! - `// differs: V; C gives W (...)`: it compiles and returns V where C gives W, until the step
//!   of PLAN.md's "Next: standard C" it names;
//! - `// planned: step N: TEXT`: it does not work yet: compiling fails with TEXT in the error, or,
//!   with no TEXT, compiling or running fails; step N makes it work, and this test then fails
//!   until the program says what it shows instead;
//! - `// limit: TEXT`: compiling fails with TEXT by design (double, which the VM cannot hold).
//!
//! docs/c-standard.md lists every program with its status.

mod common;

use c2_script::vm::{self, Value};
use c2_script::{compile, library, parse};
use common::{root, sdk_texts, tables};

/// Every program is compiled in strict mode, where C's semantics apply; #line keeps its line
/// numbers its own.
const HEAD: &str = "#pragma std c89\n#line 1\n";

/// What makes a program a script: its native table and the program table.
const TRAILER: &str = "\n#pragma natives event\nPROGRAM(100, test);\nMAIN(test);\n";

#[derive(Debug, PartialEq)]
enum Status {
    Expect(Value),
    Rejects(String),
    Differs(Value),
    Planned(String),
    Limit(String),
}

fn value(text: &str) -> Value {
    let text = text.trim();
    if text.contains('.') {
        Value::Float(text.parse().unwrap())
    } else {
        Value::Int(text.parse().unwrap())
    }
}

fn status(name: &str, text: &str) -> Status {
    let line = text.lines().nth(1).unwrap_or("");
    let (kind, rest) = line.trim_start_matches("// ").split_once(": ").unwrap_or((line, ""));
    match kind {
        "expect" => Status::Expect(value(rest)),
        "rejects" => Status::Rejects(rest.to_string()),
        "differs" => Status::Differs(value(rest.split(';').next().unwrap())),
        "planned" => Status::Planned(rest.split_once(": ").map_or("", |(_, message)| message).to_string()),
        "limit" => Status::Limit(rest.to_string()),
        _ => panic!("{name}: the second line says no status: {line}"),
    }
}

fn source(name: &str) -> String {
    let path = root().join("src/crates/c2-script/tests/c89").join(format!("{name}.c"));
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display())).replace("\r\n", "\n")
}

/// The program compiled (to bytes, so it assembles) and run: test()'s result.
fn run(name: &str, text: &str) -> c2_script::Result<Value> {
    let program = parse::parse_source(&format!("{HEAD}{text}{TRAILER}"), &format!("{name}.c"), sdk_texts())?;
    let program = library::expand(program, &common::no_texts(), sdk_texts())?;
    compile::compile_program(&program, tables())?;
    let table = vm::table_of(&program, tables())?;
    vm::Machine::new(&table, vec![Value::Int(0); 64]).run("test", Vec::new())
}

fn check(name: &str) {
    let text = source(name);
    let found = run(name, &text);
    match status(name, &text) {
        Status::Expect(wanted) | Status::Differs(wanted) => match found {
            Ok(value) => assert_eq!(value, wanted, "{name}"),
            Err(error) => panic!("{name}: {error}"),
        },
        Status::Rejects(message) | Status::Limit(message) => match found {
            Ok(value) => panic!("{name}: compiled and gave {value:?}"),
            Err(error) => assert!(error.to_string().contains(&message), "{name}: {error}"),
        },
        Status::Planned(message) => match found {
            Ok(value) => panic!("{name}: works now (gave {value:?}); say so in the program and the docs"),
            Err(error) => assert!(error.to_string().contains(&message), "{name}: {error}"),
        },
    }
}

macro_rules! programs {
    ($($name:ident),* $(,)?) => {
        $(
            #[test]
            fn $name() {
                check(stringify!($name));
            }
        )*

        const PROGRAMS: &[&str] = &[$(stringify!($name)),*];
    };
}

programs! {
    // 3.1 lexical elements
    lexical_keywords,
    lexical_identifiers,
    lexical_integer_constants,
    lexical_unsigned_constants,
    lexical_floating_constants,
    lexical_long_double_constants,
    lexical_character_constants,
    lexical_string_literals,
    lexical_string_literal_arrays,
    lexical_comments,
    lexical_trigraphs,
    lexical_wide_characters,
    lexical_wide_strings,
    // 3.2 conversions
    conversions_integer_promotion,
    conversions_signed_unsigned,
    conversions_float_to_int,
    conversions_int_to_float,
    conversions_unsigned_float,
    conversions_usual_arithmetic,
    conversions_assignment,
    conversions_void,
    conversions_null_pointer,
    conversions_pointer_integer,
    conversions_incompatible_pointers,
    // 3.3 expressions and 3.4 constant expressions
    expressions_primary,
    expressions_array_subscript,
    expressions_subscript_commutes,
    expressions_function_calls,
    expressions_struct_members,
    expressions_postfix_increment,
    expressions_prefix_increment,
    expressions_address_indirection,
    expressions_unary_arithmetic,
    expressions_sizeof,
    expressions_casts,
    expressions_multiplicative,
    expressions_additive,
    expressions_pointer_arithmetic,
    expressions_pointer_comparison,
    expressions_pointer_difference,
    expressions_shift,
    expressions_relational,
    expressions_equality,
    expressions_bitwise,
    expressions_logical,
    expressions_logical_value,
    expressions_conditional,
    expressions_conditional_structs,
    expressions_assignment,
    expressions_compound_assignment,
    expressions_comma,
    expressions_constant,
    // 3.5 declarations
    declarations_auto_register,
    declarations_register_address,
    declarations_static_locals,
    declarations_extern,
    declarations_initialised_globals,
    declarations_global_initialiser_constant,
    declarations_const,
    declarations_const_checked,
    declarations_const_pointee,
    declarations_volatile,
    declarations_integer_types,
    declarations_long_long,
    declarations_implicit_int,
    declarations_enums,
    declarations_typedef,
    declarations_struct,
    declarations_bit_fields,
    declarations_bit_field_width,
    declarations_bit_field_address,
    declarations_struct_tag_scope,
    declarations_union,
    declarations_pointers,
    declarations_pointer_to_pointer,
    declarations_arrays,
    declarations_array_size,
    declarations_multidimensional_arrays,
    declarations_char_arrays,
    declarations_global_arrays,
    declarations_static_arrays,
    declarations_global_pointers,
    declarations_function_pointers,
    declarations_double,
    declarations_block_scope,
    declarations_redeclared,
    // 3.6 statements
    statements_labels_goto,
    statements_goto_undefined_label,
    statements_compound,
    statements_expression_null,
    statements_if,
    statements_switch,
    statements_switch_duplicate_case,
    statements_while,
    statements_do,
    statements_for,
    statements_continue_break,
    statements_return,
    // 3.7 external definitions
    functions_prototypes,
    functions_kr_definitions,
    functions_array_parameters,
    functions_static,
    functions_many_parameters,
    functions_varargs,
    functions_varargs_promoted,
    functions_pointers_composite,
    functions_return_pointer,
    functions_struct_arguments,
    // 3.8 preprocessing directives
    preprocessor_object_macros,
    preprocessor_function_macros,
    preprocessor_stringify,
    preprocessor_paste,
    preprocessor_rescanning,
    preprocessor_undef,
    preprocessor_redefinition,
    preprocessor_conditionals,
    preprocessor_else_twice,
    preprocessor_if_arithmetic,
    preprocessor_line,
    preprocessor_error,
    preprocessor_pragma_null,
    preprocessor_predefined,
    preprocessor_include,
    preprocessor_header_macros,
    preprocessor_header_constants,
    // 4.1 the freestanding headers
    library_limits,
    library_float,
    library_stddef,
    library_header_typedefs,
    library_offsetof,
}

/// Every program in tests/c89 runs here and has its row in docs/c-standard.md, with its status.
#[test]
fn every_program_runs_and_is_documented() {
    let folder = root().join("src/crates/c2-script/tests/c89");
    let docs = std::fs::read_to_string(root().join("docs/c-standard.md")).unwrap();
    let mut names: Vec<String> = std::fs::read_dir(&folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter_map(|name| name.strip_suffix(".c").map(str::to_string))
        .collect();
    names.sort();
    for name in &names {
        assert!(PROGRAMS.contains(&name.as_str()), "{name}.c is not in programs!");
        let row = docs.lines().find(|line| line.contains(&format!("`{name}`")));
        let Some(row) = row else {
            panic!("{name} has no row in docs/c-standard.md");
        };
        let shown = match status(name, &source(name)) {
            Status::Expect(_) | Status::Rejects(_) => "supported",
            Status::Differs(_) => "differs",
            Status::Planned(_) => "planned",
            Status::Limit(_) => "limit",
        };
        assert!(row.to_lowercase().contains(shown), "{name}: docs/c-standard.md does not say {shown}: {row}");
    }
    assert_eq!(names.len(), PROGRAMS.len(), "a program in programs! has no file");
}

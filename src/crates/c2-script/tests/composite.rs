//! C's composite types and pointers (composite.rs) in programs that use them together, run in the
//! test VM, and what they compile to where it matters.

mod common;

use c2_script::vm::Value;
use c2_script::{compile, library, parse};
use common::{sdk_texts, tables};

const HEAD: &str = "#pragma std c89\n#line 1\n";
const TRAILER: &str = "\n#pragma natives event\nPROGRAM(100, test);\nMAIN(test);\n";

fn run(text: &str) -> c2_script::Result<Value> {
    let program = parse::parse_source(&format!("{HEAD}{text}{TRAILER}"), "test.c", sdk_texts())?;
    let program = library::expand(program, &common::no_texts(), sdk_texts())?;
    compile::compile_program(&program, tables())?;
    let table = c2_script::vm::table_of(&program, tables())?;
    c2_script::vm::Machine::new(&table, vec![Value::Int(0); 64]).run("test", Vec::new())
}

fn gives(text: &str, wanted: i128) {
    match run(text) {
        Ok(value) => assert_eq!(value, Value::Int(wanted), "{text}"),
        Err(error) => panic!("{error}\n{text}"),
    }
}

fn refuses(text: &str, message: &str) {
    match run(text) {
        Ok(value) => panic!("compiled and gave {value:?}: {text}"),
        Err(error) => assert!(error.to_string().contains(message), "{error}"),
    }
}

#[test]
fn typedef_structs_arrays_of_them_and_initializers() {
    gives(
        "typedef struct {\n    int id;\n    float weight;\n    char tag[3];\n} item;\n\nitem stock[3] = {{1, 0.5, \"ab\"}, {2, 1.5}};\n\nint test(void)\n{\n    item local[2] = {{7, 2.0, {'x'}}, 8, 3.0};\n    item *last = &stock[2];\n    int i;\n    int total = 0;\n\n    last->id = 3;\n    for (i = 0; i < 3; i++) {\n        total += stock[i].id * 10;\n    }\n    return total + local[1].id + (stock[1].weight == 1.5) * 100 + (stock[0].tag[1] == 'b') * 1000 + (local[0].tag[0] == 'x') * 10000 + (local[0].tag[2] == 0) * 100000;\n}\n",
        60 + 8 + 100 + 1000 + 10000 + 100000,
    );
}

#[test]
fn global_structs_and_unions_with_values() {
    gives(
        "struct range {\n    int low;\n    int high;\n};\n\nunion number {\n    int i;\n    float f;\n};\n\nstruct range limits = {3, 9};\nunion number one = {1};\nstruct range *current = &limits;\n\nint test(void)\n{\n    current->high += one.i;\n    return limits.low * 100 + limits.high;\n}\n",
        310,
    );
}

#[test]
fn linked_structs_through_pointers() {
    gives(
        "struct node {\n    int value;\n    struct node *next;\n};\n\nint total(struct node *at)\n{\n    int sum = 0;\n\n    while (at != 0) {\n        sum += at->value;\n        at = at->next;\n    }\n    return sum;\n}\n\nint test(void)\n{\n    struct node a;\n    struct node b;\n    struct node c;\n\n    a.value = 1;\n    a.next = &b;\n    b.value = 20;\n    b.next = &c;\n    c.value = 300;\n    c.next = 0;\n    a.next->next->value += 100;\n    return total(&a);\n}\n",
        421,
    );
}

#[test]
fn function_pointers_in_structs_and_arrays() {
    gives(
        "struct op {\n    int (*apply)(int, int);\n    int unit;\n};\n\nint add(int a, int b)\n{\n    return a + b;\n}\n\nint mul(int a, int b)\n{\n    return a * b;\n}\n\nint fold(struct op *with, int *values, int count)\n{\n    int result = with->unit;\n    int i;\n\n    for (i = 0; i < count; i++) {\n        result = with->apply(result, values[i]);\n    }\n    return result;\n}\n\nint test(void)\n{\n    struct op ops[2] = {{add, 0}, {mul, 1}};\n    int values[3] = {2, 3, 4};\n\n    return fold(&ops[0], values, 3) * 100 + fold(ops + 1, values, 3);\n}\n",
        924,
    );
}

#[test]
fn pointers_walk_arrays_and_strings_of_chars() {
    gives(
        "int length(char *text)\n{\n    char *at = text;\n    int count = 0;\n\n    while (*at++) {\n        count++;\n    }\n    return count;\n}\n\nvoid copy(char *to, char *from)\n{\n    while ((*to++ = *from++) != 0) {\n    }\n}\n\nint test(void)\n{\n    char name[8] = \"Maximus\";\n    char other[8];\n\n    copy(other, name);\n    other[3] = 0;\n    return length(name) * 10 + length(other);\n}\n",
        73,
    );
}

#[test]
fn varargs_of_mixed_types_and_nested_calls() {
    gives(
        "#include <stdarg.h>\n\nint count_floats(int count, ...)\n{\n    va_list ap;\n    int found = 0;\n\n    va_start(ap, count);\n    while (count-- > 0) {\n        if (va_arg(ap, float) > 1.0) {\n            found++;\n        }\n    }\n    va_end(ap);\n    return found;\n}\n\nint sum(int count, ...)\n{\n    va_list ap;\n    int total = 0;\n\n    va_start(ap, count);\n    while (count-- > 0) {\n        total += va_arg(ap, int);\n    }\n    return total;\n}\n\nint test(void)\n{\n    return sum(2, sum(2, 1, 2), 10) * 10 + count_floats(3, 0.5, 2.5, 3.5);\n}\n",
        132,
    );
}

#[test]
fn pointers_refuse_what_the_vm_cannot_do() {
    refuses(
        "int test(void)\n{\n    int a[2];\n    int *p = a;\n\n    return (int)p;\n}\n",
        "a pointer cannot be converted to int",
    );
    refuses("int test(void)\n{\n    int *p = (int *)4;\n\n    return 0;\n}\n", "int cannot be converted to a pointer");
    refuses("struct s {\n    int a : 3;\n};\n\nint test(void)\n{\n    return 0;\n}\n", "bit-fields are not supported");
    refuses(
        "struct later;\n\nint test(void)\n{\n    struct later x;\n\n    return 0;\n}\n",
        "struct later, which is not defined",
    );
    refuses(
        "struct p {\n    int x;\n};\n\nint test(void)\n{\n    struct p a;\n\n    return a.y;\n}\n",
        "struct p has no member y",
    );
    refuses("int test(void)\n{\n    float f;\n    int *p = &f;\n\n    return 0;\n}\n", "incompatible pointer types");
}

#[test]
fn global_arrays_index_through_their_address() {
    // an element at a constant index is the global slot itself; at another, through the array's
    // address in a hidden local
    let program = parse::parse_source(
        "int table[4];\n\nint f(int i)\n{\n    return table[2] + table[i];\n}\n\n#pragma natives event\nPROGRAM(100, f);\nMAIN(f);\n",
        "test.c",
        sdk_texts(),
    )
    .unwrap();
    let resolved =
        c2_script::sdk::resolve(&library::expand(program, &common::no_texts(), sdk_texts()).unwrap(), tables())
            .unwrap();
    let text = compile::to_asm(&resolved, tables(), None).unwrap();
    assert!(text.contains("load global 2\n"), "{text}");
    assert!(text.contains("addr global 0\n"), "{text}");
    assert!(text.contains("load ptr_idx 1\n"), "{text}");
    assert!(text.contains(".header SB2 main f globals 4"), "{text}");
}

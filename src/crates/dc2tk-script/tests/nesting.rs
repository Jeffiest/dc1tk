//! The whole compile, parse, sdk::resolve and compile::to_asm, at the parser's nesting limit, on a
//! thread with a 1 MB stack, as Windows gives a main thread, in a debug build too.

mod common;

use common::functions;
use dc2tk_script::parse;

/// A source of a function f whose body is the statement, with the event natives and a function g
/// of one parameter to call.
fn source(head: &str, statement: &str) -> String {
    format!(
        "{head}#include <event.h>\n\nMAIN(f);\n\nint g(int a)\n{{\n    return a;\n}}\n\n\
         void f(void)\n{{\n    int x;\n    {statement}\n}}\n"
    )
}

/// The statement nested as deep as the parser takes: the most repeats of a form within
/// parse::NESTING.
fn deepest(form: &dyn Fn(usize) -> String) -> String {
    let parses = |count: usize| parse::parse(&source("", &form(count))).is_ok();
    let count = (1..parse::NESTING).take_while(|&count| parses(count)).last().unwrap();
    assert!(count < parse::NESTING - 1, "{} parses", form(count + 1));
    form(count)
}

/// Each form of expression and statement, nested as deep as the parser takes, compiles on a 1 MB
/// stack (a stack overflow aborts the test run).
fn deepest_forms_compile(head: &'static str) {
    std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(move || {
            let r = |text: &str, count: usize| text.repeat(count);
            let forms: Vec<Box<dyn Fn(usize) -> String>> = vec![
                Box::new(move |count| format!("x = {}1;", r("!", count))),
                Box::new(move |count| format!("x = 1{};", r(" + 1", count))),
                Box::new(move |count| format!("x = {}1{};", r("(", count), r(")", count))),
                Box::new(move |count| format!("x = {}1{};", r("g(", count), r(")", count))),
                Box::new(move |count| format!("{}x = 1;{}", r("{ ", count), r(" }", count))),
                Box::new(move |count| format!("{}x = 1;", r("if (x) ", count))),
                Box::new(move |count| format!("{}x = 1;", r("while (x) ", count))),
                Box::new(move |count| format!("{}x = 1;{}", r("switch (x) { case 0: ", count), r(" }", count))),
                // natives in expressions are called first, into hidden locals (sdk::hoisted)
                Box::new(move |count| format!("x = {}1{};", r("get_flag(", count), r(")", count))),
                Box::new(move |count| format!("x = 1{};", r(" && get_flag(1)", count))),
                Box::new(move |count| format!("x = 1{};", r(" ? get_flag(1) : 1", count))),
                Box::new(move |count| format!("{}x = 1;", r("if (get_flag(1)) ", count))),
                Box::new(move |count| format!("{}x = 1;", r("while (get_flag(1)) ", count))),
            ];
            for form in &forms {
                let text = source(head, &deepest(form.as_ref()));
                if let Err(error) = functions(&text) {
                    panic!("{error}: {}", &text[text.len().saturating_sub(120)..]);
                }
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn the_deepest_nesting_the_parser_takes_compiles() {
    deepest_forms_compile("");
}

#[test]
fn the_deepest_nesting_the_parser_takes_compiles_with_cs_semantics() {
    deepest_forms_compile("#pragma std c89\n");
}

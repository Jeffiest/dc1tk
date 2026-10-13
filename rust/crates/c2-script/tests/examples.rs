//! Carried over from the Python tool's test_examples.py

mod common;

use c2_script::{compile, library, parse};
use common::{root, sdk_dir, sdk_texts, tables};

#[test]
fn every_example_compiles_with_the_sdk() {
    let folder = root().join("docs").join("examples");
    let mut examples: Vec<_> = std::fs::read_dir(&folder)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "c"))
        .collect();
    examples.sort();
    assert!(!examples.is_empty());
    for path in examples {
        let program = parse::parse(&c2_py::read_text(&path).unwrap()).unwrap();
        let includes = library::read_includes(&program, &path, None, &sdk_dir(), &[]).unwrap();
        let expanded = library::expand(program, &includes, sdk_texts()).unwrap();
        let data = compile::compile_program(&expanded, tables()).unwrap();
        assert!(!data.is_empty(), "{}", path.display());
    }
}

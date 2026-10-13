//! Parses each file a list names and writes `<out>/<n>.json`: the program as compact JSON, or
//! {"error": message}. A list line is a path, then optionally a tab and a JSON object of #defines.
//! For rust/parity.py's parse check.
//!
//!   cargo run --release -p c2-script --example parse_dump LIST OUT

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let [_, list, out] = args.as_slice() else {
        eprintln!("usage: parse_dump LIST OUT");
        std::process::exit(2);
    };
    let list = fs::read_to_string(list).expect("reading the list");
    let out = PathBuf::from(out);
    fs::create_dir_all(&out).expect("making the output folder");
    let mut texts = Vec::new();
    for line in list.lines().filter(|line| !line.is_empty()) {
        let (path, defines) = line.split_once('\t').unwrap_or((line, "{}"));
        let defines: HashMap<String, String> = serde_json::from_str(defines).expect("defines as a JSON object");
        let text = c2_py::read_text(Path::new(path)).unwrap_or_else(|error| panic!("reading {path}: {error}"));
        texts.push((text, defines));
    }
    let start = Instant::now();
    let parsed: Vec<_> = texts.iter().map(|(text, defines)| c2_script::parse::parse_with(text, defines)).collect();
    let parsing = start.elapsed().as_secs_f64();
    let start = Instant::now();
    let dumps: Vec<String> = parsed
        .iter()
        .map(|parsed| match parsed {
            Ok(program) => program.to_string(),
            Err(error) => serde_json::json!({"error": error.to_string()}).to_string(),
        })
        .collect();
    let dumping = start.elapsed().as_secs_f64();
    eprintln!("parsed {} files in {parsing:.2}s, made their JSON in {dumping:.2}s", dumps.len());
    for (number, dump) in dumps.iter().enumerate() {
        fs::write(out.join(format!("{number}.json")), dump).expect("writing a dump");
    }
}

//! dc2tk verify EXTRACTED: round-trips every script of an extracted disc through each stage (the
//! .stb format, native calls, assembly, and C source) and reports what does not come back the
//! same.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Result;
use rayon::prelude::*;
use serde_json::{json, Map, Value};

use dc2tk_script::definitions::Definitions;
use dc2tk_script::natives::{self, Table};
use dc2tk_script::{asm, assets, compile, disc, library, parse, slots, source, stack, stb};

use crate::files;

const STAGES: [&str; 4] = ["format", "natives", "assembly", "source"];
const STATUSES: [&str; 4] = ["pass", "fail", "error", "pending"];

fn compare(original: &[u8], rebuilt: &[u8]) -> Option<String> {
    if rebuilt == original {
        return None;
    }
    let first = original.iter().zip(rebuilt).position(|(a, b)| a != b).unwrap_or(original.len().min(rebuilt.len()));
    Some(format!("first difference at 0x{:x} (sizes 0x{:x} and 0x{:x})", first, original.len(), rebuilt.len()))
}

fn error(detail: impl std::fmt::Display) -> Value {
    json!({"status": "error", "detail": format!("ValueError({:?})", detail.to_string())})
}

fn fail(detail: String) -> Value {
    json!({"status": "fail", "detail": detail})
}

fn check_format(data: &[u8], script: &stb::Script) -> Value {
    match stb::write(script) {
        Err(problem) => error(problem),
        Ok(rebuilt) => match compare(data, &rebuilt) {
            Some(difference) => fail(difference),
            None => json!({"status": "pass", "coverage": stb::coverage(script)}),
        },
    }
}

fn check_natives(
    path: &str,
    script: &stb::Script,
    tables: &BTreeMap<String, Table>,
    rules: &[(String, String)],
) -> Value {
    let table = natives::table_for(path, rules);
    let Some(found) = tables.get(table) else {
        return json!({"status": "pending"});
    };
    let calls = stack::native_calls(script);
    let known = found.numbered();
    let mut unregistered: indexmap::IndexMap<String, i64> = indexmap::IndexMap::new();
    for call in &calls {
        let Some(number) = call.no else {
            return fail(format!("no native number for EXT {} in function at 0x{:x}", call.index, call.function));
        };
        if !known.contains_key(&number) {
            *unregistered.entry(format!("{table}:{number}")).or_default() += 1;
        }
    }
    json!({"status": "pass", "table": table, "calls": calls.len(), "unregistered": unregistered})
}

fn check_assembly(
    path: &str,
    data: &[u8],
    script: &stb::Script,
    tables: &BTreeMap<String, Table>,
    rules: &[(String, String)],
) -> Value {
    let text = asm::disassemble(script, natives::table_for(path, rules), tables);
    match asm::assemble(&text, tables) {
        Err(problem) => error(problem),
        Ok(rebuilt) => match compare(data, &rebuilt) {
            Some(difference) => fail(difference),
            None => {
                let fallbacks = text
                    .split('\n')
                    .filter(|line| {
                        let line = dc2tk_py::strip(line);
                        line.starts_with(".op ") || line.starts_with(".bytes ")
                    })
                    .count();
                json!({"status": "pass", "fallbacks": fallbacks})
            }
        },
    }
}

fn asm_functions(program: &dc2tk_script::node::Node) -> usize {
    if let Some(text) = program.get("asm").and_then(|text| text.as_str()) {
        return text.split('\n').filter(|region| region.starts_with(".function")).count();
    }
    program["functions"]
        .as_array()
        .map_or(0, |functions| functions.iter().filter(|function| function.kind() == "asm").count())
}

fn check_source(
    path: &str,
    data: &[u8],
    model: &library::Model,
    texts: &indexmap::IndexMap<String, String>,
    headers: &indexmap::IndexMap<String, String>,
    tables: &BTreeMap<String, Table>,
) -> Value {
    let program = &model.programs[path];
    let result = (|| -> dc2tk_script::Result<(String, dc2tk_script::node::Node, Vec<u8>)> {
        let text = source::text(&model.script_program(program, path)?)?;
        let parsed = parse::parse_source_with(&text, parse::UNNAMED_FILE, &Default::default(), headers, texts)?;
        let rebuilt = compile::compile_program(&library::expand(parsed.clone(), texts, headers)?, tables)?;
        Ok((text, parsed, rebuilt))
    })();
    match result {
        Err(problem) => error(problem),
        Ok((text, parsed, rebuilt)) => match compare(data, &rebuilt) {
            Some(difference) => fail(difference),
            None => {
                let includes = parsed["functions"]
                    .as_array()
                    .map_or(0, |items| items.iter().filter(|item| item.kind() == "include").count());
                json!({"status": "pass", "asm_functions": asm_functions(program), "lines": text.matches('\n').count(),
                       "includes": includes})
            }
        },
    }
}

fn number(value: &Value, key: &str) -> u64 {
    value[key].as_u64().unwrap_or(0)
}

fn summarize(results: &[Value]) -> Value {
    let mut counts = Map::new();
    for stage in STAGES {
        let mut by_status = Map::new();
        for status in STATUSES {
            let count = results.iter().filter(|result| result[stage]["status"] == status).count();
            by_status.insert(status.into(), count.into());
        }
        counts.insert(stage.into(), Value::Object(by_status));
    }
    let covered: Vec<f64> = results
        .iter()
        .filter(|result| result["format"]["status"] == "pass")
        .filter_map(|result| result["format"]["coverage"].as_f64())
        .collect();
    let mut includes: Vec<u64> = results.iter().map(|result| number(&result["source"], "includes")).collect();
    includes.sort();
    let mut calls = 0;
    let mut unregistered: BTreeMap<String, u64> = BTreeMap::new();
    for result in results {
        let outcome = &result["natives"];
        if outcome["status"] == "pass" {
            calls += number(outcome, "calls");
            for (key, count) in outcome["unregistered"].as_object().into_iter().flatten() {
                *unregistered.entry(key.clone()).or_default() += count.as_u64().unwrap_or(0);
            }
        }
    }
    let ok = STAGES.iter().all(|stage| counts[*stage]["fail"] == 0 && counts[*stage]["error"] == 0);
    let coverage =
        if covered.is_empty() { 0.0 } else { dc2tk_py::sum_floats(covered.iter().copied()) / covered.len() as f64 };
    json!({
        "scripts": results.len(),
        "stages": counts,
        "coverage": coverage,
        "native_calls": calls,
        "fallbacks": results.iter().map(|result| number(&result["assembly"], "fallbacks")).sum::<u64>(),
        "asm_functions": results.iter().map(|result| number(&result["source"], "asm_functions")).sum::<u64>(),
        "source_lines": results.iter().map(|result| number(&result["source"], "lines")).sum::<u64>(),
        "median_includes": includes.get(includes.len() / 2).copied().unwrap_or(0),
        "unregistered": unregistered,
        "ok": ok,
    })
}

/// The report: {"summary", "files": [one result per script]}.
pub fn verify(
    root: &Path,
    tables: &BTreeMap<String, Table>,
    defs: Definitions,
    headers: indexmap::IndexMap<String, String>,
) -> Result<Value> {
    let paths = files::find_scripts(root)?;
    let (defs, mut headers, _) = disc::with_header(root, defs, headers);
    let model = library::model_for(root, &paths, tables, &defs, &headers)?;
    if let Some(text) = headers.get(disc::HEADER).cloned() {
        // the names a disc decompile adds to disc.h, which the presented scripts use
        let named = assets::with_header(slots::with_header(Some(text), &model.folder_slots, &model.villagers), &model.asset_names);
        headers.insert(disc::HEADER.to_string(), named.unwrap_or_default());
    }
    let texts = model.library_texts()?;
    let results: Vec<Value> = paths
        .par_iter()
        .map(|path| -> Result<Value> {
            let data = std::fs::read(root.join(path))?;
            let script = stb::read(&data);
            Ok(json!({
                "path": path,
                "size": data.len(),
                "format": check_format(&data, &script),
                "natives": check_natives(path, &script, tables, &defs.tables),
                "assembly": check_assembly(path, &data, &script, tables, &defs.tables),
                "source": check_source(path, &data, &model, &texts, &headers, tables),
            }))
        })
        .collect::<Result<_>>()?;
    let mut summary = summarize(&results);
    summary["libraries"] = texts.len().into();
    summary["library_lines"] = texts.values().map(|text| text.matches('\n').count()).sum::<usize>().into();
    Ok(json!({"summary": summary, "files": results}))
}

fn failures(report: &Value, limit: usize) -> (Vec<String>, usize) {
    let mut lines = Vec::new();
    for result in report["files"].as_array().into_iter().flatten() {
        for stage in STAGES {
            let outcome = &result[stage];
            let status = outcome["status"].as_str().unwrap_or("");
            if status == "fail" || status == "error" {
                lines.push(format!(
                    "{}  {}  {}: {}",
                    stage,
                    status,
                    result["path"].as_str().unwrap_or(""),
                    outcome["detail"].as_str().unwrap_or("")
                ));
            }
        }
    }
    let total = lines.len();
    lines.truncate(limit);
    (lines, total)
}

pub fn format_report(report: &Value, limit: usize) -> String {
    let summary = &report["summary"];
    let mut lines = vec![format!("{} scripts", summary["scripts"]), String::new()];
    lines.push(format!("{:<10} {:>8} {:>8} {:>8} {:>8}", "stage", "pass", "fail", "error", "pending"));
    for stage in STAGES {
        let counts = &summary["stages"][stage];
        let count = |status: &str| counts[status].as_u64().unwrap_or(0);
        lines.push(format!(
            "{:<10} {:>8} {:>8} {:>8} {:>8}",
            stage,
            count("pass"),
            count("fail"),
            count("error"),
            count("pending")
        ));
    }
    lines.push(String::new());
    lines.push(format!(
        "format coverage: {}% of bytes parsed into known structures",
        dc2tk_py::format_f(summary["coverage"].as_f64().unwrap_or(0.0) * 100.0, 1)
    ));
    lines.push(format!("assembly fallbacks: {} raw .op or .bytes lines", summary["fallbacks"]));
    lines.push(format!(
        "source: {} script lines, {} functions left as asm blocks",
        summary["source_lines"], summary["asm_functions"]
    ));
    lines.push(format!(
        "libraries: {} files, {} lines; median script includes {}",
        summary["libraries"], summary["library_lines"], summary["median_includes"]
    ));
    let unregistered = summary["unregistered"].as_object().cloned().unwrap_or_default();
    let unregistered_calls: u64 = unregistered.values().filter_map(Value::as_u64).sum();
    lines.push(format!(
        "native calls: {}, of which {} use numbers the game does not register",
        summary["native_calls"], unregistered_calls
    ));
    if !unregistered.is_empty() {
        lines.push(format!(
            "  {}",
            unregistered.iter().map(|(key, count)| format!("{key} x{count}")).collect::<Vec<_>>().join(", ")
        ));
    }
    let (shown, total) = failures(report, limit);
    if total > 0 {
        lines.push(String::new());
        lines.push(format!("{} failures, first {}:", total, shown.len()));
        lines.extend(shown);
    }
    lines.push(String::new());
    lines.push(if summary["ok"] == true { "OK" } else { "FAILED" }.into());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    const UNREGISTERED: [&str; 22] = [
        "action:6",
        "action:7",
        "action:13",
        "action:14",
        "action:44",
        "action:80",
        "effect:250",
        "event:492",
        "event:800",
        "monster:17",
        "monster:18",
        "monster:20",
        "monster:67",
        "monster:68",
        "monster:69",
        "monster:141",
        "monster:142",
        "monster:143",
        "monster:154",
        "monster:155",
        "monster:157",
        "monster:159",
    ];

    fn tables() -> BTreeMap<String, Table> {
        natives::load(&files::natives_dir()).unwrap()
    }

    #[test]
    fn reports_every_script() {
        let root = std::env::temp_dir().join(format!("dc2tk-verify-{}", std::process::id()));
        std::fs::create_dir_all(root.join("map")).unwrap();
        std::fs::write(root.join("map/t01.stb"), dc2tk_disc::samples::sample_script()).unwrap();
        std::fs::write(root.join("map/T02.STB"), dc2tk_disc::samples::sample_script()).unwrap();
        std::fs::write(root.join("map/t01.cfg"), b"not a script").unwrap();
        let report = verify(&root, &tables(), Definitions::empty(), indexmap::IndexMap::new());
        std::fs::remove_dir_all(&root).unwrap();
        let report = report.unwrap();
        let summary = &report["summary"];
        assert_eq!(summary["scripts"], 2);
        for stage in STAGES {
            assert_eq!(summary["stages"][stage]["pass"], 2, "{stage}");
        }
        assert_eq!(summary["fallbacks"], 0);
        assert_eq!(summary["asm_functions"], 4);
        assert_eq!(summary["native_calls"], 2);
        assert_eq!(summary["unregistered"], json!({}));
        assert_eq!(summary["ok"], true);
        assert_eq!(summary["coverage"], 1.0);
        assert!(format_report(&report, 5).contains("OK"));
    }

    /// The whole disc round-trips: set C2S_CORPUS to an extracted disc to run it (40 s).
    #[test]
    fn corpus_round_trip() {
        let Some(corpus) = std::env::var_os("C2S_CORPUS") else {
            return;
        };
        let defs = dc2tk_script::definitions::load(&[files::default_defs()]).unwrap();
        let headers = dc2tk_script::sdk::header_texts(&defs, &files::root().join("sdk")).unwrap();
        let report = verify(Path::new(&corpus), &tables(), defs, headers).unwrap();
        let summary = &report["summary"];
        let text = format_report(&report, 20);
        assert!(summary["scripts"].as_u64().unwrap() > 0, "no .stb files under {corpus:?}");
        assert_eq!(summary["ok"], true, "{text}");
        assert_eq!(summary["coverage"], 1.0, "{text}");
        let found: Vec<&String> = summary["unregistered"].as_object().unwrap().keys().collect();
        let mut wanted: Vec<&str> = UNREGISTERED.to_vec();
        wanted.sort();
        assert_eq!(found, wanted, "{text}");
        assert_eq!(summary["fallbacks"], 0, "{text}");
        assert_eq!(summary["asm_functions"], 0, "{text}");
    }
}

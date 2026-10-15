//! dc2tk verify EXTRACTED: round-trips every script of an extracted disc through each stage (the
//! .stb format, native calls, assembly, and C source) and reports what does not come back the
//! same.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use indexmap::IndexMap;
use rayon::prelude::*;
use serde::Serialize;

use dc2tk_script::definitions::Definitions;
use dc2tk_script::natives::{self, Table};
use dc2tk_script::{asm, assets, compile, disc, library, parse, paths, slots, source, stack, stb};

/// The report `--json` writes: the summary, then one result per script.
#[derive(Serialize)]
pub struct Report {
    pub summary: Summary,
    pub files: Vec<ScriptReport>,
}

#[derive(Serialize)]
pub struct Summary {
    pub scripts: usize,
    pub stages: Stages<Counts>,
    /// The share of the scripts' bytes the format parses into known structures, averaged over
    /// the scripts that pass.
    pub coverage: f64,
    pub native_calls: usize,
    /// Raw .op or .bytes lines of the assembly.
    pub fallbacks: usize,
    /// Functions the source leaves as asm blocks.
    pub asm_functions: usize,
    pub source_lines: usize,
    pub median_includes: usize,
    /// Calls of natives the game does not register, by table:number.
    pub unregistered: BTreeMap<String, usize>,
    /// Whether every stage of every script passed or is pending.
    pub ok: bool,
    pub libraries: usize,
    pub library_lines: usize,
}

/// Something for each stage, in the order a script goes through them.
#[derive(Serialize, Default)]
pub struct Stages<T> {
    pub format: T,
    pub natives: T,
    pub assembly: T,
    pub source: T,
}

/// How many scripts came out of a stage each way.
#[derive(Serialize, Default)]
pub struct Counts {
    pub pass: usize,
    pub fail: usize,
    pub error: usize,
    pub pending: usize,
}

/// One script through every stage.
#[derive(Serialize)]
pub struct ScriptReport {
    pub path: String,
    pub size: usize,
    pub format: Outcome<FormatPass>,
    pub natives: Outcome<NativesPass>,
    pub assembly: Outcome<AssemblyPass>,
    pub source: Outcome<SourcePass>,
}

/// How a stage went: passed (with what it found), rebuilt different bytes (fail), could not run
/// (error), or not run since there is nothing to check against (pending).
#[derive(Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum Outcome<P> {
    Pass(P),
    Fail { detail: String },
    Error { detail: String },
    Pending,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Pass,
    Fail,
    Error,
    Pending,
}

impl Status {
    fn name(self) -> &'static str {
        match self {
            Status::Pass => "pass",
            Status::Fail => "fail",
            Status::Error => "error",
            Status::Pending => "pending",
        }
    }
}

impl<P> Outcome<P> {
    pub fn status(&self) -> Status {
        match self {
            Outcome::Pass(_) => Status::Pass,
            Outcome::Fail { .. } => Status::Fail,
            Outcome::Error { .. } => Status::Error,
            Outcome::Pending => Status::Pending,
        }
    }

    pub fn passed(&self) -> Option<&P> {
        match self {
            Outcome::Pass(found) => Some(found),
            _ => None,
        }
    }

    fn detail(&self) -> &str {
        match self {
            Outcome::Fail { detail } | Outcome::Error { detail } => detail,
            Outcome::Pass(_) | Outcome::Pending => "",
        }
    }

    fn error(detail: impl std::fmt::Display) -> Outcome<P> {
        Outcome::Error { detail: detail.to_string() }
    }

    /// Pass when the rebuilt bytes are the original's, else fail saying where they differ.
    fn compared(original: &[u8], rebuilt: &[u8], found: impl FnOnce() -> P) -> Outcome<P> {
        match compare(original, rebuilt) {
            Some(detail) => Outcome::Fail { detail },
            None => Outcome::Pass(found()),
        }
    }
}

#[derive(Serialize)]
pub struct FormatPass {
    pub coverage: f64,
}

#[derive(Serialize)]
pub struct NativesPass {
    pub table: String,
    pub calls: usize,
    pub unregistered: IndexMap<String, usize>,
}

#[derive(Serialize)]
pub struct AssemblyPass {
    pub fallbacks: usize,
}

#[derive(Serialize)]
pub struct SourcePass {
    pub asm_functions: usize,
    pub lines: usize,
    pub includes: usize,
}

impl ScriptReport {
    /// Each stage's name, status and detail.
    fn stages(&self) -> [(&'static str, Status, &str); 4] {
        [
            ("format", self.format.status(), self.format.detail()),
            ("natives", self.natives.status(), self.natives.detail()),
            ("assembly", self.assembly.status(), self.assembly.detail()),
            ("source", self.source.status(), self.source.detail()),
        ]
    }
}

fn compare(original: &[u8], rebuilt: &[u8]) -> Option<String> {
    if rebuilt == original {
        return None;
    }
    let first = original.iter().zip(rebuilt).position(|(a, b)| a != b).unwrap_or(original.len().min(rebuilt.len()));
    Some(format!("first difference at 0x{:x} (sizes 0x{:x} and 0x{:x})", first, original.len(), rebuilt.len()))
}

fn check_format(data: &[u8], script: &stb::Script) -> Outcome<FormatPass> {
    match stb::write(script) {
        Err(problem) => Outcome::error(problem),
        Ok(rebuilt) => Outcome::compared(data, &rebuilt, || FormatPass { coverage: stb::coverage(script) }),
    }
}

fn check_natives(
    path: &str,
    script: &stb::Script,
    tables: &BTreeMap<String, Table>,
    rules: &[(String, String)],
) -> Outcome<NativesPass> {
    let table = natives::table_for(path, rules);
    let Some(found) = tables.get(table) else {
        return Outcome::Pending;
    };
    let calls = stack::native_calls(script);
    let known = found.numbered();
    let mut unregistered: IndexMap<String, usize> = IndexMap::new();
    for call in &calls {
        let Some(number) = call.no else {
            return Outcome::Fail {
                detail: format!("no native number for EXT {} in function at 0x{:x}", call.index, call.function),
            };
        };
        if !known.contains_key(&number) {
            *unregistered.entry(format!("{table}:{number}")).or_default() += 1;
        }
    }
    Outcome::Pass(NativesPass { table: table.to_string(), calls: calls.len(), unregistered })
}

fn check_assembly(
    path: &str,
    data: &[u8],
    script: &stb::Script,
    tables: &BTreeMap<String, Table>,
    rules: &[(String, String)],
) -> Outcome<AssemblyPass> {
    let text = asm::disassemble(script, natives::table_for(path, rules), tables);
    match asm::assemble(&text, tables) {
        Err(problem) => Outcome::error(problem),
        Ok(rebuilt) => Outcome::compared(data, &rebuilt, || {
            let fallbacks = text
                .split('\n')
                .filter(|line| {
                    let line = line.trim();
                    line.starts_with(".op ") || line.starts_with(".bytes ")
                })
                .count();
            AssemblyPass { fallbacks }
        }),
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
    texts: &IndexMap<String, String>,
    headers: &IndexMap<String, String>,
    tables: &BTreeMap<String, Table>,
) -> Outcome<SourcePass> {
    let program = &model.survey.programs[path];
    let result = (|| -> dc2tk_script::Result<(String, dc2tk_script::node::Node, Vec<u8>)> {
        let text = source::text(&model.script_program(program, path)?)?;
        let options = parse::ParseOptions { headers: Some(headers), libraries: Some(texts), ..Default::default() };
        let parsed = parse::parse_with(&text, &options)?;
        let rebuilt = compile::compile_program(&library::expand(parsed.clone(), texts, headers)?, tables)?;
        Ok((text, parsed, rebuilt))
    })();
    match result {
        Err(problem) => Outcome::error(problem),
        Ok((text, parsed, rebuilt)) => Outcome::compared(data, &rebuilt, || {
            let includes = parsed["functions"]
                .as_array()
                .map_or(0, |items| items.iter().filter(|item| item.kind() == "include").count());
            SourcePass { asm_functions: asm_functions(program), lines: text.matches('\n').count(), includes }
        }),
    }
}

/// The counts and totals over every script's results.
fn summarize(results: &[ScriptReport], texts: &IndexMap<String, String>) -> Summary {
    let mut stages: Stages<Counts> = Stages::default();
    for result in results {
        let outcomes = result.stages();
        for (counts, (_, status, _)) in
            [&mut stages.format, &mut stages.natives, &mut stages.assembly, &mut stages.source]
                .into_iter()
                .zip(outcomes)
        {
            match status {
                Status::Pass => counts.pass += 1,
                Status::Fail => counts.fail += 1,
                Status::Error => counts.error += 1,
                Status::Pending => counts.pending += 1,
            }
        }
    }
    let covered: Vec<f64> =
        results.iter().filter_map(|result| result.format.passed()).map(|pass| pass.coverage).collect();
    let sources = || results.iter().filter_map(|result| result.source.passed());
    let mut includes: Vec<usize> =
        results.iter().map(|result| result.source.passed().map_or(0, |pass| pass.includes)).collect();
    includes.sort();
    let mut calls = 0;
    let mut unregistered: BTreeMap<String, usize> = BTreeMap::new();
    for found in results.iter().filter_map(|result| result.natives.passed()) {
        calls += found.calls;
        for (key, count) in &found.unregistered {
            *unregistered.entry(key.clone()).or_default() += count;
        }
    }
    let clean = |counts: &Counts| counts.fail == 0 && counts.error == 0;
    let ok = [&stages.format, &stages.natives, &stages.assembly, &stages.source].into_iter().all(clean);
    Summary {
        scripts: results.len(),
        coverage: if covered.is_empty() { 0.0 } else { covered.iter().sum::<f64>() / covered.len() as f64 },
        native_calls: calls,
        fallbacks: results.iter().filter_map(|result| result.assembly.passed()).map(|pass| pass.fallbacks).sum(),
        asm_functions: sources().map(|pass| pass.asm_functions).sum(),
        source_lines: sources().map(|pass| pass.lines).sum(),
        median_includes: includes.get(includes.len() / 2).copied().unwrap_or(0),
        unregistered,
        ok,
        libraries: texts.len(),
        library_lines: texts.values().map(|text| text.matches('\n').count()).sum(),
        stages,
    }
}

/// Every script of an extracted disc through every stage.
pub fn verify(
    root: &Path,
    tables: &BTreeMap<String, Table>,
    defs: Definitions,
    headers: IndexMap<String, String>,
) -> Result<Report> {
    let paths = paths::scripts_under(root)?;
    let (defs, mut headers, _) = disc::with_header(root, defs, headers);
    let model = library::model_for(root, &paths, tables, &defs, &headers)?;
    if let Some(text) = headers.get(disc::HEADER).cloned() {
        // the names a disc decompile adds to disc.h, which the presented scripts use
        let named = assets::with_header(
            slots::with_header(Some(text), &model.naming.folder_slots, &model.naming.villagers),
            &model.naming.asset_names,
        );
        headers.insert(disc::HEADER.to_string(), named.unwrap_or_default());
    }
    let texts = model.library_texts()?;
    let files: Vec<ScriptReport> = paths
        .par_iter()
        .map(|path| -> Result<ScriptReport> {
            let file = root.join(path);
            let data = std::fs::read(&file).with_context(|| format!("cannot read {}", file.display()))?;
            let script = stb::read(&data);
            Ok(ScriptReport {
                path: path.clone(),
                size: data.len(),
                format: check_format(&data, &script),
                natives: check_natives(path, &script, tables, &defs.tables),
                assembly: check_assembly(path, &data, &script, tables, &defs.tables),
                source: check_source(path, &data, &model, &texts, &headers, tables),
            })
        })
        .collect::<Result<_>>()?;
    Ok(Report { summary: summarize(&files, &texts), files })
}

/// The stages that failed or could not run, one line each, the first `limit`; and how many there
/// are.
fn failures(report: &Report, limit: usize) -> (Vec<String>, usize) {
    let mut lines = Vec::new();
    for result in &report.files {
        for (stage, status, detail) in result.stages() {
            if matches!(status, Status::Fail | Status::Error) {
                lines.push(format!("{}  {}  {}: {}", stage, status.name(), result.path, detail));
            }
        }
    }
    let total = lines.len();
    lines.truncate(limit);
    (lines, total)
}

pub fn format_report(report: &Report, limit: usize) -> String {
    let summary = &report.summary;
    let mut lines = vec![format!("{} scripts", summary.scripts), String::new()];
    lines.push(format!("{:<10} {:>8} {:>8} {:>8} {:>8}", "stage", "pass", "fail", "error", "pending"));
    let stages = &summary.stages;
    for (stage, counts) in [
        ("format", &stages.format),
        ("natives", &stages.natives),
        ("assembly", &stages.assembly),
        ("source", &stages.source),
    ] {
        lines.push(format!(
            "{:<10} {:>8} {:>8} {:>8} {:>8}",
            stage, counts.pass, counts.fail, counts.error, counts.pending
        ));
    }
    lines.push(String::new());
    lines.push(format!("format coverage: {:.1}% of bytes parsed into known structures", summary.coverage * 100.0));
    lines.push(format!("assembly fallbacks: {} raw .op or .bytes lines", summary.fallbacks));
    lines.push(format!(
        "source: {} script lines, {} functions left as asm blocks",
        summary.source_lines, summary.asm_functions
    ));
    lines.push(format!(
        "libraries: {} files, {} lines; median script includes {}",
        summary.libraries, summary.library_lines, summary.median_includes
    ));
    let unregistered_calls: usize = summary.unregistered.values().sum();
    lines.push(format!(
        "native calls: {}, of which {} use numbers the game does not register",
        summary.native_calls, unregistered_calls
    ));
    if !summary.unregistered.is_empty() {
        lines.push(format!(
            "  {}",
            summary.unregistered.iter().map(|(key, count)| format!("{key} x{count}")).collect::<Vec<_>>().join(", ")
        ));
    }
    let (shown, total) = failures(report, limit);
    if total > 0 {
        lines.push(String::new());
        lines.push(format!("{} failures, first {}:", total, shown.len()));
        lines.extend(shown);
    }
    lines.push(String::new());
    lines.push(if summary.ok { "OK" } else { "FAILED" }.into());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files;

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
        let report = verify(&root, &tables(), Definitions::empty(), IndexMap::new());
        std::fs::remove_dir_all(&root).unwrap();
        let report = report.unwrap();
        let summary = &report.summary;
        assert_eq!(summary.scripts, 2);
        for counts in
            [&summary.stages.format, &summary.stages.natives, &summary.stages.assembly, &summary.stages.source]
        {
            assert_eq!(counts.pass, 2);
        }
        assert_eq!(summary.fallbacks, 0);
        assert_eq!(summary.asm_functions, 4);
        assert_eq!(summary.native_calls, 2);
        assert!(summary.unregistered.is_empty());
        assert!(summary.ok);
        assert_eq!(summary.coverage, 1.0);
        assert!(format_report(&report, 5).contains("OK"));
    }

    /// --json writes the report with the keys in this order, each stage's status first.
    #[test]
    fn the_json_report_has_a_status_for_every_stage() {
        let pass = ScriptReport {
            path: "map/t01.stb".into(),
            size: 4,
            format: Outcome::Pass(FormatPass { coverage: 1.0 }),
            natives: Outcome::Pass(NativesPass {
                table: "event".into(),
                calls: 1,
                unregistered: IndexMap::from([("event:800".to_string(), 1)]),
            }),
            assembly: Outcome::Fail { detail: "first difference at 0x0".into() },
            source: Outcome::Pending,
        };
        assert_eq!(
            serde_json::to_string(&pass).unwrap(),
            r#"{"path":"map/t01.stb","size":4,"format":{"status":"pass","coverage":1.0},"natives":{"status":"pass","table":"event","calls":1,"unregistered":{"event:800":1}},"assembly":{"status":"fail","detail":"first difference at 0x0"},"source":{"status":"pending"}}"#
        );
        let summary = summarize(std::slice::from_ref(&pass), &IndexMap::new());
        assert!(!summary.ok);
        assert_eq!((summary.stages.assembly.fail, summary.stages.source.pending), (1, 1));
        let keys: Vec<String> = match serde_json::to_value(&summary).unwrap() {
            serde_json::Value::Object(map) => map.keys().cloned().collect(),
            _ => unreachable!(),
        };
        assert_eq!(
            keys,
            [
                "scripts",
                "stages",
                "coverage",
                "native_calls",
                "fallbacks",
                "asm_functions",
                "source_lines",
                "median_includes",
                "unregistered",
                "ok",
                "libraries",
                "library_lines"
            ]
        );
        let report = Report { summary, files: vec![pass] };
        assert!(format_report(&report, 5).contains("assembly  fail  map/t01.stb: first difference at 0x0"));
    }

    /// The whole disc round-trips: set C2S_CORPUS to an extracted disc to run it (40 s).
    #[test]
    #[ignore = "needs C2S_CORPUS"]
    fn corpus_round_trip() {
        let corpus = dc2tk_disc::samples::corpus();
        let defs = dc2tk_script::definitions::load(&[files::default_defs()]).unwrap();
        let headers = dc2tk_script::sdk::header_texts(&defs, &files::root().join("sdk")).unwrap();
        let report = verify(Path::new(&corpus), &tables(), defs, headers).unwrap();
        let summary = &report.summary;
        let text = format_report(&report, 20);
        assert!(summary.scripts > 0, "no .stb files under {corpus:?}");
        assert!(summary.ok, "{text}");
        assert_eq!(summary.coverage, 1.0, "{text}");
        let found: Vec<&String> = summary.unregistered.keys().collect();
        let mut wanted: Vec<&str> = UNREGISTERED.to_vec();
        wanted.sort();
        assert_eq!(found, wanted, "{text}");
        assert_eq!(summary.fallbacks, 0, "{text}");
        assert_eq!(summary.asm_functions, 0, "{text}");
    }
}

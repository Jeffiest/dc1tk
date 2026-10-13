//! c2: decompile, compile and mod Dark Chronicle. The Rust port of cli.py; commands print what
//! cli.py's do.

mod files;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use rayon::prelude::*;

use c2_disc::{extract, rebuild, Image};
use c2_script::{asm, decompile, definitions, disc, names, natives, notes, sdk, source, stb};

#[derive(Parser)]
#[command(name = "c2", about = "Decompile, compile and mod Dark Chronicle (Dark Cloud 2)")]
struct Cli {
    /// Print how long the command took.
    #[arg(long, global = true)]
    time: bool,
    #[command(subcommand)]
    command: Command,
}

/// The options most script commands share.
#[derive(Args, Clone)]
struct Common {
    /// Directory of native table JSON files.
    #[arg(long)]
    natives: Option<PathBuf>,
    /// Directory of SDK headers.
    #[arg(long)]
    sdk: Option<PathBuf>,
    /// Definition set to apply; repeat to layer sets (later ones win); the shipped PAL set when
    /// omitted; none for no definitions.
    #[arg(long)]
    defs: Vec<PathBuf>,
}

impl Common {
    fn natives_dir(&self) -> PathBuf {
        self.natives.clone().unwrap_or_else(files::natives_dir)
    }

    fn tables(&self) -> Result<BTreeMap<String, natives::Table>> {
        Ok(natives::load(&self.natives_dir())?)
    }

    fn def_sets(&self) -> Vec<PathBuf> {
        if self.defs.len() == 1 && self.defs[0] == Path::new("none") {
            return vec![];
        }
        if self.defs.is_empty() {
            vec![files::default_defs()]
        } else {
            self.defs.clone()
        }
    }

    fn table_rules(&self) -> Result<Vec<(String, String)>> {
        Ok(definitions::table_rules(&self.def_sets())?)
    }

    fn sdk_dir(&self) -> PathBuf {
        self.sdk.clone().unwrap_or_else(|| files::root().join("sdk"))
    }

    fn definitions(&self) -> Result<definitions::Definitions> {
        let sets = self.def_sets();
        if sets.is_empty() {
            return Ok(definitions::Definitions::empty());
        }
        Ok(definitions::load(&sets)?)
    }
}

#[derive(Subcommand)]
enum Command {
    /// Write every file in the disc's DATA.DAT.
    Extract { iso: PathBuf, out: String },
    /// Write a script as assembly text.
    Disassemble {
        stb: PathBuf,
        /// File to write; standard output when omitted.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Native table; chosen by the definitions' path rules when omitted.
        #[arg(long, value_parser = natives::TABLES)]
        table: Option<String>,
        #[command(flatten)]
        common: Common,
    },
    /// Write a script as C source.
    Decompile {
        stb: PathBuf,
        /// File to write; standard output when omitted.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Native table; chosen by the definitions' path rules when omitted.
        #[arg(long, value_parser = natives::TABLES)]
        table: Option<String>,
        #[command(flatten)]
        common: Common,
    },
    /// Decompile every script under a folder, one at a time as decompile does, into OUT/<path>.c.
    #[command(hide = true)]
    ScriptSources {
        root: PathBuf,
        out: PathBuf,
        #[command(flatten)]
        common: Common,
    },
    /// Build a script from assembly text.
    Assemble {
        source: PathBuf,
        /// Script file to write.
        #[arg(short, long)]
        output: PathBuf,
        /// Directory of native table JSON files.
        #[arg(long)]
        natives: Option<PathBuf>,
    },
    /// Write a copy of an image with files of DATA.DAT replaced or added, from a folder holding
    /// files.json (a list of disc paths) and the files 0.bin, 1.bin, ... in that order.
    #[command(hide = true)]
    WriteDisc { iso: PathBuf, out: PathBuf, files: PathBuf },
    /// Disassemble every script under a folder into OUT/<path>.asm (\n line endings).
    #[command(hide = true)]
    DisassembleTree {
        root: PathBuf,
        out: PathBuf,
        #[command(flatten)]
        common: Common,
    },
    /// Decompile every script under a folder into OUT/<path>.json: the program tree before names
    /// and docs, as compact JSON.
    #[command(hide = true)]
    DecompileDump {
        root: PathBuf,
        out: PathBuf,
        /// Write the program as source text (OUT/<path>.c) instead of JSON.
        #[arg(long)]
        source: bool,
        /// Apply the definitions' names, script entries and translations.
        #[arg(long)]
        named: bool,
        #[command(flatten)]
        common: Common,
    },
    /// Print what the source index reads from a tree, as JSON: outlines, includers, header
    /// constants, and the calls to the named functions with each argument's folded value.
    #[command(hide = true)]
    IndexDump {
        tree: PathBuf,
        names: Vec<String>,
        #[command(flatten)]
        common: Common,
    },
    /// Compile every resolved program tree under a folder (<path>.json, as the Python's
    /// compile.to_asm receives them) to OUT/<path>.asm and OUT/<path>.stb.
    #[command(hide = true)]
    ToAsmTree {
        trees: PathBuf,
        out: PathBuf,
        #[arg(long)]
        natives: Option<PathBuf>,
    },
    /// Resolve every program tree under a folder (<path>.json, as the Python's sdk.resolve
    /// receives them) and write the result to OUT/<path>.json.
    #[command(hide = true)]
    ResolveTree {
        trees: PathBuf,
        out: PathBuf,
        #[arg(long)]
        natives: Option<PathBuf>,
    },
    /// Print the disc.h a decompile would write for an extracted disc.
    #[command(hide = true)]
    DiscHeader {
        root: PathBuf,
        #[command(flatten)]
        common: Common,
    },
    /// Assemble every OUT/<path>.asm a disassemble-tree wrote and compare with the script it came
    /// from; prints the ones that differ.
    #[command(hide = true)]
    AssembleTree {
        root: PathBuf,
        asm: PathBuf,
        #[arg(long)]
        natives: Option<PathBuf>,
    },
}

fn run_extract(iso: &Path, out: &str) -> Result<ExitCode> {
    let image = Image::open(iso).with_context(|| format!("cannot read {}", iso.display()))?;
    let manifest = extract::extract(image.bytes(), Path::new(out))?;
    let packed = manifest.files.iter().filter(|item| item.packed).count();
    println!("{} files written to {} ({} from packs)", manifest.files.len(), out, packed);
    println!("{} scripts listed in {}", manifest.scripts.len(), files::python_path(&format!("{out}/manifest.json")));
    Ok(ExitCode::SUCCESS)
}

fn chosen_table(table: &Option<String>, path: &Path, rules: &[(String, String)]) -> String {
    match table {
        Some(table) => table.clone(),
        None => natives::table_for(&definitions::resolved_posix(path), rules).to_string(),
    }
}

fn run_disassemble(path: &Path, output: Option<&Path>, table: &Option<String>, common: &Common) -> Result<ExitCode> {
    let data = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    let table = chosen_table(table, path, &common.table_rules()?);
    let text = asm::disassemble(&stb::read(&data), &table, &common.tables()?);
    files::write_output(output, &text)?;
    Ok(ExitCode::SUCCESS)
}

fn run_assemble(source: &Path, output: &Path, natives_dir: &Option<PathBuf>) -> Result<ExitCode> {
    let text = files::read_text(source)?;
    let tables = natives::load(&natives_dir.clone().unwrap_or_else(files::natives_dir))?;
    let data = asm::assemble(&text, &tables).with_context(|| source.display().to_string())?;
    std::fs::write(output, data).with_context(|| format!("cannot write {}", output.display()))?;
    Ok(ExitCode::SUCCESS)
}

fn run_disassemble_tree(root: &Path, out: &Path, common: &Common) -> Result<ExitCode> {
    let rules = common.table_rules()?;
    let tables = common.tables()?;
    let scripts = files::find_scripts(root)?;
    scripts.par_iter().try_for_each(|relative| -> Result<()> {
        let path = root.join(relative);
        let data = std::fs::read(&path)?;
        let text = asm::disassemble(&stb::read(&data), &chosen_table(&None, &path, &rules), &tables);
        let target = out.join(format!("{relative}.asm"));
        std::fs::create_dir_all(target.parent().unwrap())?;
        std::fs::write(target, text)?;
        Ok(())
    })?;
    println!("{} scripts disassembled into {}", scripts.len(), out.display());
    Ok(ExitCode::SUCCESS)
}

fn run_decompile_dump(root: &Path, out: &Path, as_source: bool, named: bool, common: &Common) -> Result<ExitCode> {
    let defs = common.definitions()?;
    let rules = &defs.tables;
    let tables = common.tables()?;
    let known = names::Known::new(names::merged(&defs.names));
    let nothing = (HashSet::new(), HashMap::new(), HashMap::new());
    let scripts = files::find_scripts(root)?;
    scripts.par_iter().try_for_each(|relative| -> Result<()> {
        let path = root.join(relative);
        let script = stb::read(&std::fs::read(&path)?);
        let mut program = decompile::decompile(&script, &chosen_table(&None, &path, rules), &tables);
        if named {
            // names, the script's entry and translated strings, without the SDK's names yet
            let lookup = known.with_loose(&names::loose_map(&program));
            program = names::apply(program, &lookup, &nothing.0, &nothing.1).with_context(|| relative.clone())?;
            let entry = definitions::script_entry(&defs, &path.to_string_lossy()).cloned().unwrap_or_default();
            program = names::with_script(program, &entry, &nothing.0, &nothing.2);
            program = notes::annotate(program, &defs.strings);
        }
        let (target, text) = if as_source {
            (out.join(format!("{relative}.c")), source::text(&program).with_context(|| relative.clone())?)
        } else {
            (out.join(format!("{relative}.json")), serde_json::to_string(&program)?)
        };
        std::fs::create_dir_all(target.parent().unwrap())?;
        std::fs::write(target, text)?;
        Ok(())
    })?;
    println!("{} scripts decompiled into {}", scripts.len(), out.display());
    Ok(ExitCode::SUCCESS)
}

fn decompile_setup(common: &Common) -> Result<c2_script::decompiler::Setup> {
    Ok(c2_script::decompiler::Setup::new(common.definitions()?, common.tables()?, &common.sdk_dir())?)
}

fn run_decompile(path: &Path, output: Option<&Path>, table: &Option<String>, common: &Common) -> Result<ExitCode> {
    if path.is_dir() {
        anyhow::bail!("decompiling a whole extracted disc is not ported yet; use cli.py decompile for that");
    }
    let setup = decompile_setup(common)?;
    let table = chosen_table(table, path, &setup.defs.tables);
    let text = c2_script::decompiler::script_source(&setup, path, &table)?;
    files::write_output(output, &text)?;
    Ok(ExitCode::SUCCESS)
}

fn run_script_sources(root: &Path, out: &Path, common: &Common) -> Result<ExitCode> {
    let setup = decompile_setup(common)?;
    let scripts = files::find_scripts(root)?;
    let failed: Vec<String> = scripts
        .par_iter()
        .filter_map(|relative| {
            let path = root.join(relative);
            let result = (|| -> Result<()> {
                let table = chosen_table(&None, &path, &setup.defs.tables);
                let text = c2_script::decompiler::script_source(&setup, &path, &table)?;
                let target = out.join(format!("{relative}.c"));
                std::fs::create_dir_all(target.parent().unwrap())?;
                std::fs::write(target, text)?;
                Ok(())
            })();
            result.err().map(|error| format!("{relative}: {error:#}"))
        })
        .collect();
    for line in &failed {
        println!("failed: {line}");
    }
    println!("{} scripts decompiled into {}, {} failed", scripts.len(), out.display(), failed.len());
    Ok(if failed.is_empty() { ExitCode::SUCCESS } else { ExitCode::from(1) })
}

fn run_index_dump(tree: &Path, names: &[String], common: &Common) -> Result<ExitCode> {
    use c2_script::index;
    let defs = common.definitions()?;
    let headers = index::tree_headers(tree, &defs, &common.sdk_dir())?;
    let consts = index::constants(headers.values());
    let outlines = index::outlines(tree)?;
    let outline_json: serde_json::Map<String, serde_json::Value> = outlines
        .iter()
        .map(|(path, outline)| {
            let programs: Vec<serde_json::Value> = outline
                .programs
                .iter()
                .map(|(number, function)| serde_json::json!({"number": number, "function": function}))
                .collect();
            (path.clone(), serde_json::json!({"programs": programs, "includes": outline.includes}))
        })
        .collect();
    let folded = |text: &str| match index::value(text, &consts) {
        Some(index::Folded::Int(value)) => serde_json::json!(value as i64),
        Some(index::Folded::Text(text)) => serde_json::json!(text),
        None => serde_json::Value::Null,
    };
    let names: std::collections::BTreeSet<String> = names.iter().cloned().collect();
    let calls: Vec<serde_json::Value> = index::calls(tree, &names)?
        .into_iter()
        .map(|call| {
            let values: Vec<serde_json::Value> = call.args.iter().map(|arg| folded(arg)).collect();
            serde_json::json!({"path": call.path, "line": call.line, "function": call.function, "name": call.name,
                "args": call.args, "text": call.text, "meant": call.meant, "values": values})
        })
        .collect();
    let consts: BTreeMap<&String, &i64> = consts.iter().collect();
    let dump = serde_json::json!({
        "outlines": outline_json,
        "includers": index::includers(&outlines),
        "constants": consts,
        "calls": calls,
    });
    println!("{}", c2_py::json::dumps(&dump, &c2_py::json::Options::compact()));
    Ok(ExitCode::SUCCESS)
}

fn run_to_asm_tree(trees: &Path, out: &Path, natives_dir: &Option<PathBuf>) -> Result<ExitCode> {
    let tables = natives::load(&natives_dir.clone().unwrap_or_else(files::natives_dir))?;
    let found = sdk::files_under(trees, "json");
    let failed: Vec<String> = found
        .par_iter()
        .filter_map(|relative| {
            let result = (|| -> Result<()> {
                let value: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(trees.join(relative))?)?;
                let program = c2_script::node::Node::from(value);
                let text = c2_script::compile::to_asm(&program, &tables, None)?;
                let stem = relative.strip_suffix(".json").unwrap_or(relative);
                let target = out.join(format!("{stem}.asm"));
                std::fs::create_dir_all(target.parent().unwrap())?;
                std::fs::write(&target, &text)?;
                std::fs::write(out.join(format!("{stem}.stb")), asm::assemble(&text, &tables)?)?;
                Ok(())
            })();
            result.err().map(|error| format!("{relative}: {error:#}"))
        })
        .collect();
    for line in &failed {
        println!("failed: {line}");
    }
    println!("{} programs compiled, {} failed", found.len(), failed.len());
    Ok(if failed.is_empty() { ExitCode::SUCCESS } else { ExitCode::from(1) })
}

fn run_resolve_tree(trees: &Path, out: &Path, natives_dir: &Option<PathBuf>) -> Result<ExitCode> {
    let tables = natives::load(&natives_dir.clone().unwrap_or_else(files::natives_dir))?;
    let found = sdk::files_under(trees, "json");
    let failed: Vec<String> = found
        .par_iter()
        .filter_map(|relative| {
            let result = (|| -> Result<()> {
                let value: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(trees.join(relative))?)?;
                let resolved = sdk::resolve(&c2_script::node::Node::from(value), &tables)?;
                let target = out.join(relative);
                std::fs::create_dir_all(target.parent().unwrap())?;
                std::fs::write(&target, resolved.to_string())?;
                Ok(())
            })();
            result.err().map(|error| format!("{relative}: {error:#}"))
        })
        .collect();
    for line in &failed {
        println!("failed: {line}");
    }
    println!("{} programs resolved, {} failed", found.len(), failed.len());
    Ok(if failed.is_empty() { ExitCode::SUCCESS } else { ExitCode::from(1) })
}

fn run_disc_header(root: &Path, common: &Common) -> Result<ExitCode> {
    let defs = common.definitions()?;
    let headers = sdk::header_texts(&defs, &common.sdk_dir())?;
    let (_, _, text) = disc::with_header(root, defs, headers);
    print!("{}", text.unwrap_or_default());
    Ok(ExitCode::SUCCESS)
}

fn run_assemble_tree(root: &Path, asm_dir: &Path, natives_dir: &Option<PathBuf>) -> Result<ExitCode> {
    let tables = natives::load(&natives_dir.clone().unwrap_or_else(files::natives_dir))?;
    let scripts = files::find_scripts(root)?;
    let wrong: Vec<String> = scripts
        .par_iter()
        .filter_map(|relative| {
            let original = std::fs::read(root.join(relative)).ok()?;
            let text = files::read_text(&asm_dir.join(format!("{relative}.asm"))).ok();
            let built = text.and_then(|text| asm::assemble(&text, &tables).ok());
            (built.as_deref() != Some(&original[..])).then(|| relative.clone())
        })
        .collect();
    for path in &wrong {
        println!("differs: {path}");
    }
    println!("{} scripts assembled, {} differ", scripts.len(), wrong.len());
    Ok(if wrong.is_empty() { ExitCode::SUCCESS } else { ExitCode::from(1) })
}

fn run_write_disc(iso: &Path, out: &Path, folder: &Path) -> Result<ExitCode> {
    let listed: Vec<String> = serde_json::from_str(&std::fs::read_to_string(folder.join("files.json"))?)
        .with_context(|| format!("{} is not a JSON list of paths", folder.join("files.json").display()))?;
    let mut files = BTreeMap::new();
    for (number, path) in listed.into_iter().enumerate() {
        let data = std::fs::read(folder.join(format!("{number}.bin")))?;
        files.insert(path, data);
    }
    let written = rebuild::write_disc(iso, out, &files)?;
    for item in &written.placed {
        let how = if item.added {
            "new"
        } else if item.moved {
            "moved"
        } else {
            "in place"
        };
        println!("{} {} ({} bytes)", how, item.path, item.size);
    }
    if written.grown > 0 {
        println!("the image grew by {} sectors", written.grown);
    }
    if written.placed.iter().any(|item| item.added) {
        println!(
            "the game's file index has {} bytes free for more new files (each takes 13 plus its path)",
            written.index_free
        );
    }
    Ok(ExitCode::SUCCESS)
}

fn run(command: &Command) -> Result<ExitCode> {
    match command {
        Command::Extract { iso, out } => run_extract(iso, out),
        Command::Disassemble { stb, output, table, common } => run_disassemble(stb, output.as_deref(), table, common),
        Command::Assemble { source, output, natives } => run_assemble(source, output, natives),
        Command::Decompile { stb, output, table, common } => run_decompile(stb, output.as_deref(), table, common),
        Command::ScriptSources { root, out, common } => run_script_sources(root, out, common),
        Command::WriteDisc { iso, out, files } => run_write_disc(iso, out, files),
        Command::DisassembleTree { root, out, common } => run_disassemble_tree(root, out, common),
        Command::AssembleTree { root, asm, natives } => run_assemble_tree(root, asm, natives),
        Command::DiscHeader { root, common } => run_disc_header(root, common),
        Command::IndexDump { tree, names, common } => run_index_dump(tree, names, common),
        Command::ToAsmTree { trees, out, natives } => run_to_asm_tree(trees, out, natives),
        Command::ResolveTree { trees, out, natives } => run_resolve_tree(trees, out, natives),
        Command::DecompileDump { root, out, source, named, common } => {
            run_decompile_dump(root, out, *source, *named, common)
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let start = Instant::now();
    let result = run(&cli.command);
    if cli.time {
        eprintln!("took {:.2?}", start.elapsed());
    }
    match result {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{error:#}");
            ExitCode::from(1)
        }
    }
}

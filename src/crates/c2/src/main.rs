//! c2: decompile, compile and mod Dark Chronicle. It began as the port of the Python tool cli.py,
//! whose output it gives byte for byte.

mod asset_tree;
mod assets;
mod data_cmd;
mod defs;
mod files;
mod map_cmd;
mod modding;
mod setup;
mod tree_decompile;
mod verify;
mod xref;

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
    /// Extract the retail disc and decompile every script with the definitions.
    Setup(setup::SetupArgs),
    /// Generate native function tables from a ChronicleTwo checkout.
    Natives(setup::NativesArgs),
    /// Write every file of the disc: DATA.DAT's files and the packs' members, the disc's other files
    /// and its layout.
    Extract { iso: PathBuf, out: String },
    /// Write a map as a glTF scene to edit in Blender (and build back with a mod's map: line).
    MapExport(map_cmd::MapExportArgs),
    /// What changed in the extracted folder since extracting: scripts, assets and disc files.
    Diff {
        /// The disc image the extracted folder came from.
        iso: PathBuf,
        /// The extracted folder (disc/, scripts/, assets/).
        #[arg(long, default_value = "extracted")]
        extracted: PathBuf,
    },
    /// Write a disc image back from an extracted disc (its files, DATA.DAT's files and its layout).
    Pack {
        /// The extracted disc.
        #[arg(default_value = "extracted/disc")]
        extracted: PathBuf,
        /// Image to write.
        #[arg(short, long)]
        output: PathBuf,
        /// Compare the image written with this one, byte for byte.
        #[arg(long)]
        check: Option<PathBuf>,
    },
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
    /// Write a script, or a whole extracted disc, as C source.
    Decompile {
        /// A script, or an extracted disc directory.
        stb: PathBuf,
        /// File or directory to write; standard output for one script when omitted.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Overwrite files edited since the last decompile.
        #[arg(long)]
        force: bool,
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
    /// Build a script from C source.
    Compile {
        source: PathBuf,
        /// Script file to write; the source path with .stb when omitted.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Native table, when the source does not name one.
        #[arg(long, value_parser = natives::TABLES)]
        table: Option<String>,
        #[command(flatten)]
        common: Common,
    },
    /// Compile every script of a decompiled tree and compare each with the disc's.
    #[command(hide = true)]
    CompileTree {
        tree: PathBuf,
        extracted: PathBuf,
        #[command(flatten)]
        common: Common,
    },
    /// Compile a source tree and write a new disc image with the changed scripts.
    Build(modding::BuildArgs),
    /// Make, list and apply mods: folders of changed files layered over a tree.
    Mod {
        #[command(subcommand)]
        command: modding::ModCommand,
    },
    /// Round-trip every script in an extracted disc.
    Verify {
        extracted: PathBuf,
        /// Also write the full report to this file.
        #[arg(long)]
        json: Option<PathBuf>,
        /// Failures to print.
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[command(flatten)]
        common: Common,
    },
    /// List files edited, added or removed since the tree was decompiled.
    Changes {
        /// A decompiled source tree.
        source: PathBuf,
    },
    /// Give scripts their own copy of a library, so edits to it reach only them.
    Fork {
        /// A decompiled source tree.
        source: PathBuf,
        /// The library, as scripts include it (lib/...).
        library: String,
        /// Path for the copy (lib/...).
        copy: String,
        /// Scripts to switch to the copy, relative to the tree.
        #[arg(required = true)]
        scripts: Vec<String>,
    },
    /// Boot a disc image in PCSX2.
    Play {
        /// The disc image.
        image: PathBuf,
        /// PCSX2's executable; else $PCSX2, the PATH or the usual install folder.
        #[arg(long)]
        pcsx2: Option<PathBuf>,
        /// Show the PS2 boot screens instead of skipping them.
        #[arg(long)]
        full_boot: bool,
    },
    /// Work on definition sets.
    Defs {
        #[command(subcommand)]
        command: defs::DefsCommand,
    },
    /// Save flags (or save counters) the scripts and engine use, and which are free.
    Flags {
        /// A decompiled source tree.
        source: PathBuf,
        /// List every used flag (or counter) with its name and users.
        #[arg(long)]
        used: bool,
        /// Show every use of this flag.
        #[arg(long, conflicts_with = "counter")]
        flag: Option<i64>,
        /// The 16-bit save counters (get_counter, set_counter) instead of the flags.
        #[arg(long)]
        counters: bool,
        /// Show every use of this save counter.
        #[arg(long)]
        counter: Option<i64>,
        #[command(flatten)]
        common: Common,
    },
    /// What starts an event, what enters a map, where a monster is used.
    Xref(xref::XrefArgs),
    /// Every map: how the game uses it and whether the player plays there.
    Maps(xref::MapsArgs),
    /// One map: programs and free numbers, trigger points, player positions, entries.
    Map(xref::MapArgs),
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
    #[command(flatten)]
    Assets(assets::Command),
    #[command(flatten)]
    AssetTree(asset_tree::Command),
}

fn run_extract(iso: &Path, out: &str) -> Result<ExitCode> {
    let image = Image::open(iso).with_context(|| format!("cannot read {}", iso.display()))?;
    let manifest = extract::extract(image.bytes(), Path::new(out))?;
    let packed = manifest.files.iter().filter(|item| item.packed).count();
    println!("{} files written to {} ({} from packs)", manifest.files.len(), out, packed);
    println!("{} scripts listed in {}", manifest.scripts.len(), files::python_path(&format!("{out}/manifest.json")));
    Ok(ExitCode::SUCCESS)
}

fn run_pack(extracted: &Path, output: &Path, check: Option<&Path>) -> Result<ExitCode> {
    let packed = c2_disc::layout::pack(extracted, output)?;
    println!(
        "{} written: {} disc files, {} files of DATA.DAT, {} structure sectors, {} bytes",
        output.display(),
        packed.files,
        packed.data_files,
        packed.structure_sectors,
        packed.size
    );
    let Some(original) = check else {
        return Ok(ExitCode::SUCCESS);
    };
    let (a, b) = (Image::open(original)?, Image::open(output)?);
    match c2_disc::layout::first_difference(a.bytes(), b.bytes()) {
        None => {
            println!("identical to {}, byte for byte", original.display());
            Ok(ExitCode::SUCCESS)
        }
        Some(at) => {
            println!("differs from {} at byte {} (sector {})", original.display(), at, at / 2048);
            Ok(ExitCode::from(1))
        }
    }
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

fn run_decompile(
    path: &Path,
    output: Option<&Path>,
    force: bool,
    table: &Option<String>,
    common: &Common,
) -> Result<ExitCode> {
    if path.is_dir() {
        tree_decompile::run(path, output, force, common.definitions()?, common.tables()?, &common.sdk_dir())?;
        return Ok(ExitCode::SUCCESS);
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

fn run_compile(source_path: &Path, output: Option<&Path>, table: &Option<String>, common: &Common) -> Result<ExitCode> {
    let defs = common.definitions()?;
    let headers = sdk::header_texts(&defs, &common.sdk_dir())?;
    let tables = common.tables()?;
    let sdk_dir = common.sdk_dir();
    let tools =
        c2_script::compile::Toolchain { headers: &headers, rules: &defs.tables, tables: &tables, sdk_dir: &sdk_dir };
    let options = c2_script::compile::Options { table: table.as_deref(), ..Default::default() };
    let data = c2_script::compile::compile_file(source_path, &tools, &options)
        .with_context(|| source_path.display().to_string())?;
    let output = output.map(Path::to_path_buf).unwrap_or_else(|| source_path.with_extension("stb"));
    std::fs::write(&output, data).with_context(|| format!("cannot write {}", output.display()))?;
    Ok(ExitCode::SUCCESS)
}

/// The scripts of a decompiled tree (outside lib/), by path from it.
fn source_scripts(tree: &Path) -> Vec<String> {
    sdk::files_under(tree, "c").into_iter().filter(|path| !path.starts_with("lib/")).collect()
}

fn run_compile_tree(tree: &Path, extracted: &Path, common: &Common) -> Result<ExitCode> {
    let defs = common.definitions()?;
    let headers = sdk::header_texts(&defs, &common.sdk_dir())?;
    let tables = common.tables()?;
    let sdk_dir = common.sdk_dir();
    let tools =
        c2_script::compile::Toolchain { headers: &headers, rules: &defs.tables, tables: &tables, sdk_dir: &sdk_dir };
    let scripts = source_scripts(tree);
    let results: Vec<(String, Option<String>)> = scripts
        .par_iter()
        .map(|relative| {
            let result = (|| -> Result<Option<String>> {
                let plain = c2_script::pathnames::unnamed(tree, relative)?;
                let table_path = plain.clone();
                let options = c2_script::compile::Options { table_path: Some(&table_path), ..Default::default() };
                let data = c2_script::compile::compile_file(&tree.join(relative), &tools, &options)?;
                let stem = plain.strip_suffix(".c").unwrap_or(&plain);
                for suffix in [".stb", ".STB"] {
                    if let Ok(original) = std::fs::read(extracted.join(format!("{stem}{suffix}"))) {
                        return Ok((original != data).then(|| "differs from the disc".to_string()));
                    }
                }
                Ok(Some("no script on the disc to compare".into()))
            })();
            (relative.clone(), result.unwrap_or_else(|error| Some(format!("{error:#}"))))
        })
        .collect();
    let mut wrong = 0;
    for (path, problem) in &results {
        if let Some(problem) = problem {
            wrong += 1;
            println!("{path}: {problem}");
        }
    }
    println!("{} scripts compiled, {} not the disc's bytes", results.len(), wrong);
    Ok(if wrong == 0 { ExitCode::SUCCESS } else { ExitCode::from(1) })
}

/// The flags, or the counters when counters is set: a summary, the used list, or one number's uses.
fn run_flags(source: &Path, used: bool, counters: bool, number: Option<i64>, common: &Common) -> Result<ExitCode> {
    use c2_script::{flags, index};
    let defs = common.definitions()?;
    let consts = index::constants(index::tree_headers(source, &defs, &common.sdk_dir())?.values());
    let found = if counters { flags::scan_counters(source, &consts)? } else { flags::scan(source, &consts)? };
    let text = match number {
        Some(number) => flags::who(&found, number, &consts),
        None if used => flags::used_list(&found, &consts),
        None => flags::summary(&found),
    };
    files::write_output(None, &format!("{text}\n"))?;
    Ok(ExitCode::SUCCESS)
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
        Command::Setup(args) => setup::run_setup(args),
        Command::Natives(args) => setup::run_natives(args),
        Command::Extract { iso, out } => run_extract(iso, out),
        Command::MapExport(args) => map_cmd::run_map_export(args),
        Command::Diff { iso, extracted } => {
            println!("{}", c2_script::capture::report(&c2_script::capture::changes(extracted, iso)?));
            Ok(ExitCode::SUCCESS)
        }
        Command::Pack { extracted, output, check } => run_pack(extracted, output, check.as_deref()),
        Command::Disassemble { stb, output, table, common } => run_disassemble(stb, output.as_deref(), table, common),
        Command::Assemble { source, output, natives } => run_assemble(source, output, natives),
        Command::Verify { extracted, json, limit, common } => {
            let defs = common.definitions()?;
            let headers = sdk::header_texts(&defs, &common.sdk_dir())?;
            let report = verify::verify(extracted, &common.tables()?, defs, headers)?;
            println!("{}", verify::format_report(&report, *limit));
            if let Some(json) = json {
                let options = c2_py::json::Options { ensure_ascii: false, ..c2_py::json::Options::indented(1) };
                c2_py::write_text(json, &c2_py::json::dumps(&report, &options))?;
            }
            Ok(if report["summary"]["ok"] == true { ExitCode::SUCCESS } else { ExitCode::from(1) })
        }
        Command::Changes { source } => {
            println!("{}", c2_script::tree::changes_text(source)?);
            Ok(ExitCode::SUCCESS)
        }
        Command::Fork { source, library, copy, scripts } => {
            let still = c2_script::tree::fork(source, library, copy, scripts)?;
            println!("{} now includes {}", scripts.join(", "), copy);
            println!("{} scripts still include {}", still.len(), library);
            Ok(ExitCode::SUCCESS)
        }
        Command::Build(args) => modding::run_build(args),
        Command::Mod { command } => modding::run_mod(command),
        Command::Play { image, pcsx2, full_boot } => {
            let found = modding::launch(image, pcsx2.as_deref(), !*full_boot)?;
            println!("started {} with {}", image.display(), found.display());
            Ok(ExitCode::SUCCESS)
        }
        Command::Defs { command } => defs::run(command),
        Command::Flags { source, used, flag, counters, counter, common } => {
            run_flags(source, *used, *counters || counter.is_some(), flag.or(*counter), common)
        }
        Command::Xref(args) => xref::run_xref(args),
        Command::Maps(args) => xref::run_maps(args),
        Command::Map(args) => xref::run_map(args),
        Command::Compile { source, output, table, common } => run_compile(source, output.as_deref(), table, common),
        Command::CompileTree { tree, extracted, common } => run_compile_tree(tree, extracted, common),
        Command::Decompile { stb, output, force, table, common } => {
            run_decompile(stb, output.as_deref(), *force, table, common)
        }
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
        Command::Assets(command) => assets::run(command),
        Command::AssetTree(command) => asset_tree::run(command),
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

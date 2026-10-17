//! dc2tk defs: learn a definition set from edited source, report how much of a disc it names, list
//! what it leaves unnamed, rename a constant of its headers, and record which disc it is
//! for.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::Subcommand;

use dc2tk_script::{constants, coverage, decompiler, definitions, learn, paths, sdk};

use crate::files;
use crate::options::Common;

#[derive(Subcommand)]
pub enum DefsCommand {
    /// Record names and docs from edited source into a definition set.
    Learn {
        /// An edited .c file or source tree.
        source: PathBuf,
        /// Definition set directory to update.
        into: PathBuf,
        #[command(flatten)]
        common: Common,
    },
    /// Report how much of a disc a definition set names.
    Check {
        /// An extracted disc directory.
        extracted: PathBuf,
        #[command(flatten)]
        common: Common,
    },
    /// List unnamed functions, most called first.
    Todo {
        /// An extracted disc directory.
        extracted: PathBuf,
        /// Rows to print.
        #[arg(long, default_value_t = 50)]
        limit: usize,
        #[command(flatten)]
        common: Common,
    },
    /// Rename a constant of a definition set's headers (a flag, a music file), with what the set
    /// says about it and its uses in decompiled trees and mods.
    Rename {
        /// Definition set directory whose include/ headers declare the constant.
        set: PathBuf,
        /// The constant's name now.
        old: String,
        /// Its new name.
        new: String,
        /// A decompiled tree or mod folder whose source uses the constant; repeat for more.
        #[arg(long = "in")]
        folders: Vec<PathBuf>,
        #[command(flatten)]
        common: Common,
    },
    /// Record which disc a definition set is for: its scripts' hashes.
    Disc {
        /// An extracted disc directory.
        extracted: PathBuf,
        /// Definition set directory to write disc.json into.
        into: PathBuf,
        #[command(flatten)]
        common: Common,
    },
}

pub fn run(command: &DefsCommand) -> Result<ExitCode> {
    match command {
        DefsCommand::Learn { source, into, common } => {
            let defs = common.definitions()?;
            let headers = sdk::header_texts(&defs, &common.sdk_dir())?;
            // includes are looked for in the shipped SDK folder, whatever --sdk says
            let sdk_dir = files::root().join("sdk");
            let result = learn::learn(source, into, &common.tables()?, &headers, &defs.tables, &sdk_dir)?;
            let mut learned: Vec<(&String, &usize)> = result.learned.iter().collect();
            learned.sort();
            for (table, count) in learned {
                println!("{table:<8} {count:>5} functions learned");
            }
            println!("{} scripts with globals or docs learned", result.scripts);
            for (path, error) in &result.failures {
                println!("skipped {}: {}", files::shown_path(path), error);
            }
            Ok(if result.failures.is_empty() { ExitCode::SUCCESS } else { ExitCode::from(1) })
        }
        DefsCommand::Check { extracted, common } => {
            let defs = common.definitions()?;
            let paths = paths::scripts_under(extracted)?;
            println!("{}", coverage::format_check(&coverage::check(extracted, &paths, &defs)?));
            let scripts = decompiler::read_scripts(extracted, &paths)?;
            println!("{}", decompiler::disc_match_text(&defs, &definitions::disc_differences(&scripts, &defs, "")));
            Ok(ExitCode::SUCCESS)
        }
        DefsCommand::Rename { set, old, new, folders, common } => {
            let defs = common.definitions()?;
            let headers = sdk::header_texts(&defs, &common.sdk_dir())?;
            let renamed = constants::rename(set, old, new, &headers, folders)?;
            for file in &renamed.set_files {
                println!("renamed in {}", files::shown_path(file));
            }
            for (folder, changed) in &renamed.folders {
                println!(
                    "{} file{} renamed in {}",
                    changed.len(),
                    if changed.len() == 1 { "" } else { "s" },
                    files::shown_path(folder)
                );
            }
            Ok(ExitCode::SUCCESS)
        }
        DefsCommand::Disc { extracted, into, common } => {
            common.definitions()?;
            let paths = paths::scripts_under(extracted)?;
            let count = coverage::write_disc(extracted, &paths, into)?;
            let written = into.join(definitions::DISC);
            println!("{} script hashes written to {}", count, files::shown_path(written));
            Ok(ExitCode::SUCCESS)
        }
        DefsCommand::Todo { extracted, limit, common } => {
            let defs = common.definitions()?;
            let headers = sdk::header_texts(&defs, &common.sdk_dir())?;
            let paths = paths::scripts_under(extracted)?;
            let (rows, total) = coverage::todo(extracted, &paths, &defs, &common.tables()?, &headers, *limit)?;
            println!("{total} functions still unnamed (counting language variants once); most called first:");
            for row in rows {
                println!("{:>6}  {}  {}", row.calls, row.path, row.name);
            }
            Ok(ExitCode::SUCCESS)
        }
    }
}

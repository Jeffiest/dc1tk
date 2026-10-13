//! c2 defs: learn a definition set from edited source, report how much of a disc it names, list
//! what it leaves unnamed, and record which disc it is for.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::Subcommand;
use rayon::prelude::*;

use c2_script::{coverage, definitions, learn, sdk};

use crate::files;
use crate::tree_decompile::disc_match_text;
use crate::Common;

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
        extracted: PathBuf,
        #[command(flatten)]
        common: Common,
    },
    /// List unnamed functions, most called first.
    Todo {
        extracted: PathBuf,
        /// Rows to print.
        #[arg(long, default_value_t = 50, allow_negative_numbers = true)]
        limit: i64,
        #[command(flatten)]
        common: Common,
    },
    /// Record which disc a definition set is for: its scripts' hashes.
    Disc {
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
                println!("skipped {}: {}", files::python_path(&path.to_string_lossy()), error);
            }
            Ok(if result.failures.is_empty() { ExitCode::SUCCESS } else { ExitCode::from(1) })
        }
        DefsCommand::Check { extracted, common } => {
            let defs = common.definitions()?;
            let paths = files::find_scripts(extracted)?;
            println!("{}", coverage::format_check(&coverage::check(extracted, &paths, &defs)?));
            let scripts: Vec<(String, Vec<u8>)> = paths
                .par_iter()
                .map(|path| Ok((path.clone(), std::fs::read(extracted.join(path))?)))
                .collect::<Result<_>>()?;
            println!("{}", disc_match_text(&defs, &definitions::disc_differences(&scripts, &defs, "")));
            Ok(ExitCode::SUCCESS)
        }
        DefsCommand::Disc { extracted, into, common } => {
            common.definitions()?;
            let paths = files::find_scripts(extracted)?;
            let count = coverage::write_disc(extracted, &paths, into)?;
            let written = into.join(definitions::DISC);
            println!("{} script hashes written to {}", count, files::python_path(&written.to_string_lossy()));
            Ok(ExitCode::SUCCESS)
        }
        DefsCommand::Todo { extracted, limit, common } => {
            let defs = common.definitions()?;
            let headers = sdk::header_texts(&defs, &common.sdk_dir())?;
            let paths = files::find_scripts(extracted)?;
            let (rows, total) = coverage::todo(extracted, &paths, &defs, &common.tables()?, &headers, *limit)?;
            println!("{total} functions still unnamed (counting language variants once); most called first:");
            for row in rows {
                println!("{:>6}  {}  {}", row.calls, row.path, row.name);
            }
            Ok(ExitCode::SUCCESS)
        }
    }
}

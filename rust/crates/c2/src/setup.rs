//! c2 setup ISO TREE: extract the retail disc (or reuse an extraction) and decompile every script;
//! and c2 natives CHRONICLETWO: the native tables from a ChronicleTwo checkout.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Args;

use c2_disc::{extract, Image};
use c2_script::natives;

use crate::{files, tree_decompile, Common};

#[derive(Args)]
pub struct SetupArgs {
    /// The retail disc image.
    iso: PathBuf,
    /// Directory for the decompiled source tree.
    output: PathBuf,
    /// Where to extract the disc (reused when already there).
    #[arg(long, default_value = "extracted")]
    extracted: String,
    /// Overwrite files edited since the last decompile.
    #[arg(long)]
    force: bool,
    #[command(flatten)]
    common: Common,
}

pub fn run_setup(args: &SetupArgs) -> Result<ExitCode> {
    let extracted = Path::new(&args.extracted);
    {
        let image = Image::open(&args.iso).with_context(|| format!("cannot read {}", args.iso.display()))?;
        if !extracted.join("manifest.json").is_file() {
            let manifest = extract::extract(image.bytes(), extracted)?;
            println!("{} files extracted to {}", manifest.files.len(), files::python_path(&args.extracted));
        } else {
            extract::extract_boot(image.bytes(), extracted)?;
        }
    }
    let common = &args.common;
    tree_decompile::run(
        extracted,
        Some(&args.output),
        args.force,
        common.definitions()?,
        common.tables()?,
        &common.sdk_dir(),
    )?;
    Ok(ExitCode::SUCCESS)
}

#[derive(Args)]
pub struct NativesArgs {
    /// A ChronicleTwo checkout.
    chronicletwo: PathBuf,
    /// Directory for the table JSON files.
    #[arg(long)]
    out: Option<String>,
}

pub fn run_natives(args: &NativesArgs) -> Result<ExitCode> {
    let out = args.out.clone().unwrap_or_else(|| files::natives_dir().to_string_lossy().into_owned());
    let tables = match natives::generate(&args.chronicletwo, Path::new(&out)) {
        // an error, printed as "error: ..."
        Err(c2_script::Error::Invalid(message)) => anyhow::bail!("error: {message}"),
        other => other?,
    };
    for table in &tables {
        let path = files::python_path(&format!("{out}/{}.json", table.table));
        println!("{:<8} {:>4} natives  {}", table.table, table.natives.len(), path);
    }
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use c2_script::{source, tree};

    /// A fresh folder for a test, emptied first.
    fn scratch(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("c2-setup-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    fn no_definitions() -> Common {
        Common { natives: None, sdk: None, defs: vec![PathBuf::from("none")] }
    }

    // tests/test_commands.py: decompile EXTRACTED -o OUT --defs none, extracting the sample disc
    // first when needed.
    fn decompiled(folder: &Path, force: bool) -> Result<PathBuf> {
        let extracted = folder.join("extracted");
        if !extracted.is_dir() {
            extract::extract(&c2_disc::samples::sample_disc(), &extracted)?;
        }
        let out = folder.join("source");
        let common = no_definitions();
        tree_decompile::run(&extracted, Some(&out), force, common.definitions()?, common.tables()?, &common.sdk_dir())?;
        Ok(out)
    }

    #[test]
    fn decompiling_a_disc_records_what_it_wrote_and_writes_editor_flags() {
        let folder = scratch("records");
        let out = decompiled(&folder, false).unwrap();
        assert!(out.join(source::EDITOR_FLAGS).is_file());
        assert!(tree::originals(&out).unwrap().unwrap().contains_key("map/t01.c"));
        let status = tree::status(&out).unwrap().unwrap();
        assert!(status.edited.is_empty() && status.added.is_empty() && status.removed.is_empty());
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn decompiling_again_keeps_edits_unless_forced() {
        let folder = scratch("edits");
        let out = decompiled(&folder, false).unwrap();
        decompiled(&folder, false).unwrap();
        c2_py::write_text(&out.join("map/t01.c"), "// mine\n").unwrap();
        let error = decompiled(&folder, false).expect_err("decompile should refuse to overwrite an edit");
        assert!(error.to_string().contains("map/t01.c"));
        assert_eq!(files::read_text(&out.join("map/t01.c")).unwrap(), "// mine\n");
        decompiled(&folder, true).unwrap();
        assert!(tree::status(&out).unwrap().unwrap().edited.is_empty());
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn setup_extracts_once_then_reuses_the_extraction() {
        let folder = scratch("setup");
        let iso = folder.join("disc.iso");
        std::fs::write(&iso, c2_disc::samples::sample_disc()).unwrap();
        let extracted = folder.join("extracted");
        let args = SetupArgs {
            iso,
            output: folder.join("source"),
            extracted: extracted.to_string_lossy().into_owned(),
            force: false,
            common: no_definitions(),
        };
        run_setup(&args).unwrap();
        assert!(extracted.join("manifest.json").is_file());
        assert!(folder.join("source/map/t01.c").is_file());
        // a second setup decompiles the extraction already there, without writing it again
        std::fs::remove_file(extracted.join("readme.txt")).unwrap();
        run_setup(&args).unwrap();
        assert!(!extracted.join("readme.txt").exists());
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn natives_from_a_folder_without_the_sources_is_an_error() {
        let folder = scratch("natives");
        let out = Some(folder.join("out").to_string_lossy().into_owned());
        assert!(run_natives(&NativesArgs { chronicletwo: folder.join("missing"), out }).is_err());
        std::fs::remove_dir_all(&folder).unwrap();
    }
}

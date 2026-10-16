//! dc2tk setup DISC: extract the retail disc (or reuse an extraction, or take one given instead of
//! the image), decompile every script, write the asset tree, every map as a glTF scene and every
//! data table as JSON, all under extracted/;
//! and dc2tk natives CHRONICLETWO: the native tables from a ChronicleTwo checkout.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{bail, Result};
use clap::Args;

use dc2tk_disc::{extract, DiscSource, Image};
use dc2tk_script::{datatables, decompiler, natives, sdk};

use crate::files;
use crate::options::Common;

#[derive(Args)]
pub struct SetupArgs {
    /// The retail disc: its image, or an extracted disc, which is used where it is (and
    /// --extracted is not).
    disc: PathBuf,
    /// Directory for the decompiled source tree.
    #[arg(default_value = "extracted/scripts")]
    output: PathBuf,
    /// Where to extract the disc image (reused when already there).
    #[arg(long, default_value = "extracted/disc")]
    extracted: PathBuf,
    /// Where to write the asset tree (replaced when already there).
    #[arg(long, default_value = "extracted/assets")]
    assets: PathBuf,
    /// Leave the asset tree out.
    #[arg(long)]
    no_assets: bool,
    /// Where to write every map as a glTF scene, a folder each (a map's folder replaced when
    /// already there).
    #[arg(long, default_value = crate::map_cmd::MAPS)]
    maps: PathBuf,
    /// Leave the maps out.
    #[arg(long)]
    no_maps: bool,
    /// Where to write every data table as JSON, the files `mod asset data:...` copies into a mod
    /// (replaced when already there).
    #[arg(long, default_value = "extracted/data")]
    data: PathBuf,
    /// Leave the data tables out.
    #[arg(long)]
    no_data: bool,
    /// Overwrite files edited since the last decompile.
    #[arg(long)]
    force: bool,
    #[command(flatten)]
    common: Common,
}

pub fn run_setup(args: &SetupArgs) -> Result<ExitCode> {
    let extracted = if args.disc.is_dir() {
        // an extracted disc, checked for its manifest
        DiscSource::open(&args.disc)?;
        &args.disc
    } else {
        extract_image(&args.disc, &args.extracted)?;
        &args.extracted
    };
    let common = &args.common;
    decompile_tree(extracted, Some(&args.output), args.force, common)?;
    let mut code = ExitCode::SUCCESS;
    if !args.no_assets {
        code = crate::asset_tree::run_assets(&args.disc, &args.assets, true, &common.def_sets())?;
    }
    if !args.no_maps || !args.no_data {
        let disc = DiscSource::open(extracted)?;
        if !args.no_maps {
            let maps = crate::map_cmd::export_all(&disc, &args.maps)?;
            println!("{} maps written to {} as glTF scenes", maps.len(), files::shown_path(&args.maps));
        }
        if !args.no_data {
            let tables = datatables::export_all(&disc, &args.data)?;
            println!("{tables} data tables written to {} as JSON", files::shown_path(&args.data));
        }
    }
    Ok(code)
}

/// The disc image extracted into `extracted`, or the extraction already there brought up to date
/// (the boot executable, and the disc's layout an older extraction lacks).
fn extract_image(iso: &Path, extracted: &Path) -> Result<()> {
    {
        let image = Image::open(iso)?;
        if extracted.join(extract::MANIFEST).is_file() {
            extract::extract_boot(image.bytes(), extracted)?;
            if !extracted.join(dc2tk_disc::layout::LAYOUT).is_file() {
                dc2tk_disc::layout::record(image.bytes(), extracted)?;
                println!("the disc's other files and layout written to {}", files::shown_path(extracted));
            }
        } else {
            let manifest = extract::extract(image.bytes(), extracted)?;
            println!("{} files extracted to {}", manifest.files.len(), files::shown_path(extracted));
        }
    }
    Ok(())
}

/// Decompiles an extracted disc, or a folder of one, into a source tree, saying what it wrote.
pub fn decompile_tree(extracted: &Path, output: Option<&Path>, force: bool, common: &Common) -> Result<()> {
    let Some(out) = output else {
        bail!("decompiling a directory needs -o OUT_DIR");
    };
    let written =
        decompiler::decompile_tree(extracted, out, force, common.definitions()?, &common.tables()?, &common.sdk_dir())?;
    if let Some(warning) = &written.warning {
        println!("warning: {warning}");
    }
    for path in &written.kept {
        println!("kept {path}: the decompiler no longer writes it, but it was edited");
    }
    if !written.removed.is_empty() {
        let count = written.removed.len();
        println!("removed {count} file{} the decompiler no longer writes", if count == 1 { "" } else { "s" });
    }
    println!(
        "{} scripts and {} library files written to {}",
        written.scripts,
        written.libraries,
        files::shown_path(out)
    );
    Ok(())
}

#[derive(Args)]
pub struct NativesArgs {
    /// A ChronicleTwo checkout.
    chronicletwo: PathBuf,
    /// Directory for the table JSON files; src/natives when omitted.
    #[arg(long)]
    out: Option<PathBuf>,
}

pub fn run_natives(args: &NativesArgs) -> Result<ExitCode> {
    let out = args.out.clone().unwrap_or_else(files::natives_dir);
    let tables = match natives::generate(&args.chronicletwo, &out) {
        // an error, printed as "error: ..."
        Err(error) if error.is_input() => anyhow::bail!("error: {error}"),
        other => other?,
    };
    for table in &tables {
        let path = files::shown_path(out.join(format!("{}.json", table.table)));
        println!("{:<8} {:>4} natives  {}", table.table, table.natives.len(), path);
    }
    Ok(ExitCode::SUCCESS)
}

#[derive(Args)]
pub struct ApiDocsArgs {
    /// The SDK's folder; sdk when omitted.
    #[arg(long)]
    sdk: Option<PathBuf>,
    /// Directory for the pages; docs/coding/api when omitted.
    #[arg(long)]
    out: Option<PathBuf>,
}

pub fn run_api_docs(args: &ApiDocsArgs) -> Result<ExitCode> {
    let sdk_dir = args.sdk.clone().unwrap_or_else(|| files::root().join("sdk"));
    let out = args.out.clone().unwrap_or_else(|| files::root().join("docs").join("coding").join("api"));
    for name in sdk::reference::write_pages(&sdk_dir, &out)? {
        println!("{}", files::shown_path(out.join(name)));
    }
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dc2tk_script::{source, tree};

    /// A fresh folder for a test, emptied first.
    fn scratch(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("dc2tk-setup-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    fn no_definitions() -> Common {
        Common { no_defs: true, ..Common::default() }
    }

    // decompile EXTRACTED -o OUT --no-defs, extracting the sample disc
    // first when needed.
    fn decompiled(folder: &Path, force: bool) -> Result<PathBuf> {
        let extracted = folder.join("extracted");
        if !extracted.is_dir() {
            extract::extract(&dc2tk_disc::samples::sample_disc(), &extracted)?;
        }
        let out = folder.join("source");
        let common = no_definitions();
        decompile_tree(&extracted, Some(&out), force, &common)?;
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
        dc2tk_disc::textfile::write_text(&out.join("map/t01.c"), "// mine\n").unwrap();
        let error = decompiled(&folder, false).expect_err("decompile should refuse to overwrite an edit");
        assert!(error.to_string().contains("map/t01.c"));
        assert_eq!(dc2tk_disc::textfile::read_text(&out.join("map/t01.c")).unwrap(), "// mine\n");
        decompiled(&folder, true).unwrap();
        assert!(tree::status(&out).unwrap().unwrap().edited.is_empty());
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn setup_extracts_once_then_reuses_the_extraction() {
        let folder = scratch("setup");
        let iso = folder.join("disc.iso");
        std::fs::write(&iso, dc2tk_disc::samples::sample_disc()).unwrap();
        let extracted = folder.join("extracted");
        let args = SetupArgs {
            disc: iso,
            output: folder.join("source"),
            extracted: extracted.clone(),
            assets: folder.join("assets"),
            no_assets: true,
            maps: folder.join("maps"),
            no_maps: true,
            data: folder.join("data"),
            no_data: true,
            force: false,
            common: no_definitions(),
        };
        run_setup(&args).unwrap();
        assert!(extracted.join(extract::MANIFEST).is_file());
        assert!(folder.join("source/map/t01.c").is_file());
        // a second setup decompiles the extraction already there, without writing it again
        std::fs::remove_file(extracted.join("readme.txt")).unwrap();
        run_setup(&args).unwrap();
        assert!(!extracted.join("readme.txt").exists());
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn setup_takes_an_extracted_disc_where_it_is() {
        let folder = scratch("setup-extracted");
        let disc = folder.join("disc");
        extract::extract(&dc2tk_disc::samples::sample_disc(), &disc).unwrap();
        let args = SetupArgs {
            disc: disc.clone(),
            output: folder.join("source"),
            extracted: folder.join("unused"),
            assets: folder.join("assets"),
            no_assets: true,
            maps: folder.join("maps"),
            no_maps: true,
            data: folder.join("data"),
            no_data: true,
            force: false,
            common: no_definitions(),
        };
        run_setup(&args).unwrap();
        assert!(folder.join("source/map/t01.c").is_file());
        assert!(!folder.join("unused").exists());
        // a folder that is no extracted disc says so
        let args = SetupArgs { disc: folder.join("source"), ..args };
        let error = run_setup(&args).unwrap_err().to_string();
        assert!(error.contains("is not an extracted disc"), "{error}");
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn natives_from_a_folder_without_the_sources_is_an_error() {
        let folder = scratch("natives");
        let out = Some(folder.join("out"));
        assert!(run_natives(&NativesArgs { chronicletwo: folder.join("missing"), out }).is_err());
        std::fs::remove_dir_all(&folder).unwrap();
    }
}

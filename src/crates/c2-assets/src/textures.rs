//! Textures as PNG files for mods: a folder named after the texture file plus .png
//! (c02a.img.png/) with one PNG per picture it changes, named after the picture (c02a01.png).
//! Pictures the folder leaves out stay as they are.

use std::collections::HashMap;
use std::path::Path;

use crate::{bail, im3, Error, Result};

pub const SUFFIX: &str = ".png";

/// The picture's file name: characters a file name cannot hold become "_".
pub fn file_name(name: &str) -> String {
    let clean: String =
        name.chars().map(|c| if "<>:\"/\\|?*".contains(c) || (c as u32) < 0x20 { '_' } else { c }).collect();
    clean + SUFFIX
}

/// (file name, PNG) for each picture of a texture file that has pixels, in the file's order; a
/// name given twice keeps its first place and the last picture's PNG.
pub fn export(data: &[u8]) -> Result<Vec<(String, Vec<u8>)>> {
    let mut out: Vec<(String, Vec<u8>)> = Vec::new();
    for image in im3::read(data)? {
        if image.no_image {
            continue;
        }
        let mut name = file_name(&image.name);
        if out.iter().any(|(existing, _)| *existing == name) {
            name = file_name(&format!("{}_{}", image.name, image.index.unwrap_or(0)));
        }
        let png = im3::to_png(&image, true)?;
        match out.iter_mut().find(|(existing, _)| *existing == name) {
            Some(slot) => slot.1 = png,
            None => out.push((name, png)),
        }
    }
    Ok(out)
}

/// The files of a folder whose names end in the suffix (ignoring case, as glob does on Windows),
/// sorted as pathlib sorts them.
pub(crate) fn glob_suffix(folder: &Path, suffix: &str) -> Result<Vec<std::path::PathBuf>> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(folder)? {
        let path = entry?.path();
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let matches = if cfg!(windows) { name.to_lowercase().ends_with(suffix) } else { name.ends_with(suffix) };
        if matches {
            found.push(path);
        }
    }
    Ok(crate::sorted_paths(found))
}

/// The texture file with the pictures a folder of PNGs gives in place of the original's.
pub fn build(folder: &Path, original: &[u8]) -> Result<Vec<u8>> {
    let pngs = glob_suffix(folder, SUFFIX)?
        .into_iter()
        .map(|path| {
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            Ok((name, std::fs::read(&path)?))
        })
        .collect::<Result<Vec<_>>>()?;
    build_from(&pngs, original)
}

/// build from (file name, PNG) pairs, in the order a folder's sorted listing gives them.
pub fn build_from(pngs: &[(String, Vec<u8>)], original: &[u8]) -> Result<Vec<u8>> {
    let mut images = im3::read(original)?;
    let mut by_name: HashMap<String, usize> = HashMap::new();
    for (position, image) in images.iter().enumerate() {
        by_name.entry(file_name(&image.name)).or_insert(position);
        by_name.entry(file_name(&format!("{}_{}", image.name, image.index.unwrap_or(0)))).or_insert(position);
    }
    for (name, png) in pngs {
        let Some(&position) = by_name.get(name) else {
            let mut names: Vec<String> = images
                .iter()
                .filter(|image| !image.no_image)
                .map(|image| {
                    let file = file_name(&image.name);
                    file[..file.len() - SUFFIX.len()].to_string()
                })
                .collect();
            names.sort();
            bail!("{} names no picture of the texture file (it has {})", name, names.join(", "));
        };
        if images[position].no_image {
            bail!("{name} is a render target with no pixels");
        }
        images[position] = im3::from_png(png, &images[position]).map_err(|error| match error {
            Error::Invalid(message) => Error::invalid(format!("{name}: {message}")),
            other => other,
        })?;
    }
    im3::write(&images, Some(original))
}

#[cfg(test)]
mod tests {
    use super::*;
    use im3::{Format, Image, PngMode};

    fn sample_texture_file() -> Vec<u8> {
        let mut image = Image::new("tex01", Format::T8, 16, 16);
        image.palette = Some((0..256).map(|i| [i as u8, 255 - i as u8, 0, 0x80]).collect());
        image.indices = Some((0..16).flat_map(|y| (0..16).map(move |x| ((x + y) % 256) as u8)).collect());
        let mut other = Image::new("tex02", Format::Ct32, 8, 8);
        other.pixels = Some([10, 20, 30, 0x80].repeat(64));
        im3::write(&[image, other], None).unwrap()
    }

    fn temp_folder(name: &str) -> std::path::PathBuf {
        let folder = std::env::temp_dir().join(format!("c2-assets-textures-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn a_texture_file_exports_one_png_per_picture_and_builds_back_unchanged() {
        let data = sample_texture_file();
        let exported = export(&data).unwrap();
        let names: Vec<&str> = exported.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["tex01.png", "tex02.png"]);
        let folder = temp_folder("unchanged");
        for (name, png) in &exported {
            std::fs::write(folder.join(name), png).unwrap();
        }
        assert_eq!(build(&folder, &data).unwrap(), data);
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn a_folder_changes_only_the_pictures_it_holds() {
        let data = sample_texture_file();
        let folder = temp_folder("partial");
        // tex02 in blue; tex01 is left out and keeps its pixels
        let blue = im3::write_png(8, 8, PngMode::Rgb, &[0, 0, 255].repeat(64), &[], &[]).unwrap();
        std::fs::write(folder.join("tex02.png"), blue).unwrap();
        let built = im3::read(&build(&folder, &data).unwrap()).unwrap();
        std::fs::remove_dir_all(folder).unwrap();
        assert_eq!(built[1].pixels.as_deref().unwrap(), [0, 0, 255, 0x80].repeat(64));
        assert_eq!(built[0].indices, im3::read(&data).unwrap()[0].indices);
    }

    #[test]
    fn a_png_for_no_picture_is_refused_with_the_names_there_are() {
        let data = sample_texture_file();
        let folder = temp_folder("refused");
        std::fs::write(folder.join("tex99.png"), im3::write_png(8, 8, PngMode::Rgb, &[0; 192], &[], &[]).unwrap())
            .unwrap();
        let error = build(&folder, &data).unwrap_err().to_string();
        std::fs::remove_dir_all(folder).unwrap();
        assert!(error.contains("tex01") && error.contains("tex02"), "{error}");
    }
}

//! Textures as PNG files for mods: a folder named after the texture file plus .png
//! (c02a.img.png/) with one PNG per picture it changes, named after the picture (c02a01.png).
//! Pictures the folder leaves out stay as they are. new_file makes a texture file of its own from
//! PNGs instead, each picture in the format of a template picture.

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

/// The largest side of a picture: TEX0's TW and TH go up to 2^10.
pub const MAX_SIDE: usize = 1024;

/// The picture of a texture file a new file's pictures take their format from: the one at entry
/// `picture`, or the first with pixels.
pub fn template_picture(template: &[u8], picture: Option<usize>) -> Result<im3::Image> {
    let images = im3::read(template)?;
    let found = match picture {
        Some(at) => images.into_iter().find(|image| image.index == Some(at)),
        None => images.into_iter().find(|image| !image.no_image),
    };
    match found {
        Some(image) if !image.no_image => Ok(image),
        Some(_) => bail!("the template's picture is a render target with no pixels"),
        None => bail!("the template texture file has no picture with pixels"),
    }
}

/// A new texture file holding one picture per (name, PNG), each in the format of `like` (pixel
/// format, palette size, swizzling, mip levels, header fields), with its own name and size. The
/// PNG's alpha maps to the PS2's (255 is 0x80, opaque); a paletted format takes an indexed PNG with
/// few enough colours as it is and quantises any other. Sides are 1 to 1024 texels (a multiple of
/// 16 wide for swizzled 8-bit pictures), and powers of two when `power_of_two` (a model's textures
/// wrap at the power of two above their size). Always an IM3, as the game's own files are.
pub fn new_file(pngs: &[(String, Vec<u8>)], like: &im3::Image, power_of_two: bool) -> Result<Vec<u8>> {
    if pngs.is_empty() {
        bail!("no pictures: give PNG files");
    }
    let base = picture_base(like);
    let mut images = Vec::new();
    for (name, png) in pngs {
        images.push(new_picture(name, png, &base, power_of_two, &images)?);
    }
    im3::write(&images, None)
}

/// A template whose colours read as PS2 alpha (0x80 opaque), so PNG alpha is scaled to it.
fn picture_base(like: &im3::Image) -> im3::Image {
    let mut base = like.clone();
    if base.paletted() {
        let n = base.palette.as_ref().map_or(256, Vec::len).max(1);
        base.palette = Some(vec![[0, 0, 0, 0x80]; n]);
        base.indices = None;
    } else {
        base.pixels = Some(vec![0, 0, 0, 0x80]);
    }
    base.clamp = base.clamp.or(Some(5));
    base
}

/// A new picture (no entry index yet) from a PNG in the format of `base` (picture_base), checked
/// against new_file's rules and the names of `others`.
fn new_picture(
    name: &str,
    png: &[u8],
    base: &im3::Image,
    power_of_two: bool,
    others: &[im3::Image],
) -> Result<im3::Image> {
    let fail = |message: String| Error::invalid(format!("{name}: {message}"));
    let latin1 = name.chars().all(|c| (c as u32) < 0x100);
    if name.is_empty() || !latin1 || name.chars().count() > 0x1F {
        return Err(fail("a picture's name is 1 to 31 Latin-1 characters".into()));
    }
    if others.iter().any(|image: &im3::Image| image.name.eq_ignore_ascii_case(name)) {
        return Err(fail("two pictures of this name (ignoring case)".into()));
    }
    let decoded = im3::read_png(png).map_err(|error| fail(error.to_string()))?;
    let (w, h) = (decoded.width, decoded.height);
    if !(1..=MAX_SIDE).contains(&w) || !(1..=MAX_SIDE).contains(&h) {
        return Err(fail(format!("{w}x{h}: sides are 1 to {MAX_SIDE} texels")));
    }
    if power_of_two && !(w.is_power_of_two() && h.is_power_of_two()) {
        return Err(fail(format!("{w}x{h}: a model's texture has sides that are powers of two")));
    }
    if base.format == im3::Format::T8 && base.swizzled && !w.is_multiple_of(16) {
        return Err(fail(format!("{w}x{h}: this format needs a width that is a multiple of 16")));
    }
    let mut image = im3::from_png(png, base).map_err(|error| fail(error.to_string()))?;
    image.name = name.to_string();
    image.raw_name = None;
    image.index = None;
    Ok(image)
}

/// The texture file (an IM3) with new pictures added at its end, each in the format of its first
/// picture with pixels and named by the PNG's name, under new_file's rules (powers of two: a map's
/// textures wrap). Its other entries (#texanime scripts) stay.
pub fn with_new_pictures(pngs: &[(String, Vec<u8>)], original: &[u8]) -> Result<Vec<u8>> {
    let mut images = im3::read(original)?;
    let Some(like) = images.iter().find(|image| !image.no_image) else {
        bail!("the texture file has no picture with pixels to take a format from");
    };
    let base = picture_base(like);
    for (name, png) in pngs {
        let picture = new_picture(name, png, &base, true, &images)?;
        images.push(picture);
    }
    im3::write(&images, Some(original))
}

/// Whether two PNGs hold the same pixels (size and RGBA), however they are encoded.
pub fn same_pixels(a: &[u8], b: &[u8]) -> Result<bool> {
    if a == b {
        return Ok(true);
    }
    let (a, b) = (im3::read_png(a)?, im3::read_png(b)?);
    Ok(a.width == b.width && a.height == b.height && im3::png_rgba(&a)? == im3::png_rgba(&b)?)
}

/// (picture name, PNG) for every PNG a mod gives for a new texture file: the file itself, or each
/// PNG in a folder, named by its file stem.
pub fn pngs_in(source: &Path) -> Result<Vec<(String, Vec<u8>)>> {
    let paths = if source.is_dir() { glob_suffix(source, SUFFIX)? } else { vec![source.to_path_buf()] };
    if paths.is_empty() {
        bail!("{} holds no PNG files", source.display());
    }
    paths
        .into_iter()
        .map(|path| {
            let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
            if !name.to_lowercase().ends_with(SUFFIX) {
                bail!("{name} is not a PNG file");
            }
            Ok((name[..name.len() - SUFFIX.len()].to_string(), std::fs::read(&path)?))
        })
        .collect()
}

/// A new texture file from a mod's PNG or folder of PNGs, with the picture of the template texture
/// file at entry `picture` (or its first) as the format.
pub fn build_new(source: &Path, template: &[u8], picture: Option<usize>) -> Result<Vec<u8>> {
    new_file(&pngs_in(source)?, &template_picture(template, picture)?, false)
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
        let folder = std::env::temp_dir().join(format!("dc2tk-assets-textures-{name}-{}", std::process::id()));
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

    /// A UI-like texture file: one swizzled 8-bit picture with a palette, as the game's own are.
    fn ui_template() -> Vec<u8> {
        let mut image = Image::new("logo", Format::T8, 64, 32);
        image.swizzled = true;
        image.palette = Some((0..256).map(|i| [i as u8, i as u8, 0, 0x80]).collect());
        image.indices = Some((0..64 * 32).map(|i| (i % 7) as u8).collect());
        im3::write(&[image], None).unwrap()
    }

    /// An RGBA PNG of w x h with a few colours, some see-through.
    fn rgba_png(w: usize, h: usize, seed: u8) -> Vec<u8> {
        let colours = [[255, 0, 0, 255], [0, 255, seed, 255], [0, 0, 255, 0], [seed, 200, 100, 255]];
        let data: Vec<u8> = (0..w * h).flat_map(|i| colours[(i / 3 + i / w) % 4]).collect();
        im3::write_png(w, h, PngMode::Rgba, &data, &[], &[]).unwrap()
    }

    #[test]
    fn a_new_texture_file_holds_the_pngs_as_pictures_of_their_own_names_and_sizes() {
        let template = ui_template();
        let pngs = vec![
            ("calc_bg".to_string(), rgba_png(256, 128, 7)),
            ("calc_key".to_string(), rgba_png(32, 32, 9)),
            ("wide".to_string(), rgba_png(48, 20, 1)),
        ];
        let built = new_file(&pngs, &template_picture(&template, None).unwrap(), false).unwrap();
        assert_eq!(&built[..4], b"IM3\0");
        let images = im3::read(&built).unwrap();
        let shapes: Vec<(&str, usize, usize, Format, bool)> = images
            .iter()
            .map(|image| (image.name.as_str(), image.width, image.height, image.format, image.swizzled))
            .collect();
        assert_eq!(
            shapes,
            [
                ("calc_bg", 256, 128, Format::T8, true),
                ("calc_key", 32, 32, Format::T8, true),
                ("wide", 48, 20, Format::T8, true)
            ]
        );
        // exported again, each picture is its PNG's colours, alpha included
        let exported = export(&built).unwrap();
        for ((name, png), (file, again)) in pngs.iter().zip(&exported) {
            assert_eq!(*file, format!("{name}.png"));
            let (a, b) = (im3::read_png(png).unwrap(), im3::read_png(again).unwrap());
            let rgba = |p: &im3::Png| -> Vec<u8> {
                if p.paletted {
                    p.data.iter().flat_map(|&i| p.palette[i as usize]).collect()
                } else {
                    p.data.clone()
                }
            };
            assert_eq!((a.width, a.height), (b.width, b.height));
            assert_eq!(rgba(&a), rgba(&b), "{name}");
        }
        // and the exported PNGs build the same file again
        let again: Vec<(String, Vec<u8>)> =
            exported.iter().map(|(file, png)| (file.trim_end_matches(".png").to_string(), png.clone())).collect();
        assert_eq!(new_file(&again, &template_picture(&template, None).unwrap(), false).unwrap(), built);
    }

    #[test]
    fn a_new_texture_file_refuses_what_the_game_cannot_hold() {
        let like = template_picture(&ui_template(), None).unwrap();
        let fails = |pngs: Vec<(&str, Vec<u8>)>, power_of_two: bool, words: &str| {
            let pngs: Vec<(String, Vec<u8>)> = pngs.into_iter().map(|(name, png)| (name.to_string(), png)).collect();
            let error = new_file(&pngs, &like, power_of_two).unwrap_err().to_string();
            assert!(error.contains(words), "{error}");
        };
        fails(vec![("odd", rgba_png(40, 16, 0))], false, "multiple of 16");
        fails(vec![("big", rgba_png(2048, 16, 0))], false, "1 to 1024");
        fails(vec![("npot", rgba_png(48, 16, 0))], true, "powers of two");
        fails(vec![("a_name_that_is_far_too_long_for_it", rgba_png(16, 16, 0))], false, "31");
        fails(vec![("same", rgba_png(16, 16, 0)), ("SAME", rgba_png(16, 16, 1))], false, "two pictures");
        fails(vec![], false, "no pictures");
    }

    #[test]
    fn new_pictures_go_at_the_end_in_the_first_pictures_format() {
        let data = sample_texture_file();
        let red = im3::write_png(16, 16, PngMode::Rgb, &[255, 0, 0].repeat(256), &[], &[]).unwrap();
        let grown = with_new_pictures(&[("wood".into(), red.clone())], &data).unwrap();
        let images = im3::read(&grown).unwrap();
        let names: Vec<&str> = images.iter().map(|image| image.name.as_str()).collect();
        assert_eq!(names, ["tex01", "tex02", "wood"]);
        assert_eq!(images[2].format, Format::T8);
        // the pictures it had are as they were
        assert_eq!(export(&grown).unwrap()[..2], export(&data).unwrap()[..]);
        let wood = &export(&grown).unwrap()[2];
        assert!(same_pixels(&wood.1, &red).unwrap());
        // a name taken, or a side that is not a power of two
        assert!(with_new_pictures(&[("TEX01".into(), red.clone())], &data).is_err());
        let odd = im3::write_png(12, 16, PngMode::Rgb, &[0; 12 * 16 * 3], &[], &[]).unwrap();
        assert!(with_new_pictures(&[("odd".into(), odd)], &data).is_err());
    }

    #[test]
    fn same_pixels_ignores_the_encoding() {
        let rgb = im3::write_png(2, 1, PngMode::Rgb, &[1, 2, 3, 4, 5, 6], &[], &[]).unwrap();
        let rgba = im3::write_png(2, 1, PngMode::Rgba, &[1, 2, 3, 255, 4, 5, 6, 255], &[], &[]).unwrap();
        let paletted = im3::write_png(2, 1, PngMode::P, &[1, 0], &[[4, 5, 6, 255], [1, 2, 3, 255]], &[]).unwrap();
        assert!(same_pixels(&rgb, &rgba).unwrap());
        assert!(same_pixels(&rgb, &paletted).unwrap());
        let other = im3::write_png(2, 1, PngMode::Rgb, &[1, 2, 3, 4, 5, 7], &[], &[]).unwrap();
        assert!(!same_pixels(&rgb, &other).unwrap());
    }

    #[test]
    fn every_picture_of_a_name_is_searched() {
        let file = |colour: u8| {
            let mut image = Image::new("shared", Format::Ct32, 8, 8);
            image.pixels = Some([colour, 0, 0, 0x80].repeat(64));
            im3::write(&[image], None).unwrap()
        };
        let search =
            crate::to_gltf::TextureSearch::new(&[("a.img".into(), file(10)), ("b.img".into(), file(200))]).unwrap();
        let pngs = search.pngs_named("shared").unwrap();
        assert_eq!(pngs.len(), 2, "both maps' pictures of the name");
        assert!(!same_pixels(&pngs[0], &pngs[1]).unwrap());
        // the first is the one a lookup by name finds
        assert_eq!(search.png("shared").unwrap().unwrap(), pngs[0]);
        assert!(search.pngs_named("missing").unwrap().is_empty());
    }
}

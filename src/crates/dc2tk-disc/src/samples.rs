//! Small discs, file indexes, packs and a script built in memory, for tests: the port of the
//! Python tests' build.py. Built for this crate's tests, and for other crates' with the
//! test-support feature (which they enable in their dev-dependencies).

use crate::{archive, iso, text};

const SECTOR: usize = iso::SECTOR as usize;
pub const ROOT_SECTOR: usize = 18;

fn both32(value: u32) -> Vec<u8> {
    [value.to_le_bytes(), value.to_be_bytes()].concat()
}

fn both16(value: u16) -> Vec<u8> {
    [value.to_le_bytes(), value.to_be_bytes()].concat()
}

fn sectors(size: usize) -> usize {
    size.div_ceil(SECTOR).max(1)
}

fn pad(data: &[u8], alignment: usize) -> Vec<u8> {
    let mut out = data.to_vec();
    out.resize(data.len().div_ceil(alignment) * alignment, 0);
    out
}

fn directory_record(name: &[u8], extent: usize, size: usize, is_dir: bool) -> Vec<u8> {
    let length = 33 + name.len() + usize::from(name.len().is_multiple_of(2));
    let mut record = vec![0u8; length];
    record[0] = length as u8;
    record[2..10].copy_from_slice(&both32(extent as u32));
    record[10..18].copy_from_slice(&both32(size as u32));
    record[25] = if is_dir { 2 } else { 0 };
    record[28..32].copy_from_slice(&both16(1));
    record[32] = name.len() as u8;
    record[33..33 + name.len()].copy_from_slice(name);
    record
}

/// An ISO 9660 image with the files in its root folder.
pub fn iso_image(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut layout = Vec::new();
    let mut sector = ROOT_SECTOR + 1;
    for (name, data) in files {
        layout.push((*name, sector, *data));
        sector += sectors(data.len());
    }
    let mut root = directory_record(b"\x00", ROOT_SECTOR, SECTOR, true);
    root.extend(directory_record(b"\x01", ROOT_SECTOR, SECTOR, true));
    for (name, start, data) in &layout {
        root.extend(directory_record(format!("{name};1").as_bytes(), *start, data.len(), false));
    }
    let mut image = vec![0u8; sector * SECTOR];
    let pvd = &mut image[16 * SECTOR..17 * SECTOR];
    pvd[0] = 1;
    pvd[1..6].copy_from_slice(b"CD001");
    pvd[6] = 1;
    pvd[156..190].copy_from_slice(&directory_record(b"\x00", ROOT_SECTOR, SECTOR, true));
    image[17 * SECTOR..17 * SECTOR + 7].copy_from_slice(b"\xffCD001\x01");
    image[ROOT_SECTOR * SECTOR..ROOT_SECTOR * SECTOR + root.len()].copy_from_slice(&root);
    for (_, start, data) in &layout {
        image[start * SECTOR..start * SECTOR + data.len()].copy_from_slice(data);
    }
    image
}

fn cp932(text: &str) -> Vec<u8> {
    text::encode(text).expect("the sample names are cp932")
}

/// DATA.HD4 and DATA.DAT holding the files, by their paths on the disc ("\map\t01.stb").
pub fn archive_files(files: &[(&str, &[u8])]) -> (Vec<u8>, Vec<u8>) {
    let table_size = 12 * (files.len() + 1);
    let mut names = Vec::new();
    let mut name_offsets = Vec::new();
    for (path, _) in files {
        name_offsets.push(table_size + names.len());
        names.extend(cp932(path));
        names.push(0);
    }
    let mut dat = Vec::new();
    let mut table = Vec::new();
    for ((_, data), name_offset) in files.iter().zip(name_offsets) {
        for value in [name_offset, data.len(), dat.len() / SECTOR] {
            table.extend((value as i32).to_le_bytes());
        }
        dat.extend(pad(data, SECTOR));
    }
    for value in [table_size + names.len(), 0, 0] {
        table.extend((value as i32).to_le_bytes());
    }
    table.extend(names);
    table.push(0);
    (table, dat)
}

/// A pack of the members, ended by an empty slot (archive::write_pack).
pub fn pack(members: &[(&str, &[u8])]) -> Vec<u8> {
    archive::write_pack(members).expect("the sample members' names fit a slot")
}

fn ints(values: &[i32]) -> Vec<u8> {
    values.iter().flat_map(|value| value.to_le_bytes()).collect()
}

fn instruction(op: i32, arg1: i32, arg2: i32) -> Vec<u8> {
    ints(&[op, arg1, arg2])
}

fn function_record(record: i32, code: i32, local: i32, arg: i32, arg_types: &[i32]) -> Vec<u8> {
    let mut types = arg_types.to_vec();
    types.resize(10, 0);
    [ints(&[record - code + 0x38, 0, local, arg]), ints(&types)].concat()
}

/// A small script: a main program that prints, calls a helper and loops, and the helper.
pub fn sample_script() -> Vec<u8> {
    let code = 0x50;
    let main = code;
    let main_code = main + 0x38;
    let hello = main_code + 12 * 12;
    let helper = hello + 16;
    let mut header = b"SB2\x00".to_vec();
    header.extend(ints(&[main, code, 0x40, 2, 0, 1]));
    header.extend([0u8; 0x24]);
    let programs = ints(&[100, main, 200, helper]);
    let main_body = [
        instruction(3, 3, hello - code),
        instruction(20, 1, 0),
        instruction(3, 1, 12),
        instruction(3, 1, 5),
        instruction(19, 0, helper - code),
        instruction(2, 0, 1),
        instruction(21, 3, 0),
        instruction(1, 0, 1),
        instruction(17, main_code + 10 * 12 - code, 0),
        instruction(23, 0, 0),
        instruction(27, 0, 0),
        instruction(0, 0, 0),
    ]
    .concat();
    let strings = b"hello\x00stale\x00item".to_vec();
    let helper_body =
        [instruction(22, 0, 0), instruction(3, 1, 7), instruction(15, 0, 0), instruction(0, 0, 0)].concat();
    [
        header,
        programs,
        function_record(main, code, 1, 0, &[]),
        main_body,
        strings,
        function_record(helper, code, 1, 1, &[1]),
        helper_body,
    ]
    .concat()
}

/// The disc the extract and decompile tests use: a script, a pack holding a script, and a text file.
pub fn sample_disc() -> Vec<u8> {
    let script = sample_script();
    let chara = pack(&[("info.cfg", b"MODEL c01;"), ("c01.stb", &script)]);
    let (index, dat) =
        archive_files(&[("\\map\\t01.stb", &script), ("\\chara\\c01.chr", &chara), ("\\readme.txt", b"hello")]);
    iso_image(&[("DATA.HD4", &index), ("DATA.DAT", &dat)])
}

#[cfg(test)]
mod tests {
    use super::*;

    // tests/test_iso.py

    fn small_disc() -> Vec<u8> {
        iso_image(&[("DATA.HD4", b"index"), ("DATA.DAT", &[b'x'; 5000])])
    }

    #[test]
    fn finds_files_case_insensitively() {
        let image = small_disc();
        let record = iso::find(&image, "/data.hd4").unwrap().unwrap();
        assert_eq!(iso::read_file(&image, &record), b"index");
    }

    #[test]
    fn reads_multi_sector_file() {
        let image = small_disc();
        let record = iso::find(&image, "DATA.DAT").unwrap().unwrap();
        assert_eq!(iso::read_file(&image, &record), &[b'x'; 5000]);
    }

    #[test]
    fn missing_file_is_none() {
        assert_eq!(iso::find(&small_disc(), "SYSTEM.CNF").unwrap(), None);
    }

    #[test]
    fn rejects_non_iso() {
        assert!(iso::root_record(&[0u8; 40000]).is_err());
    }

    // tests/test_archive.py

    #[test]
    fn parses_index_with_backslash_paths() {
        let (index, _) = archive_files(&[("\\map\\t01.stb", b"abc"), ("\\menu\\cfg7\\comdat.cfg", &[b'x'; 3000])]);
        let entries = archive::parse_index(&index).unwrap();
        let fields: Vec<(&str, i32, i32)> =
            entries.iter().map(|entry| (entry.path.as_str(), entry.size, entry.sector)).collect();
        assert_eq!(fields, vec![("map/t01.stb", 3, 0), ("menu/cfg7/comdat.cfg", 3000, 1)]);
    }

    #[test]
    fn parses_pack() {
        let script = [b"SB2\x00".as_slice(), &[1u8; 40]].concat();
        let data = pack(&[("info.cfg", b"MODEL a;"), ("c01.stb", &script)]);
        let members = archive::parse_pack(&data).unwrap();
        let names: Vec<&str> = members.iter().map(|member| member.name.as_str()).collect();
        assert_eq!(names, ["info.cfg", "c01.stb"]);
        assert_eq!(members[0].data(&data), b"MODEL a;");
    }

    #[test]
    fn pack_needs_terminator() {
        let data = pack(&[("a.bin", b"1234")]);
        let members = archive::parse_pack(&data).unwrap();
        assert_eq!(members.iter().map(|member| member.name.as_str()).collect::<Vec<_>>(), ["a.bin"]);
        assert!(archive::parse_pack(&data[..data.len() - archive::PACK_SLOT]).is_none());
        assert!(archive::parse_pack(&[data.as_slice(), &[0u8; 16]].concat()).is_none());
    }

    #[test]
    fn rejects_non_packs() {
        assert!(archive::parse_pack(b"").is_none());
        assert!(archive::parse_pack(&b"plain text file\n".repeat(20)).is_none());
        assert!(archive::parse_pack(&sample_script()).is_none());
        assert!(archive::parse_pack(&[0u8; 200]).is_none());
    }
}

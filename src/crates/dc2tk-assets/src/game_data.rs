//! The kinds of game data built on [`data_table`] (docs/game/data.md), by the name a `data:`
//! line gives them: where each reads and how it is read. Items and shops have their own modules
//! ([`crate::item_data`], [`crate::shop_data`]), as do the layouts and chests
//! ([`crate::dungeon_layout`], [`crate::chest_data`]), which rewrite whole rooms and groups.

use crate::data_table::Table;
use crate::{
    dungeon_data, dungeon_layout, exe_tables, fishing_data, georama_data, invention_data, monster_data, town_data,
    Error, Result,
};

/// A kind of game data: its name, the disc files it reads (by path in DATA.DAT, a pack's member as
/// `pack.d/member`) and how its table is read from them, in that order, and the boot executable.
#[derive(Debug)]
pub struct DataKind {
    pub name: &'static str,
    pub paths: fn() -> Vec<String>,
    pub read: fn(&[Vec<u8>], &[u8]) -> Result<Table>,
}

/// A kind kept in the executable alone reads no disc file.
fn no_files() -> Vec<String> {
    Vec::new()
}

pub const KINDS: [DataKind; 13] = [
    DataKind { name: "monsters", paths: no_files, read: |_, elf| monster_data::read(elf) },
    DataKind { name: "inventions", paths: invention_data::paths, read: invention_data::read },
    DataKind { name: "georama", paths: georama_data::paths, read: |files, _| georama_data::read(files) },
    DataKind { name: "medals", paths: dungeon_data::medal_paths, read: |files, _| dungeon_data::medals(files) },
    DataKind { name: "floors", paths: dungeon_data::floor_paths, read: |files, _| dungeon_data::floors(files) },
    DataKind { name: "parts", paths: no_files, read: |_, elf| dungeon_layout::parts_table(elf) },
    DataKind { name: "fishing", paths: fishing_data::paths, read: fishing_data::read },
    DataKind { name: "characters", paths: no_files, read: |_, elf| exe_tables::characters(elf) },
    DataKind { name: "requests", paths: town_data::request_paths, read: |files, _| town_data::requests(files) },
    DataKind {
        name: "townspeople",
        paths: town_data::townspeople_paths,
        read: |files, _| town_data::townspeople(files),
    },
    DataKind { name: "gifts", paths: no_files, read: |_, elf| exe_tables::gifts(elf) },
    DataKind { name: "spheda", paths: no_files, read: |_, elf| exe_tables::spheda(elf) },
    DataKind { name: "breeding", paths: no_files, read: |_, elf| exe_tables::breeding(elf) },
];

/// The kind of this name; an unknown name is an error listing the kinds.
pub fn kind(name: &str) -> Result<&'static DataKind> {
    KINDS.iter().find(|kind| kind.name == name).ok_or_else(|| {
        let names: Vec<&str> = KINDS.iter().map(|kind| kind.name).collect();
        Error::invalid(format!("no data table {name} (the tables built on data_table are {})", names.join(", ")))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kind_is_found_by_its_name_and_an_unknown_one_is_an_error() {
        assert_eq!(kind("medals").unwrap().name, "medals");
        assert_eq!((kind("medals").unwrap().paths)()[0], "menu/dngmap/dflr0.cfg");
        assert!((kind("monsters").unwrap().paths)().is_empty());
        let error = kind("medal").unwrap_err().to_string();
        assert!(error.starts_with("no data table medal (") && error.contains("townspeople"), "{error}");
    }
}

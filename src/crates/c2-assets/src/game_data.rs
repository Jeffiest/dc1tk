//! The kinds of game data built on [`data_table`] (docs/assets/data.md), by the name a `data:`
//! line gives them: where each reads and how it is read. Items, shops and monsters have their own
//! modules ([`crate::item_data`], [`crate::shop_data`], [`crate::monster_data`]).

use crate::data_table::Table;
use crate::{bail, dungeon_data, exe_tables, fishing_data, georama_data, invention_data, Result};

pub const KINDS: [&str; 6] = ["inventions", "georama", "medals", "floors", "fishing", "characters"];

/// The disc files a kind reads, by path in DATA.DAT (a pack's member as `pack.d/member`).
pub fn paths(kind: &str) -> Vec<String> {
    match kind {
        "inventions" => invention_data::paths(),
        "georama" => georama_data::paths(),
        "medals" => dungeon_data::medal_paths(),
        "floors" => dungeon_data::floor_paths(),
        "fishing" => fishing_data::paths(),
        _ => Vec::new(),
    }
}

/// The kind's table from its files (by [`paths`]) and the boot executable.
pub fn read(kind: &str, files: &[Vec<u8>], elf: &[u8]) -> Result<Table> {
    match kind {
        "inventions" => invention_data::read(files, elf),
        "georama" => georama_data::read(files),
        "medals" => dungeon_data::medals(files),
        "floors" => dungeon_data::floors(files),
        "fishing" => fishing_data::read(files, elf),
        "characters" => exe_tables::characters(elf),
        _ => bail!("no data table {kind}"),
    }
}

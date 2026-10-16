//! Memory budgets (docs/game/memory.md): the fixed memory the engine reads files into and builds
//! maps, models, textures and sound banks in, what a load of the disc's files uses of it, and the
//! check `build` runs over the loads a mod's files change.
//!
//! Some budgets are certain and so are their uses: a file read whole into the 3,200,000-byte read
//! buffer, a map's placements against its placement table. Others are certain in size but the use
//! is estimated: what building a model, a map or a character pack takes of a memory stack is
//! counted the way the engine allocates it (mgCMemory::Alloc in quadwords), for the parts that
//! dominate (models, collision, motions, pictures) and not for every small table. Where several
//! loads share one pool (a town's map, its villagers and its events), the budget is not one
//! number, and a load is compared with the largest of its kind on the disc instead.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};

use dc2tk_assets::bytes::{i32_at, u16_at, u32_at};
use dc2tk_assets::im3;
use dc2tk_assets::info_cfg::{self, Arg};
use dc2tk_assets::mds::{self as mdsfile, F_COLOUR, PRIM_STRIP};
use dc2tk_assets::models::{SKIN_BONES, SKIN_VERTICES};
use dc2tk_disc::{archive, text, DiscSource};

/// Bytes in a quadword: mgCMemory sizes and allocations count quadwords.
const QW: u64 = 16;

/// The read buffer every whole-file load goes through: `read_buffer`, 0x30D40 quadwords in town
/// (EditInit, editloop.cpp: MenuBuffer__2, read_buffer_end = read_buffer + 0x30D400) and 200000
/// quadwords in dungeons (memoryInit, dng_main.cpp:286 BuffReadData; EntryEventScript sets
/// read_buffer to it). A map's .map, .cfg, .mpk and .sky, a character pack, a sound bank are read
/// into it whole.
pub const READ_BUFFER: u64 = 3_200_000;
/// Where the loading screen's packets start inside the read buffer in town (EditInit:
/// stSetBuffer(read_buffer + 0x2F9B80, 0x1388 quadwords)) while the first map of a town loads.
pub const TOWN_LOADING_SCREEN: u64 = 0x2F_9B80;
/// The same in dungeons (InitDungeonMain, dng_main.cpp:291: BuffReadData + 0x2E630 quadwords).
pub const DUNGEON_LOADING_SCREEN: u64 = 0x2E_6300;
/// Placements a map may have: the placement table SetPlacePartsBuff allocates (map.cpp:224),
/// 0x140 by default (GetLoadMapInfo and LoadSubMap, mapjump.cpp) and 400 in dungeons
/// (LoadDungeonMapFile, dng_event.cpp). The add map's placements count too.
pub const TOWN_PLACEMENTS: usize = 0x140;
pub const DUNGEON_PLACEMENTS: usize = 400;
/// The script buffer a map's event script is read into: 20000 quadwords, ScriptBuffer__2 in town
/// (EditInit, 0x4E20) and BuffScriptData in dungeons (memoryInit, dng_main.cpp:273). The script
/// takes its size in quadwords, SetEventScript then 0x40 + 0x180 more (event.cpp:106), and
/// mgCMemory::Alloc refuses to reach the end, so a script may have this many bytes.
pub const SCRIPT_BUFFER: u64 = 20000;
pub const SCRIPT_FILE: u64 = (SCRIPT_BUFFER - 1 - 0x40 - 0x180) * QW;

/// The memory stacks Max's and Monica's packs build into: mem_table (charasetup.cpp:462, data
/// 0x351D50, quadwords), row 0 in dungeons and row 2 in town (GetCharaMemAllocPtr; SetupMainUnit
/// passes edit_mode 1 from EditInit). Stack 0 takes the model pack (mainchr/c0N_baseM.chr in
/// dungeons, chara/c0N_base.chr in town), 2 the sword, 3 the second weapon, 4 the hat or
/// accessory (charasetup.cpp:100-170).
pub const DUNGEON_PARTY_STACKS: [u64; 7] = [68500, 40000, 7000, 4650, 3000, 9900, 4500];
pub const TOWN_PARTY_STACKS: [u64; 7] = [75000, 40000, 7000, 4650, 3000, 9900, 0];

/// Largest estimated use on the retail disc of each estimated budget, measured by the corpus test
/// `corpus_retail_fits_the_budgets` (which fails when one of these no longer matches the disc).
pub const RETAIL_TOWN_MAP: u64 = 9_471_152;
pub const RETAIL_INTERIOR_MAP: u64 = 3_752_880;
pub const RETAIL_DUNGEON_MAP: u64 = 6_911_504;

mod chara;
mod check;
mod disc;
mod maps;
mod mds;
mod spu;
mod stack;
#[cfg(test)]
mod tests;
mod vram;

pub use self::chara::*;
pub use self::check::*;
pub use self::disc::*;
pub use self::maps::*;
use self::mds::*;
pub use self::spu::*;
pub use self::stack::*;
pub use self::vram::*;

/// A pack member by name, ignoring case as GetPackFile does.
fn member<'a>(pack: &'a [u8], name: &str) -> Option<&'a [u8]> {
    let lower = name.to_lowercase();
    archive::parse_pack_with_data(pack)?
        .into_iter()
        .find(|(member, _)| member.to_lowercase() == lower)
        .map(|(_, data)| data)
}

fn members(pack: &[u8]) -> Vec<(String, &[u8])> {
    archive::parse_pack_with_data(pack).unwrap_or_default()
}

fn round_up(value: u64, to: u64) -> u64 {
    value.div_ceil(to) * to
}

//! Inventions (docs/assets/data.md, "Inventions, ideas and scoops"): the recipes of `inv6.lst`
//! and the scoop table of the executable, as one [`data_table`] table (`data:inventions`).
//!
//! CInventDataManage::LoadAnalyzeInventFile (inventmn.cpp) reads `DATATABLESET n;` and then
//! `DATASET item, idea, idea, idea, [material, count] × m, unknown, scale, x, y, z;` into
//! INVENT_DATA_INFO (_INVENT_DATASET: m is (arguments - 9) / 2; a statement without a material
//! fills nothing). The menu reads the copy in `menu/<lang>/inv2_bg.pac` or `photo_bg1.pac`, event
//! checks `menu/inv6.lst`; the disc has 19 copies, all alike, and an edit goes into every one.
//! Recipes are named by their place in the file (`recipe`, from 0): two recipes may make one item.
//!
//! `scoop_table` (0x352DE0, SCOOP_DATA 0x14 × 53) gives each scoop (an idea from 1000) the menu
//! flag that makes it known and its slot in the scoop records; its text is `scoop.cfg`'s, text.

use serde_json::Value;

use crate::data_table::{self, Adding, Kind, Place, Record, Table, S16, S8, U8};
use crate::Result;

/// The menu folders by language that hold copies.
pub const LANGUAGES: usize = 6;
/// scoop_table and its records.
pub const SCOOP_TABLE: u32 = 0x352DE0;
pub const SCOOPS: usize = 53;
pub const SCOOP_RECORD: usize = 0x14;
/// The recipes the inventions menu holds: MenuInventInit gives InventTeigiStack 0x210 16-byte
/// units, _INVENT_DATATABLESET allocates 0x24 bytes a recipe, and the allocation reaching the end
/// fails (CheckItemTable's 0x400 units hold 454).
pub const RECIPES_MOST: usize = 234;
const RECIPES_WHY: &str = "the inventions menu's 0x2100-byte table, InventTeigiStack";
/// MakeItemNeeds holds four materials (HowMuchZairyouMakeItem).
pub const MATERIALS_MOST: usize = 4;

/// Every copy of inv6.lst, the one event checks read first; pack members as `pack.d/member`.
pub fn paths() -> Vec<String> {
    let mut out = vec!["menu/inv6.lst".to_string()];
    for language in 0..LANGUAGES {
        out.push(format!("menu/{language}/inv6.lst"));
        out.push(format!("menu/{language}/inv2_bg.pac.d/inv6.lst"));
        out.push(format!("menu/{language}/photo_bg1.pac.d/inv6.lst"));
    }
    out
}

pub fn read(files: &[Vec<u8>], elf: &[u8]) -> Result<Table> {
    let scripts = data_table::parse_all(&paths(), files)?;
    let copies: Vec<Vec<usize>> = scripts.iter().map(|script| data_table::tagged(script, "DATASET")).collect();
    let mut recipes = Vec::new();
    for (number, &first) in copies[0].iter().enumerate() {
        let count = scripts[0].statements[first].args().len();
        // the copies that have this recipe in the same shape
        let at: Vec<(usize, usize)> = copies
            .iter()
            .enumerate()
            .filter_map(|(file, list)| list.get(number).map(|&statement| (file, statement)))
            .filter(|&(file, statement)| scripts[file].statements[statement].args().len() == count)
            .collect();
        if count < 11 {
            continue;
        }
        let mut record = Record::new(&[("recipe", Value::from(number))]);
        record.arg("item", S16, &at, 0);
        record.field("ideas", S16, Place::Args { at: at.clone(), arg: 1, len: 3 });
        record.field(
            "materials",
            S16,
            Place::List {
                at: at.clone(),
                arg: 4,
                tail: 5,
                entry: vec![("item", S16), ("count", U8)],
                count: (1, MATERIALS_MOST),
            },
        );
        record.arg("unknown_10", S16, &at, count - 5);
        record.arg("scale", Kind::Float, &at, count - 4);
        record.field("position", Kind::Float, Place::Args { at: at.clone(), arg: count - 3, len: 3 });
        record.block = at.iter().map(|&(file, statement)| (file, statement, statement)).collect();
        recipes.push(record);
    }
    let mut table = Table::new("an invention table", scripts, elf.to_vec());
    table.section("recipes", &["recipe"], recipes);
    let counts = (0..table.scripts.len())
        .filter_map(|file| data_table::tagged(&table.scripts[file], "DATATABLESET").last().map(|&at| (file, at, 0)))
        .collect();
    table.adding(Adding { counts, most: RECIPES_MOST, why: RECIPES_WHY, place: Some("recipe"), ..Adding::default() });
    let start = data_table::elf_offset(elf, SCOOP_TABLE, SCOOPS * SCOOP_RECORD, "scoop table")?;
    let mut scoops = Vec::new();
    for number in 0..SCOOPS {
        let at = start + number * SCOOP_RECORD;
        let id = i16::from_le_bytes([elf[at], elf[at + 1]]);
        let mut record = Record::new(&[("scoop", Value::from(id))]);
        record.bytes("flag", S16, at + 2, 1).bytes("info", S8, at + 4, 1);
        scoops.push(record);
    }
    table.section("scoops", &["scoop"], scoops);
    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Every copy of inv6.lst and the scoop table of an extracted disc (C2S_CORPUS) read, write
    /// back unchanged, and their JSON changes nothing; an edit reaches all 19 copies.
    #[test]
    fn corpus_inventions_round_trip() {
        let Some(corpus) = std::env::var_os("C2S_CORPUS") else {
            return;
        };
        let root = std::path::Path::new(&corpus);
        let files: Vec<Vec<u8>> = paths().iter().map(|path| std::fs::read(root.join(path)).unwrap()).collect();
        let elf = std::fs::read(root.join(crate::monster_data::EXECUTABLE)).unwrap();
        let table = read(&files, &elf).unwrap();
        assert_eq!(table.write(), files);
        assert_eq!(table.check(), Vec::<String>::new());
        let recipes = table.sections[0].records.len();
        assert_eq!((recipes, table.sections[1].records.len()), (136, SCOOPS));
        let edit = json!({"recipes": [{"recipe": 0, "materials": [{"item": 210, "count": 1}], "scale": 2}]});
        let (edits, patches) = table.changes(&edit, "m").unwrap();
        assert!(patches.is_empty());
        let written = table.apply(&edits).unwrap();
        for (file, data) in written.iter().enumerate() {
            let again = read(&vec![data.clone(); paths().len()], &elf).unwrap().to_json();
            assert_eq!(again["recipes"][0]["materials"], json!([{"item": 210, "count": 1}]), "copy {file}");
            assert_eq!(again["recipes"][0]["scale"], 2);
        }
        // a new recipe in every copy, counted, and the most refused past
        let new = json!({"recipes": [{"recipe": recipes, "like": 0, "item": 300, "ideas": [1, 2, 3]}]});
        let (edits, _) = table.changes(&new, "m").unwrap();
        let written = table.apply(&edits).unwrap();
        let materials = table.to_json()["recipes"][0]["materials"].clone();
        for data in &written {
            let again = read(&vec![data.clone(); paths().len()], &elf).unwrap();
            let json = again.to_json();
            assert_eq!(json["recipes"][recipes]["item"], 300);
            assert_eq!(json["recipes"][recipes]["ideas"], json!([1, 2, 3]));
            assert_eq!(json["recipes"][recipes]["materials"], materials);
            assert!(String::from_utf8_lossy(data).contains(&format!("DATATABLESET {};", recipes + 1)));
        }
        let error = table.changes(&json!({"recipes": [{"recipe": recipes + 1, "like": 0}]}), "m").unwrap_err();
        assert!(error.to_string().contains(&format!("is the next one, {recipes}")), "{error}");
        let many: Vec<Value> = (recipes..=RECIPES_MOST).map(|recipe| json!({"recipe": recipe, "like": 0})).collect();
        let error = table.changes(&json!({ "recipes": many }), "m").unwrap_err();
        assert!(error.to_string().contains("InventTeigiStack"), "{error}");
        eprintln!("{recipes} recipes in {} copies and {SCOOPS} scoops round-trip", files.len());
    }
}

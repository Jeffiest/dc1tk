//! What dungeon floors' treasure chests hold (docs/assets/dungeons.md section 4, `data:chests`):
//! `dungeon/cfg_file/tbox_d0<n>.cfg`, n the dungeon from 1 (AutoSetTreasureBox loads
//! `tbox_d0%d.cfg` with the dungeon's number + 1, and draws the chests of the floor being entered).
//!
//! CreatTresuarBoxInfo (dng_event.cpp) runs the script into TRESURE_BOX_FLOOR_INFO:
//! `GROUP_START groups, items;` (read and dropped; the disc's give the counts), then per group
//! `GROUP id, count;` and `ITEM item, rank, count, ...;` lines (any number of entries a line, appended
//! to the group; the group uses its first `count`), `FLOOR_START floors, references;` (kept as
//! floor_start, otherwise the counts) and `FLOOR floor, count, group, ...;` per floor (its groups by
//! id). At most 64 groups of 96 items, floors 0-127 of 64 groups. PickupRandomItem picks one of the
//! floor's groups at random and one of its items until the item's rank is at least (or at most) the
//! one asked for; a floor naming a group the file lacks hangs the game (it prints and loops), so a
//! floor's groups must exist and have items.
//!
//! The JSON: `{"chests": [{"dungeon": 0, "groups": [{"group": 12, "items": [{"item": 1, "rank":
//! 15, "count": 1}, ...]}], "floors": [{"floor": 1, "groups": [12, 13, 44]}]}]}`, dungeon from 0 as
//! the other floor tables number them (0 is tbox_d01.cfg). A mod gives only the dungeons, groups and
//! floors it changes, each group's `items` and each floor's `groups` whole; a group or floor the
//! file lacks is added. Changed blocks are written again in the files' layout (five items a line),
//! with the start lines' counts kept true; the rest keep their bytes. Several mods merge; two giving
//! one group or floor different contents are refused.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use crate::cfg_text::{Arg, Script, Statement};
use crate::json::{self, member};
use crate::{bail, Context, Result};

pub const FOLDER: &str = "dungeon/cfg_file";
/// The chest scripts on the disc: tbox_d01.cfg to tbox_d08.cfg (d08, "back dungeon 2", has none).
pub const DUNGEONS: usize = 8;
pub const GROUPS_MOST: usize = 64;
pub const ITEMS_MOST: usize = 96;
pub const FLOORS: i32 = 128;
pub const FLOOR_GROUPS_MOST: usize = 64;
/// Items a line as the disc writes them.
const ITEMS_A_LINE: usize = 5;

pub fn paths() -> Vec<String> {
    (1..=DUNGEONS).map(|number| format!("{FOLDER}/tbox_d0{number}.cfg")).collect()
}

/// An item a group can give: (item, rank, count).
pub type Item = (i32, i32, i32);

#[derive(Clone, Debug, PartialEq)]
struct Group {
    id: i32,
    items: Vec<Item>,
    /// The GROUP statement and its ITEM statements.
    at: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq)]
struct Floor {
    floor: i32,
    groups: Vec<i32>,
    at: usize,
}

#[derive(Clone, Debug)]
struct File {
    script: Script,
    group_start: Option<usize>,
    floor_start: Option<usize>,
    groups: Vec<Group>,
    floors: Vec<Floor>,
    newline: Vec<u8>,
}

fn ints(statement: &Statement) -> Vec<i32> {
    statement.values().iter().map(Arg::int).collect()
}

impl File {
    fn parse(data: &[u8], what: &str) -> Result<File> {
        let script = Script::parse(data).context(what)?;
        let mut file = File {
            group_start: None,
            floor_start: None,
            groups: Vec::new(),
            floors: Vec::new(),
            newline: if data.windows(2).any(|pair| pair == b"\r\n") { b"\r\n".to_vec() } else { b"\n".to_vec() },
            script: Script { statements: Vec::new(), tail: Vec::new() },
        };
        for (at, statement) in script.statements.iter().enumerate() {
            let values = ints(statement);
            match statement.tag.as_str() {
                "GROUP_START" => file.group_start = Some(at),
                "FLOOR_START" => file.floor_start = Some(at),
                "GROUP" => {
                    let (id, count) = (values.first().copied().unwrap_or(0), values.get(1).copied().unwrap_or(0));
                    file.groups.push(Group { id, items: Vec::with_capacity(count.max(0) as usize), at: vec![at] });
                }
                "ITEM" => {
                    let Some(group) = file.groups.last_mut() else { bail!("{what}: ITEM before any GROUP") };
                    group
                        .items
                        .extend(values.as_chunks::<3>().0.iter().map(|&[item, rank, count]| (item, rank, count)));
                    group.at.push(at);
                }
                "FLOOR" => {
                    let count = values.get(1).copied().unwrap_or(0).max(0) as usize;
                    let groups: Vec<i32> = values.iter().skip(2).take(count).copied().collect();
                    file.floors.push(Floor { floor: values.first().copied().unwrap_or(0), groups, at });
                }
                tag => bail!("{what}: {tag} where a chest script has no place for it"),
            }
        }
        // a GROUP's count is how many of its items the engine uses; the disc's give them all
        for group in &file.groups {
            let count = ints(&script.statements[group.at[0]]).get(1).copied().unwrap_or(0);
            if count as usize != group.items.len() {
                bail!("{what}: GROUP {} counts {count} items but lists {}", group.id, group.items.len());
            }
        }
        file.script = script;
        Ok(file)
    }
}

/// One change: a group's items or a floor's groups, in a dungeon.
#[derive(Clone, Debug, PartialEq)]
pub enum Field {
    Group(i32, Vec<Item>),
    Floor(i32, Vec<i32>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub dungeon: usize,
    pub field: Field,
    pub module: String,
    pub what: String,
}

impl Change {
    fn key(&self) -> (usize, &'static str, i32) {
        match &self.field {
            Field::Group(id, _) => (self.dungeon, "group", *id),
            Field::Floor(floor, _) => (self.dungeon, "floor", *floor),
        }
    }
}

/// The chest scripts of [`paths`].
#[derive(Clone, Debug)]
pub struct Chests {
    files: Vec<File>,
}

fn whole(value: &Value, what: &str) -> Result<i32> {
    value
        .as_i64()
        .filter(|number| (i32::MIN as i64..=i32::MAX as i64).contains(number))
        .map(|number| number as i32)
        .ok_or_else(|| crate::Error::invalid(format!("{what} is a whole number")))
}

impl Chests {
    pub fn read(files: &[Vec<u8>]) -> Result<Chests> {
        if files.len() != DUNGEONS {
            bail!("{} chest scripts given, {DUNGEONS} wanted", files.len());
        }
        let paths = paths();
        let files = files.iter().zip(&paths).map(|(data, path)| File::parse(data, path)).collect::<Result<_>>()?;
        Ok(Chests { files })
    }

    pub fn write(&self) -> Vec<Vec<u8>> {
        self.files.iter().map(|file| file.script.write()).collect()
    }

    pub fn to_json(&self) -> Value {
        let dungeons: Vec<Value> = self
            .files
            .iter()
            .enumerate()
            .map(|(dungeon, file)| {
                let groups: Vec<Value> = file
                    .groups
                    .iter()
                    .map(|group| {
                        let items: Vec<Value> = group
                            .items
                            .iter()
                            .map(|(item, rank, count)| json!({"item": item, "rank": rank, "count": count}))
                            .collect();
                        json!({"group": group.id, "items": items})
                    })
                    .collect();
                let floors: Vec<Value> =
                    file.floors.iter().map(|floor| json!({"floor": floor.floor, "groups": floor.groups})).collect();
                json!({"dungeon": dungeon, "groups": groups, "floors": floors})
            })
            .collect();
        json!({ "chests": dungeons })
    }

    /// The changes a mod's JSON makes: each group and floor that differs from the files'.
    pub fn changes(&self, given: &Value, module: &str) -> Result<Vec<Change>> {
        let mut out = Vec::new();
        let Some(records) = given.get("chests") else {
            if given.as_object().is_some_and(|object| object.is_empty()) {
                return Ok(out);
            }
            bail!("a chest table is {{\"chests\": [...]}}");
        };
        let Some(records) = records.as_array() else {
            bail!("\"chests\" is a list of dungeons, not {}", json::shape(records));
        };
        for record in records {
            let record = json::object(record, "each of \"chests\"")?;
            let dungeon = whole(member(record, "dungeon"), "a dungeon's \"dungeon\" (0 is tbox_d01.cfg)")?;
            if !(0..DUNGEONS as i32).contains(&dungeon) {
                bail!("dungeon {dungeon}: the dungeons are 0 to {}", DUNGEONS - 1);
            }
            let dungeon = dungeon as usize;
            let file = &self.files[dungeon];
            for key in record.keys() {
                if !["dungeon", "groups", "floors"].contains(&key.as_str()) {
                    bail!("dungeon {dungeon}: no field {key} (a dungeon has groups and floors)");
                }
            }
            for group in json::list(record, "groups", format!("dungeon {dungeon}"))? {
                let group = json::object(group, format!("dungeon {dungeon}: each of \"groups\""))?;
                let id = whole(member(group, "group"), &format!("dungeon {dungeon}: a group's \"group\" (its id)"))?;
                let what = format!("dungeon {dungeon} group {id}");
                let Some(listed) = group.get("items").and_then(Value::as_array) else {
                    bail!("{what}: items is a list of {{item, rank, count}}")
                };
                let mut items = Vec::new();
                for entry in listed {
                    let entry = json::object(entry, format!("{what}: each of \"items\""))?;
                    let field = |key: &str| whole(member(entry, key), &format!("{what}: an item's {key}"));
                    items.push((field("item")?, field("rank")?, field("count")?));
                }
                if items.is_empty() || items.len() > ITEMS_MOST {
                    bail!("{what}: a group gives 1 to {ITEMS_MOST} items");
                }
                if file.groups.iter().find(|known| known.id == id).map(|known| &known.items) != Some(&items) {
                    let shown = format!("{what}: {} items", items.len());
                    out.push(Change { dungeon, field: Field::Group(id, items), module: module.into(), what: shown });
                }
            }
            for floor in json::list(record, "floors", format!("dungeon {dungeon}"))? {
                let floor = json::object(floor, format!("dungeon {dungeon}: each of \"floors\""))?;
                let number = whole(member(floor, "floor"), &format!("dungeon {dungeon}: a floor's \"floor\""))?;
                let what = format!("dungeon {dungeon} floor {number}");
                if !(0..FLOORS).contains(&number) {
                    bail!("{what}: the floors are 0 to {}", FLOORS - 1);
                }
                let Some(listed) = floor.get("groups").and_then(Value::as_array) else {
                    bail!("{what}: groups is a list of group ids")
                };
                let groups =
                    listed.iter().map(|id| whole(id, &format!("{what}: a group id"))).collect::<Result<Vec<_>>>()?;
                if groups.is_empty() || groups.len() > FLOOR_GROUPS_MOST {
                    bail!("{what}: a floor draws from 1 to {FLOOR_GROUPS_MOST} groups");
                }
                if file.floors.iter().find(|known| known.floor == number).map(|known| &known.groups) != Some(&groups) {
                    let shown =
                        format!("{what}: groups {}", groups.iter().map(i32::to_string).collect::<Vec<_>>().join(", "));
                    out.push(Change {
                        dungeon,
                        field: Field::Floor(number, groups),
                        module: module.into(),
                        what: shown,
                    });
                }
            }
        }
        Ok(out)
    }

    /// The files with the changes made, by [`paths`]. Two changes of one group or floor with
    /// different contents are refused, naming both mods; every floor's groups must exist.
    pub fn apply(&self, changes: &[Change]) -> Result<Vec<Vec<u8>>> {
        let mut wanted: BTreeMap<(usize, &'static str, i32), &Change> = BTreeMap::new();
        for change in changes {
            match wanted.get(&change.key()) {
                Some(known) if known.field != change.field => {
                    bail!(
                        "{} and {} give different contents ({}; {})",
                        known.module,
                        change.module,
                        known.what,
                        change.what
                    )
                }
                Some(_) => {}
                None => {
                    wanted.insert(change.key(), change);
                }
            }
        }
        let paths = paths();
        let mut out = Vec::new();
        for (dungeon, file) in self.files.iter().enumerate() {
            let mine: Vec<&Change> = wanted.values().copied().filter(|change| change.dungeon == dungeon).collect();
            if mine.is_empty() {
                out.push(file.script.write());
                continue;
            }
            let mut groups: Vec<(i32, Vec<Item>, bool)> =
                file.groups.iter().map(|group| (group.id, group.items.clone(), false)).collect();
            let mut floors: Vec<(i32, Vec<i32>, bool)> =
                file.floors.iter().map(|floor| (floor.floor, floor.groups.clone(), false)).collect();
            for change in &mine {
                match &change.field {
                    Field::Group(id, items) => match groups.iter_mut().find(|(known, _, _)| known == id) {
                        Some(group) => *group = (*id, items.clone(), true),
                        None => groups.push((*id, items.clone(), true)),
                    },
                    Field::Floor(number, ids) => match floors.iter_mut().find(|(known, _, _)| known == number) {
                        Some(floor) => *floor = (*number, ids.clone(), true),
                        None => floors.push((*number, ids.clone(), true)),
                    },
                }
            }
            let path = &paths[dungeon];
            if groups.len() > GROUPS_MOST {
                bail!("{path}: {} groups; the game holds {GROUPS_MOST}", groups.len());
            }
            for (number, ids, _) in &floors {
                if let Some(missing) =
                    ids.iter().find(|id| !groups.iter().any(|(known, items, _)| known == *id && !items.is_empty()))
                {
                    bail!("{path}: floor {number} draws from group {missing}, which the file does not have (the game would hang)");
                }
            }
            out.push(rewritten(file, &groups, &floors)?);
        }
        Ok(out)
    }
}

fn group_text(id: i32, items: &[Item], nl: &str) -> String {
    let mut text = format!("GROUP {id},{};", items.len());
    for line in items.chunks(ITEMS_A_LINE) {
        let values: Vec<String> = line.iter().map(|(item, rank, count)| format!("{item},{rank},{count}")).collect();
        text.push_str(&format!("{nl}ITEM {};", values.join(",")));
    }
    text
}

fn floor_text(floor: i32, groups: &[i32]) -> String {
    let ids: Vec<String> = groups.iter().map(i32::to_string).collect();
    format!("FLOOR {floor},{},{};", groups.len(), ids.join(","))
}

/// The file with its changed groups and floors written again in place, the new ones after the last
/// of their kind, and the start lines' counts made true.
fn rewritten(file: &File, groups: &[(i32, Vec<Item>, bool)], floors: &[(i32, Vec<i32>, bool)]) -> Result<Vec<u8>> {
    let nl_bytes = file.newline.clone();
    let nl = String::from_utf8_lossy(&nl_bytes).into_owned();
    let mut statements: Vec<Statement> = Vec::new();
    let push = |statements: &mut Vec<Statement>, text: String, before: Vec<u8>| -> Result<()> {
        let mut parsed = Script::parse(text.as_bytes())?.statements;
        if let Some(first) = parsed.first_mut() {
            first.before = before;
        }
        statements.extend(parsed);
        Ok(())
    };
    let item_count: usize = groups.iter().map(|(_, items, _)| items.len()).sum();
    let reference_count: usize = floors.iter().map(|(_, ids, _)| ids.len()).sum();
    // the ITEM lines of the groups written again
    let changed = |id: i32| groups.iter().any(|(known, _, changed)| *known == id && *changed);
    // where new groups follow: the last group's last line, or its GROUP line when it is written again
    let last_group = file
        .groups
        .last()
        .map(|group| if changed(group.id) { group.at[0] } else { *group.at.last().unwrap() })
        .or(file.group_start);
    let last_floor = file.floors.last().map(|floor| floor.at).or(file.floor_start);
    let dropped: Vec<usize> = file
        .groups
        .iter()
        .filter(|group| changed(group.id))
        .flat_map(|group| group.at.iter().skip(1).copied())
        .collect();
    for (at, statement) in file.script.statements.iter().enumerate() {
        if dropped.contains(&at) {
            continue;
        }
        if Some(at) == file.group_start {
            push(&mut statements, format!("GROUP_START {},{item_count};", groups.len()), statement.before.clone())?;
        } else if Some(at) == file.floor_start {
            push(
                &mut statements,
                format!("FLOOR_START {},{reference_count};", floors.len()),
                statement.before.clone(),
            )?;
        } else if let Some(group) = file.groups.iter().find(|group| group.at[0] == at) {
            match groups.iter().find(|(id, _, changed)| *id == group.id && *changed) {
                Some((id, items, _)) => push(&mut statements, group_text(*id, items, &nl), statement.before.clone())?,
                None => statements.push(statement.clone()),
            }
        } else if let Some(floor) = file.floors.iter().find(|floor| floor.at == at) {
            match floors.iter().find(|(number, _, changed)| *number == floor.floor && *changed) {
                Some((number, ids, _)) => push(&mut statements, floor_text(*number, ids), statement.before.clone())?,
                None => statements.push(statement.clone()),
            }
        } else {
            statements.push(statement.clone());
        }
        // new groups and floors after the last of their kind
        if Some(at) == last_group {
            for (id, items, _) in groups.iter().filter(|(id, _, _)| !file.groups.iter().any(|group| group.id == *id)) {
                push(&mut statements, group_text(*id, items, &nl), nl_bytes.clone())?;
            }
        }
        if Some(at) == last_floor {
            for (number, ids, _) in
                floors.iter().filter(|(number, _, _)| !file.floors.iter().any(|floor| floor.floor == *number))
            {
                push(&mut statements, floor_text(*number, ids), nl_bytes.clone())?;
            }
        }
    }
    if last_group.is_none() || last_floor.is_none() {
        bail!("a chest script without GROUP_START or FLOOR_START cannot take new groups or floors");
    }
    let tail = if file.script.tail.is_empty() { nl_bytes } else { file.script.tail.clone() };
    Ok(Script { statements, tail }.write())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<Vec<u8>> {
        let one = b"//chests\r\nGROUP_START 2,3;\r\nGROUP 12,1;\r\nITEM 1,15,1;\r\nGROUP 13,2;\r\nITEM 268,60,3,275,70,2;\r\n\r\n\r\nFLOOR_START 2,3;\r\nFLOOR 1,1,12;\r\nFLOOR 2,2,13,12;\r\n".to_vec();
        let mut files = vec![b"GROUP_START 0,0;\r\n\r\n\r\nFLOOR_START 0,103;\r\n".to_vec(); DUNGEONS];
        files[0] = one;
        files
    }

    #[test]
    fn chests_read_and_write_back() {
        let files = sample();
        let chests = Chests::read(&files).unwrap();
        assert_eq!(chests.write(), files);
        let json = chests.to_json();
        assert_eq!(json["chests"][0]["groups"][1]["items"][1], json!({"item": 275, "rank": 70, "count": 2}));
        assert_eq!(json["chests"][0]["floors"][1], json!({"floor": 2, "groups": [13, 12]}));
        assert!(chests.changes(&json, "m").unwrap().is_empty());
        assert_eq!(chests.apply(&[]).unwrap(), files);
    }

    #[test]
    fn groups_and_floors_change_and_are_added() {
        let chests = Chests::read(&sample()).unwrap();
        let edit = json!({"chests": [{"dungeon": 0,
            "groups": [{"group": 13, "items": [{"item": 5, "rank": 1, "count": 1}]},
                       {"group": 20, "items": [{"item": 7, "rank": 9, "count": 2}]}],
            "floors": [{"floor": 1, "groups": [20]}, {"floor": 3, "groups": [12, 20]}]}]});
        let changes = chests.changes(&edit, "m").unwrap();
        assert_eq!(changes.len(), 4);
        let written = chests.apply(&changes).unwrap();
        assert_eq!(
            String::from_utf8_lossy(&written[0]),
            "//chests\r\nGROUP_START 3,3;\r\nGROUP 12,1;\r\nITEM 1,15,1;\r\nGROUP 13,1;\r\nITEM 5,1,1;\r\nGROUP 20,1;\r\nITEM 7,9,2;\r\n\r\n\r\n\
             FLOOR_START 3,5;\r\nFLOOR 1,1,20;\r\nFLOOR 2,2,13,12;\r\nFLOOR 3,2,12,20;\r\n"
        );
        assert_eq!(written[1], sample()[1]);
        let again = Chests::read(&written).unwrap();
        assert!(again.changes(&edit, "m").unwrap().is_empty());
        // a floor drawing from a group no one gives would hang the game
        let missing = json!({"chests": [{"dungeon": 0, "floors": [{"floor": 1, "groups": [99]}]}]});
        assert!(chests.apply(&chests.changes(&missing, "m").unwrap()).unwrap_err().to_string().contains("hang"));
        // two mods giving one group different items
        let other = json!({"chests": [{"dungeon": 0, "groups": [{"group": 13, "items": [{"item": 6, "rank": 1, "count": 1}]}]}]});
        let mut both = chests.changes(&edit, "a").unwrap();
        both.extend(chests.changes(&other, "b").unwrap());
        assert!(chests.apply(&both).unwrap_err().to_string().contains("a and b"));
    }

    #[test]
    fn a_member_of_the_wrong_shape_is_refused_not_ignored() {
        let chests = Chests::read(&sample()).unwrap();
        let refused = |edit: Value| chests.changes(&edit, "m").unwrap_err().to_string();
        let error = refused(json!({"chests": [{"dungeon": 0, "groups": {"group": 13, "items": []}}]}));
        assert_eq!(error, "dungeon 0: \"groups\" is a list, not an object");
        assert!(refused(json!({"chests": [{"dungeon": 0, "floors": 3}]})).contains("\"floors\" is a list"));
        assert!(refused(json!({"chests": {"dungeon": 0}})).contains("a list of dungeons"));
        assert!(refused(json!({"chests": [[0]]})).contains("is an object"));
        let bad_item = json!({"chests": [{"dungeon": 0, "groups": [{"group": 1, "items": [5]}]}]});
        assert!(refused(bad_item).contains("an object"));
        // absent, they change nothing
        assert!(chests.changes(&json!({"chests": [{"dungeon": 0}]}), "m").unwrap().is_empty());
    }

    /// The chest scripts of an extracted disc (C2S_CORPUS) read, write back unchanged, and their
    /// JSON changes nothing.
    #[test]
    fn corpus_chests_round_trip() {
        let Some(corpus) = std::env::var_os("C2S_CORPUS") else {
            return;
        };
        let files: Vec<Vec<u8>> =
            paths().iter().map(|path| std::fs::read(std::path::Path::new(&corpus).join(path)).unwrap()).collect();
        let chests = Chests::read(&files).unwrap();
        assert_eq!(chests.write(), files);
        let json = chests.to_json();
        assert!(chests.changes(&json, "m").unwrap().is_empty());
        assert_eq!(json["chests"][0]["groups"][0]["group"], 12);
        assert_eq!(json["chests"].as_array().unwrap().len(), DUNGEONS);
        // every group and floor written again gives the same table
        let all: Vec<Change> = (0..DUNGEONS)
            .flat_map(|dungeon| {
                let file = &chests.files[dungeon];
                file.groups
                    .iter()
                    .map(move |group| Change {
                        dungeon,
                        field: Field::Group(group.id, group.items.clone()),
                        module: "m".into(),
                        what: String::new(),
                    })
                    .chain(file.floors.iter().map(move |floor| Change {
                        dungeon,
                        field: Field::Floor(floor.floor, floor.groups.clone()),
                        module: "m".into(),
                        what: String::new(),
                    }))
            })
            .collect();
        let again = Chests::read(&chests.apply(&all).unwrap()).unwrap();
        assert_eq!(again.to_json(), json);
    }
}

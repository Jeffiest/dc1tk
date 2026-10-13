//! Carried over from the Python tool's test_slots.py

use std::collections::BTreeMap;

use c2_script::messages::{NameKey, NameTable};
use c2_script::node;
use c2_script::node::Node;
use c2_script::sdk::Context;
use c2_script::slots::{self, Survey};
use indexmap::IndexMap;

fn load(slot: i64, pack: &str) -> Node {
    node!({"kind": "native", "name": "_LOAD_CHARA", "args": [{"kind": "int", "value": 4}, {"kind": "string", "text": pack},
                                                              {"kind": "string", "text": "info.cfg"}, {"kind": "int", "value": slot}]})
}

fn program(loads: Vec<Node>) -> Node {
    node!({"functions": [{"kind": "function", "body": loads}]})
}

#[test]
fn slots_a_folder_always_fills_with_one_character_are_named() {
    let mut found = Survey::new();
    slots::survey(
        &mut found,
        "map/m/m01/m01.stb",
        &program(vec![
            load(32, "event/e109/e109_starbre.chr"),
            load(37, "chara/c01_base.chr"),
            load(9, "chara/p01_20a.chr"),
            load(40, "a.chr"),
        ]),
    );
    slots::survey(
        &mut found,
        "map/m/m01/m01b.stb",
        &program(vec![load(32, "event/e110/e110_starbre.chr"), load(37, "chara/c01_base.chr"), load(40, "b.chr")]),
    );
    let pack_names: NameTable = [(NameKey::Text("chara/p01_20a.chr".into()), "Giles or Apia".to_string())].into_iter().collect();
    let names = slots::names(&found, &pack_names);
    // 32 holds Star Breaker from two events; 9 is a shared villager model and 40 two characters
    let folder: BTreeMap<i64, String> =
        [(32, "CHARA_M01_STARBRE".to_string()), (37, "CHARA_M01_MAX".to_string())].into_iter().collect();
    let expected: BTreeMap<String, BTreeMap<i64, String>> = [("map/m/m01".to_string(), folder)].into_iter().collect();
    assert_eq!(names, expected);
    assert!(slots::header_text(&names).contains("CHARA_M01_MAX = 37,"));
    let context = slots::in_folder(Some(Context::default()), "map/m/m01/m01.stb", &names).unwrap();
    let members: IndexMap<i64, String> =
        [(32, "CHARA_M01_STARBRE".to_string()), (37, "CHARA_M01_MAX".to_string())].into_iter().collect();
    assert_eq!(context.enums.len(), 1);
    assert_eq!(context.enums["chara_slot_folder"], members);
    assert!(slots::in_folder(Some(Context::default()), "map/m/m02/m02.stb", &names).unwrap().enums.is_empty());
}

//! Carried over from the Python tool's test_slots.py

use std::collections::BTreeMap;

use dc2tk_script::messages::{NameKey, NameTable};
use dc2tk_script::node;
use dc2tk_script::node::Node;
use dc2tk_script::sdk::Context;
use dc2tk_script::slots::{self, Survey};
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
    let pack_names: NameTable =
        [(NameKey::Text("chara/p01_20a.chr".into()), "Giles or Apia".to_string())].into_iter().collect();
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

#[test]
fn villager_tables_give_each_slot_the_villagers_a_chapter_puts_there() {
    let root = std::env::temp_dir().join(format!("dc2tk-villagers-{}", std::process::id()));
    std::fs::create_dir_all(root.join("map")).unwrap();
    std::fs::write(
        root.join("npc_place4.cfg"),
        "PROG_INFO  1 , 0 , 0 , 0 ,\"start\";\nPROG_INFO  2 , 1 , 0 , 1 ,\"one\";\nPROG_INFO  3 , 2 , 0 , 2 ,\"two\";\n\
         NPC  1 ;\n\tPROGRESS  1 , 0 ,\"x\";\n\t\tNOON_PLACE  1 ;\n\t\tNIGHT_PLACE  1 ;\n\tPROGRESS_END;\nNPC_END;\n\
         NPC  2 ;\n\tPROGRESS  3 , 0 ,\"x\";\n\t\tNOON_PLACE  1 ;\n\tPROGRESS_END;\nNPC_END;\n\
         NPC  3 ;\n\tPROGRESS  1 , 0 ,\"x\";\n\t\tNOON_PLACE  2 ;\n\t\tNIGHT_PLACE  2 ;\n\tPROGRESS_END;\nNPC_END;\n",
    )
    .unwrap();
    std::fs::write(root.join("place.cfg"), "NPC_PLACE  1 , 1 ;\nNPC_PLACE_END;\nNPC_PLACE  2 , 2 ;\nNPC_PLACE_END;\n")
        .unwrap();
    std::fs::write(
        root.join("npc.cfg"),
        "NPC_INFO  1 ,\"a\",\"p01_01a\",0;\nNPC_INFO  2 ,\"b\",\"p01_02a\",0;\nNPC_INFO  3 ,\"c\",\"p01_03a\",0;\n",
    )
    .unwrap();
    std::fs::write(
        root.join("map/map1.cfg"),
        "MAP_NAME \"\",\"\",\"\",0,0;\nMAP_NAME \"t01\",\"Town\",\"\", 3 , 2 , 1 , 1 ;\nMAP_NAME \"i01h01\",\"Shop\",\"\", 4 , 2 , 1 , 1 ;\n",
    )
    .unwrap();
    let tables = dc2tk_script::villagers::Tables::read(&root).unwrap();
    std::fs::remove_dir_all(&root).ok();
    let slots = tables.slots();
    // villager 1 stays in the town; villager 2 joins it from chapter 3, sorted after 1
    assert_eq!(slots["t01"][&8], [1].into_iter().collect());
    assert_eq!(slots["t01"][&9], [2].into_iter().collect());
    // an interior's villagers load from slot 24
    assert_eq!(slots["i01h01"][&24], [3].into_iter().collect());
    assert_eq!(tables.map_of("map/i/i01/h01/i01h01b_1.stb").as_deref(), Some("i01h01"));
    let mut found = Survey::new();
    slots::survey(&mut found, "map/i/i01/h01/i01h01_1.stb", &program(vec![load(25, "chara/e1_steve.chr")]));
    let npc_names: NameTable = [(NameKey::Int(3), "Cedric".to_string())].into_iter().collect();
    let names: IndexMap<String, NameTable> = [("npc_id".to_string(), npc_names)].into_iter().collect();
    let changing =
        slots::villagers(&found, &BTreeMap::new(), Some(&tables), &names, &["map/i/i01/h01/i01h01_1.stb".to_string()]);
    assert_eq!(changing.names["i01h01"][&24], "CHARA_I01_H01_CEDRIC");
    let context = slots::on_map(Some(Context::default()), Some("i01h01"), &changing).unwrap();
    assert_eq!(context.enums["chara_slot_villager"][&24], "CHARA_I01_H01_CEDRIC");
    assert!(slots::villager_header_text(&changing).contains("CHARA_I01_H01_CEDRIC = 24,"));
}

fn slot_param() -> Node {
    node!({"name": "chara", "kind": "chara_slot"})
}

#[test]
fn a_slot_a_load_reaches_takes_its_characters_name() {
    let mut found = Survey::new();
    slots::survey(
        &mut found,
        "map/s/s27/s27.stb",
        &program(vec![load(8, "event/e438/e438_paw.chr"), load(8, "event/e438/e438_shigu.chr")]),
    );
    let loaded = slots::loaded_names(&found, &BTreeMap::new(), &NameTable::new());
    assert_eq!(loaded["map/s/s27"][&(8, "event/e438/e438_paw.chr".to_string())], "CHARA_S27_PAW");
    let mut context = Context::default();
    context.slot_packs = loaded["map/s/s27"].iter().map(|(key, name)| (key.clone(), name.clone())).collect();
    let any = node!({"name": "x"});
    context.natives.insert(
        "load_chara".into(),
        node!({"name": "load_chara", "params": [any.clone(), any.clone(), any.clone(), slot_param()]}),
    );
    context.natives.insert("delete_chara".into(), node!({"name": "delete_chara", "params": [slot_param()]}));
    context.natives.insert("activate_chara".into(), node!({"name": "activate_chara", "params": [slot_param()]}));
    let native = |name: &str, args: Node| node!({"kind": "native", "name": name, "args": args});
    let int = |value: i64| node!({"kind": "int", "value": value});
    let load_paw = native(
        "load_chara",
        node!([{"kind": "int", "value": 5}, {"kind": "string", "text": "event/e438/e438_paw.chr"},
               {"kind": "string", "text": "info.cfg"}, {"kind": "int", "value": 8}]),
    );
    let call = |name: &str| node!({"kind": "expr", "expr": {"kind": "call", "name": name, "args": []}});
    let scene = node!({"kind": "function", "name": "scene", "params": [], "locals": [], "body": [
        load_paw,
        native("activate_chara", node!([int(8)])),
        call("keeps"),
        native("activate_chara", node!([int(8)])),
        call("loads"),
        native("activate_chara", node!([int(8)])),
    ]});
    let keeps = node!({"kind": "function", "name": "keeps", "params": [], "locals": [], "body": []});
    let loads = node!({"kind": "function", "name": "loads", "params": [], "locals": [],
                       "body": [native("delete_chara", node!([int(8)]))]});
    let shown = dc2tk_script::slotflow::named_slots(vec![scene, keeps, loads], &[], &context);
    let body = shown[0]["body"].as_array().unwrap();
    let slot_of = |at: usize, position: usize| body[at]["args"][position].clone();
    assert_eq!(slot_of(0, 3), node!({"kind": "var", "name": "CHARA_S27_PAW"}));
    assert_eq!(slot_of(1, 0), node!({"kind": "var", "name": "CHARA_S27_PAW"}));
    // a call that leaves slot 8 alone keeps it; one that deletes it does not
    assert_eq!(slot_of(3, 0), node!({"kind": "var", "name": "CHARA_S27_PAW"}));
    assert_eq!(slot_of(5, 0), int(8));
}

//! Carried over from the Python tool's test_assets.py

use std::collections::{BTreeSet, HashSet};

use c2_script::node;
use c2_script::sdk::Context;
use c2_script::{assets, notes};
use indexmap::IndexMap;

fn key(prefix: &str, text: &str) -> (String, String) {
    (prefix.to_string(), text.to_string())
}

#[test]
fn asset_names_come_from_glosses_and_stay_apart() {
    let (stand, stand_2, wave) = ("\u{7acb}\u{3061}", "\u{7acb}\u{3061}\u{ff12}", "\u{624b}\u{3092}\u{632f}\u{308b}");
    let glosses: IndexMap<String, String> = [
        (notes::string_key(stand), "stand".to_string()),
        (notes::string_key(stand_2), "stand".to_string()),
        (notes::string_key(wave), "wave (loop)".to_string()),
    ]
    .into_iter()
    .collect();
    let found: BTreeSet<(String, String)> =
        [key("MOTION", stand), key("MOTION", stand_2), key("MOTION", wave), key("ANIM", "\u{7b11}\u{9854}")].into();
    let names = assets::names(&found, &glosses, &HashSet::from(["MOTION_WAVE_LOOP".to_string()]));
    let mut sorted: Vec<((String, String), String)> = names.clone().into_iter().collect();
    sorted.sort();
    let mut expected = vec![
        (key("MOTION", stand), "MOTION_STAND".to_string()),
        (key("MOTION", stand_2), "MOTION_STAND_2".to_string()),
        (key("MOTION", wave), "MOTION_WAVE_LOOP_2".to_string()),
    ];
    expected.sort();
    assert_eq!(sorted, expected);
    assert!(assets::header_text(&names).contains("#define MOTION_STAND \"\u{7acb}\u{3061}\""));
}

#[test]
fn asset_strings_passed_to_known_parameters_are_replaced() {
    let mut found = Context::default();
    found.by_original.insert("_SET_MOTION".into(), node!({"name": "set_motion", "params": [{"name": "motion_name"}]}));
    found.asset_names.insert(key("MOTION", "\u{7acb}\u{3061}"), "MOTION_STAND".into());
    let nodes =
        node!([{"kind": "native", "name": "_SET_MOTION", "args": [{"kind": "string", "text": "\u{7acb}\u{3061}"}]}]);
    assert_eq!(assets::named(&nodes, &found)[0]["args"], node!([{"kind": "var", "name": "MOTION_STAND"}]));
}

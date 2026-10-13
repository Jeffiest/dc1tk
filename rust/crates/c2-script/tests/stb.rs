//! Carried over from the Python tool's test_stb.py

mod common;

use c2_script::stb::{self, Function, Region, Script};
use common::{sample_script, Random};

fn kinds(script: &Script) -> Vec<&'static str> {
    script
        .regions
        .iter()
        .map(|region| match region {
            Region::Header(_) => "header",
            Region::Programs { .. } => "programs",
            Region::Function(_) => "function",
            Region::Raw { .. } => "raw",
        })
        .collect()
}

fn functions(script: &Script) -> Vec<&Function> {
    script.functions().collect()
}

#[test]
fn sample_round_trips_and_is_fully_parsed() {
    let data = sample_script();
    let script = stb::read(&data);
    assert_eq!(stb::write(&script).unwrap(), data);
    assert_eq!(stb::coverage(&script), 1.0);
    assert_eq!(kinds(&script), ["header", "programs", "function", "function"]);
}

#[test]
fn sample_fields() {
    let script = stb::read(&sample_script());
    let header = script.header.as_ref().unwrap();
    assert_eq!(&header.magic, b"SB2\x00");
    assert_eq!(header.global_num, 1);
    let Region::Programs { entries, .. } = &script.regions[1] else { panic!("no programs") };
    assert_eq!(entries.iter().map(|entry| entry.no).collect::<Vec<_>>(), [100, 200]);
    let found = functions(&script);
    let (main, helper) = (found[0], found[1]);
    let names: Vec<&str> = main.instructions.iter().map(|item| item.name()).collect();
    assert_eq!(names[names.len() - 3..], ["WAIT", "END", "FUNC_END"]);
    assert_eq!(main.strings, ["hello"]);
    assert_eq!(main.padding, b"stale\x00item");
    assert_eq!(helper.arg, 1);
    assert_eq!(helper.arg_types[..2], [1, 0]);
    assert_eq!(helper.instructions[0].name(), "NOP");
}

#[test]
fn edit_changes_output() {
    let mut script = stb::read(&sample_script());
    let Region::Programs { entries, .. } = &mut script.regions[1] else { panic!("no programs") };
    entries[0].no = 101;
    assert_ne!(stb::write(&script).unwrap(), sample_script());
}

#[test]
fn header_padding_survives() {
    let mut data = sample_script();
    data[0x1C..0x40].fill(0xcc);
    data[0..4].copy_from_slice(b"STB\x00");
    let script = stb::read(&data);
    assert_eq!(stb::write(&script).unwrap(), data);
    assert_eq!(stb::coverage(&script), 1.0);
}

#[test]
fn trailing_bytes_join_last_function() {
    let mut data = sample_script();
    data.extend([0u8; 12]);
    let script = stb::read(&data);
    assert_eq!(stb::write(&script).unwrap(), data);
    assert_eq!(stb::coverage(&script), 1.0);
    assert_eq!(functions(&script).last().unwrap().padding, [0u8; 12]);
}

#[test]
fn unknown_opcode_falls_back_to_raw() {
    let mut data = sample_script();
    let helper = functions(&stb::read(&data))[1].offset;
    data[helper + stb::FUNCDATA_SIZE] = 99;
    let script = stb::read(&data);
    assert_eq!(stb::write(&script).unwrap(), data);
    assert_eq!(*kinds(&script).last().unwrap(), "raw");
    assert!(stb::coverage(&script) < 1.0);
}

#[test]
fn unknown_bytes_survive() {
    let data = Random::new(7).bytes(3000);
    assert_eq!(stb::write(&stb::read(&data)).unwrap(), data);
}

#[test]
fn short_and_empty_files_survive() {
    let mut samples: Vec<Vec<u8>> = vec![b"".to_vec(), b"SB2".to_vec()];
    for count in [30, 80] {
        let mut data = b"SB2\x00".to_vec();
        data.extend(vec![0xffu8; count]);
        samples.push(data);
    }
    for data in samples {
        assert_eq!(stb::write(&stb::read(&data)).unwrap(), data);
    }
}

#[test]
fn a_pool_block_holds_the_file_strings_so_far() {
    // the first function's strings, rounded up to 16 with zeros
    let (prefix, padding, pool) = stb::pool_block(b"", &["ab".to_string()]).unwrap();
    let mut expected = b"ab\x00".to_vec();
    expected.extend([0u8; 13]);
    assert!(prefix.is_empty());
    assert_eq!(padding, [0u8; 13]);
    assert_eq!(pool, expected);
    // the next one writes the pool, then its own strings
    let (prefix, padding, pool) = stb::pool_block(&pool, &["cd".to_string()]).unwrap();
    let mut wanted = vec!["ab".to_string()];
    wanted.extend(vec![String::new(); 13]);
    assert_eq!(prefix, wanted);
    assert_eq!(padding, [0u8; 13]);
    assert_eq!(pool.len(), 32);
    // a function with no strings writes the whole pool, which the reader takes as padding
    let (prefix, padding, after) = stb::pool_block(&pool, &[]).unwrap();
    assert!(prefix.is_empty());
    assert_eq!(padding, pool);
    assert_eq!(after, pool);
}

#[test]
fn the_code_section_ends_on_16() {
    assert_eq!(stb::tail_padding(0x40 + 0x38, 0x40), 8);
    assert_eq!(stb::tail_padding(0x50, 0x40), 0);
}

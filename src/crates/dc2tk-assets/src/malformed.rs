//! Malformed input: a small valid file of each binary format the tools read from a mod or a disc,
//! cut short at every length and with single bytes changed at every place, read (and what reads it
//! next, run) without a panic: every case is an Ok or an Err. A stand-in for a fuzzer, cheap enough
//! to run with every test.

use std::panic::{catch_unwind, AssertUnwindSafe};

use serde_json::Value;

use crate::folder::Folder;
use crate::mds::{self, Material, Mds, Mdt, Node, Prim, PRIM_TRIANGLES};
use crate::{font, from_gltf, im3, map_import, mot, music, sequence, soundbank, soundfont, to_gltf};

/// The cases of a file: every prefix, and each byte set to 0, to 0xFF, its top bit flipped and
/// one more.
fn cases(data: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut out: Vec<(String, Vec<u8>)> =
        (0..data.len()).map(|n| (format!("the first {n} bytes"), data[..n].to_vec())).collect();
    for at in 0..data.len() {
        for value in [0, 0xFF, data[at] ^ 0x80, data[at].wrapping_add(1)] {
            if value != data[at] {
                let mut changed = data.to_vec();
                changed[at] = value;
                out.push((format!("byte {at} set to 0x{value:02x}"), changed));
            }
        }
    }
    out
}

/// Runs `read` over every case of `data` and fails naming the cases that panicked. With
/// MALFORMED_TRACE set, each case is named before it runs: the last named is the one that aborts
/// the test (an allocation too big to make is an abort, not a panic).
fn sweep(name: &str, data: &[u8], read: impl Fn(&[u8])) {
    assert!(catch_unwind(AssertUnwindSafe(|| read(data))).is_ok(), "{name}: the valid file panics");
    let trace = std::env::var_os("MALFORMED_TRACE").is_some();
    let panicked: Vec<String> = cases(data)
        .into_iter()
        .filter(|(what, case)| {
            if trace {
                eprintln!("{name}: {what}");
            }
            catch_unwind(AssertUnwindSafe(|| read(case))).is_err()
        })
        .map(|(what, _)| what)
        .collect();
    assert!(
        panicked.is_empty(),
        "{name}: {} cases panic, the first: {:?}",
        panicked.len(),
        &panicked[..panicked.len().min(8)]
    );
}

fn model() -> Mds {
    let mut model = Mds::new();
    model.nodes.push(Node::new("root", -1));
    let mesh = Mdt {
        vertices: vec![[0.0, 0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0], [1.0, 1.0, 0.0, 1.0]],
        normals: vec![[0.0, 0.0, 1.0, 1.0]],
        uvs: vec![[0.0, 0.0, 1.0, 0.0]; 3],
        materials: vec![Material::new("tex01")],
        prims: vec![Prim {
            kind: PRIM_TRIANGLES,
            material: 0,
            verts: vec![vec![0, 0, 0], vec![1, 0, 1], vec![2, 0, 2]],
        }],
        ..Mdt::default()
    };
    model.nodes.push(Node { mdt: Some(mesh), ..Node::new("body", 0) });
    model
}

#[test]
fn malformed_models_are_errors_not_panics() {
    sweep("MDS", &mds::build(&model()).unwrap(), |data| {
        if let Ok(model) = mds::parse(data) {
            let _ = mds::build(&model);
            let _ = model.world_matrices();
        }
    });
}

#[test]
fn malformed_key_lists_are_errors_not_panics() {
    let lists = [
        mot::KeyList::new(3, 1, 0, vec![(0, [1.0, 0.0, 0.0, 0.0]), (10, [0.5, 0.5, 0.5, 0.5])]),
        mot::KeyList::new(4, 2, 2, vec![(0, [1.0, 2.0, 3.0, 0.0])]),
    ];
    sweep("key list", &mot::build(&lists).unwrap(), |data| {
        if let Ok(lists) = mot::parse(data) {
            let _ = mot::build(&lists);
        }
        let _ = mot::parse_bbp(data);
    });
}

#[test]
fn malformed_texture_archives_are_errors_not_panics() {
    let mut eight = im3::Image::new("tex01", im3::Format::T8, 16, 4);
    eight.palette = Some((0..256).map(|i| [i as u8, 255 - i as u8, 0, 0x80]).collect());
    eight.indices = Some((0..16 * 4).map(|i| (i % 251) as u8).collect());
    let mut four = im3::Image::new("tex02", im3::Format::T4, 4, 4);
    four.palette = Some((0..16).map(|i| [i * 16, 0, 0, 0x40]).collect());
    four.indices = Some((0..16).map(|i| i % 16).collect());
    let mut rgb = im3::Image::new("tex03", im3::Format::Ct16, 2, 2);
    rgb.pixels = Some([8, 16, 24, 0x80].repeat(4));
    let data = im3::write(&[eight, four, rgb], None).unwrap();
    sweep("IM3", &data, |data| {
        if let Ok(images) = im3::read(data) {
            for image in &images {
                if let Ok(png) = im3::to_png(image, true) {
                    let _ = im3::from_png(&png, image);
                }
            }
            let _ = im3::write(&images, Some(data));
        }
    });
    let png = im3::to_png(&im3::read(&data).unwrap()[0], true).unwrap();
    sweep("PNG", &png, |data| {
        if let Ok(png) = im3::read_png(data) {
            let _ = im3::png_rgba(&png);
        }
    });
}

/// A font page of 16 colours, 512 by 20 pixels (one row of glyphs).
fn font_page() -> Vec<u8> {
    let mut out = b"TIM2\x04\x00\x01\x00".to_vec();
    out.extend([0; 8]);
    let image = 512 * 20 / 2;
    out.extend(((0x30 + image + 64) as u32).to_le_bytes());
    out.extend(64u32.to_le_bytes());
    out.extend((image as u32).to_le_bytes());
    out.extend(0x30u16.to_le_bytes());
    out.extend(16u16.to_le_bytes());
    out.extend([0, 1, 3, 4]);
    out.extend(512u16.to_le_bytes());
    out.extend(20u16.to_le_bytes());
    out.extend([0; 24]);
    out.extend((0..image).map(|i| i as u8));
    for alpha in 0..16u8 {
        out.extend([255, 255, 255, alpha * 8]);
    }
    out
}

#[test]
fn malformed_font_pages_are_errors_not_panics() {
    // the pixels are many bytes alike: their prefixes stand for the rest
    let page = font_page();
    let head = &page[..0x60];
    sweep("font page", head, |head| {
        let mut data = head.to_vec();
        data.extend(&page[head.len().min(page.len())..]);
        if let Ok(page) = font::Page::read(&data) {
            for glyph in 0..page.glyphs() + 2 {
                let _ = page.cell(glyph);
            }
            let _ = page.write();
        }
    });
}

#[test]
fn malformed_sound_banks_are_errors_not_panics() {
    let bank = soundbank::test_bank(&[22050], 100);
    sweep("sound bank", &bank, |data| {
        if let Ok(bank) = soundbank::Bank::from_bytes(data) {
            if let Ok(entries) = bank.entries() {
                for entry in &entries {
                    let _ = bank.voices(entry);
                }
            }
            for vag in 0..bank.hd.vags.len() {
                let _ = bank.decode_vag(vag);
            }
            let _ = soundfont::write(&bank, "x");
            let _ = bank.clone().build();
        }
    });
    let wav =
        soundbank::wav_bytes(&soundbank::Sound { rate: 22050, samples: vec![0, 100, -100, 5], loop_start: Some(1) })
            .unwrap();
    sweep("WAV", &wav, |data| {
        let _ = soundbank::read_wav(data);
    });
}

fn a_sequence() -> sequence::Sequence {
    use sequence::{Event, Message};
    sequence::Sequence {
        division: sequence::DIVISION,
        events: vec![
            Event::new(0, Message::tempo(500_000)),
            Event::new(10, Message::Program { channel: 1, program: 3 }),
            Event::new(0, Message::NoteOn { channel: 1, key: 60, velocity: 100 }),
            Event::new(200, Message::NoteOff { channel: 1, key: 60 }),
            Event::new(0, Message::end()),
        ],
    }
}

#[test]
fn malformed_sequences_are_errors_not_panics() {
    let sq = a_sequence().write().unwrap();
    sweep("sequence", &sq, |data| {
        if let Ok(sequence) = sequence::Sequence::read(data) {
            let _ = sequence.write();
            let _ = (sequence.notes(), sequence.programs(), sequence.loop_ticks());
        }
    });
    let midi = music::to_midi(&a_sequence(), "x");
    sweep("MIDI", &midi, |data| {
        if let Ok(smf) = music::Smf::read(data) {
            let _ = smf.write();
        }
        let _ = soundbank::midi_notes(data);
        let _ = music::from_midi(data, &a_sequence(), &|_| true);
    });
}

/// A model exported as glTF: its JSON and its buffer.
fn exported() -> (Value, Vec<u8>) {
    let mut out = Folder::new();
    to_gltf::export_model(&model(), &mut out, "x", &to_gltf::Options::default()).unwrap();
    let json = serde_json::from_slice(out.get("x.gltf").unwrap()).unwrap();
    (json, out.get("x.bin").unwrap().to_vec())
}

/// What a glTF goes through: read, its meshes matched to the template's frames, imported.
fn import(json: &Value, bin: &[u8], template: &[u8]) {
    let bin = bin.to_vec();
    let Ok(gltf) = from_gltf::with_buffers(json.clone(), &|_| Ok(std::sync::Arc::new(bin.clone()))) else {
        return;
    };
    let Ok(model) = mds::parse(template) else {
        return;
    };
    let _ = from_gltf::frames_of(&gltf.json, &model);
    let _ = from_gltf::mesh_geometry(&gltf, &model);
    let _ = from_gltf::frame_geometry(&gltf, &model);
    let _ = from_gltf::import_gltf(&gltf, template, None, None, None);
    let _ = from_gltf::material_textures(&gltf);
    let _ = map_import::subtree(&gltf.json, &[0, 1]);
}

/// Every integer of a JSON value replaced by each of `values` in turn, one at a time.
fn integer_cases(json: &Value, values: &[i64]) -> Vec<Value> {
    fn places(value: &Value, path: String, out: &mut Vec<String>) {
        match value {
            Value::Number(number) if number.is_i64() || number.is_u64() => out.push(path),
            Value::Array(items) => {
                items.iter().enumerate().for_each(|(at, item)| places(item, format!("{path}/{at}"), out))
            }
            Value::Object(map) => map.iter().for_each(|(key, item)| places(item, format!("{path}/{key}"), out)),
            _ => {}
        }
    }
    let mut paths = Vec::new();
    places(json, String::new(), &mut paths);
    let mut out = Vec::new();
    for path in paths {
        for &value in values {
            let mut changed = json.clone();
            *changed.pointer_mut(&path).unwrap() = Value::from(value);
            out.push(changed);
        }
    }
    out
}

#[test]
fn malformed_gltf_is_an_error_not_a_panic() {
    let (json, bin) = exported();
    let template = mds::build(&model()).unwrap();
    // the buffer cut short and changed
    sweep("glTF buffer", &bin, |bin| import(&json, bin, &template));
    // every index, count, offset and size in the JSON out of range, negative or huge
    let panicked: Vec<String> = integer_cases(&json, &[-1, 3, 99, 1 << 40, i64::MAX])
        .into_iter()
        .filter(|case| catch_unwind(AssertUnwindSafe(|| import(case, &bin, &template))).is_err())
        .map(|case| case.to_string())
        .collect();
    assert!(panicked.is_empty(), "{} glTF cases panic, the first: {}", panicked.len(), panicked[0]);
    // the JSON text cut short
    let text = serde_json::to_vec(&json).unwrap();
    sweep("glTF text", &text[..text.len().min(400)], |text| {
        let _ = from_gltf::load_gltf(text, &|_| Ok(bin.clone()));
    });
    // nesting past serde_json's limit (128) is an error, not a stack overflow
    let deep = format!("{}{}", "[".repeat(100_000), "]".repeat(100_000));
    assert!(from_gltf::load_json(&deep).is_err());
}

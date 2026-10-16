use super::*;
use dc2tk_assets::mds as mdsfile;
use std::path::Path;

/// A disc of files given in memory.
struct Files(BTreeMap<String, Vec<u8>>);

impl Disc for Files {
    fn read(&self, path: &str) -> Option<Cow<'_, [u8]>> {
        self.0.iter().find(|(name, _)| name.eq_ignore_ascii_case(path)).map(|(_, data)| Cow::Borrowed(data.as_slice()))
    }

    fn paths(&self) -> Vec<String> {
        self.0.keys().cloned().collect()
    }
}

fn prim(kind: u32, count: u32) -> Prim {
    Prim { kind, count, material: 0 }
}

#[test]
fn packets_follow_the_vu_batches() {
    // 57 corners of 3 quadwords fill one batch of a fixed model: 2 header + 1 end + 4 + 171
    assert_eq!(packet(&prim(3, 57), None), 178);
    assert_eq!(packet(&prim(3, 58), None), 185);
    // a strip repeats two corners in the next batch: 57, then 60 + 2 - 57 = 5
    assert_eq!(packet(&prim(4, 60), None), 3 + (4 + 171) + (4 + 15));
    // with colours a corner takes 4 quadwords and a batch 42 corners
    assert_eq!(packet(&prim(3 | F_COLOUR, 42), None), 3 + 4 + 168);
    // skinned: two weight quadwords a corner; 32 bones leave vu1_offset 116, 21 corners a batch
    assert_eq!(packet(&prim(3, 21), Some(32)), 3 + 4 + 21 * 5);
    assert_eq!(packet(&prim(3, 0), None), 3);
}

#[test]
fn stacks_count_as_mgcmemory_does() {
    let mut stack = Stack::default();
    stack.alloc(3);
    stack.alloc64(2);
    assert_eq!(stack.used, 6);
    stack.align64();
    assert_eq!(stack.bytes(), 8 * 16);
}

#[test]
fn a_model_is_counted_from_its_mds() {
    let mut mds = mdsfile::Mds::new();
    mds.nodes.push(mdsfile::Node::new("root", -1));
    let mut mesh = mdsfile::Node::new("box--x", 0);
    mesh.mdt = Some(mdsfile::Mdt {
        vertices: vec![[0.0, 0.0, 0.0, 1.0]; 3],
        normals: vec![[0.0, 1.0, 0.0, 1.0]],
        uvs: vec![[0.0, 0.0, 1.0, 0.0]],
        materials: vec![mdsfile::Material::new("wood")],
        prims: vec![mdsfile::Prim { kind: 3, material: 0, verts: vec![vec![0, 0, 0], vec![1, 0, 0], vec![2, 0, 0]] }],
        ..Default::default()
    });
    mds.nodes.push(mesh);
    let data = mdsfile::build(&mds).unwrap();
    let mut stack = Stack::default();
    model(&mut stack, &data, &|_| Visual::Fixed, None).unwrap();
    // frames 2*17+2, table 1+2; root: name 1, attr 11; box: name 1, attr 11, bound 11,
    // visual 7 + 1, material 3, face 3, group 4, packet 3 + 4 + 9
    let expected = 36 + 3 + (1 + 11) + (1 + 11 + 11 + 8 + 3 + 3 + 4 + 16);
    assert_eq!(stack.used, expected);
    let mut stack = Stack::default();
    collision(&mut stack, &data).unwrap();
    // 2 CColFrames 2*18+2, the mesh's CCollisionMDT 7, one triangle 5, bound 13
    assert_eq!(stack.used, 38 + 7 + 5 + 13);
}

#[test]
fn motions_convert_key_lists() {
    // two lists of 4 and 0 keys: each Mot_List 3, values keys+1, key frames keys/4+1
    let mut mot = Vec::new();
    for (keys, more) in [(4u32, 1u32), (0, 0)] {
        let mut header = [0u8; 32];
        header[16..20].copy_from_slice(&keys.to_le_bytes());
        header[20..24].copy_from_slice(&more.to_le_bytes());
        mot.extend(header);
        mot.extend(vec![0u8; 32 * keys as usize]);
    }
    let mut stack = Stack::default();
    key_list_memory(&mut stack, &mot);
    assert_eq!(stack.used, (3 + 5 + 2) + (3 + 1 + 1));
}

/// A TIM2 picture header as EnterTexture reads it.
fn tim2(kind: u8, width: u16, height: u16, mips: u8) -> Vec<u8> {
    let mut data = vec![0u8; 0x30];
    data[0x21] = mips;
    data[0x23] = kind;
    data[0x24..0x26].copy_from_slice(&width.to_le_bytes());
    data[0x26..0x28].copy_from_slice(&height.to_le_bytes());
    data
}

/// An IM3 file of pictures (name, group, picture).
fn im3(pictures: &[(&str, i32, Vec<u8>)]) -> Vec<u8> {
    let mut out = b"IM3\0".to_vec();
    out.extend(0u32.to_le_bytes());
    out.extend((pictures.len() as u32).to_le_bytes());
    out.extend(0u32.to_le_bytes());
    let mut body: Vec<u8> = Vec::new();
    let start = 0x10 + 0x40 * pictures.len();
    for (name, group, picture) in pictures {
        let mut entry = vec![0u8; 0x40];
        entry[..name.len()].copy_from_slice(name.as_bytes());
        entry[0x24..0x28].copy_from_slice(&((start + body.len()) as i32).to_le_bytes());
        entry[0x2C..0x30].copy_from_slice(&group.to_le_bytes());
        out.extend(entry);
        body.extend(picture);
    }
    out.extend(body);
    out
}

#[test]
fn pictures_fill_texture_blocks() {
    // 256x256 at 32 bits is 1024 GS blocks; four fit the 4768 of a block, the fifth spills
    let big = tim2(3, 256, 256, 1);
    let five: Vec<(&str, i32, Vec<u8>)> = (0..5).map(|_| ("p", 0, big.clone())).collect();
    let used = shared_block(&[&im3(&five)]);
    assert_eq!(used, BlockUse { blocks: 2, spills: 1 });
    // a group starts its own block, and spilling moves the later groups on
    let grouped = im3(&[("a", 0, big.clone()), ("b", 1, big.clone())]);
    assert_eq!(shared_block(&[&grouped]).blocks, 2);
    // unswizzled 8-bit pictures go into the Z buffer first (256x256: 256 blocks, 1024 of it)
    let small = tim2(5, 256, 256, 1);
    let mut blocks = Blocks::new();
    enter_img(&mut blocks, &im3(&[("z", 0, small)]), 0, TOWN_BLOCKS);
    assert_eq!(blocks.remain(0), BLOCK_VRAM - CLUT_BLOCKS);
    // a mip chain counts each level a quarter of the last (64 + 16 + 4 blocks), in whole pages
    let chain = texture(&tim2(3, 64, 64, 3), 0, false, false).unwrap();
    assert_eq!(chain.vram, 96);
    assert_eq!(chain.vram % PAGE_BLOCKS, 0);
}

/// An IMG (or IM2) file of pictures (name, picture).
fn img(magic: &[u8; 3], pictures: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut out = magic.to_vec();
    out.push(0);
    out.extend((pictures.len() as u32).to_le_bytes());
    out.extend([0u8; 8]);
    let mut body: Vec<u8> = Vec::new();
    let start = 0x10 + 0x30 * pictures.len();
    for (name, picture) in pictures {
        let mut entry = vec![0u8; 0x30];
        entry[..name.len()].copy_from_slice(name.as_bytes());
        entry[0x20..0x24].copy_from_slice(&((start + body.len()) as i32).to_le_bytes());
        out.extend(entry);
        body.extend(picture);
    }
    out.extend(body);
    out
}

#[test]
fn an_img_file_spills_but_enters_as_one_block() {
    // five 1024-block pictures: the fifth goes on to block 1, yet EnterIMGFile returns 0 for an
    // IMG file (its running block and its last block move together), so the next file is
    // entered at block 1, beside the spilled picture
    let big = tim2(3, 256, 256, 1);
    let five: Vec<(&str, Vec<u8>)> = (0..5).map(|_| ("p", big.clone())).collect();
    for magic in [b"IMG", b"IM2"] {
        let mut blocks = Blocks::new();
        assert_eq!(enter_img(&mut blocks, &img(magic, &five), 0, TOWN_BLOCKS), 0);
        assert_eq!((blocks.used(), blocks.spills), (1, 1));
        // the spilled picture stays in block 0's list, as the engine leaves it
        assert_eq!(blocks.remain(0), BLOCK_VRAM - 5 * 1024);
    }
    // an IM3 file returns the blocks it went past
    let three: Vec<(&str, i32, Vec<u8>)> = (0..5).map(|_| ("p", 0, big.clone())).collect();
    assert_eq!(enter_img(&mut Blocks::new(), &im3(&three), 0, TOWN_BLOCKS), 1);
}

#[test]
fn map_entries_read_the_game_tables_as_the_engine_loads_them() {
    let row = |code: &str, kind: i64, number: usize| format!("MAP_NAME \"{code}\",\"\",\"\",{kind},1;// {number}\r\n");
    // the game's table: Palm Brinks (10) and its parts (11 to 14) by number, the others by type
    let mut game = String::from("MAP_NAME_NUM 18;\r\n");
    for number in 0..10 {
        game += &row("", 0, number);
    }
    for (number, code) in ["m01", "m02", "m03", "m04", "m05"].into_iter().enumerate() {
        game += &row(code, 3, 10 + number);
    }
    game += &row("s19", 8, 15);
    game += &format!("// {}", row("x01", 7, 16));
    game += &row("a02ia", 2, 16);
    game += &row("s51", 7, 17);
    // ignored: map.cfg, which the game never reads, and a later language's second listing of s51
    let old = format!("MAP_NAME_NUM 1;\r\n{}", row("s19", 3, 0));
    let other = format!("MAP_NAME_NUM 2;\r\n{}{}", row("S51", 4, 0), row("i03h02", 4, 1));
    let disc = Files(BTreeMap::from([
        ("map/map.cfg".to_string(), old.into_bytes()),
        ("map/map1.cfg".to_string(), game.into_bytes()),
        ("map/map2.cfg".to_string(), other.into_bytes()),
    ]));
    let kinds: Vec<(String, MapKind)> = map_entries(&disc).into_iter().map(|entry| (entry.name, entry.kind)).collect();
    let expected = [
        ("m01", MapKind::Town),
        ("m02", MapKind::Interior),
        ("m03", MapKind::Interior),
        ("m04", MapKind::Interior),
        ("m05", MapKind::Interior),
        ("s19", MapKind::Dungeon),
        ("a02ia", MapKind::Interior),
        ("s51", MapKind::Dungeon),
        ("i03h02", MapKind::Interior),
    ];
    assert_eq!(kinds, expected.map(|(name, kind)| (name.to_string(), kind)));
}

#[test]
fn party_packs_find_their_stacks() {
    assert_eq!(party_stack("mainchr/c01_base3.chr"), Some((0, 68500, "dungeon")));
    assert_eq!(party_stack("chara/c02_base.chr"), Some((0, 75000, "town")));
    assert_eq!(party_stack("mainchr/ws012.chr"), Some((2, 7000, "dungeon")));
    assert_eq!(party_stack("mainchr/ha001.chr"), Some((4, 3000, "dungeon")));
    assert_eq!(party_stack("chara/c02a.chr"), None);
    assert_eq!(SCRIPT_FILE, 312_816);
}

#[test]
fn whole_files_and_scripts_are_checked() {
    let original = Files(BTreeMap::from([
        ("map/s/s17/s17_1.stb".to_string(), vec![0u8; 1000]),
        ("chara/c02a.chr".to_string(), vec![0u8; 1000]),
        ("menu/a.img".to_string(), vec![0u8; 10]),
    ]));
    let modded = BTreeMap::from([
        ("map/s/s17/s17_1.stb".to_string(), vec![0u8; 400_000]),
        ("chara/c02a.chr".to_string(), vec![0u8; 3_300_000]),
        ("menu/a.img".to_string(), vec![0u8; 20]),
    ]);
    let overlay = Overlay::new(&original, &modded);
    let changed: Vec<String> = modded.keys().cloned().collect();
    let found = check(&original, &overlay, &changed);
    let errors: Vec<&Finding> = found.iter().filter(|item| item.level == Level::Error).collect();
    assert_eq!(errors.len(), 2, "{found:?}");
    assert!(errors.iter().any(|item| item.text.contains("script buffer")));
    assert!(errors.iter().any(|item| item.text.contains("read buffer")));
    assert!(found.iter().any(|item| item.level == Level::Note && item.text.starts_with("menu/a.img")));
    // nothing changed, nothing to say
    assert!(check(&original, &original, &[]).is_empty());
}

fn bank(samples: usize) -> Vec<u8> {
    dc2tk_assets::models::write_pack(&[("x.hd".to_string(), vec![0; 32]), ("x.bd".to_string(), vec![0; samples])])
        .unwrap()
}

#[test]
fn sound_banks_are_checked_against_their_regions() {
    let index = "800\tx\tEV\ty\t-\tBS_051\tFG_000\tSR_051\tSR_051_000\tOB_001\t-\tOB_001\t*\tOB_009\t-\tEV_050_100\n\
                 801\tx\tEV\ty\t-\t*\t*\t*\t*\t*\t*\t*\t*\t*\t-\t-\n";
    let sets = sound_sets(index.as_bytes());
    assert_eq!(sets[0], (800, vec!["BS_051".to_string(), "FG_000".into(), "SR_051".into(), "OB_001".into()]));
    assert_eq!(sets[1], (801, Vec::<String>::new()));
    let original = Files(BTreeMap::from([
        ("snd2/MAP_index.txt".to_string(), index.as_bytes().to_vec()),
        ("snd2/bs/BS_051.snd".to_string(), bank(0x8_0000)),
        ("snd2/fg/FG_000.snd".to_string(), bank(0x8_0000)),
        ("snd2/SY_000.snd".to_string(), bank(0x1_0000)),
    ]));
    let modded = BTreeMap::from([
        ("snd2/fg/FG_000.snd".to_string(), bank(0x9_0000)),
        ("snd2/SY_000.snd".to_string(), bank(0x6_DD80)),
    ]);
    let overlay = Overlay::new(&original, &modded);
    let found = check(&original, &overlay, &modded.keys().cloned().collect::<Vec<_>>());
    let errors: Vec<&str> =
        found.iter().filter(|item| item.level == Level::Error).map(|item| item.text.as_str()).collect();
    // FG_000 with BS_051 is 0x110020 together; SY_000 outgrows its region and the IOP window
    assert!(errors.iter().any(|text| text.contains("sound set 800")), "{found:?}");
    assert!(errors.iter().any(|text| text.contains("system sound region")), "{found:?}");
    assert!(errors.iter().any(|text| text.contains("IOP")), "{found:?}");
    assert!(
        found.iter().any(|item| item.level == Level::Warning && item.text.starts_with("snd2/fg/FG_000.snd")),
        "{found:#?}"
    );
}

/// A bank whose .cfg lists entries (and nothing else a table counts).
fn bank_of_entries(entries: usize) -> Vec<u8> {
    let cfg: String = (0..entries).map(|entry| format!("{entry}\tSE_{entry}\r\n")).collect::<String>() + "END\r\n";
    dc2tk_assets::models::write_pack(&[
        ("x.hd".to_string(), vec![0; 32]),
        ("x.bd".to_string(), vec![0; 16]),
        ("x.cfg".to_string(), cfg.into_bytes()),
    ])
    .unwrap()
}

#[test]
fn sound_tables_are_checked_against_their_layers_stack() {
    // 12 bytes an entry, in quadwords, and 2 more
    assert_eq!(ee_tables("FG_000", &bank_of_entries(10)), Some((8 + 2) * QW));
    let index = "800\tx\tEV\ty\t-\tBS_051\tFG_000\tSR_051\tSR_051_000\tOB_001\tOB_002\t*\n";
    let original = Files(BTreeMap::from([
        ("snd2/MAP_index.txt".to_string(), index.as_bytes().to_vec()),
        ("snd2/fg/FG_000.snd".to_string(), bank_of_entries(10)),
        ("snd2/sr/SR_051.snd".to_string(), bank_of_entries(10)),
        ("snd2/ob/OB_001.snd".to_string(), bank_of_entries(10)),
        ("snd2/ob/OB_002.snd".to_string(), bank_of_entries(40)),
    ]));
    let modded = BTreeMap::from([
        // 8,208 bytes of 8,192; 1,040 of 1,024; 560 + 512 of 1,024 between the set's object banks
        ("snd2/fg/FG_000.snd".to_string(), bank_of_entries(681)),
        ("snd2/sr/SR_051.snd".to_string(), bank_of_entries(84)),
        ("snd2/ob/OB_001.snd".to_string(), bank_of_entries(44)),
    ]);
    let overlay = Overlay::new(&original, &modded);
    let found = check(&original, &overlay, &modded.keys().cloned().collect::<Vec<_>>());
    let warned = |path: &str| {
        found.iter().any(|item| {
            item.level == Level::Warning && item.text.starts_with(path) && item.text.contains("EE memory stack")
        })
    };
    assert!(warned("snd2/fg/FG_000.snd") && warned("snd2/sr/SR_051.snd"), "{found:#?}");
    assert!(warned("snd2/ob/OB_001.snd"), "{found:#?}");
    assert!(found.iter().any(|item| item.text.contains("with sound set 800's other object banks")), "{found:#?}");
    // tables that did not grow say nothing
    let same = BTreeMap::from([("snd2/fg/FG_000.snd".to_string(), bank_of_entries(10))]);
    let overlay = Overlay::new(&original, &same);
    assert!(!check(&original, &overlay, &["snd2/fg/FG_000.snd".to_string()])
        .iter()
        .any(|item| item.text.contains("EE")));
}

#[test]
fn map_files_name_their_map() {
    assert_eq!(map_of("map/s/s17/s17.mpk").as_deref(), Some("s17"));
    assert_eq!(map_of("map/d/d01/f01/d01f01.ipk").as_deref(), Some("d01f01"));
    assert_eq!(map_of("map/cmn/g00/g00.mpk"), None);
    assert_eq!(map_of("map/s/s17/s17_1.stb"), None);
    assert_eq!(add_folder("g00/g00"), ("map/cmn/g00".to_string(), "g00".to_string()));
    assert_eq!(n(3_200_000), "3,200,000");
}

/// The disc's own loads stay within every budget, and the largest estimated uses are the
/// RETAIL_ constants (when one fails, the budget or the estimate is wrong). Set DC2TK_CORPUS to
/// an extracted disc (`extracted/disc`).
#[test]
#[ignore = "needs DC2TK_CORPUS"]
fn corpus_retail_fits_the_budgets() {
    let corpus = dc2tk_disc::samples::corpus();
    let disc = DiscSource::open(Path::new(&corpus)).unwrap();
    let mut wrong = Vec::new();
    let mut largest: BTreeMap<MapKind, (u64, String)> = BTreeMap::new();
    let entries = map_entries(&disc);
    assert!(entries.len() > 180, "{} maps listed", entries.len());
    for entry in &entries {
        let Some(load) = map_load(&disc, entry) else { continue };
        let blocks = map_blocks(&disc, entry).unwrap();
        let name = &entry.name;
        if load.read > entry.kind.loading_screen() {
            wrong.push(format!("{name}: read {}", load.read));
        }
        if load.placements > entry.kind.placements() {
            wrong.push(format!("{name}: {} placements", load.placements));
        }
        let limit = match entry.kind {
            MapKind::Dungeon => DUNGEON_EFFECT_BLOCK,
            MapKind::Interior => TOWN_EFFECT_BLOCK - TOWN_INTERIOR_BLOCK,
            MapKind::Town => TOWN_EFFECT_BLOCK,
        };
        if blocks.blocks > limit {
            wrong.push(format!("{name}: {} texture blocks", blocks.blocks));
        }
        let slot = largest.entry(entry.kind).or_default();
        if load.stack > slot.0 {
            *slot = (load.stack, name.clone());
        }
    }
    assert_eq!(largest[&MapKind::Town].0, RETAIL_TOWN_MAP, "{largest:?}");
    assert_eq!(largest[&MapKind::Interior].0, RETAIL_INTERIOR_MAP, "{largest:?}");
    assert_eq!(largest[&MapKind::Dungeon].0, RETAIL_DUNGEON_MAP, "{largest:?}");
    // every sound set's field banks fit together, and every bank its region
    let index = disc.read("snd2/MAP_index.txt").unwrap();
    let paths = disc.paths();
    let mut samples: HashMap<String, u64> = HashMap::new();
    let mut tables: BTreeMap<&str, (u64, String)> = BTreeMap::new();
    let mut object_tables: HashMap<String, u64> = HashMap::new();
    for path in
        paths.iter().filter(|path| path.to_lowercase().starts_with("snd2/") && path.to_lowercase().ends_with(".snd"))
    {
        let size = bank_samples(&disc.read(path).unwrap()).unwrap();
        let name = path.rsplit('/').next().unwrap().to_uppercase().replace(".SND", "");
        let budget = match &name[..2] {
            "BG" => SPU_MUSIC,
            "SY" => SPU_SYSTEM,
            "SP" => SPU_MENU,
            "CH" => SPU_CHARACTER,
            _ => SPU_FIELD,
        };
        if size + 16 > budget || (IOP_STAGING + 1..=IOP_STAGING + 0x100).contains(&size) || size > 2 * IOP_STAGING {
            wrong.push(format!("{path}: {size} bytes of samples"));
        }
        if let Some((stack, layer)) = ee_table_stack(&name) {
            let used = ee_tables(&name, &disc.read(path).unwrap()).unwrap();
            if used > stack {
                wrong.push(format!("{path}: {used} bytes of tables"));
            }
            let slot = tables.entry(layer).or_default();
            if used > slot.0 {
                *slot = (used, name.clone());
            }
            if layer == "object" {
                object_tables.insert(name.clone(), used);
            }
        }
        samples.insert(name, size);
    }
    // the largest tables of the base, battle and ambient layers (docs/assets/sounds.md section 8)
    assert_eq!(tables["battle"], (7_072, "FG_000".to_string()), "{tables:?}");
    assert_eq!(tables["ambient"].0, 368, "{tables:?}");
    let sets = sound_sets(&index);
    assert!(sets.len() > 350, "{} sound sets", sets.len());
    for (set, banks) in sets {
        let total: u64 = banks.iter().map(|bank| samples.get(bank).map_or(0, |size| size + 16)).sum();
        if total > SPU_FIELD {
            wrong.push(format!("sound set {set}: {total}"));
        }
        let objects: u64 = banks.iter().filter_map(|bank| object_tables.get(bank)).sum();
        if objects > ee_table_stack("OB").unwrap().0 {
            wrong.push(format!("sound set {set}: {objects} bytes of object tables"));
        }
    }
    for path in disc.paths() {
        let lower = path.to_lowercase();
        if lower.ends_with(".stb") && lower.starts_with("map/") {
            let size = disc.read(path).unwrap().len() as u64;
            if size > SCRIPT_FILE {
                wrong.push(format!("{path}: script {size}"));
            }
        }
        if lower.ends_with(".sky") {
            for (name, img) in img_members(&disc.read(path).unwrap()) {
                if shared_block(&[img]).blocks > 1 {
                    wrong.push(format!("{path}: {name} spills"));
                }
            }
        }
        if !lower.ends_with(".chr") {
            continue;
        }
        let data = disc.read(path).unwrap();
        // e612_gri_kao.chr (6.2 MB) is never loaded: no script names it
        if data.len() as u64 > READ_BUFFER && !lower.ends_with("e612_gri_kao.chr") {
            wrong.push(format!("{path}: {} bytes", data.len()));
        }
        let limits = skinned_limits(&data);
        if !limits.is_empty() {
            wrong.push(format!("{path}: {limits:?}"));
        }
        // the packs a party slot loads with LoadPack (LoadSkin's costumes and shoes are only
        // estimated the same way); Max loads c01_base2 to 4 (his sword's model number + 2,
        // GetMainCharaModelName), so c01_base0 and 1 are left out
        let Some((stack, quadwords, _)) = party_stack(path) else { continue };
        let skin = [1, 5].contains(&stack);
        let unused = lower.ends_with("c01_base0.chr") || lower.ends_with("c01_base1.chr");
        let estimate = chara_pack_bytes(&data).unwrap();
        if !skin && !unused && estimate > quadwords * QW {
            wrong.push(format!("{path}: estimated {estimate} of {}", quadwords * QW));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

//! What the ported tests share: the repository's data folders, the native tables, the fixtures of
//! the Python tool's tests/build.py, a temporary folder, and running C text in the test VM (the Python tool's vm.run).
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use dc2tk_script::natives::{self, Table};
use dc2tk_script::node::Node;
use dc2tk_script::vm::{self, Value};
use dc2tk_script::{compile, library, parse, sdk};
use indexmap::IndexMap;

/// The repository folder (this crate is src/crates/dc2tk-script).
pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..").canonicalize().unwrap()
}

/// The native tables, as natives.load() reads them.
pub fn tables() -> &'static BTreeMap<String, Table> {
    static TABLES: OnceLock<BTreeMap<String, Table>> = OnceLock::new();
    TABLES.get_or_init(|| natives::load(&root().join("src").join("natives")).unwrap())
}

/// sdk.SDK_DIR.
pub fn sdk_dir() -> PathBuf {
    root().join("sdk")
}

/// definitions.DEFAULT_SET.
pub fn default_set() -> PathBuf {
    root().join("definitions").join("pal")
}

/// sdk.load_texts().
pub fn sdk_texts() -> &'static IndexMap<String, String> {
    static TEXTS: OnceLock<IndexMap<String, String>> = OnceLock::new();
    TEXTS.get_or_init(|| sdk::load_texts(&sdk_dir()).unwrap())
}

/// A map of texts from pairs.
pub fn texts(pairs: &[(&str, &str)]) -> IndexMap<String, String> {
    pairs.iter().map(|(key, value)| (key.to_string(), value.to_string())).collect()
}

pub fn no_texts() -> IndexMap<String, String> {
    IndexMap::new()
}

/// The text of an error, as str(error).
pub fn message<T: std::fmt::Debug>(result: dc2tk_script::Result<T>) -> String {
    match result {
        Ok(value) => panic!("expected an error, got {value:?}"),
        Err(error) => error.to_string(),
    }
}

/// A folder removed when dropped (tempfile.TemporaryDirectory).
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new() -> TempDir {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNT: AtomicUsize = AtomicUsize::new(0);
        let name = format!(
            "dc2tk-test-{}-{}-{}",
            std::process::id(),
            COUNT.fetch_add(1, Ordering::SeqCst),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        );
        let path = std::env::temp_dir().join(name);
        std::fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Writes a file, making its folders.
pub fn write(path: &Path, data: impl AsRef<[u8]>) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, data).unwrap();
}

/// A small deterministic generator for the tests that used random.Random (only the spread of the
/// values matters, not Python's exact sequence).
pub struct Random(u64);

impl Random {
    pub fn new(seed: u64) -> Random {
        Random(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03)
    }

    pub fn next(&mut self) -> u64 {
        // splitmix64
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// random.randrange(stop).
    pub fn below(&mut self, stop: usize) -> usize {
        (self.next() % stop as u64) as usize
    }

    pub fn bytes(&mut self, count: usize) -> Vec<u8> {
        (0..count).map(|_| self.below(256) as u8).collect()
    }
}

// the Python tool's tests/build.py

pub const SECTOR: usize = 2048;
const ROOT_SECTOR: usize = 18;
const PACK_SLOT: i32 = 0x50;

fn both32(value: u32) -> Vec<u8> {
    let mut out = value.to_le_bytes().to_vec();
    out.extend(value.to_be_bytes());
    out
}

fn both16(value: u16) -> Vec<u8> {
    let mut out = value.to_le_bytes().to_vec();
    out.extend(value.to_be_bytes());
    out
}

fn sectors(size: usize) -> usize {
    1.max(size.div_ceil(SECTOR))
}

pub fn pad(data: &[u8], alignment: usize) -> Vec<u8> {
    let mut out = data.to_vec();
    out.resize(data.len().next_multiple_of(alignment), 0);
    out
}

fn directory_record(name: &[u8], extent: usize, size: usize, is_dir: bool) -> Vec<u8> {
    let length = 33 + name.len() + usize::from(name.len().is_multiple_of(2));
    let mut record = vec![0u8; length];
    record[0] = length as u8;
    record[2..10].copy_from_slice(&both32(extent as u32));
    record[10..18].copy_from_slice(&both32(size as u32));
    record[25] = if is_dir { 2 } else { 0 };
    record[28..32].copy_from_slice(&both16(1));
    record[32] = name.len() as u8;
    record[33..33 + name.len()].copy_from_slice(name);
    record
}

/// An ISO image of the files, in the root folder.
pub fn iso_image(files: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut layout = Vec::new();
    let mut sector = ROOT_SECTOR + 1;
    for (name, data) in files {
        layout.push((*name, sector, data));
        sector += sectors(data.len());
    }
    let mut root = directory_record(b"\x00", ROOT_SECTOR, SECTOR, true);
    root.extend(directory_record(b"\x01", ROOT_SECTOR, SECTOR, true));
    for (name, start, data) in &layout {
        root.extend(directory_record(format!("{name};1").as_bytes(), *start, data.len(), false));
    }
    let mut image = vec![0u8; sector * SECTOR];
    let mut pvd = vec![0u8; SECTOR];
    pvd[0] = 1;
    pvd[1..6].copy_from_slice(b"CD001");
    pvd[6] = 1;
    pvd[156..190].copy_from_slice(&directory_record(b"\x00", ROOT_SECTOR, SECTOR, true));
    image[16 * SECTOR..17 * SECTOR].copy_from_slice(&pvd);
    image[17 * SECTOR..17 * SECTOR + 7].copy_from_slice(b"\xffCD001\x01");
    image[ROOT_SECTOR * SECTOR..ROOT_SECTOR * SECTOR + root.len()].copy_from_slice(&root);
    for (_, start, data) in &layout {
        image[start * SECTOR..start * SECTOR + data.len()].copy_from_slice(data);
    }
    image
}

fn cp932(text: &str) -> Vec<u8> {
    dc2tk_disc::text::encode(text).expect("a cp932 name")
}

/// DATA.HD4 and DATA.DAT holding the files.
pub fn archive_files(files: &[(&str, Vec<u8>)]) -> (Vec<u8>, Vec<u8>) {
    let table_size = 12 * (files.len() + 1);
    let mut names = Vec::new();
    let mut name_offsets = Vec::new();
    for (path, _) in files {
        name_offsets.push(table_size + names.len());
        names.extend(cp932(path));
        names.push(0);
    }
    let mut dat = Vec::new();
    let mut table = Vec::new();
    for ((_, data), name_offset) in files.iter().zip(name_offsets) {
        for value in [name_offset as i32, data.len() as i32, (dat.len() / SECTOR) as i32] {
            table.extend(value.to_le_bytes());
        }
        dat.extend(pad(data, SECTOR));
    }
    for value in [(table_size + names.len()) as i32, 0, 0] {
        table.extend(value.to_le_bytes());
    }
    table.extend(names);
    table.push(0);
    (table, dat)
}

/// A pack (.chr) of members.
pub fn pack(members: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    for (name, data) in members {
        let body = pad(data, 16);
        let mut header = cp932(name);
        header.resize(header.len().max(64), 0);
        for value in [PACK_SLOT, data.len() as i32, PACK_SLOT + body.len() as i32] {
            header.extend(value.to_le_bytes());
        }
        out.extend(pad(&header, 16));
        out.extend(body);
    }
    out.extend(vec![0u8; PACK_SLOT as usize]);
    out
}

fn ints(values: &[i32]) -> Vec<u8> {
    values.iter().flat_map(|value| value.to_le_bytes()).collect()
}

pub fn instruction(op: i32, arg1: i32, arg2: i32) -> Vec<u8> {
    ints(&[op, arg1, arg2])
}

fn function_record(record: i32, code: i32, local: i32, arg: i32, arg_types: &[i32]) -> Vec<u8> {
    let addr = record - code + 0x38;
    let mut out = ints(&[addr, 0, local, arg]);
    let mut types = arg_types.to_vec();
    types.resize(10, 0);
    out.extend(ints(&types));
    out
}

/// A two-function script: main prints "hello", calls a helper and loops; its strings block keeps
/// stale bytes.
pub fn sample_script() -> Vec<u8> {
    let code = 0x50;
    let main = code;
    let main_code = main + 0x38;
    let hello = main_code + 12 * 12;
    let helper = hello + 16;
    let mut out = b"SB2\x00".to_vec();
    out.extend(ints(&[main, code, 0x40, 2, 0, 1]));
    out.extend(vec![0u8; 0x24]);
    out.extend(ints(&[100, main, 200, helper]));
    out.extend(function_record(main, code, 1, 0, &[]));
    for (op, arg1, arg2) in [
        (3, 3, hello - code),
        (20, 1, 0),
        (3, 1, 12),
        (3, 1, 5),
        (19, 0, helper - code),
        (2, 0, 1),
        (21, 3, 0),
        (1, 0, 1),
        (17, main_code + 10 * 12 - code, 0),
        (23, 0, 0),
        (27, 0, 0),
        (0, 0, 0),
    ] {
        out.extend(instruction(op, arg1, arg2));
    }
    out.extend(b"hello\x00stale\x00item");
    out.extend(function_record(helper, code, 1, 1, &[1]));
    for (op, arg1, arg2) in [(22, 0, 0), (3, 1, 7), (15, 0, 0), (0, 0, 0)] {
        out.extend(instruction(op, arg1, arg2));
    }
    out
}

// the Python tool's vm.py

/// vm.functions(text): the text with the SDK's headers, compiled to the VM's function table.
pub fn functions(text: &str) -> dc2tk_script::Result<vm::Functions> {
    let program = library::expand(parse::parse(text)?, &no_texts(), sdk_texts())?;
    vm::lines_table(&compile::to_asm(&sdk::resolve(&program, tables())?, tables(), None)?)
}

/// vm.run_function: runs a function with 64 zero globals unless given.
pub fn run_function(
    table: &vm::Functions,
    name: &str,
    args: Vec<Value>,
    globals: Vec<Value>,
) -> dc2tk_script::Result<Value> {
    vm::Machine::new(table, globals).run(name, args)
}

/// vm.run(text, name, args): runs a function of a script and returns its result.
pub fn run(text: &str, name: &str, args: &[Value]) -> dc2tk_script::Result<Value> {
    let table = functions(text)?;
    run_function(&table, name, args.to_vec(), vec![Value::Int(0); 64])
}

/// The JSON of a tree node, for comparing trees in messages.
pub fn json(node: &Node) -> String {
    node.to_string()
}

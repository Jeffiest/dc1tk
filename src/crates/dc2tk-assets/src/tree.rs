//! The asset tree: every texture, character model and sound of the disc once, in formats common
//! tools open, and a manifest naming each by an id and listing every place the disc has it (see
//! "The asset tree" in src/docs/design.md).
//!
//! ```text
//! manifest.json
//! textures/<group>/<id>.png           one per distinct picture
//! models/<group>/<id>/<id>.gltf, .bin one per distinct character pack
//! sounds/<bank>/<NNN>[_<NAME>].wav    one per sound that plays a sample, in the folder of its first bank
//! text/<language>/<name>.txt          one per message file name and language (text.rs)
//! music/<bank's place>/<id>.mid       one per distinct sequence (.sq) of a bank (music.rs)
//! music/<bank's place>.sf2            its bank's programs, to hear it with (soundfont.rs)
//! ```
//!
//! A picture, pack or bank found at several places with the same bytes is one asset. Ids are the
//! kind and the game's own name (`texture:c02a02`, `model:c02fuku1`, `bank:EV_000_130`); of a
//! name's versions, one keeps the bare name (see name_assets) and the others take `@` and where
//! they are first found (`texture:a01ia_01@a01ic`). Sound entries take the names the definition set's header gives them
//! (`sound:SE_EV_000_130_GRIFFON_BREAK_WINDOW`), else their bank and number (`sound:EV_000_130/4`).
//! The folders are only for browsing: `<group>` is the folder of the disc file the asset is first
//! found in.

use std::collections::{BTreeMap, HashMap, HashSet};

use rayon::prelude::*;
use sha2::{Digest, Sha256};

use dc2tk_disc::extract::{MAX_PACK_DEPTH, PACK_SUFFIX};
use dc2tk_disc::files;
use dc2tk_disc::{archive, rebuild};

use crate::manifest::{self, Manifest};
use crate::soundbank::{self, Bank, EntryKind};
use crate::{im3, models, text, Context, Error, Result};

pub const MANIFEST: &str = "manifest.json";
pub const TEXTURES: &str = "textures";
pub const MODELS: &str = "models";
pub const SOUNDS: &str = "sounds";
pub const MUSIC: &str = "music";

/// A file of the disc's DATA.DAT, by its path there.
pub struct DiscFile<'a> {
    pub path: String,
    pub data: &'a [u8],
}

/// Where the disc has something: a file of DATA.DAT and the chain of pack members inside it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Place {
    pub path: String,
    pub members: Vec<String>,
}

impl Place {
    /// The path extract writes it at (a.pac.d/b.chr.d/c.img).
    pub fn display(&self) -> String {
        let mut out = self.path.clone();
        for member in &self.members {
            out.push_str(PACK_SUFFIX);
            out.push('/');
            out.push_str(member);
        }
        out
    }

    /// The folder an asset first found here is filed under: the disc file's folder.
    pub fn group(&self) -> String {
        match self.path.rsplit_once('/') {
            Some((folder, _)) => folder.to_string(),
            None => "root".to_string(),
        }
    }

    /// What tells apart a version of a name first found here: the disc file's name without its
    /// suffix when the asset is a member of it (d05f01 for map/d/d05/d05f01.ipk), else the disc
    /// file's folder with "-" between parts (dungeon-chara for dungeon/chara/c02_base.chr).
    pub fn label(&self) -> String {
        let file = self.path.rsplit('/').next().unwrap_or(&self.path);
        if !self.members.is_empty() {
            return match file.rsplit_once('.') {
                Some((stem, _)) if !stem.is_empty() => stem.to_string(),
                _ => file.to_string(),
            };
        }
        self.group().replace('/', "-")
    }
}

/// The kinds of asset, by their id's prefix.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    Texture,
    Model,
    Bank,
    Sound,
    Text,
    /// A sequence (.sq) of a bank: music, or an ambient loop.
    Music,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Texture => "texture",
            Kind::Model => "model",
            Kind::Bank => "bank",
            Kind::Sound => "sound",
            Kind::Text => "text",
            Kind::Music => "music",
        }
    }

    pub fn id(self, name: &str) -> String {
        format!("{}:{name}", self.name())
    }

    /// The kind an id's prefix names (texture, model, bank, sound, text, music).
    pub fn from_name(name: &str) -> Option<Kind> {
        [Kind::Texture, Kind::Model, Kind::Bank, Kind::Sound, Kind::Text, Kind::Music]
            .into_iter()
            .find(|kind| kind.name() == name)
    }
}

/// The game files the tree reads: texture files (.img), character packs (.chr), sound banks
/// (.snd) and message files (.mes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum FileKind {
    Textures,
    Pack,
    Bank,
    Messages,
}

pub fn file_kind(name: &str) -> Option<FileKind> {
    let lower = name.to_lowercase();
    if lower.ends_with(".img") {
        Some(FileKind::Textures)
    } else if lower.ends_with(".chr") {
        Some(FileKind::Pack)
    } else if lower.ends_with(".snd") {
        Some(FileKind::Bank)
    } else if text::is_message_file(&lower) {
        Some(FileKind::Messages)
    } else {
        None
    }
}

/// A game file of a kind the tree reads, at its place.
pub struct GameFile<'a> {
    pub place: Place,
    pub kind: FileKind,
    pub data: &'a [u8],
}

/// What a walk of the disc finds: the game files, and how many it leaves out because a member of
/// the same name comes before them in their pack, so that a build, which finds members by name,
/// cannot reach them.
pub struct Found<'a> {
    pub files: Vec<GameFile<'a>>,
    pub shadowed: usize,
}

fn walk<'a>(path: &str, chain: &mut Vec<String>, data: &'a [u8], out: &mut Found<'a>) {
    let name = chain.last().map_or(path, String::as_str);
    if let Some(kind) = file_kind(name) {
        out.files.push(GameFile { place: Place { path: path.to_string(), members: chain.clone() }, kind, data });
    }
    // as deep as extract goes
    if chain.len() >= MAX_PACK_DEPTH {
        return;
    }
    if let Some(members) = archive::parse_pack(data) {
        for member in &members {
            let reached = rebuild::member_named(&members, &member.name, false).map(|found| found.offset);
            if reached != Some(member.offset) {
                out.shadowed += usize::from(file_kind(&member.name).is_some());
                continue;
            }
            chain.push(member.name.clone());
            walk(path, chain, member.data(data), out);
            chain.pop();
        }
    }
}

/// Every texture file, character pack and sound bank of the disc, in the disc's order (members
/// after the pack holding them, as extract lists them). A path the disc's index gives twice is
/// read once, as a build finds it: the first.
pub fn game_files<'a>(disc: &[DiscFile<'a>]) -> Found<'a> {
    let mut seen = HashSet::new();
    let disc: Vec<&DiscFile<'a>> = disc.iter().filter(|file| seen.insert(file.path.as_str())).collect();
    let found: Vec<Found<'a>> = disc
        .par_iter()
        .map(|file| {
            let mut out = Found { files: Vec::new(), shadowed: 0 };
            walk(&file.path, &mut Vec::new(), file.data, &mut out);
            out
        })
        .collect();
    let shadowed = found.iter().map(|found| found.shadowed).sum();
    Found { files: found.into_iter().flat_map(|found| found.files).collect(), shadowed }
}

pub fn sha256(parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    hasher.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// A file name for an id's name: characters a file name cannot hold become "_", as do trailing
/// dots and spaces, and names Windows reserves (CON, NUL, ...) get a "_" in front.
pub fn file_stem(name: &str) -> String {
    let mut out: String =
        name.chars().map(|c| if "<>:\"/\\|?*".contains(c) || (c as u32) < 0x20 { '_' } else { c }).collect();
    if out.is_empty() {
        out.push('_');
    }
    if out.ends_with(['.', ' ']) {
        out.pop();
        out.push('_');
    }
    let base = out.split('.').next().unwrap_or("").to_uppercase();
    let reserved = matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (base.len() == 4
            && (base.starts_with("COM") || base.starts_with("LPT"))
            && base.as_bytes()[3].is_ascii_digit()
            && base.as_bytes()[3] != b'0');
    if reserved {
        out.insert(0, '_');
    }
    out
}

/// Ids for distinct contents in the order they are first found, from (name, label) pairs: the
/// first content of a name takes the name, a later one name@label, then name@label.2, and so on.
/// Names are told apart as Windows tells file names apart (ignoring case, after file_stem), so
/// every id has a file of its own.
pub fn assign_names(items: &[(String, String)]) -> Vec<String> {
    let mut taken = HashSet::new();
    items
        .iter()
        .map(|(name, label)| {
            let first = [name.clone(), format!("{name}@{label}")];
            let mut candidates = first.into_iter().chain((2..).map(|n| format!("{name}@{label}.{n}")));
            candidates.find(|candidate| taken.insert(file_stem(candidate).to_lowercase())).unwrap()
        })
        .collect()
}

/// A path for a glTF uri: bytes outside the unreserved set, "/" and "@" percent-encoded.
fn uri_path(path: &str) -> String {
    let mut out = String::new();
    for &b in path.as_bytes() {
        if b.is_ascii_alphanumeric() || b"-._~/@".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The names the definition set's header gives sound entries, by bank: {"EV_000_130": [(4,
/// "SE_EV_000_130_GRIFFON_BREAK_WINDOW"), ...]}. An enum se_<bank>_<set> names the entries of that
/// bank; any other enum ending in _se names the entries of the bank the comment above it names
/// last (snd2/SY_000).
pub type SoundNames = BTreeMap<String, Vec<(usize, String)>>;

pub fn sound_names(header: &str) -> SoundNames {
    let mut out = SoundNames::new();
    let mut comment: Vec<&str> = Vec::new();
    let mut current: Option<String> = None;
    for line in header.lines() {
        let line = line.trim();
        if let Some(bank) = &current {
            if line.starts_with('}') {
                current = None;
                continue;
            }
            let code = line.split("//").next().unwrap_or("");
            let Some((name, value)) = code.split_once('=') else {
                continue;
            };
            let name = name.trim();
            let value = value.trim().trim_end_matches(',').trim();
            if let Ok(number) = value.parse::<usize>() {
                out.entry(bank.clone()).or_default().push((number, name.to_string()));
            }
            continue;
        }
        if line.starts_with("//") {
            comment.push(line);
            continue;
        }
        if let Some(rest) = line.strip_prefix("enum ") {
            let name = rest.trim_end_matches('{').trim();
            // the definition set's suffix (_pal)
            let stem = name.rsplit_once('_').map_or(name, |(stem, _)| stem);
            current = if let Some(bank) = stem.strip_prefix("se_") {
                Some(bank.to_uppercase())
            } else if stem.ends_with("_se") {
                let text = comment.join(" ");
                text.match_indices("snd2/")
                    .filter_map(|(at, _)| {
                        let rest = &text[at + 5..];
                        let end = rest.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).unwrap_or(rest.len());
                        (end > 0).then(|| rest[..end].to_uppercase())
                    })
                    .last()
            } else {
                None
            };
        }
        comment.clear();
    }
    out
}

/// A picture's name as text. im3 reads a name's bytes as Latin-1, and the game's names are cp932
/// (森の美女), so a name is decoded as cp932 where it can be.
pub fn picture_name(name: &str) -> String {
    let bytes: Option<Vec<u8>> = name.chars().map(|c| u8::try_from(c as u32).ok()).collect();
    bytes.and_then(|bytes| dc2tk_disc::text::decode_strict(&bytes)).unwrap_or_else(|| name.to_string())
}

/// A picture of a texture file.
pub(crate) struct Picture {
    /// The game file (texture file) it is in.
    file: usize,
    /// Its entry in the file.
    entry: usize,
    name: String,
    format: &'static str,
    width: usize,
    height: usize,
    /// The hash of its game bytes: whether 8-bit pixels are stored swizzled, and its TIM2.
    game_hash: String,
}

enum Surveyed {
    Pictures(Vec<Picture>),
    Whole(String),
    Failed(String),
}

fn survey(index: usize, file: &GameFile) -> Surveyed {
    match file.kind {
        FileKind::Textures => match im3::read(file.data) {
            Ok(images) => Surveyed::Pictures(
                images
                    .into_iter()
                    .filter(|image| !image.no_image)
                    .map(|image| Picture {
                        file: index,
                        entry: image.index.unwrap_or(0),
                        format: image.format.name(),
                        width: image.width,
                        height: image.height,
                        game_hash: sha256(&[&[image.swizzled as u8], image.tim2.as_deref().unwrap_or_default()]),
                        name: image.name,
                    })
                    .collect(),
            ),
            Err(error) => Surveyed::Failed(error.to_string()),
        },
        FileKind::Pack | FileKind::Bank | FileKind::Messages => Surveyed::Whole(sha256(&[file.data])),
    }
}

/// One distinct content: where it is first found, and every place (game file, picture entry).
pub(crate) struct Asset {
    /// The game's name for it (a picture's as im3 reads it).
    pub name: String,
    /// The name as the id and manifest give it.
    pub shown: String,
    pub game_hash: String,
    /// Where it is: the game file, by its place in the walk, and the picture's entry.
    pub places: Vec<(usize, Option<usize>)>,
    /// The id's name, without the kind.
    pub id: String,
}

impl Asset {
    pub fn first(&self) -> usize {
        self.places[0].0
    }
}

/// Groups found things into distinct contents in the order they are first found.
fn distinct(found: impl Iterator<Item = (String, String, usize, Option<usize>)>) -> Vec<Asset> {
    let mut assets: Vec<Asset> = Vec::new();
    let mut by_key: HashMap<(String, String), usize> = HashMap::new();
    for (name, game_hash, file, entry) in found {
        match by_key.get(&(name.clone(), game_hash.clone())) {
            Some(&at) => assets[at].places.push((file, entry)),
            None => {
                by_key.insert((name.clone(), game_hash.clone()), assets.len());
                assets.push(Asset {
                    shown: name.clone(),
                    name,
                    game_hash,
                    places: vec![(file, entry)],
                    id: String::new(),
                });
            }
        }
    }
    assets
}

/// Gives each asset its id. The bare name goes to a version that is a file of its own on the disc
/// (snd2/sp/SP_003.snd) before copies inside packs (menu/0/inv0_bg.pac's), else to the first
/// found.
fn name_assets(assets: &mut [Asset], files: &[GameFile]) {
    let mut order: Vec<usize> = (0..assets.len()).collect();
    order.sort_by_key(|&at| !assets[at].places.iter().any(|&(file, _)| files[file].place.members.is_empty()));
    let items: Vec<(String, String)> =
        order.iter().map(|&at| (assets[at].shown.clone(), files[assets[at].first()].place.label())).collect();
    for (&at, id) in order.iter().zip(assign_names(&items)) {
        assets[at].id = id;
    }
}

/// The name a pack or bank goes by: its file name without the suffix.
fn file_name_stem(place: &Place) -> String {
    let name =
        place.members.last().map_or_else(|| place.path.rsplit('/').next().unwrap_or(&place.path), String::as_str);
    match name.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem.to_string(),
        _ => name.to_string(),
    }
}

/// What a texture export gives the rest: its PNG's path and hash.
struct TextureOut {
    file: String,
    hash: String,
}

/// What a model export gives the manifest.
struct ModelOut {
    files: Vec<(String, String)>,
    textures: Vec<String>,
    placeholders: Vec<String>,
}

/// One entry of a bank.
struct EntryOut {
    index: usize,
    plays: &'static str,
    label: String,
    category: String,
    /// The WAV's file name, hash, and the hash of the samples it plays (the VAG).
    wav: Option<(String, String, String)>,
    /// The WAV, not yet written, of an entry of a bank with other versions: written only if the
    /// entry is its own sound (write_held_wavs).
    held: Option<Vec<u8>>,
}

/// Counts of what the tree holds and what the disc has.
#[derive(Clone, Debug, Default)]
pub struct Summary {
    pub picture_places: usize,
    pub texture_files: usize,
    pub textures: usize,
    /// Distinct pictures by their game bytes alone, names aside.
    pub distinct_pictures: usize,
    /// Picture names with more than one version.
    pub names_with_versions: usize,
    pub pack_places: usize,
    /// Distinct packs by name and bytes (each a model or not converted), and by bytes alone.
    pub distinct_packs: usize,
    pub distinct_pack_contents: usize,
    pub models: usize,
    pub bank_places: usize,
    pub banks: usize,
    pub sounds: usize,
    pub sounds_exported: usize,
    pub sounds_named: usize,
    pub placeholders: usize,
    pub message_places: usize,
    pub texts: usize,
    /// Sequences of the banks, and their places.
    pub music: usize,
    pub music_places: usize,
    /// Game files the tree has no asset for, and why: packs with no model, files the converters
    /// cannot read.
    pub failed: Vec<(Place, String)>,
    /// Game files left out because a member of the same name comes first in their pack.
    pub shadowed: usize,
}

/// Writes a file of the tree, by its path in the tree ("/" between parts).
pub type Write<'w> = dyn Fn(&str, &[u8]) -> Result<()> + Sync + 'w;

/// The disc's texture files, character packs and banks grouped into distinct contents and named:
/// what the tree and the index of ids (ids::Index) share, so both give every asset the same id.
pub(crate) struct Named<'a> {
    pub files: Vec<GameFile<'a>>,
    pub shadowed: usize,
    pub texture_files: usize,
    pictures: Vec<Picture>,
    pub pack_places: usize,
    pub bank_places: usize,
    /// Distinct packs by their bytes alone.
    pub pack_contents: usize,
    pub textures: Vec<Asset>,
    pub packs: Vec<Asset>,
    pub banks: Vec<Asset>,
    /// Message files' assets, their places indexes into files.
    pub texts: Vec<text::TextAsset>,
    pub message_places: usize,
    /// The banks' sequences (.sq members), their places the bank's file and the member's position
    /// in it (sequence_place).
    pub sequences: Vec<Asset>,
    /// Game files that give no asset, by their index in files, and why.
    pub failed: Vec<(usize, String)>,
}

/// The members of a pack a build can reach by name, with their positions.
fn reachable_members(data: &[u8]) -> Vec<(usize, archive::Member)> {
    let members = archive::parse_pack(data).unwrap_or_default();
    let reached = |member: &archive::Member| {
        rebuild::member_named(&members, &member.name, false).map(|found| found.offset) == Some(member.offset)
    };
    members.iter().enumerate().filter(|(_, member)| reached(member)).map(|(at, member)| (at, member.clone())).collect()
}

/// The sequences of the banks among files: (name, hash, bank's file, member position).
fn sequences_of(files: &[GameFile]) -> Vec<(String, String, usize, Option<usize>)> {
    let mut out = Vec::new();
    for (index, file) in files.iter().enumerate().filter(|(_, file)| file.kind == FileKind::Bank) {
        for (position, member) in reachable_members(file.data) {
            // Dark Cloud 1 packs hold empty .sq placeholders
            if member.name.to_lowercase().ends_with(".sq") && member.size > 0 {
                let stem = member.name.rsplit_once('.').map_or(member.name.as_str(), |(stem, _)| stem).to_string();
                out.push((stem, sha256(&[member.data(file.data)]), index, Some(position)));
            }
        }
    }
    out
}

/// A sequence's place: its bank's place and the member at a position of the bank.
pub(crate) fn sequence_place(files: &[GameFile], file: usize, position: Option<usize>) -> Place {
    let mut place = files[file].place.clone();
    let members = archive::parse_pack(files[file].data).unwrap_or_default();
    if let Some(member) = position.and_then(|position| members.get(position)) {
        place.members.push(member.name.clone());
    }
    place
}

pub(crate) fn named<'a>(disc: &[DiscFile<'a>]) -> Named<'a> {
    let Found { files, shadowed } = game_files(disc);
    let surveyed: Vec<Surveyed> = files.par_iter().enumerate().map(|(index, file)| survey(index, file)).collect();
    let mut failed: Vec<(usize, String)> = Vec::new();
    let mut texture_files = 0;
    let mut pictures = Vec::new();
    let mut packs = Vec::new();
    let mut banks = Vec::new();
    let mut messages: Vec<(usize, String)> = Vec::new();
    for (index, found) in surveyed.into_iter().enumerate() {
        let place = &files[index].place;
        match found {
            Surveyed::Pictures(found) => {
                texture_files += 1;
                if found.is_empty() {
                    failed.push((index, "the texture file has no pictures with pixels".to_string()));
                }
                pictures.extend(found);
            }
            Surveyed::Whole(hash) if files[index].kind == FileKind::Pack => {
                // a pack of several characters (Dark Cloud 1's event packs: d02eb02.chr's
                // e07eb1.cfg and e07eb2.cfg) is a model per cfg, each named after its cfg, which
                // model_pack then reads it with
                let cfgs = character_cfgs(files[index].data);
                if cfgs.is_empty() {
                    packs.push((file_name_stem(place), hash, index, None));
                } else {
                    for cfg in cfgs {
                        packs.push((cfg.clone(), format!("{hash}:{cfg}"), index, None));
                    }
                }
            }
            Surveyed::Whole(hash) if files[index].kind == FileKind::Messages => messages.push((index, hash)),
            Surveyed::Whole(hash) => banks.push((file_name_stem(place), hash, index, None)),
            Surveyed::Failed(error) => failed.push((index, error)),
        }
    }
    let (pack_places, bank_places) = (packs.len(), banks.len());
    let pack_contents = packs.iter().map(|pack| &pack.1).collect::<HashSet<_>>().len();
    let mut textures = distinct(pictures.iter().map(|p| (p.name.clone(), p.game_hash.clone(), p.file, Some(p.entry))));
    for texture in &mut textures {
        texture.shown = picture_name(&texture.name);
    }
    name_assets(&mut textures, &files);
    let mut packs = distinct(packs.into_iter());
    name_assets(&mut packs, &files);
    let mut banks = distinct(banks.into_iter());
    name_assets(&mut banks, &files);
    let mut sequences = distinct(sequences_of(&files).into_iter());
    name_assets(&mut sequences, &files);
    let found: Vec<(&Place, &str)> =
        messages.iter().map(|(index, hash)| (&files[*index].place, hash.as_str())).collect();
    let mut texts = text::text_assets(&found);
    for asset in &mut texts {
        asset.places = asset.places.iter().map(|&at| messages[at].0).collect();
    }
    let message_places = messages.len();
    Named {
        files,
        shadowed,
        texture_files,
        pictures,
        pack_places,
        bank_places,
        pack_contents,
        textures,
        packs,
        banks,
        texts,
        message_places,
        sequences,
        failed,
    }
}

/// The id of a bank's entry: the header's name for it, unless an earlier entry took that name, else
/// the bank's id and the entry's number (sound:EV_000_130/4). Says whether it is the name.
pub(crate) fn sound_id(
    bank: &str,
    entry: usize,
    names: &HashMap<usize, String>,
    taken: &mut HashSet<String>,
) -> (String, bool) {
    match names.get(&entry) {
        Some(name) if taken.insert(Kind::Sound.id(name)) => (Kind::Sound.id(name), true),
        _ => (Kind::Sound.id(&format!("{bank}/{entry}")), false),
    }
}

/// What each entry of a bank plays, as a hash: its kind, its sequence's bytes, and each voice's
/// samples, rate, loop, key and base note. Two versions of a bank's name play an entry alike when
/// the hashes agree (the menu packs' copies of snd2/sp/SP_001.snd differ only in their volumes).
/// None when the bank cannot be read.
pub(crate) fn entry_sounds(data: &[u8]) -> Option<Vec<String>> {
    let bank = Bank::from_bytes(data).ok()?;
    let mut out = Vec::new();
    for entry in bank.entries().ok()? {
        let mut parts: Vec<Vec<u8>> = vec![entry.kind.name().as_bytes().to_vec()];
        let sequence = match entry.kind {
            EntryKind::SeSeq { seq } => Some(("mid", seq)),
            EntryKind::Sq { seq } => Some(("sq", seq)),
            _ => None,
        };
        if let Some((ext, at)) = sequence {
            let member = *crate::soundbank::find(&bank.pack, ext).get(at)?;
            parts.push(bank.pack.members[member].data.clone());
        }
        for voice in bank.voices(&entry).ok()? {
            let vag = &bank.hd.vags[voice.vag];
            let base = bank.hd.samples[voice.sample].as_ref().map_or(0, |sample| sample.base_note);
            parts.push(format!("{} {} {} {}", vag.rate, vag.attribute, voice.key, base).into_bytes());
            parts.push(bank.vag_data[voice.vag].clone());
        }
        let refs: Vec<&[u8]> = parts.iter().map(Vec::as_slice).collect();
        out.push(sha256(&refs));
    }
    Some(out)
}

/// The sound ids of the banks' entries and the places each reaches.
#[derive(Default)]
pub(crate) struct BankSounds {
    /// For each bank, in the order given, (entry, sound id) for each of its entries.
    pub ids: Vec<Vec<(usize, String)>>,
    /// Each sound id's places: (bank, entry) by the bank's position in the order given. The first
    /// is the bank whose entry the id names.
    pub places: HashMap<String, Vec<(usize, usize)>>,
    /// The ids that are the header's names.
    pub named: HashSet<String>,
    /// sound:BANK/N for an entry known by another id: a named entry, or an entry of a version of
    /// a bank that plays as the first version's does.
    pub aliases: BTreeMap<String, String>,
}

/// Whether each bank's name has other versions among the banks: only then can its entries be
/// another bank's sounds.
fn versioned(banks: &[Asset]) -> Vec<bool> {
    let mut versions: HashMap<String, usize> = HashMap::new();
    for asset in banks {
        *versions.entry(asset.name.to_uppercase()).or_default() += 1;
    }
    banks.iter().map(|asset| versions[&asset.name.to_uppercase()] > 1).collect()
}

/// The sound id of every entry of the banks (entries[i] the entry numbers of banks[i], None for
/// a bank that cannot be read). An entry is one sound with every place the disc has it: where
/// versions of a bank's name differ (copies in menu packs whose volumes differ), an entry that
/// plays alike in another version is the same sound, so a mod replacing it reaches every copy.
/// The bank with the bare name is taken first, so its entries keep the header's names.
pub(crate) fn bank_sounds(
    banks: &[Asset],
    files: &[GameFile],
    entries: &[Option<Vec<usize>>],
    names: &SoundNames,
) -> BankSounds {
    let shared = versioned(banks);
    let hashes: Vec<Option<Vec<String>>> = banks
        .par_iter()
        .zip(&shared)
        .map(|(asset, &shared)| if shared { entry_sounds(files[asset.first()].data) } else { None })
        .collect();
    let mut order: Vec<usize> = (0..banks.len()).collect();
    order.sort_by_key(|&at| banks[at].id.contains('@'));
    let mut out = BankSounds { ids: vec![Vec::new(); banks.len()], ..BankSounds::default() };
    let mut taken = HashSet::new();
    let mut alike: HashMap<(String, usize, String), String> = HashMap::new();
    for at in order {
        let Some(numbers) = &entries[at] else {
            continue;
        };
        let asset = &banks[at];
        let entry_names = entry_names(asset, names);
        for &entry in numbers {
            let hash = hashes[at].as_ref().and_then(|hashes| hashes.get(entry));
            let key = hash.map(|hash| (asset.name.to_uppercase(), entry, hash.clone()));
            let own = Kind::Sound.id(&format!("{}/{entry}", asset.id));
            let id = match key.as_ref().and_then(|key| alike.get(key)) {
                Some(id) => {
                    out.aliases.insert(own, id.clone());
                    id.clone()
                }
                None => {
                    let (id, is_named) = sound_id(&asset.id, entry, &entry_names, &mut taken);
                    if is_named {
                        out.named.insert(id.clone());
                        out.aliases.insert(own, id.clone());
                    }
                    if let Some(key) = key {
                        alike.insert(key, id.clone());
                    }
                    id
                }
            };
            out.places.entry(id.clone()).or_default().push((at, entry));
            out.ids[at].push((entry, id));
        }
    }
    out
}

/// Writes the asset tree for the disc's files through `write`, the manifest last, and says what
/// it holds.
pub fn build(disc: &[DiscFile], names: &SoundNames, write: &Write) -> Result<Summary> {
    let found = named(disc);
    let files = &found.files;
    let texture_outs = export_textures(&found.textures, files, write)?;
    let texture_ids = texture_lookup(&found.textures, &texture_outs);
    let texture_files: HashMap<String, (String, String)> = found
        .textures
        .iter()
        .zip(&texture_outs)
        .map(|(asset, out)| (Kind::Texture.id(&asset.id), (out.file.clone(), files[asset.first()].place.group())))
        .collect();
    let model_outs: Vec<Result<ModelOut>> = found
        .packs
        .par_iter()
        .map(|asset| export_model(asset, files, &texture_ids, &texture_files, write))
        .collect::<Result<Vec<_>>>()?;
    let mut bank_outs: Vec<Result<Vec<EntryOut>>> = found
        .banks
        .par_iter()
        .zip(versioned(&found.banks))
        .map(|(asset, versioned)| export_bank(asset, files, names, versioned, write))
        .collect::<Result<Vec<_>>>()?;
    let fonts = text::Fonts::from_files(disc.iter().map(|file| (file.path.as_str(), file.data)));
    let text_outs: Vec<Result<(String, String)>> =
        found.texts.par_iter().map(|asset| export_text(asset, files, &fonts, write)).collect::<Result<Vec<_>>>()?;
    let music_outs = export_music(&found.sequences, files, write)?;

    let mut tree = Building::new(&found);
    tree.textures(&found.textures, &found.pictures, texture_outs);
    tree.models(&found.packs, model_outs);
    let bank_entries: Vec<Option<Vec<usize>>> = bank_outs
        .iter()
        .map(|out| out.as_ref().ok().map(|entries| entries.iter().map(|entry| entry.index).collect()))
        .collect();
    let sounds = bank_sounds(&found.banks, files, &bank_entries, names);
    write_held_wavs(&found.banks, &mut bank_outs, &sounds, write)?;
    tree.banks(&found.banks, bank_outs, &sounds);
    tree.texts(&found.texts, text_outs);
    tree.music(&found.sequences, music_outs);
    let (manifest, summary) = tree.finish(sounds.aliases);
    write(MANIFEST, &manifest.text())?;
    Ok(summary)
}

/// The manifest and summary as the tree's assets are added to them, kind by kind.
struct Building<'a> {
    files: &'a [GameFile<'a>],
    assets: BTreeMap<String, manifest::Asset>,
    summary: Summary,
    /// Game files that give no asset, by their index in files, and why.
    failed: Vec<(usize, String)>,
}

impl<'a> Building<'a> {
    fn new(found: &'a Named<'a>) -> Building<'a> {
        let pictures = &found.pictures;
        let mut versions: HashMap<&str, usize> = HashMap::new();
        for asset in &found.textures {
            *versions.entry(asset.name.as_str()).or_default() += 1;
        }
        let summary = Summary {
            shadowed: found.shadowed,
            texture_files: found.texture_files,
            pack_places: found.pack_places,
            bank_places: found.bank_places,
            distinct_pack_contents: found.pack_contents,
            distinct_packs: found.packs.len(),
            picture_places: pictures.len(),
            distinct_pictures: pictures.iter().map(|p| &p.game_hash).collect::<HashSet<_>>().len(),
            names_with_versions: versions.values().filter(|&&n| n > 1).count(),
            message_places: found.message_places,
            ..Summary::default()
        };
        Building { files: &found.files, assets: BTreeMap::new(), summary, failed: found.failed.clone() }
    }

    /// The manifest's place of a game file, with the picture or entry there.
    fn place(&self, file: usize) -> manifest::AssetPlace {
        manifest::AssetPlace::new(&self.files[file].place)
    }

    /// What an asset's export gave, or, when it could not be exported, None with every game
    /// file it is at recorded as not converted, and why.
    fn exported<T>(&mut self, files: impl Iterator<Item = usize>, out: Result<T>) -> Option<T> {
        match out {
            Ok(out) => Some(out),
            Err(error) => {
                self.failed.extend(files.map(|file| (file, error.to_string())));
                None
            }
        }
    }

    fn textures(&mut self, textures: &[Asset], pictures: &[Picture], outs: Vec<TextureOut>) {
        let shapes: HashMap<(usize, usize), (&'static str, usize, usize)> =
            pictures.iter().map(|p| ((p.file, p.entry), (p.format, p.width, p.height))).collect();
        for (asset, out) in textures.iter().zip(outs) {
            let (format, width, height) = shapes[&(asset.first(), asset.places[0].1.unwrap_or(0))];
            let places = asset
                .places
                .iter()
                .map(|&(file, picture)| manifest::AssetPlace { picture, ..self.place(file) })
                .collect();
            let texture = manifest::Asset::Texture {
                name: asset.shown.clone(),
                files: [(out.file, out.hash)].into_iter().collect(),
                game_hash: asset.game_hash.clone(),
                format: format.to_string(),
                width,
                height,
                places,
            };
            self.assets.insert(Kind::Texture.id(&asset.id), texture);
        }
        self.summary.textures = textures.len();
    }

    fn models(&mut self, packs: &[Asset], outs: Vec<Result<ModelOut>>) {
        for (asset, out) in packs.iter().zip(outs) {
            let Some(out) = self.exported(asset.places.iter().map(|&(file, _)| file), out) else {
                continue;
            };
            self.summary.models += 1;
            self.summary.placeholders += out.placeholders.len();
            let model = manifest::Asset::Model {
                name: asset.shown.clone(),
                files: out.files.into_iter().collect(),
                game_hash: asset.game_hash.clone(),
                textures: out.textures,
                placeholders: out.placeholders,
                places: asset.places.iter().map(|&(file, _)| self.place(file)).collect(),
            };
            self.assets.insert(Kind::Model.id(&asset.id), model);
        }
    }

    /// Each bank, and each of its entries that is its own sound (an entry another version of the
    /// bank plays alike is that version's sound).
    fn banks(&mut self, banks: &[Asset], outs: Vec<Result<Vec<EntryOut>>>, sounds: &BankSounds) {
        for (at, (asset, out)) in banks.iter().zip(outs).enumerate() {
            let Some(entries) = self.exported(asset.places.iter().map(|&(file, _)| file), out) else {
                continue;
            };
            self.summary.banks += 1;
            let bank_id = Kind::Bank.id(&asset.id);
            let folder = format!("{SOUNDS}/{}", file_stem(&asset.id));
            let mut sound_ids = Vec::new();
            for (entry, (_, id)) in entries.iter().zip(&sounds.ids[at]) {
                sound_ids.push(id.clone());
                let reached = &sounds.places[id];
                if reached[0].0 != at {
                    continue;
                }
                if sounds.named.contains(id) {
                    self.summary.sounds_named += 1;
                }
                let places = reached
                    .iter()
                    .flat_map(|&(bank, entry)| banks[bank].places.iter().map(move |&(file, _)| (file, entry)))
                    .map(|(file, entry)| manifest::AssetPlace { entry: Some(entry), ..self.place(file) })
                    .collect();
                let (files, game_hash) = match &entry.wav {
                    Some((name, hash, game_hash)) => {
                        self.summary.sounds_exported += 1;
                        (
                            Some([(format!("{folder}/{name}"), hash.clone())].into_iter().collect()),
                            Some(game_hash.clone()),
                        )
                    }
                    None => (None, None),
                };
                let sound = manifest::Asset::Sound {
                    bank: bank_id.clone(),
                    entry: entry.index,
                    plays: entry.plays.to_string(),
                    label: entry.label.clone(),
                    category: entry.category.clone(),
                    files,
                    game_hash,
                    places,
                };
                self.summary.sounds += 1;
                self.assets.insert(id.clone(), sound);
            }
            let bank = manifest::Asset::Bank {
                name: asset.shown.clone(),
                folder,
                game_hash: asset.game_hash.clone(),
                sounds: sound_ids,
                places: asset.places.iter().map(|&(file, _)| self.place(file)).collect(),
            };
            self.assets.insert(bank_id, bank);
        }
    }

    fn texts(&mut self, texts: &[text::TextAsset], outs: Vec<Result<(String, String)>>) {
        for (asset, out) in texts.iter().zip(outs) {
            let Some((file, hash)) = self.exported(asset.places.iter().copied(), out) else {
                continue;
            };
            self.summary.texts += 1;
            let text = manifest::Asset::Text {
                name: asset.name.clone(),
                language: asset.language.short().to_string(),
                files: [(file, hash)].into_iter().collect(),
                game_hash: asset.game_hash.clone(),
                versions: asset.versions,
                places: asset.places.iter().map(|&file| self.place(file)).collect(),
            };
            self.assets.insert(Kind::Text.id(&asset.id), text);
        }
    }

    fn music(&mut self, sequences: &[Asset], outs: Vec<MusicOut>) {
        for (asset, out) in sequences.iter().zip(outs) {
            let Some(out) = self.exported(asset.places.iter().map(|&(file, _)| file), out) else {
                continue;
            };
            self.summary.music += 1;
            self.summary.music_places += asset.places.len();
            let places = asset
                .places
                .iter()
                .map(|&(file, at)| manifest::AssetPlace::new(&sequence_place(self.files, file, at)))
                .collect();
            let music = manifest::Asset::Music {
                name: asset.shown.clone(),
                files: out.into_iter().collect(),
                game_hash: asset.game_hash.clone(),
                places,
            };
            self.assets.insert(Kind::Music.id(&asset.id), music);
        }
    }

    /// The manifest of the assets added, with the aliases and the files not converted, and the
    /// summary.
    fn finish(mut self, aliases: BTreeMap<String, String>) -> (Manifest, Summary) {
        self.failed.sort_by_key(|(file, _)| *file);
        let place = |file: usize| self.files[file].place.clone();
        self.summary.failed = self.failed.iter().map(|(file, error)| (place(*file), error.clone())).collect();
        let unconverted = self
            .failed
            .iter()
            .map(|(file, error)| {
                let place = place(*file);
                manifest::Unconverted { path: place.path, members: place.members, error: error.clone() }
            })
            .collect();
        let counts = manifest::Counts {
            texture: self.summary.textures,
            model: self.summary.models,
            bank: self.summary.banks,
            sound: self.summary.sounds,
            text: self.summary.texts,
            music: self.summary.music,
        };
        let manifest = Manifest { counts, assets: self.assets, aliases, unconverted };
        (manifest, self.summary)
    }
}

/// What a sequence's export gives the manifest: its files and their hashes.
type MusicOut = Result<Vec<(String, String)>>;

/// Every sequence as a MIDI file, from its bank's own file where the disc has one (else the first
/// pack holding it), beside a SoundFont of that bank's programs: music/snd2/bgm/BG_000_000.mid
/// and music/snd2/bgm/BG_000.sf2 for a sequence of snd2/bgm/BG_000.snd (each bank's SoundFont
/// written once). Each gives its files and their hashes, or why it cannot be exported.
fn export_music(sequences: &[Asset], files: &[GameFile], write: &Write) -> Result<Vec<MusicOut>> {
    let home = |asset: &Asset| *asset.places.iter().min_by_key(|(file, _)| files[*file].place.members.len()).unwrap();
    let mut by_bank: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (index, asset) in sequences.iter().enumerate() {
        by_bank.entry(home(asset).0).or_default().push(index);
    }
    let groups: Vec<(usize, Vec<usize>)> = by_bank.into_iter().collect();
    let done: Vec<Vec<(usize, MusicOut)>> = groups
        .par_iter()
        .map(|(file, indices)| -> Result<Vec<(usize, MusicOut)>> {
            let place = files[*file].place.display();
            let base = format!("{MUSIC}/{}", place.rsplit_once('.').map_or(place.as_str(), |(stem, _)| stem));
            let folder = base.rsplit_once('/').map_or(MUSIC, |(folder, _)| folder);
            // a Dark Cloud 1 pack: the bank named as its first sequence, else its first bank
            let data = files[*file].data;
            let font = Bank::from_bytes(data)
                .or_else(|error| {
                    let pairs = Bank::pairs(data)?;
                    let wanted = indices.first().map(|&i| sequences[i].shown.to_lowercase());
                    let pair = pairs
                        .iter()
                        .find(|pair| Some(pair.to_lowercase()) == wanted)
                        .or(pairs.first())
                        .ok_or(error)?;
                    Bank::from_pair(data, pair)
                })
                .and_then(|bank| crate::soundfont::write(&bank, &file_name_stem(&files[*file].place)));
            let font = match font {
                Ok(font) => {
                    let path = format!("{base}{}", crate::soundfont::SUFFIX);
                    write(&path, &font)?;
                    Ok((path, sha256(&[&font])))
                }
                Err(error) => Err(error.to_string()),
            };
            let members = archive::parse_pack(files[*file].data).unwrap_or_default();
            let mut out = Vec::new();
            for &index in indices {
                let asset = &sequences[index];
                let (path, hash) = match &font {
                    Ok(found) => found.clone(),
                    Err(error) => {
                        out.push((index, Err(Error::invalid(error.clone()))));
                        continue;
                    }
                };
                let Some(member) = home(asset).1.and_then(|position| members.get(position)) else {
                    out.push((index, Err(Error::invalid("the bank has no such sequence"))));
                    continue;
                };
                match crate::music::export(member.data(files[*file].data), &asset.shown) {
                    Ok(midi) => {
                        let midi_path = format!("{folder}/{}{}", file_stem(&asset.id), crate::music::SUFFIX);
                        write(&midi_path, &midi)?;
                        out.push((index, Ok(vec![(midi_path, sha256(&[&midi])), (path, hash)])));
                    }
                    Err(error) => out.push((index, Err(error))),
                }
            }
            Ok(out)
        })
        .collect::<Result<Vec<_>>>()?;
    let mut outs: Vec<Option<MusicOut>> = (0..sequences.len()).map(|_| None).collect();
    for (index, out) in done.into_iter().flatten() {
        outs[index] = Some(out);
    }
    Ok(outs.into_iter().map(|out| out.expect("every sequence is exported")).collect())
}

/// A message file's text, from its reference's first place, written to text/<language>/<name>.txt:
/// the file and its hash, or why it cannot be read.
fn export_text(
    asset: &text::TextAsset,
    files: &[GameFile],
    fonts: &text::Fonts,
    write: &Write,
) -> Result<Result<(String, String)>> {
    let file = &files[asset.places[0]];
    let exported = match text::export(file.data, &file.place.display(), &Kind::Text.id(&asset.id), fonts) {
        Ok(exported) => exported,
        Err(error) => return Ok(Err(error)),
    };
    let path = text::text_file(&asset.id);
    write(&path, exported.as_bytes())?;
    Ok(Ok((path.clone(), sha256(&[exported.as_bytes()]))))
}

/// The PNG of every texture, from the first place it is found, written to
/// textures/<group>/<id>.png.
fn export_textures(textures: &[Asset], files: &[GameFile], write: &Write) -> Result<Vec<TextureOut>> {
    // each texture file read once for the textures first found in it
    let mut by_file: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (index, asset) in textures.iter().enumerate() {
        by_file.entry(asset.first()).or_default().push(index);
    }
    let groups: Vec<(usize, Vec<usize>)> = by_file.into_iter().collect();
    let done: Vec<Vec<(usize, TextureOut)>> = groups
        .par_iter()
        .map(|(file, indices)| -> Result<Vec<(usize, TextureOut)>> {
            let images = im3::read(files[*file].data)?;
            let mut out = Vec::new();
            for &index in indices {
                let asset = &textures[index];
                let entry = asset.places[0].1.unwrap_or(0);
                let Some(image) = images.iter().find(|image| image.index == Some(entry)) else {
                    return Err(Error::invalid(format!("{}: no picture {entry}", files[*file].place.display())));
                };
                let png = im3::to_png(image)?;
                let path = format!("{TEXTURES}/{}/{}.png", files[*file].place.group(), file_stem(&asset.id));
                write(&path, &png)?;
                out.push((index, TextureOut { file: path, hash: sha256(&[&png]) }));
            }
            Ok(out)
        })
        .collect::<Result<Vec<_>>>()?;
    let mut outs: Vec<Option<TextureOut>> = (0..textures.len()).map(|_| None).collect();
    for (index, out) in done.into_iter().flatten() {
        outs[index] = Some(out);
    }
    Ok(outs.into_iter().map(|out| out.expect("every texture is exported")).collect())
}

/// Texture ids by (picture name, PNG hash), and by the lowercase name for the first version of a
/// name.
struct TextureIds {
    by_content: HashMap<(String, String), String>,
    by_name: HashMap<String, String>,
}

fn texture_lookup(textures: &[Asset], outs: &[TextureOut]) -> TextureIds {
    let mut by_content = HashMap::new();
    let mut by_name = HashMap::new();
    for (asset, out) in textures.iter().zip(outs) {
        let id = Kind::Texture.id(&asset.id);
        by_content.entry((asset.name.clone(), out.hash.clone())).or_insert_with(|| id.clone());
        by_name.entry(asset.name.to_lowercase()).or_insert(id);
    }
    TextureIds { by_content, by_name }
}

/// The folder of a model in the tree.
fn model_folder(asset: &Asset, files: &[GameFile]) -> String {
    format!("{MODELS}/{}/{}", files[asset.first()].place.group(), file_stem(&asset.id))
}

/// The glTF of a character pack, written to models/<group>/<id>/<id>.gltf with its .bin. Its
/// textures point at the tree's PNGs: the pack's own pictures at their texture ids, a texture the
/// pack does not have at the first version of that name; one the disc has nowhere stays the
/// export's placeholder, written beside the model under textures/.
fn export_model(
    asset: &Asset,
    files: &[GameFile],
    ids: &TextureIds,
    texture_files: &HashMap<String, (String, String)>,
    write: &Write,
) -> Result<Result<ModelOut>> {
    let (data, _) = model_pack(files[asset.first()].data, &asset.name);
    let exported = match models::export(&data).and_then(|exported| Ok((exported, models::unpacked(&data)?.1.stem))) {
        Ok(found) => found,
        Err(error) => return Ok(Err(error)),
    };
    let (exported, stem) = exported;
    let get = |path: &str| exported.iter().find(|(p, _)| p == path).map(|(_, content)| content);
    let (Some(gltf), Some(bin)) = (get(&format!("{stem}.gltf")), get(&format!("{stem}.bin"))) else {
        return Ok(Err(Error::invalid("the export has no glTF")));
    };
    let folder = model_folder(asset, files);
    let id_stem = file_stem(&asset.id);
    let up = "../".repeat(folder.split('/').count());
    let mut json =
        crate::from_gltf::load_json(std::str::from_utf8(gltf).map_err(|_| Error::invalid("the glTF is not UTF-8"))?)?;
    let mut out = ModelOut { files: Vec::new(), textures: Vec::new(), placeholders: Vec::new() };
    if let Some(serde_json::Value::Array(images)) = json.get_mut("images") {
        for image in images {
            let name = image.get("name").and_then(serde_json::Value::as_str).unwrap_or_default().to_string();
            let png = get(&format!("{}/{name}.png", models::TEXTURES))
                .ok_or_else(|| Error::invalid(format!("the export has no texture {name}")))?;
            let found =
                ids.by_content.get(&(name.clone(), sha256(&[png]))).or_else(|| ids.by_name.get(&name.to_lowercase()));
            match found {
                Some(id) => {
                    let (file, _) = &texture_files[id];
                    image["uri"] = serde_json::Value::String(uri_path(&format!("{up}{file}")));
                    if !out.textures.contains(id) {
                        out.textures.push(id.clone());
                    }
                }
                None => {
                    let path = format!("{folder}/{}/{name}.png", models::TEXTURES);
                    write(&path, png)?;
                    out.files.push((path, sha256(&[png])));
                    out.placeholders.push(name);
                }
            }
        }
    }
    if let Some(serde_json::Value::Array(buffers)) = json.get_mut("buffers") {
        if let Some(buffer) = buffers.first_mut() {
            buffer["uri"] = serde_json::Value::String(uri_path(&format!("{id_stem}.bin")));
        }
    }
    let text = serde_json::to_string(&json).expect("a JSON value serializes").into_bytes();
    let gltf_path = format!("{folder}/{id_stem}.gltf");
    let bin_path = format!("{folder}/{id_stem}.bin");
    write(&gltf_path, &text)?;
    write(&bin_path, bin)?;
    out.files.insert(0, (gltf_path, sha256(&[&text])));
    out.files.insert(1, (bin_path, sha256(&[bin])));
    Ok(Ok(out))
}

/// The tree's glTF of a model as the export wrote it: its images at textures/<name>.png and its
/// buffer at <model>.bin, the model's name from the pack's info.cfg. A model built from the tree
/// takes its meshes, skin and pose from the glTF; its textures are assets of their own.
pub fn exported_gltf(tree_gltf: &[u8], pack: &[u8], name: &str) -> Result<(String, Vec<u8>)> {
    let stem = models::unpacked(&model_pack(pack, name).0)?.1.stem;
    let mut json = crate::from_gltf::load_json(
        std::str::from_utf8(tree_gltf).map_err(|_| Error::invalid("the glTF is not UTF-8"))?,
    )?;
    if let Some(serde_json::Value::Array(images)) = json.get_mut("images") {
        for image in images {
            let name = image.get("name").and_then(serde_json::Value::as_str).unwrap_or_default().to_string();
            image["uri"] = serde_json::Value::String(format!("{}/{name}.png", models::TEXTURES));
        }
    }
    if let Some(serde_json::Value::Array(buffers)) = json.get_mut("buffers") {
        if let Some(buffer) = buffers.first_mut() {
            buffer["uri"] = serde_json::Value::String(format!("{stem}.bin"));
        }
    }
    Ok((stem, serde_json::to_string(&json).expect("a JSON value serializes").into_bytes()))
}

const INFO_CFG: &str = "info.cfg";

/// A character pack as the model converters read it. Most monsters' packs give their model in a
/// cfg named after the pack (b04a.cfg in b04a.chr) instead of info.cfg; such a pack is read with
/// that cfg (failing that, its only .cfg) named info.cfg. Also gives, for a renamed pack, the cfg's
/// own name field, which build_model puts back.
pub(crate) fn model_pack<'a>(data: &'a [u8], name: &str) -> (std::borrow::Cow<'a, [u8]>, Option<Vec<u8>>) {
    let unchanged = (std::borrow::Cow::Borrowed(data), None);
    let Some(members) = archive::parse_pack(data) else {
        return unchanged;
    };
    if members.iter().any(|member| member.name.eq_ignore_ascii_case(INFO_CFG)) {
        return unchanged;
    }
    let cfgs: Vec<&archive::Member> =
        members.iter().filter(|member| member.name.to_lowercase().ends_with(".cfg")).collect();
    let own = format!("{name}.cfg");
    // Dark Cloud 1's town and event characters: base.cfg (c04b.chr), else the one cfg naming a
    // MODEL (d02eb02.chr's e07eb1.cfg)
    let names_model = |member: &&&archive::Member| {
        crate::info_cfg::read(member.data(data)).model.as_ref().is_some_and(|model| model.is_set())
    };
    let with_model: Vec<&&archive::Member> = cfgs.iter().filter(names_model).collect();
    // the pack's own cfg first when it names the model (c04bjump.chr's names only motions, and
    // c04bjcam.cfg the model)
    let found = with_model
        .iter()
        .find(|member| member.name.eq_ignore_ascii_case(&own))
        .copied()
        .or_else(|| cfgs.iter().find(|member| member.name.eq_ignore_ascii_case(&own)).filter(|_| with_model.is_empty()))
        .or_else(|| cfgs.iter().find(|member| member.name.eq_ignore_ascii_case("base.cfg")))
        .or(match with_model.as_slice() {
            [only] => Some(*only),
            _ => None,
        })
        .or(match cfgs.as_slice() {
            [only] => Some(only),
            _ => None,
        });
    let Some(cfg) = found else {
        return unchanged;
    };
    let mut renamed = data.to_vec();
    let old = rename_in_place(&mut renamed, cfg, INFO_CFG.as_bytes());
    (std::borrow::Cow::Owned(renamed), Some(old))
}

/// The stems of a pack's cfgs that each name a MODEL, when the pack holds more than one and
/// model_pack would not read it whole (no info.cfg or base.cfg); else none.
fn character_cfgs(data: &[u8]) -> Vec<String> {
    let Some(members) = archive::parse_pack(data) else {
        return Vec::new();
    };
    let is = |member: &archive::Member, name: &str| member.name.eq_ignore_ascii_case(name);
    if members.iter().any(|member| is(member, INFO_CFG) || is(member, "base.cfg")) {
        return Vec::new();
    }
    let named: Vec<String> = members
        .iter()
        .filter(|member| member.name.to_lowercase().ends_with(".cfg"))
        .filter(|member| crate::info_cfg::read(member.data(data)).model.as_ref().is_some_and(|model| model.is_set()))
        .map(|member| member.name[..member.name.len() - 4].to_string())
        .collect();
    if named.len() < 2 {
        return Vec::new();
    }
    named
}

/// Writes a member's name field in place, the rest of the pack as it is, and gives the field's old
/// bytes. archive::Pack, read and written whole, would write the members' padding as zeros, and a
/// template pack must keep every byte for the build to give the disc's.
fn rename_in_place(pack: &mut [u8], member: &archive::Member, name: &[u8]) -> Vec<u8> {
    let field = member.offset - archive::PACK_SLOT;
    let old = pack[field..field + archive::PACK_NAME].to_vec();
    pack[field..field + archive::PACK_NAME].fill(0);
    pack[field..field + name.len()].copy_from_slice(name);
    old
}

/// models::build for a character pack read as model_pack reads it.
pub fn build_model(folder: &std::path::Path, data: &[u8], name: &str) -> Result<Vec<u8>> {
    let (pack, old) = model_pack(data, name);
    let mut built = match moved_vertices(folder, &pack)? {
        Some(built) => built,
        None => models::build(folder, &pack)?,
    };
    if let Some(old) = old {
        let members = archive::parse_pack(&built).ok_or_else(|| Error::invalid("the built pack is not a pack"))?;
        let cfg = members
            .iter()
            .find(|member| member.name == INFO_CFG)
            .ok_or_else(|| Error::invalid("the built pack has no info.cfg"))?;
        rename_in_place(&mut built, cfg, &old);
    }
    Ok(built)
}

/// A character pack whose glTF differs from the export only in vertex positions, with them written
/// into its model's own bytes (vertex_patch), so the pack keeps its size; None otherwise.
fn moved_vertices(folder: &std::path::Path, pack: &[u8]) -> Result<Option<Vec<u8>>> {
    let (_, files) = models::unpacked(pack)?;
    // the export's name, else the folder's one glTF (an asset tree names it after the asset)
    let mut path = folder.join(format!("{}.gltf", files.stem));
    if !path.is_file() {
        let gltfs: Vec<std::path::PathBuf> = std::fs::read_dir(folder)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("gltf")))
            .collect();
        let [only] = gltfs.as_slice() else {
            return Ok(None);
        };
        path = only.clone();
    }
    let Ok(raw) = std::fs::read(&path) else {
        return Ok(None);
    };
    let given = crate::from_gltf::load_gltf(&raw, &|uri| {
        std::fs::read(folder.join(uri)).map_err(|e| Error::invalid(format!("cannot read {uri}: {e}")))
    })?;
    let exported = models::export(pack)?;
    let get = |name: &str| exported.iter().find(|(n, _)| n == name).map(|(_, d)| d.clone());
    let Some(gltf) = get(&format!("{}.gltf", files.stem)) else {
        return Ok(None);
    };
    let exported_gltf =
        crate::from_gltf::load_gltf(&gltf, &|uri| get(uri).ok_or_else(|| Error::invalid(uri.to_string())))?;
    let member = [files.model.clone()];
    let Some(model) = dc2tk_disc::rebuild::read_member(pack, &member) else {
        return Ok(None);
    };
    match crate::vertex_patch::patch(model, &exported_gltf, &given)? {
        Some(patched) if patched != model => Ok(Some(dc2tk_disc::rebuild::replaced_member(pack, &member, &patched)?)),
        _ => Ok(None),
    }
}

/// A pack model_pack renamed, with its info.cfg given back the name field it had (`old`).
pub(crate) fn restore_cfg_name(mut built: Vec<u8>, old: &[u8]) -> Result<Vec<u8>> {
    let members = archive::parse_pack(&built).ok_or_else(|| Error::invalid("the built pack is not a pack"))?;
    let cfg = members
        .iter()
        .find(|member| member.name == INFO_CFG)
        .ok_or_else(|| Error::invalid("the built pack has no info.cfg"))?
        .clone();
    rename_in_place(&mut built, &cfg, old);
    Ok(built)
}

/// models::export for a character pack read as model_pack reads it: the export's glTF, buffer and
/// textures/, which build_model builds back.
pub fn exported_model(data: &[u8], name: &str) -> Result<Vec<(String, Vec<u8>)>> {
    models::export(&model_pack(data, name).0)
}

/// The stem of the model's files in a character pack (c02b for c02fuku1.chr), as the export names
/// its glTF.
pub fn model_stem(data: &[u8], name: &str) -> Result<String> {
    Ok(models::unpacked(&model_pack(data, name).0)?.1.stem)
}

/// A character pack built from a glTF and its buffer anywhere (the tree's, or one a tool wrote
/// from it): they are written into `folder` as the export names them, and built with the pack as
/// the template. The glTF's images are not read: a model's textures change through their own ids.
pub fn build_model_from(gltf: &[u8], bin: &[u8], data: &[u8], name: &str, folder: &std::path::Path) -> Result<Vec<u8>> {
    let (stem, exported) = exported_gltf(gltf, data, name)?;
    let _ = std::fs::remove_dir_all(folder);
    files::create_dir_all(folder)?;
    files::write(folder.join(format!("{stem}.gltf")), &exported)?;
    files::write(folder.join(format!("{stem}.bin")), bin)?;
    let built = build_model(folder, data, name);
    let _ = std::fs::remove_dir_all(folder);
    built
}

/// The bytes of a glTF file's buffer: the file its first buffer's uri names, beside the glTF.
pub fn gltf_buffer(path: &std::path::Path, gltf: &[u8]) -> Result<Vec<u8>> {
    let json =
        crate::from_gltf::load_json(std::str::from_utf8(gltf).map_err(|_| Error::invalid("the glTF is not UTF-8"))?)?;
    let buffers = json.get("buffers").and_then(serde_json::Value::as_array).map(Vec::as_slice).unwrap_or_default();
    let [buffer] = buffers else {
        return Err(Error::invalid(format!("{}: a model's glTF has one buffer", path.display())));
    };
    let uri = buffer.get("uri").and_then(serde_json::Value::as_str).unwrap_or_default();
    if uri.is_empty() || uri.starts_with("data:") {
        return Err(Error::invalid(format!(
            "{}: the glTF's buffer must be a .bin file beside it (glTF Separate)",
            path.display()
        )));
    }
    let file = path.parent().unwrap_or(std::path::Path::new(".")).join(uri_decoded(uri));
    std::fs::read(&file).with_context(|| file.display().to_string())
}

/// A uri's path with its %XX escapes decoded.
pub(crate) fn uri_decoded(uri: &str) -> String {
    let bytes = uri.as_bytes();
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let hex =
            bytes.get(at + 1..at + 3).and_then(|pair| u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok());
        match (bytes[at], hex) {
            (b'%', Some(byte)) => {
                out.push(byte);
                at += 3;
            }
            (byte, _) => {
                out.push(byte);
                at += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The header's names for a bank's entries, for the first version of the bank's name only.
pub(crate) fn entry_names(asset: &Asset, names: &SoundNames) -> HashMap<usize, String> {
    let found = names.iter().find(|(bank, _)| bank.eq_ignore_ascii_case(&asset.id));
    found.map(|(_, entries)| entries.iter().cloned().collect()).unwrap_or_default()
}

/// The WAV file name of an entry: its number, and the header's name for it without SE_ and the
/// bank's name (004_GRIFFON_BREAK_WINDOW.wav).
pub fn wav_name(index: usize, name: Option<&str>, bank: &str) -> String {
    let Some(name) = name else {
        return format!("{index:03}.wav");
    };
    let short = name.strip_prefix("SE_").unwrap_or(name);
    let prefix = format!("{}_", bank.to_uppercase());
    let short = short.strip_prefix(prefix.as_str()).filter(|rest| !rest.is_empty()).unwrap_or(short);
    format!("{index:03}_{}.wav", file_stem(short))
}

/// Every entry of a bank, with a WAV in sounds/<bank>/ for each that plays a sample (as
/// sounds::export picks them). Of a bank with other versions (`versioned`) the WAVs are held for
/// write_held_wavs, as an entry may be an earlier version's sound.
fn export_bank(
    asset: &Asset,
    files: &[GameFile],
    names: &SoundNames,
    versioned: bool,
    write: &Write,
) -> Result<Result<Vec<EntryOut>>> {
    let entry_names = entry_names(asset, names);
    let read = || -> Result<Vec<EntryOut>> {
        let data = files[asset.first()].data;
        // a Dark Cloud 1 pack: every sample of its banks an entry (sound_dc1::samples)
        if let Some(samples) = crate::sound_dc1::samples(data) {
            let mut banks: HashMap<String, Bank> = HashMap::new();
            let mut out = Vec::new();
            for (index, (stem, vag)) in samples.into_iter().enumerate() {
                if !banks.contains_key(&stem) {
                    banks.insert(stem.clone(), Bank::from_pair(data, &stem)?);
                }
                let bank = &banks[&stem];
                let bytes = soundbank::wav_bytes(&bank.decode_vag(vag)?)?;
                let name = wav_name(index, entry_names.get(&index).map(String::as_str), &asset.id);
                let wav = Some((name, sha256(&[&bytes]), sha256(&[&bank.vag_data[vag]])));
                let label = format!("{stem}/{vag}");
                out.push(EntryOut { index, plays: "sample", label, category: String::new(), wav, held: Some(bytes) });
            }
            return Ok(out);
        }
        let bank = Bank::from_bytes(data)?;
        let mut out = Vec::new();
        for entry in bank.entries()? {
            let voices = if entry.kind.is_key_on() { bank.voices(&entry)? } else { Vec::new() };
            let plays = match entry.kind {
                EntryKind::KeyOn { .. } if !voices.is_empty() => "sample",
                EntryKind::Sq { .. } => "sq",
                EntryKind::SeSeq { .. } => "seseq",
                _ => "nothing",
            };
            let mut held = None;
            let mut wav = None;
            if plays == "sample" {
                let vag = voices[0].vag;
                let bytes = soundbank::wav_bytes(&bank.decode_vag(vag)?)?;
                let name = wav_name(entry.index, entry_names.get(&entry.index).map(String::as_str), &asset.id);
                wav = Some((name, sha256(&[&bytes]), sha256(&[&bank.vag_data[vag]])));
                held = Some(bytes);
            }
            let label = entry.name.clone();
            let category = entry.category.clone();
            out.push(EntryOut { index: entry.index, plays, label, category, wav, held });
        }
        Ok(out)
    };
    let mut entries = match read() {
        Ok(entries) => entries,
        Err(error) => return Ok(Err(error)),
    };
    if !versioned {
        let folder = format!("{SOUNDS}/{}", file_stem(&asset.id));
        for entry in &mut entries {
            if let (Some((name, _, _)), Some(bytes)) = (&entry.wav, entry.held.take()) {
                write(&format!("{folder}/{name}"), &bytes)?;
            }
        }
    }
    Ok(Ok(entries))
}

/// Writes the WAVs export_bank held, each only when its entry is its own sound: an entry that
/// plays as an earlier version's does is that entry's sound, whose WAV is in that version's folder,
/// and the manifest lists no other.
fn write_held_wavs(
    banks: &[Asset],
    outs: &mut [Result<Vec<EntryOut>>],
    sounds: &BankSounds,
    write: &Write,
) -> Result<()> {
    for (at, (asset, out)) in banks.iter().zip(outs.iter_mut()).enumerate() {
        let Ok(entries) = out else {
            continue;
        };
        let folder = format!("{SOUNDS}/{}", file_stem(&asset.id));
        for (entry, (_, id)) in entries.iter_mut().zip(&sounds.ids[at]) {
            if let (Some((name, _, _)), Some(bytes)) = (&entry.wav, entry.held.take()) {
                if sounds.places[id][0].0 == at {
                    write(&format!("{folder}/{name}"), &bytes)?;
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::im3::{Format, Image};
    use crate::mds::{self, Material, Mds, Mdt, Node, Prim, PRIM_TRIANGLES};
    use serde_json::{json, Value};
    use std::sync::Mutex;

    pub fn pack(members: &[(&str, &[u8])]) -> Vec<u8> {
        archive::write_pack(members).unwrap()
    }

    pub fn picture(name: &str, colour: [u8; 4]) -> Image {
        let mut image = Image::new(name, Format::Ct32, 8, 8);
        image.pixels = Some(colour.repeat(64));
        image
    }

    /// A character pack with one mesh using picture tex01 (its own) and tex02 (found elsewhere).
    pub fn character(colour: [u8; 4]) -> Vec<u8> {
        character_with("info.cfg", colour)
    }

    pub fn character_with(cfg: &str, colour: [u8; 4]) -> Vec<u8> {
        let mut model = Mds::new();
        model.nodes.push(Node::new("root", -1));
        let corners = [[0, 0, 0], [1, 0, 1], [2, 0, 2], [0, 0, 0], [2, 0, 2], [3, 0, 3]];
        let mesh = Mdt {
            vertices: vec![[0.0, 0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0], [1.0, 1.0, 0.0, 1.0], [0.0, 1.0, 0.0, 1.0]],
            normals: vec![[0.0, 0.0, 1.0, 1.0]],
            uvs: vec![[0.0, 0.0, 1.0, 0.0], [1.0, 0.0, 1.0, 0.0], [1.0, 1.0, 1.0, 0.0], [0.0, 1.0, 1.0, 0.0]],
            materials: vec![Material::new("tex01"), Material::new("tex02"), Material::new("nowhere")],
            prims: (0..3)
                .map(|material| Prim {
                    kind: PRIM_TRIANGLES,
                    material,
                    verts: corners.iter().map(|c| c.to_vec()).collect(),
                })
                .collect(),
            ..Mdt::default()
        };
        model.nodes.push(Node { mdt: Some(mesh), ..Node::new("body", 0) });
        let img = im3::write(&[picture("tex01", colour)], None).unwrap();
        pack(&[(cfg, b"v2;\nMODEL \"c.mds\";\n"), ("c.mds", &mds::build(&model).unwrap()), ("c.img", &img)])
    }

    /// A small disc: a texture file in two places, a redrawn version of one of its pictures, and a
    /// character pack in two places and a second version of it.
    pub fn disc() -> Vec<(String, Vec<u8>)> {
        let shared = im3::write(&[picture("tex02", [1, 2, 3, 0x80]), picture("wall", [9, 9, 9, 0x80])], None).unwrap();
        let redrawn = im3::write(&[picture("wall", [200, 0, 0, 0x40])], None).unwrap();
        let names = text::tests::sample(&[(0, &text::tests::ascii("Pot")), (1, &text::tests::ascii("Pan"))]);
        let other_names = text::tests::sample(&[(0, &text::tests::ascii("Pot"))]);
        vec![
            ("chara/c01.chr".to_string(), character([10, 20, 30, 0x80])),
            ("map/a/a01.ipk".to_string(), pack(&[("a01.img", &shared), ("c01.chr", &character([10, 20, 30, 0x80]))])),
            ("map/b/b02.ipk".to_string(), pack(&[("b02.img", &redrawn), ("x.img", &shared)])),
            ("menu/c01.chr".to_string(), character([99, 20, 30, 0x80])),
            // a monster's pack: its cfg named after it, not info.cfg
            ("monster/m01.chr".to_string(), character_with("M01.cfg", [10, 20, 30, 0x80])),
            ("monster/motions.chr".to_string(), pack(&[("info.cfg", b"v2;\n"), ("a.mot", b"")])),
            // a message file in two menu packs, in two versions, and an old copy of the first
            ("menu/1/a.pac".to_string(), pack(&[("names.mes", &names)])),
            ("menu/1/b.pac".to_string(), pack(&[("names.mes", &other_names)])),
            ("meswin/usa/names.mes".to_string(), names.clone()),
        ]
    }

    pub fn build_disc(disc: &[(String, Vec<u8>)]) -> (Summary, BTreeMap<String, Vec<u8>>) {
        let files: Vec<DiscFile> = disc.iter().map(|(path, data)| DiscFile { path: path.clone(), data }).collect();
        let written = Mutex::new(BTreeMap::new());
        let write = |path: &str, data: &[u8]| -> Result<()> {
            written.lock().unwrap().insert(path.to_string(), data.to_vec());
            Ok(())
        };
        let names = sound_names("");
        let summary = build(&files, &names, &write).unwrap();
        (summary, written.into_inner().unwrap())
    }

    #[test]
    fn a_name_keeps_its_first_content_and_later_versions_say_where_they_are_found() {
        let items = [("wall", "a01"), ("wall", "b02"), ("WALL", "b02"), ("floor", "a01"), ("wall", "b02")];
        let items: Vec<(String, String)> = items.iter().map(|(n, l)| (n.to_string(), l.to_string())).collect();
        assert_eq!(assign_names(&items), ["wall", "wall@b02", "WALL@b02.2", "floor", "wall@b02.3"]);
    }

    #[test]
    fn file_names_hold_any_name() {
        assert_eq!(file_stem("a/b:c"), "a_b_c");
        assert_eq!(file_stem("con"), "_con");
        assert_eq!(file_stem("com1.x"), "_com1.x");
        assert_eq!(file_stem("dot."), "dot_");
        assert_eq!(file_stem(""), "_");
        assert_eq!(uri_path("../textures/a b@c.png"), "../textures/a%20b@c.png");
    }

    #[test]
    fn places_give_their_group_and_label() {
        let member = Place { path: "map/d/d05/d05f01.ipk".into(), members: vec!["a.img".into()] };
        assert_eq!((member.group(), member.label()), ("map/d/d05".to_string(), "d05f01".to_string()));
        assert_eq!(member.display(), "map/d/d05/d05f01.ipk.d/a.img");
        assert_eq!(picture_name("\u{90}X\u{82}\u{cc}\u{94}\u{fc}\u{8f}\u{97}"), "森の美女");
        assert_eq!(picture_name("c02a01"), "c02a01");
        let file = Place { path: "menu/1/npcface/14.img".into(), members: vec![] };
        assert_eq!((file.group(), file.label()), ("menu/1/npcface".to_string(), "menu-1-npcface".to_string()));
        let root = Place { path: "x.img".into(), members: vec![] };
        assert_eq!((root.group(), root.label()), ("root".to_string(), "root".to_string()));
    }

    #[test]
    fn sound_names_come_from_the_header_enums() {
        let header = "\
// System sounds: entries of the system bank snd2/SY_000 (named in its cfg).
enum system_se_pal {
    SE_CURSOR_MOVE = 0,
    SE_MENU_CONFIRM = 1,
};

// Base sounds: every map's base bank has the layout of snd2/BS_999, the full base set.
enum base_se_pal {
    SE_BASE_STEP_GRASS_L = 2,
};
enum se_ev_000_130_pal {
    SE_EV_000_130_GRIFFON_BREAK_WINDOW = 4,  // a note
};
enum item_id {
    ITEM_SWORD = 1,
};
";
        let names = sound_names(header);
        assert_eq!(names.keys().collect::<Vec<_>>(), ["BS_999", "EV_000_130", "SY_000"]);
        assert_eq!(names["SY_000"], [(0, "SE_CURSOR_MOVE".to_string()), (1, "SE_MENU_CONFIRM".to_string())]);
        assert_eq!(names["EV_000_130"], [(4, "SE_EV_000_130_GRIFFON_BREAK_WINDOW".to_string())]);
        assert_eq!(
            wav_name(4, Some("SE_EV_000_130_GRIFFON_BREAK_WINDOW"), "EV_000_130"),
            "004_GRIFFON_BREAK_WINDOW.wav"
        );
        assert_eq!(wav_name(0, Some("SE_CURSOR_MOVE"), "SY_000"), "000_CURSOR_MOVE.wav");
        assert_eq!(wav_name(12, None, "SY_000"), "012.wav");
    }

    #[test]
    fn each_content_is_written_once_with_every_place_listed() {
        let (summary, written) = build_disc(&disc());
        let manifest: Value = serde_json::from_slice(&written[MANIFEST]).unwrap();
        let assets = manifest["assets"].as_object().unwrap();
        let ids: Vec<&str> = assets.keys().map(String::as_str).collect();
        assert_eq!(
            ids,
            [
                "model:c01",
                "model:c01@menu",
                "model:m01",
                "text:en/names",
                "text:en/names@meswin-usa",
                "texture:tex01",
                "texture:tex01@c01",
                "texture:tex02",
                "texture:wall",
                "texture:wall@b02"
            ]
        );
        // a message file's id covers both versions in the menu packs, the first exported
        assert_eq!(assets["text:en/names"]["places"].as_array().unwrap().len(), 2);
        assert_eq!(assets["text:en/names"]["versions"], 2);
        assert_eq!(assets["text:en/names"]["language"], "en");
        let names = std::str::from_utf8(&written["text/en/names.txt"]).unwrap();
        assert!(names.ends_with("@0\nPot\n@1\nPan\n"), "{names}");
        assert_eq!((summary.message_places, summary.texts), (3, 2));
        // tex02 and wall: in a01.ipk and in b02.ipk; tex01: in both c01 packs on chara and map/a
        assert_eq!(assets["texture:tex02"]["places"].as_array().unwrap().len(), 2);
        assert_eq!(
            assets["texture:wall"]["places"][1],
            json!({"path": "map/b/b02.ipk", "members": ["x.img"], "picture": 1})
        );
        assert_eq!(assets["texture:tex01"]["places"].as_array().unwrap().len(), 3);
        assert_eq!(assets["model:c01"]["places"][1], json!({"path": "map/a/a01.ipk", "members": ["c01.chr"]}));
        assert!(written.contains_key("textures/map/b/wall@b02.png"));
        assert!(written.contains_key("textures/chara/tex01.png"));
        assert_eq!((summary.picture_places, summary.textures, summary.names_with_versions), (9, 5, 2));
        assert_eq!((summary.pack_places, summary.distinct_packs, summary.models), (5, 4, 3));
        // the pack with no model is listed as not converted
        assert_eq!(manifest["unconverted"][0]["path"], "monster/motions.chr");

        // the model's textures point into textures/: its own picture, the one found elsewhere, and
        // a placeholder for the one the disc has nowhere
        let model = &assets["model:c01"];
        assert_eq!(model["textures"], json!(["texture:tex01", "texture:tex02"]));
        assert_eq!(model["placeholders"], json!(["nowhere"]));
        let gltf =
            crate::from_gltf::load_json(std::str::from_utf8(&written["models/chara/c01/c01.gltf"]).unwrap()).unwrap();
        let uris: Vec<&str> = gltf
            .get("images")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i.get("uri").unwrap().as_str().unwrap())
            .collect();
        assert_eq!(
            uris,
            ["../../../textures/chara/tex01.png", "../../../textures/map/a/tex02.png", "textures/nowhere.png"]
        );
        assert!(written.contains_key("models/chara/c01/textures/nowhere.png"));
        assert_eq!(gltf.get("buffers").unwrap().as_array().unwrap()[0].get("uri").unwrap().as_str(), Some("c01.bin"));
        let menu =
            crate::from_gltf::load_json(std::str::from_utf8(&written["models/menu/c01@menu/c01@menu.gltf"]).unwrap())
                .unwrap();
        assert_eq!(
            menu.get("buffers").unwrap().as_array().unwrap()[0].get("uri").unwrap().as_str(),
            Some("c01@menu.bin")
        );

        // the file hashes the manifest gives are the files'
        for asset in assets.values() {
            for (path, hash) in asset["files"].as_object().into_iter().flatten() {
                assert_eq!(sha256(&[&written[path]]), hash.as_str().unwrap(), "{path}");
            }
        }
    }

    #[test]
    fn the_same_disc_gives_the_same_tree() {
        assert_eq!(build_disc(&disc()).1, build_disc(&disc()).1);
    }

    #[test]
    fn the_tree_gltf_goes_back_to_the_export() {
        let disc = disc();
        let (_, written) = build_disc(&disc);
        let (stem, gltf) = exported_gltf(&written["models/chara/c01/c01.gltf"], &disc[0].1, "c01").unwrap();
        let exported = models::export(&disc[0].1).unwrap();
        assert_eq!(stem, "c");
        assert_eq!(gltf, exported.iter().find(|(p, _)| p == "c.gltf").unwrap().1);
    }
}

//! Texture blocks in GS memory: what entering picture files takes of them.

use super::*;

/// GS blocks (256 bytes) a texture block may fill: every block starts at vram_top, 0x2D20
/// (GetVramTopAddress, mainloop.cpp:101: the 512x480 32-bit frame buffers and Z buffer of mgInit(2,3)
/// end at 0x2D00), and ends at vram_fix, 0x3FE0 (mgCTextureManager::Initialize) less the 32 blocks
/// of the photo work texture LoadTakePhoto keeps fixed at the top (photo.cpp:61).
pub const BLOCK_VRAM: i64 = 0x3FC0 - 0x2D20;
/// Unswizzled 8-bit pictures may be put in the Z buffer instead (CheckCopyToZBufVram, GetZBufVram:
/// frame width x width x 4 / 256), at four times their size, while it has room.
pub const BLOCK_ZBUF: i64 = 512 * 512 * 4 / 256;
pub(super) const CLUT_BLOCKS: i64 = 4;
pub(super) const PAGE_BLOCKS: i64 = 32;
/// Texture blocks of the town and dungeon texture managers (SetTableBuffer: 0xDD in EditInit,
/// 0xAF in memoryInit).
pub const TOWN_BLOCKS: i64 = 0xDD;
pub const DUNGEON_BLOCKS: i64 = 0xAF;
/// The town's texture blocks (the MapJumpMapInfo records EditInit sets): the main map and its add
/// map from 0, the interior (sub map, loaded beside the main map) from 0x18, the main map's
/// effects at 0x40 and the interior's at 0x41, the party at 0x46, villagers from 0x4E, the sky at
/// 0xA9 to 0xAC. So a main map has 0x18 blocks when an interior loads beside it, 0x40 when none
/// does; an interior 0x28.
pub const TOWN_INTERIOR_BLOCK: i64 = 0x18;
pub const TOWN_EFFECT_BLOCK: i64 = 0x40;
/// A dungeon map's textures (and its add map's) from block 0, its effects at 0xF
/// (LoadDungeonMapFile), the party's textures at 0x10 (SetupMainUnit image_block), the sky at
/// 0x4C to 0x4F.
pub const DUNGEON_EFFECT_BLOCK: i64 = 0xF;

pub(super) struct Texture {
    pub(super) vram: i64,
    zbuf: Option<i64>,
    clut: bool,
}

/// mgCTextureManager as far as GetRemainVRAM sees it: the textures of each block, in order.
pub(super) struct Blocks {
    pub(super) blocks: HashMap<i64, Vec<Texture>>,
    pub(super) spills: usize,
}

impl Blocks {
    pub(super) fn new() -> Blocks {
        Blocks { blocks: HashMap::new(), spills: 0 }
    }

    pub(super) fn remain(&self, block: i64) -> i64 {
        let mut vram = 0;
        let mut zbuf = BLOCK_ZBUF;
        for texture in self.blocks.get(&block).into_iter().flatten() {
            match texture.zbuf {
                Some(size) if size <= zbuf => zbuf -= size,
                _ => vram += texture.vram,
            }
            if texture.clut {
                vram += CLUT_BLOCKS;
            }
        }
        BLOCK_VRAM - vram
    }

    pub(super) fn used(&self) -> i64 {
        self.blocks.iter().filter(|(_, list)| !list.is_empty()).map(|(&block, _)| block + 1).max().unwrap_or(0)
    }
}

/// A TIM2 picture's fields EnterTexture reads, from the start of its file: the picture header
/// follows the 0x10-byte file header.
const TIM2_MIPMAPS: usize = 0x21;
const TIM2_IMAGE_TYPE: usize = 0x23;
const TIM2_WIDTH: usize = 0x24;
const TIM2_HEIGHT: usize = 0x26;

/// A picture file's header (mgIMG_FILE_HEADER, 0x10 bytes): the entries of an IMG or IM2 file
/// (num) and of an IM3 (num3); its entries follow it.
const IMG_COUNT: usize = 4;
const IM3_COUNT: usize = 8;
const IMG_ENTRIES: usize = 0x10;
/// An IMG or IM2 entry (mgIMG1_HEADER, 0x30 bytes): a name of 0x20 bytes, then the picture's
/// offset.
const IMG_ENTRY: usize = 0x30;
const IMG_OFFSET: usize = 0x20;
/// An IM3 entry (mgIMG_HEADER, 0x40 bytes): a name of 0x20 bytes, then the picture's offset, its
/// swizzle, its block (the group) and no_image.
const IM3_ENTRY: usize = 0x40;
const IM3_OFFSET: usize = 0x24;
const IM3_SWIZZLED: usize = 0x28;
const IM3_BLOCK: usize = 0x2C;
const IM3_NO_IMAGE: usize = 0x30;
const NAME: usize = 0x20;

/// EnterTexture for one TIM2 picture (mg_texture.cpp:482, 683): its size in GS memory, rounded up
/// to whole pages, and whether it may go in the Z buffer. None when the engine refuses it.
pub(super) fn texture(img: &[u8], at: usize, swizzled: bool, no_image: bool) -> Option<Texture> {
    let mips = *img.get(at + TIM2_MIPMAPS)? as usize;
    let bpp = im3::Format::from_type(*img.get(at + TIM2_IMAGE_TYPE)?)?.bpp() as i64;
    let u16_at =
        |field: usize| img.get(at + field..at + field + 2).map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]));
    let width = u16_at(TIM2_WIDTH)? as i64;
    let height = u16_at(TIM2_HEIGHT)? as i64;
    let mut images = [false; 4];
    if !no_image {
        for level in images.iter_mut().take(mips.max(1)) {
            *level = true;
        }
    }
    // Conv32To8 unswizzles a one-level 8-bit picture of up to 0x10000 pixels
    let swizzled = swizzled && !(bpp == 8 && mips == 1 && width * height <= 0x10000);
    let mut level = (if bpp == 24 { 32 } else { bpp }) * width * height / 256 / 8;
    let last = images.iter().position(|&present| !present).map_or(3, |first| first as i64 - 1);
    let mut blocks = 0;
    for present in images.iter().take((last + 1).max(0) as usize) {
        if *present {
            blocks += level;
        }
        level /= 4;
    }
    if !images[0] {
        blocks = level;
    }
    let vram = round_up(blocks as u64, PAGE_BLOCKS as u64) as i64;
    Some(Texture { vram, zbuf: (!swizzled && bpp == 8 && images[0]).then_some(vram * 4), clut: bpp <= 8 })
}

/// mgCTextureManager::EnterIMGFile (mg_texture.cpp:753) of a picture file at `block`: an IM3's
/// pictures sorted by group as the engine sorts them, each entered at block + group + the blocks
/// spilled so far, moving on to the next block when a block runs out ("texture over"); IMG and
/// IM2 files go into one running block. Returns what the engine returns,
/// `last_block_used - current_block`, which CMap::LoadData adds to the next file's block:
/// - for an IM3, the blocks past `block` it used (`current_block` stays `block`);
/// - for an IMG or IM2, always 0. A spill moves `current_block` on and sets the picture's block
///   to it, and `last_block_used` follows the picture's block, so the two stay equal. The next
///   file then starts one block on from this one's start, even when this one spilled past it.
pub(super) fn enter_img(blocks: &mut Blocks, img: &[u8], block: i64, block_max: i64) -> i64 {
    match img.get(..3) {
        Some(b"IM3") => enter_im3(blocks, img, block, block_max),
        Some(magic) if magic.starts_with(b"IM") => {
            enter_img1(blocks, img, block, block_max, magic == b"IM2");
            // last_block_used - current_block, which are equal here (see above)
            0
        }
        _ => 0,
    }
}

/// An entry of an IM3 file.
struct Im3Entry {
    name: Vec<u8>,
    offset: usize,
    swizzled: bool,
    /// The texture block it goes into, from the one the file is entered at.
    group: i64,
    no_image: bool,
}

fn im3_entries(img: &[u8]) -> Vec<Im3Entry> {
    let count = u32_at(img, IM3_COUNT).unwrap_or(0) as usize;
    let mut entries = Vec::new();
    for index in 0..count {
        let at = IMG_ENTRIES + index * IM3_ENTRY;
        let Some(raw) = img.get(at..at + NAME) else { break };
        entries.push(Im3Entry {
            name: raw[..raw.iter().position(|&b| b == 0).unwrap_or(NAME)].to_vec(),
            offset: i32_at(img, at + IM3_OFFSET).unwrap_or(0).max(0) as usize,
            swizzled: i32_at(img, at + IM3_SWIZZLED).unwrap_or(0) != 0,
            group: i32_at(img, at + IM3_BLOCK).unwrap_or(0) as i64,
            no_image: img.get(at + IM3_NO_IMAGE..at + IM3_NO_IMAGE + 2).is_some_and(|bytes| bytes != [0, 0]),
        });
    }
    entries
}

/// An IM3's pictures entered; the blocks past `block` they reach.
fn enter_im3(blocks: &mut Blocks, img: &[u8], block: i64, block_max: i64) -> i64 {
    let mut entries = im3_entries(img);
    // the engine's exchange sort (not stable), which leaves '#' scripts where they are
    for a in 0..entries.len().saturating_sub(1) {
        for b in a + 1..entries.len() {
            entries[a].group = entries[a].group.max(0);
            entries[b].group = entries[b].group.max(0);
            if entries[b].group < entries[a].group && entries[b].name.first() != Some(&b'#') {
                entries.swap(a, b);
            }
        }
    }
    let mut spill = 0;
    let mut last = block;
    for entry in entries {
        if entry.name.first() == Some(&b'#') || entry.name.is_empty() {
            continue;
        }
        let at = spill + block + entry.group;
        let Some(texture) = texture(img, entry.offset, entry.swizzled, entry.no_image) else { continue };
        if at < 0 || at >= block_max {
            continue;
        }
        blocks.blocks.entry(at).or_default().push(texture);
        let mut placed = at;
        if at < block_max - 1 && blocks.remain(at) < 0 {
            spill += 1;
            blocks.spills += 1;
            let moved = blocks.blocks.get_mut(&at).unwrap().pop().unwrap();
            placed = at + 1;
            blocks.blocks.entry(placed).or_default().push(moved);
        }
        last = last.max(placed);
    }
    last - block
}

/// An IMG or IM2 file's pictures entered into one running block.
fn enter_img1(blocks: &mut Blocks, img: &[u8], block: i64, block_max: i64, swizzled: bool) {
    let count = u32_at(img, IMG_COUNT).unwrap_or(0) as usize;
    let mut current = block;
    for index in 0..count {
        let at = IMG_ENTRIES + index * IMG_ENTRY;
        let Some(raw) = img.get(at..at + NAME) else { break };
        if raw[0] == 0 {
            continue;
        }
        let offset = i32_at(img, at + IMG_OFFSET).unwrap_or(0).max(0) as usize;
        let Some(texture) = texture(img, offset, swizzled, false) else { continue };
        if current >= block_max {
            continue;
        }
        blocks.blocks.entry(current).or_default().push(texture);
        // a spilled picture stays in the old block's list, only its block number moves on
        if current < block_max - 1 && blocks.remain(current) < 0 {
            current += 1;
            blocks.spills += 1;
        }
    }
}

/// What a set of picture files takes of the texture blocks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BlockUse {
    /// Blocks from the first: the next load's block minus this load's.
    pub blocks: i64,
    /// Pictures moved on to the next block because theirs was full ("texture over").
    pub spills: usize,
}

/// A map's texture blocks (CMap::LoadData, map.cpp:1506, in LoadMapFromMemory step 2): the add
/// map's picture files then the map's own, each found by the names of the .map's IMG lines and
/// then the add map's (up to 16, AddMapInfo), each file entered at the block the last one left.
pub fn map_blocks(disc: &dyn Disc, entry: &MapEntry) -> Option<BlockUse> {
    let (folder, name) = map_folder(&entry.name);
    let map = dc2tk_assets::mapfile::parse(&disc.read(&format!("{folder}/{name}.map"))?).ok()?;
    let mut names: Vec<String> = map.images.iter().take(16).cloned().collect();
    let mut packs = Vec::new();
    if !entry.add.is_empty() {
        let (add_folder, add_name) = add_folder(&entry.add);
        let add = disc
            .read(&format!("{add_folder}/{add_name}.map"))
            .and_then(|data| dc2tk_assets::mapfile::parse(&data).ok());
        for image in add.map(|add| add.images).unwrap_or_default() {
            if names.len() < 16 {
                names.push(image);
            }
        }
        packs.push(disc.read(&format!("{add_folder}/{add_name}.ipk")));
    }
    packs.push(disc.read(&format!("{folder}/{name}.ipk")));
    let block_max = if entry.kind == MapKind::Dungeon { DUNGEON_BLOCKS } else { TOWN_BLOCKS };
    let mut blocks = Blocks::new();
    let mut block = 0;
    for pack in packs.iter().flatten() {
        for image in &names {
            if let Some(img) = member(pack, image) {
                block += enter_img(&mut blocks, img, block, block_max) + 1;
            }
        }
    }
    Some(BlockUse { blocks: block.max(blocks.used()), spills: blocks.spills })
}

/// Picture files entered together at one block (a character's pictures, an effect file's): the
/// blocks they reach.
pub fn shared_block(images: &[&[u8]]) -> BlockUse {
    let mut blocks = Blocks::new();
    for img in images {
        enter_img(&mut blocks, img, 0, TOWN_BLOCKS);
    }
    BlockUse { blocks: blocks.used(), spills: blocks.spills }
}

/// The picture files a character pack enters (its IMG lines; every .img member when it names none).
pub fn pack_images(pack: &[u8]) -> Vec<&[u8]> {
    let mut slots: BTreeMap<i64, String> = BTreeMap::new();
    if let Some(info) = member(pack, "info.cfg") {
        for (tag, args) in info_cfg::parse(info) {
            if tag == "IMG" {
                if let (Some(slot), Some(name)) = (int_arg(&args, 0), text_arg(&args, 1)) {
                    slots.insert(slot, name);
                }
            }
        }
    }
    if slots.is_empty() {
        return members(pack)
            .into_iter()
            .filter(|(name, _)| name.to_lowercase().ends_with(".img"))
            .map(|(_, data)| data)
            .collect();
    }
    slots.values().filter_map(|name| member(pack, name)).collect()
}

/// The .img members of a pack (an effect file's pictures, a sky's).
pub fn img_members(pack: &[u8]) -> Vec<(String, &[u8])> {
    members(pack).into_iter().filter(|(name, _)| name.to_lowercase().ends_with(".img")).collect()
}

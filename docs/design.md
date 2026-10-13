# Design: Rust, an asset tree, and mods by name

Three changes, in this order: decide the formats below, port the tools to
Rust with the Python as the reference, then build the asset tree and the new
mod format in Rust only. Nothing is written twice.

## 1. The asset tree

`extract` already writes every file of the disc, and `decompile` turns the
scripts into a tree of C. The asset tree does the same for everything else:
`assets/` beside `decompiled/`, each asset once, in a format common tools
open.

```
c2 assets extracted -o assets    (or c2 assets pal.iso; --force replaces a tree already there)
```

```
assets/
  manifest.json                 every asset: id, kind, hashes, every place the disc has it
  textures/<group>/<id>.png     one per distinct picture
  models/<group>/<id>/          one per distinct character pack
    <id>.gltf, <id>.bin         skeleton, skin, meshes, motions as animations
  sounds/<bank>/<NNN>_<NAME>.wav  one per entry that plays a sample (<NNN>.wav when unnamed)
  maps/<map>/<map>.gltf         later: the map's pieces placed as the .map file places them
  text/<file>.txt               later: message files
```

Folders are only for browsing (`<group>` is the folder of the disc file
where the asset is first found: `chara`, `dungeon/monster`, `map/s/s51`);
nothing depends on them. What identifies an asset is its **id**. The whole
PAL disc takes 12-16 s (16 cores) and gives 750 MB in 15,646 files; the same
disc always gives the same tree, byte for byte, whether read from the image
or from `extracted/`.

### What is an asset

The tree reads three kinds of game file wherever they are, on their own or
as members of packs (as deep as `extract` goes): texture files (`.img`),
character packs (`.chr`) and sound banks (`.snd`). Over the PAL disc:

| | places on the disc | distinct (name and bytes) | distinct (bytes alone) | assets |
|---|---|---|---|---|
| pictures | 15,004 in 5,217 texture files | 7,036 | 6,452 | 7,036 textures |
| character packs | 2,997 | 2,744 | 2,692 | 2,410 models |
| sound banks | 861 | 846 | 846 | 846 banks, 16,107 sound entries |

An asset is a distinct content under one of the game's names, extracted
once; changing it changes every place the disc has it. A content found
under two names (a pack copied as `c02_win.chr`) is two assets, since the
game asks for each by its name; that is why there are more assets than
distinct bytes. A picture is its TIM2 bytes and whether its texture file
stores it swizzled; a pack or bank is its bytes.

- 334 distinct packs are not models: 309 hold no model (motions only, or
  pictures only: their pictures are textures all the same), 24 have no
  cfg (packs of other packs), one is not a pack. Most monsters' packs give
  their model in a cfg named after the pack (`b04a.cfg` in `b04a.chr`)
  instead of `info.cfg`; the tree reads such a cfg as the pack's
  `info.cfg`, so they are models.
- 6 texture files hold only render targets (no pixels) and give no texture.
- These are listed in the manifest's `unconverted`, with the reason, so
  `file:` replacements can still reach them.
- A member a pack holds twice under one name is reached by the first, as a
  build finds members by name; the second (one texture file on the PAL
  disc) is left out. A path `DATA.HD4` lists twice is read once, the first.
- The meshes of maps (`.mds` pieces in `.pcp`/`.mpk` packs) are not assets
  of their own: they belong to the maps, which come later.

### Ids

An id is the kind and the game's own name for the asset: `texture:c02a02`,
`model:c02fuku1`, `bank:EV_000_130`. Pictures are named as their texture
file names them (cp932 where the bytes are cp932: `texture:森の美女`),
packs and banks by their file name without the suffix.

Names mostly identify content, but not always (793 picture names have more
than one version, such as a map's pictures redrawn for another map). The
version that is a file of its own on the disc (`snd2/sp/SP_003.snd`) keeps
the bare name before copies inside packs (`menu/0/inv0_bg.pac`'s), and
otherwise the version found first does. The others take `@` and where they
are first found: the name of the disc file holding them when they are a
member (`texture:a01ia_01@a01ic` for `map/a/a01/a01ic.ipk`), else the disc
file's folder with `-` between parts (`model:c02_base@dungeon-chara`); if
that is taken too, `.2`, `.3`, ... Ids differ even ignoring case, as file
names on Windows must. The same disc always gives the same ids.

Sounds are named by the names the definition set's header
(`definitions/pal/include/pal.h`, which `disc.h` sits beside) gives the
entries of a bank: `enum se_ev_000_130_pal` names those of `EV_000_130`,
and the system, base and battle enums those of the bank their comment
names (`snd2/SY_000`, `snd2/BS_999`, `snd2/FG_000`). They name the bank's
bare-named version. So `sound:SE_EV_000_130_GRIFFON_BREAK_WINDOW`, and
`sound:EV_000_130/4` is the same entry by number (the manifest's `aliases`
maps one to the other); entries without a name are only by number
(`sound:EV_000_130/2`, `sound:SP_001@inv0_bg/3`). 1,497 of the 16,107
entries have names.

### The manifest

`manifest.json` lists, for every id, the kind, a SHA-256 of each file as
extracted, a SHA-256 of the game bytes, and every place: the disc path and
member chain, plus the picture (its entry in the texture file) or the
sound entry within it. A mod build reads it to find the places to change;
nobody edits it.

```json
{
 "format": 1,
 "counts": {"texture": 7036, "model": 2410, "bank": 846, "sound": 16107},
 "assets": {
  "texture:c02a02": {
   "kind": "texture", "name": "c02a02",
   "files": {"textures/chara/c02a02.png": "e081…"},
   "game_hash": "dd2e…", "format": "PSMT8", "width": 128, "height": 128,
   "places": [
    {"path": "chara/c02a.chr", "members": ["c02a.img"], "picture": 1},
    {"path": "event/LM/c02ev.chr", "members": ["c02a.img"], "picture": 2}
   ]
  },
  "model:c02_base": {
   "kind": "model", "name": "c02_base",
   "files": {"models/chara/c02_base/c02_base.gltf": "21b2…", "models/chara/c02_base/c02_base.bin": "cf1a…"},
   "game_hash": "8843…", "textures": ["texture:c02a01", "texture:c02a03"], "placeholders": [],
   "places": [{"path": "chara/c02_base.chr", "members": []}]
  },
  "bank:SY_000": {
   "kind": "bank", "name": "SY_000", "folder": "sounds/SY_000", "game_hash": "…",
   "sounds": ["sound:SE_CURSOR_MOVE", "…"], "places": [{"path": "snd2/SY_000.snd", "members": []}]
  },
  "sound:SE_CURSOR_MOVE": {
   "kind": "sound", "bank": "bank:SY_000", "entry": 0, "plays": "sample",
   "label": "カーソル移動", "category": "カーソル移動",
   "files": {"sounds/SY_000/000_CURSOR_MOVE.wav": "bfaf…"}, "game_hash": "9b72…",
   "places": [{"path": "snd2/SY_000.snd", "members": [], "entry": 0}]
  }
 },
 "aliases": {"sound:SY_000/0": "sound:SE_CURSOR_MOVE"},
 "unconverted": [{"path": "chara/c01_fishing.chr", "members": [], "error": "the pack's info.cfg names no model"}]
}
```

Assets are sorted by id. A `files` hash tells a build whether a file in a
mod is still the extracted one.

### Formats

- Textures: PNG, indexed when the picture has a palette (as `mod asset --as
  png` writes now).
- Models: glTF 2.0, as `--as gltf` writes now, with the buffer renamed to
  `<id>.bin`. Images point into `textures/` by relative path, so Blender
  finds them, and are not copied: the pack's own pictures at their texture
  ids, a texture the pack does not have at the bare-named texture of that
  name, and a texture the disc has nowhere (18 on the PAL disc) stays the
  export's placeholder, written beside the model under `textures/`. A
  model built from the tree takes its meshes, skin and pose from the glTF;
  its textures are assets of their own and change through their texture
  ids.
- Sounds: WAV per entry that plays a sample (3,771). The other entries
  are listed in the manifest with what they play, but not exported:
  sequences (`plays` `seseq`, 2,142, and `sq`, 437) until sequences are
  decoded, and entries that play nothing (9,757 numbered lines of the
  banks' cfgs). A bank's folder of WAVs is what `mod asset --as wav`
  would write, with names after the numbers.
- Maps and text: later. The `.map` file is text placing `.mds` pieces
  (position, rotation, scale), so a map is a glTF scene of placed meshes;
  writing it back is its own piece of research.

Every exported file builds back to the disc's own bytes when unedited, at
every place the manifest lists: the hidden `c2 assets-check SOURCE ASSETS`
checks every file's hash, builds each texture into each texture file the
manifest gives for it, each model's glTF and buffer into each of its packs
and each bank's folder into each of its places, compares with the disc,
and checks every texture file, pack and bank of the disc is listed. On the
PAL disc: 15,645 files, 15,004 picture places, 2,663 pack places and 861
bank places build back, no problems, in 13 s.

## 2. Mods by name

A mod is a folder with a `mod.json`. Where files sit inside the folder does
not matter: `mod.json` says what each one replaces.

```json
{
  "name": "castle-showcase",
  "title": "Castle showcase",
  "version": "1.0.0",
  "authors": ["Andrew"],
  "description": "Monica recoloured with a big head, green soldiers, new event sounds.",
  "game": "SCES-51190",
  "start_at": "s51:500",

  "replace": {
    "texture:e254a01": "art/soldier_green.png",
    "model:c02fuku*": "models/monica.gltf",
    "sound:SE_EV_000_130_GRIFFON_BREAK_WINDOW": "sounds/boom.wav",
    "sound:SE_EV_000_130_MONICA_SWING_SWORD": { "file": "sounds/chime.wav", "volume": 90 },
    "texture:a01ia_01@d05": { "file": "art/floor.png", "only": ["map/d/d05"] },
    "file:snd2/event/EV_000_130.snd": "raw/bank.snd"
  },
  "add": {
    "sound:SE_MOD_BEEP": { "file": "sounds/beep.wav", "bank": "SP_004" }
  }
}
```

- **Keys are asset ids**, values are files in the mod (any path inside it)
  or an object with `file` and options. A replacement reaches every place
  the manifest lists for the id; `only` limits it to some of them.
- **Globs** (`model:c02fuku*`) replace a set of ids with one file, such as
  every costume pack of Monica.
- **`file:`** replaces a raw disc file or pack member by path: the escape
  hatch for anything with no common-format converter yet.
- **`add`** makes new assets: a sound entry in a bank (named, so scripts
  use `SE_MOD_BEEP` from the mod's generated header), a new file.
- **Scripts** stay as now: `.c` patches with `#pragma target`, anywhere in
  the folder; `additions` and `start_at` keep their meaning, and the
  `files`/`includes` index of whole-file mods is kept as is.
- **Clashes**: two mods replacing the same id, or overlapping places, are
  refused at build, naming both mods, as script clashes are now.
- **`mod new`/`mod asset`** copy an asset out of `assets/` into the mod and
  add its line to `mod.json`; `mod check` checks every key resolves and
  every file exists.

The `disc/` layout of mods made so far is read by `mod migrate`, which
writes the equivalent `replace` lines.

## 3. The Rust port

One Rust tool, `c2`, doing everything `cli.py` does, with the same command
names and output, so the docs change little.

### Layout

A Cargo workspace at `rust/`:

| crate | Python modules it takes over |
|---|---|
| `c2-disc` | iso, archive, rebuild, extract, discfiles, index |
| `c2-script` | stb, vm, asm, stack, slots, decompile, parse, compile, source, sdk, definitions, names, tables |
| `c2-assets` | im3, textures, soundbank, sounds, mds, mot, info_cfg, to_gltf, from_gltf, models |
| `c2-mods` | mods, patches, additions, numbers, flags, startat, assets (manifest) |
| `c2` | the command line, maps, xref, verify, coverage, learn, play |

The definitions (`definitions/`), tables (`src/c2s/tables/*.json`) and SDK
headers stay data files, read by both tools during the port.

### Speed

The Python runs on one core. Most work is per file (decompile 1,398
scripts, convert 5,217 texture files), so the port runs it across all cores
(rayon) on top of being compiled. Baselines to beat, measured on this
machine (16 cores):

| task | Python |
|---|---|
| decompile every script (`decompile extracted`) | 8 min 42 s |
| verify every script round-trips (`verify extracted`) | 15 min 32 s |
| read every picture, mesh and bank once | 1 min 40 s |
| build an image | about 2 min of compiling, plus writing 4.7 GB |

### Parity

The Python stays until the Rust gives the same bytes everywhere. A `parity`
script runs both on the same inputs and compares:

1. **Disc**: `extract` gives the same tree; an unmodified build gives
   `pal.iso` byte for byte.
2. **Scripts**: `disassemble` the same text for all 1,398 scripts;
   `decompile` the same tree; `compile` the same `.stb` for every file;
   `verify` passes.
3. **Mods**: `castle-golems` builds `modded.iso` byte for byte; `calculator`
   tests pass 4 of 4; `mod check`/`mod list` print the same.
4. **Assets**: `mod asset --as png|wav|gltf` exports the same files for
   every texture file, bank and character pack, and the same rebuilt bytes
   for edited ones (the Python test fixtures).
5. **Commands**: every command in `docs/usage.md` prints the same.

Each milestone ends with its parity check passing and its tests ported to
`cargo test`. Order, by dependency:

1. `c2-disc`: ISO, index, packs, extract, write a disc.
2. `c2-script` reading: stb, vm, disassemble, definitions, names.
3. Decompile.
4. Parse, compile, SDK; then build (scripts only).
5. Mods: patches, additions, numbers, start_at.
6. Assets: textures, sounds, models.
7. The remaining commands; port the last tests; delete the Python.
8. New: the asset tree (`c2 assets`) and `mod.json` replacements.

### Open choices

- **glTF**: write it with `serde_json` as the Python does (output stays
  identical) rather than the `gltf` crate; read it with the `gltf` crate.
- **PNG**: the `png` crate, with the encoder settings matched to the
  Python's so exports are byte-identical, or compare decoded pixels only.
- **Python tests**: 201 tests; most are format round trips that become
  `cargo test`, a few check printed text and become parity checks.

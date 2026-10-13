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
assets/
  manifest.json                 every asset: id, kind, hash, where the disc has it
  textures/<group>/<id>.png     one per distinct picture
  models/<group>/<id>/          one per distinct character or object pack
    <id>.gltf, <id>.bin         skeleton, skin, meshes, motions as animations
  sounds/<bank>/<NNN>_<NAME>.wav  one per entry that plays one sound
  maps/<map>/<map>.gltf         later: the map's pieces placed as the .map file places them
  text/<file>.txt               later: message files
```

Folders are only for browsing (`<group>` is where the asset is first found:
`mainchr`, `dungeon/monster`, `map/s51`); nothing depends on them. What
identifies an asset is its **id**.

### Ids

The disc repeats most of its art. Over the PAL disc:

| | places on the disc | distinct contents | places holding a repeat |
|---|---|---|---|
| pictures | 15,000 | 6,409 | 11,293 |
| meshes | 16,865 | 7,390 | 12,796 |
| character packs | 2,997 | 2,692 | 459 |
| sound banks | 861 | 846 | 19 |

So an asset is a distinct content, extracted once, and changing it changes
every place the disc has it. Its id is its kind and the game's own name for
it: `texture:c02a02`, `model:c02fuku1`, `bank:EV_000_130`. Names mostly
identify content, but not always (783 picture names have more than one
version, such as a map's pictures redrawn for another map). Those take a
suffix naming where the version is first found: `texture:a01ia_01@d05`. Ids
are deterministic: the same disc always gives the same ids.

Sounds are named by the names `disc.h` already gives entries:
`sound:SE_EV_000_130_GRIFFON_BREAK_WINDOW`. `sound:EV_000_130/4` is the same
entry by number, for entries without a name.

`manifest.json` lists, for every id: the kind, a hash of the content as
extracted, the hash of the game bytes, and every place (`disc path` and
member chain, plus the picture or entry within it). A mod build reads it to
find the places to change; nobody edits it.

### Formats

- Textures: PNG, indexed when the picture has a palette (as `mod asset --as png` writes now).
- Models: glTF 2.0, as `--as gltf` writes now; textures referenced from
  `textures/` by relative path, so Blender finds them, and not copied.
- Sounds: WAV per entry; sequence entries (`.mid`, `.sq`) listed in the
  manifest but not exported until sequences are decoded.
- Maps and text: later. The `.map` file is text placing `.mds` pieces
  (position, rotation, scale), so a map is a glTF scene of placed meshes;
  writing it back is its own piece of research.

Every exported file builds back to the disc's own bytes when unedited, as
now.

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
8. New: the asset tree (`c2 extract --assets`) and `mod.json` replacements.

### Open choices

- **glTF**: write it with `serde_json` as the Python does (output stays
  identical) rather than the `gltf` crate; read it with the `gltf` crate.
- **PNG**: the `png` crate, with the encoder settings matched to the
  Python's so exports are byte-identical, or compare decoded pixels only.
- **Python tests**: 201 tests; most are format round trips that become
  `cargo test`, a few check printed text and become parity checks.

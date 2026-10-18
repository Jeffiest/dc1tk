# dc1tk: DC2-Toolkit on Dark Cloud 1

This is a fork of DC2-Toolkit (MIT) that also reads Dark Cloud 1 (PAL, the Chronicle PC data folder). See
`E:\Chronicle-project-Claude\DC1_TOOLKIT_PLAN.md` for the plan.

## Changes from upstream
- `natives`: a Chronicle (DC1) checkout is detected (no event_func.cpp, has editloop3.cpp). It writes `event` (editloop3.cpp,
  308) and `monster` (runscript_opcodes.cpp, 91) tables. Rows may give literal numbers.
- `extract` also takes a folder (DC1's PC data is already unpacked). It writes the same tree plus `manifest.json`, without a disc layout.
- info.cfg reader: a cfg with no `;` outside quotes ends each statement at the line's end (DC1 style). This is what turns
  on DC1 model export.
- Text: `text_dc1.rs` is upstream Chronicle's codec (port/src/gametext.cpp: the gaiji letter grid, the accents, and named
  controls like `{page}` `{cyan}` `{insert1}` `{wait 12}`). It is used when the data has `meswin/gaiji.img`, so dc1tk's .txt reads
  as upstream's text does. A file that lists an id twice shows the first.
- `lang-export`: `lang_dc1.rs` ports upstream `LocalizeExport`. It writes `<language>.json` + `hashes/` in upstream's `lang/`
  format (the port loads them directly; see upstream docs/LOCALIZATION.md).
- Sound: `Bank::from_pair` opens one hd/bd pair of a pack (DC1 packs have several, with no .vol). `sound_dc1.rs` and
  `sound-export`/`sound-build` handle WAVs per sample, a .sf2 per bank, and .mid per .sq. Music replacement by recording is upstream's
  `soundtrack.json`, and mod sound playback is MoonMod's `game.audio`.
- Maps (`map_dc1.rs`, `maps-export`, `floor-build`, `scene-build`, `layout-build`):
  - dungeon floors: `dun/<floor>.cfg` + `dun/mpd_pack/<floor>.mpd` export as `floor.cfg` (the editable script), `floor.json`
    (for reading: settings and DEF_PATS parts) and `parts/<model>.gltf`. `_a`/`_v` models export as collision. A part the reader
    cannot convert is kept as `parts/<name>.mds` and listed in `parts/not_converted.json`.
  - Georama scenes (`.scn`, PTS archives of EPARTS_ARCHIVE parts) export as `scene.json` (position, rotation, grid cells, elements;
    position/rotation/cells can be edited) and `<part>/<slot>.gltf` (lod0-3, collision, shadow, shade, camera, extra; `.bin` for
    other blocks). An edited block is appended to its part and its slots repointed; a part that grows is appended to the scene.
  - Georama layouts (`gdata0.edt`): `layout.json` (part, quarter turns, position), parts can be added or removed.
  - `mds.rs` reads DC1 collision meshes (`Dc1Collision`: triangles of 5 words + 16-byte attribute records) as dc2tk's collision
    materials (`col_ground<n>_foot<n>_area<n>_mask<n>`) and writes them back in DC1's form.
- Drawn meshes (`mds.rs` `Dc1Faces`): DC1 reads every corner as vertex, normal, UV, plus colour when the mesh has colours,
  whatever the prim's flags say (visualvu1.cpp CreateVUdataFromMDT), and ignores the faces header's last word. Used when DC2's
  reading does not fit; kinds and the header word are written back. The glTF export tolerates a normal/UV/colour index past
  its list (the game reads what follows).
- Character packs (`tree.rs` `model_pack`, `character_cfgs`): the model script is `<pack>.cfg` when it names a MODEL, else
  `base.cfg` (Goro, Xiao...), else the one cfg naming a MODEL. A pack with several (event packs) is one model asset per cfg,
  named after it.
- `vertex_patch.rs`: a glTF that differs from the export only in vertex positions is written into the model's own bytes
  (moved vertices mapped back through each mesh's fitted export transform), so the file keeps its size, as MoonMod's
  `game.resources.override` needs. Used by floor-build/scene-build and character builds before a full rebuild.
  `model-export <chr> <folder>` / `model-build <folder> <chr> -o` export and build one character pack.
- Sound in the asset tree: a DC1 pack's samples (over all its banks) are its numbered entries, so `assets` writes a WAV per
  sample and builds them back; music sequences export as MIDI with a SoundFont of the bank of the same name. A bank that
  does not read (`sound/set/snd31.snd`'s `s30a_g`, cut short) keeps its bytes.
- Motions: DC1 `KEY start, end, speed` lines (numbered from `KEY_START`), exported as clips `key100`, `key101` ...;
  motion-only packs (Toan's ladder `chara/c01dhashigo.chr`, event and villager motions) export on their character's skeleton,
  found from a character code in the pack's or its .mot's name (`c01d`, `c23a`, `p14a`; same number any letter if needed).
- A pack of one model and no cfg (`gedit/e04/chara/s06.chr`) exports and builds as a static model.
- Size: `vertex_patch` writes edited positions, normals, UVs and colours into the model's own arrays (no size change). When
  an edit still makes a file too big, `mod-pack` makes room with `fit_dc1`: comments out of tag scripts, then meshes in the
  packed layout, and says what it took out.
- `definitions/dc1/natives/*.json` are generated. `sdk-dc1/` is generated by `tools/dc1/gen_sdk.py`, with names from the original
  `_NAME` (lower case).

## Making a MoonMod package (`mod-pack`)
Put edited exports in a work folder, in the layout the export commands write (any subset):

```text
floors/<floor>/                 maps-export      (dungeon floor: floor.cfg, parts/*.gltf)
scenes/<data path of .scn>/     maps-export      (Georama scene)
layouts/<data path>.edt.json    maps-export      (Georama town layout)
models/<data path of .chr>/     model-export     (character)
scripts/<data path>.c           decompile        (dun/monstor/* use the monster natives, others event)
sound/<data path of .snd>/      sound-export
lang/<language>.json            lang-export      (only keys whose text changed matter)
soundtrack/                     the port's soundtrack folder (copied into the package as is)
monsters/<stem>/                a new enemy: monster.json, optional model/ (model-export of its base) and script.c
```

**New enemies (`monsters/<stem>/`)**: `monster.json` is `{"name", "template_monster_id", "base": "dun/monstor/<x>.chr",
"base_script": "dun/monstor/<x>.stb", "stats": {"hp", "defense", "reward_exp", "reward_money"}}`. `model/` (an edited
`model-export` of `base`) and `script.c` (an edited `decompile --table monster`) replace the base's model and AI; without
them the base's files are used. The package gets `dun/monstor/<stem>.chr` and `.stb` as new files (a stem the game does not
have, at most 15 characters), and main.lua registers them with `game.models.register` + `game.characters.register`; spawn
the enemy with `game.characters.spawn`. Tested: `mybat` from `c12a` packs 1,152,032 + 26,816 bytes, and the script
compiles back byte-identical to `c12a.stb`.

**Install (`--install <save>`)**: writes the package into `<save>/mods` (no `-o` needed) and turns it on in
`<save>/moonmod/profile.json` (backup `profile.json.before-<id>`). A rebuild replaces the package's `resources/`, and
a running game picks up rewritten files about once a second (MoonMod hot reload, play-fork from 2026-10-18).

`dc2tk mod-pack <work> <data> --id <author.mod> -o <save>/mods` builds each against the game's file, leaves out what
comes out unchanged, refuses what is larger than the game's file (MoonMod's limit), and writes `mod.json`, `main.lua`
(one `game.resources.override` per file) and `resources/<data path>`. Every build command (`floor-build`, `scene-build`,
`layout-build --original`, `model-build`, `sound-build`) prints the same size check. Text goes back into the `.mes`
files (inside `.pak`/`.pac` packs too), so a mod's text is per mod and removable. Enable the package in MoonMod's Mod Manager.

**Port mode (`--port`)**: for the PC port only (play-fork from 2026-10-18). Files may grow past the game's size, up to a
limit per kind (`.mpd` 6.4 MB, `.cfg` 1 MB, `.chr` 16 MB, `.stb` 4 MB, `.mes` 1 MB (opening `opdat/*.mes` 68,992), `.snd`
16 MB, `.scn` 8 MB, `.edt` 64 KB; see `E:\Chronicle-project-Claude\DC1_PORT_SIZE_LIMITS.md`), so mesh topology edits,
longer scripts and extra parts go in as they are. Other kinds (`.mds`, `.img`, `.pak`, ...) stay at retail size. The game
checks at load time that a grown file fits the buffer it reads it into and logs `moonmod: override for <path> grows ...`
or `moonmod: refused override ...`. Without `--port` the package also works on builds without port mode.

## Commands (Git Bash, from E:\Chronicle-project-Claude\dc1tk)
```bash
build/release/dc2tk.exe natives "C:/Users/armor/Documents/Codex/2026-10-14/i-want-to-build-an-all/Chronicle" --out definitions/dc1/natives
python tools/dc1/gen_sdk.py definitions/dc1/natives sdk-dc1
build/release/dc2tk.exe extract C:/Users/armor/AppData/Local/chronicle/data E:/dc1tk-work/extracted
build/release/dc2tk.exe assets E:/dc1tk-work/extracted -o E:/dc1tk-work/assets
build/release/dc2tk.exe assets-check E:/dc1tk-work/extracted E:/dc1tk-work/assets
build/release/dc2tk.exe decompile --no-defs --natives definitions/dc1/natives --sdk sdk-dc1 --table monster <file.stb> -o <file.c>
build/release/dc2tk.exe compile --no-defs --natives definitions/dc1/natives --sdk sdk-dc1 --table monster <file.c> -o <file.stb>
build/release/dc2tk.exe lang-export C:/Users/armor/AppData/Local/chronicle/data E:/dc1tk-work/lang
build/release/dc2tk.exe sound-export C:/Users/armor/AppData/Local/chronicle/data E:/dc1tk-work/sound --check
build/release/dc2tk.exe maps-export C:/Users/armor/AppData/Local/chronicle/data E:/dc1tk-work/maps --check
build/release/dc2tk.exe floor-build E:/dc1tk-work/maps/floors/d01main_a C:/Users/armor/AppData/Local/chronicle/data d01main_a -o E:/dc1tk-work/built
build/release/dc2tk.exe scene-build E:/dc1tk-work/maps/scenes/gedit/e01/scene.scn C:/Users/armor/AppData/Local/chronicle/data/gedit/e01/scene.scn -o E:/dc1tk-work/scene.scn
build/release/dc2tk.exe layout-build E:/dc1tk-work/maps/layouts/gedit/e01/gdata0.edt.json -o E:/dc1tk-work/gdata0.edt
build/release/dc2tk.exe sound-build E:/dc1tk-work/sound/sound/bgm/bgm0.snd C:/Users/armor/AppData/Local/chronicle/data/sound/bgm/bgm0.snd -o E:/dc1tk-work/bgm0.snd
```
To try an edit in the game, put the built file under `win-save\mods\<mod>\files\` at its data path (FILES.md), or put text in
upstream's `lang/`.

Use `--table monster` for `dun/monstor/*.stb` and `--table event` for everything else.

## Results (2026-10-18)
| what | result |
|---|---|
| scripts .stb to asm and back | 285/285 byte-identical (retail; 5 mod scripts skipped) |
| scripts .stb to C and back | 285/285 byte-identical, with DC1 native names |
| extract | 5,445 files gives 23,897 (18,452 pack members), 24 s |
| textures to PNG | 4,521 textures from 10,531 pictures; all 10,531 build back identical |
| character models to glTF | 1,026 models (was 749); Toan and Xiao checked by eye; 1,321 pack places build back identical. Not models: 102 motion-only packs, 20 packs with neither. Left: gedit/e04/chara/s06.chr (a bare .mds + .img, no cfg) |
| text (.mes) | 739 texts, readable (`{icon-jar}"{insert1}" acquired.`); all 872 files build back identical, 0 problems |
| lang-export vs the port's own `--export-text` | 863 files; every key and text identical in all 7 languages (dc1tk adds the 13-14 `opmes` storybook keys newer upstream also writes) |
| sound | 217/218 packs give 7,654 WAV/SF2/MIDI files and build back identical (sound/set/snd31.snd has a truncated .hd). A 440 Hz WAV put in bgm0.snd reads back as 438 Hz at the bank's rate |
| maps | 35/35 dungeon floors, 204/204 Georama scenes and 6/6 layouts build back identical. Every floor model (1,229 drawn + 1,074 collision) and every scene model (1,183) converts to glTF. A coloured mesh scaled x2 rebuilds and reads back with its colours. Edit test: fog, a collision surface's ground, a part scaled x1.5, a scene part moved + a cell changed, and a town building turned and moved all come through a build and re-export |
| in game (MoonMod play-fork, 2026-10-18) | package `save\mods\dc1tk.ingame-test` (`game.resources.override`): red fog/background in Divine Beast Cave, Toan's turban x1.8 and Skeleton Soldiers 3x faster all seen by Jeff; corridor walls x1.5 built and accepted (not singled out in the screenshot). Every file kept at retail size (MoonMod refuses larger) |
| mod-pack | rebuilds the in-game test package byte for byte; a text edit goes back into its .mes (one message changed) |
| sound in `assets` | 218/218 packs, 7,008 WAVs, 103 sequences as MIDI; assets-check 0 problems |
| motion-only packs | 82/102 export and build back identical (Toan's ladder: 6 clips). Left: 18 opening-scene villager motions (`opdat/chara/01p17a*`, `01p19a*`: no model pack on the disc) and `gedit/s03/chara/e106zyu.chr`, `e130zyu.chr` |
| size-growing edits | 2 extra torches (+84 bytes) fit after comments come out (3,818 bytes freed); a UV edit is written in place (no growth) |
| tests | `cargo test --release`: all pass |
| not converted | 115 bad faces header, 16 faces size mismatch, 103 cfgs with no MODEL, 7 CLUT type 1, 8 empty texture files |

## Blender add-on (`blender-plugin/dc1tk_maps`)
Imports floors (parts, collision, camera, torches), Georama scenes (parts, cells) and layouts from `maps-export`, and
exports edits back (vertex moves keep file sizes; face changes warn). See its README. 2026-10-18: d01main_a, 0101.scn
and e01 gdata0.edt round-trip and build byte-identical; edits build at retail size (Blender 4.2.3).

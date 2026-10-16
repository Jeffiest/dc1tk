# Modding

Getting started with mods: the quickest way from an edit to a disc image, the commands that find
what to edit, and how to keep your edits. The pages below say how to make mods and edit each
kind of asset. How the original game works is in [the game](../game/README.md), and writing
script code in [coding](../coding/README.md).

## The quickest way to a mod

The quickest way to a mod is to edit what `dc2tk setup` wrote and let `dc2tk mod make`
collect it. Edit scripts in `extracted/scripts`, textures, models and sounds in
`extracted/assets` (PNG, glTF, WAV), or any game file in `extracted/disc`, and
add new files there. `dc2tk diff dc2.iso` lists what changed. `dc2tk mod make dc2.iso mymod` writes
`mods/mymod`, with the scripts as whole files, each changed asset as a `replace`
line by its id (so it changes every place the disc has it), and each changed or
added disc file as a `file:` line. Then `dc2tk build extracted/scripts dc2.iso -o
mymod.iso --mod mymod`. The disc's own files beside `DATA.DAT` (movies,
`SOUND.DAT`, the indexes, the boot program) are listed but cannot go in a mod.
`build`, `diff` and `mod make` take an extracted disc folder in place of the
image too, but it has to be untouched: `build` packs the folder it is given as
it is, so a folder you edited would put your edits in under the mod's.

Boot `mymod.iso` in PCSX2 to play it.

## Finding your way around

| Question | Command |
| --- | --- |
| What runs program 510 of map s51, and what asks for it? | `dc2tk xref extracted/scripts event 510 --map s51` |
| How does the game get to map s51, and at which program? | `dc2tk xref extracted/scripts map s51` |
| Which model and AI script does a monster kind use, and who places it? | `dc2tk xref extracted/scripts monster golem` |
| Which save flags are in use? Who uses flag 54? (A mod asks for its own with `NEW_FLAG`.) | `dc2tk flags extracted/scripts`, `dc2tk flags extracted/scripts --flag 54` |
| Which save counters are in use? Who uses counter 5? (`NEW_COUNTER` for a mod's own.) | `dc2tk flags extracted/scripts --counters`, `dc2tk flags extracted/scripts --counter 5` |
| Which maps can a scene start in, and where does the player play? | `dc2tk maps extracted/scripts` |
| A map's programs, free program numbers, trigger points and player positions? | `dc2tk map extracted/scripts s17` |
| What have I changed? | `dc2tk changes extracted/scripts` (scripts), `dc2tk diff dc2.iso` (scripts, assets and disc files) |
| Turn my edits into a mod | `dc2tk mod make dc2.iso mymod` |
| Make its whole scripts patches of what changed | `dc2tk mod slim extracted/scripts mymod` |

Files keep the game's codes with a readable name after them
(`map/s/s51_raybrandt_castle/`, `dungeon/monster/e33a_golem.c`); commands
take the codes. `extracted/scripts/include/disc.h` also gives each monster kind's model
and AI script:
`MONSTER_GOLEM = 168,  // model e33a, AI e33a`. Several kinds can share one
AI script (all four kinds of `e33a`), and a story fight can use an event-only
kind with its own AI (kind 523, the castle's Griffon Soldier, runs `b254a`
while the ordinary Griffon Soldier, kind 37, runs `e254a`).

`xref` and `flags` read the source tree as text and fold constant
expressions (`50 * 10 + 0 + 10000`, `story_50`), so they see your edits as
soon as you save. Calls whose arguments are worked out at run time are listed
for you to check rather than guessed at. `xref event` lists every script that requests an
event and marks the AI scripts of monsters the map places
(see [monsters can start map events](../game/running.md#monsters-can-start-map-events)).

## Keeping your edits

`setup` and `decompile` write `original-hashes.json` at the root of the tree,
recording what they wrote. From it:

- `dc2tk changes extracted/scripts` lists every file you edited, added or removed;
- `setup` and `decompile` refuse to overwrite edited files (`--force`
  overrides);
- `dc2tk mod save extracted/scripts castle` copies your edited and added files into
  the mod `mods/castle`.

Better still, keep the tree untouched and do the work in mods:
`dc2tk mod override extracted/scripts castle --map s51 start_castle_battle` starts a
patch holding just that function, and
`dc2tk build extracted/scripts dc2.iso -o modded.iso --mod castle --mod another`
merges the chosen mods into the scripts they target. A patch holds only what it changes, so a fresh tree,
decompiled after a tools update, takes the same mods, and several mods can
change one script. Better definitions can rename a function a patch
overrides; the mod keeps the function's fingerprint, so `build` still finds
it and says its new name, and `dc2tk mod check extracted/scripts castle
--rename` puts the new name in the patch. (`mod new` copies whole files into a mod instead; `mod
list` marks a copied file whose decompiler version has changed since, to
compare. `dc2tk mod slim extracted/scripts castle` turns such copies into
patches of the functions they change, where the mod then builds to the same
bytes.) See [mods](mods.md).

## How-to pages

- [Mods](mods.md): `mod.json`, patches and their pragmas, the numbers a mod asks for, mods
  that build on mods, whole files, assets by id, tests, starting at a scene, a first test.
- [Editing scripts](scripts.md): the decompiled tree, shared code, wrapping natives, language
  variants.
- [Commands](commands.md): every `dc2tk` command and the options they share.
- [Textures](textures.md): pictures as PNG.
- [Sounds](sounds.md): sound effects as WAV.
- [Music](music.md): background music and ambient loops as MIDI, and music of a mod's own.
- [Text](text.md): the menus' text and system messages as UTF-8 text.
- [Models](models.md): character models and their motions as glTF, and characters of a mod's
  own.
- [Maps](maps.md): maps as glTF scenes in Blender, new maps and doors into them.
- [Dungeons](dungeons.md): dungeon floors, their parts and room layouts.
- [Game data](data.md): items, shops, monsters and the other data tables as JSON.
- [Memory](memory.md): what the game has room for, and what `build` checks.

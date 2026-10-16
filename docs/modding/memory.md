# Staying within memory

The game loads into fixed memory, and `build` checks every load the files it changes take part
in: a file read whole, a map's script (312,816 bytes at most; some of the disc's are within a few
thousand of it), a map's placements (320, 400 in a dungeon), texture blocks, sound memory, a
skinned mesh's vertices and bones. Each budget, where the engine sets it and how much of it the
disc already uses is in [memory budgets](../game/memory.md); this page says what `build` checks
and how it reports it. The data tables' limits on counts are checked as the tables are read
([editing game data](data.md)).

A map counts as the kind its row in the game's own map tables (`map/map<language>.cfg`) gives it,
town, interior or dungeon; a new map as its like map's. A budget that is certain and exceeded is
an error: the image is still written, but `build` says the game will not load it and exits with
1. An estimated budget exceeded, where the load grew, is a warning.

## What build checks

`build` runs the check on every disc file it rewrites whose bytes changed, and prints `error:`
where a certain budget is exceeded (the image is still written, but `build` fails), `warning:`
where an estimated one is (or a load grows past what the disc shows to work), and `memory:` with
what each changed load uses:

- **A map** whose `.map`, `.cfg`, `.mpk`, `.ipk`, `.efp` or `.sky`, or whose add map's files, changed:
  read buffer and loading screen (past the loading screen an error in town, a warning in a
  dungeon), placements, texture blocks (an error past its blocks; a warning when a town map goes
  past 24 that did not), and its estimated memory (a warning when it is more than the largest of
  its kind on the disc and more than before). No single number budgets a map, so `build` compares
  a changed map's estimated memory with the largest of its kind on the disc instead. The maps and
  their kinds come from the built disc's map tables, so a mod's new map is checked as the kind of
  the map it is like; a changed map file that no table lists is taken by its name's first letter
  (`d` a dungeon map, `i` an interior, else a town map).
- **New maps' rows** in the map table: `build` refuses rows past the table's buffer
  ([a new map](maps.md#a-new-map)).
- **A map script** (`map/**.stb`): the script buffer.
- **A character pack**: the read buffer; skinned meshes' vertices and bones; for the party's
  packs the estimate against their stack; its pictures against its texture blocks.
- **A sky**: the read buffer, and each picture file in one block.
- **A map's effects** (`.efp`): its pictures against their one block.
- **A sound bank**: the read buffer, the IOP window, its region (music and system an error, menu
  and character a warning), each sound set that loads it with the other field banks (an error
  past the field region), and a warning when a field, event or monster bank grows, since the
  scripts' fullest set leaves 5,792 bytes. For a music, base, battle, ambient or object bank whose
  tables on the EE grew, their estimated size against its layer's EE stack (a warning past it;
  a set's object banks count together; see [the sound bank format](../game/sounds.md#8-memory-the-game-gives-a-bank)).
- Any other file that grows gets a note that no budget covers it.

Not checked: the monster and event banks' combinations with each sound set (measured in this
research by following the scripts, not built into `build`), the EE tables of event and monster
banks (where they go was not traced), the Ridepod's and the monster form's stacks, the shared
pool's free space, and the menu and dungeon message files, whose room in the menu's memory and
the dungeon's heap is not known ([text](../game/text.md#limits)).

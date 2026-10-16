# The game

How Dark Chronicle (Dark Cloud 2, PAL) works and how its files are made: the facts a mod rests on,
whatever tool makes it. Nothing here is about this project's tools; how to change each kind of
file is in [modding](../modding/README.md), and how to write script code is in
[coding](../coding/README.md).

The knowledge comes from the [ChronicleTwo](https://github.com/TheMoonPeople/ChronicleTwo)
decompilation of the engine, checked against the files of the PAL disc. Each page names the engine
code a rule comes from (paths are relative to a ChronicleTwo checkout) and counts over the whole
disc.

## Engine and scripts

The game is split in two. The **engine** (the PS2 executable) draws the world, runs physics and
collision, plays animations and sound, and owns the data tables. The **scripts** decide what
happens: every cutscene, conversation, door, chest, monster brain, the player's sword combo, and
how each spell or explosion moves. Scripts call into the engine through about 1,100 engine
functions called *natives*, and the engine calls into scripts by starting their *programs* when
something happens. There are five kinds of script, each with its own table of natives: map and
event scripts, monster AI, player actions, effects and argument files
([the big picture](../coding/README.md#the-big-picture) says what each does).

## The pages

- [The disc](disc.md): the ISO, `DATA.DAT` and its index `DATA.HD4`, and pack files.
- [The script format](scripts.md): `.stb` files, the VM's instruction set, the native tables,
  and the older numberings some scripts were compiled against.
- [How scripts run](running.md): programs, event mode, shared state (save flags and counters,
  monster variables, globals), language variants, the events the dungeon starts, and the codes
  of message text.
- [Maps](maps.md): `.map`, `.pcp`, `.mpk`, `.ipk` and collision: placement, meshes, trigger
  points, animated pieces and the kinds of map.
- [Dungeon floors](dungeons.md): how a floor is laid out from its parts and room script, and what
  its chests and monsters are.
- [Character models](models.md): MDS meshes, skinning (`.wgt`, `.bbp`), `.mot` motions,
  `info.cfg` and `.clo`.
- [Textures](textures.md): `.img` archives of TIM2 pictures, their pixel formats and palettes,
  and how the engine finds a texture.
- [Sound banks](sounds.md): `.snd` packs, the `.cfg`, `.vol`, `.hd` and `.bd` inside them, and
  how a sound effect becomes voices.
- [Music](music.md): the `.sq` sequences of the background music and ambient loops, and how the
  game plays them.
- [Message text](text.md): the `.mes` files of the menus, shops, maps and townspeople, their
  glyphs and layout codes.
- [Game data](data.md): where items, shops, monsters, inventions, Georama, fishing, medals and
  characters' numbers live, and their record layouts.
- [Memory](memory.md): the fixed memory the game loads into, and how much of each budget the disc
  already uses.

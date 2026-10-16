# new-map-door: a new map with a script of its own, and a door into it

Palm Brinks Park II (`m05x`), a new map copied from Palm Brinks' park
(`m05`, which has no script), with an event script of its own, reached
through a door added in Palm Brinks (`m01`), and back through the park's
big tree ([maps](../../assets/maps.md) section 9):

- `mod.json` adds the map (`"add": {"map:m05x": {"like": "m05",
  "title": "Palm Brinks Park II", "script": "m05x.c", ...}}`, the title its
  row in the map table gets) and replaces Palm Brinks' scene with one that
  has the door. Its `"programs"` give the new door point `door_park` the
  program `ENTER_PARK`, and the park's tree point `dun07` (Palm Brinks'
  dungeon menu, program 290) the program `BACK_TO_TOWN`, by name.
- `park.h` asks build for the three program numbers (`NEW_PROGRAM`) both
  scripts use, and says where the door is.
- `m01.c`, a patch of Palm Brinks' script: `ENTER_PARK` jumps to the new
  map (`jump_to_map(-1, "m05x", map_start)`), `BACK_FROM_PARK` runs the
  town's own arrival and stands the player at the door, and the first
  arrival in the town stands the player in front of the door and says so.
- `m05x.c`, the new map's whole script: `map_start` stands the player in
  front of the big tree and shows a message (in French on a French game,
  by `LANGUAGE`), and `BACK_TO_TOWN` goes back to Palm Brinks at
  `BACK_FROM_PARK`. `build` compiles it in every language and adds it as
  `map/m/m05/m05x_1.stb` to `m05x_5.stb`, with `m05x.stb` compiled as
  Japanese beside them (only `m05x.stb`, which every language falls back
  to, when the languages compile alike).
- `"start_at": "m01"`: a new game starts in Palm Brinks, as Max.

Nothing of the game's is in this folder. `make_mod.py` (plain Python, no
Blender) copies your own exports of the two maps into the mod, adds the door
point to Palm Brinks' scene where the closed house door `door10` was (and
deletes that one), and copies this folder's files beside them:

```bash
dc2tk map-export m01
dc2tk map-export m05
python docs/examples/new-map-door/make_mod.py
dc2tk build extracted/scripts dc2.iso -o new-map-door.iso --mod new-map-door
```

`build` prints the programs the points got (`point door_park runs program
9002`), the new map's number (`MAP_M05X = 189`), the new files (the map's
four, its scripts) and the patched Palm Brinks scripts.

By hand in Blender: import `m01` in the add-on's Map panel, put the 3D
cursor on the point `door10`, Add Point, Door, name it `door_park`, delete
`door10`, export into the mod; export `m05` as it is into the mod's
`maps/m05x/`; and copy this folder's `mod.json`, `park.h`, `m01.c` and
`m05x.c` beside them.

In the game: Max stands in front of a house door near the start of Palm
Brinks' street (one that only knocked before), with a message. Press (X) at
the door: it opens, the screen fades, and the park loads as the new map,
Max facing its big tree, with the new map's message. Press (X) at the tree's
trunk: back to Palm Brinks, standing at the door.

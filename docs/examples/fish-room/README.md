# fish-room: a new room with a model, an animation and a tune of its own

The Fish Room (`fishroom`), a new map behind a door in Palm Brinks (`m01`):
a big black room with a giant fish floating in the middle, turning round and
round with a blank stare, as if it had not noticed, to a tune of its own. A
door behind where you arrive takes you back.

Everything in it is made here; nothing of the game's is in this folder:

- `make_mod.py` (run by Blender, with the [c2_maps add-on](../../../src/blender/README.md)
  on the path) imports your export of `m07` (the opening's Palm Brinks at
  night, chosen because it has no sky), deletes every placement, makes the
  background black and the fog off, and models:
  - the room: a floor of dark tiles and black walls, made a new model with
    floor, wall and camera collision (New Model from Selection);
  - the fish: a body, a forked tail and fins, with a picture of scales drawn
    here, big round eyes with tiny pupils staring straight out, and a slack
    open mouth, about 420 units long (Max is about 26), floating 230 up;
  - a door in the +Z wall, and a Door point in front of it;
  - a Piece animation point on the fish, turning it 1.5 degrees a frame
    about the vertical, for ever.

  It exports the scene into `mods/fish-room/maps/m07/` (the add-on leaves
  `replace` alone, as `add` already names that file), and, as the
  new-map-door example does in plain JSON, adds the door point `door_fish`
  to Palm Brinks' scene where the closed house door `door10` was.
- `compose.py` writes the tune, "Round and Round" (24 bars in F, looping),
  and its four instruments, synthesized as WAVs: a wobbly vibraphone for
  the tune, a marimba's broken chords, a plump bass that bloops, and the odd
  bubble.
- `mod.json` adds the map (`"like": "m07"`, its own `"script"`, the door
  point's program) and a music file of its own, `BG_200`, a copy of the
  fishing music's bank (`BG_193`) playing the tune with the four instruments
  as programs 100 to 103, so no tune of the game's changes.
- `fishroom.c` is the room's script: on arrival it plays `BG_200`
  (`load_bgm`, `play_bgm`), stands the player just inside the door facing
  the fish; the door goes back to Palm Brinks. No text pops up.
- `m01.c` patches Palm Brinks: the door into the room, the way back, and
  on the first arrival the player stands facing the door.

```bash
c2 map-export m01
c2 map-export m07
blender --background --python docs/examples/fish-room/make_mod.py
c2 build extracted/scripts dc2.iso -o fish-room.iso --mod fish-room
```

In the game: a new game starts in Palm Brinks, facing a house door.
Press (X) at the door: it opens, the screen fades, and the room loads, dark,
the fish turning over the middle of it, the tune playing. Turn round and
press (X) at the door to go back.

# spinning-cult: The Spinning Cult

An eerie mod built on the [fish-room](../fish-room/) example. Behind a door in
Palm Brinks the giant fish still turns in the dark, and someone is waiting
under it: Brother Gill, faceless in his hood. He gives Max a camera and asks
for the fish's likeness, then a shape the town can hold. Max invents the
Whirligig, and everyone he shows it to begins to turn. When five have turned,
the fish stops, and looks at him.

Nothing of the game's is in this folder: the maps come from your own disc,
the townspeople's new motions are added to your own disc's models, and Brother
Gill, the room, the fish, the pictures and the music are made here.

## Making it

```bash
c2 map-export m01
c2 map-export m07
blender --background --python docs/examples/spinning-cult/make_gill.py
blender --background --python docs/examples/spinning-cult/make_mod.py
blender --background --python docs/examples/spinning-cult/make_villagers.py
c2 build extracted/scripts dc2.iso -o spinning-cult.iso --mod spinning-cult
```

- `make_gill.py` models Brother Gill (`model/gill.gltf`, made by code): a
  gaunt robed figure, 34 units tall, a black void in his hood with two pale
  unlit eyes, eighteen bones, and six motions: `idle`, `beckon`, `raise`,
  `spin`, `turn_head` (too far, too slowly) and `vanish` (a material fade to
  nothing, played held). mod.json makes him a new pack, `mods/gill/gill.chr`.
- `make_mod.py` builds the Fish Room with the fish-room example's
  `make_mod.py`, then adds: a second fish, awake, its pupils grown huge,
  hidden until the ending and turned to the player by a Piece animation point
  in "look at the player" mode; an Invent box round the fish, so a photo of
  it gives the new idea "Endless Turning" (251); black fog, and a red light
  set for the ending. Palm Brinks gets the door and a ninth light set, dark
  with a dried-blood fog. It writes the music (`compose_turned.py`) and copies
  this folder's scripts and data into `mods/spinning-cult`.
- `make_villagers.py` exports five townspeople from your disc (Parn, Sheriff
  Blinkhorn, Giles, Aegira and Fournet), adds two motions to each in
  Blender, `spin` (arms limp, face tilted up, a slow turn on the spot) and
  `stare` (still, head hung), and exports them back into the mod, where
  build adds them as new KEYs.
- `compose_turned.py`: "The Turning", the Fish Room's tune gone wrong: a
  detuned music box in F minor over a breathing drone, a wordless choir, a
  deep bell and the odd whisper, all five instruments synthesized; a music
  file of the mod's own (`BG_201`).
- `data/items.json` adds the Whirligig (item 429, a key item like the
  Treasure Chest Key); `data/inventions.json` adds the idea Endless Turning
  and the recipe: Endless Turning, Flag and Post, from one Scrap of Metal.

## The scripts

- `cult.h`: the cult's state in one save counter (`NEW_COUNTER`), read through
  a union of an int and a struct of bit-fields: which of the five have turned,
  the stage, whether the town has gone dark, how many have turned.
- `fishroom.c`, the Fish Room's script: Gill turning with the fish; talking
  to him (he turns his head before he turns round), the camera, the hints, the
  count; the ending.
- `m01.c`, a patch of Palm Brinks: the five, turned by the Whirligig in a
  scene each, staying turned across saves; the town going dark and its music
  changing after three; after the ending, the five standing still, facing the
  Fish Room's door, for good. Palm Brinks' script is within a few hundred
  bytes of the most a map's script may be, so this patch is small: it talks
  through the script's own helpers and reads the counter as plain bits.
- `speech.h`: speech bubbles and camera helpers for the Fish Room.

## In the game

A new game starts in Palm Brinks (the first arrival moves the story on to
when the town's people are out, and enters again).

1. Go through the house door near the start of the street (the Fish Room's).
   Talk to the robed figure under the fish. He gives you a Camera, a
   notebook, a photo album and a Scrap of Metal.
2. Put the Camera in one of Max's active item slots (Items menu), and take a
   photo of the fish: the idea "Endless Turning".
3. In Palm Brinks, photograph a Flag and a Post.
4. Invent: Endless Turning, Flag and Post make the Whirligig (Invention
   menu); make it from the Scrap of Metal. Show it to Gill.
5. Talk to Parn, Sheriff Blinkhorn, Giles, Aegira and Fournet with the
   Whirligig. Each one turns. After the third, the town goes dark.
6. When all five have turned, go back into the Fish Room.

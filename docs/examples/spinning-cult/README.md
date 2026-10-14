# spinning-cult: Brother Gill, made in Blender

The leader of the Spinning Cult, for an eerie mod: a tall, gaunt figure (34
units; Max is about 26) in a long grey-green robe flaring to the floor, a
capelet, long bell sleeves and a deep pointed hood. Inside the hood there is
no face, only black and two small pale eyes set a little too far apart. A
worn fish emblem on his chest, a cord belt, thin pale hands with long
fingers. Everything is made by `make_gill.py`; nothing of the game's is in
this folder.

- `model/gill.gltf` (with `gill.bin` and `gill_skin.png`, one 256x256
  picture) is the character as Blender exports it: an armature of eighteen
  bones (`root`, `hips`, `spine`, `chest`, `neck`, `head`, and per side
  `shoulder`, `upper_arm`, `forearm`, `hand`, `fingers1`, `fingers2`), the
  skinned mesh `robe` (robe, capelet, hood, sleeves, hands, the black void
  in the hood) and a rigid mesh `eyes--c0` on the head bone, whose frame
  name carries the flag `c0` so the game draws the eyes unlit.
- `mod.json` makes it a new pack, `mods/gill/gill.chr`, like the villager
  `p01_01a`'s (see [usage](../../usage.md#mods), "New characters").
- `gill_test.c` adds the start program a new game begins at: it loads the
  pack with `load_chara`, stands Gill in front of the player at Barga's
  Valley's "talk" point and plays each motion in turn, named in a caption;
  (X) plays them again, (O) ends the test.

Motions, at 30 frames a second (step 1.00):

| name | length | what |
|---|---|---|
| `idle` | 4 s, loops | a slow breath; the head tilts to one side and back; the fingers flex |
| `beckon` | 3 s, ends held | the right hand rises towards the viewer, palm up, and the long fingers curl three times |
| `raise` | 4 s, ends held | both arms rise slowly above the head, palms up, the head tipped back; the fingers tremble |
| `spin` | 4 s, loops | one full turn on the spot about the vertical (the `root` bone) |
| `turn_head` | 4 s, loops | the head turns 90 degrees to his left, slowly, holds, and comes back |
| `vanish` | 2 s, ends held | he fades out: the robe first, the eyes last; hidden on the last frame |

`vanish` uses the shape keys build reads as material fades: each mesh has
`c2_fade_0` (material 0's fade, 0 opaque, 1 invisible) and `c2_visible`
(below 1 hidden), shape keys that move nothing, keyed in the action. Play
it with `CHARA_MOTION_HOLD`, or it loops back to visible; any other motion
shows him again.

```bash
c2 build extracted/scripts dc2.iso -o gill-test.iso --mod spinning-cult --mods docs/examples
```

To make the model again (or change it and build again):

```bash
blender --background --factory-startup --python docs/examples/spinning-cult/make_gill.py
```

# Editing scripts

How the decompiled scripts are laid out, how to change code several scripts share, how to change
what a script asks the engine for, and how to handle a map's language variants. A change to a
script is best kept in a mod's patch ([mods](mods.md)); the commands are in
[commands](commands.md). What the C means and the C you can write are in
[writing script code](../coding/README.md): [the language](../coding/language.md),
[the SDK](../coding/sdk.md) and the [natives](../coding/api/README.md). How the game runs the
scripts is in [how scripts run](../game/running.md).

## The decompiled tree

`setup` decompiles every script into `extracted/scripts/`, one `.c` per script, with the code
scripts share in library files under `lib/` (see [decompiling](commands.md#decompiling)).

Folders and files keep the game's own code and gain a readable name after
it: `map/s/s51_raybrandt_castle/s51_1.c`, `dungeon/monster/e33a_golem.c`,
`mainchr/c01_max.c`, and libraries follow their scripts' folders
(`lib/map/s/s51_raybrandt_castle/`). Map folders are named from the disc's
map table and monster AI scripts from its monster table, in the definition
set's language; the set's `paths.json` names the rest (dungeons, the main
characters). The game's codes contain underscores too, so the tree records
the disc path of every renamed file in `unnamed-paths.json`; `build`,
`defs learn`, `xref` and `mod` read it, so a renamed file still replaces
the right disc file. Commands that take a map still take the game's code
(`xref source map s51`); a monster is named by its number, its `MONSTER_`
constant or its name (`xref source monster golem`).

## Editing shared code

Code that several scripts share lives in `lib/`, and an edit there reaches
every script that includes it. A mod's patch changes a library function for
the scripts it targets only: `#pragma target monster golem` with
`#pragma override` before `wait_frames` changes the Golem's copy and leaves
the 35 other monsters that include the same toolkit alone (see
[patches](mods.md#patches)). `#pragma target library lib/...` patches every
script that includes the library instead.

Editing the tree itself still works: `build` notes each edited library that
reaches scripts you did not edit, and `fork` gives some scripts their own
copy of a library first:

```
dc2tk fork extracted/scripts lib/dungeon/monster/monster_ai_full_toolkit.c lib/dungeon/monster/golem_toolkit.c dungeon/monster/e33a_golem.c
```

## Changing what scripts ask the engine for

Much of what a scene does is in its native calls: which monsters
`place_monster` places, which sound `play_se` plays, which item `add_item`
gives. A patch can wrap a native for the scripts it targets, so every call
they make goes through a function of the mod, which calls the native with
`original(...)`, changed or not:

```
dc2tk mod override extracted/scripts rats --map s51 place_monster load_monster_data
```

writes a wrapper of each that passes every call on unchanged, with the
native's doc comment. Edit them to swap the castle fight's soldiers for
rats, wherever s51's scripts and libraries load or place them, without
copying `start_castle_battle`. Calls that leave a native's optional
arguments out still do (the wrapper sees them as 0, and `original()` passes
on only what the call gave), so `place_monster(-1)`, which places a floor's
own monsters at random, goes through the wrapper too and keeps its meaning.
Only the calls change, so the rest of the script compiles to the same bytes,
and another mod can wrap the same native: the later mod's wrapper runs first
(see [patches](mods.md#patches), `#pragma override NATIVE`). Swapped monsters may not end a
fight the original ones end (see
[monsters can start map events](../game/running.md#monsters-can-start-map-events)).

## Language variants

A map script comes in one file per language, `s51_1.c` (English) to `s51_5.c`, and the engine
loads the one for the console's language (see
[language variants](../game/running.md#language-variants)). Edit all five, or put
your code in a library they all include. A patch with `#pragma target map s51` reaches every
variant, and `LANGUAGE` lets one patch carry every language's text (see
[text in each language](mods.md#text-in-each-language)). `xref` lists the variants of one
call once.

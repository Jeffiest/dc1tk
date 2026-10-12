# Definitions

A definition set is a directory of knowledge about a body of compiled
scripts: what its functions are called, what its numbers mean, where its
text lives. The decompiler reads one or more definition sets alongside the
binaries and writes source that uses that knowledge. A definition set
contains no game code and no game text, so it can be published and shared;
anyone with the binaries gets the same readable source.

`definitions/pal/` is the set for the PAL retail disc. The same format
serves other versions of the game (NTSC, Japanese), Dark Cloud 1 once its
engine is supported, and mods.

Sections marked **(planned)** describe parts of the format that are
specified but not built yet; everything else works today.

## Engine and content

Knowledge splits in two, and only the second kind lives in a definition set.

- **Engine knowledge** is true of every script the engine runs: the
  instruction set, the native tables and what each native does, engine
  enums such as motion flags, generic helpers. It lives in `src/` (native
  tables, compiled into the tools) and `sdk/` (the C headers scripts
  include). A mod author writing new code uses the SDK and nothing else.
- **Content knowledge** is true of particular scripts: that this function
  is the baker's dialogue, that flag 520 means the bridge is repaired, which
  file holds a scene's dialogue. It lives in a definition set.

## How definitions find things

A definition set never stores the code it describes, so every entry names
its target by something that can be computed from the binary.

| Target | Identified by |
|---|---|
| Function | fingerprint of its bytecode, with a loose fingerprint as fallback |
| Script | its path on the disc; language variants share an entry |
| String | hash of the string |
| Global | script path and slot |
| Message | message id and the text file the script loads |
| Number | the kind of the place it appears (see value kinds) |
| Library | fingerprint of its member list |
| Line in a function (planned) | function fingerprint and instruction offset |

**Function fingerprints** (`src/c2s/fingerprint.py`) are the foundation, so
their definition is fixed and versioned. The fingerprint is a SHA-1 over:
the function's record (argument count, local count, argument types); its
unused strings; its instructions, with jump targets made relative to the
function, each call target replaced by the callee's fingerprint and each
string push replaced by the string itself; and the native table it runs
against. Padding and position in the file are excluded. Two functions with
the same fingerprint are the same code calling the same code, wherever they
sit and whatever the decompiler makes of them.

The **loose fingerprint** is the same with string contents blanked and
unused strings dropped. The disc's French, German, Italian, Spanish and
Japanese scripts are mostly the English code with different strings, so
they share loose fingerprints, and a name given to one copy reaches all of
them. Lookups use the exact fingerprint first. Then a function takes the
entry of the function at the same position in a language variant of its
script (`X_3.stb` and `X_1.stb` list their functions in the same order),
when both have the same loose fingerprint. Last comes the loose fingerprint
on its own, used only when every entry with that loose fingerprint agrees on
the name: functions that differ only in their strings, such as one talk
handler per villager, share a loose fingerprint without being copies of
each other.

## Layout

```
definitions/pal/
  definitions.json      manifest
  names/                functions: names, parameters, locals, docs
  scripts/              per script: doc comment, global names
  strings/              translations of strings, per language
  include/              content headers included by every script
  notes/                research notes in Markdown
  libraries.json        library paths and docs
  paths.json            readable names for disc folders and scripts the disc
                        does not name (dungeons, the main characters)
```

Every part is optional. A new set can start with just a manifest.

### Manifest

```json
{
  "name": "pal",
  "description": "Dark Chronicle PAL (SCES_511.90) retail scripts",
  "engine": "dc2",
  "extends": [],
  "language": "en",
  "include": [],
  "tables": [
    {"prefix": "dungeon/monster/", "table": "monster"},
    {"prefix": "mainchr/", "table": "action"}
  ],
  "text": {"languages": {"ja": 0, "en": 1, "fr": 2, "de": 3, "it": 4, "es": 5}}
}
```

- `extends` lists definition sets this one builds on, as paths relative to
  this one. A mod's set extends `pal`, so it only adds what the mod
  changes. Sets are applied in order and later ones override earlier ones
  entry by entry. On the command line, `--defs` can be repeated to the same
  effect.
- `language` picks the language for comments: translations and text read
  from the disc.
- `include` lists headers from the set's `include/` directory that every
  decompiled script includes; they are also on the include path when
  compiling.
- `tables` maps disc paths to native tables; the first prefix that matches
  wins and anything else uses `event`. This is content, not engine: it is
  the PAL disc's folder layout.
- `text.languages` maps language names to the digit the disc puts on
  localized files (`e215_1.txt` is English).
- `engine` names the engine the set is for. Only `dc2` exists today.
- `executable` names the disc's boot executable (`extract` saves it beside
  the archive) and where tables inside it live, such as the monster table
  (`{"address": ..., "record_size": 184}`); addresses differ between disc
  versions, so they belong to the set.
- `disc` (planned) names a file of script paths and hashes, so tools can
  tell whether a disc matches the set or has been modified.

### Names

`names/<table>.json` maps function fingerprints to what to call the
function and its variables:

```json
{
  "functions": {
    "9c1f2a7b3e4d5f60": {
      "name": "talk_to_baker",
      "loose": "41d7c09a2e6b3f15",
      "doc": "Baker dialogue; picks lines by quest progress.",
      "params": ["npc", "player"],
      "locals": ["active_chara", "quest_state"]
    }
  }
}
```

Every field is optional. `loose` is the loose fingerprint, so the entry
also names language variants. `params` and `locals` name variables in
declaration order. `doc` is printed as a comment above the function.

Several fingerprints may share a name: versions of the same helper should.
When two different functions with the same name meet in one file, or one
function appears twice, the later one gets a numbered suffix
(`do_nothing_2`); an entry point in that position is named for its program
instead (`on_stopped`, `start` for `MAIN`, `return_zero_for_program_212`).
Library functions keep one name everywhere, so where two libraries that
meet in some script both have a function of the same name, one is numbered
on every disc file. Names that clash with a native, helper, constant or
keyword get a suffix too, as do parameter and local names that repeat or
hide a function the body calls.

Planned fields: `types` gives a variable a value kind or enum, so numbers
assigned to it, compared with it and passed for it print as names;
`returns` overrides the inferred return type; `notes` attaches comments to
lines by instruction offset.

### Scripts

`scripts/<disc path>.json`, one file per script, mirroring the disc:

```json
{
  "doc": "Sindain riverbank: Max meets Lin and learns to fish.",
  "globals": ["rod_given", "visits"]
}
```

`doc` is printed at the top of the file. `globals` names the script's
globals in slot order; a global that included library code also uses keeps
its slot name, so the library compiles in every script that includes it.
An entry also applies to the script's language variants (`X.stb`,
`X_1.stb` ... `X_5.stb`) that have no entry of their own.

### Strings

`strings/<language>.json` maps string hashes (`src/c2s/notes.py`,
`string_key`) to translations, shown as comments beside the code that uses
the string:

```c
set_actor_face(0, "笑顔", "まばたき");  // smile; blink
```

Only short asset names (motions, faces, effects) belong here. Dialogue is
read from the disc.

### Content headers

`include/*.h` are ordinary c2s headers, typically of constants: story
flags, items, characters, maps, sounds. They are included by every
decompiled script and used when compiling, so `set_flag(FLAG_BRIDGE_REPAIRED,
1)` round-trips. Engine constants (motion flags, pad buttons) belong in the
SDK instead.

The PAL set ships `include/pal.h` with `enum flag_story`, a name and a doc
comment for each of the 185 save flags the retail scripts set or read by
number. Because `set_flag` and `get_flag` take a `flag`, and a kind takes
constants from every enum named after it, these print wherever a literal
flag number appears.

### Libraries

Libraries are found automatically and get a default home under `lib/`, in
the folder their users share (`lib/map/map_29.c`). `libraries.json` gives
them a better path and a doc comment, keyed by library fingerprint (a hash
of the native table and the member functions' fingerprints, in order):

```json
{
  "libraries": {
    "5e0c4f1d9a2b7c3e": {"path": "lib/town/talk_v29.c", "doc": "Shared town talk helpers."}
  }
}
```

The path must start with `lib/` and end with `.c`, and uses the game's codes
for folders (`lib/map/s/s51/`); the decompiler adds the same readable names
it gives the scripts' folders (`lib/map/s/s51_raybrandt_castle/`). Function
names inside still come from `names/`, and default function names keep using
the library's automatic name, so moving a library never changes them.

`globals` names the script globals a library's code uses, by their default
slot name:

```json
{"path": "lib/map/s/s05/scene_setup.c", "globals": {"g0": "max_chara_slot", "g1": "partner_chara_slot"}}
```

Library code is one file shared by every script that includes it, so each
of those scripts must declare the slot under the same name. A script's own
entry cannot rename a slot that included code uses; the library's name is
used instead. Where two libraries included by one script disagree about a
slot, or give two slots the same name, those names are dropped and the
slot keeps its default name everywhere the libraries are used.

## Text from the disc

The decompiler comments every message call with the line it shows, read
from the user's disc at decompile time:

```c
play_voice_message(2300010);  // "Hey...isn't that Master Utan?"
```

It needs no per-script configuration. The SDK types the relevant native
parameters with value kinds (`message`, `message_file`, `directory`); the
decompiler follows `change_dir` and `load_message_file` through each
script, localizes each file name to the set's language, and looks message
ids up in those files. A script function that passes one of its parameters
on to a message parameter gets a message parameter too, so calls to helpers
such as `play_voice_message` are commented as well. Library files are
commented where every script that includes them agrees on the text.

The file formats and the language rules are in
[definitions/pal/notes/messages.md](../definitions/pal/notes/messages.md).

## Languages

Comments can be in any of the disc's languages, chosen by the manifest's
`language`.

- Dialogue by message id is read from the disc in that language.
- Short asset names use `strings/<language>.json`.
- Inline dialogue in the French, German, Italian and Spanish scripts lines
  up string for string with the English script (`X_3.stb` with
  `X_1.stb`), so each line gets its sibling in the chosen language as a
  comment, read from the disc. Library files get it where every script that
  includes them agrees.
- The original Japanese scripts do not line up with any English script, so
  their inline dialogue has no English on the disc. The shared set leaves
  those lines untranslated: shipping our own translation would publish the
  game's dialogue. A user can add their own translations in a personal set
  that extends `pal`.

## Value kinds

Numbers and strings mean different things in different places. The SDK
declares kinds with `typedef` and types native parameters with them:

```c
typedef any message;
typedef string message_file;

extern void show_message(int mes, message text, OPTIONAL(int *char_count)) NATIVE(192);
```

A kind reads as its base type, so C tools and the compiler treat it
normally; the decompiler uses it to know what a value is.

| Kind | Shown as |
|---|---|
| `message`, `message_file`, `directory` | dialogue comments (above) |
| `item_id` | `ITEM_` constant named from `menu/cfg7/comdatmes<L>.cfg` |
| `monster_kind` | `MONSTER_` constant named from the executable's monster table |
| `map_id` | comment with the place name from `map/map<L>.cfg` |
| `npc_id` | `NPC_` constant named from `npc<L>.cfg` |
| `chara_pack` | comment with the name of the character or monster whose model pack it is, from `npc<L>.cfg` and the monster table; other file names are glossed in `strings/<language>.json` |
| `flag` | a constant from any enum named `flag` or `flag_*` |
| an enum type | a constant from that enum (`enum chara_motion_flag flags`) |

A kind takes constants from every enum that shares its name, so a content
header can add `enum flag_story { ... }` and the SDK's engine flags and the
story flags both print as names.

Kinds spread into script functions. A function that passes its parameter
straight to a kinded parameter gets that kind for the parameter, repeated
until nothing changes, so literals passed to `give_item_to_player(0x51)`
are named the same way as literals passed to the native it wraps.

Names that are the game's own text (items, monsters, NPCs) cannot ship in a
definition set, so decompiling a disc generates them: the decompiler writes
`include/disc.h` into the source tree with `enum item_id`, `enum
monster_kind` and `enum npc_id`, read from that disc in the set's language,
and every script includes it. Literals of those kinds then print as
constants (`place_monster(MONSTER_SEWER_RAT, ...)`, `add_item(ITEM_ROYAL_SWORD)`);
`compile` and `build` find the header through the source tree's `include/`
folder. Without a disc the same kinds fall back to comments.

Planned: scene character slots (which pack a slot holds) and `sound`.

## Rules that keep it safe

- Definitions only change how code is shown, never what it compiles to.
  `verify` decompiles with every definition applied and requires the same
  bytes back.
- A constant replaces a number only where the number was one literal, so
  the bytecode stays identical.
- Names that clash fall back to suffixed or default names rather than
  produce broken code.

## Workflow

The easiest way to write definitions is to edit decompiled code:

1. `cli.py setup dc2.iso decompiled` decompiles the disc with the shipped sets.
2. Rename functions, parameters, locals and globals, and add doc comments
   above functions and at the top of scripts, in `decompiled/`.
3. `cli.py defs learn decompiled definitions/pal` compiles the edited files,
   fingerprints each function, and records every name, doc and global name
   that differs from the decompiler's defaults. Because names do not change
   bytecode, any edit that still compiles to the same function teaches the
   definition set.
4. Commit the definition set.

To see where things stand:

- `cli.py defs check extracted` reports how much of the disc the set
  names, per native table, by function and by distinct class, and entries
  that match nothing on the disc.
- `cli.py defs todo extracted` lists what is still unnamed, most called
  first, with a file and current name to look at.

## What people will want to do

- Name and document functions, parameters, locals, globals and scripts.
- Say what numbers mean: flags, items, characters, maps, sounds, menus.
- Read English beside Japanese asset names and beside every line of
  dialogue, in the language of their choice.
- Organise output: library names and folders.
- Annotate behaviour: notes on specific lines, known bugs, unused code.
- Target another version: the NTSC or Japanese disc, Dark Cloud 1.
- Ship a mod with a set that extends `pal`, so others can decompile it.
- Check a set against a disc and see how much is still unknown.

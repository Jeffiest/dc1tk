# Editing sounds

How a mod replaces and adds sound effects: editing a bank as WAVs, naming sounds by id, the
names a mod's patches use for its sounds, what a mod must keep consistent, and how to check it.
The bank format and how the game plays an entry are in [the sound bank format](../game/sounds.md);
the code is described in [src/docs/assets/sounds.md](../../src/docs/assets/sounds.md). Music and
ambient loops, which are sequences inside banks, are in [editing music](music.md).

## Sound effects as WAV

A sound bank (`.snd`) holds the entries `play_se` plays by number. `mod asset ISO NAME
snd2/sp/sp_004.snd --as wav` writes a folder `sp_004.snd.wav/` with one WAV per entry that plays a
single sound (`001.wav`). Replace the ones you want with any WAV (any rate up to 48000 Hz, mixed
to mono; a `smpl` loop makes it loop), and add entries by numbering on from the last
(`005_beep.wav`, the name after the number goes into the bank's table). A WAV with a name gives
the mod's patches a constant for its entry, named as `pal.h` names the game's sounds:
`005_beep.wav` in `sp_004.snd.wav/` is `SE_SP_004_BEEP` (5), so a patch can `play_se(bank,
SE_SP_004_BEEP)`; `build` and `mod check` list them, and refuse a name `pal.h` gives another
entry. An entry that plays a sequence (a tune of the bank's instruments, edited as MIDI: see
[editing music](music.md)) is not exported; a WAV given for it replaces the tune with that one
sound. A changed entry overwrites its samples when no other entry plays them, so the bank stays
the same size or shrinks, and otherwise gets samples of its own. Unedited WAVs build back to the
disc's own bytes.

The game reads each bank from one place per copy on the disc, the file `build` rewrites (see
[which copy the game reads](../game/sounds.md#which-copy-the-game-reads)), and the asset tree lists
them all. Where copies of a bank's name differ (the menu packs' `SP_001`, `SP_002`, `SP_003`; the
two `SY_000`; `BG_193` in the fishing packs), each is a bank of its own (`bank:SP_001@inv0_bg`),
but an entry that plays the same sound in several of them (the same sequence and the same
samples, rate, loop, key and base note) is one sound with all their places, so a mod replacing
`sound:SP_001/0` or a bank's folder reaches every copy; the other versions' numbers
(`sound:SP_001@inv0_bg/0`) are aliases, and its WAV is in the first version's folder only
(`sounds/SP_001/`), so the tree has one file to edit for it.

## Sounds by id

A mod can name the sounds it replaces and adds by id in `mod.json` instead of shipping a bank's
folder. A `replace` line can set the entry's volume too, and an `add` line names the new entry
and the bank it goes into:

```json
"replace": {
  "sound:SE_EV_000_130_GRIFFON_BREAK_WINDOW": {"file": "sounds/boom.wav", "volume": 90}
},
"add": {
  "sound:SE_MOD_BEEP": {"file": "sounds/beep.wav", "bank": "SP_004"}
}
```

The calculator example ([`docs/coding/examples/calculator`](../coding/examples/calculator/)) adds
six sounds to `SP_042` this way (`sound:SE_MOD_CALC_KEY`, ...), loads the bank with
`load_sound(SND_PORT_MENU, "snd2/sp/sp_042.snd", &sound)` and plays them with
`play_se(sound, SE_MOD_CALC_KEY)`. How ids work is in [mods](mods.md).

## What a WAV may be

- PCM 8/16/24/32-bit or float, any channel count (averaged to mono), any rate (kept, or
  resampled; above 48000 is resampled to 48000).
- A `smpl` chunk loop is honoured: the start is moved to a block boundary by leading silence, and
  the loop body is made a whole number of blocks by repeating it (when 4 copies or fewer suffice)
  or stretching it by resampling (pitch change under 28 / body length). Without a loop point, a
  sound replacing a looped VAG loops whole.

## How `build` changes a bank

- An entry the folder leaves out stays as it is; a WAV that is the entry's own sound changes only
  its volume (0 to 127, when given); a KeyOn entry whose samples no other entry plays is
  overwritten in place, any other (shared samples, a sequence entry) gets samples of its own, and
  the samples it played before are cut to a silent stub when nothing plays them any more (a
  sequence's notes included).
- A WAV numbered one past the last entry adds one (named by the file, else `MOD_SE`; category
  `MOD`, volume 100, key 60); a number further on is refused, as are a WAV not named by its
  number and two WAVs for one entry.
- New sounds copy the replaced entry's first voice (ADSR, priority, group, SPU attributes,
  program LFO and volume) but set base note = key and every transpose/detune to 0, so the WAV
  plays at its own rate (assuming the hardware synth's pitch is rate x 2^((key + transposes -
  base note)/12), as documented by Sony; not measured on hardware).

## Names for sounds

- The script compiler names entries from `pal.h` enums `se_<bank>_pal`. A mod's new entries get
  names the same way: an `add` line names its entry (`sound:SE_MOD_BEEP`), and a WAV named after
  its number and a name in a bank's folder under `disc/` or of a `file:` line (`005_beep.wav` in
  `sp_004.snd.wav/`) gives `SE_SP_004_BEEP = 5`, which the mod's patches compile with and `mod
  check` prints. A name `pal.h` gives another number is refused.
- Decompiled scripts show entry numbers by name: the entry of `play_se`, `stop_se`, `pause_se`,
  `set_se_volume`, `set_se_pan`, `set_se_pitch`, `get_se_status`, `play_actor_se` and
  `play_actor_se_after` is written as a constant of the bank's enum (`se_<bank>`,
  `se_<bank>_pal`) when the decompiler knows the bank the id was read from: the system bank, a
  scene's base bank (`SE_BANK_BASE`, 1-3: `base_se`) or battle bank (`SE_BANK_BATTLE`, 4:
  `battle_se`), a bank `load_sound` loaded (a path names it by its file stem, a sound set number
  by its event bank in the definition set), or the event port's. It follows ids through
  variables, globals, parameters and helpers' out-parameters across a program's functions, a few
  rounds settling it. The event port is followed like a variable every function shares: a
  function starts with the bank its callers leave there, and an event the engine starts finds
  the port's resting bank, the one every event that loads another bank puts back before it ends
  (a dungeon floor's battle music, `EV_200_000`; a function that reads the battle music's volume
  and whose bank is unknown names entries by `se_event_port`). A library's functions take the
  banks every script that includes them agrees on.

## What a mod must keep consistent

The engine sets these limits (see [the sound bank format](../game/sounds.md)):

- A bank addresses programs 0..127 only, and `build` gives new sounds programs in that range.
- An entry's name and category are at most 63 bytes of cp932; longer columns overflow the
  engine's stack buffers.
- A port holds 10 sequences; keep a bank's `.sq` members to 10.
- Keep a bank's sample data near its original size: field, event and monster banks share one
  region of sound memory with almost nothing to spare, and a bank that outgrows it overwrites
  the system sounds (`build` warns; see [the memory budgets](memory.md)). Keep a modded bank's
  `.bd` at most its original size unless you know the region has room.
- Each added entry costs 12 bytes of its layer's memory stack for the tables; `build` warns when
  a changed bank's tables outgrow its layer's stack.
- The whole `.snd` is first read into a shared buffer of unknown size: stay within the size of
  the largest bank loaded the same way.

## Checking

- Unedited WAVs build back to the disc's own bytes.
- `build` and `mod check` list the constants a mod's named WAVs give, and refuse a name `pal.h`
  gives another entry (see [commands](commands.md)).
- `build` checks each changed bank against its region of sound memory and every sound set that
  loads it, and warns when its tables outgrow its stack.

# Editing sounds

This guide explains how to replace and add sound effects in a mod. You can edit a sound bank as WAV files, replace or add individual sounds by asset ID in `mod.json`, and play new sounds from script patches by name.

For details on the sound bank binary format and how the engine plays sound entries, see [the sound bank format](../internals/sounds.md). For implementation details, see [src/docs/assets/sounds.md](../../src/docs/assets/sounds.md). For background music and ambient loops (which are MIDI-like sequences inside sound banks), see [editing music](music.md).

## Sound effects as WAV

A sound bank (`.snd`) contains the numbered sound entries that `play_se` plays.

To edit a sound bank as a folder of WAV files:

1. Export the sound bank as WAV files:
   ```sh
   dc2tk mod asset ISO NAME snd2/sp/sp_004.snd --as wav
   ```
   This command creates a folder named `sp_004.snd.wav/` with one WAV file for each single-sound entry (for example, `001.wav`).
2. Replace any WAV file with your own WAV file (up to 48000 Hz, mixed to mono; include a `smpl` loop chunk if the sound should loop).
3. To append a new sound entry, add a WAV file numbered immediately after the last entry (for example, `005_beep.wav`). The text after the underscore becomes the entry's name in the bank table.
4. Delete any WAV files that you did not change, or leave them in the folder. Unedited WAV files build back to the exact bytes from the disc.
5. Run `dc2tk build`.

### Named WAV files and script constants

When you give a WAV file a name suffix, `dc2tk` generates a C constant for your mod's script patches using the same naming style as `pal.h`. For example, `005_beep.wav` inside `sp_004.snd.wav/` defines `SE_SP_004_BEEP` with the value `5`. Your script patch can then call:

```c
play_se(bank, SE_SP_004_BEEP);
```

Both `dc2tk build` and `dc2tk mod check` list these generated constants. They reject any name that `pal.h` already assigns to a different entry number.

### Sequence entries and sample reuse

- **Sequence entries**: Entries that play a multi-note sequence (a tune played on the bank's instruments, edited as MIDI; see [editing music](music.md)) are not exported as WAV files. If you put a WAV file in the folder for a sequence entry's number, `dc2tk build` replaces that tune with your single WAV sound.
- **Sample reuse**: When you change an entry whose audio samples are not used by any other entry, `dc2tk build` overwrites those samples in place so the bank stays the same size or shrinks. If another entry shares those samples, `dc2tk build` allocates new samples for the edited entry.

### Multiple copies of a sound bank on the disc

The game reads each sound bank from one location per copy on the disc, and `dc2tk build` rewrites that file (see [which copy the game reads](../internals/sounds.md#which-copy-the-game-reads)). The exported asset tree lists all banks.

When copies of a bank share a filename but have different contents (such as `SP_001`, `SP_002`, and `SP_003` in the menu packs, the two `SY_000` files, and `BG_193` in the fishing packs), each distinct version gets its own bank ID (for example, `bank:SP_001@inv0_bg`).

If an entry plays the exact same sound across several versions of a bank (same sequence, samples, sample rate, loop, key, and base note), `dc2tk` treats it as a single sound asset across all those locations:

- Replacing `sound:SP_001/0` or editing the bank folder updates every copy on the disc.
- The version-specific IDs (such as `sound:SP_001@inv0_bg/0`) work as aliases.
- The asset tree exports the WAV file into the first version's folder only (`sounds/SP_001/`), so you only need to edit one file.

## Sounds by id

Instead of exporting a full sound bank folder, you can replace or add individual sounds by their asset ID in `mod.json`:

- In `"replace"`, specify the replacement WAV file and an optional `"volume"` (`0` to `127`).
- In `"add"`, give the new sound's constant name, the WAV file path, and the target `"bank"`.

```json
"replace": {
  "sound:SE_EV_000_130_GRIFFON_BREAK_WINDOW": {"file": "sounds/boom.wav", "volume": 90}
},
"add": {
  "sound:SE_MOD_BEEP": {"file": "sounds/beep.wav", "bank": "SP_004"}
}
```

The calculator example ([`docs/scripting/examples/calculator`](../scripting/examples/calculator/)) adds six sounds to `SP_042` using `"add"` (`sound:SE_MOD_CALC_KEY`, and others), loads the bank with `load_sound(SND_PORT_MENU, "snd2/sp/sp_042.snd", &sound)`, and plays the sounds with `play_se(sound, SE_MOD_CALC_KEY)`. For more information on asset IDs, see [mods](mods.md).

## What a WAV may be

- **Encoding and channels**: 8-bit, 16-bit, 24-bit, or 32-bit integer PCM, or 32-bit float PCM, with any number of channels (`dc2tk build` averages multi-channel files to mono).
- **Sample rate**: Any sample rate. `dc2tk build` preserves sample rates up to 48000 Hz and resamples rates above 48000 Hz down to 48000 Hz.
- **Loop points**: `dc2tk build` honors a `smpl` chunk loop in the WAV file:
  - It aligns the loop start to an ADPCM block boundary by inserting leading silence.
  - It makes the loop body a whole number of 28-sample blocks either by repeating the loop body (when 4 or fewer copies suffice) or by stretching it via resampling (which changes pitch by less than $28 / \text{body length}$).
  - If a WAV file has no loop points and replaces a looped VAG sample, `dc2tk build` loops the entire sound.

## How `build` changes a bank

- **Unchanged and volume-only edits**: Any entry omitted from the WAV folder stays untouched. If a WAV file contains the entry's original audio data, `dc2tk build` changes only its volume (`0` to `127`, when specified).
- **Sample allocation and cleanup**:
  - If a `KeyOn` entry's samples are not shared with any other entry, `dc2tk build` overwrites those samples in place.
  - If an entry shares samples or is a sequence entry, `dc2tk build` allocates new sample data for it.
  - If the old samples are no longer used by any entry or sequence note, `dc2tk build` cuts those old samples down to a silent stub.
- **Adding entries**: A WAV file numbered 1 past the last entry adds a new sound entry (named after the filename suffix, or `MOD_SE` if unnamed, with category `MOD`, volume `100`, and key `60`). `dc2tk build` rejects WAV files numbered more than 1 past the end, WAV files without a leading entry number, and duplicate WAV files for the same entry number.
- **Voice attributes and pitch**: New sounds copy the first voice's settings from the replaced entry (ADSR envelope, priority, group, SPU attributes, program LFO, and volume). However, `dc2tk build` sets `base note = key` and sets all transpose and detune values to `0`. This plays the WAV at its native sample rate (following Sony's hardware synth pitch formula: $\text{rate} \times 2^{(\text{key} + \text{transposes} - \text{base note})/12}$; not measured on hardware).

## Names for sounds

- **Compiler constants**: The script compiler gets sound entry names from the `se_<bank>_pal` enums in `pal.h`. New entries in a mod get names the same way:
  - An `"add"` entry in `mod.json` defines a constant with its ID name (for example, `sound:SE_MOD_BEEP` defines `SE_MOD_BEEP`).
  - A numbered, named WAV file in a bank folder under `disc/` or in a `file:` entry (for example, `005_beep.wav` in `sp_004.snd.wav/`) defines `SE_SP_004_BEEP = 5`.
  - Your mod's script patches can use these constants directly, and `dc2tk mod check` prints them. `dc2tk` rejects any name that `pal.h` already gives to a different entry number.
- **Decompiled scripts**: Decompiled scripts write sound entry numbers as constants from the bank's enum (`se_<bank>` or `se_<bank>_pal`) in calls to `play_se`, `stop_se`, `pause_se`, `set_se_volume`, `set_se_pan`, `set_se_pitch`, `get_se_status`, `play_actor_se`, and `play_actor_se_after` whenever the decompiler knows which bank the sound handle came from:
  - The system bank
  - A scene's base bank (`SE_BANK_BASE`, slots 1–3: `base_se`) or battle bank (`SE_BANK_BATTLE`, slot 4: `battle_se`)
  - A bank loaded by `load_sound` (identified by its file stem when loaded by path, or by its event bank in the definition set when loaded by sound set number)
  - The event sound port
- **Cross-function tracking**: The decompiler traces sound handles through local variables, globals, parameters, and helper out-parameters across a script's functions over several passes. It treats the event port like a shared variable across functions: a function starts with whichever bank its callers leave in the port, and an engine-started event finds the port's resting bank (the bank that temporary event loads restore before ending, such as a dungeon floor's battle music `EV_200_000`). If a function uses the event port (for example, reading the battle music volume) and the active bank is unknown, the decompiler names entries using `se_event_port`. Functions in shared header libraries use a bank enum only when every script that includes the library agrees on that bank.

## What a mod must keep consistent

The game engine enforces the following limits (see [the sound bank format](../internals/sounds.md)):

- **Program numbers (`0..127`)**: A sound bank can address only programs `0` to `127`. `dc2tk build` assigns new sounds to programs within this range.
- **Name and category length**: An entry's name and category can each be at most 63 bytes in `cp932`. Longer strings overflow the engine's stack buffers.
- **Sequence limit**: A sound port holds at most 10 sequences. Keep at most 10 `.sq` members in a single bank.
- **Sample memory (`.bd` size)**: Keep a bank's `.bd` sample data at or below its original size unless you know the SPU2 memory region has free space. Field, event, and monster banks share one region of SPU2 sound memory with almost no spare room. A bank that outgrows this region overwrites the system sounds (`dc2tk build` warns when this happens; see [the memory budgets](memory.md)).
- **Table memory**: Each added sound entry uses 12 bytes on its layer's memory stack for runtime tables. `dc2tk build` warns when a modified bank's tables exceed its layer's stack budget.
- **Read buffer**: The engine first reads the entire `.snd` archive into a shared memory buffer of unknown size. Keep the total `.snd` size within the size of the largest bank loaded in the same way.

## Checking

- Unedited WAV files build back to the exact bytes from the disc.
- `dc2tk build` and `dc2tk mod check` list all C constants generated from your mod's named WAV files and reject any name that `pal.h` assigns to another entry (see [commands](commands.md)).
- `dc2tk build` checks each modified bank against its SPU2 sound memory region and every sound set that loads it, and warns if the bank's tables outgrow its memory stack.

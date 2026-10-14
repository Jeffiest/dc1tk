# Sound and music numbers

What the numbers in the scripts' sound calls select, from the engine
source (scenesnd.cpp and the native implementations) and the files on the
PAL disc. Paths are inside the extracted disc.

## Music

- `load_bgm(n)` and `load_bgm_pack(n)` open `snd2/bgm/BG_%03d.snd`
  (`CScene::GetBgmFile`). `get_map_default_bgm`, `get_bgm_no` and
  `get_saved_bgm_no` return the same numbers.
- `play_bgm(n)` and `stop_bgm(n)` take a track inside the loaded file, not
  a music number. Scripts use 0 and 1; only BG_031 (the butterfly boss,
  joined and split), BG_060 (game over, looping and once) and BG_193
  (peace, fishing battle) have a track 1.
- Each `BG_nnn.snd.d/BG_nnn.cfg` starts with the file's title. The 73 files
  are named from those titles in `enum bgm_file_pal` (`include/pal.h`);
  `map/map1.cfg` and the default music of each map's sound set confirm the
  place names. Seven names are partly guesses and say so.
- BG_037 (`BGM_BEFORE_BOSS_FLOOR`) is titled as a boss theme, but every
  dungeon plays it on the floor before the boss.
- Dungeon battle music is not a BG file: scripts load
  `snd2/event/EV_200_000.snd` by path (EV_200_010 in the Moon Flower
  Palace).

## Sound sets

- `load_sound_set(n)` loads row n of `snd2/MAP_index.txt`
  (`CScene::LoadSndFileInfo`; `map/mapN.cfg` gives each map its row). A row
  names the default music, the base, battle, ambient and object banks, an
  event bank, reverb and a description starting with a story scene code
  (`283_A`).
- `load_sound(port, n)` with a number loads only that row's event bank,
  `snd2/event/EV_xxx_yyy.snd`.
- `enum sound_set_pal` names all 394 rows for their area, scene code or
  kind (`ar` area, `bt` battle) and number. Scripts use 260 of them, and
  one id, 6127, has no row.

## Sound effects

`play_se(sound_id, n)` plays entry n of the bank `sound_id` was loaded
from: a path given to `load_sound`, a sound set's event bank,
`get_sound_id(SE_BANK_...)` (the map's ambient, base, battle or object
bank) or `get_system_sound_id()` (`SY_000`, also `play_system_se(n)`).
Every bank is a pack whose cfg names its entries, in Japanese: 840 banks,
8,050 entries. All map base banks share one layout (`BS_999`). Following
the variable from the load to `play_se` names 8,846 of the 8,858 entries;
the other 12 are in two helpers nothing calls (`slide_train_to_stop` in
s02, `run_voice_with_machine_noise` in s06), so no bank reaches them.
Monster and character sounds use the owner's own bank (`snd2/mon/EN_nnn`,
`snd2/chara/CH_nnn`).

The event port (4) holds one bank at a time. In dungeons the floor scripts
load `EV_200_000` there (`EV_200_010` is the Moon Flower Palace's) and every
scene that loads another event bank calls `load_event_sound` before it ends,
so the events the engine starts find the battle music there. The engine
plays entry 0 of whatever bank the port holds as the battle music, fading
it in near monsters, while `DNG_PAUSE_BATTLE_MUSIC` is clear and the floor
is not a boss floor (`DngBattleBgm`, dng_event.cpp); the game over and
victory scenes save its state and volume (`get_battle_bgm_volume`) and
stop and restore entry 0. `SE_EVENT_PORT_BATTLE_MUSIC` names that entry
where the scripts that share such code disagree on the bank.

## Voices and effects

- `open_stream_wav(n)` opens `%07d.wav` inside `SOUND.DAT`, indexed by
  `SOUND.HD3`; nothing names them, but the number reads as event x 10000
  plus line x 10, and the decompiler already comments each call with the
  spoken line.
- Effects are named by strings, not numbers.

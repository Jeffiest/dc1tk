# Character slots

Who fills the character slots that event scripts address by number, from
the engine source (charasetup.cpp, scenevillager.cpp, villagermngr.cpp,
editloop.cpp, dng_main.hpp) and the PAL disc's cfg files.

## Towns, fields and interiors

| Slots | Filled by |
|---|---|
| 0 to 5 | the controlled character: Max's body, melee weapon, gun and hat, or the Ridepod's parts (`SetupMainUnit`, `SetupRobo`) |
| 6, 7 | reserved, not loaded |
| 8 to 23 | villagers of the main map |
| 24 to 31 | villagers of the sub map or interior (Palm Brinks areas m02 to m05 load as a sub map of m01) |
| 120, 121 | the time gate marker and its base |
| 122, 123 | the save point and its book |
| the rest | the map's own scripts |

Maps do not list characters: `m01.cfg` has an `NPC_PLACE` block, but the
map cfg parser has no such tag and skips it.

## Villager slots

A villager's slot is the base (8 or 24) plus its position in the load
list: villager ids in ascending order whose schedule in `npc_place4.cfg`
and `place.cfg` puts them on this map for the current story progress and
time of day (night is 21:00 to 06:00). A villager with two places on a
map takes two slots, one whose model fails to load takes none, and party
members who have joined count as id + 1000. So a slot holds different
villagers at different points of the story, and the scripts find
villagers by id (`find_villager_slot(50)`) rather than by slot.

The story progress here is the chapter (`PROG_INFO` in `npc_place4.cfg`,
1 to 10, and 100 for the demo): `CVillagerMngr::GetAppearVlgr` takes, for
each villager, the last schedule point at or before the current chapter
that is that chapter or marked "以後" (from then on), its place for the time
of day in each of up to four alternatives, and `place.cfg` gives the
place's map number (the row of `map/map1.cfg`). `LoadVillager` fills slots
from 8 for the map the game jumps to; an interior (type 4 in `map1.cfg`)
fills them from 24 (`LoadSubVillager`).

Only Palm Brinks, its interiors, Jurak Mall, the Channel Entrance and
Pau's house have villagers. Most interiors always hold the same one
(Cedric in his shop at 24, Aunt Polly in the bakery, Mayor Need in his
office), which `disc.h` names per folder (`enum villager_slots_i01_h01`)
where the folder's scripts never load that slot themselves; slots whose
villager changes with the chapter (the town's passers-by) stay numbers,
with a comment naming the candidates where a script addresses one. Scripts
address villager slots by number almost only in Cedric's shop; elsewhere
the numbered slots 8 and up hold the scripts' own characters, which print
as the character a load puts there where it reaches the use.

## Dungeons

0 to 7 the party, 8 to 23 event characters the scripts load, 24 and up
the floor's monsters.

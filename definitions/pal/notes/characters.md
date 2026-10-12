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

## Dungeons

0 to 7 the party, 8 to 23 event characters the scripts load, 24 and up
the floor's monsters.

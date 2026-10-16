# Memory budgets: the code

Where the memory check `build` runs is implemented, and how its figures for the disc are kept
true. The budgets themselves are in [memory budgets](../../../docs/internals/memory.md); what `build`
reports is in [staying within memory](../../../docs/guide/memory.md).

## Modules

`dc2tk_script::memory` (`src/crates/dc2tk-script/src/memory/`) checks the loads a mod changes
against the budgets, a file each: the disc (`disc.rs`), stacks (`stack.rs`), MDS models
(`mds.rs`), character packs (`chara.rs`), maps (`maps.rs`), texture blocks (`vram.rs`), sound
memory (`spu.rs`) and the check (`check.rs`); its tests are in `tests.rs`.

The map table's use of `MapNameBuff` is `dc2tk_script::maptable::buffer_use`, which `build` uses
to refuse new maps' rows past the buffer.

## The disc's largest maps

The largest estimated map memory of each kind on the disc (town g01 9,471,152 bytes, interior
i04h01a 3,752,880, dungeon d02f01 6,911,504) is kept as `memory::RETAIL_TOWN_MAP`,
`RETAIL_INTERIOR_MAP` and `RETAIL_DUNGEON_MAP`, which `build` compares a changed map with. The
corpus test `corpus_retail_fits_the_budgets` fails when the disc no longer gives them, so
re-measure them there when the estimate or the map list changes.

## Not yet

Not built into the check: the monster and event banks' combinations with each sound set
(measured by following the scripts), the EE tables of event and monster banks, the Ridepod's and
the monster form's stacks, the shared pool's free space, and the menu and dungeon message files
(see [staying within memory](../../../docs/guide/memory.md)).

// Story and content flags of the PAL retail scripts: bit flags in the save data, read with
// get_flag and written with set_flag.
#ifndef PAL_H
#define PAL_H

enum flag_story {
    // Set in Palm Brinks when Max uses the Circus Ticket and the circus event starts, and cleared
    // when the ticket is handed out again; the ticket handler reads it.
    FLAG_CIRCUS_TICKET_USED = 1,
    // Set when Max talks to the first Palm Brinks bystander on circus night without a ticket; the
    // second bystander reads it with flag 3 to start the next event, and dungeon d01 reuses it as a
    // floor 1 first-visit mark.
    FLAG_ASKED_BYSTANDER_A = 2,
    // Set when Max talks to the second Palm Brinks bystander on circus night without a ticket; read
    // together with flag 2 to start the next circus-night event.
    FLAG_ASKED_BYSTANDER_B = 3,
    // Set when the opening sends Max to his room and on to the Repair Shop; the opening town script
    // reads it to run the Donny scene and clears it when Max receives the Classic Gun and Help
    // Receiver.
    FLAG_OPENING_DONNY_PENDING = 4,
    // Cleared by the Underground Channel arena entry script (s06 event 500); nothing reads it.
    FLAG_SCENE_S06_ENTRY_RESET = 5,
    // Set when Max loses in the Underground Channel arena with the Ridepod before the boss is
    // cleared, and when Cedric's house starts event 520; Cedric's house talk reads it to start that
    // event.
    FLAG_RIDEPOD_RESCUE_PENDING = 6,
    // Set when the Underground Channel arena scenes (events 108 and 502) end and Max is back on
    // foot; Cedric's house, the arena and Palm Brinks scripts read it to pick their state.
    FLAG_CHANNEL_ARENA_CLEARED = 7,
    // Set the first time Max takes the arena exit door toward Palm Brinks Square; the exit door
    // script reads it to play the transition only once.
    FLAG_CHANNEL_EXIT_USED = 8,
    // Set by the Mayor's Office visit on dungeon d01 floor 5 and by the train departure event; the
    // station map and chapter 1 villagers read it to show the train as running.
    FLAG_TRAIN_RUNNING = 9,
    // Set on first entry to the Underground Channel boss room d01b01, which seals the door and
    // shows the map piece; the same script reads it to skip that setup afterward.
    FLAG_D01B01_INTRO_STARTED = 10,
    // Read by Cedric's house event 520; when set Cedric takes the Energy Pack, the delivery flag is
    // set and the lesson ends. No script sets it.
    FLAG_CEDRIC_HOUSE_ENERGY_PACK_BRANCH = 13,
    // Set when Max leaves Cedric's house after Cedric has given him the Energy Pack job; Cedric's talk
    // reads it to play the follow-up event 510.
    FLAG_ENERGY_PACK_QUEST_STARTED = 17,
    // Set in Cedric's house event 500 when Cedric gives Max the Camera and Inside Scoop Memo; Cedric's
    // house scripts read it before setting flag 17.
    FLAG_CEDRIC_JOB_GIVEN = 18,
    // Set when Max has the Energy Pack invention idea and Cedric pays 400 Gilda; Cedric's talk then
    // points Max to the Item Shop for the materials.
    FLAG_ENERGY_PACK_IDEA_FOUND = 19,
    // Set when Max brings Cedric the Energy Pack; Cedric's house shows the pack on the Ridepod, and
    // Cedric's talk reads it to start his follow-up event.
    FLAG_ENERGY_PACK_DELIVERED = 20,
    // Set at the start of the Energy Pack job and cleared when Steve receives the pack; Palm Brinks
    // m01 reads it to enable the invention spot.
    FLAG_ENERGY_PACK_INVENT_SPOT = 21,
    // Read by the i02h03 villager talk during the Claire scene; when set, Erik asks Maximilian to
    // come to the usual place. No script sets it.
    FLAG_I02H03_ERIK_CLAIRE_INVITE = 22,
    // Set in dungeon d02 rooms b01 and b03 when Max uses the Lafrescia Seed on the sapling; the
    // room setup and the sapling hint read it.
    FLAG_LAFRESCIA_SEED_PLANTED = 23,
    // Set when Max uses the Sundrop on the Lafrescia sapling; room b01 setup and the floor 15 gate
    // in d02 read it before the boss floor opens.
    FLAG_LAFRESCIA_SUNDROP_USED = 24,
    // Set when the Ridepod is added as party member 2 in Cedric's house; the s06 arena entry script
    // reads it.
    FLAG_RIDEPOD_JOINED_PARTY = 25,
    // Set in the s05 forest when Max uses the Grape Juice and the wine appears; forest character
    // setup reads it, and the s05 story reset clears it.
    FLAG_GRAPE_JUICE_GIVEN = 28,
    // Set when Max stays in Cedric's house after event 520 instead of returning to the Underground
    // Channel; nothing reads it.
    FLAG_CEDRIC_HOUSE_E108C_READY = 29,
    // Cleared at the start of the s02 train arrival scene and set later in it; the scene reads it
    // to decide whether to tilt the slope parts.
    FLAG_SCENE_S02_TRAIN_SLOPE = 30,
    // Set whenever Max picks up a Geostone in a dungeon; the s02 map start reads it with the item
    // board flag to play the first Geostone scene.
    FLAG_GEOSTONE_PICKED_UP = 31,
    // Set when a Palm Brinks villager first shows the first-person view hint; the same talk handler
    // reads it to show the hint once.
    FLAG_FIRST_PERSON_HINT_SHOWN = 32,
    // Set at the start of the Sindain g01 scene where Max and Monica discuss the Geostone and
    // bringing someone to the village; nothing reads it.
    FLAG_SINDAIN_GEOSTONE_SCENE_START = 34,
    // Set when Max receives the Gold Paint in the f01 map; the same handler reads it to give it
    // only once.
    FLAG_GOLD_PAINT_RECEIVED = 36,
    // Set when Max receives 20 Purple Paint in the i04h03 room; the room reads it to give it once
    // and to hide the part that held it.
    FLAG_PURPLE_PAINT_RECEIVED = 37,
    // Set when Max receives the Photo Album in the i04h01 house; the house reads it to give it once
    // and to set up the quest 18 scene.
    FLAG_PHOTO_ALBUM_RECEIVED = 38,
    // Set while the opening town or the station map is set up and cleared at Palm Brinks m01 map
    // start; m01 reads it to choose its arrival camera after coming from map 34.
    FLAG_STATION_ARRIVAL_PENDING = 40,
    // Set when Max wins the fish race in i03h04 and gets the prize; the fish woman reads it to
    // change her lines.
    FLAG_FISH_RACE_WON = 41,
    // Set by story program 507, which plays the same Sindain Geostone scene as flag 34; the
    // gardener reads it to talk differently after it.
    FLAG_GEOSTONE_PLAN_SCENE_DONE = 42,
    // Set when event 372 or the Griffon flashback scene switches the music; the scene reads it to
    // restore the map music at its end.
    FLAG_EVENT_BGM_CHANGED = 43,
    // Set the first time Max talks to the baker's apprentice Valle, who minds the bakery for Aunt
    // Polly; the shop talk reads it to greet only once.
    FLAG_MET_VALLE = 45,
    // Set the first time Max talks to Milane's little sister Chitral at the weapon shop; the shop
    // talk reads it to greet only once.
    FLAG_MET_CHITRAL = 46,
    // Set the first time Max talks to Lawtin, the friend of Priest Bruno who covers the church; the
    // shop talk reads it to greet only once.
    FLAG_MET_LAWTIN = 47,
    // Read into a local on dungeon d02 floor start that nothing uses afterward. No script sets it.
    FLAG_D02_FLOOR_START_UNUSED = 49,
    // Set when the dungeon flag tutorial message is shown; no script reads it.
    FLAG_DUNGEON_FLAG_TUTORIAL_SHOWN = 50,
    // Set at the end of Monica's opening (s51 event 510) after she receives her starting items and
    // is sent to Palm Brinks; nothing reads it.
    FLAG_MONICA_PROLOGUE_ITEMS_GIVEN = 54,
    // Set the first time Max talks to the Paznos researcher about Chrono Unions in s11; the same
    // talk reads it to show it once.
    FLAG_CHRONO_UNION_TALK_SEEN = 55,
    // Cleared at the start of Cedric's house scenes and set when the Ridepod ride-out shot plays;
    // Cedric's house and Palm Brinks m01 read it to decide whether to skip the map music load.
    FLAG_SCENE_I01H01_RIDE_OUT = 56,
    // Set the first time the train driver talks about living in his new house in chapters other
    // than 8; the town NPC handlers read it to say it once.
    FLAG_TRAIN_DRIVER_HOME_TALK = 57,
    // Set when Max enters the correct name at the Moon Flower Palace gate in s23; d06b01 and s23
    // read it to open doors and to play the Griffon scene.
    FLAG_SIRUS_NAME_GIVEN = 58,
    // Set after the hint to protect Lin from the Evil Flame is shown in d03b02; the same event
    // reads it to show the hint once.
    FLAG_EVIL_FLAME_HINT_SHOWN = 64,
    // Set by the intro scene on dungeon d01 floor 1; the floor start script reads it to play the
    // scene once.
    FLAG_CHANNEL_INTRO_SEEN = 100,
    // Set at the start of event 101 in the i04h01 house; dungeon d01 floor 1 exit reads it to
    // decide whether to skip the music load.
    FLAG_HOUSE_FRIENDS_SCENE_SEEN = 101,
    // Set by event 102, both the Help Receiver call in d01 and the missing mother memory scene in
    // the i04h01 house; d01 floor 3 and the d01e01 town read it.
    FLAG_RECEIVER_CALL_SCENE_SEEN = 102,
    // Set by the Flotsam pump room scene in s06 and by Cedric's house event 520, and cleared when
    // the arena restarts; the d01e01 town and the s06 entry script read it.
    FLAG_FLOTSAM_PUMP_ROOM_SEEN = 103,
    // Set by the scene where Steve debuts the Ridepod in s06; Palm Brinks villagers and shop owners
    // read it to open the shop and change their lines.
    FLAG_STEVE_RIDEPOD_DEBUT_SEEN = 104,
    // Set by the Cedric invention hints scene; Cedric's talk reads it to stop scolding Max, and Palm
    // Brinks reads it for music handling.
    FLAG_CEDRIC_HINTS_SCENE_SEEN = 105,
    // Set when event 106 plays in Cedric's house and cleared when Palm Brinks re-enters it; the s06
    // entry script reads it.
    FLAG_CEDRIC_HOUSE_E106_SEEN = 106,
    // Set by the elephant ambush and boss intro scenes in s06 and by the story warp to s06b;
    // Cedric's house reads it to encourage Max after a loss.
    FLAG_ELEPHANT_AMBUSH_SEEN = 107,
    // Set by the elephant defeated scene in s06 and by Cedric's house event 108; nothing reads it.
    FLAG_ELEPHANT_BOSS_DEFEATED = 108,
    // Set by the train departure events at the station and in i03; chapter 1 villagers and the s06
    // exit read it as train running.
    FLAG_TRAIN_DEPARTURE_SEEN = 109,
    // Set by the d01b01 boss intro scene with the RoBomb tip; the d01 floor 7 start reads it.
    FLAG_D01_BOSS_INTRO_SEEN = 110,
    // Set by the d01b01 boss defeat scene; nothing reads it.
    FLAG_D01_BOSS_DEFEAT_SEEN = 111,
    // Set by event 201 in s01; the d01 floor 8 start and the s01 map start read it to prepare the
    // floor 8 info.
    FLAG_S01_EVENT_201_SEEN = 201,
    // Set by event 202 in s01 when the train first departs; the Palm Brinks station map and train
    // boarding read it.
    FLAG_FIRST_TRAIN_RIDE_SEEN = 202,
    // Set by the world explanation scene (event 203) in s08; the train door check reads it.
    FLAG_WORLD_EXPLAINED_SEEN = 203,
    // Set by event 204 in s03, the fight scene with Cedric, a bomb and a buggy; the train door
    // check reads it.
    FLAG_TRAIN_FIGHT_SCENE_SEEN = 204,
    // Set by event 205 in s03, which offers to try again or return to the title; nothing reads it.
    FLAG_TRAIN_FIGHT_RETRY_SEEN = 205,
    // Set by the scene where Max meets Monica on the train in s03; nothing reads it.
    FLAG_MAX_MET_MONICA_ON_TRAIN = 206,
    // Set by the scene where Monica explains time travel in s02; the s02 exit prompt and the
    // Sindain g01 start read it.
    FLAG_MONICA_TIME_TRAVEL_SEEN = 207,
    // Set by the Rococo scenes in Sindain g01; the s02 exit prompt and the g01 start read it with
    // flag 207.
    FLAG_ROCOCO_ENCOUNTER_SEEN = 208,
    // Set by the Grape Juice scenes in Sindain g01 and s02; Sindain g01 doors and the s05 map start
    // read it.
    FLAG_GRAPE_JUICE_SCENE_SEEN = 209,
    // Set by the dungeon d02 floor 2 intro event; the dungeon map and floor start scripts read it
    // to skip the music load.
    FLAG_D02_FLOOR2_INTRO_SEEN = 212,
    // Set by the s05 forest scene where Max notices the odd villagers; cleared by the s05 story
    // resets; the s05 map start reads it.
    FLAG_ODD_VILLAGERS_SCENE_SEEN = 215,
    // Set with flag 317 when Max receives the Map, Magic Crystal, Turkey and Fishing Rod at the
    // first dungeon; s05 story resets clear it and floor clear reads it.
    FLAG_DUNGEON_STARTER_ITEMS_GIVEN = 220,
    // Set by the Utan poison apple scene in s05; the story point setup reads it to allow the Poison
    // Apple pickup.
    FLAG_UTAN_POISON_APPLE_SCENE_SEEN = 230,
    // Set by event 235 in s05b; cleared by the s05 story reset, and nothing reads it.
    FLAG_S05_EVENT_235_SEEN = 235,
    // Set by event 240 in s05; forest setup and the villagers who talk about the swamp master read
    // it.
    FLAG_SWAMP_MASTER_E240_SEEN = 240,
    // Set by event 245 in s05; forest setup reads it with flag 240 to place the swamp master
    // character.
    FLAG_SWAMP_MASTER_E245_SEEN = 245,
    // Set by event 247 in s05b; the Sindain g01 map and the seated Firbits in g01i07 read it.
    FLAG_S05B_EVENT_247_SEEN = 247,
    // Set by the Toryon arrival and time gate explanation scenes in s47 and g01c; Sindain maps and
    // the s05 story points read it.
    FLAG_TIME_GATE_EXPLAINED = 250,
    // Set by the scene where Utan moves the boulder in s05; forest setup and Utan's talk read it.
    FLAG_UTAN_BOULDER_MOVED = 253,
    // Set by both Lafrescia sapling scenes in d02b01, the seed explosion and the Sundrop light;
    // nothing reads it.
    FLAG_LAFRESCIA_EVENT_255_SEEN = 255,
    // Set by the second Lafrescia room scene in d02b01; nothing reads it.
    FLAG_LAFRESCIA_EVENT_256_SEEN = 256,
    // Set by the d02b02 boss floor intro; the d02 floor 15 start reads it with flag 260.
    FLAG_D02_BOSS_INTRO_SEEN = 257,
    // Set by the d02b02 boss defeat scene and by the Sindain event replay; d02, the f01 map and
    // Sindain events read it.
    FLAG_D02_BOSS_DEFEAT_SEEN = 260,
    // Set once Sindain event 270 has played, cleared by the replay debug jump; the Sindain map
    // start and the Georama event gate read it.
    FLAG_SINDAIN_EVENT_270_SEEN = 270,
    // Set once Sindain event 272 has played, cleared by the replay debug jump; the f01 map start
    // and the event gate read it.
    FLAG_SINDAIN_EVENT_272_SEEN = 272,
    // Set once Sindain event 275 has played, cleared by the replay debug jump; the f01 map start
    // and NPC activity read it.
    FLAG_SINDAIN_EVENT_275_SEEN = 275,
    // Set once Sindain event 277 has played, cleared by the replay debug jump; nothing else reads
    // it.
    FLAG_SINDAIN_EVENT_277_SEEN = 277,
    // Set once Sindain event 280 has played; train crew and Sindain party NPCs read it to allow
    // leaving until the departure scene.
    FLAG_SINDAIN_EVENT_280_SEEN = 280,
    // Set by event 283 in s02; train crew and party NPCs read it to stop offering the departure.
    FLAG_SINDAIN_DEPARTURE_SEEN = 283,
    // Set once the Sindain town-complete event 285 has played; the Sindain event gate and map start
    // read it.
    FLAG_SINDAIN_EVENT_285_SEEN = 285,
    // Set when the monster talk in d02 gives Max the Sundrop; the same talk handler reads it to
    // give it once.
    FLAG_SUNDROP_RECEIVED = 290,
    // Set by event 300 in s04 and by the s17 story resets; train crew read it to allow leaving map
    // 16, and the Balance Valley g02 start reads it.
    FLAG_BALANCE_VALLEY_ARRIVAL_SEEN = 300,
    // Set by event 302 in Balance Valley g02; the g02 map start reads it with flag 300.
    FLAG_BALANCE_VALLEY_E302_SEEN = 302,
    // Set by the scene where Max finds Lin in pain in the g02i02 clinic; the Dr. Dell recruit
    // check, Palm Brinks and the g02 house entry read it.
    FLAG_LIN_ILL_SCENE_SEEN = 304,
    // Set by the scene where Dr. Dell joins in the i05h01 house; the Dell recruit check in i05h01
    // and Palm Brinks read it.
    FLAG_DELL_JOIN_SCENE_SEEN = 305,
    // Set by event 306 in g02 and the g02i02 clinic, after which Dr. Dell is in the party; Balance
    // Valley setup and g02 map events read it.
    FLAG_CLINIC_E306_SEEN = 306,
    // Set by the scene where Eina nurses a sick Max in s46; d03 floor 2 and the g02 exit checks
    // read it.
    FLAG_EINA_NURSES_MAX_SEEN = 307,
    // Set by event 308 in s17 and the d03 warp to it; nothing reads it.
    FLAG_S17_E308_SEEN = 308,
    // Set by the Condor time distortion scene in s17 and the d03 warp to it; nothing reads it.
    FLAG_CONDOR_TIME_DISTORTION_SEEN = 312,
    // Set by the Spheda lesson scene in s17; the Mina NPC and s17 setup read it to change her
    // lines.
    FLAG_SPHEDA_LESSON_SEEN = 314,
    // Set by the scene where Monica takes the club for the Spheda demo in s17; nothing reads it.
    FLAG_MONICA_SPHEDA_DEMO_SEEN = 315,
    // Set by the Spheda hole-in scene in s17; nothing reads it.
    FLAG_SPHEDA_HOLE_IN_SEEN = 316,
    // Set with flag 220 when the dungeon starter items are given; s17 story resets clear it, and
    // floor clear reads it to start the Spheda minigame.
    FLAG_SPHEDA_UNLOCKED = 317,
    // Set by event 320 with Lao Chao in the f02i08 restaurant; the clinic NPC, f02i08 and the s17
    // floor setup read it.
    FLAG_LAO_CHAO_REQUEST_HEARD = 320,
    // Set by event 324 with Lao Chao cooking in f02i08; Starlight Temple villagers and f02i08 read
    // it.
    FLAG_LAO_CHAO_COOKING_SEEN = 324,
    // Set by event 328 in the g02i02 clinic; g02 house entry and the clinic map start read it.
    FLAG_CLINIC_E328_SEEN = 328,
    // Set by event 330 in the g02i02 clinic; s18 setup and Starlight Temple villagers read it.
    FLAG_CLINIC_E330_SEEN = 330,
    // Set by the first scene on s18; the s18 map start reads it with flag 300 to play it once.
    FLAG_S18_ARRIVAL_SEEN = 332,
    // Set by the scene where Lin tells Max she wants to be a sage in s18; nothing reads it.
    FLAG_LIN_WANTS_SAGE_SEEN = 336,
    // Set by the scene where Lin remembers the lighthouse in s18; s18 setup and Lin's item question
    // read it.
    FLAG_LIN_LIGHTHOUSE_MEMORY_SEEN = 340,
    // Set by the lighthouse arrival event in d03b01 and s56; lighthouse dungeon rooms and s56 read
    // it to disable the event point.
    FLAG_LIGHTHOUSE_ARRIVAL_SEEN = 348,
    // Set by event 352 in d03b01 and s56; lighthouse rooms and s56 read it to disable the event
    // point.
    FLAG_LIGHTHOUSE_E352_SEEN = 352,
    // Set by the scene after Giltoni is beaten and Max parts with Crest in d03b02; d03 floor 23,
    // the clinic NPC and others read it.
    FLAG_CREST_FAREWELL_SEEN = 356,
    // Set by event 360 in s04; train crew and party NPCs read it with flag 372 to allow leaving map
    // 54, and g03 reads it.
    FLAG_BALANCE_VALLEY_DEPARTURE_SEEN = 360,
    // Set once Balance Valley event 364 has played, cleared by the replay jump; the Georama event
    // gate and f02i08 read it.
    FLAG_BALANCE_VALLEY_EVENT_364_SEEN = 364,
    // Set once Balance Valley event 368 has played, cleared by the replay jump; the Georama event
    // gate reads it.
    FLAG_BALANCE_VALLEY_EVENT_368_SEEN = 368,
    // Set once Balance Valley event 372 has played at Starlight Temple, cleared by the replay jump;
    // villagers and train crew read it.
    FLAG_STARLIGHT_TEMPLE_E372_SEEN = 372,
    // Set once the town-complete event 376 has played in Balance Valley, cleared by the replay
    // jump; the event gate reads it.
    FLAG_BALANCE_VALLEY_EVENT_376_SEEN = 376,
    // Set by event 400 in s09 and by the Shell Talkie hand-in and take-back in d04; Veniccio
    // villagers, d04b01 and train crew read it.
    FLAG_VENICCIO_ARRIVAL_SEEN = 400,
    // Set by event 402 in Veniccio g03; the g03 map start reads it with flag 360.
    FLAG_VENICCIO_E402_SEEN = 402,
    // Set by event 404 in Veniccio g03; g03 map setup and the town talk handlers read it.
    FLAG_VENICCIO_E404_SEEN = 404,
    // Set by the Eina bedtime story scene in s46; d04 floor 2 and the g03 exit checks read it.
    FLAG_EINA_BEDTIME_STORY_SEEN = 406,
    // Set by event 408 in s19 and f03i0301 to begin the Electric Worm request; Veniccio villagers
    // and f03 exits read it.
    FLAG_ELECTRIC_WORM_REQUEST_STARTED = 408,
    // Set by event 410 in s19; nothing reads it.
    FLAG_S19_E410_SEEN = 410,
    // Set by event 412 in f03i0301 when Max is given the Electric Worm; f03 exits and the room
    // start read it.
    FLAG_ELECTRIC_WORM_GIVEN = 412,
    // Set by event 414 in f03i0301; the room map start reads it to play the scene once.
    FLAG_LUNAR_LAB_E414_SEEN = 414,
    // Set by event 416 in s19; the s19 player-dead check reads it to choose a retry or game over.
    FLAG_S19_E416_SEEN = 416,
    // Set by event 420 in s19 and s27 when the Electric Worm request ends; Veniccio villagers and
    // s27 setup read it.
    FLAG_ELECTRIC_WORM_REQUEST_DONE = 420,
    // Set by event 428 in f03i0302 when Max is given the Shell Talkie; f03 exits and the room start
    // read it.
    FLAG_SHELL_TALKIE_GIVEN = 428,
    // Set by the Shigura Village arrival scene when Max carries the Shell Talkie; d04b01 village
    // start reads it.
    FLAG_SHIGURA_ARRIVAL_WITH_ITEM_SEEN = 432,
    // Set by event 434 in Shigura Village when Max receives the Secret Dragon Remedy; the village
    // talk handler reads it to give it once.
    FLAG_SECRET_DRAGON_REMEDY_GIVEN = 434,
    // Set by the Shigura Village arrival scene when Max lacks the Shell Talkie; d04b01 village
    // start reads it.
    FLAG_SHIGURA_ARRIVAL_WITHOUT_ITEM_SEEN = 436,
    // Set by the Shigura boss mid-fight scenes, cleared when the Shell Talkie is handed over or
    // taken back; the village setup, villagers and s27 read it.
    FLAG_SHIGURA_BOSS_MIDFIGHT_SEEN = 438,
    // Set by the Gaspard marionette scene in Shigura Village; the boss loss handler reads it to
    // start retry rolls.
    FLAG_GASPARD_MARIONETTE_SEEN = 440,
    // Set by the Shigura rescue and farewell scene; s27 story resets clear it, and the village, f03
    // lab and boss loss handler read it.
    FLAG_SHIGURA_RESCUE_SEEN = 444,
    // Set by the Ixion explanation scene in the f03i04 lunar lab; train crew, f03 villagers and the
    // Georama gate read it.
    FLAG_IXION_EXPLAINED_SEEN = 448,
    // Set by event 452 in s09; train crew and party NPCs read it with flag 448 to allow leaving map
    // 83.
    FLAG_VENICCIO_DEPARTURE_SEEN = 452,
    // Set by the scene where Max finds the building in f03; f03 villagers and the Georama gate read
    // it.
    FLAG_LUNAR_LAB_FOUND_SEEN = 456,
    // Set by the scene where Max finds the new lab in f03, cleared by the replay jump; the Georama
    // event gate reads it.
    FLAG_NEW_LAB_FOUND_SEEN = 460,
    // Set by the lunar lab arrival scene in f03i04; the lab map start reads it to play it once.
    FLAG_LUNAR_LAB_ARRIVAL_SEEN = 464,
    // Set by event 468 in f03i0301 once Veniccio is complete; the f03, g03 event gate reads it.
    FLAG_VENICCIO_TOWN_COMPLETE_SEEN = 468,
    // Set by the arrival event 500 in s10, cleared by its story replay; train crew and party NPCs
    // read it to allow leaving map 83.
    FLAG_HEIM_RADA_ARRIVAL_SEEN = 500,
    // Set by event 504 in s41; the s10 Heim Rada exit reads it to play that event once.
    FLAG_HEIM_RADA_E504_SEEN = 504,
    // Set by event 508 in Heim Rada f04 when Max is given the Time Bomb, cleared by the replay
    // jump; the f04 shop trigger, s20 and the partner hints read it.
    FLAG_TIME_BOMB_GIVEN = 508,
    // Set by the fire rain source scene in s20; the s20 map start reads it with flag 508.
    FLAG_FIRE_RAIN_SOURCE_SEEN = 512,
    // Set by the battleship attack scene in s20; the s20 map setup and edit-mode checks read it.
    FLAG_BATTLESHIP_ATTACK_SEEN = 516,
    // Set by the scene where the blue Atlamillia glows at the time gate; the blue time gate entry
    // reads it to show it once.
    FLAG_BLUE_GATE_GLOW_SEEN = 522,
    // Set by the Atlamillia check scene at the time gate; nothing reads it.
    FLAG_ATLAMILLIA_CHECK_SEEN = 523,
    // Set by event 524 in s39; the partner hint messages read it to change the hint.
    FLAG_S39_E524_SEEN = 524,
    // Set by the lava parting scene in d05b01; the Gaspard lava room setup and the partner hints
    // read it.
    FLAG_GASPARD_LAVA_PARTING_SEEN = 526,
    // Set by the Gaspard ambush scene in d05b01; the room setup and the boss loss handler read it.
    FLAG_GASPARD_AMBUSH_SEEN = 528,
    // Set by the Gaspard backstory, transformation and mother farewell scenes; the boss loss
    // handler and the blue time gate read it.
    FLAG_GASPARD_BACKSTORY_SEEN = 532,
    // Set by the Gaspard death scene, Monica's curse scene and Gaspard's return home scene;
    // Desoland villagers, partner hints and the boss loss handler read it.
    FLAG_GASPARD_DEATH_SEEN = 536,
    // Set by the Eina farewell scene in s29 and the golem army march scenes in s55; the lunar lab
    // entry and Desoland villagers read it.
    FLAG_GOLEM_ARMY_MARCH_SEEN = 540,
    // Set once Heim Rada event 548 has played, cleared by the replay jump; the Georama event gate
    // reads it.
    FLAG_HEIM_RADA_EVENT_548_SEEN = 548,
    // Set once Heim Rada event 552 has played, cleared by the replay jump; the Georama event gate
    // reads it.
    FLAG_HEIM_RADA_EVENT_552_SEEN = 552,
    // Set once Heim Rada event 556 has played, cleared by the replay jump; the Georama event gate
    // reads it.
    FLAG_HEIM_RADA_EVENT_556_SEEN = 556,
    // Set by event 560 in s39 once the Heim Rada Georama is complete; the Georama event gate reads
    // it.
    FLAG_HEIM_RADA_COMPLETE_SEEN = 560,
    // Set by the Ixion unveiling, the train rush and the other s37 scenes; the lunar lab, s33 and
    // Veniccio villagers read it.
    FLAG_IXION_UNVEILING_SEEN = 600,
    // Set by the Griffon castle approach scene in s33; the s33 map start reads it with flag 600.
    FLAG_GRIFFON_CASTLE_APPROACH_SEEN = 604,
    // Set by the Emperor Griffon reveal scene in d06e01; the d06e01 loss handler reads it to start
    // retry rolls.
    FLAG_EMPEROR_GRIFFON_REVEAL_SEEN = 608,
    // Set by the Griffon hatred vision scene in d06e01; nothing reads it.
    FLAG_GRIFFON_HATRED_VISION_SEEN = 610,
    // Set by the scenes where Griffon takes Atlamillia and unleashes his power, and by the s35 time
    // gate activation; the time travel code reads it as the late game mark.
    FLAG_GRIFFON_TAKES_ATLAMILLIA_SEEN = 612,
    // Set when the party leaves for the Moon Flower Palace and after Osmond's report scene;
    // Starlight Temple and Veniccio villagers and a d04 tip read it.
    FLAG_OSMOND_REPORT_SEEN = 616,
    // Set by the first lapis scene in s13; the partner hints read it with flags 628, 632 and 636.
    FLAG_LAPIS_STONE_1_FOUND = 624,
    // Set by the second lapis scene in s14; the partner hints read it with flags 624, 632 and 636.
    FLAG_LAPIS_STONE_2_FOUND = 628,
    // Set by the third lapis scene in s15; the partner hints read it with flags 624, 628 and 636.
    FLAG_LAPIS_STONE_3_FOUND = 632,
    // Set by the fourth lapis scene in s16; the partner hints read it with flags 624, 628 and 632.
    FLAG_LAPIS_STONE_4_FOUND = 636,
    // Set by the Griffon battle challenge scene in d06b01 and by events in s11; Starlight Temple
    // and Veniccio villagers and partner NPCs read it.
    FLAG_FINAL_STAGE_STARTED = 640,
    // Set by events in s11, s12 and s37; the waiting party NPC reads it to change what he says.
    FLAG_WAITING_NPC_RELEASED = 648,
    // Set by the Andrea garden clue scene in Moon Flower Palace g05 and by s65 and s68; the partner
    // hint messages read it.
    FLAG_ANDREA_GARDEN_CLUE_SEEN = 700,
    // Set by the first scene on s21; the s21 map start reads it to play it once.
    FLAG_S21_FIRST_VISIT_SEEN = 704,
    // Set by event 708 in s50; s50 map setup, s21 and the s50 door read it.
    FLAG_FLOWER_OF_SUN_SCENE_SEEN = 708,
    // Set by event 712 at the Moon Flower Palace gate in s23; the partner hint messages read it.
    FLAG_SIRUS_GATE_EVENT_SEEN = 712,
    // Set by the scene where Griffon appears in d06b01; the d06b01 loss handler reads it to start
    // retry rolls.
    FLAG_GRIFFON_APPEARS_SEEN = 716,
    // Set by the winged boss transformation scene in d06b01, d06 and s64; nothing reads it.
    FLAG_WINGED_BOSS_TRANSFORM_SEEN = 720,
    // Set by the boss awakens scene in d06b01 and s31; nothing reads it.
    FLAG_BOSS_AWAKENS_WITH_FLAME_SEEN = 724,
    // Set by the boss defeat scene in d06b01 and by the d06b02 monster room setups; the d06b01 loss
    // handler and partner hints read it.
    FLAG_FINAL_BOSS_DEFEATED = 728,
    // Set by event 732 in d06b03 and s67; the partner hint messages read it.
    FLAG_D06B03_EVENT_SEEN = 732,
    // Set by the farewell scenes in s66 and s98 and by the i04h01 ending; Palm Brinks m01 reads it
    // with flag 800 to start the post-ending scene.
    FLAG_ENDING_SEEN = 736,
    // Set by the scene where Andrea finds Sirus in s44, and by Moon Flower Palace g05 before it;
    // g05 reads it to see whether the analysis scene is pending.
    FLAG_ANDREA_FINDS_SIRUS_SEEN = 744,
    // Set by event 748 in s44 and by Moon Flower Palace g05 before it; g05 and the s23 entry script
    // read it.
    FLAG_MOON_FLOWER_FLASHBACK_SEEN = 748,
    // Set by the post-ending scene in Palm Brinks m01 and cleared by the ending; m01 dungeon choice
    // and program 290 read it to unlock post-game access.
    FLAG_POSTGAME_SCENE_SEEN = 800,
    // Set by the scene where Monica arrives in the mine in s52; chapter 8 villagers read it to
    // change their lines.
    FLAG_MONICA_BACK_IN_MINE_SEEN = 804,
    // Set by the scene where Flotsam is the new boss in s54; the s54 room setup and loss handler
    // read it.
    FLAG_FLOTSAM_NEW_BOSS_SEEN = 808,
    // Set by the Flotsam defeated scene in s54; the s54 loss handler reads it.
    FLAG_FLOTSAM_DEFEATED_SEEN = 812,
    // Set by the scene where the Zelmite ore boss appears in d07b01; the dark genie loss handler
    // reads it.
    FLAG_ZELMITE_BOSS_APPEARS_SEEN = 816,
    // Set by the Dark Genie shrink scene in d07b01; the d07b01 map setup and loss handler read it.
    FLAG_DARK_GENIE_SHRINK_SEEN = 820,
    // Set by the event 824 scenes in s60; nothing reads it.
    FLAG_S60_EVENT_824_SEEN = 824,
};

#endif

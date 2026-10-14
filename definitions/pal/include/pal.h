// Story and content flags of the PAL retail scripts: bit flags in the save data, read with
// get_flag and written with set_flag.
#ifndef PAL_H
#define PAL_H

enum flag_story {
    // Set the first time Max hands over the Circus Ticket at the m06 circus entrance, which plays
    // the scene of a boy snatching it (m06b event 153); with it set, using the ticket again plays
    // the circus scene and enters the dungeon. Cleared by the m06 opening (m06b event 152) when the
    // ticket is first given.
    FLAG_CIRCUS_TICKET_SNATCHED = 1,
    // Set when Max, without a ticket after the snatch, asks the seated stargazer at the m06 circus
    // about the boy; with flag 3, the girl bystander then starts m06b event 154 where the boy
    // returns the ticket. Cleared by m06b event 152. Dungeon d01 reuses it as a first-time mark on
    // floor 1 (Help Receiver intro).
    FLAG_ASKED_BYSTANDER_A = 2,
    // Set when Max, without a ticket after the snatch, asks the woman bystander at the m06 circus
    // about the boy; with flag 2, the girl bystander then starts m06b event 154 where the boy
    // returns the ticket. Cleared by m06b event 152; the opening character setup also reads it.
    FLAG_ASKED_BYSTANDER_B = 3,
    // Set by the m07 opening letter scene before the jump to the Repair Shop and by the arrival
    // scene at Donny's base (d01e01); cleared when Donny gives Max the Classic Gun and Help
    // Receiver and chapter 1 starts. While set the channel gate is blocked and menus keep the BGM;
    // the engine reads it to show evening on the menu clock.
    FLAG_OPENING_DONNY_PENDING = 4,
    // Cleared by the Underground Channel arena entry script (s06 event 500); nothing reads it.
    FLAG_SCENE_S06_ENTRY_RESET = 5,
    // Set when the Ridepod first enters the s06 pump room arena and the elephant boss intro plays
    // (s06b event 510), when Steve is knocked out there before the elephant is beaten, and by
    // Cedric's house story 108c. s06 entry reads it to play s06b event 520 instead of the intro,
    // and Cedric's house starts the retry offer (event 520) while it is set and flag 7 is clear.
    FLAG_RIDEPOD_ELEPHANT_FIGHT_STARTED = 6,
    // Set when the Ridepod beats the elephant boss in the s06 pump room (s06 event 502 and story
    // 108a); Cedric's house story 108c clears it. s06 reads it for the exit door, entry scene and
    // knock-out handling, and Cedric's talk reads it to offer Steve's refuelling and core upgrade.
    FLAG_CHANNEL_ARENA_CLEARED = 7,
    // Set the first time Max takes the arena exit door toward Palm Brinks Square; the exit door
    // script reads it to play the transition only once.
    FLAG_CHANNEL_EXIT_USED = 8,
    // Set the first time Max passes the d01 floor 5 gate, which jumps to the Mayor's Office scene
    // (edit mode 38, event 510), and by the Blackstone One departure scene at the station (event
    // 109). The station map and chapter 1 villagers read it to treat the trains as running again.
    FLAG_TRAIN_RUNNING = 9,
    // Set on first entry to the Underground Channel boss room d01b01, which seals door01, shows the
    // barrier map piece and plays the Halloween intro; later entries play the short entry scene.
    // The d01 story warps to the boss room clear it so the intro plays again.
    FLAG_D01B01_INTRO_STARTED = 10,
    // No script sets it; the engine's invention menu (inventmn.cpp) uses it. Cedric's Energy Pack
    // event 520 reads it to play the finish scene, where Cedric takes the Energy Pack, sets flag 20
    // and puts the pack on Steve; the story 105c variant checks for the Energy Pack item instead.
    FLAG_CEDRIC_HOUSE_ENERGY_PACK_BRANCH = 13,
    // Set the first time Max leaves Cedric's house after the camera lesson (flag 18), which also
    // starts the town music. Until Max has the Energy Pack idea, Cedric's talk reads it to replay
    // the photo hint reminder (i01h01b event 510).
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
    // Set by Cedric's camera lesson (i01h01b event 500) and cleared whenever his Energy Pack event
    // 520 (or story 105c) starts, i.e. once Max has the idea. Palm Brinks m01 reads it to enable
    // the invent_36-0 idea spot.
    FLAG_ENERGY_PACK_INVENT_SPOT = 21,
    // Read by the i02h03 villager talk during the Claire scene; when set, Erik asks Maximilian to
    // come to the usual place. No script sets it.
    FLAG_I02H03_ERIK_CLAIRE_INVITE = 22,
    // Set in dungeon d02 rooms b01 and b03 when Max throws the Lafrescia Seed into the small pond
    // and a sapling sprouts; the room setup reads it to show the sapling, and the pond spot then
    // asks for a Sundrop instead of the seed.
    FLAG_LAFRESCIA_SEED_PLANTED = 23,
    // Set when Max uses the Sundrop on the Lafrescia sapling; room b01 setup and the floor 15 gate
    // in d02 read it before the boss floor opens.
    FLAG_LAFRESCIA_SUNDROP_USED = 24,
    // Set when the Ridepod is added as party member 2 in Cedric's house; the s06 arena entry script
    // reads it.
    FLAG_RIDEPOD_JOINED_PARTY = 25,
    // Set when Max gives Grape Juice to Noam the fisherman in the s05 forest (event 220) and gets
    // the Fishing Rod and Mimi; the forest character setup reads it to show the wine by Noam, and
    // story program 220 clears it to replay the scene.
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
    // Set at the start of Sindain g01 event 254, where Max and Monica read the Geostone and decide
    // to bring Gordon the gardener to the village; no script reads it, but the engine's Georama
    // edit loop (editloop.cpp) does.
    FLAG_SINDAIN_GEOSTONE_SCENE_START = 34,
    // Set when Max receives the Gold Paint in the f01 map; the same handler reads it to give it
    // only once.
    FLAG_GOLD_PAINT_RECEIVED = 36,
    // Set when Max receives 20 Purple Paint in the i04h03 room; the room reads it to give it once
    // and to hide the part that held it.
    FLAG_PURPLE_PAINT_RECEIVED = 37,
    // Set when Max picks up the Photo Album in the i04h01 house; the house reads it to give it only
    // once and, on map setup, to hide the part that held it.
    FLAG_PHOTO_ALBUM_RECEIVED = 38,
    // Set by the setup of the Station (i02h01) and of the Channel Entrance (d01e01); Palm Brinks
    // m01 reads it at map start to place Max by the exit he came from (previous map 34 or 22), then
    // clears it.
    FLAG_STATION_ARRIVAL_PENDING = 40,
    // Set when Max's fish takes first place in a Finny Frenzy race in i03h04; Olivie's town talk
    // reads it to scorn him until then and afterward join the party, which clears quest 16.
    FLAG_FISH_RACE_WON = 41,
    // Set by g01 program 507 just before the event 254 scene where Max and Monica decide Gordon the
    // gardener should move to Sindain; Gordon's town talk reads it to switch from his early lines
    // to his quest 0 offer.
    FLAG_GEOSTONE_PLAN_SCENE_DONE = 42,
    // Set by story scenes that switch to event music (the f02 Crest scenes around event 372 and the
    // Luna Lab advice, the s26 Carpenterion intro) and cleared when a scene restores the music; the
    // f02 scenes, the g01c time gate explanation (event 250) and the s33 Griffon flashback read it
    // to restart their music.
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
    // No script sets it; the engine's menu system (menusys.cpp) uses it. Dungeon d02 floor start
    // reads it into a local that nothing uses afterward.
    FLAG_MENU_SYSTEM_ENGINE_FLAG = 49,
    // Set by the dungeons' random_stone_help program when its tutorial message (2600) is shown; no
    // script reads it, but the engine's dungeon code (dng_main.cpp) uses it.
    FLAG_DUNGEON_FLAG_TUTORIAL_SHOWN = 50,
    // Set by the Vanguard monster script (e202a) the first time it takes a type 0 hit, when it runs
    // event script 2510; the same script reads it so the event plays only once.
    FLAG_VANGUARD_FIRST_HIT_EVENT_SEEN = 51,
    // Set at the end of Monica's castle prologue (s51 program 510), when the party is switched back
    // to Max, the starting items (Circus Ticket, Medal Holder, Bread and others) are given and the
    // game goes to Palm Brinks; no script reads it, but the engine's main menu (menumain.cpp) uses
    // it.
    FLAG_MONICA_PROLOGUE_ITEMS_GIVEN = 54,
    // Set the first time Max talks to the Paznos researcher about Chrono Unions in s11; the same
    // talk reads it to show it once.
    FLAG_CHRONO_UNION_TALK_SEEN = 55,
    // Cleared by the Cedric's house scenes (events 105 and 108) and the m01 dawn scene; set when
    // Max rides out to retry Porcus or when m01 restores the town music at map start. Between flags
    // 105 and 109, Cedric's house exit and m01 read it to decide whether the Palm Brinks music must
    // be reloaded.
    FLAG_PALM_BRINKS_BGM_RESTORED = 56,
    // Set the first time Cedric, as a resident of a Georama town (g00, g02), says he now lives
    // there but still runs Blackstone One, outside chapter 8; his talk reads it to give that line
    // once and then ask about Steve.
    FLAG_TRAIN_DRIVER_HOME_TALK = 57,
    // Set when Max enters the correct name at the Moon Flower Palace gate in s23; d06b01 and s23
    // read it to open doors and to play the Griffon scene.
    FLAG_SIRUS_NAME_GIVEN = 58,
    // Set after the hint to protect Lin from the Evil Flame is shown in d03b02; the same event
    // reads it to show the hint once.
    FLAG_EVIL_FLAME_HINT_SHOWN = 64,
    // Set by event 100, the first visit to Underground Channel (d01) floor 1 where Donny explains
    // the Floor Map and Gate Key over the Help Receiver; the d01 floor start reads it to play the
    // scene once.
    FLAG_CHANNEL_INTRO_SEEN = 100,
    // Set by event 101, the dinner scene where Max asks his father about his mother and storms out.
    // Clearing d01 floor 1 warps to the Dining Room to play it while the flag is clear.
    FLAG_DINNER_ARGUMENT_SEEN = 101,
    // Set by event 102: Max's memory of his crying mother in his room, then Donny's second receiver
    // call in d01. Entering d01 floor 3 while clear warps to Max's room first. Engine:
    // SAVE_FLAG_MENU_TIME_MIDNIGHT.
    FLAG_MOTHER_MEMORY_SCENE_SEEN = 102,
    // Set by event 103 in s06, where Flotsam demands the red stone, and by the Cedric's house story
    // warps; cleared by the s06 warp back to event 500. Until set the d01e01 channel entrance
    // blocks the way back to Palm Brinks. Engine: SAVE_FLAG_MENU_TIME_1AM.
    FLAG_FLOTSAM_PUMP_ROOM_SEEN = 103,
    // Set by event 104 in s06, where Cedric arrives in the Ridepod Steve and beats Flotsam's
    // elephant. Villager talk reads it (Donny only offers his shop while it is clear), and s06
    // reads it for the exit door line and map lock. Engine: SAVE_FLAG_MENU_TIME_FORCED_OFF.
    FLAG_STEVE_RIDEPOD_DEBUT_SEEN = 104,
    // Set by Cedric's event 105 lesson scenes in his house (camera, photo hints, Energy Pack);
    // cleared by the Palm Brinks warps that replay event 105. Cedric's talk reads it to stop
    // scolding Max, s06 for the platform arrival, Palm Brinks for music.
    FLAG_CEDRIC_HINTS_SCENE_SEEN = 105,
    // Set by event 106 in Cedric's house, where Cedric asks if Max is ready and they ride out on
    // Steve; cleared by the Palm Brinks warp that replays event 106. The s06 entry reads it with
    // Steve in the party to lock map moves.
    FLAG_CEDRIC_HOUSE_E106_SEEN = 106,
    // Set by the elephant ambush and boss intro scenes in s06, the s06b story warp and Cedric's
    // event 520; cleared by the Palm Brinks warp to event 106. Cedric's house reads it to place
    // Steve, enable the e108c trigger and encourage Max after a loss.
    FLAG_ELEPHANT_AMBUSH_SEEN = 107,
    // Set by every event 108 variant: the elephant fight start, win and loss scenes in s06 and
    // Max's angry return to Cedric's house. Cleared by Cedric's event 520 when Steve joins the
    // party. No script reads it.
    FLAG_ELEPHANT_FIGHT_SCENE_SEEN = 108,
    // Set by the train departure events at the station and in i03; chapter 1 villagers and the s06
    // exit read it as train running.
    FLAG_TRAIN_DEPARTURE_SEEN = 109,
    // Set by event 110, the d01b01 intro where Halloween and Polcurse demand the red stone, with
    // the nose bomb tip; the d01 floor 7 start reads it to skip the music load until the intro has
    // played.
    FLAG_D01_BOSS_INTRO_SEEN = 110,
    // Set by the d01b01 boss defeat scene; nothing reads it.
    FLAG_D01_BOSS_DEFEAT_SEEN = 111,
    // Set by event 201, Max's first walk along the s01 road outside the channel. The s01 map start
    // plays it while clear and marks d01 floor 8 visited; the d01 floor 8 start skips the music
    // load until it is set.
    FLAG_S01_EVENT_201_SEEN = 201,
    // Set by event 202 in s01, where the Blackstone train roars in and Cedric invites Max aboard.
    // Unlocks boarding: the Palm Brinks station lets Max board and shows the train, and s01 and s08
    // read it to place the train and crew.
    FLAG_BLACKSTONE_ARRIVAL_SEEN = 202,
    // Set by event 203 in s08, where the crew explains the ruined world and a blast hits the train.
    // While set and flag 204 is clear the train attack is under way: train doors stay shut and s08
    // sends Max to the s03 fight.
    FLAG_WORLD_EXPLAINED_SEEN = 203,
    // Set by event 204 in s03, where Flotsam's gang chases the train in a buggy and Cedric hands
    // Max a bomb. Ends the train attack state begun by flag 203; s08 and every train door check
    // read the pair.
    FLAG_TRAIN_FIGHT_SCENE_SEEN = 204,
    // Set by event 205 after losing the train fight: in s08 Cedric says the train can take no more,
    // and in s03 the game asks to fight again or return to the title. Nothing reads it.
    FLAG_TRAIN_FIGHT_RETRY_SEEN = 205,
    // Set by the scene where Max meets Monica on the train in s03; nothing reads it.
    FLAG_MAX_MET_MONICA_ON_TRAIN = 206,
    // Set by event 207 in s02, where Monica explains time travel and joins Max. s02 plays the event
    // while clear; with flag 208 clear, Sindain g01 starts the Rococo story script and the s02 exit
    // skips the music load.
    FLAG_MONICA_TIME_TRAVEL_SEEN = 207,
    // Set by the Rococo scenes in Sindain g01; the s02 exit prompt and the g01 start read it with
    // flag 207.
    FLAG_ROCOCO_ENCOUNTER_SEEN = 208,
    // Set by the event 209 scenes: Conda gives Grape Juice in s02, Max and Monica knock at the
    // Firbit house in Sindain and Conda asks for help inside. Opens the g01i07 house door, hides an
    // s02 map part and lets the s05 start play event 215.
    FLAG_GRAPE_JUICE_SCENE_SEEN = 209,
    // Set by event 212 on the first visit to d02 floor 2, where Monica gives Max crystals and
    // explains weapon synthesis. The d02 floor start gives the crystals and plays it while clear;
    // it and the s02 dungeon menu skip the music load until then.
    FLAG_D02_FLOOR2_INTRO_SEEN = 212,
    // Set by the s05 forest scene where Max notices the odd villagers; cleared by the s05 story
    // resets; the s05 map start reads it.
    FLAG_ODD_VILLAGERS_SCENE_SEEN = 215,
    // Set by event 220 in s05 the first time Max talks to the fisherman Noam (his complaint that
    // the fish aren't biting), which the s05 scenes read to skip it later; the s05 story resets
    // clear it. The dungeon program 200 (grant_starter_items) also sets it, together with flag 317,
    // when it hands out the Map, Magic Crystal, Turkey and Fishing Rod. The engine reads it in
    // dngmenu.cpp.
    FLAG_NOAM_FISHING_SCENE_SEEN = 220,
    // Set by event 230 in s05, where Max finds Master Utan frozen after eating the forbidden fruit
    // and hears of the Holy One; s05 story resets clear it. The s05 story point setup enables the
    // event 230 trigger while clear and the Poison Apple pickup once set.
    FLAG_UTAN_POISON_APPLE_SCENE_SEEN = 230,
    // Set by event 235 in s05b, where Max and Monica decide to fish out the Holy One with Poison
    // Apple bait; cleared by the s05 story resets. No script reads it; the engine reads it in
    // fishing.cpp.
    FLAG_S05_EVENT_235_SEEN = 235,
    // Set by event 240 in s05, when Max hooks Mardan the swamp master and he asks for a Priscleen
    // Fish. s05 setup places Mardan while it is set and flag 245 is clear; villagers change their
    // swamp master lines. The engine reads it in fishing.cpp.
    FLAG_SWAMP_MASTER_E240_SEEN = 240,
    // Set by event 245 in s05, when Max brings Mardan the Priscleen Fish and he turns the three
    // Firbits back to normal; s05 setup stops placing Mardan once it is set.
    FLAG_SWAMP_MASTER_E245_SEEN = 245,
    // Set by event 247 in s05b, where the bewitched Firbits return to normal and Max and Monica
    // head back to Sindain. With flag 250 clear, entering the g01i07 Firbit house then plays the
    // Firbits' deal scene; the seated Firbits are no longer placed.
    FLAG_S05B_EVENT_247_SEEN = 247,
    // Set by the event 250 scenes: the Firbits' deal in g01i07, the Carpenterion's arrival and
    // Monica's Time Gate talk in s47 and g01c. Unlocks Georama editing in Sindain (engine:
    // editloop.cpp); Sindain maps, s05 story points and f01 read it.
    FLAG_TIME_GATE_EXPLAINED = 250,
    // Set by the scene where Utan moves the boulder in s05; forest setup and Utan's talk read it.
    FLAG_UTAN_BOULDER_MOVED = 253,
    // Set by both Lafrescia sapling scenes in d02b01, the seed explosion and the Sundrop light;
    // nothing reads it.
    FLAG_LAFRESCIA_EVENT_255_SEEN = 255,
    // Set by event 256 in d02b01, where Max and Monica grab the Lafrescia vine and are pulled up
    // toward the boss floor; nothing reads it.
    FLAG_LAFRESCIA_EVENT_256_SEEN = 256,
    // Set by event 257 in d02b02, where the Rainbow Butterfly swoops down on Max and Monica. While
    // it is set and flag 260 is clear, the d02 floor 15 start loads the d02b02 boss room instead of
    // the d02b01 Lafrescia room.
    FLAG_D02_BOSS_INTRO_SEEN = 257,
    // Set by event 260 in d02b02, where the beaten Rainbow Butterfly turns back into Holly, who
    // gives Max the Teal Envelope; the f01 story jump to event 280 also sets it. d02 floor 15 and
    // d02b01 read it, and Sindain event 280 requires it.
    FLAG_D02_BOSS_DEFEAT_SEEN = 260,
    // Set by event 270 in Jurak Mall (f01), where Max and Monica ask Jurak about Griffon; cleared
    // by the f01 story_270 debug jump. The f01 map start and the event 270/272 gates read it, and
    // the Sindain red gate skips the BGM load while it is pending.
    FLAG_SINDAIN_EVENT_270_SEEN = 270,
    // Set by event 272 in Jurak Mall (f01), where Jurak gives Max the Lafrescia Seed and Badge Box;
    // cleared by the f01 debug jump. The f01 map start, the event gate and Jurak's hint talk read
    // it.
    FLAG_SINDAIN_EVENT_272_SEEN = 272,
    // Set by event 275 in Jurak Mall (f01), when the monster badge shop opens; cleared by the f01
    // debug jump. The f01 map start and event gate read it, f01 shows the shop NPCs only once it is
    // set, and Jurak's hint talk reads it.
    FLAG_SINDAIN_EVENT_275_SEEN = 275,
    // Set when the e277 point in Jurak Mall (f01) plays the scene where Monica shows Max the
    // Himarra badge; cleared by the f01 debug jump. No script reads it.
    FLAG_SINDAIN_EVENT_277_SEEN = 277,
    // Set by event 280: Jurak remembers Griffon in f01, then the crew clears the rock from the
    // tracks at Sindain Station (s02); cleared by the f01 debug jump. Marks the train as free to
    // leave Sindain: train crew (Cedric offers departure on map 16 until flag 283, Erik's rock
    // grumble), s02 props, Jurak and Jurak Mall villagers read it.
    FLAG_TRAIN_TO_BALANCE_VALLEY_OPEN = 280,
    // Set by event 283 when the train leaves Sindain Station (s02) for Balance Valley; train crew
    // stop offering the departure from map 16, and the s04 map start reads it with flag 300 to play
    // the Balance Valley arrival.
    FLAG_SINDAIN_DEPARTURE_SEEN = 283,
    // Set by event 285 in Jurak Mall (f01) when Sindain is 100% restored and Jurak gives Max the
    // Jurak Gun; cleared by the f01 debug jump. The f01 map start and the event gate read it to
    // play it once.
    FLAG_SINDAIN_EVENT_285_SEEN = 285,
    // Set when the monster talk in d02 gives Max the Sundrop; the same talk handler reads it to
    // give it once.
    FLAG_SUNDROP_RECEIVED = 290,
    // Set by event 300 when the train arrives at Balance Valley Station (s04), and by the s17 story
    // jumps. The g02 map start, Lin's clinic (g02i02), s17 and s18 setup gate their chapter 3
    // scenes on it, and Cedric says he cannot leave map 16 yet until it is set.
    FLAG_BALANCE_VALLEY_ARRIVAL_SEEN = 300,
    // Set by event 302 in Balance Valley g02; the g02 map start reads it with flag 300.
    FLAG_BALANCE_VALLEY_E302_SEEN = 302,
    // Set by the scene where Max finds Lin in pain in the g02i02 clinic; the Dr. Dell recruit
    // check, Palm Brinks and the g02 house entry read it.
    FLAG_LIN_ILL_SCENE_SEEN = 304,
    // Set by the scene where Dr. Dell joins in the i05h01 house; the Dell recruit check in i05h01
    // and Palm Brinks read it.
    FLAG_DELL_JOIN_SCENE_SEEN = 305,
    // Set by event 306: Dr. Dell, now in the party, finds Lin poisoned by toxic mist and sends Max
    // to Lao Chao for a miracle dish, then a g02 scene explains the fields' heights. g02 refuses
    // entry to Starlight Canyon until it is set; Argo, the clinic and g02 NPC setup read it.
    FLAG_CLINIC_E306_SEEN = 306,
    // Set by event 307 on first reaching Starlight Canyon (d03) floor 2: a big bird drops Max, who
    // remembers Eina nursing him as a sick child (s46). d03 floor start reads it to play the event
    // once, and the g02 canyon entry skips the BGM load for floor 2 while it is unset.
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
    // Set by event 317 in s17, when Mrs. Condor gives Max the White Windflower after the Spheda
    // lesson, and also with flag 220 when dungeon starter items are given; s17 story jumps clear
    // it. Dungeon floor clear reads it to start Spheda, s17 and Argo's talk read it, and the engine
    // reads it in dng_debug.cpp and dngmenu.cpp.
    FLAG_SPHEDA_UNLOCKED = 317,
    // Set by event 320 with Lao Chao in the f02i08 restaurant; the clinic NPC, f02i08 and the s17
    // floor setup read it.
    FLAG_LAO_CHAO_REQUEST_HEARD = 320,
    // Set by event 324 when Lao Chao cooks the Miracle Dumplings in f02i08; the clinic replays also
    // set or clear it. Lin's clinic (g02i02), the g02 house entry and Argo read it to play the
    // dumpling delivery, and Starlight Temple villagers and f02i08 read it.
    FLAG_LAO_CHAO_COOKING_SEEN = 324,
    // Set by event 328, when the party brings Lao Chao's Miracle Dumplings back to Lin's clinic
    // (g02i02). The clinic map start and the g02 house entry read it with flag 324 to play the
    // delivery once.
    FLAG_CLINIC_E328_SEEN = 328,
    // Set by event 330 in Lin's clinic (g02i02), when Lin wakes after eating the dumplings but
    // cannot remember Crest's death; the clinic replays set or clear it. s18 scenes and exit lines
    // (Max worries about Lin until it is set), Argo, Starlight Temple villagers and the engine
    // (editanalyze.cpp) read it.
    FLAG_LIN_RECOVERED = 330,
    // Set by the first scene on s18; the s18 map start reads it with flag 300 to play it once.
    FLAG_S18_ARRIVAL_SEEN = 332,
    // Set by the scene where Lin tells Max she wants to be a sage in s18; nothing reads it.
    FLAG_LIN_WANTS_SAGE_SEEN = 336,
    // Set by the scene where Lin remembers the lighthouse in s18. The s18 exit only offers to go on
    // into d03 once it is set, and s18 setup keeps the e336 point enabled until then.
    FLAG_LIN_LIGHTHOUSE_MEMORY_SEEN = 340,
    // Set by the lighthouse arrival event in d03b01 and s56; lighthouse dungeon rooms and s56 read
    // it to disable the event point.
    FLAG_LIGHTHOUSE_ARRIVAL_SEEN = 348,
    // Set by event 352, where Lin is left to wait at the lighthouse (d03b01) and the party travels
    // back in time with the Starglass (s56). Lighthouse rooms and s56 read it to disable the event
    // point.
    FLAG_LIGHTHOUSE_E352_SEEN = 352,
    // Set by event 356 in d03b02, after Gaspard (Giltoni) is driven off and Crest shields everyone
    // from the battleship's shells. d03 floor 23 reads it to load d03b01 instead of s56, Argo reads
    // it, and the engine reads it in editanalyze.cpp.
    FLAG_CREST_FAREWELL_SEEN = 356,
    // Set by event 360 when the train leaves Balance Valley Station (s04) for Veniccio. Train crew
    // stop offering the departure from map 54, and Veniccio Station (s09) and g03 read it to play
    // the arrival scenes.
    FLAG_BALANCE_VALLEY_DEPARTURE_SEEN = 360,
    // Set by event 364 in Starlight Temple (f02), when Max and Monica find Lao Chao's Bistro open;
    // cleared by the f02 story_364 replay jump. The f02 event gate and the f02i08 bistro setup read
    // it.
    FLAG_BALANCE_VALLEY_EVENT_364_SEEN = 364,
    // Set by event 368 in Starlight Temple (f02), when the party talks about using a Starglass
    // after seeing the Star Items shop; cleared by the f02 story_368 replay jump. The f02 event
    // gate reads it.
    FLAG_BALANCE_VALLEY_EVENT_368_SEEN = 368,
    // Set by event 372: Max and Monica meet Lin as the Great Sage Crest in Starlight Temple (f02),
    // who sends them to the Luna Lab, then Cedric says the train is ready at s04; cleared by the
    // f02 replay jump. Train crew (Cedric offers departure on map 54 until flag 360), Jurak and
    // Starlight Temple villagers read it.
    FLAG_TRAIN_TO_VENICCIO_OPEN = 372,
    // Set by event 376 in Starlight Temple (f02) when Balance Valley is fully restored and Crest
    // gives Max a Serpent Slicer; cleared by the f02 story_376 replay jump. The f02 event gate
    // reads it.
    FLAG_BALANCE_VALLEY_EVENT_376_SEEN = 376,
    // Set by event 400 in s09 when Max and Monica first arrive at Veniccio Station (also by the s19
    // and d04 story replay warps); marks the start of the Veniccio chapter. s19, Shigura Village,
    // the Luna Lab doors and rooms, Starlight Temple villagers and train crew read it.
    FLAG_VENICCIO_ARRIVAL_SEEN = 400,
    // Set by event 402 in Veniccio (g03) when Max sees the sea for the first time and hears a cry
    // from the cave (also by the s19 story replays); the g03 arrival code reads it with flag 360 to
    // play the scene once.
    FLAG_FIRST_SIGHT_OF_SEA_SEEN = 402,
    // Set by event 404 when the party meets Pau tending the injured Shingala (s27, with a g03 copy
    // of the scene) and by the s19 replays. Opens the Ocean's Roar Cave entrance from Veniccio; g03
    // setup and cave exits and the s27 map start read it.
    FLAG_MET_PAU_AND_SHINGALA_SEEN = 404,
    // Set when Max first reaches Ocean's Roar Cave floor 2 and the sea memory door plays the s46
    // flashback of Eina reading the sea story (also by the s19 replays). d04 floor setup reads it
    // to play this once; g03 cave menus read it only to keep the BGM.
    FLAG_EINA_BEDTIME_STORY_SEEN = 406,
    // Set by event 408 when the party sees the Moon of Destruction murals in s19 and Shingala
    // attacks wearing a control device, then decides at Pau's House to ask Luna Lab. Starts the
    // Electric Fish plan: Osmond wakes and gives the Electric Worm; s19, s27, Lab 1 and Luna Lab
    // villagers read it.
    FLAG_SHINGALA_DEVICE_SEEN = 408,
    // Set by event 410 in s19 when the party, lacking the Electric Fish, backs away from the
    // controlled Shingala to make a plan; the s19 story replay clears it. Nothing reads it.
    FLAG_RETREAT_FROM_SHINGALA_SEEN = 410,
    // Set by event 412 in f03i0301 when Max is given the Electric Worm; f03 exits and the room
    // start read it.
    FLAG_ELECTRIC_WORM_GIVEN = 412,
    // Set by event 414 in Osmond's lab (f03i0301) when the party finds Osmond asleep at his desk
    // before flag 408; the lab's map start reads it to play the scene once.
    FLAG_OSMOND_ASLEEP_SEEN = 414,
    // Set by event 416 in s19 when Monica tells Max to feed the Electric Fish to Shingala and the
    // fight starts. The s19 game over handler reads it with flag 420 to play a voiced game over
    // during the fight.
    FLAG_SHINGALA_FIGHT_STARTED = 416,
    // Set by event 420 in s19 when the Electric Fish shuts off the device and Shingala is freed
    // (also by the following Pau scene in s27); cleared by story replays. Opens Luna Lab's Lab 2
    // door; s19, s27 Pau and Shiguras, and Osmond's talk read it.
    FLAG_SHINGALA_FREED_SEEN = 420,
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
    // Set by event 438 when the Secret Dragon Remedy heals Shingala at Pau's House (s27) and the
    // party sees Gaspard's battleship heading for Shigura Village (s40); the d04 story warps clear
    // it. With flag 444 unset it marks the village attack: Shigura Village, d04 floors, the g03
    // cave menu and s27 read it.
    FLAG_SHINGALA_HEALED_SEEN = 438,
    // Set by event 440 in Shigura Village when the Shigura turn out to be controlled by Gaspard's
    // Marionette devices and the boss fight starts; the village game over handler reads it with
    // flag 444 to play a voiced game over during the fight.
    FLAG_GASPARD_MARIONETTE_SEEN = 440,
    // Set by event 444 after the Shigura Village boss fight, when the elder thanks the party and
    // Pau and Shingala say goodbye; s27 story replays clear it. Ends the attack begun by flag 438:
    // the village, d04 floors, g03 cave menu, s27 and the Central Lab (f03i04, to play the Ixion
    // plan) read it, and so does the engine (editanalyze.cpp).
    FLAG_SHIGURA_RESCUE_SEEN = 444,
    // Set by event 448 in the Central Lab (f03i04) when Chap explains the Ixion plan, followed by
    // the s09 scene where the repaired bridge lets the party head for Mt. Gundor. Marks the end of
    // the Veniccio chapter: train crew (chapter 4 lines, leaving map 83), s09 and g03 map parts,
    // Luna Lab villagers and the Georama gate read it.
    FLAG_IXION_EXPLAINED_SEEN = 448,
    // Set by event 452 in s09 when Max and Monica board the train for Mt. Gundor; train crew and
    // party NPCs read it with flag 448 to allow leaving map 83, and the s10 stop reads it with flag
    // 500 to play the Mt. Gundor arrival.
    FLAG_VENICCIO_DEPARTURE_SEEN = 452,
    // Set by the scene where Max finds the building in f03; f03 villagers and the Georama gate read
    // it.
    FLAG_LUNAR_LAB_FOUND_SEEN = 456,
    // Set by the scene where Max finds the new lab in f03, cleared by the replay jump; the Georama
    // event gate reads it.
    FLAG_NEW_LAB_FOUND_SEEN = 460,
    // Set by the lunar lab arrival scene in f03i04; the lab map start reads it to play it once.
    FLAG_LUNAR_LAB_ARRIVAL_SEEN = 464,
    // Set by event 468 in Osmond's lab (f03i0301) when Osmond gives the Nova Cannon, played once
    // Georama town 2 (Luna Lab) is 100% rebuilt; the f03 and g03 event gates read it to play it
    // once.
    FLAG_VENICCIO_TOWN_COMPLETE_SEEN = 468,
    // Set by the chapter 5 arrival scene (event 500) at the s10 station, cleared and set again by
    // its story replay. Marks the start of chapter 5: Cedric lets the train leave s10 (map 83),
    // Borneo and Erik leave the s02, s04 and s09 stations, Cedric appears at s10, and Pau moves out
    // of the cave at s27.
    FLAG_HEIM_RADA_ARRIVAL_SEEN = 500,
    // Set by event 504 in s41, where Max and Monica watch Gaspard's ship cross the sky on the way
    // to Heim Rada. The s10 station exit reads it to send the player through s41 the first time and
    // straight to Heim Rada g04 after.
    FLAG_GASPARD_SHIP_SIGHTING_SEEN = 504,
    // Set by event 508 in the Gundorada Workshop (f04) when Garek gives Max the Time Bomb, which
    // plays on reaching elevator floor 1 once Georama analysis 403 is done; cleared by the replay
    // jump. s20 reads it (with 512) for the Fire Squall discovery scene and Mt. Gundor lines, and
    // the partner hints read it.
    FLAG_TIME_BOMB_GIVEN = 508,
    // Set by the fire rain source scene in s20; the s20 map start reads it with flag 508.
    FLAG_FIRE_RAIN_SOURCE_SEEN = 512,
    // Set by event 516 in s20, where the Time Bomb destroys the Fire Squall and Gaspard's
    // battleship attacks. The s20 setup reads it to show the wrecked Fire Squall and turn off the
    // Time Bomb use point, and the s20 dungeon entrance prompts and Mt. Gundor lines read it with
    // SAVE_FLAG_EDIT_BLOCKED.
    FLAG_FIRE_SQUALL_DESTROYED = 516,
    // Set by event 522, where Max's Atlamillia shines blue at the time gate; cleared by the f04
    // story 522 replay. The blue time gate warp reads it to play the scene once while Monica is
    // captured (flag 520 set, 532 clear).
    FLAG_BLUE_GATE_GLOW_SEEN = 522,
    // Set by the Atlamillia check scene at the time gate; nothing reads it.
    FLAG_ATLAMILLIA_CHECK_SEEN = 523,
    // Set by event 524 in the s39 operations room, where Galen gives Max the Fire Horn to cross the
    // lava after Monica falls into Mt. Gundor. The s39 map start reads it to play the scene once,
    // and the partner hints read it to say the Fire Horn will open the way.
    FLAG_FIRE_HORN_GIVEN = 524,
    // Set by the lava parting scene in d05b01; the Gaspard lava room setup and the partner hints
    // read it.
    FLAG_GASPARD_LAVA_PARTING_SEEN = 526,
    // Set by the Gaspard ambush scene in d05b01; the room setup and the boss loss handler read it.
    FLAG_GASPARD_AMBUSH_SEEN = 528,
    // Set by the Gaspard backstory, transformation and mother farewell scenes; the boss loss
    // handler and the blue time gate read it.
    FLAG_GASPARD_BACKSTORY_SEEN = 532,
    // Set by Gaspard's death scenes after the second fight in d05b01 and s49, and by the s30 story
    // replay. Marks the stage after Gaspard until the Paznos launch (flag 540): Galen offers the
    // counterattack at s30, workshop and Desoland villagers and partner hints change lines; the
    // engine reads it in editanalyze.cpp.
    FLAG_GASPARD_DEFEATED = 536,
    // Set by the event 540 scenes of the Paznos counterattack (launch in f04/s30, golem army battle
    // in s55, Eina farewell in s29); the s30 story replay clears it and plays them again. Marks the
    // stage after the battle until the Ixion unveiling: the Luna Lab central lab sends the player
    // to s53, and villagers, Desoland NPCs and partner hints change lines.
    FLAG_PAZNOS_COUNTERATTACK_DONE = 540,
    // Set by event 548 in the Gundorada Workshop (f04), the tour of the Great Workshop and Paznos,
    // once Heim Rada Georama analysis 403 is done; cleared by the f04 story 548 replay. The f04,
    // g04 and s39 event gates read it to play it once and pick the Georama hint text.
    FLAG_GREAT_WORKSHOP_TOUR_SEEN = 548,
    // Set by event 552 in the Gundorada Workshop (f04), where the party leader looks up, spots
    // something and runs off to check, once Heim Rada Georama analysis 406 is done; the story
    // replays set or clear it. The f04, g04 and s39 event gates read it to play it once and pick
    // the Georama hint text.
    FLAG_SOMETHING_ABOVE_SPOTTED_SEEN = 552,
    // Set by event 556 in the Gundorada Workshop (f04), where the party looks up at Paznos, once
    // Heim Rada Georama analysis 410 is done; the story replays set or clear it. The f04, g04 and
    // s39 event gates read it to play it once and pick the Georama hint text.
    FLAG_PAZNOS_SIGHTING_SEEN = 556,
    // Set by event 560 in s39, where Galen thanks the party for restoring Gundorada and gives
    // Monica the Holy Daedalus Blade, once the Heim Rada Georama is complete. The f04, g04 and s39
    // event gates read it; while the event is pending the g04 time gate goes to s39 instead of f04.
    FLAG_HEIM_RADA_COMPLETE_SEEN = 560,
    // Set by the Ixion unveiling in s53 and again by each time leap scene after it (s59, s58, s37)
    // and the arrival in the past in s31; marks the Moon Flower Palace era stage. The engine world
    // map reads it (SAVE_FLAG_WORLD_MAP_2), as do the partner hints, Luna Lab villagers, the
    // central lab redirect to s53 and the s33 approach scene.
    FLAG_IXION_UNVEILING_SEEN = 600,
    // Set by the Griffon castle approach scene in s33; the s33 map start reads it with flag 600.
    FLAG_GRIFFON_CASTLE_APPROACH_SEEN = 604,
    // Set by the Emperor Griffon reveal scene in d06e01, which starts that fight; the d06e01 game
    // over plays one of two voiced lines while this is set and flag 612 is not.
    FLAG_EMPEROR_GRIFFON_REVEAL_SEEN = 608,
    // Set by the Griffon hatred vision scene in d06e01; nothing reads it.
    FLAG_GRIFFON_HATRED_VISION_SEEN = 610,
    // Set by the d06e01 scenes where Griffon takes the Atlamillia, and again by the s31 quake and
    // s35 lift scenes after them. The time gate scenes read it to travel with the Ixion's mechanism
    // (with a caption saying so) in place of the Atlamillia; the d06e01 game over plays Griffon's
    // voiced lines while flag 608 is set and this is not.
    FLAG_GRIFFON_TAKES_ATLAMILLIA_SEEN = 612,
    // Set by event 616 in s70, where Osmond reports that the Moon Flower Palace has returned to the
    // present, which starts the Lapis search. The engine world map reads it
    // (SAVE_FLAG_WORLD_MAP_4); the star key gates in d02 to d05 stay shut until it is set, and
    // partner hints and Luna Lab, workshop and Desoland villagers read it.
    FLAG_OSMOND_REPORT_SEEN = 616,
    // Set when the Earth Lapis is taken in s13 (Sleeping Earth Chamber, off d02). Until then s13
    // plays the Trentos guardian fight and its exit refuses to leave; the partner hints read it
    // with flags 628, 632 and 636.
    FLAG_LAPIS_STONE_1_FOUND = 624,
    // Set when the Wind Lapis is taken in s14 (Defending Wind Chamber, off d03). Until then s14
    // sets up its guardian boss fight and its exit refuses to leave; the partner hints read it with
    // flags 624, 632 and 636.
    FLAG_LAPIS_STONE_2_FOUND = 628,
    // Set when the Water Lapis is taken in s15 (Sleeping Water Chamber, off d04). Until then s15
    // sets up its sea dragon guardian fight and its exit refuses to leave; the partner hints read
    // it with flags 624, 628 and 636.
    FLAG_LAPIS_STONE_3_FOUND = 632,
    // Set when the Fire Lapis is taken in s16 (Blazing Fire Chamber, off d05). Until then s16 sets
    // up its guardian fight and its exit refuses to leave; the partner hints read it with flags
    // 624, 628 and 632.
    FLAG_LAPIS_STONE_4_FOUND = 636,
    // Set by the event 640 scenes: the Lapis gathered and the Chrono Union fired in s11, then
    // Griffon's challenge in d06b01 and the Paznos battle with the palace in s32, s30 and s66/s98.
    // Marks the stage after the Lapis; the engine world map (SAVE_FLAG_WORLD_MAP_3), partner hints,
    // the s11 station gate and Osmond's departure to s37, and villagers read it.
    FLAG_CHRONO_UNION_ACTIVATED = 640,
    // Set by the event 648 scenes: Eina's last briefing aboard the Ixion in s37, the train lifting
    // off from s11 and the party reaching the Moon Flower Palace stop s12. Until then the s11
    // station NPC 301 offers to leave for s37; afterwards the waiting party NPC tells the player to
    // beat Griffon.
    FLAG_FINAL_BATTLE_DEPARTURE_SEEN = 648,
    // Set by event 700 on first reaching the Moon Flower Palace: Alexandra's welcome in s65/s68 and
    // Andrea's plea to restore the garden in g05. Once set, leaving the s12 stop goes to g05
    // instead of the s68 arrival scene, and the engine world map does the same
    // (SAVE_FLAG_WORLD_MAP_S12_REDIRECT); partner hints read it.
    FLAG_ANDREA_GARDEN_CLUE_SEEN = 700,
    // Set by the first scene on s21; the s21 map start reads it to play it once.
    FLAG_S21_FIRST_VISIT_SEEN = 704,
    // Set by event 708 in s50, where the broken flower is forgiven and the doors open. Once set,
    // s50 shows the doors open and lets the player through to the next palace floor, and s21 no
    // longer offers the Flower of the Sun pickup.
    FLAG_FLOWER_OF_SUN_SCENE_SEEN = 708,
    // Set by event 712 at the door to Griffon's chamber in s23, whether the guardian turns the
    // party away or asks for his master's true name. Only the partner hint messages read it.
    FLAG_GRIFFON_DOOR_GUARDIAN_SEEN = 712,
    // Set by the scene where Griffon confronts the party in d06b01, which starts that fight; the
    // d06b01 game over plays one of two voiced lines while this is set and flag 728 is not.
    FLAG_GRIFFON_APPEARS_SEEN = 716,
    // Set by event 720: Griffon spreads his wings before the second phase of the d06b01 fight, and
    // the s64 scene where Alexandra sends Sirus away. Nothing reads it.
    FLAG_WINGED_BOSS_TRANSFORM_SEEN = 720,
    // Set by event 724: Griffon rises wreathed in flame before the third phase of the d06b01 fight,
    // and the s31 scene where a lone figure laments humans fighting over stones. Nothing reads it.
    FLAG_BOSS_AWAKENS_WITH_FLAME_SEEN = 724,
    // Set by the event 728 scenes: Griffon is struck down in d06b01, then the d06b02 Dream Spiral
    // monster rooms and the Dark Element's awakening in d06b03. While set, d06b02 and d06b03 set up
    // those fights and the d06b03 game over is voiced until flag 732; partner hints read it as the
    // last stretch.
    FLAG_GRIFFON_DEFEATED = 728,
    // Set by event 732 when the Dark Element is beaten: it dies in d06b03/s67, Sirus breaks the
    // moon and fades away. The d06b03 game over stops using voiced lines once it is set, and the
    // partner hints read it as the game's end.
    FLAG_DARK_ELEMENT_DEFEATED = 732,
    // Set by the farewell scene in s66/s98 and again by the i04h01 ending, which saves the chapter
    // 8 start file. The engine menu reads it (menumain.cpp); Palm Brinks m01 reads it with flag 800
    // to play Need's Zelmite request when that file is loaded.
    FLAG_ENDING_SEEN = 736,
    // Set by the scene where Andrea finds Sirus in s44, and by Moon Flower Palace g05 before it;
    // g05 reads it to see whether the analysis scene is pending.
    FLAG_ANDREA_FINDS_SIRUS_SEEN = 744,
    // Set by event 748 in s44, the flashback where the Queen names the garden boy Sirus, which g05
    // plays once the palace Georama is complete. Until it is set, the s23 door guardian turns the
    // party away instead of asking for the password.
    FLAG_MOON_FLOWER_FLASHBACK_SEEN = 748,
    // Set by Need's Zelmite request in Palm Brinks m01, played on loading the chapter 8 file, and
    // cleared by the i04h01 ending that saves that file. The m01 Palm Mine entrances open the stage
    // 6 tree map only once it is set; the engine reads it in editanalyze.cpp and menumain.cpp.
    FLAG_POSTGAME_SCENE_SEEN = 800,
    // Set by the scene where Monica arrives in the mine in s52; chapter 8 villagers read it to
    // change their lines.
    FLAG_MONICA_BACK_IN_MINE_SEEN = 804,
    // Set by the scene where Flotsam is the new boss in s54; the s54 room setup and loss handler
    // read it.
    FLAG_FLOTSAM_NEW_BOSS_SEEN = 808,
    // Set by the Flotsam defeated scene in s54; the s54 loss handler reads it.
    FLAG_FLOTSAM_DEFEATED_SEEN = 812,
    // Set by the d07b01 scene where Max and Monica find Zelmite Ore and the Dark Genie appears,
    // which starts that fight; the d07b01 game over plays one of two voiced lines while this is set
    // and flag 820 is not.
    FLAG_ZELMITE_BOSS_APPEARS_SEEN = 816,
    // Set by the scene after the Dark Genie is beaten and shrinks away in d07b01, where Max and
    // Monica decide to tell Mayor Need about the Zelmite. Once set, d07b01 hides the boss room
    // part, d07 floor 0 exits to Palm Brinks (edit map 172) instead of the park, and the game over
    // is no longer voiced.
    FLAG_DARK_GENIE_DEFEATED = 820,
    // Set by the event 824 scenes in s60: the speech to the citizens of Palm Brinks about today's
    // victory and the cheering crowd and town views. Nothing reads it.
    FLAG_PALM_BRINKS_VICTORY_SEEN = 824,
};

// Music files: load_bgm(n) and load_bgm_pack(n) open snd2/bgm/BG_%03d.snd (CScene::GetBgmFile,
// scenesnd.cpp). Named from the title in each file's cfg header; play_bgm and stop_bgm take a
// track inside the loaded file, not one of these.
enum bgm_file_pal {
    BGM_PALM_BRINKS = 0,
    BGM_SINDAIN = 1,
    BGM_BALANCE_VALLEY = 2,
    BGM_VENICCIO = 3,
    BGM_HEIM_RADA = 4,
    BGM_KAZAROV_STONEHENGE = 5,
    BGM_MOON_FLOWER_PALACE_GEORAMA = 6,
    BGM_UNDERGROUND_CHANNEL = 15,
    BGM_RAINBOW_BUTTERFLY_WOOD = 16,
    BGM_STARLIGHT_CANYON = 17,
    BGM_OCEANS_ROAR_CAVE = 18,
    BGM_MOUNT_GUNDOR = 19,
    BGM_MOON_FLOWER_PALACE_DUNGEON = 21,
    BGM_ZELMITE_MINE = 22,
    BGM_BOSS_CLOWN_ROBOT = 30,  // name partly a guess
    BGM_BOSS_RAINBOW_BUTTERFLY = 31,
    BGM_BOSS_GASPARD_BATTLESHIP = 32,  // name partly a guess
    BGM_BOSS_LIGHTHOUSE_FIRE = 33,
    BGM_BOSS_DR_JAMING = 34,
    BGM_BOSS_GASPARD = 35,  // name partly a guess
    BGM_BOSS_GRIFFON_MOON_PEOPLE = 36,
    // Titled as the Dream Spiral boss theme, but every dungeon plays it on the floor before the boss.
    BGM_BEFORE_BOSS_FLOOR = 37,
    BGM_BOSS_GRIFFON_FORMS_2_3 = 38,
    BGM_BOSS_DARK_ELEMENT = 39,
    BGM_BOSS_DEMON = 40,  // name partly a guess
    BGM_BOSS_GRIFFON_FORM_1 = 41,
    BGM_INDOORS_1 = 46,
    BGM_CHURCH = 47,
    BGM_MENU = 48,
    BGM_HOPEFUL_SCENE = 49,
    BGM_SHOP = 50,
    BGM_TENSE_SCENE = 51,
    BGM_SAD_SCENE = 52,
    BGM_MEMORIES_SCENE = 53,
    BGM_INDOORS_2 = 54,
    BGM_INSIDE_TRAIN = 55,
    BGM_TRAIN_STATION = 56,
    BGM_NANAHIGE_THEME = 57,  // name partly a guess
    BGM_MOTHER_APPEARS = 58,
    BGM_FUTURE_EVENT = 59,
    BGM_GAME_OVER = 60,
    BGM_OPENING_RAIN = 100,
    BGM_OPENING_TALK_WITH_STARBLE = 101,  // name partly a guess
    BGM_OPENING_OUTSIDE_CIRCUS_TENT = 102,
    BGM_OPENING_INSIDE_CIRCUS_TENT = 103,
    BGM_OPENING_RED_HAIRED_BOY = 104,
    BGM_OPENING_SECRET_TALK = 105,
    BGM_OPENING_CIRCUS_ESCAPE = 106,
    BGM_OPENING_TOWN_CLOWN_ESCAPE = 107,
    BGM_OPENING_TALK_WITH_DONNY = 108,
    BGM_OPENING_CIRCUS_DISTANT = 109,
    BGM_OPENING_CLOWN_ROBOT = 110,
    BGM_FIRST_EXIT_FROM_UNDERGROUND_CHANNEL = 111,
    BGM_LIN_MEMORIES = 112,
    BGM_FLOWER_GARDEN_RECOLLECTION = 113,
    BGM_PAZNOS_FORTRESS_THEME = 114,  // name partly a guess
    BGM_IXION_THEME = 115,
    BGM_MOON_FLOWER_PALACE_VS_PAZNOS = 116,
    BGM_GRIFFON_APPROACHES = 117,
    BGM_HOPE_BEFORE_ENDING = 118,
    BGM_JURAK_MALL = 151,
    BGM_STARLIGHT_TEMPLE = 152,
    BGM_LUNATIC_WISDOM_LAB = 153,
    BGM_GUNDORADA_WORKSHOP = 154,
    BGM_FISH_RACE = 165,
    BGM_MOON_FLOWER_PALACE_EXTERIOR = 166,
    BGM_ANCIENT_MURAL = 167,
    BGM_MIDBOSS_BATTLE = 190,
    BGM_POLKAS_BUGGY_BATTLE = 191,
    BGM_VIDEO_TUTORIAL = 192,
    BGM_PEACE_AND_FISHING = 193,
    BGM_MOON_FLOWER_PALACE_BATTLE_EVENT = 194,
    BGM_NORMAL_BATTLE_EVENT = 195,
};

// Sound sets: load_sound_set(n) loads row n of snd2/MAP_index.txt (CScene::LoadSndFileInfo): the
// map's default music and its base, battle, ambient, object and event sound banks. Named for the
// area, the story scene the row's description starts with (e283a) or its kind (ar area, bt
// battle), and the row number.
enum sound_set_pal {
    SOUND_SET_DEFAULT_BS_0 = 0,
    SOUND_SET_TITLE_SN_500 = 500,
    SOUND_SET_TRAIN_AR_510 = 510,
    SOUND_SET_INDOORS_COMMON_AR_511 = 511,
    SOUND_SET_DEATH_SN_512 = 512,
    SOUND_SET_IXION_AR_513 = 513,
    SOUND_SET_IXION_AR_514 = 514,
    SOUND_SET_TRAIN_AR_516 = 516,
    SOUND_SET_FISHING_SN_517 = 517,
    SOUND_SET_ENEMY_WIPEOUT_AND_SPHEDA_SN_518 = 518,
    SOUND_SET_GEORAMA_IRON_HOUSE_AR_519 = 519,
    SOUND_SET_GEORAMA_INDOORS_COMMON_AR_520 = 520,
    SOUND_SET_FUTURE_INDOORS_COMMON_AR_521 = 521,
    SOUND_SET_GEORAMA_CHURCH_COMMON_AR_522 = 522,
    SOUND_SET_RAYBRANDT_CASTLE_E050A_800 = 800,
    SOUND_SET_RAYBRANDT_CASTLE_E050A_801 = 801,
    SOUND_SET_RAYBRANDT_CASTLE_E050A_802 = 802,
    SOUND_SET_PALM_BRINKS_BS_1000 = 1000,
    SOUND_SET_PALM_BRINKS_AR_1080 = 1080,
    SOUND_SET_PALM_BRINKS_AR_1081 = 1081,
    SOUND_SET_PALM_BRINKS_AR_1082 = 1082,
    SOUND_SET_PALM_BRINKS_SN_1083 = 1083,
    SOUND_SET_PALM_BRINKS_SN_1084 = 1084,
    SOUND_SET_PALM_BRINKS_SN_1085 = 1085,
    SOUND_SET_PALM_BRINKS_E001A_1200 = 1200,
    SOUND_SET_PALM_BRINKS_E001A_1201 = 1201,
    SOUND_SET_PALM_BRINKS_E001A_1202 = 1202,
    SOUND_SET_PALM_BRINKS_E001A_1203 = 1203,
    SOUND_SET_PALM_BRINKS_E001A_1204 = 1204,
    SOUND_SET_PALM_BRINKS_E001A_1205 = 1205,
    SOUND_SET_PALM_BRINKS_E001A_1206 = 1206,
    SOUND_SET_PALM_BRINKS_E002A_1207 = 1207,
    SOUND_SET_PALM_BRINKS_E002A_1208 = 1208,
    SOUND_SET_PALM_BRINKS_E002A_1209 = 1209,
    SOUND_SET_PALM_BRINKS_E002A_1210 = 1210,
    SOUND_SET_PALM_BRINKS_E002A_1211 = 1211,
    SOUND_SET_PALM_BRINKS_E002A_1212 = 1212,
    SOUND_SET_PALM_BRINKS_E001B_1213 = 1213,
    SOUND_SET_PALM_BRINKS_E002A_1214 = 1214,
    SOUND_SET_PALM_BRINKS_E004A_1215 = 1215,
    SOUND_SET_PALM_BRINKS_E005A_1216 = 1216,
    SOUND_SET_PALM_BRINKS_E005A_1217 = 1217,
    SOUND_SET_PALM_BRINKS_E005A_1218 = 1218,
    SOUND_SET_PALM_BRINKS_E006A_1219 = 1219,
    SOUND_SET_PALM_BRINKS_E006A_1220 = 1220,
    SOUND_SET_PALM_BRINKS_E105A_1222 = 1222,
    SOUND_SET_PALM_BRINKS_E105B_1225 = 1225,
    SOUND_SET_PALM_BRINKS_E105C_1228 = 1228,
    SOUND_SET_PALM_BRINKS_E106A_1231 = 1231,
    SOUND_SET_PALM_BRINKS_E108B_1234 = 1234,
    SOUND_SET_PALM_BRINKS_E108C_1237 = 1237,
    SOUND_SET_PALM_BRINKS_E109A_1240 = 1240,
    SOUND_SET_PALM_BRINKS_E109B_1243 = 1243,
    SOUND_SET_PALM_BRINKS_E109B_1246 = 1246,
    SOUND_SET_PALM_BRINKS_E254C_1247 = 1247,
    SOUND_SET_PALM_BRINKS_E305A_1249 = 1249,
    SOUND_SET_PALM_BRINKS_E344A_1251 = 1251,
    SOUND_SET_PALM_BRINKS_E424A_1253 = 1253,
    SOUND_SET_PALM_BRINKS_E060A_1256 = 1256,
    SOUND_SET_PALM_BRINKS_E061A_1259 = 1259,
    SOUND_SET_PALM_BRINKS_E800A_1261 = 1261,
    SOUND_SET_PALM_BRINKS_E824A_1264 = 1264,
    SOUND_SET_UNDERGROUND_CHANNEL_BS_1600 = 1600,
    SOUND_SET_UNDERGROUND_CHANNEL_SN_1601 = 1601,
    SOUND_SET_UNDERGROUND_CHANNEL_AR_1602 = 1602,
    SOUND_SET_UNDERGROUND_CHANNEL_AR_1603 = 1603,
    SOUND_SET_UNDERGROUND_CHANNEL_AR_1682 = 1682,
    SOUND_SET_UNDERGROUND_CHANNEL_AR_1683 = 1683,
    SOUND_SET_UNDERGROUND_CHANNEL_AR_1684 = 1684,
    SOUND_SET_UNDERGROUND_CHANNEL_BT_1690 = 1690,
    SOUND_SET_UNDERGROUND_CHANNEL_BT_1693 = 1693,
    SOUND_SET_UNDERGROUND_CHANNEL_E007A_1800 = 1800,
    SOUND_SET_UNDERGROUND_CHANNEL_E007A_1801 = 1801,
    SOUND_SET_UNDERGROUND_CHANNEL_E007A_1802 = 1802,
    SOUND_SET_UNDERGROUND_CHANNEL_E100A_1803 = 1803,
    SOUND_SET_UNDERGROUND_CHANNEL_E101A_1804 = 1804,
    SOUND_SET_UNDERGROUND_CHANNEL_E102A_1807 = 1807,
    SOUND_SET_UNDERGROUND_CHANNEL_E102A_1808 = 1808,
    SOUND_SET_UNDERGROUND_CHANNEL_E103A_1810 = 1810,
    SOUND_SET_UNDERGROUND_CHANNEL_E104A_1813 = 1813,
    SOUND_SET_UNDERGROUND_CHANNEL_E107A_1816 = 1816,
    SOUND_SET_UNDERGROUND_CHANNEL_E108A_1819 = 1819,
    SOUND_SET_UNDERGROUND_CHANNEL_E108B_1822 = 1822,
    SOUND_SET_UNDERGROUND_CHANNEL_E108C_1825 = 1825,
    SOUND_SET_UNDERGROUND_CHANNEL_E110A_1828 = 1828,
    SOUND_SET_UNDERGROUND_CHANNEL_E111A_1831 = 1831,
    SOUND_SET_UNDERGROUND_CHANNEL_E201A_1834 = 1834,
    SOUND_SET_UNDERGROUND_CHANNEL_E202A_1837 = 1837,
    SOUND_SET_SINDAIN_BS_2000 = 2000,
    SOUND_SET_SINDAIN_AR_2080 = 2080,
    SOUND_SET_SINDAIN_AR_2081 = 2081,
    SOUND_SET_SINDAIN_E203A_2200 = 2200,
    SOUND_SET_SINDAIN_E204A_2203 = 2203,
    SOUND_SET_SINDAIN_E205A_2206 = 2206,
    SOUND_SET_SINDAIN_E206A_2209 = 2209,
    SOUND_SET_SINDAIN_E207A_2211 = 2211,
    SOUND_SET_SINDAIN_E208A_2214 = 2214,
    SOUND_SET_SINDAIN_E209A_2217 = 2217,
    SOUND_SET_SINDAIN_E209A_2220 = 2220,
    SOUND_SET_SINDAIN_E209A_2223 = 2223,
    SOUND_SET_SINDAIN_E250A_2226 = 2226,
    SOUND_SET_SINDAIN_E250A_2229 = 2229,
    SOUND_SET_SINDAIN_E250A_2231 = 2231,
    SOUND_SET_SINDAIN_E250A_2234 = 2234,
    SOUND_SET_SINDAIN_E283A_2237 = 2237,
    SOUND_SET_SINDAIN_E254A_2238 = 2238,
    SOUND_SET_SINDAIN_E254A_2239 = 2239,
    SOUND_SET_SINDAIN_E254B_2240 = 2240,
    SOUND_SET_JURAK_MALL_BS_2300 = 2300,
    SOUND_SET_JURAK_MALL_E270A_2500 = 2500,
    SOUND_SET_JURAK_MALL_E272A_2503 = 2503,
    SOUND_SET_JURAK_MALL_E275A_2506 = 2506,
    SOUND_SET_JURAK_MALL_E277A_2509 = 2509,
    SOUND_SET_JURAK_MALL_E280A_2511 = 2511,
    SOUND_SET_JURAK_MALL_E280A_2514 = 2514,
    SOUND_SET_JURAK_MALL_E280A_2515 = 2515,
    SOUND_SET_JURAK_MALL_E285A_2517 = 2517,
    SOUND_SET_RAINBOW_BUTTERFLY_WOOD_BS_2600 = 2600,
    SOUND_SET_RAINBOW_BUTTERFLY_WOOD_SN_2601 = 2601,
    SOUND_SET_RAINBOW_BUTTERFLY_WOOD_AR_2602 = 2602,
    SOUND_SET_RAINBOW_BUTTERFLY_WOOD_AR_2603 = 2603,
    SOUND_SET_RAINBOW_BUTTERFLY_WOOD_AR_2683 = 2683,
    SOUND_SET_RAINBOW_BUTTERFLY_WOOD_AR_2684 = 2684,
    SOUND_SET_RAINBOW_BUTTERFLY_WOOD_BT_2690 = 2690,
    SOUND_SET_RAINBOW_BUTTERFLY_WOOD_BT_2693 = 2693,
    SOUND_SET_RAINBOW_BUTTERFLY_WOOD_AR_2694 = 2694,
    SOUND_SET_RAINBOW_BUTTERFLY_WOOD_E215A_2800 = 2800,
    SOUND_SET_RAINBOW_BUTTERFLY_WOOD_E212A_2801 = 2801,
    SOUND_SET_RAINBOW_BUTTERFLY_WOOD_E220A_2803 = 2803,
    SOUND_SET_RAINBOW_BUTTERFLY_WOOD_E230A_2806 = 2806,
    SOUND_SET_RAINBOW_BUTTERFLY_WOOD_E235A_2809 = 2809,
    SOUND_SET_RAINBOW_BUTTERFLY_WOOD_E240A_2812 = 2812,
    SOUND_SET_RAINBOW_BUTTERFLY_WOOD_E245A_2815 = 2815,
    SOUND_SET_RAINBOW_BUTTERFLY_WOOD_E247A_2818 = 2818,
    SOUND_SET_RAINBOW_BUTTERFLY_WOOD_E253A_2821 = 2821,
    SOUND_SET_RAINBOW_BUTTERFLY_WOOD_E255A_2824 = 2824,
    SOUND_SET_RAINBOW_BUTTERFLY_WOOD_E255B_2827 = 2827,
    SOUND_SET_RAINBOW_BUTTERFLY_WOOD_E256A_2830 = 2830,
    SOUND_SET_RAINBOW_BUTTERFLY_WOOD_E257A_2833 = 2833,
    SOUND_SET_RAINBOW_BUTTERFLY_WOOD_E260A_2836 = 2836,
    SOUND_SET_RAINBOW_BUTTERFLY_WOOD_E624A_2841 = 2841,
    SOUND_SET_BALANCE_VALLEY_BS_3000 = 3000,
    SOUND_SET_BALANCE_VALLEY_AR_3080 = 3080,
    SOUND_SET_BALANCE_VALLEY_E300A_3200 = 3200,
    SOUND_SET_BALANCE_VALLEY_E302A_3203 = 3203,
    SOUND_SET_BALANCE_VALLEY_E304A_3206 = 3206,
    SOUND_SET_BALANCE_VALLEY_E306A_3209 = 3209,
    SOUND_SET_BALANCE_VALLEY_E306A_3211 = 3211,
    SOUND_SET_BALANCE_VALLEY_E328A_3214 = 3214,
    SOUND_SET_BALANCE_VALLEY_E330A_3217 = 3217,
    SOUND_SET_BALANCE_VALLEY_E372A_3220 = 3220,
    SOUND_SET_BALANCE_VALLEY_E360A_3223 = 3223,
    SOUND_SET_STELLA_POTO_BS_3300 = 3300,
    SOUND_SET_STELLA_POTO_AR_3381 = 3381,
    SOUND_SET_STELLA_POTO_E320A_3500 = 3500,
    SOUND_SET_STELLA_POTO_E324A_3503 = 3503,
    SOUND_SET_STELLA_POTO_E364A_3506 = 3506,
    SOUND_SET_STELLA_POTO_E368A_3509 = 3509,
    SOUND_SET_STELLA_POTO_E372A_3511 = 3511,
    SOUND_SET_STELLA_POTO_E376A_3514 = 3514,
    SOUND_SET_STARLIGHT_CANYON_BS_3600 = 3600,
    SOUND_SET_STARLIGHT_CANYON_SN_3601 = 3601,
    SOUND_SET_STARLIGHT_CANYON_AR_3602 = 3602,
    SOUND_SET_STARLIGHT_CANYON_AR_3603 = 3603,
    SOUND_SET_STARLIGHT_CANYON_AR_3682 = 3682,
    SOUND_SET_STARLIGHT_CANYON_AR_3683 = 3683,
    SOUND_SET_STARLIGHT_CANYON_AR_3684 = 3684,
    SOUND_SET_STARLIGHT_CANYON_AR_3685 = 3685,
    SOUND_SET_STARLIGHT_CANYON_AR_3686 = 3686,
    SOUND_SET_STARLIGHT_CANYON_AR_3687 = 3687,
    SOUND_SET_STARLIGHT_CANYON_AR_3688 = 3688,
    SOUND_SET_STARLIGHT_CANYON_BT_3690 = 3690,
    SOUND_SET_STARLIGHT_CANYON_BT_3691 = 3691,
    SOUND_SET_STARLIGHT_CANYON_BT_3693 = 3693,
    SOUND_SET_STARLIGHT_CANYON_AR_3694 = 3694,
    SOUND_SET_STARLIGHT_CANYON_E307A_3800 = 3800,
    SOUND_SET_STARLIGHT_CANYON_E307A_3801 = 3801,
    SOUND_SET_STARLIGHT_CANYON_E308A_3803 = 3803,
    SOUND_SET_STARLIGHT_CANYON_E312A_3806 = 3806,
    SOUND_SET_STARLIGHT_CANYON_E314A_3809 = 3809,
    SOUND_SET_STARLIGHT_CANYON_E315A_3811 = 3811,
    SOUND_SET_STARLIGHT_CANYON_E316A_3814 = 3814,
    SOUND_SET_STARLIGHT_CANYON_E317A_3817 = 3817,
    SOUND_SET_STARLIGHT_CANYON_E332A_3820 = 3820,
    SOUND_SET_STARLIGHT_CANYON_E336A_3823 = 3823,
    SOUND_SET_STARLIGHT_CANYON_E340A_3826 = 3826,
    SOUND_SET_STARLIGHT_CANYON_E348A_3829 = 3829,
    SOUND_SET_STARLIGHT_CANYON_E352A_3831 = 3831,
    SOUND_SET_STARLIGHT_CANYON_E352A_3833 = 3833,
    SOUND_SET_STARLIGHT_CANYON_E352A_3836 = 3836,
    SOUND_SET_STARLIGHT_CANYON_E352A_3839 = 3839,
    SOUND_SET_STARLIGHT_CANYON_E356A_3841 = 3841,
    SOUND_SET_STARLIGHT_CANYON_E356A_3847 = 3847,
    SOUND_SET_STARLIGHT_CANYON_E628A_3850 = 3850,
    SOUND_SET_VENICCIO_BS_4000 = 4000,
    SOUND_SET_VENICCIO_AR_4080 = 4080,
    SOUND_SET_VENICCIO_AR_4082 = 4082,
    SOUND_SET_VENICCIO_E400A_4200 = 4200,
    SOUND_SET_VENICCIO_E402A_4203 = 4203,
    SOUND_SET_VENICCIO_E404A_4206 = 4206,
    SOUND_SET_VENICCIO_E404A_4207 = 4207,
    SOUND_SET_VENICCIO_E408A_4209 = 4209,
    SOUND_SET_VENICCIO_E420A_4211 = 4211,
    SOUND_SET_VENICCIO_E438A_4214 = 4214,
    SOUND_SET_VENICCIO_E438A_4215 = 4215,
    SOUND_SET_VENICCIO_E438A_4216 = 4216,
    SOUND_SET_VENICCIO_E438A_4217 = 4217,
    SOUND_SET_VENICCIO_E448A_4220 = 4220,
    SOUND_SET_VENICCIO_E452A_4223 = 4223,
    SOUND_SET_LUNATIC_WISDOM_LAB_BS_4300 = 4300,
    SOUND_SET_LUNATIC_WISDOM_LAB_AR_4382 = 4382,
    SOUND_SET_LUNATIC_WISDOM_LAB_AR_4383 = 4383,
    SOUND_SET_LUNATIC_WISDOM_LAB_E412A_4500 = 4500,
    SOUND_SET_LUNATIC_WISDOM_LAB_E414A_4503 = 4503,
    SOUND_SET_LUNATIC_WISDOM_LAB_E428A_4506 = 4506,
    SOUND_SET_LUNATIC_WISDOM_LAB_E448A_4509 = 4509,
    SOUND_SET_LUNATIC_WISDOM_LAB_E456A_4511 = 4511,
    SOUND_SET_LUNATIC_WISDOM_LAB_E460A_4514 = 4514,
    SOUND_SET_LUNATIC_WISDOM_LAB_E464A_4517 = 4517,
    SOUND_SET_LUNATIC_WISDOM_LAB_E468A_4520 = 4520,
    SOUND_SET_LUNATIC_WISDOM_LAB_E600A_4523 = 4523,
    SOUND_SET_LUNATIC_WISDOM_LAB_E600A_4524 = 4524,
    SOUND_SET_LUNATIC_WISDOM_LAB_E600A_4525 = 4525,
    SOUND_SET_LUNATIC_WISDOM_LAB_E600A_4526 = 4526,
    SOUND_SET_LUNATIC_WISDOM_LAB_E600A_4527 = 4527,
    SOUND_SET_OCEANS_ROAR_CAVE_BS_4600 = 4600,
    SOUND_SET_OCEANS_ROAR_CAVE_SN_4601 = 4601,
    SOUND_SET_OCEANS_ROAR_CAVE_AR_4602 = 4602,
    SOUND_SET_OCEANS_ROAR_CAVE_AR_4603 = 4603,
    SOUND_SET_OCEANS_ROAR_CAVE_AR_4682 = 4682,
    SOUND_SET_OCEANS_ROAR_CAVE_AR_4683 = 4683,
    SOUND_SET_OCEANS_ROAR_CAVE_AR_4684 = 4684,
    SOUND_SET_OCEANS_ROAR_CAVE_BT_4690 = 4690,
    SOUND_SET_OCEANS_ROAR_CAVE_BT_4693 = 4693,
    SOUND_SET_OCEANS_ROAR_CAVE_AR_4694 = 4694,
    SOUND_SET_OCEANS_ROAR_CAVE_E406A_4800 = 4800,
    SOUND_SET_OCEANS_ROAR_CAVE_E406A_4801 = 4801,
    SOUND_SET_OCEANS_ROAR_CAVE_E408A_4803 = 4803,
    SOUND_SET_OCEANS_ROAR_CAVE_E410A_4806 = 4806,
    SOUND_SET_OCEANS_ROAR_CAVE_E416A_4809 = 4809,
    SOUND_SET_OCEANS_ROAR_CAVE_E420A_4811 = 4811,
    SOUND_SET_OCEANS_ROAR_CAVE_E432A_4814 = 4814,
    SOUND_SET_OCEANS_ROAR_CAVE_E434A_4817 = 4817,
    SOUND_SET_OCEANS_ROAR_CAVE_E436A_4820 = 4820,
    SOUND_SET_OCEANS_ROAR_CAVE_E436A_4821 = 4821,
    SOUND_SET_OCEANS_ROAR_CAVE_E436A_4822 = 4822,
    SOUND_SET_OCEANS_ROAR_CAVE_E440A_4826 = 4826,
    SOUND_SET_OCEANS_ROAR_CAVE_E444A_4829 = 4829,
    SOUND_SET_OCEANS_ROAR_CAVE_E632A_4831 = 4831,
    SOUND_SET_HEIM_RADA_BS_5000 = 5000,
    SOUND_SET_HEIM_RADA_AR_5080 = 5080,
    SOUND_SET_HEIM_RADA_E500A_5200 = 5200,
    SOUND_SET_HEIM_RADA_E504A_5203 = 5203,
    SOUND_SET_GUNDORADA_WORKSHOP_BS_5300 = 5300,
    SOUND_SET_GUNDORADA_WORKSHOP_AR_5382 = 5382,
    SOUND_SET_GUNDORADA_WORKSHOP_AR_5383 = 5383,
    SOUND_SET_GUNDORADA_WORKSHOP_AR_5384 = 5384,
    SOUND_SET_GUNDORADA_WORKSHOP_E508A_5500 = 5500,
    SOUND_SET_GUNDORADA_WORKSHOP_E522A_5503 = 5503,
    SOUND_SET_GUNDORADA_WORKSHOP_E523A_5506 = 5506,
    SOUND_SET_GUNDORADA_WORKSHOP_E524A_5509 = 5509,
    SOUND_SET_GUNDORADA_WORKSHOP_E540A_5511 = 5511,
    SOUND_SET_GUNDORADA_WORKSHOP_E540A_5513 = 5513,
    SOUND_SET_GUNDORADA_WORKSHOP_E540A_5514 = 5514,
    SOUND_SET_GUNDORADA_WORKSHOP_E540A_5516 = 5516,
    SOUND_SET_GUNDORADA_WORKSHOP_E548A_5519 = 5519,
    SOUND_SET_GUNDORADA_WORKSHOP_E552A_5521 = 5521,
    SOUND_SET_GUNDORADA_WORKSHOP_E556A_5524 = 5524,
    SOUND_SET_GUNDORADA_WORKSHOP_E560A_5527 = 5527,
    SOUND_SET_MOUNT_GUNDOR_BS_5600 = 5600,
    SOUND_SET_MOUNT_GUNDOR_SN_5601 = 5601,
    SOUND_SET_MOUNT_GUNDOR_AR_5602 = 5602,
    SOUND_SET_MOUNT_GUNDOR_AR_5603 = 5603,
    SOUND_SET_MOUNT_GUNDOR_AR_5682 = 5682,
    SOUND_SET_MOUNT_GUNDOR_AR_5683 = 5683,
    SOUND_SET_MOUNT_GUNDOR_AR_5684 = 5684,
    SOUND_SET_MOUNT_GUNDOR_BT_5690 = 5690,
    SOUND_SET_MOUNT_GUNDOR_BT_5691 = 5691,
    SOUND_SET_MOUNT_GUNDOR_BT_5693 = 5693,
    SOUND_SET_MOUNT_GUNDOR_AR_5694 = 5694,
    SOUND_SET_MOUNT_GUNDOR_E512A_5800 = 5800,
    SOUND_SET_MOUNT_GUNDOR_E516A_5803 = 5803,
    SOUND_SET_MOUNT_GUNDOR_E520A_5806 = 5806,
    SOUND_SET_MOUNT_GUNDOR_E526A_5814 = 5814,
    SOUND_SET_MOUNT_GUNDOR_E528A_5817 = 5817,
    SOUND_SET_MOUNT_GUNDOR_E532A_5820 = 5820,
    SOUND_SET_MOUNT_GUNDOR_E532A_5821 = 5821,
    SOUND_SET_MOUNT_GUNDOR_E532A_5822 = 5822,
    SOUND_SET_MOUNT_GUNDOR_E536A_5823 = 5823,
    SOUND_SET_MOUNT_GUNDOR_E536A_5824 = 5824,
    SOUND_SET_MOUNT_GUNDOR_E536A_5825 = 5825,
    SOUND_SET_MOUNT_GUNDOR_E636A_5826 = 5826,
    SOUND_SET_KAZAROV_STONEHENGE_BS_6000 = 6000,
    SOUND_SET_KAZAROV_STONEHENGE_E616A_6201 = 6201,
    SOUND_SET_KAZAROV_STONEHENGE_E616A_6203 = 6203,
    SOUND_SET_KAZAROV_STONEHENGE_E640A_6206 = 6206,
    SOUND_SET_KAZAROV_STONEHENGE_E640A_6207 = 6207,
    SOUND_SET_KAZAROV_STONEHENGE_E640A_6208 = 6208,
    SOUND_SET_KAZAROV_STONEHENGE_E640A_6209 = 6209,
    SOUND_SET_KAZAROV_STONEHENGE_E640A_6210 = 6210,
    SOUND_SET_KAZAROV_STONEHENGE_E640A_6211 = 6211,
    SOUND_SET_KAZAROV_STONEHENGE_E640A_6212 = 6212,
    SOUND_SET_KAZAROV_STONEHENGE_E640A_6213 = 6213,
    SOUND_SET_KAZAROV_STONEHENGE_E640A_6214 = 6214,
    SOUND_SET_KAZAROV_STONEHENGE_E640A_6215 = 6215,
    SOUND_SET_KAZAROV_STONEHENGE_E640A_6216 = 6216,
    SOUND_SET_KAZAROV_STONEHENGE_E640A_6217 = 6217,
    SOUND_SET_KAZAROV_STONEHENGE_E640A_6218 = 6218,
    SOUND_SET_KAZAROV_STONEHENGE_E640A_6219 = 6219,
    SOUND_SET_KAZAROV_STONEHENGE_E640A_6220 = 6220,
    SOUND_SET_KAZAROV_STONEHENGE_E640A_6221 = 6221,
    SOUND_SET_KAZAROV_STONEHENGE_E640A_6222 = 6222,
    SOUND_SET_KAZAROV_STONEHENGE_E640A_6223 = 6223,
    SOUND_SET_KAZAROV_STONEHENGE_E648A_6234 = 6234,
    SOUND_SET_KAZAROV_STONEHENGE_E648A_6237 = 6237,
    SOUND_SET_KAZAROV_STONEHENGE_E736A_6243 = 6243,
    SOUND_SET_KAZAROV_STONEHENGE_E736B_6244 = 6244,
    SOUND_SET_MOON_FLOWER_PALACE_GEORAMA_BS_7000 = 7000,
    SOUND_SET_MOON_FLOWER_PALACE_GEORAMA_AR_7080 = 7080,
    SOUND_SET_MOON_FLOWER_PALACE_GEORAMA_AR_7081 = 7081,
    SOUND_SET_MOON_FLOWER_PALACE_GEORAMA_E616A_7200 = 7200,
    SOUND_SET_MOON_FLOWER_PALACE_GEORAMA_E616A_7201 = 7201,
    SOUND_SET_MOON_FLOWER_PALACE_GEORAMA_E616A_7202 = 7202,
    SOUND_SET_MOON_FLOWER_PALACE_GEORAMA_E700A_7203 = 7203,
    SOUND_SET_MOON_FLOWER_PALACE_GEORAMA_E700A_7204 = 7204,
    SOUND_SET_MOON_FLOWER_PALACE_GEORAMA_E704A_7206 = 7206,
    SOUND_SET_MOON_FLOWER_PALACE_GEORAMA_E704A_7207 = 7207,
    SOUND_SET_MOON_FLOWER_PALACE_GEORAMA_E704A_7208 = 7208,
    SOUND_SET_MOON_FLOWER_PALACE_GEORAMA_E744A_7209 = 7209,
    SOUND_SET_MOON_FLOWER_PALACE_GEORAMA_E748A_7211 = 7211,
    SOUND_SET_MOON_FLOWER_PALACE_BS_7600 = 7600,
    SOUND_SET_MOON_FLOWER_PALACE_SN_7601 = 7601,
    SOUND_SET_MOON_FLOWER_PALACE_AR_7602 = 7602,
    SOUND_SET_MOON_FLOWER_PALACE_AR_7603 = 7603,
    SOUND_SET_MOON_FLOWER_PALACE_AR_7680 = 7680,
    SOUND_SET_MOON_FLOWER_PALACE_AR_7681 = 7681,
    SOUND_SET_MOON_FLOWER_PALACE_AR_7682 = 7682,
    SOUND_SET_MOON_FLOWER_PALACE_AR_7683 = 7683,
    SOUND_SET_MOON_FLOWER_PALACE_AR_7684 = 7684,
    SOUND_SET_MOON_FLOWER_PALACE_AR_7685 = 7685,
    SOUND_SET_MOON_FLOWER_PALACE_AR_7686 = 7686,
    SOUND_SET_MOON_FLOWER_PALACE_AR_7687 = 7687,
    SOUND_SET_MOON_FLOWER_PALACE_AR_7688 = 7688,
    SOUND_SET_MOON_FLOWER_PALACE_AR_7689 = 7689,
    SOUND_SET_MOON_FLOWER_PALACE_BT_7690 = 7690,
    SOUND_SET_MOON_FLOWER_PALACE_BT_7691 = 7691,
    SOUND_SET_MOON_FLOWER_PALACE_BT_7692 = 7692,
    SOUND_SET_MOON_FLOWER_PALACE_BT_7693 = 7693,
    SOUND_SET_MOON_FLOWER_PALACE_BT_7694 = 7694,
    SOUND_SET_MOON_FLOWER_PALACE_BT_7695 = 7695,
    SOUND_SET_MOON_FLOWER_PALACE_BT_7696 = 7696,
    SOUND_SET_MOON_FLOWER_PALACE_BT_7697 = 7697,
    SOUND_SET_MOON_FLOWER_PALACE_E600D_7800 = 7800,
    SOUND_SET_MOON_FLOWER_PALACE_E604A_7803 = 7803,
    SOUND_SET_MOON_FLOWER_PALACE_E608A_7806 = 7806,
    SOUND_SET_MOON_FLOWER_PALACE_E610A_7809 = 7809,
    SOUND_SET_MOON_FLOWER_PALACE_E612A_7810 = 7810,
    SOUND_SET_MOON_FLOWER_PALACE_E612A_7811 = 7811,
    SOUND_SET_MOON_FLOWER_PALACE_E612A_7812 = 7812,
    SOUND_SET_MOON_FLOWER_PALACE_E612A_7813 = 7813,
    SOUND_SET_MOON_FLOWER_PALACE_E612A_7814 = 7814,
    SOUND_SET_MOON_FLOWER_PALACE_E648A_7815 = 7815,
    SOUND_SET_MOON_FLOWER_PALACE_E702A_7816 = 7816,
    SOUND_SET_MOON_FLOWER_PALACE_E708A_7817 = 7817,
    SOUND_SET_MOON_FLOWER_PALACE_E708A_7818 = 7818,
    SOUND_SET_MOON_FLOWER_PALACE_E708A_7819 = 7819,
    SOUND_SET_MOON_FLOWER_PALACE_E712A_7820 = 7820,
    SOUND_SET_MOON_FLOWER_PALACE_E716A_7823 = 7823,
    SOUND_SET_MOON_FLOWER_PALACE_E720A_7826 = 7826,
    SOUND_SET_MOON_FLOWER_PALACE_E720A_7827 = 7827,
    SOUND_SET_MOON_FLOWER_PALACE_E724A_7829 = 7829,
    SOUND_SET_MOON_FLOWER_PALACE_E724A_7830 = 7830,
    SOUND_SET_MOON_FLOWER_PALACE_E728A_7831 = 7831,
    SOUND_SET_MOON_FLOWER_PALACE_E728A_7832 = 7832,
    SOUND_SET_MOON_FLOWER_PALACE_E728A_7833 = 7833,
    SOUND_SET_MOON_FLOWER_PALACE_E728A_7835 = 7835,
    SOUND_SET_MOON_FLOWER_PALACE_E728A_7836 = 7836,
    SOUND_SET_MOON_FLOWER_PALACE_E732A_7839 = 7839,
    SOUND_SET_MOON_FLOWER_PALACE_E732A_7840 = 7840,
    SOUND_SET_MOON_FLOWER_PALACE_E732A_7841 = 7841,
    SOUND_SET_ZELMITE_MINE_BS_8600 = 8600,
    SOUND_SET_ZELMITE_MINE_SN_8601 = 8601,
    SOUND_SET_ZELMITE_MINE_AR_8602 = 8602,
    SOUND_SET_ZELMITE_MINE_AR_8603 = 8603,
    SOUND_SET_ZELMITE_MINE_AR_8683 = 8683,
    SOUND_SET_ZELMITE_MINE_AR_8684 = 8684,
    SOUND_SET_ZELMITE_MINE_BT_8690 = 8690,
    SOUND_SET_ZELMITE_MINE_BT_8693 = 8693,
    SOUND_SET_ZELMITE_MINE_AR_8694 = 8694,
    SOUND_SET_ZELMITE_MINE_E804A_8800 = 8800,
    SOUND_SET_ZELMITE_MINE_E808A_8803 = 8803,
    SOUND_SET_ZELMITE_MINE_E812A_8806 = 8806,
    SOUND_SET_ZELMITE_MINE_E816A_8809 = 8809,
    SOUND_SET_ZELMITE_MINE_E820A_8811 = 8811,
};

// System sounds: entries of the system bank snd2/SY_000 (named in its cfg), played with
// play_system_se(n) or with play_se on the id get_system_sound_id returns.
enum system_se_pal {
    SE_CURSOR_MOVE = 0,
    SE_MENU_CONFIRM = 1,
    SE_MENU_WINDOW_SWITCH = 2,
    SE_ITEM_GRAB = 3,
    SE_ITEM_RELEASE = 4,
    SE_ITEM_CANCEL = 5,
    SE_FISH_INTO_AQUARIUM = 6,
    SE_ITEM_DISCARD = 7,
    SE_ITEM_EQUIP = 8,
    SE_ITEM_REPAIR = 9,
    SE_ITEM_HEAL = 10,
    SE_INVENTION_TAKE_PHOTO = 11,
    SE_INVENTION_PHOTO_TO_BUBBLE = 12,
    SE_INVENTION_PHOTO_DELETE = 13,
    SE_INVENTION_PHOTO_HIT = 14,
    SE_CHARA_MENU_RIDE = 15,
    SE_CHARA_MENU_MONICA_TRANSFORM = 16,
    SE_CHARA_MENU_ADD_CHARA = 17,
    SE_TREASURE_GET = 18,
    SE_SUBMENU_OPEN = 19,
    SE_GEORAMA_PLACE = 20,
    SE_GEORAMA_ATTACH = 21,
    SE_GEORAMA_PAINT = 22,
    SE_GEORAMA_REMOVE = 23,
    SE_TRAP_TRIGGERED = 24,
    SE_DIALOGUE_ADVANCE = 25,
    SE_LOCK_ON = 26,
    SE_LOCK_OFF = 27,
    SE_CANNOT_EQUIP = 28,
    SE_ITEM_COUNTER_TICK = 29,
    SE_LEVEL_UP = 30,
    SE_SAVE_COMPLETE = 31,
    SE_INVENTION_TO_IDEA_MEMO = 32,
    SE_INVENTION_IDEA_REGISTERED = 33,
    SE_GEORAMA_DIG_RIVER = 34,
};

// Base sounds: entries of a map's base bank (get_sound_id(SE_BANK_BASE...)); every map's base
// bank has the layout of snd2/BS_999, the full base set. Named from its cfg.
enum base_se_pal {
    SE_BASE_STEP_GRASS_L = 2,
    SE_BASE_STEP_GRASS_R = 3,
    SE_BASE_STEP_STONE_L = 4,
    SE_BASE_STEP_STONE_R = 5,
    SE_BASE_STEP_THICK_WOOD_L = 6,
    SE_BASE_STEP_THICK_WOOD_R = 7,
    SE_BASE_STEP_CARPET_L = 8,
    SE_BASE_STEP_CARPET_R = 9,
    SE_BASE_STEP_ICE_L = 10,
    SE_BASE_STEP_ICE_R = 11,
    SE_BASE_STEP_IRON_L = 12,
    SE_BASE_STEP_IRON_R = 13,
    SE_BASE_STEP_PACKED_EARTH_L = 14,
    SE_BASE_STEP_PACKED_EARTH_R = 15,
    SE_BASE_STEP_MUD_L = 16,
    SE_BASE_STEP_MUD_R = 17,
    SE_BASE_STEP_WOOD_LADDER_L = 18,
    SE_BASE_STEP_WOOD_LADDER_R = 19,
    SE_BASE_STEP_THIN_WOOD_L = 20,
    SE_BASE_STEP_THIN_WOOD_R = 21,
    SE_BASE_STEP_HALF_WATER_L = 22,
    SE_BASE_STEP_HALF_WATER_R = 23,
    SE_BASE_STEP_IRON_LADDER_L = 24,
    SE_BASE_STEP_IRON_LADDER_R = 25,
    SE_BASE_STEP_STRAW_L = 26,
    SE_BASE_STEP_STRAW_R = 27,
    SE_BASE_STEP_SAND_L = 28,
    SE_BASE_STEP_SAND_R = 29,
    SE_BASE_STEP_IRON_MESH_L = 30,
    SE_BASE_STEP_IRON_MESH_R = 31,
    SE_BASE_STEP_EARTH_L = 32,
    SE_BASE_STEP_EARTH_R = 33,
    SE_BASE_STEP_FLOWERS_L = 34,
    SE_BASE_STEP_FLOWERS_R = 35,
    SE_BASE_STEP_SHALLOW_WATER_L = 36,
    SE_BASE_STEP_SHALLOW_WATER_R = 37,
    SE_BASE_STEP_ROPE_L = 38,
    SE_BASE_STEP_ROPE_R = 39,
    SE_BASE_STEP_ROOF_TILE_L = 40,
    SE_BASE_STEP_ROOF_TILE_R = 41,
    SE_BASE_STEP_CARPET_ON_STONE_L = 42,  // carpet that sounds like stone underneath
    SE_BASE_STEP_CARPET_ON_STONE_R = 43,
    SE_BASE_STEP_RAIN_JINGLE_L = 44,
    SE_BASE_STEP_RAIN_JINGLE_R = 45,
    SE_BASE_STEP_GLASS_L = 46,
    SE_BASE_STEP_GLASS_R = 47,
    SE_BASE_STEP_DRY_GRASS_L = 48,
    SE_BASE_STEP_DRY_GRASS_R = 49,
    SE_BASE_STEP_TENT_L = 52,
    SE_BASE_STEP_TENT_R = 53,
    SE_BASE_STEP_BROKEN_GLASS_L = 54,
    SE_BASE_STEP_BROKEN_GLASS_R = 55,
    SE_BASE_SPLASH_INTO_WATER = 58,
    SE_BASE_HINGED_DOOR_WOOD_LIGHT_OPEN = 60,
    SE_BASE_HINGED_DOOR_WOOD_LIGHT_CLOSE = 61,
    SE_BASE_HINGED_DOOR_WOOD_HEAVY_OPEN = 62,
    SE_BASE_HINGED_DOOR_WOOD_HEAVY_CLOSE = 63,
    SE_BASE_HINGED_DOOR_IRON_LIGHT_OPEN = 64,
    SE_BASE_HINGED_DOOR_IRON_LIGHT_CLOSE = 65,
    SE_BASE_HINGED_DOOR_IRON_HEAVY_OPEN = 66,
    SE_BASE_HINGED_DOOR_IRON_HEAVY_CLOSE = 67,
    SE_BASE_HINGED_DOOR_BAMBOO_MEDIUM_OPEN = 68,
    SE_BASE_HINGED_DOOR_BAMBOO_MEDIUM_CLOSE = 69,
    SE_BASE_ELEVATOR_START = 70,
    SE_BASE_ELEVATOR_ARRIVE = 71,
    SE_BASE_VERTICAL_SLIDE_DOOR_WOOD_SMALL_OPEN = 80,
    SE_BASE_VERTICAL_SLIDE_DOOR_WOOD_SMALL_CLOSE = 81,
    SE_BASE_VERTICAL_SLIDE_DOOR_WOOD_LARGE_OPEN = 82,
    SE_BASE_VERTICAL_SLIDE_DOOR_WOOD_LARGE_CLOSE = 83,
    SE_BASE_VERTICAL_SLIDE_DOOR_IRON_SMALL_OPEN = 84,
    SE_BASE_VERTICAL_SLIDE_DOOR_IRON_SMALL_CLOSE = 85,
    SE_BASE_VERTICAL_SLIDE_DOOR_IRON_LARGE_OPEN = 86,
    SE_BASE_VERTICAL_SLIDE_DOOR_IRON_LARGE_CLOSE = 87,
    SE_BASE_VERTICAL_SLIDE_DOOR_STONE_SMALL_OPEN = 88,
    SE_BASE_VERTICAL_SLIDE_DOOR_STONE_SMALL_CLOSE = 89,
    SE_BASE_VERTICAL_SLIDE_DOOR_STONE_LARGE_OPEN = 90,
    SE_BASE_VERTICAL_SLIDE_DOOR_STONE_LARGE_CLOSE = 91,
    SE_BASE_IXION_OUTER_DOOR_OPEN = 92,
    SE_BASE_IXION_OUTER_DOOR_CLOSE = 93,
    SE_BASE_IXION_INNER_DOOR_OPEN = 94,
    SE_BASE_IXION_INNER_DOOR_CLOSE = 95,
    SE_BASE_SLIDING_DOOR_STONE_MEDIUM_OPEN = 100,
    SE_BASE_SLIDING_DOOR_STONE_MEDIUM_CLOSE = 101,
    SE_BASE_FLAP_DOOR_CLOTH_MEDIUM_OPEN = 102,
    SE_BASE_FLAP_DOOR_CLOTH_MEDIUM_CLOSE = 103,
    SE_BASE_TRAIN_DOOR_OPEN = 106,
    SE_BASE_TRAIN_DOOR_CLOSE = 107,
    SE_BASE_LOCKED_DOOR = 108,
    SE_BASE_WOOD_CHEST = 110,
    SE_BASE_IRON_CHEST = 111,
    SE_BASE_STONE_CHEST = 112,
    SE_BASE_GUNDORADA_TELEPORTER = 120,
};

// Battle sounds: entries of the battle bank snd2/FG_000 (get_sound_id(SE_BANK_BATTLE)), named
// from its cfg.
enum battle_se_pal {
    SE_BATTLE_MONEY_SCATTER = 1,
    SE_BATTLE_ABS_SCATTER = 2,
    SE_BATTLE_MONEY_PICKUP = 3,
    SE_BATTLE_ABS_PICKUP = 4,
    SE_BATTLE_CHARGE_ATTACK = 6,
    SE_BATTLE_ITEM_THROW = 8,
    SE_BATTLE_WEAPON_BREAK = 9,
    SE_BATTLE_WEAPON_BREAK_WARNING = 10,
    SE_BATTLE_EXPLOSION = 11,
    SE_BATTLE_ITEM_BOUNCE_WATER = 12,
    SE_BATTLE_ITEM_BOUNCE_LAVA = 13,
    SE_BATTLE_KNOCKED_FLYING = 14,
    SE_BATTLE_COLLAPSE = 15,
    SE_BATTLE_CROUCH = 17,
    SE_BATTLE_ENEMY_LIFT = 18,
    SE_BATTLE_ENEMY_THROW = 19,
    SE_BATTLE_ENEMY_DEATH_EFFECT = 20,
    SE_BATTLE_PLAYER_DAMAGE = 22,
    SE_BATTLE_MONEY_STOLEN = 23,
    SE_BATTLE_POISON_HIT = 24,
    SE_BATTLE_ENEMY_DAMAGE_STEVE = 25,
    SE_BATTLE_ENEMY_DAMAGE_GUN = 26,
    SE_BATTLE_ENEMY_DAMAGE_MONICA = 27,
    SE_BATTLE_ENEMY_DAMAGE_MAX = 28,
    SE_BATTLE_ENEMY_DAMAGE_WHP = 29,
    SE_BATTLE_THROWN_ENEMY_LANDS = 30,
    SE_BATTLE_GRENADE_DAMAGE = 31,
    SE_BATTLE_LASER_DAMAGE = 32,
    SE_BATTLE_PLAYER_GUARD = 33,
    SE_BATTLE_HAMMER_THUD = 34,
    SE_BATTLE_STUNNED = 36,
    SE_BATTLE_ENEMY_NO_DAMAGE = 37,
    SE_BATTLE_ENEMY_GUARD = 38,
    SE_BATTLE_ENRAGE_ON = 39,
    SE_BATTLE_MAX_ATTACK = 40,
    SE_BATTLE_MAX_JUMP_ATTACK = 41,
    SE_BATTLE_MAX_CHARGE_ATTACK = 42,
    SE_BATTLE_MAX_COMBO_1 = 43,
    SE_BATTLE_MAX_COMBO_2 = 44,
    SE_BATTLE_MAX_COMBO_3 = 45,
    SE_BATTLE_MAX_COMBO_4 = 46,
    SE_BATTLE_MAX_COMBO_5 = 47,
    SE_BATTLE_MAX_BACKFLIP = 48,
    SE_BATTLE_KICK_ROCK = 54,
    SE_BATTLE_LIGHTNING_DASH = 55,
    SE_BATTLE_FINAL_BOMB = 56,
    SE_BATTLE_CRATE_BREAK = 57,
    SE_BATTLE_ROCK_BREAK = 58,
    SE_BATTLE_POT_BREAK = 59,
    SE_BATTLE_MONICA_ATTACK = 60,
    SE_BATTLE_MONICA_JUMP_ATTACK = 61,
    SE_BATTLE_MONICA_CHARGE_ATTACK = 62,
    SE_BATTLE_MONICA_COMBO_1 = 63,
    SE_BATTLE_MONICA_COMBO_2 = 64,
    SE_BATTLE_MONICA_COMBO_3 = 65,
    SE_BATTLE_MONICA_COMBO_4 = 66,
    SE_BATTLE_MONICA_COMBO_5 = 67,
    SE_BATTLE_MONICA_BACKFLIP = 68,
    SE_BATTLE_FIRE = 70,
    SE_BATTLE_ICE = 71,
    SE_BATTLE_THUNDER = 72,
    SE_BATTLE_WIND = 73,
    SE_BATTLE_HOLY = 74,
    SE_BATTLE_WATER = 75,
    SE_BATTLE_FIRE_PILLAR = 76,
    SE_BATTLE_ICE_PILLAR = 77,
    SE_BATTLE_THUNDER_PILLAR = 78,
    SE_BATTLE_WIND_PILLAR = 79,
    SE_BATTLE_HOLY_PILLAR = 80,
    SE_BATTLE_WATER_PILLAR = 81,
    SE_BATTLE_STICKY = 82,
    SE_BATTLE_STONE = 83,
    SE_BATTLE_TIME_STOP = 84,
    SE_BATTLE_CURSE = 85,
    SE_BATTLE_STATUS_CURE = 86,
};

// Sound banks scripts load (load_sound by path, or a sound set's event bank): each enum names
// the entries of one bank, from its cfg, for play_se(id, n) after the load (see notes/sounds.md).
enum se_en_287_pal {
    SE_EN_287_PRESS_ATTACK = 4,
    SE_EN_287_SPIRAL_FALL = 21,
    SE_EN_287_IDLE = 22,
};
enum se_en_288_pal {
    SE_EN_288_ARM_SWING_ATTACK = 4,
    SE_EN_288_SPIRAL_FALL = 21,
    SE_EN_288_IDLE = 22,
};
enum se_en_289_pal {
    SE_EN_289_STAB_ATTACK = 4,
    SE_EN_289_PUNCH_ATTACK = 5,
    SE_EN_289_SPIRAL_LANDING = 21,
    SE_EN_289_IDLE = 22,
};
enum se_en_290_pal {
    SE_EN_290_SWORD_COMBO_ATTACK = 5,
    SE_EN_290_SPIRAL_LANDING = 21,
    SE_EN_290_IDLE = 22,
};
enum se_en_291_pal {
    SE_EN_291_SPIRAL_LANDING = 21,
};
enum se_en_360_pal {
    SE_EN_360_DEATH_COLLAPSE = 24,
};
enum se_en_900_pal {
    SE_EN_900_GAUGE_CONFIRM = 0,
    SE_EN_900_SHOT_STRONG = 1,
    SE_EN_900_SHOT_MEDIUM = 2,
    SE_EN_900_SHOT_WEAK = 3,
    SE_EN_900_BALL_LAND_GRASS_STRONG = 5,
    SE_EN_900_BALL_LAND_GRASS_WEAK = 6,
    SE_EN_900_BALL_LAND_WATER = 11,
    SE_EN_900_SHOT_STRONGEST = 12,
    SE_EN_900_HOLE_IN = 17,
    SE_EN_900_OUT_OF_BOUNDS = 18,
    SE_EN_900_PAR_OVER_BALL_VANISH = 19,
};
enum se_en_903_pal {
    SE_EN_903_DRUM_ROLL = 0,
    SE_EN_903_WEIGH_DING = 1,
    SE_EN_903_WEIGH_TA_DA = 2,
    SE_EN_903_RESULTS_BOOM = 3,
    SE_EN_903_CYMBAL = 4,
};
enum se_ev_000_100_pal {
    SE_EV_000_100_MAX_SHIVER = 10,
    SE_EV_000_100_MAX_BRUSH_OFF = 11,
    SE_EV_000_100_TICKET_TAKE_OUT = 12,
    SE_EV_000_100_TA_DA = 13,
    SE_EV_000_100_HAPPY_BOUNCE = 14,
    SE_EV_000_100_TICKET_PUT_AWAY = 15,
    SE_EV_000_100_PENDANT = 16,
    SE_EV_000_100_NOD = 17,
    SE_EV_000_100_SIT_IN_CHAIR = 18,
    SE_EV_000_100_TOOL_SPIN = 19,
    SE_EV_000_100_TOOL_TOSS = 20,
    SE_EV_000_100_TOOL_CATCH = 21,
    SE_EV_000_100_TINKERING_1 = 22,
    SE_EV_000_100_TINKERING_2 = 23,
    SE_EV_000_100_TINKERING_3 = 24,
    SE_EV_000_100_PEER_INTO_MACHINE = 25,
    SE_EV_000_100_POLKAS_APPEAR = 26,
    SE_EV_000_100_CANE_TWIRL = 27,
    SE_EV_000_100_POSE = 28,
    SE_EV_000_100_CLOWN_JUMP = 29,
    SE_EV_000_100_CLOWN_LAND = 30,
    SE_EV_000_100_EVERYONE_JUMP = 31,
    SE_EV_000_100_MAX_SNORE = 33,
    SE_EV_000_100_MAX_TWITCH = 34,
    SE_EV_000_100_MAX_JUMP_AWAKE = 35,
    SE_EV_000_100_CHECK_CLOCK = 36,
    SE_EV_000_100_LEAVE_CHAIR = 37,
};
enum se_ev_000_105_pal {
    SE_EV_000_105_TICKET_HAND_OVER = 4,
    SE_EV_000_105_TICKET_SNATCH = 5,
    SE_EV_000_105_RUSTLE_NOISE = 6,
    SE_EV_000_105_BOY_PEEK = 7,
    SE_EV_000_105_BOY_DUCK_BACK = 8,
    SE_EV_000_105_BOY_STARTLED = 9,
    SE_EV_000_105_BOY_CAUGHT = 10,
    SE_EV_000_105_TICKET_TAKE_OUT = 11,
    SE_EV_000_105_TICKET_OFFER = 12,
    SE_EV_000_105_TICKET_RECEIVE = 13,
    SE_EV_000_105_BOY_VANISH = 14,
};
enum se_ev_000_106_pal {
    SE_EV_000_106_CROWD_SHUFFLE_1 = 4,
    SE_EV_000_106_CROWD_SHUFFLE_2 = 5,
    SE_EV_000_106_HOPELESS_SIGH = 6,
    SE_EV_000_106_TURN_AROUND = 9,
};
enum se_ev_000_110_pal {
    SE_EV_000_110_MUSCLE_POSE = 5,
    SE_EV_000_110_ELEPHANT_ENTER = 6,
    SE_EV_000_110_ELEPHANT_TRUMPET = 7,
    SE_EV_000_110_CROWD_MURMUR = 8,
    SE_EV_000_110_ELEPHANT_WHISPER = 9,
    SE_EV_000_110_ELEPHANT_EXCITED = 10,
    SE_EV_000_110_ELEPHANT_GET_UP = 11,
    SE_EV_000_110_ELEPHANT_RAISE_BALL = 12,
    SE_EV_000_110_SPARKLE = 13,
    SE_EV_000_110_ELEPHANT_JUMP = 14,
    SE_EV_000_110_ELEPHANT_KICK = 15,
    SE_EV_000_110_BALL_HIT_LIGHT = 16,
    SE_EV_000_110_BALL_CATCH = 17,
    SE_EV_000_110_CROWD_MURMUR_2 = 18,
    SE_EV_000_110_DRUM_ROLL = 19,
    SE_EV_000_110_TA_DA = 20,
    SE_EV_000_110_WHOOSH = 21,
    SE_EV_000_110_WHOOSH_2 = 25,
    SE_EV_000_110_MIRENE_JUMP = 26,
    SE_EV_000_110_MIRENE_LAND = 27,
    SE_EV_000_110_TA_DA_2 = 28,
    SE_EV_000_110_APPLAUSE = 29,
};
enum se_ev_000_115_pal {
    SE_EV_000_115_MAX_TURN_AROUND = 2,
    SE_EV_000_115_MAX_PEER = 3,
    SE_EV_000_115_CANE_THRUST = 4,
    SE_EV_000_115_NOD = 7,
    SE_EV_000_115_PAINT_CAN_1 = 17,
    SE_EV_000_115_PAINT_CAN_2 = 18,
    SE_EV_000_115_PAINT_CAN_3 = 19,
    SE_EV_000_115_PAINT_CAN_4 = 20,
    SE_EV_000_115_MAYOR_LOOK_UP = 21,
    SE_EV_000_115_MAX_TURN_AROUND_2 = 22,
    SE_EV_000_115_POLKAS_GLARE = 23,
    SE_EV_000_115_BOX_OPEN = 24,
    SE_EV_000_115_CLOWN_JUMP = 25,
    SE_EV_000_115_CLOWN_LAND = 26,
    SE_EV_000_115_POLKAS_COMMAND = 27,
    SE_EV_000_115_CLOWN_JUMP_2 = 28,
    SE_EV_000_115_CLOWN_JUMP_3 = 29,
    SE_EV_000_115_CLOWN_LAND_1 = 30,
    SE_EV_000_115_CLOWN_LAND_2 = 31,
    SE_EV_000_115_CLOWN_JUMP_4 = 32,
    SE_EV_000_115_CLOWN_JUMP_5 = 33,
    SE_EV_000_115_CLOWN_JUMP_6 = 34,
    SE_EV_000_115_CLOWN_LAND_3 = 35,
    SE_EV_000_115_CLOWN_LAND_4 = 36,
};
enum se_ev_000_120_pal {
    SE_EV_000_120_MAX_JUMP = 8,
    SE_EV_000_120_GRAB_FLOOR_EDGE = 9,
    SE_EV_000_120_CLOWN_FALL = 10,
    SE_EV_000_120_CLIMB_ONTO_FLOOR = 11,
    SE_EV_000_120_JUMP = 12,
    SE_EV_000_120_CLOWN_FROM_FRONT = 13,
    SE_EV_000_120_CLOWN_LAND = 14,
    SE_EV_000_120_ATTACK_DODGE = 15,
    SE_EV_000_120_GRAB_TRAPEZE = 16,
    SE_EV_000_120_TRAPEZE_SWING = 17,
    SE_EV_000_120_KICK_CLOWN = 18,
    SE_EV_000_120_LEAP_ACROSS = 19,
    SE_EV_000_120_CATCH = 20,
    SE_EV_000_120_LEAP_TO_FLOOR = 21,
    SE_EV_000_120_LEAP_ONTO_POLE = 22,
    SE_EV_000_120_CLIMB_POLE = 23,
    SE_EV_000_120_LEAP_ONTO_LIGHT = 24,
    SE_EV_000_120_LIGHT_SWAY = 25,
    SE_EV_000_120_CHAIN_SNAP = 26,
    SE_EV_000_120_LIGHT_FALL = 27,
    SE_EV_000_120_APPLAUSE = 28,
    SE_EV_000_120_CLOWN_LAND_2 = 29,
    SE_EV_000_120_MAX_JUMP_2 = 30,
    SE_EV_000_120_CHAIN_STRAIN = 32,
    SE_EV_000_120_MAX_FALL = 33,
    SE_EV_000_120_CLOWN_LAND_3 = 34,
    SE_EV_000_120_WRENCH_TAKE_OUT = 35,
    SE_EV_000_120_WRENCH_SWING = 36,
    SE_EV_000_120_WRENCH_RAISE = 37,
    SE_EV_000_120_EFFECT_SOUND = 38,
};
enum se_ev_000_125_pal {
    SE_EV_000_125_RIDEPOD_START_UP = 9,
    SE_EV_000_125_RIDEPOD_ARM_SWING = 10,
    SE_EV_000_125_LEAP_ONTO_SIGN = 11,
    SE_EV_000_125_CLIMB_SIGN = 12,
    SE_EV_000_125_SHAKE_OFF_CLOWN = 13,
    SE_EV_000_125_MAX_JUMP = 14,
    SE_EV_000_125_SLIDE_DOWN_ROOF = 15,
    SE_EV_000_125_WOBBLE = 16,
    SE_EV_000_125_WRENCH_READY = 17,
    SE_EV_000_125_DODGE_ATTACK = 18,
    SE_EV_000_125_KICK = 19,
    SE_EV_000_125_CLOWN_FALL = 20,
    SE_EV_000_125_FINISH_POSE = 21,
    SE_EV_000_125_RIDEPOD_JUMP = 23,
    SE_EV_000_125_RIDEPOD_LAND = 24,
    SE_EV_000_125_RIDEPOD_BECKON = 25,
    SE_EV_000_125_RIDEPOD_JUMP_2 = 26,
    SE_EV_000_125_RIDEPOD_LAND_2 = 27,
    SE_EV_000_125_HANG_FROM_FLAG = 28,
    SE_EV_000_125_MAX_LAND = 29,
    SE_EV_000_125_RIDEPOD_LAND_3 = 30,
    SE_EV_000_125_RIDEPOD_ARM_SWING_2 = 31,
    SE_EV_000_125_RIDEPOD_RAISE_ARM = 32,
    SE_EV_000_125_RIDEPOD_GRAB_MISS = 33,
    SE_EV_000_125_RIDEPOD_JUMP_3 = 34,
    SE_EV_000_125_RIDEPOD_LAND_4 = 35,
    SE_EV_000_125_MANHOLE_CLOSE = 36,
    SE_EV_000_125_MANHOLE_OPEN = 37,
    SE_EV_000_125_MANHOLE_CLOSE_2 = 38,
};
enum se_ev_000_130_pal {
    SE_EV_000_130_GRIFFON_BREAK_WINDOW = 4,
    SE_EV_000_130_MONICA_STOP = 5,
    SE_EV_000_130_GRIFFON_ENTER_WINDOW = 6,
    SE_EV_000_130_MONICA_TURN_1 = 7,
    SE_EV_000_130_MONICA_TURN_2 = 8,
    SE_EV_000_130_MONICA_LEAP = 9,
    SE_EV_000_130_GRIFFON_POUNCE = 10,
    SE_EV_000_130_GRIFFON_SOLDIER_LEAP = 11,
    SE_EV_000_130_MONICA_TAKE_SWORD = 12,
    SE_EV_000_130_MONICA_SWING_SWORD = 13,
    SE_EV_000_130_MONICA_REACH_DOOR = 14,
    SE_EV_000_130_MONICA_OPEN_DOOR = 15,
    SE_EV_000_130_MONICA_STARTLED = 16,
    SE_EV_000_130_GASPARD_TURN_AROUND = 17,
    SE_EV_000_130_GASPARD_LOOK_BACK = 18,
    SE_EV_000_130_GASPARD_USE_GATE = 19,
    SE_EV_000_130_MONICA_DROP_SWORD = 20,
};
enum se_ev_000_135_pal {
    SE_EV_000_135_MAX_TOUCH_PENDANT = 2,
    SE_EV_000_135_MAX_FLEX_ARM = 3,
    SE_EV_000_135_MAX_NOD = 4,
    SE_EV_000_135_CAMERA_TAKE_OUT = 5,
    SE_EV_000_135_CAMERA_TAKE_OUT_2 = 6,
    SE_EV_000_135_GET_ITEM_EFFECT = 7,
};
enum se_ev_000_140_pal {
    SE_EV_000_140_CLOTH_RUSTLE_1 = 2,
    SE_EV_000_140_CLOTH_RUSTLE_2 = 3,
    SE_EV_000_140_SCRATCH_HEAD = 4,
};
enum se_ev_000_145_pal {
    SE_EV_000_145_CEDRIC_FLEX_ARM = 2,
    SE_EV_000_145_MONEY_OFFER = 3,
    SE_EV_000_145_MAX_TAKE_MONEY = 4,
    SE_EV_000_145_MAX_SET_BACKPACK = 5,
    SE_EV_000_145_MAX_NOD = 6,
    SE_EV_000_145_MAX_FLEX_ARM = 7,
    SE_EV_000_145_STEVE_COMPLETE = 8,
};
enum se_ev_000_150_pal {
    SE_EV_000_150_BOTH_FLEX_ARMS = 2,
    SE_EV_000_150_STEVE_TURN_AROUND = 4,
    SE_EV_000_150_DOOR_OPEN = 5,
    SE_EV_000_150_STEVE_ENGINE = 6,
    SE_EV_000_150_STEVE_LIGHTS_ON = 8,
};
enum se_ev_000_155_pal {
    SE_EV_000_155_DOOR_KNOCK = 2,
    SE_EV_000_155_DOOR_OPEN = 3,
    SE_EV_000_155_DESK_SLAM = 6,
};
enum se_ev_000_160_pal {
    SE_EV_000_160_SPREAD_ARMS = 2,
    SE_EV_000_160_ANGRY = 3,
    SE_EV_000_160_SPREAD_ARMS_2 = 5,
    SE_EV_000_160_STATION_AMBIENCE = 6,
    SE_EV_000_160_STATION_AMBIENCE_2 = 9,
    SE_EV_000_160_DEPARTURE_BELL = 10,
    SE_EV_000_160_TRAIN_WHISTLE = 11,
    SE_EV_000_160_DOOR_OPEN = 12,
    SE_EV_000_160_TRAIN_START_MOVING = 13,
};
enum se_ev_000_165_pal {
    SE_EV_000_165_DOOR_OPEN = 3,
    SE_EV_000_165_DOOR_CLOSE = 4,
};
enum se_ev_000_170_pal {
    SE_EV_000_170_FIREWORKS = 4,
};
enum se_ev_000_190_pal {
    SE_EV_000_190_MAX_FIST_PUMP = 0,
};
enum se_ev_000_195_pal {
    SE_EV_000_195_MONICA_APPEAR = 0,
    SE_EV_000_195_MONICA_WAVE = 1,
    SE_EV_000_195_MONICA_EXCITED = 2,
};
enum se_ev_010_100_pal {
    SE_EV_010_100_TRAIN_WHISTLE = 0,
    SE_EV_010_100_DOOR_OPEN = 1,
    SE_EV_010_100_BUGGY_DRIVING = 2,
    SE_EV_010_100_BUGGY_DRIFT = 3,
    SE_EV_010_100_GUNSHOT = 4,
    SE_EV_010_100_BUGGY_DRIVING_QUIET = 5,
    SE_EV_010_100_MAX_TURN_AROUND = 6,
    SE_EV_010_100_RUMMAGE = 7,
    SE_EV_010_100_PLACE_BOMB = 8,
    SE_EV_010_100_GUNSHOT_2 = 9,
    SE_EV_010_100_GUNSHOT_3 = 10,
    SE_EV_010_100_GUNSHOT_4 = 11,
    SE_EV_010_100_DRIFT_SOUND = 12,
};
enum se_ev_010_105_pal {
    SE_EV_010_105_TRAIN_COLLAPSING = 0,
    SE_EV_010_105_TRAIN_OVERTURN = 1,
    SE_EV_010_105_EXPLOSION = 5,
};
enum se_ev_010_110_pal {
    SE_EV_010_110_BUGGY_DRIVING = 2,
    SE_EV_010_110_POLKAS_FLAIL = 3,
    SE_EV_010_110_BUGGY_EXPLODE = 4,
    SE_EV_010_110_POLKAS_LAND = 5,
    SE_EV_010_110_POLKAS_GET_UP = 7,
    SE_EV_010_110_POLKAS_GET_UP_2 = 8,
    SE_EV_010_110_POLKAS_UNDRESS_1 = 9,
    SE_EV_010_110_POLKAS_UNDRESS_2 = 10,
    SE_EV_010_110_MAX_RECOIL = 11,
    SE_EV_010_110_BOY_APPEAR = 12,
    SE_EV_010_110_POLKAS_HOLD_BOMB = 13,
    SE_EV_010_110_POLKAS_LIGHT_FUSE = 14,
    SE_EV_010_110_FUSE_BURNING = 15,
    SE_EV_010_110_MONICA_TRANSFORM = 16,
    SE_EV_010_110_MONICA_TRANSFORM_2 = 17,
    SE_EV_010_110_MONICA_TRANSFORM_3 = 18,
    SE_EV_010_110_MONICA_TRANSFORM_4 = 19,
    SE_EV_010_110_MONICA_READY_SWORD = 22,
    SE_EV_010_110_MONICA_JUMP = 23,
    SE_EV_010_110_SWORD_HIT = 25,
    SE_EV_010_110_MONICA_JUMP_2 = 26,
    SE_EV_010_110_MONICA_KICK = 27,
    SE_EV_010_110_MAGIC_CAST = 28,
    SE_EV_010_110_MAGIC_HIT = 29,
    SE_EV_010_110_POLKAS_FALL = 30,
    SE_EV_010_110_POLKAS_ROLL = 31,
    SE_EV_010_110_MONICA_LOWER_ARM = 32,
    SE_EV_010_110_MONICA_TURN_AROUND = 33,
};
enum se_ev_010_115_pal {
    SE_EV_010_115_TRAIN_STOP = 8,
    SE_EV_010_115_LOCOMOTIVE_DOOR_OPEN = 9,
    SE_EV_010_115_JUMP = 10,
    SE_EV_010_115_MONICA_FOLD_ARMS = 11,
    SE_EV_010_115_MONICA_FLEX_ARM = 12,
    SE_EV_010_115_MONICA_HAPPY_1 = 14,
    SE_EV_010_115_MAX_HANDSHAKE = 19,
};
enum se_ev_010_120_pal {
    SE_EV_010_120_ROCOCO_SNORE = 2,
    SE_EV_010_120_ROCOCO_WAKE_UP = 3,
    SE_EV_010_120_ROCOCO_JUMP = 4,
    SE_EV_010_120_ROCOCO_LAND = 5,
    SE_EV_010_120_ROCOCO_STRUGGLE = 8,
    SE_EV_010_120_MONICA_RELEASE_BEARD = 9,
    SE_EV_010_120_ROCOCO_WAKE_UP_2 = 11,
    SE_EV_010_120_ROCOCO_CLOSE_DOOR = 13,
};
enum se_ev_010_125_pal {
    SE_EV_010_125_DWARF_FLAIL = 5,
    SE_EV_010_125_DOOR_OPEN = 6,
    SE_EV_010_125_DOOR_CLOSE = 7,
    SE_EV_010_125_DWARF_HOLD_OUT_HAND = 8,
    SE_EV_010_125_DWARF_ANGRY = 10,
    SE_EV_010_125_DWARF_CALM_DOWN = 11,
    SE_EV_010_125_DWARF_FOLD_ARMS = 12,
    SE_EV_010_125_MONICA_EXASPERATED = 13,
    SE_EV_010_125_MAX_FLINCH = 14,
    SE_EV_010_125_DWARF_TURN_AROUND = 15,
    SE_EV_010_125_DWARF_CAST_SPELL = 16,
    SE_EV_010_125_PLANT_VANISH = 17,
    SE_EV_010_125_DWARF_TURN_AROUND_2 = 18,
    SE_EV_010_125_MONICA_SLUMP = 19,
    SE_EV_010_125_GET_ITEM_EFFECT = 20,
};
enum se_ev_010_135_pal {
    SE_EV_010_135_CONDA_SNORE = 8,
    SE_EV_010_135_DOOR_OPEN = 9,
    SE_EV_010_135_CONDA_LEAVE_CHAIR = 10,
    SE_EV_010_135_CONDA_TURN_AROUND = 11,
    SE_EV_010_135_DWARF_RAISE_ARM = 12,
    SE_EV_010_135_CONDA_LOOK_BACK = 13,
    SE_EV_010_135_DOOR_OPEN_2 = 14,
    SE_EV_010_135_CARPEN_LAUNCH = 15,
    SE_EV_010_135_CARPEN_EMERGE_FOREST = 16,
    SE_EV_010_135_CARPEN_DESCEND = 19,
    SE_EV_010_135_BOARDING_HATCH_LOWER = 20,
    SE_EV_010_135_BOARDING_HATCH_RAISE = 21,
    SE_EV_010_135_TIME_GATE = 22,
    SE_EV_010_135_MONICA_RAISE_ARM = 23,
    SE_EV_010_135_MAX_LOOK_AT_PENDANT = 24,
    SE_EV_010_135_MAX_NOD = 25,
};
enum se_ev_010_140_pal {
    SE_EV_010_140_UTAN_TURN_AROUND = 6,
    SE_EV_010_140_UTAN_GRAB_ROCK = 8,
    SE_EV_010_140_UTAN_LIFT_ROCK = 9,
    SE_EV_010_140_UTAN_HURT_BACK = 10,
    SE_EV_010_140_UTAN_DROP_ROCK = 11,
    SE_EV_010_140_UTAN_SIT_DOWN = 13,
};
enum se_ev_010_141_pal {
    SE_EV_010_141_CARPEN_APPEAR = 8,
    SE_EV_010_141_CONDA_INSERT_STONE = 9,
    SE_EV_010_141_GEOSTONE_ENTER_FURNACE = 10,
};
enum se_ev_010_145_pal {
    SE_EV_010_145_LEAVES_RUSTLE = 2,
    SE_EV_010_145_JURAK_BLOW = 3,
    SE_EV_010_145_MAX_MONICA_DISAPPOINTED = 5,
};
enum se_ev_010_150_pal {
    SE_EV_010_150_JURAK_SNORT = 4,
    SE_EV_010_150_SEED_FALLS = 5,
    SE_EV_010_150_GET_SEED = 6,
};
enum se_ev_010_160_pal {
    SE_EV_010_160_MONICA_GRAB_BADGE = 0,
};
enum se_ev_010_165_pal {
    SE_EV_010_165_BRANCHES_SHAKE = 8,
    SE_EV_010_165_MONICA_SLUMP = 9,
    SE_EV_010_165_MAX_LOOK_UP = 10,
    SE_EV_010_165_MAX_FIST_PUMP = 11,
    SE_EV_010_165_MONICA_TURN_AROUND = 12,
    SE_EV_010_165_MAX_FIST_PUMP_2 = 13,
    SE_EV_010_165_MONICA_NOD = 14,
    SE_EV_010_165_MAX_NOD = 15,
    SE_EV_010_165_CONDA_OPEN_LETTER = 16,
    SE_EV_010_165_CONDA_CLOSE_LETTER = 17,
};
enum se_ev_010_170_pal {
    SE_EV_010_170_STARBLE_WAVE_ARM = 2,
    SE_EV_010_170_TRAIN_DEPART = 3,
};
enum se_ev_010_180_pal {
    SE_EV_010_180_BRANCHES_SHAKE = 0,
    SE_EV_010_180_JURAK_SPIT_OUT = 1,
    SE_EV_010_180_SPARKLE = 2,
    SE_EV_010_180_MAX_TAKE_GUN = 3,
    SE_EV_010_180_GET_EFFECT = 4,
};
enum se_ev_015_100_pal {
    SE_EV_015_100_TRAIN_ARRIVE = 8,
    SE_EV_015_100_TRAIN_DOOR_OPEN = 9,
};
enum se_ev_015_105_pal {
    SE_EV_015_105_MONICA_MOVE_ARM = 0,
};
enum se_ev_015_110_pal {
    SE_EV_015_110_DOOR_OPEN = 0,
    SE_EV_015_110_DOOR_CLOSE = 1,
    SE_EV_015_110_SHIROZA_SPREAD_WINGS = 2,
    SE_EV_015_110_SHIROZA_TAKE_OFF = 3,
    SE_EV_015_110_SHIROZA_THREAT_FLAP = 4,
    SE_EV_015_110_SHIROZA_LAND_ON_PERCH = 6,
    SE_EV_015_110_MONICA_WAVE_ARM = 7,
    SE_EV_015_110_MAX_MONICA_DISAPPOINTED = 8,
    SE_EV_015_110_MAX_CLAP = 9,
    SE_EV_015_110_SHIROZA_SPREAD_WINGS_2 = 10,
    SE_EV_015_110_SHIROZA_SPREAD_WINGS_3 = 11,
};
enum se_ev_015_115_pal {
    SE_EV_015_115_SHIROZA_SPREAD_WINGS = 7,
    SE_EV_015_115_DUCK_HAND_TO_FOREHEAD = 8,
    SE_EV_015_115_MONICA_CLAP = 9,
    SE_EV_015_115_GONG = 10,
    SE_EV_015_115_MAX_FIST_PUMP = 11,
    SE_EV_015_115_MAX_NOD = 13,
};
enum se_ev_015_120_pal {
    SE_EV_015_120_KITCHEN_NOISE = 5,
    SE_EV_015_120_LAOCHAO_COOK = 6,
    SE_EV_015_120_LAOCHAO_TURN_AROUND = 7,
    SE_EV_015_120_MONICA_WAVE_ARM = 8,
};
enum se_ev_015_125_pal {
    SE_EV_015_125_LAOCHAO_NOD = 4,
    SE_EV_015_125_MAX_FIST_PUMP = 5,
    SE_EV_015_125_LAOCHAO_RAISE_LADLE = 6,
    SE_EV_015_125_LAOCHAO_RAISE_KNIFE = 7,
    SE_EV_015_125_GONG = 8,
    SE_EV_015_125_LAOCHAO_COOK = 9,
    SE_EV_015_125_DISH_READY = 10,
    SE_EV_015_125_GET_EFFECT = 11,
    SE_EV_015_125_MONICA_BOW = 12,
    SE_EV_015_125_MAX_BOW = 13,
};
enum se_ev_015_130_pal {
    SE_EV_015_130_SHIROZA_SPREAD_WINGS = 0,
    SE_EV_015_130_MONICA_WAVE_ARM = 1,
    SE_EV_015_130_MAX_NOD = 2,
};
enum se_ev_015_135_pal {
    SE_EV_015_135_LIN_SIT_UP = 6,
    SE_EV_015_135_LIN_WAKE_UP = 7,
    SE_EV_015_135_LIN_PAT_FUTON = 8,
    SE_EV_015_135_MONICA_WAVE_ARM = 9,
    SE_EV_015_135_LIN_TURN_AROUND = 10,
    SE_EV_015_135_LIN_HOLD_HEAD = 11,
    SE_EV_015_135_SHIROZA_SPREAD_WINGS = 12,
    SE_EV_015_135_MAX_MONICA_NOD = 13,
};
enum se_ev_015_140_pal {
    SE_EV_015_140_TRAIN_DEPART = 0,
};
enum se_ev_015_145_pal {
    SE_EV_015_145_MAX_TURN_AROUND = 0,
    SE_EV_015_145_MAX_POINT = 1,
    SE_EV_015_145_MONICA_NOD = 2,
};
enum se_ev_015_150_pal {
    SE_EV_015_150_MONICA_WAVE_ARM = 0,
    SE_EV_015_150_MAX_WAVE_ARM = 1,
    SE_EV_015_150_MAX_FOLD_ARMS = 2,
};
enum se_ev_015_155_pal {
    SE_EV_015_155_CREST_TALK = 6,
    SE_EV_015_155_CREST_EXCITED = 8,
    SE_EV_015_155_CREST_TALK_2 = 9,
    SE_EV_015_155_CREST_TALK_3 = 10,
    SE_EV_015_155_CREST_EXCITED_2 = 12,
    SE_EV_015_155_EFFECT_1 = 14,
    SE_EV_015_155_EFFECT_2 = 15,
    SE_EV_015_155_CREST_LOWER_HAND = 16,
    SE_EV_015_155_CREST_TURN_AROUND = 17,
    SE_EV_015_155_MAX_NOD = 19,
    SE_EV_015_155_MAX_MONICA_NOD = 21,
};
enum se_ev_015_160_pal {
    SE_EV_015_160_CREST_DRAW_SWORD = 2,
    SE_EV_015_160_GET_EFFECT = 3,
};
enum se_ev_020_100_pal {
    SE_EV_020_100_TRAIN_WHISTLE = 4,
    SE_EV_020_100_TRAIN_ARRIVE = 5,
    SE_EV_020_100_TRAIN_DOOR_OPEN = 6,
    SE_EV_020_100_MAX_MONICA_TURN_AROUND = 7,
    SE_EV_020_100_MAX_NOD = 8,
};
enum se_ev_020_105_pal {
    SE_EV_020_105_MAX_SPLASH_WATER = 0,
    SE_EV_020_105_MAX_LOOK_BACK = 1,
};
enum se_ev_020_110_pal {
    SE_EV_020_110_SHINGALA_HANG_HEAD = 5,
    SE_EV_020_110_PAU_APPLY_MEDICINE = 6,
    SE_EV_020_110_PAU_LOOK_BACK = 7,
    SE_EV_020_110_PAU_THROW_BOTTLE = 8,
    SE_EV_020_110_BOTTLE_ROLL = 9,
    SE_EV_020_110_PAU_RUMMAGE = 10,
    SE_EV_020_110_MONICA_TURN_AROUND = 11,
    SE_EV_020_110_MAX_STARTLED = 12,
    SE_EV_020_110_SHINGALA_RAISE_HEAD = 13,
    SE_EV_020_110_SHINGALA_CRY = 14,
    SE_EV_020_110_SHINGALA_THROW_MAX = 15,
    SE_EV_020_110_MAX_FALL_ON_BACKSIDE = 16,
};
enum se_ev_020_115_pal {
    SE_EV_020_115_OSMOND_SPIN_CHAIR = 2,
    SE_EV_020_115_OSMOND_GET_OFF_CHAIR = 3,
    SE_EV_020_115_OSMOND_PULL_HAND_OUT = 4,
    SE_EV_020_115_CHILLY_SILENCE = 5,
    SE_EV_020_115_OSMOND_GET_OFF_DESK = 6,
    SE_EV_020_115_OSMOND_CLATTER = 7,
    SE_EV_020_115_OSMOND_CLIMB_DESK = 8,
    SE_EV_020_115_OSMOND_RAISE_HAND = 9,
    SE_EV_020_115_GLINT = 10,
    SE_EV_020_115_MAX_FIST_PUMP = 11,
    SE_EV_020_115_MAX_CLAP = 12,
    SE_EV_020_115_OSMOND_GET_OFF_DESK_2 = 13,
    SE_EV_020_115_ITEM_GET = 14,
};
enum se_ev_020_125_pal {
    SE_EV_020_125_CHAP_THREATEN_MONICA = 2,
    SE_EV_020_125_CHAP_LOOK_BACK = 3,
    SE_EV_020_125_CHAP_LOOK_BACK_2 = 4,
    SE_EV_020_125_RESEARCHER_HAND_TRANSLATOR = 5,
    SE_EV_020_125_CHAP_HAND_TRANSLATOR = 6,
    SE_EV_020_125_MAX_TAKE_TRANSLATOR = 7,
    SE_EV_020_125_GET_EFFECT = 8,
};
enum se_ev_020_130_pal {
    SE_EV_020_130_MAX_FIST_PUMP = 3,
    SE_EV_020_130_SCREEN_LOWER = 4,
    SE_EV_020_130_SCREEN_SHOW_IXION = 5,
    SE_EV_020_130_MAX_EXCITED = 6,
    SE_EV_020_130_CHAP_TURN_AROUND = 7,
    SE_EV_020_130_MONICA_TURN_HEAD = 8,
    SE_EV_020_130_MAX_MONICA_SURPRISED = 9,
    SE_EV_020_130_MONICA_TURN_AROUND = 10,
    SE_EV_020_130_MONICA_ANGRY = 11,
    SE_EV_020_130_JAMMING_TURN_AROUND = 12,
    SE_EV_020_130_HOLOGRAM_LOWER = 13,
    SE_EV_020_130_EINA_APPEAR = 14,
    SE_EV_020_130_CHAP_TURN_AROUND_2 = 15,
    SE_EV_020_130_MAX_HOLD_PENDANT = 16,
    SE_EV_020_130_HOLOGRAM_EXPLOSION = 17,
    SE_EV_020_130_CHAP_RAISE_HAND = 18,
    SE_EV_020_130_MONICA_NOD = 19,
    SE_EV_020_130_MAX_NOD = 20,
    SE_EV_020_130_HOLOGRAM_VANISH = 21,
    SE_EV_020_130_CHAP_TURN_AROUND_3 = 22,
    SE_EV_020_130_MAX_FIST_PUMP_2 = 23,
    SE_EV_020_130_MONICA_NOD_2 = 24,
    SE_EV_020_130_MAX_NOD_2 = 25,
};
enum se_ev_020_135_pal {
    SE_EV_020_135_STARBLE_RAISE_ARM = 0,
    SE_EV_020_135_TRAIN_WHISTLE = 1,
    SE_EV_020_135_TRAIN_DEPART = 2,
};
enum se_ev_020_145_pal {
    SE_EV_020_145_MONICA_RAISE_ARM = 0,
    SE_EV_020_145_MONICA_LOWER_ARM = 1,
};
enum se_ev_020_150_pal {
    SE_EV_020_150_MAX_FIST_PUMP = 0,
};
enum se_ev_020_155_pal {
    SE_EV_020_155_MAX_TURN_AROUND = 2,
    SE_EV_020_155_MAX_OSMOND_FIST_PUMP = 3,
};
enum se_ev_020_160_pal {
    SE_EV_020_160_MAX_HOP = 2,
    SE_EV_020_160_OSMOND_POINT = 3,
    SE_EV_020_160_OSMOND_FIST_PUMP = 4,
    SE_EV_020_160_OSMOND_PREP_LAUNCH = 5,
    SE_EV_020_160_MAX_MONICA_SIT = 6,
    SE_EV_020_160_MAX_MONICA_SPIN_CHAIR = 7,
    SE_EV_020_160_OSMOND_READY = 8,
    SE_EV_020_160_MONICA_SPIN_CHAIR = 9,
    SE_EV_020_160_OSMOND_GLARE_MONICA = 10,
    SE_EV_020_160_OSMOND_RAISE_HAND = 12,
    SE_EV_020_160_OSMOND_FLIP_SWITCH = 13,
    SE_EV_020_160_ENGINE_START = 14,
};
enum se_ev_020_161_pal {
    SE_EV_020_161_DOME_OPEN = 0,
    SE_EV_020_161_RAILS_EMERGE_WATER = 1,
    SE_EV_020_161_GIRDER_STOP = 3,
    SE_EV_020_161_HORN_BLAST = 4,
    SE_EV_020_161_IXION_DEPART = 5,
};
enum se_ev_020_162_pal {
    SE_EV_020_162_G_FORCE = 0,
    SE_EV_020_162_SPACE_WARP = 1,
    SE_EV_020_162_IXION_PASS_BY = 2,
};
enum se_ev_020_163_pal {
    SE_EV_020_163_WARP_OUT = 2,
    SE_EV_020_163_IXION_LAND = 3,
    SE_EV_020_163_IXION_DOOR_OPEN = 4,
    SE_EV_020_163_MAX_MONICA_TURN_AROUND = 5,
};
enum se_ev_025_100_pal {
    SE_EV_025_100_STARBLE_STEP_WOOD_L = 0,
    SE_EV_025_100_TRAIN_ARRIVE = 6,
    SE_EV_025_100_TRAIN_DOOR_OPEN = 7,
};
enum se_ev_025_105_pal {
    SE_EV_025_105_DEATH_ARC_FLY_1 = 0,
    SE_EV_025_105_DEATH_ARC_PASS_BY = 1,
    SE_EV_025_105_MAX_FACE_MONICA = 2,
    SE_EV_025_105_MONICA_FACE_MAX = 3,
    SE_EV_025_105_MONICA_NOD = 4,
};
enum se_ev_025_110_pal {
    SE_EV_025_110_GUISCON_TURN_AROUND = 2,
    SE_EV_025_110_GUISCON_TAKE_OUT_BOMB = 3,
    SE_EV_025_110_GET_EFFECT = 4,
    SE_EV_025_110_GUISCON_CLENCH_FIST = 5,
};
enum se_ev_025_115_pal {
    SE_EV_025_115_DONAHORN_STEP_L = 0,
    SE_EV_025_115_ELEVATOR_DOOR_OPEN = 2,
    SE_EV_025_115_DONAHORN_TURN_AROUND = 3,
    SE_EV_025_115_MAX_GREET = 4,
    SE_EV_025_115_DONAHORN_HAND_HORN = 5,
    SE_EV_025_115_GET_EFFECT = 6,
    SE_EV_025_115_MAX_NOD = 7,
    SE_EV_025_115_DONAHORN_TURN_AROUND_2 = 8,
    SE_EV_025_115_DONAHORN_HAND_ON_SHOULDER = 9,
};
enum se_ev_025_120_pal {
    SE_EV_025_120_SOLDIERS_RUN_AROUND = 4,
    SE_EV_025_120_SOLDIER_TURN_AROUND = 5,
    SE_EV_025_120_DONAHORN_STEP_FORWARD = 6,
    SE_EV_025_120_MAX_HUG = 7,
    SE_EV_025_120_MAX_RUN_OFF = 8,
    SE_EV_025_120_DOOR = 9,
    SE_EV_025_120_MONICA_FIST_PUMP = 10,
    SE_EV_025_120_GOLEM_MARCH = 11,
};
enum se_ev_025_125_pal {
    SE_EV_025_125_MAX_NOD = 0,
};
enum se_ev_025_135_pal {
    SE_EV_025_135_PAZNOS_SOUND = 0,
    SE_EV_025_135_PAZNOS_SFX_1 = 1,
    SE_EV_025_135_PAZNOS_SFX_2 = 2,
    SE_EV_025_135_PAZNOS_SFX_3 = 3,
    SE_EV_025_135_PAZNOS_SFX_4 = 4,
};
enum se_ev_025_140_pal {
    SE_EV_025_140_DONAHORN_DRAW_SWORD = 2,
    SE_EV_025_140_MONICA_HOLD_SWORD = 3,
    SE_EV_025_140_MONICA_HOLD_SWORD_2 = 4,
    SE_EV_025_140_ITEM_GET = 5,
};
enum se_ev_030_100_pal {
    SE_EV_030_100_GATE_OPEN = 0,
};
enum se_ev_030_105_pal {
    SE_EV_030_105_ANDREA_APPEAR = 0,
    SE_EV_030_105_BACKGROUND_CHANGE = 1,
    SE_EV_030_105_ANDREA_VANISH = 2,
    SE_EV_030_105_MAX_TURN_AROUND = 3,
    SE_EV_030_105_MONICA_TURN_AROUND = 4,
    SE_EV_030_105_MAX_TURN_AROUND_2 = 5,
    SE_EV_030_105_MONICA_TURN_AROUND_2 = 6,
};
enum se_ev_030_110_pal {
    SE_EV_030_110_SCREEN_FLASH = 0,
    SE_EV_030_110_WHITE_FADE = 1,
};
enum se_ev_030_115_pal {
    SE_EV_030_115_SAN_STARTLED = 0,
    SE_EV_030_115_SAN_FALL = 1,
    SE_EV_030_115_ANDREA_HUG_SAN = 2,
};
enum se_ev_030_120_pal {
    SE_EV_030_120_SOLDIER_STEPS = 0,
    SE_EV_030_120_SOLDIER_STEPS_2 = 1,
    SE_EV_030_120_SOLDIER_SHOVE = 2,
    SE_EV_030_120_ANDREA_CROUCH = 3,
    SE_EV_030_120_SAN_TURN_AROUND = 4,
    SE_EV_030_120_SAN_LOOK_BACK = 5,
    SE_EV_030_120_ANDREA_STAND_UP = 6,
};
enum se_ev_040_100_pal {
    SE_EV_040_100_OSMOND_SPIN = 1,
    SE_EV_040_100_PALACE_ORB_FORM = 3,
    SE_EV_040_100_PALACE_FIRE = 4,
    SE_EV_040_100_EXPLOSION = 5,
    SE_EV_040_100_PALACE_ORB_FORM_SMALL = 6,
    SE_EV_040_100_PALACE_FIRE_SMALL = 7,
    SE_EV_040_100_EXPLOSION_LARGE = 8,
    SE_EV_040_100_CATCH_FIRE = 9,
    SE_EV_040_100_CARPEN_FLY_LARGE = 10,
    SE_EV_040_100_CONDA_SWING_STAFF = 11,
};
enum se_ev_040_101_pal {
    SE_EV_040_101_PALACE_ORB_FORM = 0,
    SE_EV_040_101_PALACE_FIRE = 1,
    SE_EV_040_101_EXPLOSION_LARGE = 2,
    SE_EV_040_101_CARPEN_FLY = 3,
    SE_EV_040_101_CARPEN_SPREAD_ARMS = 4,
    SE_EV_040_101_CARPEN_THROW_ROCK = 5,
    SE_EV_040_101_CARPEN_ROCK_HIT = 6,
    SE_EV_040_101_IXION_DOOR_OPEN = 7,
    SE_EV_040_101_IXION_WHISTLE = 8,
    SE_EV_040_101_IXION_DEPART = 9,
};
enum se_ev_040_102_pal {
    SE_EV_040_102_IXION_ARRIVE = 0,
    SE_EV_040_102_ALARM_RING = 1,
    SE_EV_040_102_OSMOND_TAKE_OUT_PROJECTOR = 2,
    SE_EV_040_102_EINA_APPEAR = 3,
    SE_EV_040_102_RUINS_GLOW = 4,
    SE_EV_040_102_RUINS_ROTATE = 5,
    SE_EV_040_102_GET_EFFECT = 6,
    SE_EV_040_102_EINA_VANISH = 7,
    SE_EV_040_102_OSMOND_BOAST = 8,
    SE_EV_040_102_BOTH_NOD = 9,
    SE_EV_040_102_MAX_NOD = 10,
};
enum se_ev_040_105_pal {
    SE_EV_040_105_EARTHQUAKE = 0,
    SE_EV_040_105_CHRONO_UNION_ACTIVATE = 1,
};
enum se_ev_040_106_pal {
    SE_EV_040_106_GRIFFON_SPREAD_WINGS = 0,
    SE_EV_040_106_GRIFFON_BATTLE_INTRO = 1,
    SE_EV_040_106_PALACE_ATTACK_READY = 2,
    SE_EV_040_106_PALACE_ATTACK_FIRE = 3,
    SE_EV_040_106_PAZNOS_DEFLECT_ATTACK = 4,
};
enum se_ev_040_107_pal {
    SE_EV_040_107_PAZNOS_FLY = 0,
    SE_EV_040_107_PAZNOS_CANNON_CHARGE_LARGE = 1,
    SE_EV_040_107_PAZNOS_CANNON_CHARGE_SMALL = 2,
    SE_EV_040_107_PAZNOS_CANNON_CHARGE_SMALL_2 = 3,
    SE_EV_040_107_DONAHORN_WAVE = 4,
};
enum se_ev_040_108_pal {
    SE_EV_040_108_PAZNOS_CANNON_FIRE = 0,
    SE_EV_040_108_PALACE_EXPLOSION = 1,
    SE_EV_040_108_PALACE_FALL = 2,
    SE_EV_040_108_MAX_FLY = 3,
    SE_EV_040_108_SOLDIER_FLIP_SWITCH = 4,
    SE_EV_040_108_PALACE_DUST_CLOUD = 5,
};
enum se_ev_040_109_pal {
    SE_EV_040_109_PAZNOS_TRANSFORM_1 = 0,
    SE_EV_040_109_PAZNOS_SPIKES_EXTEND = 1,
    SE_EV_040_109_PAZNOS_ARMS_COMBINE = 2,
    SE_EV_040_109_PAZNOS_HANDS_EMERGE = 3,
    SE_EV_040_109_PAZNOS_LEGS_COMBINE = 4,
    SE_EV_040_109_PAZNOS_FACE_EMERGE = 5,
    SE_EV_040_109_PAZNOS_EYES_GLOW = 6,
    SE_EV_040_109_CAMERA_PULL_BACK = 7,
};
enum se_ev_040_110_pal {
    SE_EV_040_110_OSMOND_STEP_1_L = 0,
    SE_EV_040_110_MAX_OSMOND_SIT = 2,
    SE_EV_040_110_MONICA_SIT = 3,
    SE_EV_040_110_CHAIR_SPIN = 4,
    SE_EV_040_110_OSMOND_TYPE_KEYS = 5,
    SE_EV_040_110_SCREEN_SHOW_EINA = 6,
    SE_EV_040_110_MAX_NOD = 7,
    SE_EV_040_110_MONICA_NOD = 8,
    SE_EV_040_110_IXION_DEPART = 9,
};
enum se_ev_040_111_pal {
    SE_EV_040_111_OSMOND_STEP_GRASS_L = 0,
    SE_EV_040_111_IXION_LAND = 4,
};
enum se_ev_040_200_pal {
    SE_EV_040_200_PAZNOS_FLY_1 = 0,
    SE_EV_040_200_PAZNOS_FLY_OFF = 1,
    SE_EV_040_200_PAZNOS_AWAIT_PALACE = 2,
    SE_EV_040_200_PAZNOS_CATCH_PALACE = 3,
    SE_EV_040_200_PAZNOS_EYES_GLOW = 4,
    SE_EV_040_200_PAZNOS_THROW_PALACE = 5,
    SE_EV_040_200_PALACE_FALL = 6,
    SE_EV_040_200_PAZNOS_FLY_3 = 7,
    SE_EV_040_200_PAZNOS_LAND = 8,
};
enum se_ev_100_100_pal {
    SE_EV_100_100_DONNY_SLIDE = 8,
    SE_EV_100_100_DONNY_LAND = 9,
    SE_EV_100_100_MAX_INSISTS = 10,
    SE_EV_100_100_TAKE_OUT_WRENCH = 11,
    SE_EV_100_100_HAND_OVER_GUN = 12,
    SE_EV_100_100_RECEIVE_GUN = 13,
    SE_EV_100_100_ITEM_GET = 14,
    SE_EV_100_100_GET_EFFECT = 15,
    SE_EV_100_100_MAX_JUMP_DOWN = 16,
    SE_EV_100_100_MAX_LAND = 17,
    SE_EV_100_100_DONNY_TAKE_OUT_RECEIVER = 18,
    SE_EV_100_100_MAX_RECEIVE_RECEIVER = 19,
};
enum se_ev_100_105_pal {
    SE_EV_100_105_CALL_BELL = 0,
    SE_EV_100_105_TAKE_OUT_RECEIVER = 1,
    SE_EV_100_105_SWITCH_ON = 2,
    SE_EV_100_105_SWITCH_OFF = 3,
    SE_EV_100_105_PUT_AWAY_RECEIVER = 4,
};
enum se_ev_100_110_pal {
    SE_EV_100_110_BANG_DESK = 6,
    SE_EV_100_110_OPEN_DOOR = 7,
    SE_EV_100_110_CLOSE_DOOR = 8,
};
enum se_ev_100_115_pal {
    SE_EV_100_115_CALL_BELL = 0,
    SE_EV_100_115_SWITCH_ON = 2,
    SE_EV_100_115_SWITCH_OFF = 3,
    SE_EV_100_115_PUT_AWAY_RECEIVER = 4,
};
enum se_ev_100_120_pal {
    SE_EV_100_120_TAKE_OUT_RECEIVER = 0,
    SE_EV_100_120_SWITCH_ON = 1,
    SE_EV_100_120_SWITCH_OFF = 2,
    SE_EV_100_120_PUT_AWAY_RECEIVER = 3,
    SE_EV_100_120_ELEPHANT_FALL = 4,
    SE_EV_100_120_ELEPHANT_THREATEN = 5,
    SE_EV_100_120_POLKAS_APPEAR = 6,
    SE_EV_100_120_MAX_LOOK_UP = 7,
    SE_EV_100_120_MAX_STEP_FORWARD = 8,
    SE_EV_100_120_MAX_PULL_BACK = 9,
    SE_EV_100_120_MAX_READY_STANCE = 10,
    SE_EV_100_120_POLKAS_COMMAND = 11,
    SE_EV_100_120_GATE_CLOSE = 27,
};
enum se_ev_100_125_pal {
    SE_EV_100_125_STEVE_FOOTSTEPS = 0,
    SE_EV_100_125_RAISE_LEG = 1,
    SE_EV_100_125_ROCK_HIT = 2,
    SE_EV_100_125_POLKAS_TURN_AROUND = 3,
    SE_EV_100_125_ENGINE = 4,
    SE_EV_100_125_FIST_PUMP = 5,
    SE_EV_100_125_JUMP = 6,
    SE_EV_100_125_PUNCH_HIT = 7,
    SE_EV_100_125_DAMAGE_VOICE = 8,
    SE_EV_100_125_UPPERCUT = 9,
    SE_EV_100_125_ENGINE_B = 10,
    SE_EV_100_125_FIST_PUMP_B = 11,
    SE_EV_100_125_LINDA_HEAL = 12,
    SE_EV_100_125_LINDA_SHIVER = 13,
    SE_EV_100_125_LINDA_GROWL = 14,
    SE_EV_100_125_ENGINE_C = 15,
    SE_EV_100_125_RAISE_ARM = 16,
    SE_EV_100_125_SWING_ARM = 17,
    SE_EV_100_125_LEVER = 18,
    SE_EV_100_125_STEVE_STOP = 19,
    SE_EV_100_125_ENGINE_2 = 20,
    SE_EV_100_125_LOWER_ARM = 21,
    SE_EV_100_125_RIDE_STEVE = 22,
    SE_EV_100_125_POLKAS_COMMAND_LINDA = 23,
    SE_EV_100_125_JUMP_B = 24,
    SE_EV_100_125_SALUTE = 25,
    SE_EV_100_125_TURN_AROUND = 26,
    SE_EV_100_125_ELEPHANT_ROAR = 27,
    SE_EV_100_125_HEAL_SPELL_PREP = 28,
};
enum se_ev_100_130_pal {
    SE_EV_100_130_ENGINE = 0,
    SE_EV_100_130_JUMP = 1,
    SE_EV_100_130_RECOVER_POSTURE = 2,
    SE_EV_100_130_ELEPHANT_FALL = 3,
    SE_EV_100_130_ELEPHANT_GROWL = 4,
    SE_EV_100_130_POLKAS_APPEAR = 6,
    SE_EV_100_130_POLKAS_SPIN = 7,
    SE_EV_100_130_POLKAS_COMMAND = 9,
    SE_EV_100_130_ELEPHANT_REACTION = 10,
};
enum se_ev_100_135_pal {
    SE_EV_100_135_ENGINE = 0,
    SE_EV_100_135_ELEPHANT_DEATH = 1,
    SE_EV_100_135_POLKAS_FLAIL = 2,
    SE_EV_100_135_POLKAS_FLY_AWAY = 3,
};
enum se_ev_100_136_pal {
    SE_EV_100_136_ENGINE = 0,
    SE_EV_100_136_ELEPHANT_ROAR = 1,
    SE_EV_100_136_JUMP = 2,
    SE_EV_100_136_DOOR_CLOSE = 4,
    SE_EV_100_136_STOP = 5,
};
enum se_ev_100_137_pal {
    SE_EV_100_137_DOOR_OPEN = 0,
    SE_EV_100_137_ENGINE = 1,
    SE_EV_100_137_JUMP = 2,
    SE_EV_100_137_RECOVER_POSTURE = 3,
    SE_EV_100_137_ELEPHANT_FALL = 4,
    SE_EV_100_137_ELEPHANT_GROWL = 5,
    SE_EV_100_137_POLKAS_SPIN = 6,
    SE_EV_100_137_ELEPHANT_ROAR = 8,
};
enum se_ev_100_140_pal {
    SE_EV_100_140_ROBOT_MOVE_LOOP = 24,
    SE_EV_100_140_ROBOT_TURN = 25,
    SE_EV_100_140_POLKAS_JUMP = 26,
    SE_EV_100_140_FALLING_WHISTLE = 27,
    SE_EV_100_140_POLKAS_FREEZE = 28,
    SE_EV_100_140_MAX_READY_STANCE = 29,
    SE_EV_100_140_POLKAS_POSE = 30,
};
enum se_ev_100_145_pal {
    SE_EV_100_145_COLLAPSE = 18,
    SE_EV_100_145_SPARK_1 = 31,
    SE_EV_100_145_SPARK_2 = 32,
    SE_EV_100_145_ROBOT_EXPLODE = 33,
    SE_EV_100_145_DEBRIS_SCATTER = 37,
};
enum se_ev_100_155_pal {
    SE_EV_100_155_TRAIN_WHISTLE = 6,
    SE_EV_100_155_TRAIN_AMBIENCE = 7,
    SE_EV_100_155_TRAIN_STOP = 8,
    SE_EV_100_155_DOOR_OPEN = 9,
    SE_EV_100_155_DOOR_CLOSE = 12,
    SE_EV_100_155_STEPS_RAISE = 13,
    SE_EV_100_155_STEPS_STOW = 14,
    SE_EV_100_155_TRAIN_WHISTLE_2 = 15,
    SE_EV_100_155_TRAIN_DEPART = 16,
};
enum se_ev_110_100_pal {
    SE_EV_110_100_MAX_STOP_MONICA = 4,
    SE_EV_110_100_MONICA_CROUCH = 5,
};
enum se_ev_110_105_pal {
    SE_EV_110_105_GNOME_HAND_ROD = 5,
    SE_EV_110_105_ITEM_GET = 6,
};
enum se_ev_110_110_pal {
    SE_EV_110_110_MONICA_PICK_APPLE = 4,
    SE_EV_110_110_UTAN_RAISE_ARM = 5,
};
enum se_ev_110_115_pal {
    SE_EV_110_115_BACK_AWAY = 4,
};
enum se_ev_110_120_pal {
    SE_EV_110_120_PULL_ROD = 4,
    SE_EV_110_120_GET_PULLED = 5,
    SE_EV_110_120_WATER_SPLASH = 6,
    SE_EV_110_120_LINE_SNAP = 7,
    SE_EV_110_120_COLLAPSE = 8,
    SE_EV_110_120_MARDAN_SURFACE = 9,
    SE_EV_110_120_MARDAN_RISING = 10,
    SE_EV_110_120_MONICA_STARTLED = 11,
};
enum se_ev_110_125_pal {
    SE_EV_110_125_MARDAN_WATER = 4,
    SE_EV_110_125_BREAK_SPELL = 5,
    SE_EV_110_125_RETURN_UNDERWATER = 6,
};
enum se_ev_110_130_pal {
    SE_EV_110_130_THROW_SEED = 0,
    SE_EV_110_130_SEED_FALL = 1,
    SE_EV_110_130_RAFFLESIA_SPROUT = 2,
};
enum se_ev_110_135_pal {
    SE_EV_110_135_THROW_DROPLET = 0,
    SE_EV_110_135_DROPLET_FALL = 1,
    SE_EV_110_135_LIGHT = 2,
    SE_EV_110_135_EARTHQUAKE = 3,
    SE_EV_110_135_MONICA_TRIP = 4,
    SE_EV_110_135_MAX_TRIP = 5,
};
enum se_ev_110_140_pal {
    SE_EV_110_140_CLING = 0,
    SE_EV_110_140_GRAB_VINE = 1,
    SE_EV_110_140_CLIMB_UP = 2,
};
enum se_ev_110_145_pal {
    SE_EV_110_145_JUMP = 0,
    SE_EV_110_145_MONICA_LAND = 1,
    SE_EV_110_145_MAX_TRIP = 2,
    SE_EV_110_145_BUTTERFLY_CRY = 4,
    SE_EV_110_145_FLY_OVERHEAD = 5,
};
enum se_ev_110_150_pal {
    SE_EV_110_150_BUTTERFLIES_GATHER = 0,
    SE_EV_110_150_HOLLY_GLOW = 1,
    SE_EV_110_150_OFFER_LETTER = 2,
    SE_EV_110_150_RECEIVE_LETTER = 3,
    SE_EV_110_150_GET_EFFECT = 4,
    SE_EV_110_150_HOLLY_GLOW_2 = 5,
    SE_EV_110_150_HOLLY_SPLIT = 6,
};
enum se_ev_110_160_pal {
    SE_EV_110_160_SPARKLE = 0,
    SE_EV_110_160_GET_EFFECT = 1,
};
enum se_ev_115_100_pal {
    SE_EV_115_100_ENTER_FLOOR = 0,
};
enum se_ev_115_105_pal {
    SE_EV_115_105_CONDOR_CRY = 0,
    SE_EV_115_105_WINGS_SMALL = 1,
    SE_EV_115_105_ARM_SWING = 3,
};
enum se_ev_115_110_pal {
    SE_EV_115_110_CONDOR_CRY = 0,
    SE_EV_115_110_WINGS_SMALL = 1,
    SE_EV_115_110_BOSS_WINGS = 2,
    SE_EV_115_110_BOSS_LANDING = 3,
    SE_EV_115_110_SLUMP_DEJECTED = 4,
    SE_EV_115_110_POINT_AT_NEST = 5,
    SE_EV_115_110_TIME_DISTORTION = 6,
    SE_EV_115_110_ARM_SWING = 7,
    SE_EV_115_110_MONICA_TURN_AROUND = 8,
    SE_EV_115_110_MONICA_TURN_AROUND_2 = 9,
    SE_EV_115_110_TIME_FRAGMENT = 10,
};
enum se_ev_115_115_pal {
    SE_EV_115_115_RAISE_FINGER = 0,
    SE_EV_115_115_TALK_MOTION = 1,
    SE_EV_115_115_TALK_MOTION_2 = 2,
    SE_EV_115_115_TURN_AROUND = 3,
    SE_EV_115_115_FIST_PUMP = 4,
    SE_EV_115_115_DISAPPOINTED_POSE = 5,
};
enum se_ev_115_120_pal {
    SE_EV_115_120_HOLD_OUT_HAND = 0,
    SE_EV_115_120_LOOK_AT_DISTORTION = 1,
    SE_EV_115_120_PLANT_FEET = 2,
    SE_EV_115_120_WIND_UP = 3,
    SE_EV_115_120_FIST_PUMP = 5,
};
enum se_ev_115_130_pal {
    SE_EV_115_130_TAKEOFF = 0,
    SE_EV_115_130_EXCHANGE_GLANCES = 2,
    SE_EV_115_130_BOSS_LANDING = 3,
    SE_EV_115_130_TAKEOFF_2 = 4,
    SE_EV_115_130_DROP_ITEM = 5,
    SE_EV_115_130_CATCH_ITEM = 6,
    SE_EV_115_130_GET_EFFECT = 7,
    SE_EV_115_130_FLY_AWAY = 8,
};
enum se_ev_115_135_pal {
    SE_EV_115_135_BIRD_CRY = 0,
    SE_EV_115_135_WINGS_SMALL = 1,
    SE_EV_115_135_WINGS_LARGE = 2,
    SE_EV_115_135_TAKE_FLIGHT = 3,
    SE_EV_115_135_BRANCH_RUSTLE = 4,
};
enum se_ev_115_140_pal {
    SE_EV_115_140_FLASH = 2,
    SE_EV_115_140_TURN_AROUND = 3,
    SE_EV_115_140_CAMERA_PULL_BACK = 4,
    SE_EV_115_140_LIN_TURN_AROUND = 5,
    SE_EV_115_140_LIN_FOLD_ARMS = 6,
    SE_EV_115_140_CREST_HAND_TO_HEAD = 7,
    SE_EV_115_140_FLASH_2 = 8,
    SE_EV_115_140_MEMO_EATER_EMERGE = 9,
    SE_EV_115_140_MEMO_EATER_LEAP = 10,
    SE_EV_115_140_MEMO_EATER_LAND = 11,
    SE_EV_115_140_MEMO_EATER_CRY = 12,
    SE_EV_115_140_MAX_NOD = 13,
};
enum se_ev_115_145_pal {
    SE_EV_115_145_MEMO_EATER_DEATH = 2,
    SE_EV_115_145_MEMO_EATER_VANISH = 3,
    SE_EV_115_145_LIN_STAND = 4,
    SE_EV_115_145_MONICA_NOD = 5,
    SE_EV_115_145_MAX_NOD = 6,
};
enum se_ev_115_150_pal {
    SE_EV_115_150_MONICA_TURN_BACK = 6,
};
enum se_ev_115_155_pal {
    SE_EV_115_155_MONICA_CROUCH = 0,
    SE_EV_115_155_PLACE_HOURGLASS = 1,
    SE_EV_115_155_HOURGLASS_GLOW = 2,
    SE_EV_115_155_HOURGLASS_GLOW_2 = 3,
};
enum se_ev_115_156_pal {
    SE_EV_115_156_DEATH_ARC_WARP_OUT = 4,
    SE_EV_115_156_DEATH_ARC_FLIGHT = 5,
    SE_EV_115_156_CREST_LEAN_FORWARD = 6,
    SE_EV_115_156_LIN_LEAN_FORWARD = 7,
    SE_EV_115_156_DEATH_ARC_FLAMES = 8,
    SE_EV_115_156_GASPARD_TURN_AROUND = 9,
    SE_EV_115_156_JAMMING_NOD = 10,
    SE_EV_115_156_CANNON_MOVE = 11,
    SE_EV_115_156_CANNON_ADJUST = 12,
    SE_EV_115_156_CANNON_FIRE = 13,
    SE_EV_115_156_CANNON_IMPACT = 14,
    SE_EV_115_156_FLAME = 15,
    SE_EV_115_156_CREST_GET_UP = 16,
    SE_EV_115_156_CREST_STAND_UP = 17,
};
enum se_ev_115_157_pal {
    SE_EV_115_157_FLAME = 0,
    SE_EV_115_157_FACE_EACH_OTHER = 1,
};
enum se_ev_115_158_pal {
    SE_EV_115_158_DEATH_ARC_FLIGHT_SMALL = 4,
    SE_EV_115_158_ELEVATOR_RISE = 5,
    SE_EV_115_158_ELEVATOR_STOP = 6,
    SE_EV_115_158_CLANG = 7,
    SE_EV_115_158_FLAMES = 8,
    SE_EV_115_158_LIN_HUG = 9,
    SE_EV_115_158_CREST_TURN_AROUND = 11,
    SE_EV_115_158_CREST_LOOK_UP = 12,
    SE_EV_115_158_CREST_JUMP = 13,
    SE_EV_115_158_FLAMES_SMALL = 14,
    SE_EV_115_158_CREST_JUMP_2 = 15,
    SE_EV_115_158_CREST_LAND = 16,
    SE_EV_115_158_DEATH_ARC_FLIGHT_LARGE = 17,
    SE_EV_115_158_FLAMES_MEDIUM = 18,
    SE_EV_115_158_MONICA_JUMP = 19,
    SE_EV_115_158_MAX_LEAN_FORWARD = 20,
    SE_EV_115_158_MONICA_LAND = 21,
    SE_EV_115_158_JAMMING_BOW = 22,
    SE_EV_115_158_JAMMING_VANISH = 23,
    SE_EV_115_158_GASPARD_BOW = 24,
    SE_EV_115_158_MONICA_RAISE_ARM = 25,
    SE_EV_115_158_MONICA_READY_SWORD = 26,
    SE_EV_115_158_FLAMES_B = 27,
    SE_EV_115_158_FLAME_MONSTER_APPEAR = 28,
    SE_EV_115_158_MAX_READY_STANCE = 29,
    SE_EV_115_158_MONICA_SWORDFIGHT = 30,
    SE_EV_115_158_GASPARD_LAND = 31,
    SE_EV_115_158_FLAME_MONSTER_LAND = 32,
};
enum se_ev_115_160_pal {
    SE_EV_115_160_DEATH_ARC_FLIGHT_MEDIUM = 4,
    SE_EV_115_160_MONICA_LOWER_SWORD = 5,
    SE_EV_115_160_MONICA_THRUST_SWORD = 6,
    SE_EV_115_160_GASPARD_LOOK_UP_MONICA = 7,
    SE_EV_115_160_EXPLOSION = 8,
    SE_EV_115_160_MONICA_TURN_AROUND = 9,
    SE_EV_115_160_GASPARD_TURN_AROUND = 10,
    SE_EV_115_160_GASPARD_JUMP = 11,
    SE_EV_115_160_GASPARD_LAND = 12,
    SE_EV_115_160_JAMMING_APPEAR = 13,
    SE_EV_115_160_FIRE_CRACKLE = 14,
    SE_EV_115_160_CREST_MAGIC_POSE = 15,
    SE_EV_115_160_CRYSTAL_FIRE = 16,
    SE_EV_115_160_CRYSTAL_IMPACT = 17,
    SE_EV_115_160_DEATH_ARC_FLIGHT = 18,
    SE_EV_115_160_DEATH_ARC_CANNON_MOVE = 19,
    SE_EV_115_160_DEATH_ARC_CANNON_FIRE = 20,
    SE_EV_115_160_DEATH_ARC_CANNON_IMPACT = 21,
    SE_EV_115_160_SHOCK = 22,
    SE_EV_115_160_DEATH_ARC_CANNON_IMPACT_B = 23,
    SE_EV_115_160_CREST_TURN_AROUND = 24,
    SE_EV_115_160_DEATH_ARC_FLIGHT_B = 25,
    SE_EV_115_160_MONICA_JUMP = 26,
    SE_EV_115_160_MONICA_LAND = 27,
    SE_EV_115_160_GASPARD_TURN_AROUND_B = 28,
    SE_EV_115_160_DEATH_ARC_WARP_IN = 29,
    SE_EV_115_160_FIRE_CRACKLE_B = 30,
    SE_EV_115_160_LIN_HELP_UP_2 = 32,
    SE_EV_115_160_LIN_RECOIL = 33,
    SE_EV_115_160_LIN_TURN_AROUND = 34,
    SE_EV_115_160_LIN_TURN_AROUND_2 = 35,
    SE_EV_115_160_MONICA_NOD = 36,
};
enum se_ev_115_165_pal {
    SE_EV_115_165_SPARKLE = 0,
    SE_EV_115_165_GET_EFFECT = 1,
};
enum se_ev_120_100_pal {
    SE_EV_120_100_EINA_CLOSE_BOOK = 0,
    SE_EV_120_100_MAX_TURN_AROUND = 1,
    SE_EV_120_100_MAX_GET_UP = 2,
};
enum se_ev_120_105_pal {
    SE_EV_120_105_SHINGALA_EMERGE_WATER = 2,
    SE_EV_120_105_SHINGALA_ATTACK = 3,
    SE_EV_120_105_MAX_ARM_SWING = 4,
    SE_EV_120_105_PUPPET_HELMET = 5,
    SE_EV_120_105_GATE_OPEN = 6,
    SE_EV_120_105_SHINGALA_ATTACK_2 = 7,
    SE_EV_120_105_SHINGALA_ATTACK_3 = 8,
    SE_EV_120_105_GATE_OPEN_B = 9,
    SE_EV_120_105_PAU_RAISE_HEAD = 10,
    SE_EV_120_105_GATE_CLOSE = 11,
};
enum se_ev_120_110_pal {
    SE_EV_120_110_GATE_OPEN = 0,
    SE_EV_120_110_PUPPET_HELMET = 1,
};
enum se_ev_120_115_pal {
    SE_EV_120_115_GATE_OPEN = 0,
    SE_EV_120_115_PUPPET_HELMET = 1,
    SE_EV_120_115_MAX_NOD = 2,
};
enum se_ev_120_120_pal {
    SE_EV_120_120_SHINGALA_SHOCKED = 4,
    SE_EV_120_120_HELMET_FALLS_OFF = 5,
    SE_EV_120_120_SHINGALA_COLLAPSE = 6,
    SE_EV_120_120_SHINGALA_PLAY = 7,
    SE_EV_120_120_EXCHANGE_GLANCES = 8,
    SE_EV_120_120_MONICA_FOLD_ARMS = 9,
    SE_EV_120_120_MAX_LOWER_ARM = 10,
};
enum se_ev_120_125_pal {
    SE_EV_120_125_MONICA_RAISE_HAND = 0,
    SE_EV_120_125_MONICA_CLENCH_HAND = 1,
};
enum se_ev_120_130_pal {
    SE_EV_120_130_USE_TRANSLATOR = 0,
    SE_EV_120_130_BAL_LOWER_HEAD = 1,
    SE_EV_120_130_GET_EFFECT = 2,
};
enum se_ev_120_140_pal {
    SE_EV_120_140_APPLY_MEDICINE = 7,
    SE_EV_120_140_HEALING_EFFECT = 8,
    SE_EV_120_140_SIGU_PLAY_WITH_PAU = 9,
    SE_EV_120_140_DEATH_ARC_FLYING = 10,
    SE_EV_120_140_DEATH_ARC_FLYING_2 = 11,
    SE_EV_120_140_DEATH_ARC_FLYOVER = 12,
    SE_EV_120_140_MONICA_TURN_AROUND = 13,
    SE_EV_120_140_MONICA_TURN_BACK = 14,
};
enum se_ev_120_145_pal {
    SE_EV_120_145_DEATH_ARC_FLYING = 0,
    SE_EV_120_145_SHIGURA_APPROACH = 1,
    SE_EV_120_145_MAX_POINT = 2,
    SE_EV_120_145_MARIOMETT_SOUND = 3,
    SE_EV_120_145_SAUCER_FLYING = 4,
    SE_EV_120_145_JAMMING_PLAY_GUITAR = 5,
    SE_EV_120_145_JAMMING_HAND_TO_EAR = 6,
    SE_EV_120_145_JAMMING_PLAY_GUITAR_2 = 7,
    SE_EV_120_145_MONICA_HALF_STEP = 8,
};
enum se_ev_120_150_pal {
    SE_EV_120_150_BURNING_SAUCER = 7,
    SE_EV_120_150_JAMMING_FALL = 8,
    SE_EV_120_150_SHIGURA_RAISE_HEAD = 9,
    SE_EV_120_150_SHIGURA_POKE_JAMMING = 10,
    SE_EV_120_150_DEATH_ARC_FLYING = 12,
    SE_EV_120_150_DEATH_ARC_FLAMES = 13,
    SE_EV_120_150_GASPARD_TURN_AROUND = 14,
    SE_EV_120_150_DEATH_ARC_FLYING_2 = 15,
    SE_EV_120_150_MAX_MONICA_TURN_AROUND = 16,
    SE_EV_120_150_SIGU_LOWER_HEAD = 17,
    SE_EV_120_150_BAL_TURN_AROUND = 18,
    SE_EV_120_150_SIGU_LOWER_HEAD_2 = 19,
    SE_EV_120_150_SIGU_RAISE_HEAD = 20,
    SE_EV_120_150_PAU_TURN_AROUND = 21,
};
enum se_ev_120_155_pal {
    SE_EV_120_155_LAPIS_SPARKLE = 0,
    SE_EV_120_155_ITEM_GET_EFFECT = 1,
};
enum se_ev_125_100_pal {
    SE_EV_125_100_FIRE_SQUALL = 1,
};
enum se_ev_125_105_pal {
    SE_EV_125_105_MAX_SET_BOMB = 2,
    SE_EV_125_105_FIRE_SQUALL_EXPLOSION = 3,
    SE_EV_125_105_MAX_HOP = 4,
    SE_EV_125_105_DEATH_ARC_FLYING_MID = 5,
    SE_EV_125_105_DEATH_ARC_FLYING_LOUD = 6,
    SE_EV_125_105_DEATH_ARC_FLAMES = 7,
    SE_EV_125_105_GASPARD_TURN_AROUND = 8,
    SE_EV_125_105_DEATH_ARC_TAKE_AIM = 11,
    SE_EV_125_105_DEATH_ARC_FLYING_MID_2 = 12,
    SE_EV_125_105_MACHINE_GUN_1 = 13,
    SE_EV_125_105_MAX_STARTLED = 14,
    SE_EV_125_105_MACHINE_GUN_2 = 15,
    SE_EV_125_105_MACHINE_GUN_3 = 16,
    SE_EV_125_105_MACHINE_GUN_4 = 17,
    SE_EV_125_105_DEATH_ARC_FLYING_SOFT = 18,
    SE_EV_125_105_DEATH_ARC_FLYING_MID_3 = 19,
    SE_EV_125_105_FIRE_CANNON = 20,
    SE_EV_125_105_DEATH_ARC_EXPLOSION = 21,
    SE_EV_125_105_MONICA_FACE_MAX = 22,
    SE_EV_125_105_MAX_FACE_MONICA = 23,
    SE_EV_125_105_DEATH_ARC_FLYING_SOFT_2 = 24,
    SE_EV_125_105_DEATH_ARC_ATTACK = 25,
    SE_EV_125_105_MAX_NOD = 26,
};
enum se_ev_125_110_pal {
    SE_EV_125_110_DEATH_ARC_FALLING = 2,
    SE_EV_125_110_DEATH_ARC_FLYING_1 = 3,
    SE_EV_125_110_DEATH_ARC_EXPLOSION = 4,
    SE_EV_125_110_DEATH_ARC_FLYING_2 = 5,
    SE_EV_125_110_CRASH_INTO_ROCK = 6,
    SE_EV_125_110_MONICA_JUMP = 7,
    SE_EV_125_110_DEATH_ARC_FLYING_2B = 8,
    SE_EV_125_110_MONICA_LAND = 9,
    SE_EV_125_110_MONICA_MAGIC_CHARGE = 10,
    SE_EV_125_110_MONICA_FIRE_MAGIC = 11,
    SE_EV_125_110_MONICA_MAGIC_HIT = 12,
    SE_EV_125_110_MONICA_SLASH = 13,
    SE_EV_125_110_MONICA_SLASH_2 = 14,
    SE_EV_125_110_MONICA_TURN_AROUND = 15,
    SE_EV_125_110_DEATH_ARC_EXPLOSION_2 = 16,
    SE_EV_125_110_GASPARD_PUNCH_MONICA = 20,
    SE_EV_125_110_MONICA_COLLAPSE = 21,
    SE_EV_125_110_SWORD_DROPS = 22,
    SE_EV_125_110_DEATH_ARC_FALL_1 = 23,
    SE_EV_125_110_DEATH_ARC_FALL_2 = 24,
};
enum se_ev_125_125_pal {
    SE_EV_125_125_FIRE_HORN_BLOW = 0,
    SE_EV_125_125_LAVA_CRACKS = 1,
};
enum se_ev_125_130_pal {
    SE_EV_125_130_GASPARD_TURN_AROUND = 2,
    SE_EV_125_130_GASPARD_SWING_SWORD_1 = 3,
    SE_EV_125_130_GASPARD_JUMP = 4,
    SE_EV_125_130_GASPARD_LAND = 5,
    SE_EV_125_130_GASPARD_RAISE_SWORD = 6,
    SE_EV_125_130_GASPARD_SWING_SWORD_2 = 7,
};
enum se_ev_125_135_pal {
    SE_EV_125_135_GASPARD_STAND_UP = 2,
    SE_EV_125_135_MONICA_GET_UP = 3,
    SE_EV_125_135_MONICA_SLASH_AT = 4,
    SE_EV_125_135_GASPARD_SHOCKWAVE = 5,
    SE_EV_125_135_MONICA_COLLAPSE = 6,
    SE_EV_125_135_MONICA_LOOK_UP = 7,
    SE_EV_125_135_GASPARD_TURN_AROUND = 8,
    SE_EV_125_135_GASPARD_TURN_AROUND_2 = 9,
    SE_EV_125_135_MONICA_STAND_UP = 10,
    SE_EV_125_135_GASPARD_STARTLED = 11,
    SE_EV_125_135_GASPARD_REACH_OUT = 12,
    SE_EV_125_135_GASPARD_TURN_AROUND_3 = 13,
    SE_EV_125_135_GASPARD_RAISE_HAND = 14,
    SE_EV_125_135_GATE_LAUNCH = 15,
    SE_EV_125_135_STRANGE_LIGHT = 16,
    SE_EV_125_135_GASPARD_HANG_HEAD = 17,
    SE_EV_125_135_GASPARD_RECOIL = 18,
    SE_EV_125_135_MAX_MONICA_LOOK_UP = 19,
    SE_EV_125_135_GASPARD_STRUGGLE = 20,
    SE_EV_125_135_GASPARD_TRANSFORM = 21,
    SE_EV_125_135_GASPARD_ATTACK = 22,
    SE_EV_125_135_MONICA_TURN_AROUND = 23,
};
enum se_ev_125_140_pal {
    SE_EV_125_140_GASPARD_EXPLOSION = 2,
    SE_EV_125_140_MAX_MONICA_CROUCH = 3,
    SE_EV_125_140_GASPARD_MOVE_HEAD = 4,
    SE_EV_125_140_GASPARD_MOVE_HEAD_2 = 5,
    SE_EV_125_140_GASPARD_VANISH = 6,
    SE_EV_125_140_GASPARD_OPEN_DOOR = 7,
    SE_EV_125_140_DOOR_CLOSES = 8,
    SE_EV_125_140_GASPARD_SIT_IN_CHAIR = 9,
    SE_EV_125_140_MAX_FACE_MONICA = 10,
};
enum se_ev_125_145_pal {
    SE_EV_125_145_LAPIS_SPARKLE = 0,
    SE_EV_125_145_ITEM_GET_EFFECT = 1,
};
enum se_ev_130_100_pal {
    SE_EV_130_100_MAX_READY_STANCE = 3,
    SE_EV_130_100_MONICA_READY_STANCE = 4,
    SE_EV_130_100_SUN_REMOVE_HOOD = 5,
};
enum se_ev_130_105_pal {
    SE_EV_130_105_WHITE_FADE_1 = 0,
    SE_EV_130_105_WHITE_FADE_2 = 1,
    SE_EV_130_105_WIND = 2,
    SE_EV_130_105_LIGHT_GLOW = 3,
    SE_EV_130_105_SUNFLOWER_VANISH = 4,
};
enum se_ev_130_110_pal {
    SE_EV_130_110_SUN_COLLAPSE = 0,
    SE_EV_130_110_SUN_GET_UP = 1,
    SE_EV_130_110_SUN_STEAL_ATLA_1 = 2,
    SE_EV_130_110_SUN_STEAL_ATLA_2 = 3,
    SE_EV_130_110_SUN_JUMP = 4,
    SE_EV_130_110_SUN_LAND = 5,
    SE_EV_130_110_SUN_TURN_AROUND = 6,
    SE_EV_130_110_SUN_TRANSFORM = 7,
    SE_EV_130_110_GRIFFON_RAISE_WIND = 8,
    SE_EV_130_110_GRIFFON_CLENCH_FIST = 9,
    SE_EV_130_110_GRIFFON_WAVE_HAND = 10,
    SE_EV_130_110_MAX_READY_STANCE = 11,
    SE_EV_130_110_MAX_WAVE_HAND = 12,
    SE_EV_130_110_GRIFFON_RAISE_HAND = 13,
    SE_EV_130_110_MAX_MONICA_READY = 14,
    SE_EV_130_110_GRIFFON_LOOK_TO_SKY = 15,
    SE_EV_130_110_EARTHQUAKE = 16,
};
enum se_ev_130_111_pal {
    SE_EV_130_111_EARTHQUAKE = 0,
    SE_EV_130_111_ROCK_JUTS_OUT = 1,
    SE_EV_130_111_PALACE_RISING = 2,
    SE_EV_130_111_PALACE_RISING_2 = 3,
    SE_EV_130_111_RING_APPEARS = 4,
};
enum se_ev_130_112_pal {
    SE_EV_130_112_GRIFFON_LOWER_HAND = 0,
    SE_EV_130_112_MONICA_WAVE_HAND = 1,
    SE_EV_130_112_GRIFFON_RAISE_HAND = 2,
    SE_EV_130_112_EARTHQUAKE = 3,
};
enum se_ev_130_113_pal {
    SE_EV_130_113_PALACE_VANISH = 0,
};
enum se_ev_130_114_pal {
    SE_EV_130_114_GRIFFON_LOWER_HAND = 0,
};
enum se_ev_130_115_pal {
    SE_EV_130_115_MAX_RAISE_ARM = 0,
    SE_EV_130_115_MONICA_EXCITED = 1,
};
enum se_ev_130_120_pal {
    SE_EV_130_120_WHITE_FLASH = 0,
    SE_EV_130_120_WHITE_FADE = 1,
    SE_EV_130_120_ANDREA_CROUCH = 2,
    SE_EV_130_120_ANDREA_STAND_UP = 3,
    SE_EV_130_120_DOOR_OPENS = 4,
};
enum se_ev_130_125_pal {
    SE_EV_130_125_DOOR_OPENS = 0,
};
enum se_ev_130_130_pal {
    SE_EV_130_130_GRIFFON_TURN_AROUND = 0,
    SE_EV_130_130_MAX_READY_STANCE = 1,
    SE_EV_130_130_MONICA_POINT = 2,
    SE_EV_130_130_GRIFFON_FLURRY_1 = 3,
    SE_EV_130_130_GRIFFON_FLURRY_2 = 4,
};
enum se_ev_130_135_pal {
    SE_EV_130_135_GRIFFON_LAND = 0,
    SE_EV_130_135_GRIFFON_STAND_UP = 1,
    SE_EV_130_135_GRIFFON_SPREAD_WINGS = 2,
    SE_EV_130_135_GRIFFON_HOLD_WINGS = 3,
    SE_EV_130_135_GRIFFON_EYES_GLOW = 4,
    SE_EV_130_135_GRIFFON_SPREAD_SWORDS = 5,
    SE_EV_130_135_GRIFFON_READY_STANCE = 6,
};
enum se_ev_130_140_pal {
    SE_EV_130_140_GRIFFON_COLLAPSE = 0,
    SE_EV_130_140_GRIFFON_SHOUT = 1,
    SE_EV_130_140_GRIFFON_HAND_IGNITES = 2,
    SE_EV_130_140_GRIFFON_LEAN_FORWARD = 3,
    SE_EV_130_140_GRIFFON_READY_STANCE = 4,
};
enum se_ev_130_145_pal {
    SE_EV_130_145_GRIFFON_COLLAPSE = 0,
    SE_EV_130_145_LIGHT_SPARKLE = 1,
};
enum se_ev_130_146_pal {
    SE_EV_130_146_DE_APPEAR = 0,
    SE_EV_130_146_DE_HAND_EMERGES = 1,
    SE_EV_130_146_DE_RAISE_STAFF = 2,
    SE_EV_130_146_DE_LOWER_STAFF = 3,
    SE_EV_130_146_MAX_SET_DOWN_SUN = 4,
    SE_EV_130_146_MAX_STAND_UP = 5,
    SE_EV_130_146_MAX_READY_STANCE = 6,
    SE_EV_130_146_DE_TRANSFORM = 7,
    SE_EV_130_146_DE_POWER_UP = 8,
    SE_EV_130_146_DE_EYES_GLOW = 9,
    SE_EV_130_146_DE_EMIT_LIGHT = 10,
    SE_EV_130_146_DE_BAND_WRAPS = 11,
};
enum se_ev_130_147_pal {
    SE_EV_130_147_DE_RAISE_ARM = 0,
    SE_EV_130_147_DE_POINT = 1,
    SE_EV_130_147_MOON_SOUND_LOUD = 2,
    SE_EV_130_147_MOON_SOUND_SOFT = 3,
    SE_EV_130_147_DE_FOLD_HANDS = 4,
    SE_EV_130_147_DE_SPREAD_ARMS = 5,
    SE_EV_130_147_DE_MAGIC_HIT = 6,
    SE_EV_130_147_MAX_MONICA_TURN_AROUND = 7,
    SE_EV_130_147_SUN_MAGIC_CHARGE = 8,
    SE_EV_130_147_DE_THROW_STAFF = 9,
    SE_EV_130_147_SUN_STAFF_IMPALE = 10,
    SE_EV_130_147_DE_PULL_STAFF = 11,
    SE_EV_130_147_SUN_STAFF_PULLED_OUT = 12,
    SE_EV_130_147_MAX_MONICA_STAND_UP = 13,
    SE_EV_130_147_DE_WARP_SPACE = 14,
};
enum se_ev_130_148_pal {
    SE_EV_130_148_SPIRAL_OVERVIEW = 0,
    SE_EV_130_148_MAMMO_FALL = 1,
    SE_EV_130_148_MAMMO_TRUNK_SWING = 2,
    SE_EV_130_148_MAMMO_STOMP = 3,
};
enum se_ev_130_150_pal {
    SE_EV_130_150_DE_VANISH = 0,
    SE_EV_130_150_MOON_SOUND = 1,
    SE_EV_130_150_SUN_GET_UP = 2,
    SE_EV_130_150_MOON_SOUND_2 = 3,
    SE_EV_130_150_SUN_HAND_GLOW = 4,
    SE_EV_130_150_SUN_FIRE_MAGIC = 5,
    SE_EV_130_150_MOON_EXPLOSION = 6,
    SE_EV_130_150_SUN_COLLAPSE = 7,
};
enum se_ev_130_160_pal {
    SE_EV_130_160_CHRONO_UNION = 2,
    SE_EV_130_160_MAX_RUB_EYES = 3,
    SE_EV_130_160_EINA_HUG_MAX = 4,
    SE_EV_130_160_FOUR_VANISH = 5,
    SE_EV_130_160_CHRONO_UNION_CLOSE = 6,
};
enum se_ev_130_161_pal {
    SE_EV_130_161_WRITE_LETTER = 0,
    SE_EV_130_161_WRITE_LETTER_2 = 1,
    SE_EV_130_161_PUT_DOWN_PEN = 2,
    SE_EV_130_161_LEAN_BACK_IN_CHAIR = 3,
};
enum se_ev_130_170_pal {
    SE_EV_130_170_DE_FLURRY = 0,
};
enum se_ev_150_100_pal {
    SE_EV_150_100_POLKAS_FLY_IN = 0,
    SE_EV_150_100_POLKAS_RAISE_ARM = 1,
    SE_EV_150_100_POLKAS_FINISH_POSE = 2,
    SE_EV_150_100_POLKAS_CHARGE = 3,
};
enum se_ev_150_105_pal {
    SE_EV_150_105_POLKAS_SPARK = 0,
    SE_EV_150_105_POLKAS_EXPLOSION = 1,
};
enum se_ev_150_110_pal {
    SE_EV_150_110_DYNAMITE_FUSE = 0,
    SE_EV_150_110_DYNAMITE_EXPLOSION = 1,
    SE_EV_150_110_MAX_POINT = 2,
    SE_EV_150_110_MAJIN_LAND = 3,
    SE_EV_150_110_MAJIN_CROUCH = 4,
    SE_EV_150_110_MAJIN_STEP_FORWARD = 5,
};
enum se_ev_150_115_pal {
    SE_EV_150_115_MAJIN_VANISH = 0,
};
enum se_ev_200_000_pal {
    SE_EV_200_000_BATTLE_MUSIC = 0,
};
enum se_ev_300_100_pal {
    SE_EV_300_100_TRAIN_RUNNING = 2,
    SE_EV_300_100_EXPLOSION = 5,
    SE_EV_300_100_MAX_TURN_AROUND = 6,
    SE_EV_300_100_NPC_TURN_AROUND = 7,
    SE_EV_300_100_ATTACK_LOOP = 8,
};
enum se_sp_000_pal {
    SE_SP_000_ENEMY_WIPEOUT_FANFARE = 0,
};
enum se_sp_006_pal {
    SE_SP_006_ITEM_GET = 0,
};
enum se_sp_010_pal {
    SE_SP_010_CLOWN_APPEARS = 0,
    SE_SP_010_CLOWN_APPEARS_VOICE = 1,
    SE_SP_010_BOX_APPEARS = 2,
    SE_SP_010_WHICH_ONE_VOICE = 3,
    SE_SP_010_TREASURE_APPEARS = 4,
    SE_SP_010_TREASURE_APPEARS_VOICE = 5,
    SE_SP_010_TREASURE_OFFER = 6,
    SE_SP_010_TREASURE_OFFER_VOICE = 7,
};
enum se_sp_011_pal {
    SE_SP_011_COIN_SPIN = 0,
    SE_SP_011_ANGEL_WIN = 1,
    SE_SP_011_DEVIL_LOSE = 2,
    SE_SP_011_COIN_STOP = 3,
};
enum se_sp_012_pal {
    SE_SP_012_GEOSTONE_GET = 0,
    SE_SP_012_GEOSTONE_GET_EFFECT = 1,
};
enum se_sp_013_pal {
    SE_SP_013_TIME_GATE = 0,
};
enum se_sp_014_pal {
    SE_SP_014_GATE_OPEN = 0,
};
enum se_sp_018_pal {
    SE_SP_018_TRAIN_STOP = 0,
};
enum se_sp_019_pal {
    SE_SP_019_IXION_ARRIVE = 0,
};
enum se_sp_026_pal {
    SE_SP_026_INSERT_STONE = 0,
    SE_SP_026_ROCK_BLOWN_AWAY = 1,
};
enum se_sp_027_pal {
    SE_SP_027_WHITE_WIND = 0,
};
enum se_sp_028_pal {
    SE_SP_028_COBWEB_VANISH = 0,
};
enum se_sp_030_pal {
    SE_SP_030_GATE_OPEN = 0,
    SE_SP_030_GATE_CLOSE = 1,
    SE_SP_030_INSERT_HANDLE = 2,
};
enum se_sp_031_pal {
    SE_SP_031_CUT_TREE = 0,
    SE_SP_031_TREE_FALLS_1 = 1,
    SE_SP_031_MAX_JUMP = 2,
    SE_SP_031_LAND_ON_TREE = 3,
    SE_SP_031_LAND_ON_GROUND = 4,
    SE_SP_031_TREE_FALLS_2 = 5,
};
enum se_sp_032_pal {
    SE_SP_032_LEAVE_MAP = 0,
    SE_SP_032_ENTER_MAP = 1,
};
enum se_sp_033_pal {
    SE_SP_033_GATE_OPEN = 0,
    SE_SP_033_GATE_CLOSE = 1,
    SE_SP_033_INSERT_STONE = 2,
};
enum se_sp_034_pal {
    SE_SP_034_INSERT_FRAGMENT = 0,
    SE_SP_034_SHELL_CLOSES = 1,
    SE_SP_034_WATERFALL_CLOSES = 2,
};
enum se_sp_035_pal {
    SE_SP_035_FOUNTAIN = 1,
    SE_SP_035_BLOW_UPWARD = 2,
    SE_SP_035_JUMP = 3,
    SE_SP_035_LAND = 4,
};
enum se_sp_036_pal {
    SE_SP_036_ATTACH_ROPE = 0,
    SE_SP_036_CLIMB_DOWN = 1,
    SE_SP_036_JUMP = 2,
    SE_SP_036_LAND = 3,
};
enum se_sp_037_pal {
    SE_SP_037_GATE_OPEN = 0,
    SE_SP_037_GATE_OPEN_CLOSE = 1,
    SE_SP_037_QUEEN_PITCHER_WATER = 2,
};
enum se_sp_038_pal {
    SE_SP_038_INSERT_NEEDLE = 0,
    SE_SP_038_DOOR_OPENS = 1,
};
enum se_sp_040_pal {
    SE_SP_040_GATE_OPEN = 0,
    SE_SP_040_GATE_CLOSE = 1,
};
enum se_sp_041_pal {
    SE_SP_041_GATE_OPEN = 0,
    SE_SP_041_GATE_CLOSE = 1,
};
enum se_sp_043_pal {
    SE_SP_043_TIME_GATE = 0,
    SE_SP_043_ATLAMILLIA_COLOR_CHANGE = 1,
    SE_SP_043_TIME_WARP = 2,
};
enum se_sp_044_pal {
    SE_SP_044_HIMARA_GIVE_DROP = 0,
};
enum se_sp_047_pal {
    SE_SP_047_DOOR_OPENS = 0,
};
enum se_sp_048_pal {
    SE_SP_048_LEAVE_FLOOR = 0,
    SE_SP_048_ENTER_FLOOR = 1,
};
enum se_sp_049_pal {
    SE_SP_049_ICE_GATE_OPEN = 0,
};
enum se_sp_050_pal {
    SE_SP_050_SEAL = 0,
};
enum se_sp_051_pal {
    SE_SP_051_GRIFFON_APPEARS = 0,
};
enum se_sp_052_pal {
    SE_SP_052_MOON_FALLS = 0,
};

#endif

// c2s SDK. Natives for event scripts: towns, maps, cutscenes and dungeon systems.
//
// Natives are engine functions, declared extern with their number in
// NATIVE(...). A native with one result returns it (count = get_counter(3););
// natives with several results take pointers (get_actor_pos(0, &x, &y, &z);).
// A native's result can only be assigned to a variable: natives push nothing
// on the VM stack, so they cannot appear inside a larger expression.
// OPTIONAL(...) lists arguments a call may leave out. Natives never wait; scripts
// wait with `wait;`, which yields one frame (60 per second). While the player
// skips a cutscene the engine does not run natives until the script reaches
// `skip_end;`.
//
// NATIVE(B, N) means the game's scripts call the native as B + N. A call can
// pick another form with an AT(B) or DIRECT prefix.

#ifndef C2S_EVENT_H
#define C2S_EVENT_H

#include <c2s.h>
#pragma natives event

// Value kinds: what a parameter's value means. They read as their base type;
// the decompiler uses them to comment values (a message id gets the line it
// shows, read from the disc).
typedef any message;
typedef string message_file;
typedef string directory;
typedef int item_id;
typedef any map_id;
typedef int flag;
typedef int bgm_file;
typedef int sound_set;
typedef int system_se;
typedef int chara_slot;
typedef int npc_id;
typedef int monster_kind;
typedef string chara_pack;

enum program {
    // Sets the map up from story flags without starting a scene: most retail bodies only set objects
    // and flags without calling _INITIALIZE, some program 100 functions call the same function first
    // (s01), and the engine starts it directly only from the debug future-map select (mainloop3.cpp).
    setup = 99,
    // Start event of a town, field or interior map: the edit loop runs it on entry when no other event
    // is named (editloop.cpp EditInit and EditMapJump), _MAP_JUMP, _GOTO_INTERIOR, _GOTO_OUTSIDE and
    // _FUNCTION_MAP_JUMP default to it, and retail scripts print "start_event" there; in dungeon
    // scripts it is only the first value of the debug run-event command (dng_debug.cpp command_int).
    map_start = 100,
    // Developer test hook: the edit loop reloads the map script and runs 150 when Square is pressed on
    // the second controller (editloop.cpp kKeyOpenEvent, GamePad Down2), and scripts reuse it as a
    // _LOAD_SCRIPT target.
    debug_test = 150,
    // Train door function point on station maps (s02, s04, s09-s12, s31, s34, i02h01): retail bodies
    // ask whether to depart and then jump to the destination map.
    board_train = 250,
    // Gate event: in towns the engine sends point number 300 when the player stands in the coloured
    // time gate marker (CScene::GetMapEvent, SCENE_GAMEOBJ_SLOT_TG) and scripts warp through it, and
    // on dungeon floors 300 is the first "door01" function point that needs an item to open.
    gate = 300,
    // Locked door of a dungeon floor part: every "key" function point in the dungeon maps carries
    // point number 348, and retail bodies ask to use item 340 to open it.
    key_door = 348,
    // Runs when a fish race in the gyorace subgame ends normally (gyorace.cpp RunEvent(350)); defined
    // only by i03h05.
    fish_race_end = 350,
    // Runs when a fish race ends in the title-screen bonus mode (gyorace.cpp RunEvent(352) when
    // OmakeFlag is set); defined only by i03h05.
    fish_race_end_bonus = 352,
    // Save point: in towns the engine sends point number 400 when the player stands at the save point
    // game object (CScene::GetMapEvent, SCENE_GAMEOBJ_SLOT_SAVEPOINT), and most retail bodies open the
    // menu (open_menu_faded).
    save_point = 400,
    // In towns the edit loop runs it when the player presses the confirm button next to a character
    // (editloop.cpp GetTalkEvent then RunEvent(1000)); in dungeon scripts it is the default start
    // event of the dungeon loop, which retail scripts use to pick the floor from the menu
    // (dng_main.cpp sets event_no 1000 when none is given).
    talk_or_dungeon_start = 1000,
    // Starts a dungeon floor whose number is already chosen: _GOTO_DNG from the town gate menu, the
    // new game and save-load paths in title.cpp and the edit-loop menu jump pass 1010, and retail
    // bodies set the floor up (and program 1000 ends by calling it).
    floor_start = 1010,
    // Runs when the player presses confirm at a treasure chest on a dungeon floor (dng_main.cpp
    // TreasureBoxMan->CheckEvent).
    treasure_chest = 1100,
    // Runs when the player steps onto one of the floor's random circles (dng_main.cpp
    // RandomCircle.CheckEvent).
    random_circle = 1120,
    // Runs when the player presses confirm at the floor's geostone (dng_main.cpp GeoStone.CheckEvent).
    geostone = 1130,
    // Runs when the player presses confirm or square next to a monster that can talk (dng_main.cpp
    // CheckMonsterTolk, which also fills EdEventInfo.monster_talk).
    monster_talk = 1150,
    // Runs when the controlled character's HP reaches zero (actionchara.cpp queues battle_area event
    // 1200 with ACTION_PROG_DEAD), and retail bodies run the game-over sequence.
    player_dead = 1200,
    // Runs once when no monsters or mimics are left on the floor or boss map (dng_main.cpp sets
    // battle_clear).
    monsters_cleared = 1500,
    // One-time help shown the first time Max or Monica presses confirm at a random stone (dng_main.cpp
    // SearchRandomStone, save bit flag 0x32).
    random_stone_help = 2500,
    // One-time help requested by the e202a monster script through _RUN_EVENT_SCRIPT(2510) when its
    // attack type is 0 (guarded by bit flag 51); retail bodies show dungeon message 2610.
    monster_help = 2510,
    // One-time help shown at the first level up in a dungeon (dng_main.cpp LevelupInfo.phase, save bit
    // flag 0x34).
    level_up_help = 2520,
    // One-time help shown the first time the robot character moves in its bike form (dng_main.cpp
    // USER_CHARA_ROBO with move_type 3, SAVE_FLAG_ROBO_BIKE_EVENT_SEEN).
    robot_bike_help = 2530,
    // One-time help shown the first time Monica's magic sword gauge holds a charge (dng_main.cpp
    // GetMagicSwordCounterNow, save bit flag 0x3B).
    magic_sword_help = 2540,
    // Spheda: the player stands at the ball and presses the action button (sphida.cpp
    // SPHIDA_EVENT_SHOT).
    spheda_shot = 3000,
    // Spheda: the player presses the menu button near the ball (sphida.cpp SPHIDA_EVENT_NEAR_BALL).
    spheda_menu = 3001,
    // Spheda: as spheda_menu with fewer than two shots of par left (sphida.cpp
    // SPHIDA_EVENT_NEAR_BALL_LAST).
    spheda_menu_last = 3002,
    // Spheda bonus courses: the player presses the menu button away from the ball (sphida.cpp
    // SPHIDA_EVENT_OMAKE_AWAY).
    spheda_menu_away = 3003,
    // Spheda test start: the debug subgame menu enters dungeon map 2 floor 1 with event 4000
    // (mainloop.cpp), and retail bodies load the floor and call setup_spheda.
    spheda_debug_start = 4000,
    // Start of the title-screen bonus dungeon mode (title.cpp InitOmakeEnv, OMAKE_TYPE_DUNGEON), where
    // retail bodies open the Spheda course menu; the *_sfida scripts also _LOAD_SCRIPT into it.
    spheda_bonus_menu = 6000,
    // Loads a Spheda bonus hole after the course menu: *_sfida scripts start it with _LOAD_SCRIPT and
    // retail bodies place the ball and call setup_spheda.
    spheda_bonus_course = 6001,
    // Story event 1 「OPENING PART１」 (opening part 1) on map m07, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_1 = 10010,
    // Story event 2 「OPENING PART１.5」 (opening part 1.5) on map m06, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_2 = 10020,
    // Story event 3 「OPENING PART2」 (opening part 2) on dungeon 1 floor 21, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_3 = 10030,
    // Story event 4 「OPENING PART3」 (opening part 3) on dungeon 1 floor 21, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_4 = 10040,
    // Story event 5 「OPENING PART4」 (opening part 4) on dungeon 1 floor 21, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_5 = 10050,
    // Story event 6 「OPENING PART5」 (opening part 5) on map m06, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_6 = 10060,
    // Story event 7 「OPENING PART6」 (opening part 6) on map d01e01, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_7 = 10070,
    // Story event 50 「プロローグ・モニカの戦い」 (prologue, Monica's battle) on dungeon 1 floor 20, from the debug
    // event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_50 = 10500,
    // Story event 100 「ダンジョンでの最初の通信」 (first call in the dungeon) on dungeon 1 floor 1, from the debug
    // event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_100 = 11000,
    // Story event 101 「回想・主人公とジラードの食事」 (flashback, the hero's meal with Gerard) on map i04h01b, from
    // the debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_101 = 11010,
    // Story event 102 「回想・母の思い出」 (flashback, memories of mother) on map i04h01, from the debug event
    // list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_102 = 11020,
    // Story event 103 「象登場」 (the elephant appears) on dungeon 1 floor 4, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_103 = 11030,
    // Story event 104 「ひとまず退却」 (retreat for now) on dungeon 1 floor 4, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_104 = 11040,
    // Story event 105A 「次の日の朝」 (the next morning) on map m01, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_105a = 11051,
    // Story event 105B 「スターブルのヒント」 (Starbull's hint) on map m01, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_105b = 11052,
    // Story event 105C 「発明完了！」 (invention complete) on map m01, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_105c = 11053,
    // Story event 106 「スティーブ始動」 (Steve starts up) on map m01, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_106 = 11060,
    // Story event 107 「象との再戦」 (rematch with the elephant) on dungeon 1 floor 4, from the debug event
    // list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_107 = 11070,
    // Story event 108A 「象を倒して、ポルカス退却」 (elephant beaten, Flotsam retreats) on dungeon 1 floor 4, from
    // the debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_108a = 11081,
    // Story event 108B 「象に負ける」 (losing to the elephant) on dungeon 1 floor 4, from the debug event
    // list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_108b = 11082,
    // Story event 108C 「象と３度目以降」 (the elephant from the third try on) on map i01h01, from the debug
    // event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_108c = 11083,
    // Story event 109A 「町長の隠し事」 (the mayor's secret) on map m03, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_109a = 11091,
    // Story event 109B 「バース鉄道復活」 (the railway revived) on map i03h01a, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_109b = 11092,
    // Story event 110 「ピエロロボ登場・ボス戦前」 (clown robot appears, before the boss fight) on dungeon 1 floor
    // 7, from the debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event +
    // part.
    story_110 = 11100,
    // Story event 111 「ピエロロボ破壊・ボス戦後」 (clown robot destroyed, after the boss fight) on dungeon 1 floor
    // 7, from the debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event +
    // part.
    story_111 = 11110,
    // Story event 201 「外へ出る」 (going outside) on map s01, from the debug event list event/view_pal.txt,
    // whose program numbers are 10000 + 10 x event + part.
    story_201 = 12010,
    // Story event 202 「列車に乗り込む」 (boarding the train) on map s01, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_202 = 12020,
    // Story event 203 「列車の中での会話」 (talk on the train) on map s08, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_203 = 12030,
    // Story event 204 「ポルカス再び登場」 (Flotsam appears again) on map s03, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_204 = 12040,
    // Story event 205 「列車崩壊」 (the train breaks apart) on map s08, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_205 = 12050,
    // Story event 206 「モニカ登場・ポルカス撃退」 (Monica appears, Flotsam driven off) on map s03, from the debug
    // event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_206 = 12060,
    // Story event 207 「森へ到着・モニカの説明」 (arriving at the forest, Monica's explanation) on map s02, from
    // the debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_207 = 12070,
    // Story event 208 「コビトたちとの出会い」 (meeting the dwarves) on map g01, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_208 = 12080,
    // Story event 209 「コビトたちの事情」 (the dwarves' trouble) on map g01, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_209 = 12090,
    // Story event 215 「小人たち発見・怪魚の沼の幸せボケ」 (dwarves found dazed at the monster-fish swamp) on map s05,
    // from the debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event +
    // part.
    story_215 = 12150,
    // Story event 220 「ノームとの会話」 (talk with the gnome) on map s05, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_220 = 12200,
    // Story event 230 「具合が悪くなったマスターウータン」 (Master Utan falls ill) on map s05, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_230 = 12300,
    // Story event 235 「沼の前にて」 (in front of the swamp) on map s05, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_235 = 12350,
    // Story event 240 「エンペラマーダンを釣り上げる！」 (reeling in the Emperor Mardan) on map s05, from the debug
    // event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_240 = 12400,
    // Story event 245 「マーダン魔法解く」 (Mardan's spell broken) on map s05, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_245 = 12450,
    // Story event 247 「元に戻った３人、一時帰還」 (the three restored, temporary return) on map s05, from the debug
    // event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_247 = 12470,
    // Story event 250 「カーペントリオン発進！」 (the Carpentrion launches) on map g01i07, from the debug event
    // list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_250 = 12500,
    // Story event 253 「岩をどかすマスターウータン」 (Master Utan moves the rock) on map s05, from the debug event
    // list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_253 = 12530,
    // Story event 254A 「ジオラマやろう」 (let's do Georama) on map s02, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_254a = 12541,
    // Story event 254B 「移住のさせ方」 (how to make people move in) on map g01, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_254b = 12542,
    // Story event 255A 「ラフレシア育つその1（種）」 (the Rafflesia grows, part 1 (seed)) on dungeon 2 floor 15,
    // from the debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event +
    // part.
    story_255a = 12551,
    // Story event 255B 「ラフレシア育つその2（太陽のしずく）」 (the Rafflesia grows, part 2 (sun drop)) on dungeon 2
    // floor 15, from the debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x
    // event + part.
    story_255b = 12552,
    // Story event 256 「花の上へ」 (onto the flower) on dungeon 2 floor 15, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_256 = 12560,
    // Story event 257 「七色蝶登場」 (the rainbow butterfly appears) on dungeon 2 floor 15, from the debug
    // event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_257 = 12570,
    // Story event 260 「七色蝶を倒す・ホーリーとの別れ」 (rainbow butterfly beaten, farewell to Hollie) on dungeon 2
    // floor 15, from the debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x
    // event + part.
    story_260 = 12600,
    // Story event 270 「ジオラマ進行１・ジュラク復活」 (Georama progress 1, Jurak revived) on map f01, from the debug
    // event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_270 = 12700,
    // Story event 272 「ジュラクとの会話」 (talk with Jurak) on map f01, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_272 = 12720,
    // Story event 275 「ジオラマ進行２・ジュラクモール開店」 (Georama progress 2, Jurak Mall opens) on map f01, from the
    // debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_275 = 12750,
    // Story event 277 「モンスターへんげのこと」 (about monster transformation) on map f01, from the debug event
    // list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_277 = 12770,
    // Story event 280 「ジオラマ進行３・ジュラク大いに語る」 (Georama progress 3, Jurak talks at length) on map f01, from
    // the debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_280 = 12800,
    // Story event 283 「次のエリアへ→てんびん谷」 (on to the next area, Balance Valley) on map s02, from the debug
    // event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_283 = 12830,
    // Story event 285 「ジオラマ進行１００％ボーナス」 (Georama 100% bonus) on map f01, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_285 = 12850,
    // Story event 300 「てんびん谷到着」 (arriving at Balance Valley) on map s04, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_300 = 13000,
    // Story event 302 「これがてんびん谷」 (this is Balance Valley) on map g02, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_302 = 13020,
    // Story event 304 「リンが危ない」 (Lin is in danger) on map g02i02, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_304 = 13040,
    // Story event 305 「ダック先生を連れてくる」 (bringing the doctor) on map i05h01, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_305 = 13050,
    // Story event 306 「ダンク先生到着・冒険の始まり」 (the doctor arrives, the adventure begins) on map g02i02, from
    // the debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_306 = 13060,
    // Story event 307 「回想・看病をしてくれる母さん」 (flashback, mother nursing the hero) on dungeon 3 floor 2, from
    // the debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_307 = 13070,
    // Story event 308 「ボスがいないボスの谷」 (the boss's valley without its boss) on dungeon 3 floor 8, from the
    // debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_308 = 13080,
    // Story event 312 「ボスの谷・困っているボス」 (the boss's valley, the troubled boss) on dungeon 3 floor 8, from
    // the debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_312 = 13120,
    // Story event 317 「ボスの巣を救う、白き風の花ゲット」 (saving the boss's nest, getting the white wind flower) on
    // dungeon 3 floor 8, from the debug event list event/view_pal.txt, whose program numbers are 10000
    // + 10 x event + part.
    story_317 = 13170,
    // Story event 320 「ラオチャオに初めて会う」 (first meeting with Lao Chao) on map f02i08, from the debug event
    // list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_320 = 13200,
    // Story event 324 「ラオチャオ調理」 (Lao Chao cooks) on map f02i08, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_324 = 13240,
    // Story event 328 「リンの家に戻る」 (back to Lin's house) on map g02i02, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_328 = 13280,
    // Story event 330 「リン復活」 (Lin recovers) on map g02i02, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_330 = 13300,
    // Story event 332 「ヨルダの木到着」 (arriving at the Jorda tree) on dungeon 3 floor 18, from the debug
    // event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_332 = 13320,
    // Story event 336 「リンを連れて木の下へ・メムイータ登場」 (taking Lin under the tree, the Memo Eater appears) on
    // dungeon 3 floor 18, from the debug event list event/view_pal.txt, whose program numbers are
    // 10000 + 10 x event + part.
    story_336 = 13360,
    // Story event 340 「メムイータを倒す」 (Memo Eater beaten) on dungeon 3 floor 18, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_340 = 13400,
    // Story event 344 「ニード町長・釣り大会開催する」 (Mayor Need holds a fishing contest) on map s45, from the debug
    // event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_344 = 13440,
    // Story event 348 「灯台に初めて来ました」 (first visit to the lighthouse) on dungeon 3 floor 23, from the
    // debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_348 = 13480,
    // Story event 352 「星の砂時計を使う」 (using the star hourglass) on dungeon 3 floor 23, from the debug
    // event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_352 = 13520,
    // Story event 356 「ギルトーニ撃退・クレストとの別れ」 (Gaspard driven off, farewell to Crest) on map d03b02, from
    // the debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_356 = 13560,
    // Story event 360 「次のエリアへ出発」 (leaving for the next area) on map s04, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_360 = 13600,
    // Story event 364 「ジオラマ進行１・ラオチャオ飯店復活」 (Georama progress 1, Lao Chao restaurant revived) on map
    // f02, from the debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event
    // + part.
    story_364 = 13640,
    // Story event 368 「ジオラマ進行２・アイテム屋「星の砂時計」」 (Georama progress 2, the Star Hourglass item shop) on map
    // f02, from the debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event
    // + part.
    story_368 = 13680,
    // Story event 372 「ジオラマ進行３・大賢者クレスト復活」 (Georama progress 3, the sage Crest revived) on map f02,
    // from the debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event +
    // part.
    story_372 = 13720,
    // Story event 376 「ジオラマ進行１００％ボーナス」 (Georama 100% bonus) on map f02, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_376 = 13760,
    // Story event 400 「ベニーティオ到着」 (arriving at Veniccio) on map s09, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_400 = 14000,
    // Story event 402 「はじめての海」 (the sea for the first time) on map g03, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_402 = 14020,
    // Story event 404 「出会い・パウとシグー」 (meeting Pau and Shigura) on map s27, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_404 = 14040,
    // Story event 406 「回想・海ってどんなところ？」 (flashback, what is the sea like) on dungeon 4 floor 2, from the
    // debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_406 = 14060,
    // Story event 408 「操られるシグー」 (Shigura under control) on dungeon 4 floor 9, from the debug event
    // list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_408 = 14080,
    // Story event 410 「アイテムなしでシグーのところに来る」 (reaching Shigura without the item) on dungeon 4 floor 9,
    // from the debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event +
    // part.
    story_410 = 14100,
    // Story event 412 「オズモンドとの出会い（E408後）」 (meeting Osmond (after E408)) on map f03i0301, from the
    // debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_412 = 14120,
    // Story event 414 「オズモンドとの出会い・寝ている（E408前）」 (meeting Osmond asleep (before E408)) on map f03i0301,
    // from the debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event +
    // part.
    story_414 = 14140,
    // Story event 416 「シグーへの挑戦」 (challenging Shigura) on dungeon 4 floor 9, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_416 = 14160,
    // Story event 420 「シグー正気に戻る」 (Shigura comes to his senses) on dungeon 4 floor 9, from the debug
    // event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_420 = 14200,
    // Story event 424 「ニード町長・ギョレースカップ開催」 (Mayor Need holds the fish race cup) on map s45, from the
    // debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_424 = 14240,
    // Story event 428 「Ｄｒ・チャップとの出会い」 (meeting Dr. Chapp) on map f03i0302, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_428 = 14280,
    // Story event 432 「シーグラの村到着」 (arriving at the Shingala village) on dungeon 4 floor 20, from the
    // debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_432 = 14320,
    // Story event 436 「シーグラの村到着・ほんやく機なし」 (arriving at the Shingala village without a translator) on
    // dungeon 4 floor 20, from the debug event list event/view_pal.txt, whose program numbers are
    // 10000 + 10 x event + part.
    story_436 = 14360,
    // Story event 438 「シグー復活・その時、村が襲われる！」 (Shigura revived, then the village is attacked) on map s27,
    // from the debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event +
    // part.
    story_438 = 14380,
    // Story event 440 「竜たちを操る男ドクタージャミング」 (Dr. Jaming, the man controlling the dragons) on dungeon 4
    // floor 20, from the debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x
    // event + part.
    story_440 = 14400,
    // Story event 444 「さよならシグー・南へ帰るシーグラたち」 (goodbye Shigura, the Shingala head south) on dungeon 4
    // floor 20, from the debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x
    // event + part.
    story_444 = 14440,
    // Story event 448 「開発開始！時を越える列車」 (development begins, the train that crosses time) on map f03i04,
    // from the debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event +
    // part.
    story_448 = 14480,
    // Story event 452 「次のエリアへ出発・ガンドール火山へ」 (leaving for the next area, the Gundorada volcano) on map
    // s09, from the debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event
    // + part.
    story_452 = 14520,
    // Story event 456 「ジオラマ進行１ルナ研（オズモンド）復活」 (Georama progress 1, Osmond's Luna lab revived) on map
    // f03, from the debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event
    // + part.
    story_456 = 14560,
    // Story event 460 「ジオラマ進行２・Dr.チャップ復活」 (Georama progress 2, Dr. Chapp revived) on map f03, from the
    // debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_460 = 14600,
    // Story event 464 「ジオラマ進行３・センターラボ復活」 (Georama progress 3, the Center Lab revived) on map f03i04,
    // from the debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event +
    // part.
    story_464 = 14640,
    // Story event 468 「ジオラマ進行１００％ボーナス」 (Georama 100% bonus) on map f03i0301, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_468 = 14680,
    // Story event 500 「ヘイムラダ到着・降り注ぐ火炎雨」 (arriving at Heim Rada under raining fire) on map s10, from
    // the debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_500 = 15000,
    // Story event 504 「上空の軍艦・急げユリス」 (the warship overhead, hurry Yuris) on map s41, from the debug
    // event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_504 = 15040,
    // Story event 508 「工場長グイスコン・時限ばくだんをくれる」 (factory chief Gaskon hands over a time bomb) on map f04,
    // from the debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event +
    // part.
    story_508 = 15080,
    // Story event 512 「火口に初めて来ました」 (first visit to the crater) on dungeon 5 floor 16, from the debug
    // event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_512 = 15120,
    // Story event 516 「時限ばくだん使用・軍艦現る」 (using the time bomb, the warship appears) on dungeon 5 floor
    // 16, from the debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event +
    // part.
    story_516 = 15160,
    // Story event 520 「落ちていく軍艦・モニカは！？」 (the warship falls, where is Monica) on dungeon 5 floor 16,
    // from the debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event +
    // part.
    story_520 = 15200,
    // Story event 522 「赤いアトラミリアが青く光る不思議」 (the red Atlamillia glows blue) on map f04, from the debug
    // event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_522 = 15220,
    // Story event 524 「反乱軍指導者ドナ・ホーンと出会う」 (meeting rebel leader Donna Horn) on map s39, from the debug
    // event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_524 = 15240,
    // Story event 526 「溶岩の川・ふたつに割れる」 (the lava river splits in two) on dungeon 5 floor 22, from the
    // debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_526 = 15260,
    // Story event 528 「ギルトーニとの最後の戦い」 (last battle with Gaspard) on dungeon 5 floor 22, from the debug
    // event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_528 = 15280,
    // Story event 532 「ギルトーニの告白」 (Gaspard's confession) on dungeon 5 floor 22, from the debug event
    // list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_532 = 15320,
    // Story event 536 「ギルトーニの最期・回想」 (Gaspard's end, flashback) on dungeon 5 floor 22, from the debug
    // event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_536 = 15360,
    // Story event 540 「反乱軍反撃開始」 (the rebels strike back) on map s30, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_540 = 15400,
    // Story event 548 「ジオラマ進行１大工房＆グイスコン復活」 (Georama progress 1, the great workshop and Gaskon revived)
    // on map f04, from the debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x
    // event + part.
    story_548 = 15480,
    // Story event 552 「ジオラマ進行２・ＯＰルーム・ドナホーン」 (Georama progress 2, the OP room and Donna Horn) on map
    // f04, from the debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event
    // + part.
    story_552 = 15520,
    // Story event 556 「ジオラマ進行３・パズノス要塞完成」 (Georama progress 3, the Paznos fortress complete) on map
    // f04, from the debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event
    // + part.
    story_556 = 15560,
    // Story event 560 「ジオラマ進行１００％・父の剣」 (Georama 100%, father's sword) on map s39, from the debug event
    // list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_560 = 15600,
    // Story event 600 「イクシオン発進・未来への旅立ち」 (the Ixion launches toward the future) on map f03i04, from the
    // debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_600 = 16000,
    // Story event 604 「月花宮殿に来ました」 (arriving at the Moon Flower Palace) on map s33, from the debug
    // event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_604 = 16040,
    // Story event 608 「月花宮殿にてグリフォンに出会う」 (meeting Griffon at the Moon Flower Palace) on dungeon 6 floor
    // 0, from the debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event +
    // part.
    story_608 = 16080,
    // Story event 610 「グリフォンの言い分」 (Griffon's side of the story) on dungeon 6 floor 0, from the debug
    // event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_610 = 16100,
    // Story event 612 「奪われたアトラミリア・宮殿浮上」 (the Atlamillia stolen, the palace rises) on dungeon 6 floor
    // 0, from the debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event +
    // part.
    story_612 = 16120,
    // Story event 616 「一時帰還・エイナの指令」 (temporary return, Eina's orders) on map s70, from the debug event
    // list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_616 = 16160,
    // Story event 640 「ラピスが揃う・クロノユニオン発動」 (the Lapis gathered, the Chronounion activated) on map s66,
    // from the debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event +
    // part.
    story_640 = 16400,
    // Story event 648 「再び・宮殿へ乗り込む」 (storming the palace again) on map s37, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_648 = 16480,
    // Story event 700 「アンドレアの幻（ジオラマ説明）」 (Andrea's vision (Georama explanation)) on map s68, from the
    // debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_700 = 17000,
    // Story event 702 「ホールにかけられたヒントの絵」 (the hint painting in the hall) on map d06e01, from the debug
    // event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_702 = 17020,
    // Story event 704 「栽培園の思い出」 (memories of the cultivation garden) on dungeon 6 floor 11, from the
    // debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_704 = 17040,
    // Story event 708 「思い出・花を摘んだ人」 (memory, the one who picked the flower) on dungeon 6 floor 21, from
    // the debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_708 = 17080,
    // Story event 716 「グリフォン現る」 (Griffon appears) on dungeon 6 floor 27, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_716 = 17160,
    // Story event 720 「戦闘中回想１」 (flashback during battle 1) on dungeon 6 floor 27, from the debug event
    // list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_720 = 17200,
    // Story event 724 「戦闘中回想２」 (flashback during battle 2) on dungeon 6 floor 27, from the debug event
    // list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_724 = 17240,
    // Story event 728 「思い出してサン・ダークエレメント登場」 (remember, Sun; the Dark Element appears) on dungeon 6
    // floor 27, from the debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x
    // event + part.
    story_728 = 17280,
    // Story event 732 「エンディングPART1・星の転生」 (ending part 1, rebirth of the star) on map s67, from the
    // debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_732 = 17320,
    // Story event 736 「エンディングPART2・別れ」 (ending part 2, farewell) on map s66, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_736 = 17360,
    // Unlisted story event 737 on map s66 right after ending part 2 (story_736), reached by _MAP_JUMP;
    // it follows the 10000 + 10 x event numbering of event/view_pal.txt.
    story_737 = 17370,
    // Story event 744 「ジオラマ進行１花壇復活」 (Georama progress 1, the flower beds revived) on map s44, from the
    // debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_744 = 17440,
    // Story event 748 「ジオラマ進行２・花の庭園完成」 (Georama progress 2, the flower garden complete) on map s44,
    // from the debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event +
    // part.
    story_748 = 17480,
    // Story event 800 「ニード町長奮起！バース鉄道拡大計画」 (Mayor Need's railway expansion plan) on map m02, from the
    // debug event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_800 = 18000,
    // Story event 804 「モニカが来た」 (Monica arrives) on dungeon 7 floor 5, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_804 = 18040,
    // Story event 808 「メタルポルカス登場」 (Metal Flotsam appears) on dungeon 7 floor 27, from the debug event
    // list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_808 = 18080,
    // Story event 812 「メタルポルカス撃退」 (Metal Flotsam driven off) on dungeon 7 floor 27, from the debug
    // event list event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_812 = 18120,
    // Story event 816 「魔神登場」 (the genie appears) on dungeon 7 floor 38, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_816 = 18160,
    // Story event 820 「魔神撃退」 (the genie driven off) on dungeon 7 floor 38, from the debug event list
    // event/view_pal.txt, whose program numbers are 10000 + 10 x event + part.
    story_820 = 18200,
    // Boss battle "VS Linda" on dungeon 1 floor 4, listed with type B in the debug event list
    // event/view_pal.txt.
    boss_linda = 21010,
    // Boss battle "VS Halloween" on dungeon 1 floor 7, listed with type B in the debug event list
    // event/view_pal.txt.
    boss_halloween = 21020,
    // Boss battle "VS Flotsam (Subgame)" on map s03, listed with type B in the debug event list
    // event/view_pal.txt.
    boss_flotsam = 22010,
    // Boss battle "VS Butterfly" on dungeon 2 floor 15, listed with type B in the debug event list
    // event/view_pal.txt.
    boss_butterfly = 22030,
    // Boss battle "VS Memo Eater" on dungeon 3 floor 18, listed with type B in the debug event list
    // event/view_pal.txt.
    boss_memo_eater = 23020,
    // Boss battle "VS Gaspard and Fire" on dungeon 3 floor 23, listed with type B in the debug event
    // list event/view_pal.txt.
    boss_gaspard_and_fire = 23030,
    // Boss battle "VS Puppet Shigura" on dungeon 4 floor 9, listed with type B in the debug event list
    // event/view_pal.txt.
    boss_puppet_shigura = 24010,
    // Boss battle "VS Jaming and Shingala" on dungeon 4 floor 20, listed with type B in the debug
    // event list event/view_pal.txt.
    boss_jaming_and_shingala = 24020,
    // Boss battle "VS Death Ark" on dungeon 5 floor 16, listed with type B in the debug event list
    // event/view_pal.txt.
    boss_death_ark = 25010,
    // Boss battle "VS Gaspard 2" on dungeon 5 floor 22, listed with type B in the debug event list
    // event/view_pal.txt.
    boss_gaspard_2 = 25020,
    // Boss battle "VS Monster Gaspard" on dungeon 5 floor 22, listed with type B in the debug event
    // list event/view_pal.txt.
    boss_monster_gaspard = 25030,
    // Boss battle "VS Trentos" on dungeon 2 floor 12, listed with type B in the debug event list
    // event/view_pal.txt.
    boss_trentos = 26010,
    // Boss battle "VS Lapis Garter" on dungeon 3 floor 15, listed with type B in the debug event list
    // event/view_pal.txt.
    boss_lapis_garter = 26020,
    // Boss battle "VS Sea Dragon" on dungeon 4 floor 14, listed with type B in the debug event list
    // event/view_pal.txt.
    boss_sea_dragon = 26030,
    // Boss battle "VS Inferno" on dungeon 5 floor 8, listed with type B in the debug event list
    // event/view_pal.txt.
    boss_inferno = 26040,
    // Boss battle "VS Sirus" on dungeon 6 floor 0, listed with type B in the debug event list
    // event/view_pal.txt.
    boss_sirus = 26050,
    // Boss battle "VS Emperor Griffon 1-3" on dungeon 6 floor 27, listed with type B in the debug
    // event list event/view_pal.txt.
    boss_emperor_griffon = 27010,
    // Boss battle "VS Dream Spiral 1" on dungeon 6 floor 27, listed with type B in the debug event
    // list event/view_pal.txt.
    boss_dream_spiral_1 = 27020,
    // Boss battle "VS Dream Spiral 2" on dungeon 6 floor 27, listed with type B in the debug event
    // list event/view_pal.txt.
    boss_dream_spiral_2 = 27030,
    // Boss battle "VS Dream Spiral 3" on dungeon 6 floor 27, listed with type B in the debug event
    // list event/view_pal.txt.
    boss_dream_spiral_3 = 27040,
    // Boss battle "VS Dream Spiral 4" on dungeon 6 floor 27, listed with type B in the debug event
    // list event/view_pal.txt.
    boss_dream_spiral_4 = 27050,
    // Boss battle "VS Dream Spiral 5" on dungeon 6 floor 27, listed with type B in the debug event
    // list event/view_pal.txt.
    boss_dream_spiral_5 = 27060,
    // Boss battle "VS Dark Element" on dungeon 6 floor 27, listed with type B in the debug event list
    // event/view_pal.txt.
    boss_dark_element = 27070,
    // Boss battle "VS Metal Flotsam" on dungeon 7 floor 27, listed with type B in the debug event list
    // event/view_pal.txt.
    boss_metal_flotsam = 28010,
    // Boss battle "VS Dark Genie" on dungeon 7 floor 38, listed with type B in the debug event list
    // event/view_pal.txt.
    boss_dark_genie = 28020,
};

#define PI 3.1415927

// Alpha blend macro passed to mgCDrawEnv::SetAlpha (mg_drawenv.hpp mgAlphaMacroID); 0 leaves the
// alpha register as it was.
enum alpha_macro {
    MG_ALPHA_MACRO_NONE = 0,
    MG_ALPHA_MACRO_BLEND = 1,
    MG_ALPHA_MACRO_ADD = 2,
    MG_ALPHA_MACRO_SUB = 3,
    MG_ALPHA_MACRO_OPAQUE = 4,
    MG_ALPHA_MACRO_ADD_FIX = 5,
};

// Which Atlamillia frames AtraMiriaOnOff changes: both, or only the first or second.
enum atlamillia_part {
    ATLAMILLIA_BOTH = 0,
    ATLAMILLIA_FIRST = 1,
    ATLAMILLIA_SECOND = 2,
};

// Dungeon floor generation options (AUTOMAP_GEN_FLAG in automap.hpp).
enum automap_gen_flag {
    AUTOMAP_GEN_DUMMY_TREE = 1,
    AUTOMAP_GEN_PRESET_FLOOR = 2,
    AUTOMAP_GEN_DUMMY_MOUNTAIN = 4,
    AUTOMAP_GEN_FIXED_START = 8,
    AUTOMAP_GEN_NO_IN_OUT = 16,
    AUTOMAP_GEN_NO_HEALING = 32,
    AUTOMAP_GEN_FIXED_FLOOR = 64,
};

// Bits of the save data's 8-bit control value CSaveData::bit_ctrl: 1 blocks the world and dungeon
// map moves (menumain.cpp message 0x2A), 2 blocks Georama edit mode (editloop.cpp), 8 hides the
// Atlamillia on loaded characters (charasetup.cpp, _LOAD_CHARA), 0x10 opens the dungeon floor map
// in place of the world map move (SetCommonMenuModeID).
enum bit_ctrl {
    BIT_CTRL_NO_MAP_MOVE = 1,
    BIT_CTRL_NO_EDIT = 2,
    BIT_CTRL_HIDE_ATLAMILLIA = 8,
    BIT_CTRL_DUNGEON_MAP_MOVE = 16,
};

// Camera _RESET_CAMERA puts back on character 0: the active camera at its current distance and
// angle, or the previous camera behind the character's facing.
enum camera_reset_mode {
    CAMERA_RESET_ACTIVE = 0,
    CAMERA_RESET_BEHIND = 1,
};

// Ways the player-controlled camera is kept from turning, bits of CCameraControl::rot_cancel
// (cameracontrol.hpp CameraRotCancel).
enum camera_rot_cancel {
    CAMERA_ROT_CANCEL_BUTTON = 1,
    CAMERA_ROT_CANCEL_ANALOG = 2,
    CAMERA_ROT_CANCEL_ROT_BACK = 64,
    CAMERA_ROT_CANCEL_AUTO_MOVE = 128,
};

// Playback flags of a character motion (CharaMotionFlag in character.hpp, as
// CCharacter2::SetMotion takes them).
enum chara_motion_flag {
    CHARA_MOTION_PAUSE = 1,
    CHARA_MOTION_HOLD = 2,
    CHARA_MOTION_RESTART = 4,
};

// Status condition bits of a party character (userdata.hpp CHARA_STATUS_ATTR).
enum chara_status_attr {
    CHARA_STATUS_POISON = 1,
    CHARA_STATUS_SLOW = 2,
    CHARA_STATUS_UNK_4 = 4,
    CHARA_STATUS_UNK_8 = 8,
    CHARA_STATUS_POWER = 16,
    CHARA_STATUS_UNK_20 = 32,
    CHARA_STATUS_UNK_40 = 64,
};

// Button _CHECK_BUTTON tests: confirm and cancel follow the language's circle and cross layout.
enum check_button {
    CHECK_BUTTON_CANCEL = 0,
    CHECK_BUTTON_CONFIRM = 1,
    CHECK_BUTTON_SQUARE = 2,
    CHECK_BUTTON_SELECT = 3,
};

// How a cross-fade takes the captured screen away (CrossFadeType in effectlist.hpp); any value
// other than CROSS_FADE_WIPE dissolves.
enum cross_fade_type {
    CROSS_FADE_DISSOLVE = 0,
    CROSS_FADE_WIPE = 1,
};

// Bits of DNG_BATTLE_AREA::floor_status, the current floor's seal restrictions (scenesnd.hpp
// DNG_FLOOR_DISABLE_*).
enum dng_floor_status {
    DNG_FLOOR_DISABLE_MAX = 1,
    DNG_FLOOR_DISABLE_MONICA = 2,
    DNG_FLOOR_DISABLE_ITEMS = 4,
};

// Dungeon systems an event pauses, bits of DNG_BATTLE_AREA::pause_flag (scenesnd.hpp DNG_PAUSE_*);
// 8 is unnamed in the decomp and makes DngStep return right after monster AI, skipping effects and
// scene characters.
enum dng_pause {
    DNG_PAUSE_MONSTER_AI = 1,
    DNG_PAUSE_PLAYER_STEP = 2,
    DNG_PAUSE_PLAYER_CONTROL = 4,
    DNG_PAUSE_SCENE_STEP = 8,
    DNG_PAUSE_MONSTER_DRAW = 16,
    DNG_PAUSE_SKY_DRAW = 32,
    DNG_PAUSE_MAP_DRAW = 128,
    DNG_PAUSE_MINIMAP = 256,
    DNG_PAUSE_PICKUPS = 512,
    DNG_PAUSE_PLAYER_STATUS = 1024,
    DNG_PAUSE_EXIT_HEAL = 2048,
    DNG_PAUSE_ENEMY_STEP = 4096,
    DNG_PAUSE_WEAPON_DRAW = 8192,
    DNG_PAUSE_BATTLE_MUSIC = 16384,
    DNG_PAUSE_PAD_RESET = 32768,
    DNG_PAUSE_MONSTER_NAMES = 65536,
};

// Dungeon weather (scenesnd.hpp DNG_WEATHER).
enum dng_weather {
    DNG_WEATHER_NORMAL = 0,
    DNG_WEATHER_RAIN = 2,
};

// Command ScriptDebugCommand runs; 0 adds 512 HP to the dungeon party character.
enum dungeon_debug_command {
    DNG_DEBUG_ADD_HP = 0,
};

// Place GetDungeonEventPoint finds on the dungeon floor (DungeonEventPointKind in dng_event.hpp).
enum dungeon_event_point_kind {
    DUNGEON_EVENT_POINT_PLAYER = 0,
    DUNGEON_EVENT_POINT_TREASURE_BOX = 1,
    DUNGEON_EVENT_POINT_WAY_20 = 2,
    DUNGEON_EVENT_POINT_WAY_24 = 3,
};

// Command of _SET_FUNC_ETC: 0 sets map jump flag 0x80 and requests EVENT_REQUEST_RESTART_EDIT, 1
// requests EVENT_REQUEST_RESET_EDIT.
enum edit_event_command {
    EDIT_EVENT_RESTART = 0,
    EDIT_EVENT_RESET = 1,
};

// Field of the scene's CSceneEventData that _GET_EVENT_DATA and _SET_EVENT_DATA read or write.
enum event_data_field {
    EVENT_DATA_FLAG = 0,
    EVENT_DATA_EVENT_NO = 1,
    EVENT_DATA_POINT_NO = 2,
    EVENT_DATA_ARG1 = 3,
    EVENT_DATA_ARG2 = 4,
    EVENT_DATA_ARG3 = 5,
    EVENT_DATA_CHECK_TYPE = 6,
    EVENT_DATA_MAP_EVENT_NO = 7,
    EVENT_DATA_POS_YAW = 8,
    EVENT_DATA_PARTS_NO = 9,
    EVENT_DATA_POSITION = 10,
    EVENT_DATA_ROTATION = 11,
    EVENT_DATA_SCALE = 12,
    EVENT_DATA_CHARA_SLOT = 13,
    EVENT_DATA_CHARA_NO = 14,
    EVENT_DATA_GAMEOBJ_NO = 15,
};

// Draw pass of an event sprite (eventsprite.hpp EVENT_SPRITE2_DRAW).
enum event_sprite_draw {
    EVENT_SPRITE2_DRAW_OFF = 0,
    EVENT_SPRITE2_DRAW_NORMAL = 1,
    EVENT_SPRITE2_DRAW_FIRST = 2,
};

// Where an event sprite lives (eventsprite.hpp EVENT_SPRITE2_TYPE).
enum event_sprite_type {
    EVENT_SPRITE2_TYPE_SCREEN = 0,
    EVENT_SPRITE2_TYPE_WORLD = 1,
};

// Command of _GET_FISHINGTOURNAMENT_ETC.
enum fish_tour_get_command {
    FISH_TOUR_GET_RECORDS = 0,
    FISH_TOUR_GET_PRIZE = 1,
    FISH_TOUR_GET_ENTRY_REMAIN = 2,
    FISH_TOUR_GET_TOP_WEIGHT = 3,
    FISH_TOUR_GET_RANK = 4,
};

// Command of _SET_FISHINGTOURNAMENT_ETC.
enum fish_tour_set_command {
    FISH_TOUR_SET_RANK = 0,
    FISH_TOUR_SET_RESET_RECORD = 1,
    FISH_TOUR_SET_LOAD_PRIZE = 2,
    FISH_TOUR_SET_COUNT = 3,
};

// Field of a dungeon floor's save record (DNG_FLOOR_SAVE) that _GET_FLOOR_INFO and _SET_FLOOR_INFO
// address.
enum floor_info_field {
    FLOOR_INFO_UNK_0 = 0,
    FLOOR_INFO_FAST_DESTROY_TIME = 1,
    FLOOR_INFO_UNK_8 = 2,
    FLOOR_INFO_UNK_A = 3,
    FLOOR_INFO_SPHEDA_CLEAR = 4,
    FLOOR_INFO_FLAG = 5,
    FLOOR_INFO_KILL_COUNT = 6,
    FLOOR_INFO_VISIT_COUNT = 7,
};

// Command of _GET_GYORACE_ETC.
enum gyorace_get_command {
    GYORACE_GET_AQUARIUM_NO = 0,
    GYORACE_GET_RANKING = 1,
    GYORACE_GET_CLASS = 2,
    GYORACE_GET_RACE_NO = 3,
    GYORACE_GET_PRIZE = 4,
    GYORACE_GET_TOUR_COUNT = 5,
};

// Command of _SET_GYORACE_ETC.
enum gyorace_set_command {
    GYORACE_SET_AQUARIUM_NO = 0,
    GYORACE_SET_RANKING = 1,
    GYORACE_SET_CLASS = 2,
    GYORACE_SET_RACE_NO = 3,
    GYORACE_SET_ADD_TOUR_COUNT = 4,
    GYORACE_SET_FISH_NAME = 5,
    GYORACE_SET_RACE_TIME = 6,
    GYORACE_SET_LOAD_PRIZE = 7,
};

// How CBattleCharaInfo::AddHp_Rate scales the change: by max or current HP, optionally never
// dropping below 1.
enum hp_rate_kind {
    HP_RATE_MAX = 0,
    HP_RATE_NOW = 1,
    HP_RATE_MAX_KEEP_1 = 2,
    HP_RATE_NOW_KEEP_1 = 3,
};

// Which lip sync motion _TRG_PAKU_MOTION starts: the talk motion, the idle motion once the talk
// motion has ended, or the idle motion at once.
enum lip_sync_motion_mode {
    LIP_SYNC_TALK = 0,
    LIP_SYNC_IDLE_WHEN_DONE = 1,
    LIP_SYNC_IDLE = 2,
};

// Command of _GET_MENU_ETC.
enum menu_get_command {
    MENU_GET_SPHEDA_CLEARS = 0,
    MENU_GET_SQUARE_EVENT = 1,
    MENU_GET_FISH_RECORD = 2,
    MENU_GET_SCOOP_KNOWN = 3,
    MENU_GET_SCOOP_OBTAINED = 4,
    MENU_GET_SCOOP_TOTAL = 5,
    MENU_GET_DONY_SHOP = 6,
    MENU_GET_DOWNLOAD_STEP = 7,
    MENU_GET_DOWNLOAD_DONE = 8,
};

// Menu an event asks the game to open, MenuArg.open_type (menumain.hpp MenuOpenType).
enum menu_open_type {
    MENU_OPEN_MAIN_TOWN = 0,
    MENU_OPEN_MAIN_DUNGEON = 1,
    MENU_OPEN_GEORAMA = 2,
    MENU_OPEN_DNG_TREE_MAP = 3,
    MENU_OPEN_CHARA_CHANGE = 4,
    MENU_OPEN_NAME_REGIST = 5,
    MENU_OPEN_SHOP = 6,
    MENU_OPEN_SAVE = 7,
    MENU_OPEN_TITLE_SAVE = 8,
    MENU_OPEN_USE_ITEM = 9,
    MENU_OPEN_INVENT = 10,
    MENU_OPEN_CHAPTER = 11,
    MENU_OPEN_REMOVAL = 12,
    MENU_OPEN_WORLD_MOVE = 13,
    MENU_OPEN_CHARA_CHANGE_DUNGEON = 14,
    MENU_OPEN_GYORACE_FISH_SEL = 15,
    MENU_OPEN_MAIN_TOWN_ITEM_OVER = 16,
    MENU_OPEN_MAIN_DUNGEON_ITEM_OVER = 17,
    MENU_OPEN_OPTION = 18,
    MENU_OPEN_WORLD_MOVE_B = 19,
    MENU_OPEN_COSTUME = 20,
    MENU_OPEN_MAIN_CHARA_BG_DUNGEON = 21,
    MENU_OPEN_USE_ITEM_B = 22,
    MENU_OPEN_GYORACE = 23,
    MENU_OPEN_SPHIDA = 24,
    MENU_OPEN_MONSTER_BOOK = 25,
    MENU_OPEN_SUBGAME_SAVE = 26,
    MENU_OPEN_TITLE_SUBGAME_SAVE = 27,
    MENU_OPEN_SPHIDA_SCORE_VIEW = 28,
    MENU_OPEN_MAIN_CHARA_BG = 29,
};

// Command of _SET_MENU_ETC; 2 and 6 do nothing.
enum menu_set_command {
    MENU_SET_KEYWORD = 0,
    MENU_SET_FINISH_TOUR = 1,
    MENU_SET_DOWNLOAD_SHOW = 3,
    MENU_SET_LOADING_RESET = 4,
    MENU_SET_DOWNLOAD_START = 5,
    MENU_SET_DOWNLOAD_CLEAR = 7,
};

// Value _GET_MES_ETC reads: the window's line indent or cursor centring flag, the fish count of
// the main aquarium tank, or two fields of a villager's info.
enum mes_info_query {
    MES_INFO_LINE_INDENT = 0,
    MES_INFO_CURSOR_CENTERING = 1,
    MES_INFO_AQUARIUM_FISH_NUM = 2,
    MES_INFO_VILLAGER = 3,
};

// Message window setting _SET_MES_ETC changes, each switch case naming the ClsMes field it writes.
enum mes_option {
    MES_OPT_LINE_INDENT = 0,
    MES_OPT_CURSOR_CENTERING = 1,
    MES_OPT_HALF_FONT_WIDTH = 2,
    MES_OPT_AQUARIUM_FISH = 3,
    MES_OPT_FIXED_FRAME = 4,
    MES_OPT_LINE_SHADE = 5,
    MES_OPT_LINE_ALPHA = 6,
    MES_OPT_TEXT_COLOR = 7,
    MES_OPT_OUTLINE = 8,
    MES_OPT_TEXT_OFFSET = 9,
    MES_OPT_ALPHA = 10,
};

// Settings ClsMes::Preset applies to a message window (MesPreset in nd_meswin.hpp).
enum mes_preset {
    MES_PRESET_FUKIDASHI = 0,
    MES_PRESET_SYSTEM = 1,
    MES_PRESET_PLAIN = 2,
    MES_PRESET_NPC_NAME = 3,
    MES_PRESET_WINDOW = 4,
    MES_PRESET_SMALL_FUKIDASHI = 5,
    MES_PRESET_WINDOW_REVEAL = 6,
};

// Message buffer _MES_SET_BUFF points a window at: 0 calls ClsMes::SetBuff with the sysmes.mes
// buffer, any other value SetBuff_system with system.mes.
enum mes_source {
    MES_SOURCE_SYSMES = 0,
    MES_SOURCE_SYSTEM = 1,
};

// Frame a message window draws around its text (MesWindowMode in nd_meswin.hpp).
enum mes_window_mode {
    MES_WIN_NONE = 0,
    MES_WIN_FUKIDASHI = 1,
    MES_WIN_HELP = 2,
    MES_WIN_FLOATING = 3,
    MES_WIN_VERSATILE_1 = 4,
    MES_WIN_YESNO = 5,
    MES_WIN_VERSATILE_3 = 6,
    MES_WIN_BOTTOM = 7,
    MES_WIN_VERSATILE_4 = 8,
    MES_WIN_DQ_FUKIDASHI = 9,
    MES_WIN_DQ_FUKIDASHI_2 = 10,
    MES_WIN_CENTRE = 11,
    MES_WIN_PLAIN = 12,
};

// Sound _MES_SE_PLAY plays: 0 and 1 play system sound 25 (scripts use them for confirm and cancel
// on a message), 2 SYSTEM_SE_CURSOR, 3 SYSTEM_SE_DECIDE.
enum message_se {
    MES_SE_CONFIRM = 0,
    MES_SE_CANCEL = 1,
    MES_SE_CURSOR = 2,
    MES_SE_DECIDE = 3,
};

// Mode of _SET_MOVIE_CC.
enum movie_caption_mode {
    MOVIE_CAPTION_ENABLE = 0,
    MOVIE_CAPTION_SET = 1,
};

// Which model _LOAD_CHARA_NPC loads: 0 the character model (NPC_MODEL_PATH_CHARA), any other value
// the event train model (NPC_MODEL_PATH_EVENT_TRAIN).
enum npc_model_set {
    NPC_MODEL_CHARA = 0,
    NPC_MODEL_EVENT_TRAIN = 1,
};

// Party character status bits CUserDataManager::SetPartyCharaStatus stores: 1 in the party now, 2
// joined and waiting in town, 4 living in a house, and 128 alone records the character as joined
// with full ability points.
enum npc_status {
    NPC_STATUS_IN_PARTY = 1,
    NPC_STATUS_JOINED = 2,
    NPC_STATUS_RESIDENT = 4,
    NPC_STATUS_JOIN = 128,
};

// Controller button bits as CGamePad reports them (gamepad.hpp PadButton).
enum pad_button {
    PAD_L2 = 1,
    PAD_R2 = 2,
    PAD_L1 = 4,
    PAD_R1 = 8,
    PAD_TRIANGLE = 16,
    PAD_CIRCLE = 32,
    PAD_CROSS = 64,
    PAD_SQUARE = 128,
    PAD_SELECT = 256,
    PAD_L3 = 512,
    PAD_R3 = 1024,
    PAD_START = 2048,
    PAD_UP = 4096,
    PAD_RIGHT = 8192,
    PAD_DOWN = 16384,
    PAD_LEFT = 32768,
};

// What a photo's subject number means in CheckInventPhoto: USER_PICTURE_INFO neta_id, npc_no or
// monster_no.
enum photo_subject_kind {
    PHOTO_SUBJECT_IDEA = 0,
    PHOTO_SUBJECT_NPC = 2,
    PHOTO_SUBJECT_MONSTER = 3,
};

// Kind of pickup that flies to the player (dng_object.hpp PULL_ITEM_TYPE).
enum pull_item_type {
    PULL_ITEM_MONEY = 0,
    PULL_ITEM_WEAPON_EXP = 1,
    PULL_ITEM_GATE_KEY = 2,
    PULL_ITEM_MONEY_LARGE = 3,
    PULL_ITEM_ITEM = 4,
    PULL_ITEM_ITEM2 = 5,
    PULL_ITEM_BADGE = 6,
    PULL_ITEM_STOLEN = 7,
};

// Command of _GET_QUEST_ETC.
enum quest_get_command {
    QUEST_GET_STATUS = 0,
};

// Command of _SET_QUEST_ETC: 0 calls QuestRequestSetFlag, 1 QuestRequestClear.
enum quest_set_command {
    QUEST_SET_ACCEPTED = 0,
    QUEST_SET_CLEARED = 1,
};

// Save bit flags the engine itself names (SAVE_BIT_FLAG in savedata.hpp, kBitFlagGekkaView in
// editmenu.cpp, bit_flag_tour_cycled in menuaqua.cpp); other flag numbers stay plain.
enum flag_engine {
    SAVE_FLAG_ROBO_BIKE_EVENT_SEEN = 53,
    SAVE_FLAG_TOUR_CYCLED = 63,
    SAVE_FLAG_ITEM_BOARD_EXPANDED = 254,
    SAVE_FLAG_TOURNAMENT_STARTED = 344,
    SAVE_FLAG_TOURNAMENT_CYCLE = 424,
    SAVE_FLAG_EDIT_BLOCKED = 520,
    SAVE_FLAG_GEKKA_VIEW = 702,
    SAVE_FLAG_COSTUME_UNLOCK = 799,
};

// Command of _GET_SAVEDATA_ETC.
enum save_get_command {
    SAVE_GET_PROGRESS = 0,
    SAVE_GET_BADGE = 1,
    SAVE_GET_ELECTRIC_FISH = 2,
    SAVE_GET_BAIT = 3,
    SAVE_GET_PLAY_TIME = 4,
    SAVE_GET_CAPTION_OFF = 5,
    SAVE_GET_TOUR_TYPE = 6,
};

// Command of _SET_SAVEDATA_ETC.
enum save_set_command {
    SAVE_SET_PROGRESS = 0,
    SAVE_SET_BADGE = 1,
    SAVE_SET_REPAIR_ALL = 2,
    SAVE_SET_SKIP_LOAD_BGM = 3,
    SAVE_SET_DELETE_ELECTRIC_FISH = 4,
};

// Scene type of a character slot as CScene::SetType stores it: colprim.cpp lets type 1 be hit only
// by attacks aimed at the player and type 3 only by attacks aimed at monsters, and dng_main.cpp
// draws type 4 in the later pass.
enum scene_chara_type {
    SCENE_CHARA_TYPE_PLAYER = 1,
    SCENE_CHARA_TYPE_MONSTER = 3,
    SCENE_CHARA_TYPE_LATE_DRAW = 4,
};

// How the event camera turns while it orbits an object (sceneseq.hpp SceneCmrSyncMode).
enum scene_cmr_sync_mode {
    SCENE_CMR_SYNC_FIXED = 0,
    SCENE_CMR_SYNC_YAW = 1,
    SCENE_CMR_SYNC_FRAME = 2,
};

// List of scene slots a call addresses (scene.hpp SCENE_DATA_KIND).
enum scene_data_kind {
    SCENE_DATA_CHARA = 1,
    SCENE_DATA_MAP = 2,
    SCENE_DATA_MESSAGE = 3,
    SCENE_DATA_CAMERA = 4,
    SCENE_DATA_SKY = 5,
    SCENE_DATA_GAMEOBJ = 6,
    SCENE_DATA_EFFECT = 7,
};

// Speed curve of an eased camera or actor move or turn (sceneseq.hpp SceneSeqEase).
enum scene_seq_ease {
    SCENE_SEQ_EASE_IN_OUT = 0,
    SCENE_SEQ_EASE_IN = 1,
    SCENE_SEQ_EASE_OUT = 2,
};

// Status bits of a scene slot: the shared slot bits (scene.hpp SCENE_DATA_STATUS) and the
// character slot bits (scenevillager.hpp SCENE_CHARA_STATUS).
enum scene_status {
    SCENE_DATA_LOADED = 1,
    SCENE_DATA_ACTIVE = 2,
    SCENE_DATA_ASSIGNED = 4,
    SCENE_CHARA_NO_SHADOW = 8,
    SCENE_CHARA_HIDE = 16,
    SCENE_CHARA_EXCLAMATION = 32,
    SCENE_CHARA_NO_LIGHTING = 64,
    SCENE_CHARA_NO_FADE = 128,
    SCENE_CHARA_NO_DIST = 256,
    SCENE_CHARA_NO_MODEL = 512,
};

// Scene character slots the engine always fills the same way (charasetup.cpp, LoadGameObject).
// 1 to 5 hold the controlled character's weapons or the Ridepod's parts; villagers take 8 to 23
// and 24 to 31 in an order that changes with the story, and scripts reuse those slots for their
// own characters, so those have no names.
enum chara_slot_engine {
    CHARA_PLAYER = 0,
    CHARA_TIME_GATE_MARKER = 120,
    CHARA_TIME_GATE_BASE = 121,
    CHARA_SAVE_POINT = 122,
    CHARA_SAVE_BOOK = 123,
};

// Scene sound bank whose id _GET_SND_ID reads: CScene se_env_id, se_base_id (1 to 3),
// se_battle_id, or a sound source through GetSeSrcID.
enum se_bank {
    SE_BANK_ENV = 0,
    SE_BANK_BASE = 1,
    SE_BANK_BASE_2 = 2,
    SE_BANK_BASE_3 = 3,
    SE_BANK_BATTLE = 4,
    SE_BANK_SOURCE = 5,
};

// Sound port of the sound manager (snd_mngr.hpp sndPORT).
enum snd_port {
    SND_PORT_BGM = 0,
    SND_PORT_OB = 1,
    SND_PORT_ENV = 2,
    SND_PORT_BASE = 3,
    SND_PORT_EVENT = 4,
    SND_PORT_ENEMY = 5,
    SND_PORT_SYSTEM = 6,
    SND_PORT_CHARA = 7,
    SND_PORT_MENU = 8,
    SND_PORT_BATTLE = 9,
    SND_PORT_BGM2 = 11,
};

// Command of _STOPWATCH.
enum stopwatch_command {
    STOPWATCH_START = 0,
    STOPWATCH_STOP = 1,
    STOPWATCH_LIMIT = 2,
    STOPWATCH_LAYOUT = 3,
};

// Minigame that sgInitSubGame starts (subgame.hpp SUBGAME_TYPE).
enum subgame_type {
    SUBGAME_NONE = 0,
    SUBGAME_FISHING = 1,
    SUBGAME_GYORACE = 2,
    SUBGAME_BUGGY = 3,
    SUBGAME_UNUSED = 4,
};

// State of a treasure box slot (TreasureBoxState in dng_event.hpp); 2 keeps the box but drops it
// from the mini map and the nearest-box search, so it can no longer be opened.
enum treasure_box_state {
    TREASURE_BOX_STATE_NONE = 0,
    TREASURE_BOX_STATE_UNOPENED = 1,
    TREASURE_BOX_STATE_OPENED = 2,
};

// Scene memory stack a load allocates from (CScene::stack[], bound in editloop.cpp for town and
// dng_main.cpp for dungeons): 0 player characters, 1 main map data, 2 main map characters, 3 sub
// data, 4 sub map characters, 5 to 8 the four event buffers. Clearing a stack also clears every
// stack above it.
enum memory_stack {
    MEMORY_STACK_PLAYER = 0,
    MEMORY_STACK_MAIN_DATA = 1,
    MEMORY_STACK_MAIN_CHARA = 2,
    MEMORY_STACK_SUB_DATA = 3,
    MEMORY_STACK_SUB_CHARA = 4,
    MEMORY_STACK_EVENT = 5,
    MEMORY_STACK_EVENT_2 = 6,
    MEMORY_STACK_EVENT_3 = 7,
    MEMORY_STACK_EVENT_4 = 8,
};

// Equipment slot of a party character (userdata.hpp CHARA_DATA::equip[5]): the two weapons, then
// the hat, shoes and body costume. Extends action.h's existing equip_slot with slots 3 and 4.
enum equip_slot {
    EQUIP_MAIN_WEAPON = 0,
    EQUIP_SUB_WEAPON = 1,
    EQUIP_HAT = 2,
    EQUIP_SHOES = 3,
    EQUIP_BODY = 4,
};

// Playable character a party-data native addresses (userdata.hpp USER_CHARA): Max, Monica, the
// Ridepod, or Monica's monster form. Natives that read equipment accept only 0 and 1.
enum user_chara {
    USER_CHARA_MAX = 0,
    USER_CHARA_MONICA = 1,
    USER_CHARA_ROBO = 2,
    USER_CHARA_MONSTER = 3,
};

// Stream number the stream natives take; the engine discards it and always uses its event stream,
// number 1 (event_func.cpp event_stream), which is the only value scripts pass.
enum stream_channel {
    STREAM_CHANNEL_EVENT = 1,
};

// Horizontal position on the 512 x 416 screen that full-screen event sprites are laid out in
// (mglib.cpp GetScreenSize; set_sprite_size stretches 416 to fill the 480-line PAL screen).
enum screen_x {
    SCREEN_WIDTH = 512,
};

// Vertical position on that 512 x 416 screen.
enum screen_y {
    SCREEN_HEIGHT = 416,
};

// Colour and alpha level of an event sprite, where 128 is 1.0: full alpha, and a colour that
// leaves the texture as it is (eventsprite.cpp CEventSprite2 defaults).
enum color_level {
    COLOR_FULL = 128,
};

// Returns the bitmask of controller buttons currently held down.
// Original name: _GET_PADON.
extern int get_pad_held(void) NATIVE(0);

// Returns the bitmask of controller buttons pressed this frame.
// Original name: _GET_PADDOWN.
extern int get_pad_pressed(void) NATIVE(1);

// Returns the bitmask of controller buttons released this frame.
// Original name: _GET_PADUP.
extern int get_pad_released(void) NATIVE(2);

// Reads the analog sticks as floats, each the raw stick value divided by 128. Writes as many of
// left x, left y, right x, right y as arguments are given.
// Original name: _GET_APAD.
extern void get_analog_sticks(float *left_x, float *left_y, OPTIONAL(float *right_x, float *right_y)) NATIVE(3);

// Requests a move into an interior map once the event ends. jump_point is the entry point (-1 for
// the default), event_no is the event started on arrival (default 100) and entrance picks the
// interior's entrance (default 0).
// Original name: _GOTO_INTERIOR.
extern void go_to_interior(int jump_point, string map_name, OPTIONAL(enum program event_no, int entrance)) NATIVE(6);

// Requests a move from an interior back out to an outdoor map. jump_point is the entry point (-1
// for the default) and event_no is the event started on arrival (default 100).
// Original name: _GOTO_OUTSIDE.
extern void go_to_outside(int jump_point, string map_name, OPTIONAL(enum program event_no)) NATIVE(7);

// Resets the event system state (sequences, sprites, effects, lip sync, world coordinate) and
// turns off camera follow on the active camera. Called at the start of an event.
// Original name: _INITIALIZE.
extern void init_event(void) NATIVE(8);

// Loads a character model from a pack file into a scene character slot, using memory stack
// stack_no. cfg_name names the model inside the pack (such as "info.cfg"); a non-zero no_outline
// loads the model without its outline. A single argument is instead an id of a stored argument
// list holding the first four values.
// Original name: _LOAD_CHARA.
extern void load_chara(enum memory_stack stack_no, chara_pack pack_path, string cfg_name, chara_slot chara, OPTIONAL(int no_outline)) NATIVE(9);

// With one argument, makes that scene character active so it is stepped and drawn. With two
// arguments, the first is on/off and the second the character: 0 deactivates it, non-zero
// activates it.
// Original name: _CHARA_ACTIVE.
extern void set_chara_active(int on_or_chara, OPTIONAL(int chara)) NATIVE(10);

// Empties scene memory stack stack_no and every stack above it, releasing the buffers of the
// higher ones.
// Original name: _CLEAR_STACK.
extern void clear_memory_stack(enum memory_stack stack_no) NATIVE(11);

// Gives memory stack stack_no the free space left in the stack below it, locking the lower stack.
// Stacks 0 and 1 are ignored.
// Original name: _ASSIGN_STACK.
extern void assign_memory_stack(enum memory_stack stack_no) NATIVE(12);

// Sets or clears a bit flag in the save data. value 0 clears it, non-zero sets it.
// Original name: _SET_FLAG.
extern void set_flag(flag flag, int value) NATIVE(13);

// Returns a bit flag from the save data (0 or 1).
// Original name: _GET_FLAG.
extern int get_flag(flag flag) NATIVE(14);

// Stores a 16-bit counter in the save data at index.
// Original name: _SET_CNT.
extern void set_counter(int index, int value) NATIVE(15);

// Returns the 16-bit save data counter at index.
// Original name: _GET_CNT.
extern int get_counter(int index) NATIVE(16);

// Sets the directory that file names given to later load calls are relative to. An empty string,
// "DEFAULT" or no argument resets it to the default.
// Original name: _SET_CURRENT_DIR.
extern void set_current_dir(OPTIONAL(string dir)) NATIVE(17);

// Removes a scene character and clears any event object handles that refer to it. Its texture
// block is freed too unless delete_texture is 0 (default 1).
// Original name: _DELETE_CHARA.
extern void delete_chara(chara_slot chara, OPTIONAL(int delete_texture)) NATIVE(18);

// Loads an extra motion pack into an already loaded scene character, using memory stack stack_no.
// cfg_name names the entry inside the pack (such as "info.cfg"). A single argument is instead an
// id of a stored argument list.
// Original name: _LOAD_MOTION.
extern void load_chara_motion(enum memory_stack stack_no, chara_pack pack_path, string cfg_name, chara_slot chara) NATIVE(19);

// Requests a jump to another map once the event ends. map is a map number or a map name string;
// jump_point is the entry point (-1 for the default) and event_no is the event started on arrival
// (default 100).
// Original name: _MAP_JUMP.
extern void jump_to_map(int jump_point, map_id map, OPTIONAL(enum program event_no)) NATIVE(20);

// Starts the event rain effect when on is non-zero, stops it when 0.
// Original name: _SET_RAIN.
extern void set_rain(int on) NATIVE(21);

// Removes the extra motions that were added to a scene character with load_chara_motion.
// Original name: _DEL_EXT_MOTION.
extern void delete_chara_ext_motion(chara_slot chara) NATIVE(22);

// Does nothing in the retail game.
// Original name: _SET_MARKER.
extern void set_marker(int value) NATIVE(23);

// Sets the origin and Y rotation (radians) of the event's world coordinate frame, which event
// positions and the camera are placed relative to. Pass all four values, or none to reset the
// frame to the world origin.
// Original name: _SET_WORLD_COORD.
extern void set_world_coord(OPTIONAL(float x, float y, float z, float rot_y)) NATIVE(24);

// Ends the event: applies a pending world coordinate to the camera and resets sequences, sprites,
// hit effects, lip sync and the world coordinate.
// Original name: _FINISH.
extern void finish_event(void) NATIVE(25);

// Finds a place on the dungeon floor and writes its position and Y rotation. kind 0 (the default)
// is the grid cell the player stands in, 1 the treasure box the player is at, 2 the first way32 to
// way35 map part, 3 the first way36 to way51 map part.
// Original name: _GET_DUN_WORLD_COORD.
extern void get_dungeon_event_point(float *x, float *y, float *z, float *rot_y, OPTIONAL(enum dungeon_event_point_kind kind)) NATIVE(26);

// Loads an .img texture file into event texture block image_no, using memory stack stack_no. With
// three arguments the image is also registered as event sprite image image_no; with a fourth
// argument the texture block number is written into it instead.
// Original name: _LOAD_IMG.
extern void load_image(enum memory_stack stack_no, string file_name, int image_no, OPTIONAL(int *block)) NATIVE(27);

// Frees the event texture block image_no loaded with load_image.
// Original name: _DEL_IMG.
extern void delete_image(int image_no) NATIVE(28);

// Reads two integers and does nothing with them in the retail game.
// Original name: _SET_DNG_MAP.
extern void set_dungeon_map(int value_a, int value_b) NATIVE(29);

// Loads an item's model into a scene character slot, using memory stack stack_no. cfg_name names
// the model inside the item's pack; variant picks which of the item's model files to use (default
// 0).
// Original name: _LOAD_ITEM.
extern void load_item_model(enum memory_stack stack_no, item_id item, string cfg_name, int chara, OPTIONAL(int variant)) NATIVE(30);

// Opens the use-item menu after the script yields. The following integers are passed to the menu
// as parameters (item numbers it accepts). When the menu closes, the chosen item is written into
// used_item; scripts treat 0 or less as cancelled.
// Original name: _GOTO_USE_ITEM.
extern void open_use_item_menu(int *used_item, OPTIONAL(item_id item, ...)) NATIVE(31);

// Sets or clears an event-local bit flag that is not saved. value 0 clears it.
// Original name: _SET_LOCAL_FLAG.
extern void set_local_flag(int flag, int value) NATIVE(32);

// Returns an event-local bit flag (0 or 1).
// Original name: _GET_LOCAL_FLAG.
extern int get_local_flag(int flag) NATIVE(33);

// Opens the party selection menu after the script yields.
// Original name: _GOTO_SELECT_PARTY.
extern void open_party_select_menu(void) NATIVE(34);

// Starts reading files from disc in the background into the shared read buffer, where later load
// calls find them. Each argument is a file path string, or an item number followed by a variant
// number naming that item's model file. Poll check_background_load until it finishes.
// Original name: _SET_LOADBG_FILE.
extern void start_background_load(any file, ...) NATIVE(35);

// Returns 1 while background file reads are still in progress, 0 once all are done.
// Original name: _CHECK_LOADBG_FILE.
extern int check_background_load(void) NATIVE(36);

// Does nothing and reports failure in the retail game; the out parameters are left unchanged.
// Original name: _GET_TB_ITEMNO.
extern void get_treasure_box_items(int *item_a, int *item_b) NATIVE(37);

// Sets the state value of the treasure box nearest the player.
// Original name: _SET_TB_STATUS.
extern void set_treasure_box_state(enum treasure_box_state state) NATIVE(38);

// Sets how far the lid of the treasure box nearest the player is open, 0.0 closed to 1.0 open.
// Original name: _SET_TB_ANGLE.
extern void set_treasure_box_lid(float open) NATIVE(39);

// Gives the player count of an item (default 1).
// Original name: _ADD_ITEM.
extern void add_item(item_id item, OPTIONAL(int count)) NATIVE(40);

// Returns an item's weapon category: 1 Max melee, 2 Max gun, 3 Monica melee, 4 Monica magic, 0
// anything else. Fails for an unknown item.
// Original name: _GET_ITEM_TYPE.
extern int get_item_type(item_id item) NATIVE(41);

// Returns the index of the first empty slot in the player's bag, or -1 when the bag is full.
// Original name: _GET_ITEM_SPACE.
extern int get_free_item_slot(void) NATIVE(42);

// Returns the scale factor that fits a scene character's bounding box to a cube of half-size size.
// Original name: _GET_ADJUST_POLYGON_SCALE.
extern float get_chara_fit_scale(chara_slot chara, float size) NATIVE(43);

// Plays a full-screen movie file (.PSS) to the end before returning, using memory stack stack_no,
// and draws captions when enabled. init_sound 0 skips initialising the sound library (default 1).
// Original name: _LOAD_MOVIE.
extern void play_movie(enum memory_stack stack_no, string file_name, OPTIONAL(int init_sound)) NATIVE(44);

// Sets every event-local counter to 0.
// Original name: _INIT_LOCAL_CNT.
extern void clear_local_counters(void) NATIVE(45);

// Captures the screen and cross-fades from it. With one argument it is the length in frames of a
// dissolve. With three arguments they are direction (0 fade in, otherwise fade out), type (1 wipe,
// otherwise dissolve) and frames. Frames are at 60 per second and are converted to 50 Hz frames
// internally, with a minimum of 1.
// Original name: _SET_CROSSFADE.
extern void start_crossfade(int frames_or_direction, OPTIONAL(enum cross_fade_type type, int frames)) NATIVE(46);

// Sets the scene's time of day in hours, wrapped into 0 to 24.
// Original name: _SET_TIME.
extern void set_time_of_day(float hours) NATIVE(47);

// Selects which lighting set of the active map is used. Out-of-range values are ignored.
// Original name: _SET_ACTIVE_LIGHT.
extern void set_map_light(int light_no) NATIVE(48);

// Sets the texture animations used for lip sync on an actor: talk_anim is the moving mouth,
// idle_anim (optional) the closed mouth. trigger_lip_sync_anim then switches between them.
// Original name: _SET_PAKU_ANIM.
extern void set_lip_sync_anim(int actor, string talk_anim, OPTIONAL(string idle_anim)) NATIVE(49);

// Clears the lip sync texture animation setup.
// Original name: _RESET_PAKU_ANIM.
extern void reset_lip_sync_anim(void) NATIVE(50);

// Switches the lip sync texture animation. on non-zero stops the idle animation and plays the talk
// animation; 0 stops the talk animation and plays the idle animation, if one is set.
// Original name: _TRG_PAKU_ANIM.
extern void trigger_lip_sync_anim(int on) NATIVE(51);

// Puts a camera back to following character 0. mode 0 sets the active camera to follow at its
// current distance, keeping its current angle and height unless given. mode 1 sets the previous
// camera to follow behind the character's facing plus angle (radians), with height defaulting to
// its current height, and hands control back.
// Original name: _RESET_CAMERA.
extern void reset_camera(enum camera_reset_mode mode, OPTIONAL(float angle, float height)) NATIVE(52);

// Returns the number of the playable character currently in control.
// Original name: _GET_ACTIVE_CHR_NO.
extern int get_active_chara(void) NATIVE(53);

// Sets which playable character is in control, in the save data and battle state.
// Original name: _SET_ACTIVE_CHR_NO.
extern void set_active_chara(int chara) NATIVE(54);

// Sets the current floor of the current dungeon in the save data.
// Original name: _DNG_SET_FLOOR_ID.
extern void set_dungeon_floor(int floor) NATIVE(55);

// Returns the current floor of the current dungeon.
// Original name: _DNG_GET_FLOOR_ID.
extern int get_dungeon_floor(void) NATIVE(56);

// Sets the body motions used while an actor talks: talk_motion while speaking and idle_motion
// after, each with motion flags. trigger_lip_sync_motion then switches between them.
// Original name: _SET_PAKU_MOTION.
extern void set_lip_sync_motion(int actor, string talk_motion, enum chara_motion_flag talk_flags, OPTIONAL(string idle_motion, enum chara_motion_flag idle_flags)) NATIVE(57);

// Clears the lip sync motion setup.
// Original name: _RESET_PAKU_MOTION.
extern void reset_lip_sync_motion(void) NATIVE(58);

// Switches the lip sync motion. mode 0 plays the talk motion, 1 plays the idle motion once the
// talk motion has finished, 2 plays the idle motion at once. Modes 1 and 2 do nothing when no idle
// motion is set.
// Original name: _TRG_PAKU_MOTION.
extern void trigger_lip_sync_motion(enum lip_sync_motion_mode mode) NATIVE(59);

// Sets the screen clear colour, components 0 to 255, alpha defaulting to 128.
// Original name: _SET_BG_COLOR.
extern void set_background_color(float r, float g, float b, OPTIONAL(float a)) NATIVE(60);

// Opens the dungeon map menu after the script yields, passing value as the menu's first parameter.
// Original name: _GOTO_DNG_MAP.
extern void open_dungeon_map_menu(int value) NATIVE(61);

// Ends the event and switches the game to the dungeon. floor and event_no default to -1 (none).
// Original name: _GOTO_DNG.
extern void go_to_dungeon(int dungeon, OPTIONAL(int floor, enum program event_no)) NATIVE(62);

// Ends the event and switches the game to the town edit mode for map_no. event_no defaults to -1
// (none).
// Original name: _GOTO_EDIT.
extern void go_to_edit_mode(map_id map_no, OPTIONAL(enum program event_no)) NATIVE(63);

// Writes up to five result values left by the last menu, one per argument given.
// Original name: _GET_MENU_PARAM.
extern void get_menu_result(int *result_0, OPTIONAL(int *result_1, int *result_2, int *result_3, int *result_4)) NATIVE(64);

// Loads the model of party character npc (1 to 31) into scene character slot chara, using memory
// stack stack_no. model_set 0 uses the normal model, otherwise the event train model.
// Original name: _LOAD_CHARA_NPC.
extern void load_npc_chara(enum npc_model_set model_set, enum memory_stack stack_no, npc_id npc, int chara) NATIVE(65);

// With one argument (-1 on the disc), places the floor's treasure boxes automatically. With five,
// puts a box holding one of item at x, y, z facing rot_y (radians).
// Original name: _AUTO_SET_TREASURE_BOX.
extern void place_treasure_box(item_id item, OPTIONAL(float x, float y, float z, float rot_y)) NATIVE(66);

// Dungeon only. With one argument (-1 on the disc), places the floor's monsters at random spots
// away from the event point. Otherwise spawns monster at x, y, z with optional rotation in radians
// and a talk_param that get_monster_talk_data returns later.
// Original name: _AUTO_SET_MONSTER.
extern void place_monster(monster_kind monster, OPTIONAL(float x, float y, float z, float rot_x, float rot_y, float rot_z, int talk_param)) NATIVE(67);

// Loads a dungeon map and resets the floor state, sound and music. When room_cfg is given the
// floor is built by the map generator from that room layout file, with gen_flags as generation
// options (0x1 dummy trees, 0x2 preset floor, 0x4 dummy mountain, 0x8 fixed start, 0x10 no way in
// or out, 0x20 no healing point, 0x40 fixed floor).
// Original name: _LOAD_DUNGEON_MAP_FILE.
extern void load_dungeon_map(string map_name, OPTIONAL(string room_cfg, enum automap_gen_flag gen_flags)) NATIVE(68);

// Dungeon only. With one argument (ignored), resets monster memory and loads every monster listed
// for the current floor. With two, loads the single monster; a non-zero reset clears monster
// memory first.
// Original name: _LOAD_MONSTER_FILE.
extern void load_monster_data(monster_kind monster, OPTIONAL(int reset)) NATIVE(69);

// Returns the status bits of party character npc (1 to 32); the value 1 bit means the character is
// in the party now.
// Original name: _GET_NPC_STATUS.
extern int get_npc_status(npc_id npc) NATIVE(70);

// Sets the status bits of party character npc (1 to 32).
// Original name: _SET_NPC_STATUS.
extern void set_npc_status(npc_id npc, enum npc_status status) NATIVE(71);

// Returns the number (1 to 32) of the first party character whose status has the in-party bit set,
// or -1 when there is none.
// Original name: _GET_NOW_PARTY_CHARA.
extern int get_party_npc(void) NATIVE(72);

// Stores an event-local counter that is not saved.
// Original name: _SET_LOCAL_CNT.
extern void set_local_counter(int index, int value) NATIVE(73);

// Returns an event-local counter. Fails for an index out of range.
// Original name: _GET_LOCAL_CNT.
extern int get_local_counter(int index) NATIVE(74);

// Returns the index of the first event-local counter equal to value, or -1 when none matches.
// Original name: _GET_LOCAL_CNT2.
extern int find_local_counter(int value) NATIVE(75);

// Writes the position and Y rotation (radians) of train seat seat (0 to 11). On map 120 the height
// is 0.
// Original name: _GET_TRAIN_NPC_POS.
extern void get_train_seat_pos(int seat, float *x, float *y, float *z, float *rot_y) NATIVE(76);

// Opens the chapter title screen for chapter (counted from 1) after the script yields.
// Original name: _GOTO_DRAW_CHAPTER.
extern void show_chapter_title(int chapter) NATIVE(77);

// Sets the projection distance used while the event is drawn; larger values give a narrower view.
// Original name: _SET_PROJECTION.
extern void set_projection(float distance) NATIVE(78);

// Returns the event's projection distance.
// Original name: _GET_PROJECTION.
extern float get_projection(...) NATIVE(79);

// Fades the screen in over frames from colour r, g, b (0 to 255). Frames are at 60 per second and
// are converted to 50 Hz frames internally, with a minimum of 1.
// Original name: _SET_FADE_IN.
extern void fade_in(int frames, OPTIONAL(float r, float g, float b)) NATIVE(80);

// Fades the screen out over frames to colour r, g, b (0 to 255, default black). Frames are at 60
// per second and are converted to 50 Hz frames internally, with a minimum of 1.
// Original name: _SET_FADE_OUT.
extern void fade_out(int frames, OPTIONAL(float r, float g, float b)) NATIVE(81);

// Runs a debug command. command 0 adds 512 HP to the dungeon party character.
// Original name: _DNG_DEBUG_COMMAND.
extern void run_dungeon_debug_command(enum dungeon_debug_command command) NATIVE(82);

// Seeks the disc drive to the start of the named file.
// Original name: _CD_SEEK.
extern void seek_disc_file(string file_name) NATIVE(83);

// Returns atan2(dx, dz): the Y rotation in radians that faces along the offset dx, dz.
// Original name: _GET_ROT_LOOK_POS.
extern float get_look_yaw(float dx, float dz) NATIVE(84);

// Sets the strength of the scene's motion blur; 0 turns it off.
// Original name: _SET_MOTION_BLUR.
extern void set_motion_blur(int strength) NATIVE(85);

// Requests loading another event script and starting event_no in it. "_1" is inserted before
// ".stb" when the path lacks it, and the path is converted for the current language.
// Original name: _LOAD_SCRIPT.
extern void load_event_script(string path, enum program event_no) NATIVE(86);

// Computes a camera for a conversation between two points: the look-at point is their midpoint
// raised 30 units and the camera sits at a fixed offset rotated by angle (radians) around it.
// Writes the camera position and look-at position.
// Original name: _SET_TALK_CAMERA.
extern void calc_talk_camera(float x1, float y1, float z1, float x2, float y2, float z2, float angle, float *cam_x, float *cam_y, float *cam_z, float *look_x, float *look_y, float *look_z) NATIVE(87);

// Starts a spark burst in hit effect slot (0 to 4) at x, y, z. The optional settings default to
// direction (0, 1, 0), spread 50, speed 35, power 0, gravity 0.05, life 30 frames and count 32
// sparks; pass all of them or none.
// Original name: _HIT_EFFECT.
extern void spawn_hit_effect(int slot, float x, float y, float z, OPTIONAL(float dir_x, float dir_y, float dir_z, float spread, float speed, float power, float gravity, int life, int count)) NATIVE(88);

// Copies scene character src into slot dst, sharing its textures and allocating from memory stack
// stack_no. A single argument is instead an id of a stored argument list.
// Original name: _COPY_CHARA.
extern void copy_chara(enum memory_stack stack_no, int src, int dst) NATIVE(89);

// Returns the pad button the event treats as its start button.
// Original name: _GET_START_BUTTON.
extern int get_start_button(void) NATIVE(90);

// Requests entering an interior once the event ends. jump_point is the entry point (-1 for the
// default) and event_no the event started on arrival (default 100). Unused on the disc.
// Original name: _MOVE_INTERIOR.
extern void enter_interior(int jump_point, string map_name, OPTIONAL(enum program event_no)) NATIVE(91);

// Reads data on the dungeon monster the player spoke to: its scene character slot, its monster id
// and the talk_param it was placed with. With two arguments the second receives talk_param.
// Original name: _GET_MONSTER_TALK_DATA.
extern void get_monster_talk_data(int *monster_chara, int *id_or_param, OPTIONAL(int *talk_param)) NATIVE(92);

// Enables or disables a named function point (an interaction trigger) on the active map. place is
// a map part number or part name string; an empty string means the map's own function points.
// Original name: _FUNC_POINT_SHOW.
extern void set_func_point_enabled(any place, string name, int enable) NATIVE(93);

// Returns the current map number.
// Original name: _GET_NOW_MAP_NO.
extern int get_map_no(void) NATIVE(94);

// Returns the current sub map number.
// Original name: _GET_NOW_SUBMAP_NO.
extern int get_submap_no(void) NATIVE(95);

// Returns the previous map number.
// Original name: _GET_OLD_MAP_NO.
extern int get_prev_map_no(void) NATIVE(96);

// Returns the previous sub map number.
// Original name: _GET_OLD_SUBMAP_NO.
extern int get_prev_submap_no(void) NATIVE(97);

// Sets which scene character rain splashes come off. -1 clears the splash particles.
// Original name: _SET_RAIN_CHARA_NO.
extern void set_rain_chara(chara_slot chara) NATIVE(98);

// Finds a placed part by name on the active edit map and writes its position and Y rotation.
// Original name: _GET_EDIT_PARTS_POS.
extern void get_edit_part_pos(string part_name, float *x, float *y, float *z, float *rot_y) NATIVE(99);

// Writes the position of a scene character.
// Original name: _GET_CHARA_POS.
extern void get_chara_pos(chara_slot chara, float *x, float *y, float *z) NATIVE(100);

// Writes the screen position in pixels of a point at 85% of a scene character's height, where a
// speech balloon would go.
// Original name: _GET_CHARA_TALK_POS.
extern void get_chara_screen_pos(chara_slot chara, int *screen_x, int *screen_y) NATIVE(101);

// Turns a scene character's yaw toward a world point by at most max_turn radians, taking the
// shorter way. Call it once per frame in a loop to turn smoothly.
// Original name: _TURN_CHARA.
extern void turn_chara_toward(chara_slot chara, float x, float y, float z, float max_turn) NATIVE(102);

// Places a scene character at a world position. Called with a single argument, that argument is
// instead the key of a stored argument block holding the character and position.
// Original name: _SET_CHARA_POS.
extern void set_chara_pos(chara_slot chara, float x, float y, OPTIONAL(float z)) NATIVE(103);

// Sets a scene character's rotation in radians around the x, y and z axes; each angle is wrapped
// into -pi to pi. Called with a single argument, that argument is the key of a stored argument
// block holding the character and rotation.
// Original name: _SET_CHARA_ROT.
extern void set_chara_rot(chara_slot chara, float x, float y, OPTIONAL(float z)) NATIVE(104);

// Reads a scene character's rotation in radians around the x, y and z axes into three out
// parameters.
// Original name: _GET_CHARA_ROT.
extern void get_chara_rot(chara_slot chara, any x, any y, any z) NATIVE(105);

// Starts a motion or motion sequence on a scene character by name. flags: 1 pauses on the current
// frame, 2 holds the last frame instead of looping, 4 restarts from the first frame without
// blending. When step is given and not -1 and flag 4 is set, the motion is applied at once and
// advances step frames per tick; reset non-zero clears the current motion first.
// Original name: _SET_MOTION.
extern void set_chara_motion(chara_slot chara, string motion_name, OPTIONAL(enum chara_motion_flag flags, float step, int reset)) NATIVE(106);

// Sets how many motion frames a scene character's current motion advances each tick.
// Original name: _SET_STEP.
extern void set_chara_motion_step(chara_slot chara, float step) NATIVE(107);

// Turns a texture animation group on a scene character's texture block on or off. With on = 1 it
// starts the named group; with on = 0 it stops the named group, or every group when no name is
// given.
// Original name: _SET_TEX_ANIM.
extern void set_chara_texture_anim(chara_slot chara, int on, OPTIONAL(string anim_name)) NATIVE(108);

// Sets a scene character's scale along x, y and z (1.0 is normal size). Called with a single
// argument, that argument is the key of a stored argument block holding the character and scale.
// Original name: _SET_SCALE.
extern void set_chara_scale(chara_slot chara, float x, float y, float z) NATIVE(109);

// Makes the follower character's root frame follow the named model frame (bone) of chara, without
// parenting it, so it moves with that bone.
// Original name: _SET_REFERENCE.
extern void attach_chara_to_frame(chara_slot chara, string frame_name, int follower) NATIVE(110);

// Stops a character's root frame following another model's frame, undoing attach_chara_to_frame on
// the follower.
// Original name: _DEL_REFERENCE.
extern void detach_chara_from_frame(chara_slot chara) NATIVE(111);

// Sets the no-cull attribute on a scene character's shadow frame so the shadow is not clipped off
// screen (1) or is culled normally (0). The code reads the second argument whenever any argument
// is passed, so pass it explicitly.
// Original name: _SHADOW_CLIP_OFF.
extern void set_shadow_no_cull(chara_slot chara, OPTIONAL(int no_cull)) NATIVE(112);

// Returns the yaw in radians that faces a scene character toward a world point, ignoring height.
// Returns 0 when the point is directly above or below the character.
// Original name: _GET_COORDINATE_ANGLE.
extern float get_chara_angle_to(chara_slot chara, float x, float y, float z) NATIVE(113);

// Returns a scene character's body width from its model info.
// Original name: _GET_CHARA_WIDTH.
extern float get_chara_width(chara_slot chara) NATIVE(114);

// Returns a scene character's body height from its model info, in world units.
// Original name: _GET_CHARA_HEIGHT.
extern float get_chara_height(chara_slot chara) NATIVE(115);

// Returns a scene character's body depth from its model info.
// Original name: _GET_CHARA_WEIGHT.
extern float get_chara_depth(chara_slot chara) NATIVE(116);

// Shows (1) or hides (0) a scene character. With fade = 1 it fades in or out instead of popping,
// changing alpha by fade_speed per frame (default 0.1, so 10 frames).
// Original name: _SET_CHARA_SHOW.
extern void set_chara_visible(chara_slot chara, int visible, OPTIONAL(int fade, float fade_speed)) NATIVE(117);

// Reads whether a scene character is shown into visible. The fade flag is written only when argc
// is 2, and then to the slot after visible, so as shipped a three-argument call never receives it.
// Original name: _GET_CHARA_SHOW.
extern void get_chara_visible(chara_slot chara, int *visible, OPTIONAL(int *fade)) NATIVE(118);

// Turns the stepping of a scene character's cloth and hair animation on (1) or off (0).
// Original name: _CHARA_DA_ENABLE.
extern void set_chara_cloth_anim(chara_slot chara, int on) NATIVE(119);

// Returns how far a scene character's current motion has played, from 0.0 to 1.0. Fails unless
// called with two arguments.
// Original name: _GET_MOT_NOW_WAIT.
extern float get_chara_motion_progress(chara_slot chara) NATIVE(120);

// Returns 1 when a scene character's motion reaches its last frame within the next two ticks or no
// motion plays, else 0.
// Original name: _CHECK_MOTION_END.
extern int check_chara_motion_end(chara_slot chara) NATIVE(121);

// Like set_chara_motion but for an action character, setting the motion on it and on every part
// chained onto it. Flags: 1 pause, 2 hold last frame, 4 restart without blending.
// Original name: _ACTCHR_SET_MOTION.
extern void set_chain_chara_motion(chara_slot chara, string motion_name, OPTIONAL(enum chara_motion_flag flags, float step, int reset)) NATIVE(122);

// Sets an action character's second sound bank, the loaded sound id its motions play extra sound
// effects from.
// Original name: _SET_CHARA_EX_SOUNDID.
extern void set_chara_extra_sound(chara_slot chara, int sound) NATIVE(123);

// Copies an action character's sound settings (sound banks and footsteps) to every part chained
// onto it.
// Original name: _ACTCHR_SOUND_INFO_COPY.
extern void copy_chara_sound_info(chara_slot chara) NATIVE(124);

// Opens a message window on a message. An int is a message number: from the text file loaded with
// load_message_file if any, else from the town NPC talk text for window 0, else from the event
// message file; showing the number the window already holds moves to its next page, and -1 lays
// out nothing. A string is shown as literal text. The optional out parameter receives the number
// of characters laid out.
// Original name: _MES_MAKE.
extern void show_message(int mes, message text, OPTIONAL(int *char_count)) NATIVE(192);

// Closes a message window, clears its message number and text, and restores its default reveal
// speed and bubble centre.
// Original name: _MES_CLOSE.
extern void close_message(int mes) NATIVE(193);

// Moves a message window on to its next page when it is waiting at a page break and not scrolling;
// otherwise does nothing.
// Original name: _MES_NEXTPAGE.
extern void advance_message_page(int mes) NATIVE(194);

// Places a speech bubble and points its tail at the speaker. With three arguments it takes the
// speaker and listener scene characters and uses their screen positions; with five it takes
// speaker_x, speaker_y, listener_x, listener_y screen coordinates directly.
// Original name: _SET_MES_AUTOSET.
extern void place_message_bubble(int mes, int speaker, int listener, OPTIONAL(int listener_x, int listener_y)) NATIVE(195);

// Turns a speech bubble's tail on (1) or off (0), optionally setting the tail's length and half
// its width at the root, in screen pixels.
// Original name: _SET_MES_SHIPPO.
extern void set_message_tail(int mes, int on, OPTIONAL(int length, int half_width)) NATIVE(196);

// Forces a speech bubble into a screen slot, counting from 1; 0 lets the game choose.
// Original name: _SET_MES_POS.
extern void set_message_slot(int mes, int slot) NATIVE(197);

// Sets how many characters a message window reveals per frame; 0 shows the text at once. The
// optional second value also sets the default speed the window returns to on close.
// Original name: _SET_MES_DRAWSPEED.
extern void set_message_speed(int mes, float speed, OPTIONAL(float default_speed)) NATIVE(198);

// Puts a message window's select cursor on a line; -1 hides it. Moving from no cursor restarts the
// cursor animation.
// Original name: _SET_MES_CURSOR.
extern void set_message_cursor(int mes, int line) NATIVE(199);

// Returns 1 when all the window's text is shown (state 3), else 0.
// Original name: _CHECK_MES_COMPLETE.
extern int is_message_shown(int mes) NATIVE(200);

// Returns 1 when the window is waiting at a page break (state 5), else 0.
// Original name: _CHECK_MES_WAIT.
extern int is_message_page_wait(int mes) NATIVE(201);

// Returns 1 when the window has fully faded out (state 0), else 0.
// Original name: _CHECK_MES.
extern int is_message_closed(int mes) NATIVE(202);

// Sets whether a message window draws the button prompt once its text is shown (1) or not (0).
// Original name: _SET_MES_OKURI.
extern void set_message_prompt(int mes, int on) NATIVE(203);

// Switches a message window to the speech-bubble frame (window mode 1). The second argument is
// read and ignored.
// Original name: _SET_MES_FUKIDASHI.
extern void set_message_bubble(int mes, int unused) NATIVE(204);

// Sets a message window's frame mode straight to 1 (speech bubble) when on is non-zero, else 0 (no
// frame), without changing its colours.
// Original name: _SET_MES_WIN_FLAG.
extern void set_message_window_flag(int mes, int on) NATIVE(205);

// Puts the active camera's eye at a world position at once. Called with a single argument, that
// argument is the key of a stored argument block holding the position.
// Original name: _SET_CAMERA_POS.
extern void set_camera_pos(float x, float y, float z) NATIVE(206);

// Reads the active camera's eye position into x, y and z.
// Original name: _GET_CAMERA_POS.
extern void get_camera_pos(any x, any y, any z) NATIVE(207);

// Puts the active camera's look-at point at a world position at once. Called with a single
// argument, that argument is the key of a stored argument block holding the position.
// Original name: _SET_CAMERA_REF.
extern void set_camera_target(float x, float y, float z) NATIVE(208);

// Reads the active camera's look-at point into x, y and z.
// Original name: _GET_CAMERA_REF.
extern void get_camera_target(any x, any y, any z) NATIVE(209);

// Sets how many steps the active camera's eye and look-at point take to reach their next
// positions.
// Original name: _SET_CAMERA_SPEED.
extern void set_camera_speed(float steps) NATIVE(210);

// Moves the active camera's eye and look-at point the given number of steps toward their next
// positions; a negative count puts them there at once.
// Original name: _CAMERA_STEP.
extern void step_camera(int steps) NATIVE(211);

// Sets the frame drawn around a message window, with its colour and outline: 0 none, 1 speech
// bubble, 2 help (becomes 4), 3 floating menu, 4 plain menu, 5 yes/no menu, 6 menu with selection
// band, 7 bottom centre, 8 fourth menu style, 9 bottom bubble, 10 bottom bubble (timer variant),
// 11 screen centre, 12 plain (stored as 0).
// Original name: _SET_MES_WINDOW_MODE.
extern void set_message_window_mode(int mes, enum mes_window_mode mode) NATIVE(212);

// Empties a message window and applies a preset: 0 speech bubble at the language's reveal speed, 1
// system text shown at once, 2 free-placed text shown at once, 3 names over characters, 4 plain
// menu window, 5 small-font bubble with slow fade, 6 plain menu revealing one character per frame.
// Original name: _SET_MES_PRESET.
extern void apply_message_preset(int mes, enum mes_preset preset) NATIVE(213);

// Sets the system message number that item code slot 1 to 16 inserts into the window's text. Slots
// outside 1 to 16 are ignored.
// Original name: _SET_MES_ITEM_DIRECT.
extern void set_message_sysmes(int mes, int slot, int sysmes_no) NATIVE(214);

// Sets item code slot 1 to 16 of a message window to insert an item's name, given the item number.
// Slots outside 1 to 16 are ignored.
// Original name: _SET_MES_ITEM.
extern void set_message_item(int mes, int slot, item_id item) NATIVE(215);

// Sets a number the message text inserts: slot 0 sets the single value code, slots 1 to 16 set the
// numbered value codes and clear their padding width.
// Original name: _SET_MES_VALUE.
extern void set_message_value(int mes, int slot, int value) NATIVE(216);

// Returns a message window's state: 0 closed, 1 opening, 2 revealing text, 3 fully shown, 4
// closing, 5 waiting at a page break, 6 scrolling.
// Original name: _GET_MES_STATUS.
extern int get_message_state(int mes) NATIVE(217);

// Looks up the event message number of a kind of line a party-capable town character says and
// writes it to mes_no (0 for an unknown character); kind 0 is the character's name. For kind 4 it
// also writes the character's option count to option_count.
// Original name: _GET_PARTY_CHARA_MES_NO.
extern void get_npc_message_no(npc_id npc, int kind, int *mes_no, OPTIONAL(int *option_count)) NATIVE(218);

// Points a message window at a message buffer: source 0 makes the window read its messages from
// sysmes.mes, any other value makes its item codes read from system.mes. The third argument is
// read and ignored.
// Original name: _MES_SET_BUFF.
extern void set_message_source(int mes, enum mes_source source, int unused) NATIVE(219);

// Returns a message window's frame mode (see set_message_window_mode).
// Original name: _GET_MES_WINDOW_MODE.
extern int get_message_window_mode(any mes, ...) NATIVE(220);

// Returns 1 while a voice stream started by the event is playing, else 0.
// Original name: _GET_MES_VOICE.
extern int is_voice_playing(void) NATIVE(222);

// Sets the first line of a message window that the select cursor can choose, for question menus.
// Original name: _SET_MES_QUESTION_GYOU.
extern void set_message_first_choice(int mes, int line) NATIVE(223);

// Returns the first selectable line of a message window.
// Original name: _GET_MES_QUESTION_GYOU.
extern int get_message_first_choice(int mes) NATIVE(224);

// Sets the number of frames before a message window closes by itself; 0 for never.
// Original name: _SET_MES_CLOSE_CNT.
extern void set_message_close_timer(int mes, int frames) NATIVE(225);

// Sets one of several message window options chosen by option: 0 line indent on/off, 1 cursor
// centring on/off, 2 half-width character width (float, negative for default), 3 shows the
// aquarium fish list (no values), 4 fixed frame size (width, height, or x, y, width, height), 5
// shade of a line (line, shade), 6 alpha of a line (line, alpha), 7 text colour (r, g, b, a), 8
// outline style, 9 text offset inside the fixed frame (x, y, negative for default), 10 window
// alpha (128 opaque).
// Original name: _SET_MES_ETC.
extern void set_message_option(int mes, enum mes_option option, OPTIONAL(any value1, int value2, int value3, int value4)) NATIVE(226);

// Reads one of several values chosen by query: 0 line indent flag, 1 cursor centring flag, 2
// number of fish in the main aquarium tank, each into the third argument. Query 3 takes a villager
// number as the third argument and writes two fields of that villager's info to the fourth and
// fifth.
// Original name: _GET_MES_ETC.
extern void get_message_info(int mes, enum mes_info_query query, any value, OPTIONAL(int *result1, int *result2)) NATIVE(227);

// Loads a text file into the event scene's memory stack and makes show_message read message
// numbers from it. Called with two arguments it unloads instead and the window goes back to the
// event message file.
// Original name: _LOAD_MES.
extern void load_message_file(int mes, enum memory_stack stack_no, OPTIONAL(message_file file_name)) NATIVE(228);

// Plays a system sound effect for message windows: kind 0 or 1 plays system sound 25, 2 plays
// sound 0, 3 plays sound 1. Other values fail.
// Original name: _MES_SE_PLAY.
extern void play_message_se(enum message_se kind) NATIVE(229);

// Sets string slot 1 to 16 that the message text's string codes insert.
// Original name: _SET_MES_STR.
extern void set_message_string(int mes, int slot, string text) NATIVE(230);

// Returns whether a message window draws its button prompt. The code only runs when called with a
// single argument, so the usual two-argument call fails and leaves the result unchanged.
// Original name: _GET_MES_OKURI.
extern int get_message_prompt(int mes) NATIVE(231);

// Does nothing in the retail build: the body reads no arguments and reports success. Parameters
// follow how the disc calls it.
// Original name: _ASQ_INIT.
extern void init_asq(int slot) NATIVE(300);

// Does nothing in the retail build: the body reads no arguments and reports success. Parameters
// follow how the disc calls it.
// Original name: _ASQ_SYNC_CHARA.
extern void sync_asq_chara(chara_slot slot, int chara) NATIVE(301);

// Does nothing in the retail build: the body reads no arguments and reports success. Parameters
// follow how the disc calls it.
// Original name: _ASQ_SET_POS.
extern void set_asq_pos(int slot, float x, float y, float z) NATIVE(302);

// Does nothing in the retail build: the body reads no arguments and reports success. Parameters
// follow how the disc calls it.
// Original name: _ASQ_MOVE.
extern void move_asq_to(int slot, float x, float y, float z, int frames) NATIVE(303);

// Does nothing in the retail build: the body reads no arguments and reports success. Never called
// on the disc.
// Original name: _ASQ_MOVE_STEP.
extern void set_asq_move_step(void) NATIVE(304);

// Does nothing in the retail build: the body reads no arguments and reports success. Parameters
// follow how the disc calls it.
// Original name: _ASQ_ROT_REF.
extern void turn_asq_to_point(int slot, float x, float y, float z, float rate) NATIVE(305);

// Does nothing in the retail build: the body reads no arguments and reports success. Never called
// on the disc.
// Original name: _ASQ_ROT_ANGLE.
extern void turn_asq_to_angle(void) NATIVE(306);

// Does nothing in the retail build: the body reads no arguments and reports success. Never called
// on the disc.
// Original name: _ASQ_CLEAR_ROT.
extern void clear_asq_rot(void) NATIVE(307);

// Does nothing in the retail build: the body reads no arguments and reports success. Never called
// on the disc.
// Original name: _ASQ_WAIT_ROT.
extern void wait_asq_rot(void) NATIVE(308);

// Does nothing in the retail build: the body reads no arguments and reports success. Never called
// on the disc.
// Original name: _ASQ_ROT_MOVE.
extern void turn_asq_while_moving(void) NATIVE(309);

// Does nothing in the retail build: the body reads no arguments and reports success. Parameters
// follow how the disc calls it.
// Original name: _ASQ_SET_ROT.
extern void set_asq_rot(int slot, float angle) NATIVE(310);

// Does nothing in the retail build: the body reads no arguments and reports success. Never called
// on the disc.
// Original name: _ASQ_DELAY_ROT.
extern void delay_asq_rot(void) NATIVE(311);

// Does nothing in the retail build: the body reads no arguments and reports success. Never called
// on the disc.
// Original name: _ASQ_CHECK.
extern void check_asq(void) NATIVE(312);

// Does nothing in the retail build: the body reads no arguments and reports success. Parameters
// follow how the disc calls it.
// Original name: _ASQ_MOTION_TRG.
extern void trigger_asq_motion(int slot) NATIVE(313);

// Does nothing in the retail build: the body reads no arguments and reports success. Parameters
// follow how the disc calls it.
// Original name: _ASQ_MOTION_PLAY.
extern void play_asq_motion(int slot, string motion_name) NATIVE(314);

// Does nothing in the retail build: the body reads no arguments and reports success. Parameters
// follow how the disc calls it.
// Original name: _ASQ_MOTION_STOP.
extern void stop_asq_motion(int slot, any value) NATIVE(315);

// Does nothing in the retail build: the body reads no arguments and reports success. Never called
// on the disc.
// Original name: _ASQ_MOTION_NEXT.
extern void queue_asq_motion(void) NATIVE(316);

// Does nothing in the retail build: the body reads no arguments and reports success. Parameters
// follow how the disc calls it.
// Original name: _ASQ_ANIME_TRG.
extern void trigger_asq_anime(int slot) NATIVE(317);

// Does nothing in the retail build: the body reads no arguments and reports success. Parameters
// follow how the disc calls it.
// Original name: _ASQ_ANIME.
extern void set_asq_anime(int slot, any anime, OPTIONAL(any extra)) NATIVE(318);

// Does nothing in the retail build: the body reads no arguments and reports success. Never called
// on the disc.
// Original name: _ASQ_SE_PLAY.
extern void play_asq_se(void) NATIVE(319);

// Shows (1) or hides (0) one of the 8 event images (0 to 7). The visibility flag comes first and
// the image number second.
// Original name: _IMG_SET_DRAW.
extern void show_image(int on, int image) NATIVE(320);

// Sets the part of the texture an event image shows, in texels: left x, top y, width, height.
// Original name: _IMG_SET_GET.
extern void set_image_source_rect(int image, int x, int y, int width, int height) NATIVE(321);

// Sets the screen rectangle an event image draws into, in pixels: left x, top y, width, height.
// Original name: _IMG_SET_PUT.
extern void set_image_screen_rect(int image, int x, int y, int width, int height) NATIVE(322);

// Gives an event image (0 to 7) the loaded texture of the given name.
// Original name: _IMG_SET_NAME.
extern void set_image_texture(int image, string texture_name) NATIVE(323);

// Slides an event image's top-left corner to screen x, y over the given number of frames; 0 jumps
// there.
// Original name: _IMG_SET_MOVE.
extern void move_image_to(int image, int x, int y, int frames) NATIVE(324);

// Fades an event image's alpha in to 128 (fade_in = 1) or out to 0 (fade_in = 0) over the given
// number of frames.
// Original name: _IMG_SET_FADE.
extern void fade_image(int image, int fade_in, int frames) NATIVE(325);

// Sets an event image's colour and alpha, each 0 to 128 where 128 is full.
// Original name: _IMG_SET_COLOR.
extern void set_image_color(int image, int r, int g, int b, int a) NATIVE(326);

// Resets an event sprite (0 to 47) to a hidden, untextured, white screen sprite.
// Original name: _SPRITE_INIT.
extern void init_sprite(int sprite) NATIVE(350);

// Sets the draw pass of an event sprite: 0 hidden, 1 normal pass, 2 first pass.
// Original name: _SPRITE_SET_DRAW.
extern void set_sprite_draw(int sprite, enum event_sprite_draw mode) NATIVE(351);

// Sets where an event sprite lives: 0 on screen (screen position, turns around the screen z axis),
// 1 in the world as a depth-tested billboard.
// Original name: _SPRITE_SET_TYPE.
extern void set_sprite_type(int sprite, enum event_sprite_type type) NATIVE(352);

// Gives an event sprite the named texture from a texture block.
// Original name: _SPRITE_SET_TEXTURE.
extern void set_sprite_texture(int sprite, string texture_name, int tex_block) NATIVE(353);

// Sets the centre of an event sprite: screen pixels for screen sprites (y is shifted down 32),
// world units for world sprites. z defaults to 0.
// Original name: _SPRITE_SET_POS.
extern void set_sprite_pos(int sprite, enum screen_x x, enum screen_y y, OPTIONAL(float z)) NATIVE(354);

// Sets an event sprite's width and height before scaling. A height of 416 or 448 is enlarged by
// 64.
// Original name: _SPRITE_SET_PUTSIZE.
extern void set_sprite_size(int sprite, enum screen_x width, enum screen_y height) NATIVE(355);

// Sets the part of the texture an event sprite shows, in texels: u, v, width, height.
// Original name: _SPRITE_SET_UVSIZE.
extern void set_sprite_uv(int sprite, int u, int v, int width, int height) NATIVE(356);

// Sets an event sprite's colour and alpha, with 128.0 as full.
// Original name: _SPRITE_SET_COLOR.
extern void set_sprite_color(int sprite, enum color_level r, enum color_level g, enum color_level b, enum color_level a) NATIVE(357);

// Sets an event sprite's horizontal and vertical scale. A fourth argument seen on the disc is
// ignored.
// Original name: _SPRITE_SET_SCALE.
extern void set_sprite_scale(int sprite, float scale_x, float scale_y, OPTIONAL(any unused)) NATIVE(358);

// Sets an event sprite's alpha blend mode: 1 normal, 2 additive, 3 subtractive, 4 none, 5 full
// additive. Sprites start at 1.
// Original name: _SPRITE_SET_ALPHAB.
extern void set_sprite_blend(int sprite, enum alpha_macro mode) NATIVE(359);

// Returns 1 while the camera sequence is still following its path, 0 once it has reached the end.
// Original name: _CMRS_CHECK.
extern int is_camera_path_running(void) NATIVE(400);

// Stops every track of the event camera sequence, clears its state and drops all queued camera
// commands. Call it before queuing a new camera shot.
// Original name: _CMRS_INIT.
extern void clear_camera_seq(void) NATIVE(401);

// Queues a wait of a number of frames on the camera position track, so the next queued eye or
// look-at command starts that much later.
// Original name: _CMRS_PRDELAY.
extern void delay_camera_moves(int frames) NATIVE(402);

// Queues putting the camera eye at a world position at once, on the camera position track.
// Original name: _CMRS_SET_POS.
extern void set_camera_seq_pos(float x, float y, float z) NATIVE(403);

// Queues putting the camera look-at point at a world position at once, on the camera position
// track.
// Original name: _CMRS_SET_REF.
extern void set_camera_seq_ref(float x, float y, float z) NATIVE(404);

// Queues moving the camera eye to (x, y, z) and the look-at point to (ref_x, ref_y, ref_z) at an
// even speed over a number of frames. The call fails with any count other than seven arguments;
// one disc script passes nine.
// Original name: _CMRS_MOVE.
extern void move_camera_to(float x, float y, float z, float ref_x, float ref_y, float ref_z, int frames, OPTIONAL(any extra1, any extra2)) NATIVE(405);

// Queues moving only the camera look-at point to a world position at an even speed over a number
// of frames.
// Original name: _CMRS_MOVE_REF.
extern void move_camera_ref(float x, float y, float z, int frames) NATIVE(406);

// Queues removing every point from the camera path, on the camera position track.
// Original name: _CMRS_INIT_PAS.
extern void clear_camera_path(void) NATIVE(407);

// Queues setting how many frames the camera takes to run the whole camera path.
// Original name: _CMRS_SET_PAS_FRM.
extern void set_camera_path_frames(int frames) NATIVE(408);

// Queues adding an eye point (x, y, z) and a look-at point (ref_x, ref_y, ref_z) to the end of the
// camera path, up to 16 points.
// Original name: _CMRS_ADD_PAS.
extern void add_camera_path_point(float x, float y, float z, float ref_x, float ref_y, float ref_z) NATIVE(409);

// Queues running the camera along a spline through the camera path points over the path's frame
// count; the position track continues once the end is reached.
// Original name: _CMRS_START_PAS.
extern void start_camera_path(void) NATIVE(410);

// Queues a wait of a number of frames on the camera orbit track (angle, height and distance), so
// the next queued orbit command starts that much later.
// Original name: _CMRS_AHDDELAY.
extern void delay_camera_orbit(int frames) NATIVE(411);

// Queues placing the camera eye at an angle around the look-at point, keeping its distance. The
// angle is in radians about the vertical axis; the eye goes to ref + dist * (sin angle, 0, cos
// angle).
// Original name: _CMRS_SET_ANGLE.
extern void set_camera_orbit_angle(float angle) NATIVE(412);

// Queues placing the camera eye a height in world units above the look-at point.
// Original name: _CMRS_SET_HEIGHT.
extern void set_camera_orbit_height(float height) NATIVE(413);

// Queues placing the camera eye a horizontal distance in world units from the look-at point,
// keeping its angle.
// Original name: _CMRS_SET_DIST.
extern void set_camera_orbit_dist(float dist) NATIVE(414);

// Queues placing the camera eye around the look-at point by angle in radians about the vertical
// axis, height above it and horizontal distance from it, all at once. While the camera follows an
// event object these set the follow angle, height and distance instead.
// Original name: _CMRS_SET_AHD.
extern void set_camera_orbit(float angle, float height, float dist) NATIVE(415);

// Queues changing the camera's angle (radians), height and distance around the look-at point to
// new values at an even speed over a number of frames.
// Original name: _CMRS_MOVE_AHD.
extern void move_camera_orbit(float angle, float height, float dist, int frames) NATIVE(416);

// Queues a wait of a number of frames on the camera fade track, so the next queued fade starts
// that much later.
// Original name: _CMRS_FADE_DELAY.
extern void delay_camera_fade(int frames) NATIVE(419);

// Queues a fade track command that does nothing and finishes at once.
// Original name: _CMRS_FADE_INIT.
extern void init_camera_fade(void) NATIVE(420);

// Queues fading the screen in from a colour over a number of frames, on the camera fade track. The
// colour components run from 0 to 255 and default to black; give all three or none.
// Original name: _CMRS_FADE_IN.
extern void fade_camera_in(int frames, OPTIONAL(float r, float g, float b)) NATIVE(421);

// Queues fading the screen out to a colour over a number of frames, on the camera fade track. The
// colour components run from 0 to 255 and are read only when all three are given; otherwise the
// fade is to black.
// Original name: _CMRS_FADE_OUT.
extern void fade_camera_out(int frames, OPTIONAL(float r, float g, float b)) NATIVE(422);

// Queues letting the camera's last eye and look-at movement carry on and die away: each frame for
// a number of frames the movement per frame is multiplied by rate (such as 0.96) and applied.
// Original name: _CMRS_PR_SLOWING.
extern void slow_camera_move(float rate, int frames) NATIVE(423);

// Queues letting the camera's last angle, height and distance change carry on and die away: each
// frame for a number of frames the change per frame is multiplied by rate and applied.
// Original name: _CMRS_AHD_SLOWING.
extern void slow_camera_orbit(float rate, int frames) NATIVE(424);

// Queues marking a return point on the camera orbit track; commands after it are kept so a later
// loop_camera_orbit can run them again.
// Original name: _CMRS_AHD_KEEP.
extern void mark_camera_orbit_loop(void) NATIVE(425);

// Queues jumping the camera orbit track back to the return point set by mark_camera_orbit_loop,
// repeating the commands after it.
// Original name: _CMRS_AHD_RETURN.
extern void loop_camera_orbit(void) NATIVE(426);

// Queues marking a return point on the camera position track; commands after it are kept so a
// later loop_camera_moves can run them again.
// Original name: _CMRS_PR_KEEP.
extern void mark_camera_move_loop(void) NATIVE(427);

// Queues jumping the camera position track back to the return point set by mark_camera_move_loop,
// repeating the commands after it.
// Original name: _CMRS_PR_RETURN.
extern void loop_camera_moves(void) NATIVE(428);

// Queues a wait of a number of frames on the camera shake track, so the next queued shake starts
// that much later.
// Original name: _CMRS_QUAKE_DELAY.
extern void delay_camera_shake(int frames) NATIVE(429);

// Queues shaking the camera eye and look-at point by an offset (x, y, z in world units) that
// alternates direction every two frames and shrinks to nothing over a number of frames. With
// frames = -1 the shake keeps its full size and never ends until the camera sequence is cleared.
// Original name: _CMRS_QUAKE.
extern void shake_camera(float x, float y, float z, int frames) NATIVE(430);

// Queues moving the camera eye to (x, y, z) and the look-at point to (ref_x, ref_y, ref_z) over a
// number of frames with eased speed. ease is 0 to speed up and slow down, 1 to speed up only, 2 to
// slow down only; ease_rate is the share of the frame count (0 to 1) spent changing speed.
// Original name: _CMRS_MOVE2.
extern void move_camera_eased(float x, float y, float z, float ref_x, float ref_y, float ref_z, int frames, enum scene_seq_ease ease, float ease_rate) NATIVE(431);

// Queues changing the camera's angle (radians), height and distance around the look-at point over
// a number of frames with eased speed. ease is 0 to speed up and slow down, 1 to speed up only, 2
// to slow down only; ease_rate is the share of the frame count spent changing speed.
// Original name: _CMRS_MOVE_AHD2.
extern void move_camera_orbit_eased(float angle, float height, float dist, int frames, enum scene_seq_ease ease, float ease_rate) NATIVE(432);

// Queues a wait of a number of frames on the camera character track, so the next queued character
// attach starts that much later.
// Original name: _CMRS_CHARA_DELAY.
extern void delay_camera_chara(int frames) NATIVE(433);

// Queues keeping a character at a distance in world units in front of the camera eye, along the
// line to the look-at point, for a number of frames. frames = -1 keeps it there until the camera
// sequence is cleared.
// Original name: _CMRS_CHARA_ATTACH.
extern void attach_chara_to_camera(chara_slot chara, float dist, int frames) NATIVE(434);

// Queues moving only the camera eye to a world position at an even speed over a number of frames.
// Original name: _CMRS_MOVE_POS.
extern void move_camera_pos(float x, float y, float z, int frames) NATIVE(435);

// Queues shaking only the camera look-at point by an offset (x, y, z in world units) that flips
// direction every frame and shrinks to nothing over a number of frames. With frames = -1 the shake
// keeps its full size and never ends until the camera sequence is cleared.
// Original name: _CMRS_QUAKE2.
extern void shake_camera_target(float x, float y, float z, int frames) NATIVE(436);

// Returns 1 while any track of the actor's sequence still has queued commands running, and 0 once
// every track has finished. Poll it in a loop with wait to hold the script until the actor is
// done.
// Original name: _OBJS_CHECK.
extern int check_actor_busy(int actor) NATIVE(450);

// Stops every track of the actor's sequence, drops its queued commands, forgets which event object
// it drives and clears its position and rotation. Any second argument is ignored.
// Original name: _OBJS_INIT.
extern void clear_actor_seq(int actor, OPTIONAL(any unused)) NATIVE(451);

// Makes the actor's sequence drive an event object handle, set up earlier by the event object
// handle natives. Most scripts pass the same number for both.
// Original name: _OBJS_SYNC_OBJ.
extern void bind_actor_object(int actor, int handle) NATIVE(452);

// Queues a wait of a number of frames on the actor's position track, so the next queued position
// command starts that much later.
// Original name: _OBJS_POS_DELAY.
extern void delay_actor_moves(int actor, int frames) NATIVE(453);

// Queues putting the actor at a world position at once. The call fails with any count other than
// four arguments.
// Original name: _OBJS_SET_POS.
extern void set_actor_pos(int actor, any x, float y, float z, OPTIONAL(any extra)) NATIVE(454);

// Queues moving the actor to a world position at an even speed over a number of frames. With
// on_ground = 1 the actor's height follows the ground under it while moving and at the end.
// Original name: _OBJS_MOVE.
extern void move_actor_to(int actor, float x, float y, float z, int frames, OPTIONAL(int on_ground)) NATIVE(455);

// Queues removing every point from the actor's walking path.
// Original name: _OBJS_INIT_PAS.
extern void clear_actor_path(int actor) NATIVE(456);

// Queues setting how many frames the actor takes to walk its whole path.
// Original name: _OBJS_SET_PAS_FRM.
extern void set_actor_path_frames(int actor, int frames) NATIVE(457);

// Queues adding a world position to the end of the actor's walking path, up to 16 points.
// Original name: _OBJS_ADD_PAS.
extern void add_actor_path_point(int actor, float x, float y, float z) NATIVE(458);

// Queues walking the actor along a spline through its path points over the path's frame count,
// turning it to face the direction of travel. With on_ground = 1 its height is also fitted to the
// ground each frame.
// Original name: _OBJS_START_PAS.
extern void start_actor_path(int actor, OPTIONAL(int on_ground)) NATIVE(459);

// Queues a wait of a number of frames on the actor's rotation track, so the next queued rotation
// command starts that much later.
// Original name: _OBJS_ROT_DELAY.
extern void delay_actor_turns(int actor, int frames) NATIVE(460);

// Queues setting the actor's rotation at once, in radians about the x, y and z axes.
// Original name: _OBJS_SET_ROT.
extern void set_actor_rot(int actor, float rot_x, float rot_y, float rot_z) NATIVE(461);

// Queues turning the actor to a rotation (radians) at an even speed over a number of frames. With
// a negative frame count it instead adds the rotation every frame and keeps spinning until the
// actor's sequence is cleared. The call fails without the frames argument.
// Original name: _OBJS_ROTATION.
extern void turn_actor_to(int actor, float rot_x, float rot_y, float rot_z, OPTIONAL(int frames)) NATIVE(462);

// Queues turning the actor to face a world position over a number of frames.
// Original name: _OBJS_REFERENCE.
extern void turn_actor_toward(int actor, float x, float y, float z, int frames) NATIVE(463);

// Queues a wait of a number of frames on the actor's motion track, so the next queued motion
// command starts that much later.
// Original name: _OBJS_MOTION_DELAY.
extern void delay_actor_motion(int actor, OPTIONAL(int frames)) NATIVE(464);

// Queues starting a named motion on the actor at once. flags: 1 holds the current frame, 2 stops
// on the last frame instead of looping, 4 starts from the first frame without blending. speed,
// when not -1, sets the playback step and restarts the motion from its first frame; apply_now = 1
// also steps the motion once and resets the actor's dynamic parts so the new pose shows
// immediately.
// Original name: _OBJS_SET_MOTION.
extern void set_actor_motion(int actor, string motion_name, OPTIONAL(enum chara_motion_flag flags, float speed, int apply_now)) NATIVE(465);

// Queues a named motion that starts once the actor's current motion ends. flags: 1 holds the
// current frame, 2 stops on the last frame instead of looping, 4 starts from the first frame
// without blending; speed, when not -1, sets the playback step.
// Original name: _OBJS_NEXT_MOTION.
extern void queue_actor_motion(int actor, string motion_name, OPTIONAL(enum chara_motion_flag flags, float speed, any extra5)) NATIVE(466);

// Queues holding the actor's motion track until its current motion ends.
// Original name: _OBJS_MOTION_WAIT.
extern void wait_actor_motion_end(int actor) NATIVE(467);

// Queues telling the actor's motion sequence to move on to its next part.
// Original name: _OBJS_SEQ_MOT_TRG.
extern void trigger_actor_motion_seq(int actor) NATIVE(468);

// Queues setting how fast the actor blends from one motion into the next; values of 0 or below
// become the default 0.2.
// Original name: _OBJS_CHENGE_STEP.
extern void set_actor_blend_speed(int actor, OPTIONAL(float step)) NATIVE(469);

// Queues a wait of a number of frames on the actor's texture animation track, so the next queued
// texture animation command starts that much later.
// Original name: _OBJS_TEXA_DELAY.
extern void delay_actor_texture_anim(int actor, OPTIONAL(int frames)) NATIVE(470);

// Queues turning a texture animation on the actor on or off. With on = 0 and no name it stops all
// of the actor's texture animations; with on = 1 it plays the named animation, such as a face or
// eye-blink loop.
// Original name: _OBJS_TEX_ANIME.
extern void set_actor_texture_anim(int actor, int on, OPTIONAL(string anim_name)) NATIVE(471);

// Queues holding the actor's motion track until its motion sequence reaches its trigger state
// (status 3).
// Original name: _OBJS_SEQ_MOT_TRG_WAIT.
extern void wait_actor_motion_trigger(int actor) NATIVE(472);

// Queues moving the actor to a world position along a jump arc over a number of frames. gravity
// shapes the arc; disc scripts pass small negative values such as -0.4.
// Original name: _OBJS_JUMP.
extern void jump_actor_to(int actor, float x, float y, float z, float gravity, int frames) NATIVE(473);

// Queues a wait of a number of frames on the actor's colour track, so the next queued colour
// change starts that much later.
// Original name: _OBJS_COLOR_DELAY.
extern void delay_actor_color(int actor, int frames) NATIVE(474);

// Queues changing the colour and alpha of the actor over a number of frames, or at once when
// frames is 0 or left out. Components run from 0 to 128, where 128 is normal; only event sprites
// take a colour.
// Original name: _OBJS_SET_COLOR.
extern void set_actor_color(int actor, float r, float g, float b, float a, OPTIONAL(int frames)) NATIVE(475);

// Queues putting the actor's motion back to its start.
// Original name: _OBJS_RESET_MOTION.
extern void reset_actor_motion(int actor) NATIVE(476);

// Queues a wait of a number of frames on the actor's scale track, so the next queued scale change
// starts that much later.
// Original name: _OBJS_SCALE_DELAY.
extern void delay_actor_scale(int actor, int frames) NATIVE(477);

// Queues changing the actor's scale on each axis over a number of frames, or at once when frames
// is 0 or left out. 1.0 is normal size.
// Original name: _OBJS_SET_SCALE.
extern void set_actor_scale(int actor, float scale_x, float scale_y, float scale_z, OPTIONAL(int frames)) NATIVE(478);

// Queues jumping the actor's current motion to a time, in motion frames from its start.
// Original name: _OBJS_SET_MOTION_NOW_TIME.
extern void set_actor_motion_time(int actor, float time) NATIVE(479);

// Queues jumping the actor's current motion to a share of its length, from 0.0 at the start to 1.0
// at the end.
// Original name: _OBJS_SET_MOTION_WAIT_TIME.
extern void set_actor_motion_ratio(int actor, float ratio) NATIVE(481);

// Queues setting how many motion frames the actor's motion advances each step; 1.0 is normal
// speed.
// Original name: _OBJS_SET_STEP.
extern void set_actor_motion_speed(int actor, float step) NATIVE(482);

// Queues keeping the actor at a named frame (bone) of another event object handle, plus an offset,
// for a number of frames: 0 places it once, -1 keeps it there until the sequence is cleared. Takes
// 3, 4, 6 or 7 arguments; with 4 the fourth is the frame count, with 6 or 7 the offset is x, y, z
// and the seventh is the frame count.
// Original name: _OBJS_SET_EOH_FRAME_POS.
extern void attach_actor_to_frame(int actor, int handle, string frame_name, OPTIONAL(any x_or_frames, float y, float z, int frames)) NATIVE(483);

// Queues adding an offset (world units) to the actor's position every frame for a number of
// frames, 1 when left out.
// Original name: _OBJS_ADD_POS.
extern void add_actor_pos(int actor, float x, float y, float z, OPTIONAL(int frames)) NATIVE(484);

// Queues a wait of a number of frames on the actor's sound effect track, so the next queued sound
// effect plays that much later.
// Original name: _OBJS_SE_DELAY.
extern void delay_actor_se(int actor, int frames) NATIVE(485);

// Queues playing a sound effect, by its number within a loaded sound bank, on the actor's sound
// effect track.
// Original name: _OBJS_SE_PLAY.
extern void play_actor_se(int actor, int sound, int se_no) NATIVE(486);

// Queues writing the actor's sequence position and rotation to its object and putting its dynamic-
// animation parts (swinging hair, cloth and the like) back to rest. Extra arguments are ignored.
// Original name: _OBJS_RESET_DA_POSITION.
extern void reset_actor_dynamics(int actor, OPTIONAL(any unused1, any unused2, any unused3)) NATIVE(487);

// Queues moving the actor to a world position over a number of frames with eased speed. ease is 0
// to speed up and slow down, 1 to speed up only, 2 to slow down only; ease_rate is the share of
// the frame count spent changing speed. The call fails with any count other than seven arguments.
// Original name: _OBJS_MOVE2.
extern void move_actor_eased(int actor, float x, float y, float z, int frames, enum scene_seq_ease ease, OPTIONAL(float ease_rate)) NATIVE(488);

// Queues keeping the actor at a distance in world units in front of the active camera eye, along
// its view line, for a number of frames; -1 keeps it there until the sequence is cleared.
// Original name: _OBJS_ATTACH_CAMERA.
extern void attach_actor_to_camera(int actor, float dist, int frames) NATIVE(489);

// Queues turning the actor to a rotation (radians) over a number of frames with eased speed. ease
// is 0 to speed up and slow down, 1 to speed up only, 2 to slow down only; ease_rate is the share
// of the frame count spent changing speed.
// Original name: _OBJS_ROTATION2.
extern void turn_actor_eased(int actor, float rot_x, float rot_y, float rot_z, int frames, enum scene_seq_ease ease, float ease_rate) NATIVE(490);

// Queues stepping the actor's motion once right away, so a newly set motion's pose shows without
// waiting for the next frame.
// Original name: _OBJS_NORMAL_DRIVE.
extern void step_actor_motion(int actor) NATIVE(491);

// Does nothing.
// Original name: _GET_CONTENTS_POS.
extern void get_contents_pos(void) NATIVE(501);

// Writes the world position of the pot or box the player carries and throws in dungeons into x, y
// and z.
// Original name: _GET_BPOT_POS.
extern void get_pot_pos(float *x, float *y, float *z) NATIVE(502);

// Returns the state of the carried pot: 0 none, 1 held, 2 thrown and flying.
// Original name: _GET_BPOT_STATUS.
extern int get_pot_state(void) NATIVE(503);

// Does nothing and reports failure; the result is left unchanged.
// Original name: _GET_PERSON_STATUS.
extern int get_person_status(int npc) NATIVE(504);

// Returns the number of the character the player controls in the event scene.
// Original name: _GET_CONTROL_CHRID.
extern int get_player_chara(void) NATIVE(505);

// Writes the eye position of the event scene's previous camera (the one set_prev_camera_id names)
// into x, y and z.
// Original name: _GET_BEFORE_CAMERA_POS.
extern void get_prev_camera_pos(float *x, float *y, float *z) NATIVE(506);

// Writes the look-at position of the event scene's previous camera (the one set_prev_camera_id
// names) into x, y and z.
// Original name: _GET_BEFORE_CAMERA_REF.
extern void get_prev_camera_ref(float *x, float *y, float *z) NATIVE(507);

// Gives the active follow camera a look-at position it moves toward.
// Original name: _SET_CAMERA_NEXT_REF.
extern void set_camera_next_ref(float x, float y, float z) NATIVE(508);

// Asks the game to open a menu once the event yields. menu_type picks it, such as 6 shop, 7 save,
// 16 or 17 the item menu for overflowing items, 25 the monster book; any further arguments are
// passed to that menu as parameters.
// Original name: _GOTO_MENU.
extern void open_menu(enum menu_open_type menu_type, OPTIONAL(int param, ...)) NATIVE(509);

// Does nothing and reports failure.
// Original name: _GET_MENU_STATUS.
extern void get_menu_status(void) NATIVE(510);

// Loads the model of a party member's current equipment for an event character, using memory stack
// stack_no. member is 0 or 1; slot 0 to 2 loads the equipment as character attach_chara and
// attaches it to the matching bone of chara, slot 3 or 4 loads it as a skin onto chara.
// Original name: _LOAD_EQUIP.
extern void load_equip_model(enum memory_stack stack_no, enum user_chara member, enum equip_slot slot, string cfg_name, int chara, OPTIONAL(int attach_chara)) NATIVE(511);

// Returns the item number in an equipment slot (0 to 4; 0 and 1 are the weapons) of party member 0
// or 1.
// Original name: _GET_EQUIP_ITEMNO.
extern int get_equip_item(enum user_chara member, enum equip_slot slot) NATIVE(512);

// Turns the passing of the time of day in the event scene on (1) or off (0).
// Original name: _SET_TIME_STEP_ENABLE.
extern void set_time_flow(int on) NATIVE(513);

// Does nothing.
// Original name: _SET_DOOR_MATERIAL.
extern void set_door_material(void) NATIVE(514);

// Lets the player skip the current cutscene, fading to black when skipped.
// Original name: _INIT_DRAMA_SCENE.
extern void enable_scene_skip(void) NATIVE(515);

// Sets which camera number of the event scene is the active one.
// Original name: _SET_ACTIVE_CMRID.
extern void set_active_camera_id(int camera) NATIVE(516);

// Sets which camera number of the event scene counts as the previous camera, the one the
// get_prev_camera natives read.
// Original name: _SET_BEFORE_CMRID.
extern void set_prev_camera_id(int camera) NATIVE(517);

// Loads a dungeon map for display in an event into memory stack stack_no, from event texture file
// tex_index, and builds the path the player's piece moves along from room to next_room. The map
// starts hidden.
// Original name: _DNGMAP_LOAD.
extern void load_event_dng_map(enum memory_stack stack_no, int tex_index, int dungeon, int room, int next_room) NATIVE(519);

// Frees the textures of the event dungeon map.
// Original name: _DNGMAP_DELETE.
extern void unload_event_dng_map(void) NATIVE(520);

// Starts (1) or stops (0) the player's piece moving along its path on the event dungeon map.
// Original name: _DNGMAP_MOVE_PIECE.
extern void move_dng_map_piece(int moving) NATIVE(521);

// Shows (non-zero) or hides (0) the event dungeon map.
// Original name: _DNGMAP_ONOFF.
extern void show_event_dng_map(int on) NATIVE(522);

// Shows the event dungeon map and fades it in (fade_in = 1) or out (fade_in = 0) over a number of
// frames.
// Original name: _DNGMAP_SET_FADE.
extern void fade_event_dng_map(int fade_in, int frames) NATIVE(523);

// Writes the eye position the event scene's previous camera is moving toward into x, y and z.
// Original name: _GET_BEFORE_CAMERA_NEXT_POS.
extern void get_prev_eye_goal(float *x, float *y, float *z) NATIVE(524);

// Writes the look-at position the event scene's previous camera is moving toward into x, y and z.
// Original name: _GET_BEFORE_CAMERA_NEXT_REF.
extern void get_prev_ref_goal(float *x, float *y, float *z) NATIVE(525);

// Gives the active follow camera an eye position it moves toward.
// Original name: _SET_CAMERA_NEXT_POS.
extern void set_camera_goal_pos(float x, float y, float z) NATIVE(526);

// Casts a line segment from the from point to the to point against the map's collision polygons,
// skipping polygons whose ignore bits overlap ignore_mask, and also against treasure boxes when
// check_boxes is 1. Here hit is the index of the nearest polygon hit or -1 for none, and kind is
// that polygon's area kind. The outputs depend on the argument count: 9 writes hit; 10 writes hit,
// kind; 12 writes hit x, y, z, hit; 13 adds kind; 15 writes hit x, y, z, the normalized reflection
// direction x, y, z, then hit; 16 adds kind.
// Original name: _CHK_INTERSECTION_POINT.
extern void check_line_hit(int ignore_mask, float from_x, float from_y, float from_z, float to_x, float to_y, float to_z, int check_boxes, any *result1, OPTIONAL(any *result2, any *result3, any *result4, any *result5, any *result6, any *result7, any *result8)) NATIVE(527);

// Sets the world point the active follow camera looks at and trails. The camera moves toward it as
// it steps, with the follow offset added on top.
// Original name: _SET_FCAMERA_FOLLOW.
extern void set_follow_pos(float x, float y, float z) NATIVE(528);

// Sets the world point the active follow camera looks at and trails. The code is identical to
// set_follow_pos.
// Original name: _SET_FCAMERA_FOLLOW_A.
extern void set_follow_pos_a(float x, float y, float z) NATIVE(529);

// Sets an offset in world units that the active follow camera adds to its follow point.
// Original name: _SET_FCAMERA_FOLLOW_OFS.
extern void set_follow_offset(float x, float y, float z) NATIVE(530);

// Turns following on (1) or off (any other value) for the active follow camera. While off, the
// camera stays where it is.
// Original name: _SET_FCAMERA_FOLLOW_FLAG.
extern void set_camera_follow(int on) NATIVE(531);

// Advances the active follow camera by a number of frames toward its follow point and target
// angle. A negative value snaps it there at once.
// Original name: _FCAMERA_STEP.
extern void step_follow_camera(int frames) NATIVE(532);

// Sets the angle in radians around the follow point that the active follow camera turns toward.
// The camera eases to it as it steps.
// Original name: _SET_FCAMERA_ANGLE.
extern void set_follow_angle(float angle) NATIVE(533);

// Sets how far above the follow point the active follow camera sits, in world units.
// Original name: _SET_FCAMERA_HEIGHT.
extern void set_follow_height(float height) NATIVE(534);

// Sets how far the active follow camera stays from its follow point, in world units.
// Original name: _SET_FCAMERA_DIST.
extern void set_follow_distance(float distance) NATIVE(535);

// Computes the direction from the first point to the second. With 7 arguments the last receives
// the yaw in radians, atan2(dx, dz). With 9 arguments the last three receive a rotation of pitch,
// yaw and 0.
// Original name: _GET_REF_ANGLE.
extern void get_direction_angle(float from_x, float from_y, float from_z, float to_x, float to_y, float to_z, float *yaw, OPTIONAL(float *rot_y, float *rot_z)) NATIVE(536);

// Stores a stage number in the dungeon record of the save data.
// Original name: _DNG_SET_STAGE_ID.
extern void set_dungeon_stage(int stage) NATIVE(537);

// Returns the stage number stored in the dungeon record of the save data.
// Original name: _DNG_GET_STAGE_ID.
extern int get_dungeon_stage(void) NATIVE(538);

// Turns the player's camera control on or off for the active camera. Nonzero enables both
// following and pad control; 0 disables both so a script can drive the camera.
// Original name: _SET_CAMERA_CTRL.
extern void set_camera_control(int on) NATIVE(539);

// Returns the current angle in radians of the active follow camera around its follow point.
// Original name: _GET_FCAMERA_ANGLE.
extern float get_follow_angle(void) NATIVE(540);

// Returns the height of the active follow camera above its follow point, in world units.
// Original name: _GET_FCAMERA_HEIGHT.
extern float get_follow_height(void) NATIVE(541);

// Returns the distance of the active follow camera from its follow point, in world units.
// Original name: _GET_FCAMERA_DIST.
extern float get_follow_distance(void) NATIVE(542);

// Looks up an item in the list of inventions the player has made. Returns its position in that
// list, or -1 when it has not been invented.
// Original name: _GET_INVENTION_ID.
extern int find_invention_index(item_id item) NATIVE(543);

// Carries out the map jump stored on the scene when one is pending: it leaves the current interior
// when the stored target is the exit name, otherwise it enters the named interior map. Fails when
// no jump is pending.
// Original name: _FUNCTION_MAP_JUMP.
extern void run_pending_map_jump(void) NATIVE(544);

// Hands control to the door sequence for the door the scene last touched, passing that door's
// place numbers, position and facing angle.
// Original name: _FUNCTION_DOOR_MODE.
extern void start_door_mode(void) NATIVE(545);

// Returns how much money the player has.
// Original name: _GET_MONEY.
extern int get_money(void) NATIVE(546);

// Adds money to the player's purse. A negative amount takes money away.
// Original name: _ADD_MONEY.
extern void add_money(int amount) NATIVE(547);

// Returns how many of an item the player holds.
// Original name: _GET_ITEM_NUM.
extern int get_item_count(item_id item) NATIVE(548);

// Checks whether a pad button went down this frame. button 0 is cancel and 1 is confirm (cross and
// circle, swapped for Japanese), 2 is square and 3 is select. Returns 1 or 0.
// Original name: _CHECK_BUTTON.
extern int check_button_pressed(enum check_button button) NATIVE(549);

// Returns the game language: 0 Japanese, 1 English, 2 French, 3 German, 4 Italian, 5 Spanish.
// Original name: _GET_LANGUAGE.
extern int get_language(void) NATIVE(550);

// Checks whether the player has the three ideas an invention item needs, from photos or memo
// entries. Returns 1 when all three are present, else 0.
// Original name: _CHECK_INVENT_ITEM.
extern int check_invention_ideas(item_id item) NATIVE(551);

// Lets a registered villager move on its own again (on = 1) or holds it in place (on = 0). chara
// is the scene character slot the villager was registered in.
// Original name: _SET_AI.
extern void set_villager_ai(int on, int chara) NATIVE(552);

// Counts the photos in the player's photo slots that show a subject. Called as (subject, &count)
// the subject is an idea number; with three arguments kind picks what subject means: 0 idea
// number, 2 NPC number, 3 monster number.
// Original name: _CHECK_INVENT_PHOTO.
extern void count_photos_of(int subject, enum photo_subject_kind kind, OPTIONAL(int *count)) NATIVE(553);

// Returns how many photo slots hold a photo.
// Original name: _GET_PHOTO_NUM.
extern int get_photo_count(void) NATIVE(554);

// Does nothing.
// Original name: _SET_CONTENTS_ETC.
extern void set_contents_misc(void) NATIVE(555);

// Sets or clears status bits on a scene entry. kind picks the list: 1 character, 2 map, 3 message,
// 4 camera, 6 or 7 game object; on defaults to 1, and 0 clears the bits instead. bits is always
// read, so a two-argument call reads a stray stack value.
// Original name: _SET_STATUS.
extern void set_scene_status(enum scene_data_kind kind, int index, OPTIONAL(enum scene_status bits, int on)) NATIVE(556);

// Starts a minigame: 1 fishing, 2 fish race, 3 buggy game.
// Original name: _GOTO_SUBGAME.
extern void start_subgame(enum subgame_type type) NATIVE(557);

// Sets fish race state by command: 0 aquarium number, 1 ranking, 2 class, 3 race progress, each
// from value; 4 adds 1 to the tournament count; 7 reloads the race prizes. Commands 5 and 6 are
// called as (command, message, name_slot, entry) and copy that race entry's fish name (5) or race
// time as mm:ss:hh text (6) into the message's name slot, counted from 1.
// Original name: _SET_GYORACE_ETC.
extern void set_fish_race_value(enum gyorace_set_command command, OPTIONAL(any value, int name_slot, int entry)) NATIVE(558);

// Reads fish race state into value by command: 0 aquarium number, 1 ranking, 2 class, 3 race
// progress, 5 tournament count. Command 4 is called as (4, race, place, &prize_item, &prize_count)
// and reads the prize for a place counted from 1.
// Original name: _GET_GYORACE_ETC.
extern void get_fish_race_value(enum gyorace_get_command command, any value, OPTIONAL(int place, int *prize_item, int *prize_count)) NATIVE(559);

// Changes save data by command: 0 sets the game progress to value; 1 unlocks the badge of monster
// value; 2 repairs every ridepod part and refills its energy; 3 sets a flag that makes the next
// load_bgm do nothing; 4 removes the first electric fish from the aquarium. A third argument is
// ignored.
// Original name: _SET_SAVEDATA_ETC.
extern void set_save_value(enum save_set_command command, OPTIONAL(int value, int unused)) NATIVE(560);

// Reads save data into value by command: 0 game progress, 2 whether the first aquarium tank holds
// an electric fish, 4 play time in frames, 5 the caption-off setting, 6 the current fishing
// tournament kind. Commands 1 and 3 are called as (command, input, &result): 1 gives whether the
// badge of monster input is unlocked, 3 the bait item on fishing rod input.
// Original name: _GET_SAVEDATA_ETC.
extern void get_save_value(enum save_get_command command, any value, OPTIONAL(int *result)) NATIVE(561);

// Removes every dungeon monster and clears ridepod projectiles, then resets monster memory,
// collision, effects, monster textures and the monster sound port. The argument is not read; the
// disc passes -1.
// Original name: _DEL_MONSTER.
extern void clear_monsters(any unused) NATIVE(562);

// Menu helpers by command: 0 (prompt, answer, code) sets the text a name entry checks against; 1
// clears fish fatigue and ends the current fishing tournament; 3 shows (1) or hides (0) the
// Geostone parts announcement; 4 resets the loading bar; 5 (stack, town) builds a town's Geostone
// announcement in a scene memory stack and starts the download effect; 7 clears the announcement.
// Commands 2 and 6 do nothing.
// Original name: _SET_MENU_ETC.
extern void set_menu_value(enum menu_set_command command, OPTIONAL(any arg1, any arg2, any arg3)) NATIVE(563);

// Menu queries by command, written to value: 0 floors with a cleared spheda challenge, 1 the town
// square event type, 3 scoops newly marked known, 4 scoops newly marked obtained, 8 1 once the
// Geostone download effect has ended. 2 (fish, &size) gives a fish's record size; 5 (&obtained,
// &total) the scoop totals; 6 (&count, &status) Donny's shop lineup count and status; 7 (confirm,
// &done) steps the Geostone announcement and gives 1 when it has ended.
// Original name: _GET_MENU_ETC.
extern void get_menu_value(enum menu_get_command command, any value, OPTIONAL(any *value2)) NATIVE(564);

// Checks one town analysis entry. analysis_id is town * 100 + entry, with towns counted from 1,
// and the result is whether all of the entry's conditions hold.
// Original name: _GET_ANALYZE.
extern int get_analysis_flag(int analysis_id) NATIVE(565);

// Returns a town's Georama completion percentage, the sum of the percent of every analysis entry
// that is met. town counts from 0.
// Original name: _GET_DIORAMA_PERCENT.
extern int get_town_percent(int town) NATIVE(566);

// Georama helpers by command: 1 (slot) loads the current villager's model into that character slot
// at the town's start point; 2 reloads the NPC living in the placed house into slot 8; 3
// (&can_burn) gives whether any placed part can burn.
// Original name: _GEORAMA_FUNC.
extern void run_georama_command(int command, OPTIONAL(any value)) NATIVE(567);

// Searches scene character slots 8 to 63 for the character with this number. Returns the slot, or
// -1 when none matches.
// Original name: _GET_CHARA_ID.
extern int find_chara_slot(int chara_no) NATIVE(568);

// Starts a background load of the current dungeon's monster talk message file in the current
// language.
// Original name: _SET_LOADBG_FILE_MONS_TALK.
extern void load_monster_talk_file(void) NATIVE(569);

// Copies the loaded monster talk file into a message window's text data, allocating from a scene
// memory stack. Use after load_monster_talk_file has finished loading.
// Original name: _LOAD_MES_MONS_TALK.
extern void load_monster_talk_message(int message, enum memory_stack stack) NATIVE(570);

// Leaves the event for Georama edit mode. The argument is not read.
// Original name: _GOTO_EDITMODE.
extern void start_georama_mode(any unused) NATIVE(571);

// Returns the story chapter derived from the game progress: 0 below progress 2, 1 at progress 2 or
// 3, otherwise progress minus 2.
// Original name: _GET_CHAPTER.
extern int get_chapter(void) NATIVE(572);

// Returns a value from a built-in table of NPC train data. npc picks the row, counted from 1, and
// column the value; scripts use column 1 for a seat and 2 for an x offset.
// Original name: _GET_NPC_TRAIN_ETC.
extern int get_npc_train_value(int column, int npc) NATIVE(573);

// Registers a villager in a scene character slot. With a place number it starts at that villager
// place's position and facing; without, a blank place is allocated from scene memory stack 4.
// Original name: _REGISTER_VILLAGER.
extern void register_villager(chara_slot slot, npc_id villager, OPTIONAL(int place)) NATIVE(574);

// Switches between two named part groups on the active maps for the first-person view. Nonzero
// shows the first group and hides the second; 0 does the reverse.
// Original name: _EYE_VIEW_DRAW_ON_OFF.
extern void set_eye_view_parts(int on) NATIVE(575);

// Changes a quest request by command: 0 sets its accepted flag to flag, 1 marks it cleared and
// ignores flag.
// Original name: _SET_QUEST_ETC.
extern void set_quest_state(enum quest_set_command command, int quest, int flag) NATIVE(576);

// Returns a quest request's state; command must be 0. The result is -1 invalid, 0 not taken, 1
// accepted, 2 cleared.
// Original name: _GET_QUEST_ETC.
extern int get_quest_state(enum quest_get_command command, int quest) NATIVE(577);

// Like check_line_hit but sweeps a pipe of the given radius from the from point to the to point,
// collecting up to 16 hits. Here hit is the number of hits and the position, reflection and kind
// come from the first. The outputs depend on the argument count: 10 writes hit; 11 writes hit,
// kind; 13 writes hit x, y, z, hit; 14 adds kind; 16 writes hit x, y, z, reflection x, y, z, hit;
// 17 adds kind.
// Original name: _CHK_INTERSECTION_POINT_PIPE.
extern void check_pipe_hit(int ignore_mask, float from_x, float from_y, float from_z, float to_x, float to_y, float to_z, float radius, int check_boxes, any *result1, OPTIONAL(any *result2, any *result3, any *result4, any *result5, any *result6, any *result7, any *result8)) NATIVE(578);

// Returns the number of the interior map the player last left, or -1 while still inside an
// interior.
// Original name: _GET_OLD_INTERIOR_MAP_NO.
extern int get_last_interior_map(void) NATIVE(579);

// Writes one field of the scene's current event data: 0 flag, 1 event number, 2 point number, 3 to
// 5 event arguments 1 to 3, 6 map event check type, 7 map event number, 13 character slot, 14
// character number.
// Original name: _SET_EVENT_DATA.
extern void set_event_data(enum event_data_field field, int value) NATIVE(581);

// Stopwatch by command: 0 starts timing against the play time; 1 stops it and writes minutes,
// seconds, hundredths, and 1 when under 7261 frames, or -1 into value1 if it was never started. 2
// (frames) sets the time limit; 3 (x, y, style) sets the screen position and display style.
// Original name: _STOPWATCH.
extern void run_stopwatch(enum stopwatch_command command, OPTIONAL(any *value1, any *value2, any *value3, int *value4)) NATIVE(582);

// Asks the event loop to act on the town's edit event: 0 sets a scene map flag and restarts it, 1
// resets it.
// Original name: _SET_FUNC_ETC.
extern void request_edit_event(enum edit_event_command command) NATIVE(583);

// Fishing tournament queries by command: 0 (message) writes the top-ten record table into a
// message window; 2, 3 and 4 write the free entry places, the total weight of the three heaviest
// fish, and the rank into value. 1 is called as (1, place, &prize_item, &prize_count) and reads
// the prize for a place counted from 1.
// Original name: _GET_FISHINGTOURNAMENT_ETC.
extern void get_fishing_tournament_value(enum fish_tour_get_command command, any value, OPTIONAL(int *prize_item, int *prize_count)) NATIVE(584);

// Sets the camera distance beyond which a character is not drawn, in world units, on the character
// and all its linked parts.
// Original name: _SET_CHARA_FAR_DIST.
extern void set_chara_draw_distance(chara_slot chara, float distance) NATIVE(585);

// Turns scene lighting on or off for a character's model, or for one named frame of it. Off draws
// it flat with neutral color 128.
// Original name: _SET_MODEL_LIGHT_SWITCH.
extern void set_chara_lighting(chara_slot chara, int on, OPTIONAL(string frame_name)) NATIVE(586);

// Sets the flat color of a character's model, or of one named frame of it. Each channel runs 0 to
// 255 with 128 as neutral.
// Original name: _SET_MODEL_LIGHT_COLOR.
extern void set_chara_flat_color(chara_slot chara, float r, float g, float b, float a, OPTIONAL(string frame_name)) NATIVE(587);

// Returns the extra mode flag, nonzero while a bonus mode started from the title screen is being
// played.
// Original name: _GET_OMAKE_FLAG.
extern int get_extra_mode(void) NATIVE(588);

// Sets the scene wind used by swinging model parts, as a strength and a direction vector that is
// normalized. A negative power removes the wind, and the direction can then be left out.
// Original name: _SET_WIND.
extern void set_wind(float power, OPTIONAL(float dir_x, float dir_y, float dir_z)) NATIVE(589);

// Sets the extra mode flag that marks a bonus mode started from the title screen.
// Original name: _SET_OMAKE_FLAG.
extern void set_extra_mode(int flag) NATIVE(590);

// Stops every sound on a sound port and reinitializes it. Ports: 0 music, 1 objects, 2 ambient, 3
// base map, 4 event, 5 monsters, 6 system, 8 menu, 9 battle, 11 second music.
// Original name: _SND_INIT_PORT.
extern void reset_sound_port(enum snd_port port) NATIVE(600);

// Loads a sound bank onto a port and gives its sound id. Called as (port, file, &sound_id) or as
// (file, &sound_id) for port 4; file is a path string, or an int sound data number whose event
// sound file is used. Port 8 uses the second event sound buffer.
// Original name: _SND_LOAD_SOUND.
extern void load_sound(enum snd_port port, any file, OPTIONAL(int *sound_id)) NATIVE(601);

// Plays sound effect se_no from a loaded sound bank. volume runs 0 to 127 and pan 0 to 127 with 64
// centered; without them the bank defaults are used.
// Original name: _SND_SE_PLAY.
extern void play_se(int sound_id, int se_no, OPTIONAL(int volume, int pan)) NATIVE(602);

// Reads the sound id of a scene sound bank: 0 ambient, 1 to 3 base map, 4 battle. Bank 5 is called
// as (5, source_no, &source_sound_id) for a sound source; with one argument the id of the last
// loaded sound is written.
// Original name: _GET_SND_ID.
extern void get_sound_id(enum se_bank bank, int *sound_id, OPTIONAL(int *source_sound_id)) NATIVE(603);

// Pauses sound effect se_no of a loaded sound bank.
// Original name: _SND_SE_PAUSE.
extern void pause_se(int sound_id, int se_no) NATIVE(604);

// Stops sound effect se_no of a loaded sound bank.
// Original name: _SND_SE_STOP.
extern void stop_se(int sound_id, int se_no) NATIVE(605);

// Sets the volume of a playing sound effect. An int is an absolute volume from 0 to 127; a float
// scales the effect's default volume, capped at 127.
// Original name: _SND_SET_SE_VOL.
extern void set_se_volume(int sound_id, int se_no, any volume) NATIVE(606);

// Sets the pan of sound effect se_no in a sound bank, 0 to 127 with 64 centered.
// Original name: _SND_SET_SE_PAN.
extern void set_se_pan(int sound_id, int se_no, int pan) NATIVE(607);

// Sets the pitch of sound effect se_no in a sound bank.
// Original name: _SND_SET_SE_PITCH.
extern void set_se_pitch(int sound_id, int se_no, int pitch) NATIVE(608);

// Stops every sound effect on a sound port; -1 stops all ports except music.
// Original name: _SND_SE_ALL_STOP.
extern void stop_all_se_on_port(enum snd_port port) NATIVE(609);

// Loads a music file by number into the scene's music slot. Does nothing when that file is already
// loaded, or once after set_save_value command 3 set the skip flag.
// Original name: _LOAD_BGM.
extern void load_bgm(bgm_file bgm_file) NATIVE(610);

// Plays a track of the loaded music file, stopping the previous one; scripts use track 0. volume
// runs 0 to 127, and -1 or leaving it out uses the default.
// Original name: _PLAY_BGM.
extern void play_bgm(int track, OPTIONAL(int volume)) NATIVE(611);

// Stops a track of the loaded music file.
// Original name: _STOP_BGM.
extern void stop_bgm(int track) NATIVE(612);

// Starts opening the event voice stream. An int voice is a voice number found in the voice packs
// and a string is a stream file name; with three arguments voice is a pack name and base_name a
// file inside it. The channel is not used; poll check_stream_busy until it finishes.
// Original name: _STREAM_OPEN.
extern void open_stream(enum stream_channel channel, any voice, OPTIONAL(string base_name)) NATIVE(613);

// Plays the opened event voice stream. Without a volume the maximum stream volume is used; the
// volume is lowered by the current reverb depth. The channel is not used.
// Original name: _STREAM_PLAY.
extern void play_stream(enum stream_channel channel, OPTIONAL(int volume)) NATIVE(614);

// Stops and closes the event voice stream. The channel is not used.
// Original name: _STREAM_STOP.
extern void stop_stream(enum stream_channel channel) NATIVE(615);

// Sets up the opened event voice stream for mono or stereo and starts preloading it. The channel
// is not used.
// Original name: _STREAM_STANDBY.
extern void prepare_stream(enum stream_channel channel) NATIVE(616);

// Queues a camera sequence command that keeps the camera orbiting an actor at the actor's position
// plus the offset, at an angle in radians, a height and a distance in world units. frame_name
// follows that frame of the model instead (empty for the actor itself); mode 0 keeps the angle
// fixed, 1 turns with the actor's heading, 2 follows the frame's full orientation.
// Original name: _CMRS_SYNC_OBJ.
extern void attach_camera_to_actor(int actor, float ofs_x, float ofs_y, float ofs_z, float angle, float height, float distance, OPTIONAL(string frame_name, enum scene_cmr_sync_mode mode)) NATIVE(617);

// Queues a camera sequence command that stops the camera orbiting an actor.
// Original name: _CMRS_RELEASE_OBJ.
extern void detach_camera_from_actor(void) NATIVE(618);

// Returns the state of the event voice stream, 0 when it is idle or finished. The channel is not
// used.
// Original name: _STREAM_GET_STATUS.
extern int get_stream_status(enum stream_channel channel) NATIVE(619);

// Returns the sound id of the system sound bank, for use with play_se.
// Original name: _GET_SYS_SND_ID.
extern int get_system_sound_id(void) NATIVE(620);

// Checks whether a stream request is still in progress. Returns nonzero until open_stream or
// prepare_stream has finished.
// Original name: _STREAM_OPEN_CHECK.
extern int check_stream_busy(void) NATIVE(621);

// Loads an ambient sound bank by number, unless it is already loaded.
// Original name: _LOAD_SE_ENV.
extern void load_env_sound(int bank) NATIVE(622);

// Plays an ambient loop from the ambient sound bank, stopping the previous one. -1 switches back
// to the automatic ambient sound at full volume. volume is truncated to a whole number, so only 0
// and 1 are useful.
// Original name: _PLAY_ENV_BGM.
extern void play_env_sound(int se_no, OPTIONAL(float volume)) NATIVE(623);

// Plays a sound effect from the system sound bank. Negative numbers do nothing.
// Original name: _SYS_SE_PLAY.
extern void play_system_se(system_se se_no) NATIVE(624);

// Stops and unloads all positional sound sources and resets the event and object sound ports.
// Original name: _INIT_SE_SRC.
extern void reset_source_sounds(void) NATIVE(625);

// Stops the ambient loop, unloads the ambient sound bank and resets the sound sources.
// Original name: _INIT_SE_ENV.
extern void reset_env_sounds(void) NATIVE(626);

// Stops and unloads the base map sound bank and the battle sound bank.
// Original name: _INIT_SE_BAS.
extern void reset_base_sounds(void) NATIVE(627);

// Loads a positional sound source bank by number, unless it is already loaded.
// Original name: _LOAD_SE_SRC.
extern void load_source_sound(int bank) NATIVE(628);

// Does nothing and reports failure.
// Original name: _LOAD_SE_FOOT.
extern void load_footstep_sound(void) NATIVE(629);

// Does nothing and reports failure.
// Original name: _LOAD_SE_DOOR.
extern void load_door_sound(void) NATIVE(630);

// Does nothing and reports failure.
// Original name: _LOAD_SE_BOX.
extern void load_box_sound(void) NATIVE(631);

// Loads a battle sound bank by number, unless it is already loaded.
// Original name: _LOAD_SE_BATTLE.
extern void load_battle_sound(int bank) NATIVE(632);

// Releases a sound port in the sound driver, freeing what was loaded on it, and reinitializes the
// port.
// Original name: _SND_DELETE_PORT.
extern void delete_sound_port(enum snd_port port) NATIVE(633);

// Stops the ambient loop.
// Original name: _STOP_ENV_BGM.
extern void stop_env_sound(void) NATIVE(634);

// Fades the music in from silence over a number of frames.
// Original name: _FADE_IN_BGM.
extern void fade_in_bgm(int frames) NATIVE(635);

// Fades the music out over a number of frames.
// Original name: _FADE_OUT_BGM.
extern void fade_out_bgm(int frames) NATIVE(636);

// Sets the volume of the playing music. An int sets it directly (0 to 127, negative restores the
// bank's default volume); a float multiplies the current volume by that factor.
// Original name: _SET_BGM_VOL.
extern void set_bgm_volume(any volume) NATIVE(637);

// Does nothing and reports failure. The engine leaves this native empty.
// Original name: _SND_SET_REVERB.
extern void set_sound_reverb(void) NATIVE(638);

// Sets the volume of the map's ambient environment sound loop, as a factor from 0.0 to 1.0.
// Original name: _SND_SET_ENV_VOL.
extern void set_env_bgm_volume(float volume) NATIVE(639);

// Reads the output level of the event stream and returns 1 when either channel is outside -10 to
// 10, 0 when both are near silence. The channel argument is ignored; the event stream is always
// used.
// Original name: _STREAM_SILENT_CHECK.
extern int check_stream_audible(enum stream_channel channel) NATIVE(640);

// Turns on (1) or off (0) the environment sound following the time band of the map.
// Original name: _AUTO_CHANGE_ENV.
extern void set_env_bgm_auto(int on) NATIVE(641);

// Makes the next scene music load be skipped once, keeping the music already loaded. The flag
// clears itself when used.
// Original name: _BGM_LOAD_CANCEL.
extern void skip_next_bgm_load(void) NATIVE(642);

// Makes the next sound set load (load_sound_set or a map change) be skipped once. The flag clears
// itself when used.
// Original name: _SOUND_LOAD_CANCEL.
extern void skip_next_sound_load(void) NATIVE(643);

// Clears the flag set by skip_next_bgm_load so the next music load happens normally.
// Original name: _BGM_LOAD_ENABLE.
extern void allow_bgm_load(void) NATIVE(644);

// Clears the flag set by skip_next_sound_load so the next sound set load happens normally.
// Original name: _SOUND_LOAD_ENABLE.
extern void allow_sound_load(void) NATIVE(645);

// Loads the base sound effect bank with the given number, unless that bank is already loaded or
// the number is negative. Reports failure in those cases.
// Original name: _LOAD_SE_BASE.
extern void load_se_base(int bank_no) NATIVE(646);

// Loads everything listed for a sound data id: the base, battle, environment and source effect
// banks, the reverb setting and the environment sound loop. Banks already loaded are kept. Skipped
// once after skip_next_sound_load.
// Original name: _LOAD_SOUND.
extern void load_sound_set(sound_set sound_data_id) NATIVE(647);

// Stops and closes the event stream, restoring its saved volume. The channel argument is ignored;
// the event stream is always used.
// Original name: _STREAM_CLOSE.
extern void close_stream(enum stream_channel channel) NATIVE(648);

// Marks the event stream as reading from a file name, but the engine only builds the path and
// never opens the file. The channel argument is ignored.
// Original name: _STREAM_OPEN2.
extern void open_stream_named(enum stream_channel channel, string file_name) NATIVE(649);

// Makes the given music number the loaded music pack, taking the file from the preloaded
// background buffers. Does nothing when that number is already loaded or negative, and fails when
// the file was not preloaded.
// Original name: _LOAD_BGM_PACK.
extern void load_bgm_pack(bgm_file bgm_no) NATIVE(650);

// Returns the number of the currently loaded music pack.
// Original name: _GET_BGM_NO.
extern int get_bgm_no(...) NATIVE(651);

// Returns the master volume factor of the active music channel.
// Original name: _GET_MASTER_VOL.
extern float get_bgm_master_volume(void) NATIVE(652);

// Sets the master volume factor of the active music channel and reapplies the music volume. 1.0 is
// normal.
// Original name: _SET_MASTER_VOL.
extern void set_bgm_master_volume(float volume) NATIVE(653);

// Returns the volume the dungeon battle music is played at.
// Original name: _GET_BTL_BGM_VOL.
extern float get_battle_bgm_volume(void) NATIVE(654);

// Sets the volume the dungeon battle music is played at.
// Original name: _SET_BTL_BGM_VOL.
extern void set_battle_bgm_volume(float volume) NATIVE(655);

// Sets a sound processor parameter on both cores that the menus also use when switching reverb:
// nonzero on sets it to -4, 0 sets it to -52.
// Original name: _SND_IN_REVERB.
extern void set_reverb_input(int on) NATIVE(656);

// Stops every sound effect still playing from the scene's loaded source effect banks.
// Original name: _SND_STOP_SRC.
extern void stop_source_se(void) NATIVE(657);

// Pauses the music playing on the active music channel.
// Original name: _SND_PAUSE_BGM.
extern void pause_bgm(void) NATIVE(658);

// Opens the numbered stream file (number printed as seven digits plus .wav, such as 5400015.wav)
// on the event stream for playback. The channel argument is ignored; the event stream is always
// used.
// Original name: _STREAM_OPEN3.
extern void open_stream_wav(enum stream_channel channel, int stream_no) NATIVE(659);

// Saves the active music's state (playing, paused or stopped, track, volumes, loaded pack) into a
// single save slot that restore_bgm_status reads. The argument is ignored.
// Original name: _GET_ACTIVE_BGM_STATUS.
extern void save_bgm_status(int unused) NATIVE(660);

// Restores the music state saved by save_bgm_status: track, volumes, and whether it plays, is
// paused or is stopped. The argument is ignored.
// Original name: _SET_ACTIVE_BGM_STATUS.
extern void restore_bgm_status(int unused) NATIVE(661);

// Returns the music pack number stored by the last save_bgm_status.
// Original name: _GET_BGM_STATUS_NOW_NO.
extern int get_saved_bgm_no(void) NATIVE(662);

// Returns the playback state of a sequence sound: 0 stopped, 1 playing, 2 paused, 3 port paused,
// or -1 when the sound id or number is invalid or the sound is not a sequence.
// Original name: _GET_SE_STATUS.
extern int get_se_status(int sound_id, int se_no) NATIVE(663);

// Stops every sound effect, including source effects and looping effects, and resets the looping
// effect manager.
// Original name: _SE_ALL_STOP.
extern void stop_all_se(void) NATIVE(664);

// Stops the music, resets the music channel and stops every sound effect.
// Original name: _SOUND_ALL_STOP.
extern void stop_all_sound(void) NATIVE(665);

// Makes the next music play request be ignored once. The flag clears itself when used.
// Original name: _BGM_PLAY_CANCEL.
extern void skip_next_bgm_play(void) NATIVE(666);

// Clears the flag set by skip_next_bgm_play so the next music play request happens normally.
// Original name: _BGM_PLAY_ENABLE.
extern void allow_bgm_play(void) NATIVE(667);

// Returns the default music number of a map's sound data, or -1 when it has none. Called without
// map, it uses the current map (or sub map when one is active).
// Original name: _GET_DEF_BGM_NO.
extern int get_map_default_bgm(map_id map) NATIVE(668);

// Controls subtitles shown over a movie. With mode 0, value turns captions on (1) or off (0), and
// off clears all 18 caption slots. With mode 1, value is the slot (0 to 17) and the caption shows
// text from start_frame for frames frames, both at 60 per second.
// Original name: _SET_MOVIE_CC.
extern void set_movie_caption(enum movie_caption_mode mode, int value, OPTIONAL(int start_frame, string text, int frames)) NATIVE(669);

// Registers a scene character as a town villager in the given villager slot, using the character's
// current position and facing as its home spot. The spot record is allocated from the numbered
// scene memory stack.
// Original name: _REGISTER_VILLAGER2.
extern void register_villager_here(chara_slot chara, int villager_slot, enum memory_stack memory_stack) NATIVE(670);

// Runs a fishing tournament command. 0 sets the tournament rank to value (kept within 0 to 100), 1
// clears the recorded entries, 2 loads the tournament prize list, 3 advances the tournament
// counter (wrapping back to 1).
// Original name: _SET_FISHINGTOURNAMENT_ETC.
extern void set_fishing_tournament(enum fish_tour_set_command command, OPTIONAL(int value)) NATIVE(671);

// Binds an event object handle (0 to 31) to a scene character, so the other handle natives act on
// it.
// Original name: _EOH_SYNC_CHARA.
extern void bind_handle_chara(int handle, int chara) NATIVE(700);

// Binds a handle to a placed map part of the active maps, found by number or by name. With a piece
// name it binds that piece of the part in local coordinates; with a frame name too it binds that
// frame of the piece.
// Original name: _EOH_SYNC_OBJ.
extern void bind_handle_map_part(int handle, any part, OPTIONAL(string piece_name, string frame_name)) NATIVE(701);

// Moves the object bound to a handle to a position in world units, relative to the event's world
// origin when one is set. Sprites and frames take the position as given.
// Original name: _EOH_SET_POS.
extern void set_handle_pos(int handle, float x, float y, float z) NATIVE(702);

// Sets the rotation of the object bound to a handle, in radians, offset by the event's world
// rotation for characters and world objects. Sprites use only z. The engine rejects calls with
// three arguments, so pass all of x, y and z.
// Original name: _EOH_SET_ROT.
extern void set_handle_rot(int handle, float x, float y, OPTIONAL(float z)) NATIVE(703);

// Writes the position of the object bound to a handle, in the event's coordinates, to x, y and z.
// They are left unchanged when the handle is empty.
// Original name: _EOH_GET_POS.
extern void get_handle_pos(int handle, float *x, float *y, float *z) NATIVE(704);

// Writes the rotation in radians of the object bound to a handle to x, y and z. They are left
// unchanged when the handle is empty.
// Original name: _EOH_GET_ROT.
extern void get_handle_rot(int handle, float *x, float *y, float *z) NATIVE(705);

// Starts a named motion on the character bound to a handle. flags is a bit set: 1 pauses, 2 holds
// the last frame instead of looping, 4 restarts from the first frame. When step is given and not
// -1, the motion switches at once and plays at that many frames per tick. Calls with a fifth
// argument are rejected.
// Original name: _EOH_SET_MOTION.
extern void set_handle_motion(int handle, string motion_name, OPTIONAL(enum chara_motion_flag flags, float step, int unused)) NATIVE(706);

// Sets how many motion frames the character bound to a handle advances per tick.
// Original name: _EOH_SET_STEP.
extern void set_handle_motion_step(int handle, float step) NATIVE(707);

// Turns texture animations of the character bound to a handle on or off. (handle, 0) stops them
// all; (handle, on, name) turns the named one on or off; (handle, name) turns it on; (handle,
// off_name, on_name) switches from one to the other.
// Original name: _EOH_SET_TEX_ANIM.
extern void set_handle_texture_anim(int handle, any mode, OPTIONAL(string anim_name)) NATIVE(708);

// Sets the scale of the object bound to a handle on each axis. 1.0 is normal size.
// Original name: _EOH_SET_SCALE.
extern void set_handle_scale(int handle, float x, float y, float z) NATIVE(709);

// Shows (1) or hides (0) the character or map object bound to a handle. For a function point it
// enables or disables the point.
// Original name: _EOH_SET_SHOW.
extern void set_handle_visible(int handle, int visible) NATIVE(710);

// Returns whether the object bound to a handle is shown. The result is left unchanged when the
// handle is empty.
// Original name: _EOH_GET_SHOW.
extern int get_handle_visible(int handle) NATIVE(711);

// Shows or hides a named model frame of the character or frame bound to a handle. Any nonzero
// visible counts as 1.
// Original name: _EOH_SET_FRAME_SHOW.
extern void set_handle_frame_visible(int handle, string frame_name, int visible) NATIVE(712);

// Turns the shadow of the character bound to a handle on (1) or off (0).
// Original name: _EOH_SET_SHADOW.
extern void set_handle_shadow(int handle, int on) NATIVE(713);

// Writes a translation into the root frame matrix of the character, or the local matrix of the
// frame, bound to a handle, offsetting the model from its position.
// Original name: _EOH_SET_TRANSLATE.
extern void set_handle_translate(int handle, float x, float y, float z) NATIVE(714);

// Binds a handle to an event sprite by number.
// Original name: _EOH_SYNC_SPRITE.
extern void bind_handle_sprite(int handle, int sprite) NATIVE(715);

// Sets the footstep sound id of the character bound to a handle.
// Original name: _EOH_SET_FOOT_SOUND_ID.
extern void set_handle_foot_sound(int handle, int foot_sound_id) NATIVE(716);

// Writes a raw draw value into a named model frame of the character or frame bound to a handle. 0
// hides the frame.
// Original name: _EOH_SET_FRAME_STATUS.
extern void set_handle_frame_draw(int handle, string frame_name, int draw) NATIVE(717);

// Writes the world position of a named model frame of the character bound to a handle, in the
// event's coordinates, to x, y and z. They are left unchanged when the frame is not found.
// Original name: _EOH_GET_FRAME_POS.
extern void get_handle_frame_pos(int handle, string frame_name, float *x, float *y, float *z) NATIVE(718);

// Sets the sound effect bank id that the character bound to a handle plays its motion sounds from.
// Original name: _EOH_SET_SOUND_ID.
extern void set_handle_se_bank(int handle, int sound_id) NATIVE(719);

// Returns the draw value of a named model frame of the character or frame bound to a handle, or 0
// when the frame is not found.
// Original name: _EOH_GET_FRAME_STATUS.
extern int get_handle_frame_draw(int handle, string frame_name) NATIVE(720);

// Binds a handle to a named model frame inside a scene character, so it can be moved, rotated or
// hidden on its own.
// Original name: _EOH_SYNC_CHROBJ.
extern void bind_handle_chara_frame(int handle, int chara, string frame_name) NATIVE(721);

// Sets the fade flag of the character bound to a handle.
// Original name: _EOH_SET_FADE_FLAG.
extern void set_handle_fade_flag(int handle, int flag) NATIVE(722);

// Updates the position of the character bound to a handle, resets its dynamic animation parts
// (swinging pieces such as hair or cloth) to rest, then steps them 10 times. Used after
// teleporting a character.
// Original name: _EOH_RESET_DA_POSITION.
extern void reset_handle_dynamics(int handle) NATIVE(723);

// Writes a raw draw value into a named frame of the shadow model of the character bound to a
// handle. 0 hides it.
// Original name: _EOH_SET_SHADOW_FRAME_STATUS.
extern void set_handle_shadow_draw(int handle, string frame_name, int draw) NATIVE(724);

// Binds a handle to the dungeon geostone object.
// Original name: _EOH_SYNC_GEOSTONE.
extern void bind_handle_geostone(int handle) NATIVE(725);

// Binds a handle to the character with the given name among the player's linked characters. The
// second argument is ignored.
// Original name: _EOH_SYNC_SEARCH_CHARA.
extern void bind_handle_named_chara(int handle, int unused, string chara_name) NATIVE(726);

// Applies the pending motion change of the character bound to a handle at once instead of waiting
// for the next update.
// Original name: _EOH_NORMAL_DRIVE.
extern void apply_handle_motion(int handle) NATIVE(727);

// Binds a handle to a placed part of the active edit map, found by number or by name. With a piece
// name it binds that piece in local coordinates; with a frame name too it binds that frame of the
// piece.
// Original name: _EOH_SYNC_EDIT_OBJ.
extern void bind_handle_edit_part(int handle, any part, OPTIONAL(string piece_name, string frame_name)) NATIVE(728);

// Sets the opacity of a named model frame of the character or frame bound to a handle, from 0.0
// (invisible) to 1.0.
// Original name: _EOH_SET_FRAME_ALPHA.
extern void set_handle_frame_alpha(int handle, string frame_name, float alpha) NATIVE(729);

// Binds a handle to a named function point of the active map. part picks the placed part that owns
// it, by number or by name; an empty name "" searches the map's own points.
// Original name: _EOH_SYNC_FUNCP.
extern void bind_handle_func_point(int handle, any part, string point_name) NATIVE(730);

// Sets the footstep sound effect bank of the character bound to a handle.
// Original name: _EOH_SET_FOOT_SE_ID.
extern void set_handle_foot_bank(int handle, int bank) NATIVE(731);

// Binds a handle to the door part of the generated dungeon floor and returns 1 on success, 0
// otherwise.
// Original name: _EOH_SYNC_DOOR_PARTS.
extern int bind_handle_door(int handle) NATIVE(732);

// Clears the sphida (dungeon golf) game state so no game exists.
// Original name: _SPHIDA_INIT.
extern void init_sphida(void) NATIVE(750);

// Creates a sphida game in the numbered scene memory stack and starts it. kind 0 places the pin
// and ball at random on the dungeon floor, 1 uses the fixed story hole, 2 sets up bonus course
// number course. tex_bank is the texture bank the status panel draws from.
// Original name: _SPHIDA_SET_UP.
extern void create_sphida_game(enum memory_stack memory_stack, int tex_bank, OPTIONAL(int kind, int course)) NATIVE(751);

// Sets whether the sphida game runs; while 0 nothing is stepped or drawn. Fails when no sphida
// game exists.
// Original name: _SPHIDA_SET_PLAY_FLAG.
extern void set_sphida_playing(int on) NATIVE(752);

// Lets the player scroll the sphida mini map while an event runs (1) or not (0). Fails when no
// sphida game exists.
// Original name: _SPHIDA_SET_MINIMAP_FLAG.
extern void set_sphida_map_scroll(int on) NATIVE(753);

// Shows (1) or hides (0) the shot line points on the sphida mini map. Fails when no sphida game
// exists.
// Original name: _SPHIDA_SET_MM_LINE_FLAG.
extern void set_sphida_line_marks(int on) NATIVE(754);

// Sets one of the 5 shot line points (index 0 to 4) marked on the sphida mini map, in world units.
// Indexes of 5 or more are ignored. Fails when no sphida game exists.
// Original name: _SPHIDA_SET_MM_LINE_POS.
extern void set_sphida_line_point(int index, float x, float y, float z) NATIVE(755);

// Sets the position of the sphida pin in world units. Fails when no sphida game exists.
// Original name: _SPHIDA_SET_PIN_POS.
extern void set_sphida_pin_pos(float x, float y, float z) NATIVE(756);

// Writes the position of the sphida pin to x, y and z. Fails when no sphida game exists.
// Original name: _SPHIDA_GET_PIN_POS.
extern void get_sphida_pin_pos(float *x, float *y, float *z) NATIVE(757);

// Sets the position of the sphida ball in world units. Fails when no sphida game exists.
// Original name: _SPHIDA_SET_BALL_POS.
extern void set_sphida_ball_pos(float x, float y, float z) NATIVE(758);

// Writes the position of the sphida ball to x, y and z. Fails when no sphida game exists.
// Original name: _SPHIDA_GET_BALL_POS.
extern void get_sphida_ball_pos(float *x, float *y, float *z) NATIVE(759);

// Sets the pin colour, 0 or 1, which picks its mini map marker. Fails when no sphida game exists.
// Original name: _SPHIDA_SET_PIN_COL.
extern void set_sphida_pin_color(int color) NATIVE(760);

// Returns the pin colour (0 or 1). Fails when no sphida game exists.
// Original name: _SPHIDA_GET_PIN_COL.
extern int get_sphida_pin_color(void) NATIVE(761);

// Sets the ball colour, 0 or 1, which picks its mini map marker. Fails when no sphida game exists.
// Original name: _SPHIDA_SET_BALL_COL.
extern void set_sphida_ball_color(int color) NATIVE(762);

// Returns the ball colour (0 or 1). Fails when no sphida game exists.
// Original name: _SPHIDA_GET_BALL_COL.
extern int get_sphida_ball_color(void) NATIVE(763);

// Sets the shots of par left for the hole. Ignored while the debug flag is on. Fails when no
// sphida game exists.
// Original name: _SPHIDA_SET_PAR_COUNT.
extern void set_sphida_par(int par) NATIVE(764);

// Returns the shots of par left for the hole (one more while the debug flag is on). Fails when no
// sphida game exists.
// Original name: _SPHIDA_GET_PAR_COUNT.
extern int get_sphida_par(void) NATIVE(765);

// Returns the sphida mini map scale (0 to 2, from the dungeon map setting). Fails when no sphida
// game exists.
// Original name: _SPHIDA_GET_MINI_LEVEL.
extern int get_sphida_map_scale(void) NATIVE(766);

// Returns the texture bank the sphida status panel draws from. Fails when no sphida game exists.
// Original name: _SPHIDA_GET_TEXB.
extern int get_sphida_tex_bank(void) NATIVE(767);

// Shows (1) or hides (0) the sphida status panel and power gauge. Fails when no sphida game
// exists.
// Original name: _SPHIDA_SET_STATUS_FLAG.
extern void set_sphida_status_panel(int on) NATIVE(768);

// Resets the sphida power gauge to idle: no power, cursor at rest, result code -10. Fails when no
// sphida game exists.
// Original name: _SPHIDA_RESET_POWGAGE.
extern void reset_sphida_gauge(void) NATIVE(769);

// Starts a swing on the sphida power gauge; the cursor begins to charge. Fails when no sphida game
// exists.
// Original name: _SPHIDA_START_POWGAGE.
extern void start_sphida_gauge(void) NATIVE(770);

// Presses the sphida power gauge: while charging it sets the power, while running back to the
// impact point it stops the cursor for judging. Does nothing at other stages. Fails when no sphida
// game exists.
// Original name: _SPHIDA_TRIGGER_POWGAGE.
extern void press_sphida_gauge(void) NATIVE(771);

// Returns the power set on the sphida gauge, from 0.0 to 1.0. Fails when no sphida game exists.
// Original name: _SPHIDA_GET_SHOT_POW.
extern float get_sphida_shot_power(void) NATIVE(772);

// Returns the result of the last swing: -10 no swing yet, -3 to 3 how far from the impact point
// the cursor stopped (0 is on it), 4 ran past without a stop, 5 ran back to empty without power
// set. Fails when no sphida game exists.
// Original name: _SPHIDA_GET_POWGAGE_CODE.
extern int get_sphida_gauge_result(void) NATIVE(773);

// Sets the width of the accurate zone around the impact point on the sphida gauge, kept within 1
// to 6. Fails when no sphida game exists.
// Original name: _SPHIDA_SET_POWGAGE_SAFE_LEVEL.
extern void set_sphida_safe_zone(int level) NATIVE(774);

// Writes the stats of sphida club number 9 to 14: power is the launch speed, extra is a second
// float the shot scripts read, and safe_level is the gauge safe zone width. Fails for other
// numbers.
// Original name: _SPHIDA_GET_CULB_DEF.
extern void get_sphida_club_stats(int club_no, float *power, float *extra, int *safe_level) NATIVE(775);

// Sets the spin mark on the sphida ball panel, x across and y down, each from -1.0 to 1.0. Fails
// when no sphida game exists.
// Original name: _SPHIDA_SET_SPIN_MARK_POS.
extern void set_sphida_spin_mark(float x, float y) NATIVE(776);

// Sets the club the player holds in the sphida game (9 to 14). Fails when no sphida game exists.
// Original name: _SPHIDA_SET_CULB_NO.
extern void set_sphida_club(int club_no) NATIVE(777);

// Stores the angle in radians at which the sphida ball is struck, which the status panel uses to
// draw the carry. Nothing is calculated here. Fails when no sphida game exists.
// Original name: _SPHIDA_CALC_CARRY.
extern void set_sphida_carry_angle(float angle) NATIVE(778);

// Writes the screen position of the sphida gauge cursor to x and y. Fails when no sphida game
// exists.
// Original name: _SPHIDA_GET_PG_CURSOR_POS.
extern void get_sphida_cursor_pos(float *x, float *y) NATIVE(779);

// Loads the pin collision from a model file that must already be in the preloaded background
// buffers, allocating from the numbered scene memory stack. Fails when no sphida game exists.
// Original name: _SPHIDA_SET_COL_MODEL.
extern void load_sphida_pin_collision(enum memory_stack memory_stack, string model_file) NATIVE(780);

// Writes the sphida prize item and its count for the current dungeon floor to item and count.
// index picks one of the floor's 3 prizes; values above 2 use the last.
// Original name: _SPHIDA_GET_PRIZE.
extern void get_sphida_prize(int index, int *item, int *count) NATIVE(781);

// Marks whether the last challenge of the current spheda hole is under way (nonzero) or not (0).
// Fails while no spheda game is running.
// Original name: _SPHIDA_SET_LAST_CHALLENGE.
extern void set_sphida_last_challenge(int on) NATIVE(782);

// Returns the spheda last-challenge flag set by set_sphida_last_challenge. Fails while no spheda
// game is running.
// Original name: _SPHIDA_GET_LAST_CHALLENGE.
extern int get_sphida_last_challenge(void) NATIVE(783);

// Returns 1 while one of the spheda bonus courses is being played, else 0. Fails while no spheda
// game is running.
// Original name: _SPHIDA_GET_OMAKE_MODE.
extern int get_sphida_bonus_mode(void) NATIVE(784);

// Sets the current spheda hole number in the save data.
// Original name: _SPHIDA_SET_NOW_HOLE.
extern void set_sphida_hole(int hole) NATIVE(785);

// Returns the current spheda hole number from the save data.
// Original name: _SPHIDA_GET_NOW_HOLE.
extern int get_sphida_hole(void) NATIVE(786);

// Stores a spheda score for hole 0 to 8 in the save data; hole = -1 means the current hole. Out-
// of-range holes are ignored.
// Original name: _SPHIDA_SET_SCORE.
extern void set_sphida_score(int hole, int score) NATIVE(787);

// Returns the saved spheda score of hole 0 to 8. With hole = -1 it returns the total of all nine
// holes; holes above 8 give 0.
// Original name: _SPHIDA_GET_SCORE.
extern int get_sphida_score(int hole) NATIVE(788);

// Debug leftover that does nothing.
// Original name: _MT_TEST.
extern void run_mt_test(void) NATIVE(998);

// Debug leftover that does nothing and ignores its argument.
// Original name: _TEST.
extern void run_test(OPTIONAL(any value)) NATIVE(999);

// Sets the three out variables to 0.0.
// Original name: _ZERO_VECTOR.
extern void set_vector_zero(float *x, float *y, float *z) NATIVE(1000);

// Normalizes the vector held in x, y, z in place to length 1.
// Original name: _NORMAL_VECTOR.
extern void normalize_vector(float *x, float *y, float *z) NATIVE(1001);

// Copies src_x, src_y, src_z into the three out variables.
// Original name: _COPY_VECTOR.
extern void copy_vector(float *x, float *y, float *z, float src_x, float src_y, float src_z) NATIVE(1002);

// Adds add_x, add_y, add_z to the vector held in x, y, z, in place.
// Original name: _ADD_VECTOR.
extern void add_vector(float *x, float *y, float *z, float add_x, float add_y, float add_z) NATIVE(1003);

// Subtracts sub_x, sub_y, sub_z from the vector held in x, y, z, in place.
// Original name: _SUB_VECTOR.
extern void subtract_vector(float *x, float *y, float *z, float sub_x, float sub_y, float sub_z) NATIVE(1004);

// Multiplies the vector held in x, y, z by scale, in place.
// Original name: _SCALE_VECTOR.
extern void scale_vector(float *x, float *y, float *z, float scale) NATIVE(1005);

// Divides the vector held in x, y, z by divisor, in place. Fails and leaves the vector unchanged
// when divisor is 0.
// Original name: _DIV_VECTOR.
extern void divide_vector(float *x, float *y, float *z, float divisor) NATIVE(1006);

// Returns the length of the vector x, y, z. Pass 0.0 for y to get the length on the ground plane.
// Original name: _DIST_VECTOR.
extern float get_vector_length(float x, float y, float z) NATIVE(1007);

// Returns the distance between two points, in world units.
// Original name: _DIST_VECTOR2.
extern float get_distance(float x1, float y1, float z1, float x2, float y2, float z2) NATIVE(1008);

// Returns the square root of value.
// Original name: _SQRT.
extern float get_square_root(float value) NATIVE(1009);

// Returns atan2(y, x) in radians from -pi to pi. Scripts pass x then z of an offset to get a yaw
// angle.
// Original name: _ATAN2F.
extern float get_atan2(float y, float x) NATIVE(1010);

// Compares two angles in radians, wrapping their difference into -pi..pi. Returns 0 when a - b is
// within tolerance, 1 when it is larger and -1 when it is smaller.
// Original name: _ANGLE_CMP.
extern int compare_angles(float a, float b, float tolerance) NATIVE(1011);

// Wraps the angle held in the variable into the range -pi..pi radians, in place.
// Original name: _ANGLE_LIMIT.
extern void wrap_angle(float *angle) NATIVE(1012);

// Reads a random number from 0 up to but not including range into result. A float range gives a
// float; an int range gives an int.
// Original name: _GET_RAND.
extern void get_random(any range, any *result) NATIVE(1013);

// Returns the distance from point p to the segment from a to b, or -1.0 when p projects outside
// the segment.
// Original name: _LINE_POINT_DIST.
extern float get_segment_point_dist(float ax, float ay, float az, float bx, float by, float bz, float px, float py, float pz) NATIVE(1014);

// Creates the single event sword trail, a ribbon drawn between pairs of points, in the given scene
// memory stack. capacity is how many point pairs it keeps and division how many smoothed points
// are made between two pairs; both colours start at 128, 128, 128, 128.
// Original name: _CREATE_SWORD_EFFECT.
extern void create_sword_trail(enum memory_stack stack_no, int capacity, int division) NATIVE(1015);

// Forgets the event sword trail made by create_sword_trail so it is no longer drawn. Its memory
// stays in the scene stack.
// Original name: _DELETE_SWORD_EFFECT.
extern void delete_sword_trail(void) NATIVE(1016);

// Sets the RGBA colour of the leading edge and of the trailing edge of the event sword trail, each
// 0 to 255 with 128 as neutral. Fails if no trail exists.
// Original name: _SWORD_EFFECT_COLOR.
extern void set_sword_trail_color(int edge_r, int edge_g, int edge_b, int edge_a, int back_r, int back_g, int back_b, int back_a) NATIVE(1017);

// Records a new pair of points, the leading edge and the trailing edge, into the event sword
// trail. life defaults to 1.0 and drops by 0.1 each frame until the pair disappears. Does nothing
// if no trail exists.
// Original name: _SWORD_EFFECT_ADD_POINT.
extern void add_sword_trail_point(float edge_x, float edge_y, float edge_z, float back_x, float back_y, float back_z, OPTIONAL(float life)) NATIVE(1018);

// Moves a character by an offset added to its current position, in world units.
// Original name: _ADD_CHARA_POS.
extern void add_chara_pos(chara_slot chara, float dx, float dy, float dz) NATIVE(1019);

// Turns a character by adding to its current rotation, in radians. Each axis is wrapped into
// -pi..pi.
// Original name: _ADD_CHARA_ROT.
extern void add_chara_rot(chara_slot chara, float rx, float ry, float rz) NATIVE(1020);

// Puts a treasure box in the first free slot of the dungeon at x, y, z, turned by angle radians,
// holding count of item number item (count defaults to 1).
// Original name: _POST_TREASURE_BOX.
extern void spawn_treasure_box(float x, float y, float z, float angle, item_id item, OPTIONAL(int count)) NATIVE(1021);

// Finds the map parts placed within a box of half-size radius around x, y, z and reads the
// position of the nearest one into the out variables. Fails without writing when no parts are
// found.
// Original name: _GET_PARTS_ORIGIN.
extern void get_nearest_parts_pos(float x, float y, float z, float radius, float *parts_x, float *parts_y, float *parts_z) NATIVE(1022);

// Advances the player-controlled camera by one frame.
// Original name: _CTRLC_STEP.
extern void step_control_camera(void) NATIVE(1023);

// Puts the player-controlled camera's eye at an angle around its look-at point, in radians,
// keeping its distance and height.
// Original name: _CTRLC_SET_ROTATE.
extern void set_control_camera_angle(float angle) NATIVE(1024);

// Moves the player-controlled camera from the pad's turn and swing-back input, using the
// character's rotation and keeping the camera clear of map walls near its look-at point. Fails if
// the character does not exist.
// Original name: _CTRLC_MOVE_CAMERA.
extern void move_control_camera(int chara) NATIVE(1025);

// Sets which ways the player-controlled camera may not move, as bits: 1 shoulder buttons do not
// turn it, 2 right stick does not turn it, 0x40 the swing-behind button does nothing, 0x80 it does
// not swing round walls. 0 allows all.
// Original name: _CTRLC_SET_ROT_CANCEL.
extern void set_camera_rot_cancel(enum camera_rot_cancel mask) NATIVE(1026);

// Sets the limits of the player-controlled camera: height added to the eye at the nearest and
// farthest distance, and the minimum and maximum horizontal distance from the look-at point, in
// world units.
// Original name: _CTRLC_MOVE_RANGE.
extern void set_control_camera_range(float near_height, float far_height, float min_dist, float max_dist) NATIVE(1027);

// Finds the nearest active treasure box of the dungeon within 30 units of x, y, z. Reads its
// position into box_x, box_y, box_z and its slot into index, or 0, 0, 0 and -1 when none is near.
// Original name: _GET_NEAR_TBOX_POS.
extern void get_nearest_treasure_box(float x, float y, float z, float *box_x, float *box_y, float *box_z, int *index) NATIVE(1028);

// Stub left in the engine: it returns nothing and always reports failure.
// Original name: _CONV_CHRNO_S2L.
extern int convert_chara_no(int short_no) NATIVE(1029);

// Creates a weapon trail in trail slot 0 or 1 of a character, allocated in the given scene memory
// stack. capacity is how many recorded points it keeps and division how many smoothed points are
// made between two; both colours start at 128, 128, 128, 128.
// Original name: _SWE_INIT.
extern void init_chara_trail(chara_slot chara, int slot, enum memory_stack stack_no, int capacity, int division) NATIVE(1030);

// Sets the RGBA colours of the two edges of a character's weapon trail, each 0 to 255 with 128 as
// neutral. Fails if the trail slot is empty.
// Original name: _SWE_SET_COLOR.
extern void set_chara_trail_color(chara_slot chara, int slot, int r0, int g0, int b0, int a0, int r1, int g1, int b1, int a1) NATIVE(1031);

// Maps a character's weapon trail with the named texture from texture block tex_block, using the
// texel rectangle starting at u, v with the given width and height. Fails if the trail slot is
// empty or the texture is not found.
// Original name: _SWE_SET_TEXTURE.
extern void set_chara_trail_texture(chara_slot chara, int slot, int tex_block, string tex_name, int u, int v, int width, int height) NATIVE(1032);

// Starts a character's weapon trail between two named model frames, such as the blade's base and
// tip. length is how many smoothed points stay fully opaque, hold_frames how long it stays before
// fading and fade_frames how long the fade takes.
// Original name: _SWE_START_EFFECT.
extern void start_chara_trail(chara_slot chara, int slot, string from_frame, string to_frame, int length, int fade_frames, int hold_frames) NATIVE(1033);

// Sets the scene type of a character slot. Type 1 is hit by attacks aimed at the player, 3 by
// attacks aimed at monsters, and 4 is drawn in the later effect pass after the other characters.
// Original name: _SET_CHARA_TYPE.
extern void set_chara_scene_type(chara_slot chara, enum scene_chara_type type) NATIVE(1034);

// Reads a field of the event that started the script into the out variables. field: 0 flag, 1
// event number, 2 point number, 3-5 event args 1-3, 6 map event check type, 7 map event number, 8
// position x, y, z and yaw, 9 map parts number, 10 position x, y, z, 11 rotation (y only with one
// out, x, y, z with three), 12 scale x, y, z, 13 character slot, 14 character number, 15 game
// object number.
// Original name: _GET_EVENT_DATA.
extern void get_event_data(enum event_data_field field, any *value, OPTIONAL(any *value2, any *value3, any *value4)) NATIVE(1035);

// Stores the previous floor number of the current dungeon in the save data.
// Original name: _DNG_SET_PREV_FLOOR.
extern void set_prev_floor(int floor) NATIVE(1036);

// Returns the previous floor number of the current dungeon from the save data.
// Original name: _DNG_GET_PREV_FLOOR.
extern int get_prev_floor(void) NATIVE(1037);

// Stub left in the engine: it does nothing and always reports failure.
// Original name: _DNG_SET_FAST_FLOOR.
extern void set_fast_floor(void) NATIVE(1038);

// Writes one field of a dungeon floor's save record. field: 0 unknown, 1 best clear time in
// frames, 2 and 3 unknown, 4 spheda cleared, 5 progress bits ORed into the flags (1 open, 8
// practice, 0x10 time clear, 0x20 fishing, 0x80 spheda, 0x100 geostone found, 0x200 geostone read,
// 0x400 seal cleared), 6 kill count, 7 visit count.
// Original name: _SET_FLOOR_INFO.
extern void set_floor_info(int dungeon, int floor, enum floor_info_field field, any value) NATIVE(1039);

// Returns one field of a dungeon floor's save record, using the same field numbers as
// set_floor_info; field 5 gives all the progress flag bits.
// Original name: _GET_FLOOR_INFO.
extern int get_floor_info(int dungeon, int floor, enum floor_info_field field) NATIVE(1040);

// Returns the floor that follows floor along the given route of the current dungeon map, or 0 when
// the floor is unknown.
// Original name: _GET_NEXT_FLOOR.
extern int get_next_floor(int floor, int route) NATIVE(1041);

// Turns off auto-repeat for held controller buttons.
// Original name: _PAD_AUTO_REPEAT_OFF.
extern void disable_pad_auto_repeat(void) NATIVE(1042);

// Turns on auto-repeat for the controller buttons in mask, firing after delay frames and then
// every interval frames while held.
// Original name: _PAD_SET_AUTO_REPEAT.
extern void set_pad_auto_repeat(enum pad_button mask, int delay, int interval) NATIVE(1043);

// Sets (on = 1) or clears (on = 0) dungeon pause bits. Bits: 1 monster AI, 2 player step, 4 player
// control, 0x10 monster draw, 0x20 sky draw, 0x80 map draw, 0x100 minimap, 0x200 pickups, 0x400
// player status, 0x800 exit heal, 0x1000 enemy step, 0x2000 weapon draw, 0x4000 battle music,
// 0x8000 pad reset, 0x10000 monster names.
// Original name: _DNG_PAUSE.
extern void set_dungeon_pause(enum dng_pause mask, int on) NATIVE(1044);

// Returns the dungeon pause bits that are set within mask; nonzero means at least one is set.
// Original name: _DNG_CHECK_PAUSE.
extern int check_dungeon_pause(enum dng_pause mask) NATIVE(1045);

// Resets the dungeon frame timer to 0.
// Original name: _DNG_RESET_TIMER.
extern void reset_dungeon_timer(void) NATIVE(1046);

// Returns the number of frames counted since the dungeon timer was last reset.
// Original name: _DNG_GET_TIMER.
extern int get_dungeon_timer(void) NATIVE(1047);

// Loads a skin onto a character from a pack file into scene memory stack stack_no. pack is either
// an item number, whose model file is used, or a file path string; info_name names the skin entry
// inside the pack.
// Original name: _LOAD_SKIN.
extern void load_chara_skin(enum memory_stack stack_no, any pack, string info_name, int chara) NATIVE(1048);

// Tests the line from one point to another against the map's camera collision. With 7 arguments
// the 7th receives the index of the polygon hit or -1; with 10 arguments the 7th to 9th receive
// the hit position and the 10th the polygon index.
// Original name: _CHK_CAMERA_COL.
extern void check_camera_collision(float from_x, float from_y, float from_z, float to_x, float to_y, float to_z, any *hit_or_hit_x, OPTIONAL(float *hit_y, float *hit_z, int *hit)) NATIVE(1049);

// Finds the map parts nearest x, y, z within radius and looks up the named function point on it.
// found receives 1 and the point's world position goes into the out variables, or found receives 0
// when the parts has no such point; fails without writing if no parts are near.
// Original name: _GET_PARTS_FUNC_POS.
extern void get_parts_point_pos(float x, float y, float z, float radius, string point_name, int *found, float *point_x, float *point_y, float *point_z) NATIVE(1050);

// Reads the world position of random circle 0 to 2 of the floor into the out variables; circle =
// -1 means the circle the player last stood on. Fails without writing when there is none.
// Original name: _RANDOM_CIRCLE_GET_POS.
extern void get_random_circle_pos(int circle, float *x, float *y, float *z) NATIVE(1051);

// Turns off random circle 0 to 2 of the floor; circle = -1 means the circle the player last stood
// on.
// Original name: _RANDOM_CIRCLE_OFF.
extern void disable_random_circle(int circle) NATIVE(1052);

// Swaps the first eight lighting entries of the active map with the next eight, if the map has at
// least sixteen.
// Original name: _DNG_XCHG_MAP_LIGHT.
extern void swap_map_lighting(void) NATIVE(1053);

// Stops the geostone floating up and down.
// Original name: _GEOSTONE_ANIME_OFF.
extern void stop_geostone_anim(void) NATIVE(1054);

// Sets whether the geostone is on the floor. With 0 the geostone's map parts are moved away to a
// fixed hidden position.
// Original name: _GEOSTONE_SET_FLAG.
extern void set_geostone_flag(int on) NATIVE(1055);

// Attaches the geostone model to a named frame of a character's model so it follows it, such as
// "item" for a held item.
// Original name: _GEOSTONE_SET_REFERENCE.
extern void attach_geostone(chara_slot chara, string frame_name) NATIVE(1056);

// Detaches the geostone model from the character frame set by attach_geostone.
// Original name: _GEOSTONE_DEL_REFERENCE.
extern void detach_geostone(void) NATIVE(1057);

// Returns how the player character moves: 0 on foot, 1 or 4 ridepod on legs, 2 or 5 on treads, 3
// on wheels, 6 through the air.
// Original name: _GET_ROBO_MOVE_TYPE.
extern int get_player_move_type(void) NATIVE(1058);

// Sets the scene's exit flag, which event scripts read back with get_exit_flag.
// Original name: _SET_EXIT_FLAG.
extern void set_exit_flag(int value) NATIVE(1059);

// Returns the scene's exit flag.
// Original name: _GET_EXIT_FLAG.
extern int get_exit_flag(void) NATIVE(1060);

// Returns whether this is the E3 show demo build. The retail game always gives 0.
// Original name: _GET_E3_VERSION.
extern int get_e3_version(void) NATIVE(1061);

// Returns the state of logical pad button 0 to 127 this frame; nonzero when the button fires
// according to its binding.
// Original name: _CHK_PAD_CTRL.
extern int check_pad_button(int button) NATIVE(1062);

// Stops the player-controlled camera from moving.
// Original name: _CTRLC_STAY.
extern void stop_control_camera(void) NATIVE(1063);

// Sets the scale applied to the colours the dungeon is drawn with; 1.0 is normal.
// Original name: _BSCN_SET_BLIGHT_RATE.
extern void set_dungeon_brightness(float rate) NATIVE(1064);

// Rolls and returns a random trap number for a random circle of kind 0 or 1. Gives -2 when the
// active character is 2, and -1 for character 3 unless the trap is 4 or 12.
// Original name: _GET_RND_CIRCLE_TRAPID.
extern int get_random_circle_trap(int kind) NATIVE(1065);

// Applies the effect of a random circle trap to the active character. Trap 1 returns 10% of the
// sum of both weapons' maximum ABS; 2 refills HP, 3 repairs both characters' weapons, 6 takes half
// the HP, 8 to 11 halve a weapon's durability. The result is only written for trap 1.
// Original name: _SET_RND_CIRCLE_STATUS.
extern float apply_random_circle_trap(int trap) NATIVE(1066);

// Slides the dungeon status bar in (show = 1) or out (show = 0). speed is the fraction moved per
// frame, default 0.02 and scaled by 1.2; a speed of 1.0 or more shows or hides it at once.
// Original name: _SET_STATUSBAR_SHOW.
extern void show_status_bar(int show, OPTIONAL(float speed)) NATIVE(1067);

// Scatters count pickups from x, y, z that fly to the player. type: 0 coin and 3 large coin (value
// is the money), 1 weapon growth orb (value is the growth for the active character's weapon), 2
// gate key, 4 item.
// Original name: _SET_PULL_ITEM.
extern void spawn_pull_items(float x, float y, float z, enum pull_item_type type, int count, float value) NATIVE(1068);

// Opens the dungeon character change menu; the script is suspended while it is open. The argument
// is read and ignored.
// Original name: _MENU_CHARA_CHENGE.
extern void open_chara_change_menu(int unused) NATIVE(1069);

// Returns the sound bank handle loaded into sound port 0 to 11.
// Original name: _GET_EVENT_INFO_SNDID.
extern int get_sound_port_bank(enum snd_port port) NATIVE(1070);

// Finds map parts in the active maps by number or by name and reads its position into the out
// variables. Fails if none is found.
// Original name: _GET_PARTS_POS.
extern void get_parts_pos(any parts, float *x, float *y, float *z) NATIVE(1071);

// Makes the running drama scene unskippable by clearing its skip state.
// Original name: _CANCEL_DRAMA_SCENE.
extern void disable_scene_skip(void) NATIVE(1072);

// Returns the current motion frame of the random circle model.
// Original name: _GET_RNDC_MOT_NOWT.
extern float get_circle_motion_frame(void) NATIVE(1073);

// Sets the current motion frame of a character, such as to sync it with get_circle_motion_frame.
// Original name: _SET_CHARA_MOT_NOWT.
extern void set_chara_motion_frame(chara_slot chara, float frame) NATIVE(1074);

// Advances a character's motion one step: changes to its pending motion, blends into it and poses
// the model.
// Original name: _CHARA_NORMAL_DRIVE.
extern void update_chara_motion(chara_slot chara) NATIVE(1075);

// Puts a character's cloth and hair back to rest on its model, then simulates them for 10 steps so
// they settle.
// Original name: _CHARA_RESET_DA.
extern void reset_chara_cloth(chara_slot chara) NATIVE(1076);

// Switches the dungeon player to another character, reloading its model, icons and sounds. Fails
// if that character is already active.
// Original name: _DNG_SETUP_MAIN_UNIT.
extern void change_player_chara(enum user_chara chara) NATIVE(1077);

// With an int, adds the character to the party when state is 1 and removes it otherwise. Passing
// &variable instead reads 1 into it if the character is in the party, else 0.
// Original name: _JOIN_PARTY_MEMBER.
extern void set_party_member(enum user_chara chara, any state) NATIVE(1078);

// Lets the player change to a character (on = 1) or forbids it (on = 0).
// Original name: _SET_CHARA_CHANGE_FLAG.
extern void set_chara_change_enabled(enum user_chara chara, int on) NATIVE(1079);

// Unmasks (on = 1) or masks (on = 0) changing to a character, a second switch kept apart from
// set_chara_change_enabled.
// Original name: _SET_CHARA_CHANGE_MASK.
extern void set_chara_change_mask(enum user_chara chara, int on) NATIVE(1080);

// Sets the directory later file names are loaded relative to, such as "event/dng/". An empty
// string, or no argument, clears it so files load from the root.
// Original name: _CHANGE_DIR.
extern void change_dir(OPTIONAL(directory dir)) NATIVE(1081);

// Removes count of an item from the player's inventory; count defaults to 1.
// Original name: _SUB_ITEM.
extern void remove_item(item_id item, OPTIONAL(int count)) NATIVE(1082);

// Equips an owned item on character 0 to 2, swapping it with the item in its slot. Fails if the
// item is not owned or is already equipped.
// Original name: _SET_CHARA_EQUIP.
extern void equip_chara_item(enum user_chara chara, item_id item) NATIVE(1083);

// Loads a pack file into the read buffer so later file loads search it first.
// Original name: _LOAD_PACK_FILE.
extern void load_pack_file(string file_name) NATIVE(1084);

// Sets (on = 1) or clears (otherwise) the given bits of the 8-bit control value kept in the save
// data.
// Original name: _SET_BIT_CTRL.
extern void set_bit_ctrl(enum bit_ctrl bits, int on) NATIVE(1085);

// Returns 1 if any of the given bits of the save data's control value are set, else 0.
// Original name: _GET_BIT_CTRL.
extern int check_bit_ctrl(enum bit_ctrl bits) NATIVE(1086);

// Loads a compiled argument script (.stb) and runs it at once to build the event argument data,
// kept in scene memory stack stack_no.
// Original name: _LOAD_ARG.
extern void load_arg_script(enum memory_stack stack_no, string file_name) NATIVE(1087);

// Returns how many of an item the player owns.
// Original name: _GET_ITEM_HAVE_NUM.
extern int get_item_have_count(item_id item) NATIVE(1088);

// Sets the logical pad button that skips the drama scene.
// Original name: _SET_SKIP_BOTTON.
extern void set_skip_button(int button) NATIVE(1089);

// Sets the colour the screen fades to when the drama scene is skipped, as red, green and blue from
// 0 to 255.
// Original name: _SET_SKIP_FCOL.
extern void set_skip_fade_color(float r, float g, float b) NATIVE(1090);

// Returns 1. This build always reports debug mode on; scripts on the disc never call it.
// Original name: _GET_DEBUG_MODE.
extern int get_debug_mode(void) NATIVE(1091);

// Looks up a map by number or by name and returns its type: 1 for a georama map that has edit
// data, -1 when the map is not found.
// Original name: _GET_MAP_TYPE.
extern int get_map_type(any map, ...) NATIVE(1092);

// Reinitializes the dungeon scene's collision primitive manager, removing every collision
// primitive it holds.
// Original name: _DNG_COLLISION_ALL_CLR.
extern void clear_dungeon_collisions(void) NATIVE(1093);

// Turns drawing of the map behind the event on (1) or off (0).
// Original name: _SET_MAP_DRAW.
extern void set_map_draw(int on) NATIVE(1094);

// Returns the memory card load state the current game mode was entered with (1 after loading a
// save from the title screen), then resets it to 0, so only the first call sees it.
// Original name: _CHECK_MC_LOAD.
extern int check_memory_card_load(void) NATIVE(1095);

// Sets the scene's current map number. When it changes, the old number is kept as the previous
// map.
// Original name: _SET_NOW_MAP_NO.
extern void set_current_map(map_id map_no) NATIVE(1096);

// Reads the treasure chest nearest the player in the dungeon: its flags, the two item numbers it
// holds and the count of each.
// Original name: _GET_TBOX_PARAM.
extern void get_chest_contents(int *flags, int *item_a, int *item_b, int *count_a, int *count_b) NATIVE(1097);

// Makes the scene skip its next load of town villagers and sub villagers. Each skip flag clears
// itself once used.
// Original name: _CANCEL_LOAD_VILLAGER.
extern void skip_villager_load(void) NATIVE(1098);

// Sets a flag so the next request to start the now-loading screen returns at once without showing
// it.
// Original name: _CANCEL_NOW_LOADING.
extern void skip_loading_screen(void) NATIVE(1099);

// Creates the event effect script manager in scene memory stack stack_no. It uses the event's
// texture blocks starting texb_offset blocks in (default 0). Call before any other effect script
// native.
// Original name: _ESM_INITIALIZE.
extern void init_effect_scripts(enum memory_stack stack_no, OPTIONAL(int texb_offset)) NATIVE(1100);

// Gives the effect script manager a work heap of heap_size bytes (default 14000) taken from scene
// memory stack stack_no. Needs init_effect_scripts first.
// Original name: _ESM_INIT_FIX.
extern void set_effect_script_heap(enum memory_stack stack_no, OPTIONAL(int heap_size)) NATIVE(1101);

// Deletes every running effect, unloads every effect script base, empties the work heap, frees
// their texture blocks and removes the effect script manager.
// Original name: _ESM_CLEAR.
extern void clear_effect_scripts(void) NATIVE(1102);

// Loads an effect script base, given by name or by number, so effects can be created from it. Does
// nothing if it is already loaded.
// Original name: _ESM_LOAD_BASE.
extern void load_effect_script(any effect) NATIVE(1103);

// Starts an effect from a loaded effect script base. owner (default -1) is the id the effect
// belongs to. With the third argument the effect also takes a free slot under owner and slot
// receives its number, or -1 on failure; later natives use owner and slot to address it. Natives
// without owner and slot act on the effect created last.
// Original name: _ESM_CREATE.
extern void create_effect(string effect_name, OPTIONAL(int owner, int *slot)) NATIVE(1104);

// Switches the effect in owner's slot to program number 300, the program effect scripts use for
// their ending.
// Original name: _ESM_FINISH.
extern void finish_effect(int owner, int slot) NATIVE(1105);

// Deletes the effect in owner's slot at once.
// Original name: _ESM_DELETE.
extern void delete_effect(int owner, int slot) NATIVE(1106);

// Sets the first work vector of the effect created last, usually its position in world units. The
// engine also accepts a five argument form (owner, slot, x, y, z) for an effect in a slot.
// Original name: _ESM_SET_VECT1.
extern void set_effect_vector1(float x, float y, float z) NATIVE(1107);

// Sets the second work vector of the effect created last, such as a scale or direction, depending
// on the effect script. The engine also accepts a five argument form (owner, slot, x, y, z) for an
// effect in a slot.
// Original name: _ESM_SET_VECT2.
extern void set_effect_vector2(float x, float y, float z) NATIVE(1108);

// Sets the target id of the effect created last; -1 for none. The engine also accepts a three
// argument form (owner, slot, target_id) for an effect in a slot.
// Original name: _ESM_SET_TARGET_ID.
extern void set_effect_target(int target_id) NATIVE(1109);

// Builds the effect script base effect_name from file_name, which must already be loaded by the
// background loader or present in the loaded pack.
// Original name: _ESM_LOAD_BASE_PACK.
extern void load_effect_script_pack(string effect_name, string file_name) NATIVE(1110);

// Sets work value number index of the effect created last to an int or float. The engine also
// accepts a four argument form (owner, slot, index, value) for an effect in a slot.
// Original name: _ESM_SET_VALUE.
extern void set_effect_value(int index, any value) NATIVE(1111);

// Changes a party character's status condition bits (1 poison, 2 slow, 0x10 power up, others
// unnamed). With clear = 1 the bits are removed, otherwise they are added. The engine also accepts
// a two argument form (status_bits, clear) for the character active in battle.
// Original name: _SET_CHARA_CONDITION.
extern void set_chara_status(enum user_chara chara, enum chara_status_attr status_bits, int clear) NATIVE(1200);

// Adds amount to the weapon durability gauge in weapon_slot of the character active in battle;
// negative values reduce it. For the Ridepod or a monster form it changes that gauge instead.
// Original name: _ADD_WHP.
extern void add_weapon_hp(enum equip_slot weapon_slot, int amount) NATIVE(1201);

// Changes the active battle character's HP by rate. mode 0 scales by max HP, mode 1 by current HP;
// modes 2 and 3 do the same but never drop below 1. The bar animates over frames (default 0).
// Original name: _ADD_HP_RATE.
extern void add_hp_rate(float rate, enum hp_rate_kind mode, OPTIONAL(float frames)) NATIVE(1202);

// Returns the scene's time of day, a float from 0 to 24.
// Original name: _GET_TIME.
extern float get_time_of_day(...) NATIVE(1203);

// With one argument (&over_item) it receives the number of the first item the player holds more of
// than its limit, or 0. With three arguments (item, count, &allowed) allowed receives count if the
// player can hold count more of item, otherwise 0.
// Original name: _CHECK_GET_ITEM_LIMIT.
extern void check_item_limit(any item, OPTIONAL(int count, int *allowed)) NATIVE(1204);

// Returns how many items sit in the overflow slots past the end of the player's current bag.
// Original name: _CHECK_ITEM_OVER.
extern int check_bag_overflow(void) NATIVE(1205);

// Returns the main game mode: 1 town and georama, 2 dungeon, 3 title screen; other values are
// debug screens.
// Original name: _GET_NOW_LOOP_NO.
extern int get_game_mode(void) NATIVE(1206);

// Compares the time since reset_floor_timer with the floor's target clear time and saves a new
// best. Returns 0 for no record, 1 for the first clear within the target time, 2 for a better
// time.
// Original name: _IS_CLEAR_DESTROY.
extern int check_fast_clear(void) NATIVE(1207);

// Tests the floor's practice condition. timing selects which condition kinds are tested now (the
// disc uses 0 and 1). result receives 0 none, 1 failed, 2 met for the first time, 3 met again;
// challenge_type receives the floor's condition kind (-1 for none, 6 to 9 for the parameterised
// kind 2).
// Original name: _IS_CLEAR_PRACTICE.
extern void check_floor_challenge(int timing, int *result, int *challenge_type) NATIVE(1208);

// Returns the floor's sub games as bits: 1 spheda challenge, 2 fishing record.
// Original name: _IS_PLAY_SUB_GAME.
extern int get_floor_sub_games(void) NATIVE(1209);

// Starts timing the floor's clear time from the current play time. The argument is ignored; the
// disc passes -1.
// Original name: _RESET_SUBJECT_COUNTER.
extern void reset_floor_timer(int unused) NATIVE(1210);

// Resets the raster screen wave effect and sets its amplitude, speed and pitch. The disc passes
// speed and pitch as fractions of pi.
// Original name: _SCR_EFF_INIT_RASTER.
extern void init_screen_wave(float amplitude, float speed, float pitch) NATIVE(1211);

// Turns the screen wave on, moving amplitude, speed and pitch to the given values over frames
// frames (at once when frames is 0 or 1). A value of -1 keeps that setting.
// Original name: _SCR_EFF_START_RASTER.
extern void start_screen_wave(float amplitude, float speed, float pitch, int frames) NATIVE(1212);

// Turns the screen wave off, moving amplitude, speed and pitch to the given values over frames
// frames first (at once when frames is 0 or 1). A value of -1 keeps that setting.
// Original name: _SCR_EFF_STOP_RASTER.
extern void stop_screen_wave(float amplitude, float speed, float pitch, int frames) NATIVE(1213);

// Plays a motion on an animated piece of a placed map part, such as a train engine. part is the
// placed part's number or name, searched in every active map; piece_name names the piece inside
// it. motion_flags (default 0) are passed to the motion.
// Original name: _SET_MPCHARA_MOTION.
extern void set_map_part_motion(any part, string piece_name, string motion_name, OPTIONAL(enum chara_motion_flag motion_flags)) NATIVE(1214);

// Moves a function point of the active map to x, y, z. part is a placed part's number or name, or
// "" for the map's own function points.
// Original name: _FUNC_POINT_POS.
extern void set_func_point_pos(any part, string point_name, float x, float y, float z) NATIVE(1215);

// Turns the player camera toward the yaw angle (radians), step by step each frame until it is
// within 0.1 of it.
// Original name: _CTRLC_ROT_BACK.
extern void turn_camera_to(float angle) NATIVE(1216);

// Compares the part name of placed part part_no in the active map with name; returns 0 when they
// match, nonzero otherwise, and -1 when the part does not exist.
// Original name: _PARTS_NAME_STRCMP.
extern int compare_part_name(int part_no, string name) NATIVE(1217);

// Returns 0; this build is never a trial version.
// Original name: _GET_TRIAL_VERSION.
extern int get_trial_version(void) NATIVE(1218);

// Shows (1) or hides (0) the current dungeon floor's title in a frame at the bottom of the screen.
// Original name: _SET_FLOOR_EPISODE.
extern void show_floor_title(int on) NATIVE(1219);

// Reads the position of a function point in the active map. part is a placed part's number or
// name, or "" for the map's own function points.
// Original name: _FUNC_POINT_GET_POS.
extern void get_func_point_pos(any part, string point_name, float *x, float *y, float *z) NATIVE(1220);

// Reads the rotation of a function point in the active map, in radians. With one out argument it
// receives the Y rotation; with three they receive X, Y and Z. part is a placed part's number or
// name, or "" for the map's own points.
// Original name: _FUNC_POINT_GET_ROT.
extern void get_func_point_rot(any part, string point_name, float *rot, OPTIONAL(float *rot_y, float *rot_z)) NATIVE(1221);

// Stops the action of a scene character and plays its default motion on it and every chained part.
// motion_flags (default 0) are passed to the motion.
// Original name: _ACTCHR_SET_DEF_MOTION.
extern void play_default_motion(int actor, OPTIONAL(enum chara_motion_flag motion_flags)) NATIVE(1222);

// Adds synthesis points to the weapon in weapon_slot (0 or 1) of chara (0 or 1).
// Original name: _ADD_FUSION_POINT.
extern void add_synth_points(enum user_chara chara, enum equip_slot weapon_slot, int points) NATIVE(1223);

// Returns the game's debug flag.
// Original name: _GET_DEBUG_FLAG.
extern int get_debug_flag(void) NATIVE(1224);

// Marks the door at a world position as open on the dungeon mini map and updates the navigation
// map around it.
// Original name: _MINIMAP_DOOR_ENABLE.
extern void open_minimap_door(float x, float y, float z) NATIVE(1225);

// Returns nonzero on a boss floor, where the mini map symbols are hidden.
// Original name: _DNG_CHECK_BOSS_MAP.
extern int check_boss_floor(void) NATIVE(1226);

// Sets the event the dungeon runs when it can; -1 clears the request.
// Original name: _DNG_RUN_EVENT.
extern void queue_dungeon_event(int event_no) NATIVE(1227);

// Returns whether the player may switch to chara (0 Max, 1 Monica, 2 Ridepod, 3 Monica's monster
// form). It checks the unlock flags, HP, blocking status conditions and the floor's restrictions.
// Original name: _CHECK_ENABLE_CHARA_CHANGE.
extern int check_chara_switchable(enum user_chara chara) NATIVE(1228);

// Prepares the sepia screen effect: a screen sized texture in the event texture block at
// block_offset (default 0) and an image buffer from scene memory stack stack_no, or the shared
// read buffer when stack_no is -1.
// Original name: _INIT_SEPIA.
extern void init_sepia(enum memory_stack stack_no, OPTIONAL(int block_offset)) NATIVE(1229);

// Captures the current screen, tints it sepia and shows it until end_sepia. Needs init_sepia
// first.
// Original name: _START_SEPIA.
extern void start_sepia(void) NATIVE(1230);

// Turns the sepia screen effect off.
// Original name: _END_SEPIA.
extern void end_sepia(void) NATIVE(1231);

// Copies a loaded dungeon monster's model into scene character slot actor, replacing what was
// there. stack_no is the scene memory stack the copy is allocated from; monster is the monster's
// base number.
// Original name: _COPY_MONS2SCNCHR.
extern void copy_monster_to_actor(enum memory_stack stack_no, int monster, int actor) NATIVE(1232);

// Clears the lock on scene memory stack stack_no so it can be reused.
// Original name: _UNLOCK_STACK.
extern void unlock_stack(enum memory_stack stack_no) NATIVE(1233);

// Clears the scene's flag that says an event is running.
// Original name: _RESET_EVENT_TRG.
extern void clear_event_running(void) NATIVE(1234);

// Sets the character id stored in scene character slot.
// Original name: _SET_CHARA_NO.
extern void set_slot_chara_id(chara_slot slot, int chara_id) NATIVE(1235);

// Returns the character id stored in scene character slot, or -1 when the slot does not exist.
// Original name: _GET_CHARA_NO.
extern int get_slot_chara_id(chara_slot slot) NATIVE(1236);

// Searches the villager and talk slots of the scene for chara_id and returns its slot number, or
// -1.
// Original name: _SEARCH_CHARA_NO.
extern int find_villager_slot(npc_id chara_id) NATIVE(1237);

// Looks for a random stone within 30 units of x, y, z. found receives 1 and the out position the
// stone's position, or found is -1 and the position is 0, 0, 0.
// Original name: _GET_NEAR_RANDOM_STONE_POS.
extern void find_near_random_stone(float x, float y, float z, float *stone_x, float *stone_y, float *stone_z, int *found) NATIVE(1238);

// Prepares the mono flash screen effect: two screen sized textures in the first two event texture
// blocks and their image buffers from scene memory stack stack_no, or the shared read buffer when
// stack_no is negative.
// Original name: _INIT_MONO_FLASH.
extern void init_mono_flash(enum memory_stack stack_no) NATIVE(1239);

// Captures the current screen and starts the mono flash effect, switching images every interval
// frames. Needs init_mono_flash first.
// Original name: _START_MONO_FLASH.
extern void start_mono_flash(int interval) NATIVE(1240);

// Turns the mono flash screen effect off.
// Original name: _END_MONO_FLASH.
extern void end_mono_flash(void) NATIVE(1241);

// Removes the town villager with character id npc from the scene's villager manager.
// Original name: _DELETE_VILLAGER.
extern void delete_villager(npc_id npc) NATIVE(1242);

// Sets the dungeon weather: 0 normal, 2 rain. Rain also switches the environment sound to its rain
// variant.
// Original name: _DNG_SET_WEATHER.
extern void set_dungeon_weather(enum dng_weather weather) NATIVE(1243);

// Sets a party character's maximum HP and refills HP to it.
// Original name: _SET_CHARA_MAXHP.
extern void set_chara_max_hp(enum user_chara chara, int max_hp) NATIVE(1244);

// Sets a party character's defence value.
// Original name: _SET_CHARA_DEFENCE.
extern void set_chara_defence(enum user_chara chara, int defence) NATIVE(1245);

// Compares the placement name of placed part part_no in the active map with name; returns 0 when
// they match, nonzero otherwise, and -1 when the part does not exist.
// Original name: _PLACE_PARTS_NAME_STRCMP.
extern int compare_place_name(int part_no, string name) NATIVE(1246);

// Opens the item choice menu and leaves the script so the menu can run; follow with wait. When the
// menu closes, item receives the number of the item chosen. With consume = 1 a chosen item from
// the allowed list is used up. The trailing arguments list allowed item numbers.
// Original name: _GOTO_USE_ITEM2.
extern void open_item_choice_menu(int *item, int consume, item_id allowed_item, ...) NATIVE(1247);

// Sets or clears every georama analysis condition linked to analysis entry entry of map map_no.
// Original name: _DBG_SET_ANALYZE_FLAG.
extern void set_georama_analyze_flag(map_id map_no, int entry, int flag) NATIVE(1248);

// Shows or hides the Atlamillia model parts on scene character actor. mode 0 changes both parts, 1
// the first, 2 the second.
// Original name: _ATRAMIRIA_ON_OFF.
extern void set_atlamillia_visible(enum atlamillia_part mode, int actor, int visible) NATIVE(1249);

// Adds amount to the player's medal count, kept between 0 and 999.
// Original name: _ADD_YARIKOMI_MEDAL.
extern void add_medals(int amount) NATIVE(1250);

// Sets the dungeon's map effect id.
// Original name: _SET_MAP_EFFECT_ID.
extern void set_map_effect_id(int effect_id) NATIVE(1251);

// Returns the dungeon's map effect id.
// Original name: _GET_MAP_EFFECT_ID.
extern int get_map_effect_id(void) NATIVE(1252);

// Calls the dungeon floor start hook, which is empty in this build.
// Original name: _DNG_FLOOR_INIT.
extern void init_dungeon_floor(void) NATIVE(1253);

// Calls the dungeon floor end hook, which is empty in this build.
// Original name: _DNG_FLOOR_FINISH.
extern void finish_dungeon_floor(void) NATIVE(1254);

// Moves all twelve random stones of the generated floor far below the map, out of play.
// Original name: _CLEAR_RND_STONE.
extern void hide_random_stones(void) NATIVE(1255);

// Returns the current floor's status flags (1 Max blocked, 2 Monica blocked, 4 items blocked,
// other bits set by scripts).
// Original name: _GET_FLOOR_STATUS.
extern int get_floor_status(void) NATIVE(1256);

// Sets the bits in mask on the current floor's status flags when on is nonzero, clears them when
// 0.
// Original name: _SET_FLOOR_STATUS.
extern void set_floor_status(enum dng_floor_status mask, int on) NATIVE(1257);

// Returns the attribute flags of the generated floor's grid cell at a world position, or 1 when
// the position is off the grid.
// Original name: _AMG_GET_ATTR_STATUS.
extern int get_floor_cell_attr(float x, float y, float z) NATIVE(1258);

// Turns on fading for scene character actor and sets the distance in world units within which it
// is not drawn.
// Original name: _SET_NEAR_DIST.
extern void set_actor_near_dist(int actor, float dist) NATIVE(1259);

// Stores a time of day in hours in the event info, for a later get_saved_time.
// Original name: _SET_KEEP_TIME.
extern void set_saved_time(float hours) NATIVE(1260);

// Returns the time of day stored with set_saved_time.
// Original name: _GET_KEEP_TIME.
extern float get_saved_time(void) NATIVE(1261);

// Returns the active map's part number of the generated floor's door, or -1 when there is none.
// Original name: _GET_DOOR_PARTS_ID.
extern int get_door_part(void) NATIVE(1262);

// Puts Monica's default weapon back on and, unless her costume is flagged to stay, her default
// costume pieces too. The argument is ignored.
// Original name: _CHECK_EQUEP_CHANGE.
extern void reset_monica_equipment(int unused) NATIVE(1263);

// Changes a party character's HP by rate times the gauge (for example -0.7 removes 70 percent),
// never dropping below 1.
// Original name: _ADD_HP_RATE2.
extern void add_chara_hp_rate(enum user_chara chara, float rate) NATIVE(1264);

// Clears every rocket, machine gun shot and laser in the dungeon and deletes every running dungeon
// effect script.
// Original name: _DNG_EFFECT_ALL_CLEAR.
extern void clear_dungeon_effects(void) NATIVE(1265);

// When on is nonzero the active BGM's volume follows the time of day lighting; 0 turns that off.
// Original name: _AUTO_CHENGE_BGM_VOL.
extern void set_bgm_time_volume(int on) NATIVE(1266);

// Reads the durability of the weapon in weapon_slot (0 or 1) of chara (0 Max, 1 Monica, 2 Ridepod,
// 3 Monica's monster form). whp receives the current value and max, when given, the maximum.
// Original name: _UDATA_GET_WHP.
extern void get_chara_weapon_hp(enum user_chara chara, enum equip_slot weapon_slot, int *whp, OPTIONAL(int *max)) NATIVE(1267);

// Adds amount to the durability of the weapon in weapon_slot (0 or 1) of chara (0 Max, 1 Monica, 2
// Ridepod, 3 Monica's monster form); negative values reduce it.
// Original name: _UDATA_ADD_WHP.
extern void add_chara_weapon_hp(enum user_chara chara, enum equip_slot weapon_slot, int amount) NATIVE(1268);

// Reads the absorption (ABS) points of the weapon in weapon_slot (0 or 1) of chara (0 Max, 1
// Monica, 2 Ridepod, 3 Monica's monster form). abs receives the current value and max, when given,
// the maximum; for the Ridepod max is 0.
// Original name: _UDATA_GET_ABS.
extern void get_chara_weapon_abs(enum user_chara chara, enum equip_slot weapon_slot, int *abs, OPTIONAL(int *max)) NATIVE(1269);

// Adds amount to the absorption (ABS) points of the weapon in weapon_slot (0 or 1) of chara (0
// Max, 1 Monica, 2 Ridepod, 3 Monica's monster form); negative values reduce them.
// Original name: _UDATA_ADD_ABS.
extern void add_chara_weapon_abs(enum user_chara chara, enum equip_slot weapon_slot, int amount) NATIVE(1270);

// Starts a dungeon effect script by name at x, y, z and sets its value 0 to value. The engine also
// accepts a seven argument form (name, x, y, z, x2, y2, z2) that sets the second vector instead.
// Original name: _DNG_CREATE_EFFECT.
extern void create_dungeon_effect(string effect_name, float x, float y, float z, int value) NATIVE(1271);

// Moves the items Monica has equipped into the player's bag, for when she leaves the party.
// Original name: _LEAVE_MONICA_ITEM_CHECK.
extern void return_monica_items(void) NATIVE(1272);

// Allows (1) or blocks (0) the player pausing the game.
// Original name: _PAUSE_ENABLE_FLAG.
extern void set_pause_enabled(int on) NATIVE(1273);

// Starts the fishing tournament schedule on the current day.
// Original name: _FORCE_BOOT_TOUR.
extern void start_fishing_tournament(void) NATIVE(1274);

// Numbers the disc calls that this table never registers. The engine prints "not found ext" and
// moves on, so these calls do nothing.
extern void _EXT_492(...) NATIVE(492);
extern void _EXT_800(...) NATIVE(800);

#include <helpers/event.h>

#endif

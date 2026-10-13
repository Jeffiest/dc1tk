// A made-up town shopkeeper. Until the bridge is repaired she only chats; after that she warms to
// the player over a few talks, gives a free item once, and remarks on how often they drop by.
//
// Read greeting.c first for how programs, `wait;` and actors work. This example is about
// remembering things: in a global (for this stay on the map), and in save flags and counters
// (kept in the save data).

#include <event.h>
// modkit_say shows a line and waits for the player (see greeting.c).
#include "modkit/text.c"

// Animation names from the shopkeeper's model file (Japanese, see greeting.c).
#define MOTION_STAND "立ち"
#define MOTION_BOW "お辞儀"

// The scene character slot the shopkeeper is in (town villagers fill slots 8 and up).
#define SHOPKEEPER 8

// Save data this script uses. A flag is one bit; a counter is a 16-bit number. Both survive saving
// and loading. The flags are numbers the game does not use (825 to 2046); in a mod, write
// NEW_FLAG(FLAG_BRIDGE_REPAIRED); and the like instead, and the build picks numbers no other mod
// uses. The counter here stands in for one the mod reserves.
#define FLAG_BRIDGE_REPAIRED 1205
#define FLAG_GIFT_GIVEN 1210
#define COUNTER_FRIENDSHIP 40

// The free item, by its number; a decompiled tree names every item in include/disc.h
// (ITEM_...), read from the disc.
#define GIFT_ITEM 150

// Talks this stay. A global starts at 0 each time the map's script loads, so it forgets the count
// when the player leaves the map; the friendship counter above does not.
int visits;

PROGRAM(map_start, on_map_start);
// Runs when the player presses the confirm button next to a character in town.
PROGRAM(talk_or_dungeon_start, on_talk);
MAIN(on_map_start);

// Puts the shopkeeper behind her counter when the player enters the map.
void on_map_start(void)
{
    init_event();
    init_chara_actor(0, SHOPKEEPER);
    place_actor(0, 120.0, 0.0, -40.0, PI);
    loop_actor_motion(0, MOTION_STAND);
}

// Hands over the free item and marks it given, so it is only ever given once.
void give_gift(void)
{
    // Bows once, then goes back to standing.
    play_actor_motion_then_loop(0, MOTION_BOW, MOTION_STAND);
    // Puts the item in the player's inventory.
    add_item(GIFT_ITEM);
    // A message from the game's own text files can name the item: set_message_item(0, 1, GIFT_ITEM)
    // fills that message's item slot 1. A line written here just says it.
    modkit_say("Here, take this. On the house!");
    set_flag(FLAG_GIFT_GIVEN, 1);
}

// Talks to the player: the bridge flag, the friendship counter and the gift flag pick the line,
// and from the third talk this stay she remarks on the visits.
void on_talk(void)
{
    int friendship;

    visits = visits + 1;
    if (!get_flag(FLAG_BRIDGE_REPAIRED)) {
        modkit_say("No deliveries until that bridge is fixed, I'm afraid.");
    } else {
        // The counter goes up by one each talk until it reaches 3.
        friendship = get_counter(COUNTER_FRIENDSHIP);
        if (friendship < 3) {
            set_counter(COUNTER_FRIENDSHIP, friendship + 1);
            modkit_say("The bridge is open again! Business is picking up.");
        } else if (!get_flag(FLAG_GIFT_GIVEN)) {
            give_gift();
        } else {
            modkit_say("Always good to see a regular.");
        }
    }
    if (visits >= 3) {
        modkit_say("You do like it here, don't you?");
    }
}

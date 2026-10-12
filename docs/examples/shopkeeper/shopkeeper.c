// A made-up town shopkeeper. Until the bridge is repaired she only chats; after that she warms to
// the player over a few talks, gives a free item once, and remarks on how often they drop by.

#include <event.h>

int visits;

PROGRAM(map_start, on_map_start);
PROGRAM(talk_or_dungeon_start, on_talk);
MAIN(on_map_start);

// Shows a message and waits until the player closes it.
void say(int line)
{
    int closed;

    show_message(0, line);
    closed = 0;
    while (!closed) {
        wait;
        closed = is_message_closed(0);
    }
}

// Puts the shopkeeper behind her counter.
void on_map_start(void)
{
    init_event();
    init_chara_actor(0, 8);
    place_actor(0, 120.0, 0.0, -40.0, PI);
    loop_actor_motion(0, "立ち");  // stand
}

// Hands over the free item, names it in the message and marks it given in the save.
void give_gift(void)
{
    play_actor_motion_then_loop(0, "お辞儀", "立ち");  // bow; stand
    add_item(150);
    set_message_item(0, 0, 150);
    say(320);
    set_flag(1210, 1);
}

// Talks to the player: the bridge flag, the saved friendship counter and the gift flag pick the
// line, and the visit count adds a remark from the third talk on.
void on_talk(void)
{
    int bridge_repaired;
    int friendship;
    int gift_given;

    visits = visits + 1;
    bridge_repaired = get_flag(1205);
    if (!bridge_repaired) {
        say(300);
    } else {
        friendship = get_counter(40);
        if (friendship < 3) {
            set_counter(40, friendship + 1);
            say(310);
        } else {
            gift_given = get_flag(1210);
            if (!gift_given) {
                give_gift();
            } else {
                say(330);
            }
        }
    }
    if (visits >= 3) {
        say(340);
    }
}

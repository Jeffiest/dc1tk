// Mod spinning-cult: a speech bubble from a character, as the game's townspeople talk, and a quiet
// pause. Uses modkit's input helpers.

// Shows text in a bubble from speaker towards listener until the player presses (X) or (O).
static void say_from(int speaker, int listener, string text)
{
    set_message_window_mode(0, MES_WIN_FUKIDASHI);
    set_message_tail(0, 1);
    set_message_speed(0, 1);
    set_message_prompt(0, 1);
    wait;
    wait;
    set_message_slot(0, 0);
    show_message(0, text);
    place_message_bubble(0, speaker, listener);
    modkit_wait(10);
    modkit_wait_for_confirm();
    set_message_cursor(0, -1);
    close_message(0);
    set_message_slot(0, 0);
}

static void say(int speaker, string text)
{
    say_from(speaker, CHARA_PLAYER, text);
}

// Stands chara facing the point (x, z).
static void face_point(int chara, float x, float z)
{
    float pos[3];
    float yaw;

    get_chara_pos(chara, &pos[0], &pos[1], &pos[2]);
    yaw = get_look_yaw(x - pos[0], z - pos[2]);
    set_chara_rot(chara, 0.0, yaw, 0.0);
}

// Stands chara facing another character.
static void face_chara(int chara, int other)
{
    float pos[3];

    get_chara_pos(other, &pos[0], &pos[1], &pos[2]);
    face_point(chara, pos[0], pos[2]);
}

// Puts the camera at eye height a little way from chara, looking at its face from the side of
// the character `from`.
static void close_on(int chara, int from, float distance, float height)
{
    float a[3];
    float b[3];
    float dx;
    float dz;
    float length;

    get_chara_pos(chara, &a[0], &a[1], &a[2]);
    get_chara_pos(from, &b[0], &b[1], &b[2]);
    dx = b[0] - a[0];
    dz = b[2] - a[2];
    length = get_distance(0.0, 0.0, 0.0, dx, 0.0, dz);
    if (length < 1.0) {
        length = 1.0;
    }
    set_camera_pos(a[0] + dx / length * distance, a[1] + height, a[2] + dz / length * distance);
    set_camera_target(a[0], a[1] + height - 2.0, a[2]);
}

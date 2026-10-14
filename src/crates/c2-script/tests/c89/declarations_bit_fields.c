// 3.5.2.1 Structure specifiers: bit-fields
// expect: 1012
// Bit-fields share an int slot from its lowest bit; a plain int one is signed. A field is read as
// (slot & mask) >> offset and stored with the slot's other bits kept, so its value wraps to its
// width. :0 starts the next slot, and a 32-bit field is a slot of its own.
struct flags {
    unsigned ready : 1;
    unsigned mode : 3;
    int delta : 4;
    int : 0;
    unsigned top : 30;
    int last : 2;
    int whole : 32;
};

struct flags global = {1, 5, -3};

int mode_of(struct flags *f)
{
    return f->mode;
}

int test(void)
{
    struct flags f = {1, 6, -2, 100, 1, -7};
    struct flags g;
    int ok = 1;
    int old;

    ok = ok && f.ready == 1 && f.mode == 6 && f.delta == -2 && f.top == 100 && f.last == 1 && f.whole == -7;
    f.mode = 9;
    ok = ok && f.mode == 1 && f.ready == 1 && f.delta == -2;
    f.delta = 7;
    f.delta++;
    ok = ok && f.delta == -8;
    old = f.delta--;
    ok = ok && old == -8 && f.delta == 7;
    f.mode += 3;
    ok = ok && f.mode == 4 && mode_of(&f) == 4;
    f.top = 1073741823;
    f.last = -1;
    ok = ok && f.top == 1073741823 && f.last == -1 && f.mode == 4;
    ok = ok && (f.ready = 3) == 1;
    g = f;
    ok = ok && g.mode == 4 && g.last == -1;
    ok = ok && global.ready == 1 && global.mode == 5 && global.delta == -3 && global.top == 0;
    return ok * 1000 + sizeof(struct flags);
}

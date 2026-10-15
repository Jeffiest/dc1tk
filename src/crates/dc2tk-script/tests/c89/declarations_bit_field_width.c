// 3.5.2.1 Structure specifiers: a bit-field's width
// rejects: bit-field wide has 33 bits; an int has 32
struct too_wide {
    unsigned wide : 33;
};

int test(void)
{
    struct too_wide w;

    w.wide = 1;
    return w.wide;
}

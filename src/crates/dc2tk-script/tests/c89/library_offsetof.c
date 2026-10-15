// 4.1.5 Common definitions <stddef.h>: offsetof
// expect: 20161204
// Offsets and sizes are C's bytes (char 1, short 2, the rest 4, each aligned to its size).
#include <stddef.h>

struct pair {
    char c;
    int b;
    short s[3];
    struct inner {
        char x;
        short y;
    } in;
};

int test(void)
{
    return offsetof(struct pair, b) + offsetof(struct pair, s[2]) * 100 + offsetof(struct pair, in.y) * 10000
        + sizeof(struct pair) * 1000000;
}

// 4.1.5 Common definitions <stddef.h>: its typedefs as type names
// expect: 232
// A header's typedef is a type word wherever a type goes (a return type, a parameter, a member, a
// cast, a typedef), unless the file declares the name as something else.
#include <stddef.h>

size_t length(const wchar_t *s)
{
    size_t n = 0;

    while (s[n] != 0) {
        n++;
    }
    return n;
}

typedef size_t count;

struct buffer {
    size_t used;
    wchar_t text[4];
};

int test(void)
{
    struct buffer b;
    count c = (count)3;
    size_t *p = &b.used;
    ptrdiff_t d = (ptrdiff_t)-2;

    b.text[0] = 'a';
    b.text[1] = 'b';
    b.text[2] = 0;
    *p = length(b.text);
    return (int)b.used * 100 + (int)c * 10 + (d == -2) + (sizeof(count) == 4);
}

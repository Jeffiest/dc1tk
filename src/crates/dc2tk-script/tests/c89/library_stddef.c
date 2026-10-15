// 4.1.5 Common definitions <stddef.h>
// expect: 1
#include <stddef.h>

int test(void)
{
    size_t s = sizeof(short);
    ptrdiff_t d = -1;
    wchar_t w = 'w';

    return NULL == 0 && s == 2 && d < 0 && w == 119 && sizeof(size_t) == 4;
}

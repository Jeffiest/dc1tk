// 3.8.2 Source file inclusion
// expect: 1
#include <limits.h>
#include <stddef.h>

int test(void)
{
    size_t size = sizeof(int);

    return size == 4 && CHAR_BIT == 8 && NULL == 0;
}

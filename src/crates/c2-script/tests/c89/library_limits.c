// 2.2.4.2 Numerical limits: <limits.h>
// expect: 1
#include <limits.h>

int test(void)
{
    signed char c = SCHAR_MAX;
    unsigned int u = UINT_MAX;

    c++;
    u++;
    return CHAR_BIT == 8 && SCHAR_MIN == -128 && SCHAR_MAX == 127 && UCHAR_MAX == 255 && CHAR_MIN == -128
        && CHAR_MAX == 127 && SHRT_MIN == -32768 && SHRT_MAX == 32767 && USHRT_MAX == 65535
        && INT_MAX == 2147483647 && INT_MIN == -INT_MAX - 1 && UINT_MAX == 4294967295 && LONG_MAX == INT_MAX
        && LONG_MIN == INT_MIN && ULONG_MAX == UINT_MAX && MB_LEN_MAX == 2 && c == SCHAR_MIN && u == 0
        && INT_MAX + 1 == INT_MIN;
}

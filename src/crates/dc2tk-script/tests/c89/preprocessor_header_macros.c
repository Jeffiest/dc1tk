// 3.8.2 Source file inclusion: a header's macros in the including file's #if
// expect: 1
#include <limits.h>

int test(void)
{
#if INT_MAX > 0 && defined(CHAR_BIT) && UINT_MAX == 0xffffffff
    return INT_MAX > 0;
#else
    return 0;
#endif
}

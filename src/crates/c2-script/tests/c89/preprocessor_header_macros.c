// 3.8.2 Source file inclusion: a header's macros in the including file's #if
// differs: 0; C gives 1 (a header's #defines reach the file that includes it as constants for code, not as macros for #if)
#include <limits.h>

int test(void)
{
#if INT_MAX > 0
    return 1;
#else
    return INT_MAX < 0;
#endif
}

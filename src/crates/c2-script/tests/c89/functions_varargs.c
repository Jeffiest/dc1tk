// 4.8 Variable arguments <stdarg.h>
// planned: step 5:
#include <stdarg.h>

int sum(int count, ...)
{
    return count;
}

int test(void)
{
    return sum(2, 3, 4);
}

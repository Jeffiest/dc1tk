// 4.1.5 Common definitions <stddef.h>: offsetof
// planned: step 5:
#include <stddef.h>
struct pair {
    int a;
    int b;
};

int test(void)
{
    return offsetof(struct pair, b);
}

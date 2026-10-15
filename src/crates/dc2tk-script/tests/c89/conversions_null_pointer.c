// 3.2.2.3 Pointers: null pointer constants
// expect: 7
// 0, NULL and (void *)0 are null pointers; a pointer compares with one (the VM gives no address
// as a number, so two pointers do not compare: see expressions_pointer_comparison).
#include <stddef.h>

int test(void)
{
    int x;
    int *p = &x;
    int *q = 0;
    int *r = NULL;
    int *s = (void *)0;

    return (p != 0) + (q == 0) * 2 + !r * 4 + (s != NULL) * 8 + (p == NULL) * 16;
}

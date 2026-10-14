// 3.2.2.3 Pointers: null pointer constants
// planned: step 5:
// A pointer compared with 0, and pointer arithmetic, need pointers as values.
int test(void)
{
    int x;
    int *p = &x;
    int *q = 0;

    return p != 0 && q == 0;
}

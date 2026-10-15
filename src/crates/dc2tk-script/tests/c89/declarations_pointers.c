// 3.5.4.1 Pointer declarators
// expect: 9
int test(void)
{
    int x = 4;
    int *p = &x;
    int *const q = &x;

    *p += 2;
    *q += 3;
    return x;
}

// 3.3.6 Additive operators: pointers
// expect: 518
int test(void)
{
    int a[5] = {1, 2, 3, 4, 5};
    int *p = a;
    int *q;

    p = p + 2;
    q = p - 1;
    p++;
    ++q;
    p += 1;
    q -= 2;
    return *p * 100 + *q * 10 + *(a + 3) + p[-1];
}

// 3.3.6 Additive operators
// expect: 1
int test(void)
{
    int big = 2147483647;

    return 2 + 3 == 5 && 2 - 3 == -1 && big + 1 < 0;
}

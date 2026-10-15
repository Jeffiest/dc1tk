// 3.3.13, 3.3.14 Logical AND and OR: the result
// expect: 11
int test(void)
{
    int a = 0;
    int b = 7;

    return (a || 5) * 10 + (b && 3) + (a && 5) + (b || 0) - 1;
}

// 3.3.15 Conditional operator
// expect: 1
int test(void)
{
    int a = 3;

    return (a > 2 ? 10 : 20) == 10 && (a > 5 ? 1 : a > 2 ? 2 : 3) == 2;
}

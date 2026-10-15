// 3.5.1 Storage-class specifiers: auto and register
// expect: 3
int test(void)
{
    auto int a = 1;
    register int b = 2;

    return a + b;
}

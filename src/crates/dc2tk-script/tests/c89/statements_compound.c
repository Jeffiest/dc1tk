// 3.6.2 Compound statement, or block
// expect: 3
int test(void)
{
    int a = 1;

    {
        int b = 2;

        a += b;
    }
    {}
    return a;
}

// 3.1.2.2 Linkages of identifiers: static functions
// expect: 6
static int helper(int n)
{
    return n * 3;
}

int test(void)
{
    return helper(2);
}

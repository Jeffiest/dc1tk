// 3.3.9 Equality operators
// expect: 2
int test(void)
{
    return (1 == 1) + (1 != 2) + (1 == 2);
}

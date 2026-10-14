// 3.3.8 Relational operators
// expect: 4
int test(void)
{
    return (1 < 2) + (2 <= 2) + (3 > 2) + (3 >= 3) + (1 > 2);
}

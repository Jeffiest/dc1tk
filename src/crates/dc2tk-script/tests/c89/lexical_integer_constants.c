// 3.1.3.2 Integer constants
// expect: 1
// Decimal, octal and hexadecimal, with u and l suffixes.
int test(void)
{
    return 10 == 012 && 0x1f == 31 && 0X1F == 31 && 017 == 15 && 0 == 00
        && 5u == 5 && 5L == 5 && 5UL == 5 && 7lu == 7;
}

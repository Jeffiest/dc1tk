// 3.2.1.5 Usual arithmetic conversions
// expect: 1
// Mixed int and float gives float; an unsigned operand makes the other unsigned.
int test(void)
{
    unsigned int big = 3000000000u;

    return 1 + 0.5 == 1.5 && 7 / 2 == 3 && 7 / 2.0 == 3.5 && -1 > 1u && big / 3 == 1000000000
        && big % 7 == 3000000000u % 7 && (big >> 31) == 1;
}

// 3.2.1.3 Floating and integral: unsigned
// expect: 1
// Unsigned values past INT_MAX convert to and from float as their value, not their bits.
int test(void)
{
    unsigned int big = 3000000000u;
    float f = (float)big;
    unsigned int back = (unsigned int)f;

    return f > 2.9e9 && f < 3.1e9 && back > 2999000000u && back < 3001000000u;
}

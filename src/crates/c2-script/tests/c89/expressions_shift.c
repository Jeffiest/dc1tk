// 3.3.7 Bitwise shift operators
// expect: 1
// >> of a negative int is implementation-defined; here it divides, rounding toward zero.
int test(void)
{
    int n = 3;
    unsigned int u = 0x80000000;

    return (1 << 4) == 16 && (1 << n) == 8 && (100 >> 2) == 25 && (-8 >> 1) == -4 && (u >> 31) == 1
        && (u >> n) == 0x10000000;
}

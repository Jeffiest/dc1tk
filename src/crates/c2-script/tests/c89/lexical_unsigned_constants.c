// 3.1.3.2 Integer constants: types
// expect: 1
// A constant too large for int (and long) is unsigned, as is one with a u suffix.
int test(void)
{
    return 0xFFFFFFFF > 0 && 4294967295 > 0 && 0u - 1 > 0 && -1 < 0 && 2147483647 > 0;
}

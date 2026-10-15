// 3.8.1 Conditional inclusion: constant expressions
// expect: 1
// Integer arithmetic in long and unsigned long (32 bits), with C's precedence.
#define SQUARE(x) ((x) * (x))
#if SQUARE(3) == 9 && (1 ? 2 : 3) == 2 && -1 < 0 && -1 > 0u && (7 % 3) == 1 && (1 << 4) == 16 \
    && ('A' == 65) && (0x10 | 1) == 17 && (6 ^ 3) == 5 && ~0 == -1 && !0 && (3 & 1 == 1) == 1
#define OK 1
#else
#define OK 0
#endif

int test(void)
{
    return OK;
}

// 3.8.1 Conditional inclusion
// expect: 15
#define ONE 1
#undef TWO

int test(void)
{
    int r = 0;

#if ONE
    r += 1;
#endif
#if defined(ONE) && !defined TWO
    r += 2;
#endif
#ifdef ONE
    r += 4;
#else
    r = -100;
#endif
#ifndef TWO
#if ONE == 2
    r = -100;
#elif ONE == 1
    r += 8;
#else
    r = -100;
#endif
#endif
#if 0
    this is not code, and need not be
#endif
    return r;
}

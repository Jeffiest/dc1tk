// 3.4 Constant expressions
// expect: 1
// In array sizes, enumerators, case labels and #if.
#define ROWS (2 + 1)
enum { FIRST = 1 << 2, SECOND = FIRST * 2 + 1 };

int test(void)
{
    int a[ROWS * 2];
    int found = 0;

    switch (9) {
    case SECOND:
        found = 1;
        break;
    case FIRST + 1:
        found = 2;
        break;
    }
#if ROWS * 2 == 6 && SIX_UNDEFINED == 0
    return sizeof a == 24 && found == 1;
#else
    return 0;
#endif
}

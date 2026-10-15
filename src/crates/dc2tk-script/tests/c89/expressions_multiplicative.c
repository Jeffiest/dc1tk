// 3.3.5 Multiplicative operators
// expect: 1
// Division of negatives truncates toward zero (implementation-defined in C89; C99's rule).
int test(void)
{
    return 7 * 6 == 42 && 7 / 2 == 3 && -7 / 2 == -3 && 7 % 3 == 1 && -7 % 3 == -1 && 7.0 / 2 == 3.5;
}

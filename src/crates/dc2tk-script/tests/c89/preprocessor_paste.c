// 3.8.3.3 The ## operator
// expect: 1
#define CAT(a, b) a ## b
#define VAR(n) var_ ## n

int test(void)
{
    int VAR(1) = 5;
    int xy = 7;

    return var_1 == 5 && CAT(x, y) == 7 && CAT(1, 2) == 12 && CAT(0x, 10) == 16;
}

// 3.8.3 Macro replacement: function-like macros
// expect: 1
#define SQUARE(x) ((x) * (x))
#define MAX(a, b) ((a) > (b) ? (a) : (b))
#define ZERO() 0
#define CALL(f, arg) f(arg)
#define LONG(a, \
             b) ((a) - (b))

int test(void)
{
    int i = 2;

    return SQUARE(i + 1) == 9 && MAX(3, SQUARE(2)) == 4 && ZERO() == 0 && CALL(SQUARE, 3) == 9
        && MAX(MAX(1, 5), 2) == 5 && LONG(5, 2) == 3;
}

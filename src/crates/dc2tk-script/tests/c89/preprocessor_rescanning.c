// 3.8.3.4 Rescanning and further replacement
// expect: 1
// A macro is not replaced again inside its own replacement.
int total = 5;
#define total (total + 1)
#define f(a) a * g
#define g(a) f(a)

int test(void)
{
    int g = 3;

    return total == 6 && f(2)(9) == 2 * 9 * g;
}

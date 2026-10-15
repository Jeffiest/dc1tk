// 3.8.3 Macro replacement: object-like macros
// expect: 1
// A constant stays a named constant; any other replacement is text, as in C.
#define TEN 10
#define NEGATIVE -4
#define SUM 1 + 2
#define BLOCK { return 1; }
#define NOTHING

int ok(void) BLOCK

int test(void)
{
    NOTHING
    return TEN == 10 && NEGATIVE == -4 && SUM * 3 == 7 && ok();
}

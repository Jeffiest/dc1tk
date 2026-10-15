// 3.3.3.3 Unary arithmetic operators
// expect: 1
int test(void)
{
    int a = 5;

    return +a == 5 && -a == -5 && ~a == -6 && !a == 0 && !0 == 1 && - -a == 5;
}

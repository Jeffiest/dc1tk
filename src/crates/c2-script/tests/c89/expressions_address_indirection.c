// 3.3.3.2 Address and indirection operators
// expect: 42
int test(void)
{
    int x = 41;
    int *p = &x;

    *p = *p + 1;
    return x;
}

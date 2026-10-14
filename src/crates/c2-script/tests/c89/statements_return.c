// 3.6.6.4 The return statement
// expect: 4
void nothing(void)
{
    return;
}

int value(int n)
{
    if (n > 1) {
        return n * 2;
    }
    return 0;
}

int test(void)
{
    nothing();
    return value(2);
}

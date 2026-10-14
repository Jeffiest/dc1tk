// 3.3.13, 3.3.14 Logical AND and OR
// expect: 1
// The right operand runs only when the left does not decide.
int calls;

int touch(int value)
{
    calls++;
    return value;
}

int test(void)
{
    int a = 0 && touch(1);
    int b = 1 || touch(1);
    int c = 1 && touch(0);

    return !a && b && !c && calls == 1;
}

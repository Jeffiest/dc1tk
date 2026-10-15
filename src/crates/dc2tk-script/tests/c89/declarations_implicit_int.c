// 3.5.2 Type specifiers: implicit int
// expect: 5
// A declaration with specifiers but no type is an int (C89).
static total = 2;

int test(void)
{
    register count = 3;
    const unsigned three = 3;

    return total + count + three - 3;
}

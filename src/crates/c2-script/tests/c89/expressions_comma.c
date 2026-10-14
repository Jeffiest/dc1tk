// 3.3.17 Comma operator
// expect: 1
int test(void)
{
    int a;
    int b;
    int i;
    int j;

    a = (b = 2, b + 1);
    for (i = 0, j = 10; i < j; i++, j--) {
    }
    return a == 3 && b == 2 && i == 5 && (1, 2) == 2;
}

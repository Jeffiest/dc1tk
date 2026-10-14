// 3.3.16.1 Simple assignment
// expect: 1
int test(void)
{
    int a;
    int b;
    int c;

    a = b = c = 7;
    return a == 7 && b == 7 && (c = 3) == 3 && c == 3;
}

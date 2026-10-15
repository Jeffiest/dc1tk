// 3.6.3 Expression and null statements
// expect: 2
int test(void)
{
    int a = 1;

    a + 1;
    ;
    for (; a < 2; a++)
        ;
    return a;
}

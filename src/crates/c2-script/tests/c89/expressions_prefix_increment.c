// 3.3.3.1 Prefix increment and decrement
// expect: 1
int test(void)
{
    int i = 5;

    return ++i == 6 && --i == 5 && i == 5;
}

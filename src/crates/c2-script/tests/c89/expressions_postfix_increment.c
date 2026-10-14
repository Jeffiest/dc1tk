// 3.3.2.4 Postfix increment and decrement
// expect: 1
int test(void)
{
    int i = 5;
    int a[2] = {1, 2};
    int j = 0;
    int old;

    old = i++;
    a[j++]--;
    return old == 5 && i == 6 && a[0] == 0 && j == 1 && i-- == 6 && i == 5;
}

// 3.3.2.1 Array subscripting
// expect: 30
int test(void)
{
    int a[4] = {10, 20, 30, 40};
    int i = 1;

    return a[i + 1];
}

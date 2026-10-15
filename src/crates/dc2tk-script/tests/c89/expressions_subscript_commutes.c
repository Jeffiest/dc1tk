// 3.3.2.1 Array subscripting: E1[E2] is *(E1 + E2)
// expect: 17
int test(void)
{
    int a[2] = {5, 6};
    int i = 1;

    return i[a] + *(a + i) + 0[a];
}

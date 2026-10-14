// 3.3.2.1 Array subscripting: E1[E2] is *(E1 + E2)
// planned: step 5:
// i[a] and *(a + i) need pointer arithmetic.
int test(void)
{
    int a[2] = {5, 6};
    int i = 1;

    return i[a] + *(a + i);
}

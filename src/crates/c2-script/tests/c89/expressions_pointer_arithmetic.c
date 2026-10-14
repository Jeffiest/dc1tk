// 3.3.6 Additive operators: pointers
// planned: step 5:
int test(void)
{
    int a[3] = {1, 2, 3};
    int *p = a;

    p = p + 2;
    return *p;
}

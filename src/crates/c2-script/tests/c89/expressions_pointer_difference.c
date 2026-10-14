// 3.3.6 Additive operators: the difference of two pointers
// limit: the difference of two pointers is not supported
int test(void)
{
    int a[2];
    int *p = a;
    int *q = &a[1];

    return q - p;
}

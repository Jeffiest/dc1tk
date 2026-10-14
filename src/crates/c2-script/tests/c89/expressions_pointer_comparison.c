// 3.3.8, 3.3.9 Relational and equality operators: two pointers
// limit: comparing two pointers is not supported
// The VM gives no address as a number: a pointer compares with a null pointer only.
int test(void)
{
    int a[2];
    int *p = a;
    int *q = &a[1];

    return p < q;
}

// 3.5.4.2 Array declarators, 3.5.7 Initialization
// expect: 1
// Elements an initializer leaves out start at 0; [] takes its size from the initializer.
int test(void)
{
    int a[5] = {1, 2};
    int b[] = {7, 8, 9};
    float f[2] = {1.5};

    return a[1] == 2 && a[4] == 0 && sizeof b == 12 && b[2] == 9 && f[1] == 0;
}

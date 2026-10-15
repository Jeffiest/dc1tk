// 3.5.1 Storage-class specifiers: register
// rejects: b is a register variable, which has no address
int test(void)
{
    register int b = 2;
    int *p = &b;

    return *p;
}

// 3.1.3.1 Floating constants: the l suffix
// limit: long double is not supported
// 1.5L is a long double, which the VM's 32-bit floats cannot hold.
int test(void)
{
    return (int)1.5L;
}

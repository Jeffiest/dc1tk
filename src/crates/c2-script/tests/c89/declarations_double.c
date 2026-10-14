// 3.5.2 Type specifiers: double
// limit: double is not supported: the VM has 32-bit floats only; use float
int test(void)
{
    double d = 1.5;

    return (int)d;
}

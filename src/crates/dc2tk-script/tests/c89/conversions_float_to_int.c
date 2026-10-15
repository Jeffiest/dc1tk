// 3.2.1.3 Floating and integral
// expect: 1
// A float converted to an integer type drops its fraction.
int test(void)
{
    return (int)2.7 == 2 && (int)-2.7 == -2 && (int)0.5 == 0;
}

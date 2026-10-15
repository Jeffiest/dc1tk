// 3.3.4 Cast operators
// expect: 1
int test(void)
{
    float f = 3.75;
    int i = 300;

    return (int)f == 3 && (float)i == 300.0 && (long)f == 3 && (unsigned char)i == 44 && (short)70000 == 4464;
}

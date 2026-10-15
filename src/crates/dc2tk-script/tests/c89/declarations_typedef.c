// 3.5.6 Type definitions
// expect: 1
typedef int number;
typedef unsigned char byte;

number twice(number n)
{
    return n * 2;
}

int test(void)
{
    typedef float real;
    number a = 21;
    byte b = 300;
    real r = 0.5;

    return twice(a) == 42 && b == 44 && r == 0.5 && sizeof(byte) == 1;
}

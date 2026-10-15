// 3.3.10 to 3.3.12 Bitwise AND, exclusive OR and inclusive OR
// expect: 1
int test(void)
{
    int a = 12;
    int b = 10;

    return (a & b) == 8 && (a ^ b) == 6 && (a | b) == 14 && (-1 & 255) == 255 && (-1 ^ 1) == -2;
}

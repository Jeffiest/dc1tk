// 3.5.2 Type specifiers: char, short, int, long, signed, unsigned
// expect: 1
// Each wraps at its width, in assignments, steps, compound assignments, arguments and results.
unsigned char low_byte(int value)
{
    return value;
}

int byte_sum(unsigned char a, unsigned char b)
{
    return a + b;
}

int test(void)
{
    signed char sc = 127;
    unsigned char uc = 255;
    short sh = 32767;
    unsigned short us = 65535;
    long l = 2147483647;
    unsigned long ul = 4294967295;
    char c = 'A';
    unsigned char bytes[2] = {250, 251};
    unsigned char before;
    int i = 0;

    sc++;
    uc++;
    sh++;
    us++;
    l++;
    ul++;
    c += 1;
    bytes[i] += 10;
    uc = 255;
    before = uc++;
    return sc == -128 && uc == 0 && sh == -32768 && us == 0 && l < 0 && ul == 0 && c == 'B' && bytes[0] == 4
        && before == 255 && low_byte(0x1234) == 0x34 && byte_sum(300, 300) == 88 && (uc = 256) == 0;
}

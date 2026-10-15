// 3.2.1.2 Signed and unsigned integers
// expect: 1
// -1 converts to the largest unsigned int, and back.
int test(void)
{
    unsigned int u = -1;
    int i;

    i = u;
    return u == 4294967295 && u > 0 && i == -1 && (unsigned char)-1 == 255 && (signed char)255 == -1;
}

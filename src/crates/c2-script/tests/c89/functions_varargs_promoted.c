// 4.8.1.2 va_arg of a promoted type
// expect: 1
// A char or short after ... is passed as an int (the default argument promotions); va_arg of char
// or short reads the int and converts it back.
#include <stdarg.h>

int pack(int count, ...)
{
    va_list ap;
    int total;
    char c;
    unsigned char u;
    short s;

    va_start(ap, count);
    c = va_arg(ap, char);
    u = va_arg(ap, unsigned char);
    s = va_arg(ap, short);
    total = c * 1000000 + u * 1000 + s;
    va_end(ap);
    return total;
}

int test(void)
{
    char c = 7;
    unsigned char u = 200;
    short s = -5;

    return pack(3, c, u, s) == 7000000 + 200000 - 5 && pack(3, 'a', 300, 70000) == 97 * 1000000 + 44 * 1000 + 4464;
}

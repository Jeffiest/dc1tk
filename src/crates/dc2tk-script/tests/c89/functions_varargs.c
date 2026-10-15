// 4.8 Variable arguments <stdarg.h>
// expect: 61
#include <stdarg.h>

int sum(int count, ...)
{
    va_list ap;
    int total = 0;

    va_start(ap, count);
    while (count-- > 0) {
        total += va_arg(ap, int);
    }
    va_end(ap);
    return total;
}

float average(int count, ...)
{
    va_list ap;
    float total = 0;
    int i;

    va_start(ap, count);
    for (i = 0; i < count; i++) {
        total += va_arg(ap, float);
    }
    va_end(ap);
    return total / count;
}

int test(void)
{
    return sum(3, 1, 2, 3) * 10 + sum(0) + (average(2, 1.5f, 2.5f) == 2.0);
}

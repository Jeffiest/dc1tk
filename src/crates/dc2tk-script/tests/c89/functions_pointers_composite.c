// 3.5.4.3 Pointers to functions that take or return structs, or take ...
// expect: 1632151
// A call through a pointer passes what a direct call does: a struct's address, the address of the
// caller's place for a struct result, the array of the arguments after ...
#include <stdarg.h>

struct pair {
    int a;
    int b;
};

int sum_pair(struct pair p)
{
    p.a += 100;
    return p.a + p.b;
}

struct pair swapped(struct pair p)
{
    struct pair q;

    q.a = p.b;
    q.b = p.a;
    return q;
}

struct pair make(int a, int b)
{
    struct pair p;

    p.a = a;
    p.b = b;
    return p;
}

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

void nothing(int x)
{
}

int test(void)
{
    int (*f)(struct pair) = sum_pair;
    struct pair (*g)(struct pair) = swapped;
    struct pair (*h)(int, int) = &make;
    int (*v)(int, ...) = sum;
    void (*n)(int) = nothing;
    struct pair p = {1, 2};
    struct pair q;

    q = g(p);
    n(3);
    return f(p) * 10000 + q.a * 1000 + q.b * 100 + h(4, 5).b * 10 + (p.a == 1) + v(3, 1, 2, 3) * 100000;
}

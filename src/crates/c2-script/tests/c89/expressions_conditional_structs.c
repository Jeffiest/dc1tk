// 3.3.15 Conditional operator: structure operands
// expect: 3222
// A ?: of two structs gives one of their addresses, which is copied or reached through as a
// struct's value is.
struct pair {
    int a;
    float b;
};

struct pair make(int a)
{
    struct pair p;

    p.a = a;
    p.b = a / 2.0f;
    return p;
}

int test(void)
{
    struct pair x = {1, 1.5};
    struct pair y = {2, 2.5};
    struct pair z;
    int pick = 0;

    z = pick ? x : y;
    pick = (pick ? x : y).a * 10 + (1 ? x : y).a;
    z.a += (pick > 0 ? make(30) : x).a;
    return z.a * 100 + pick + (z.b == 2.5);
}

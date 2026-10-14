// 3.3.2.2, 3.6.6.4 Structures as arguments and return values
// expect: 3024
// A struct is passed by its address and copied by the function, and returned through a hidden
// pointer to where the caller wants it.
struct point {
    int x;
    float y;
};

struct point make(int x, float y)
{
    struct point p;

    p.x = x;
    p.y = y;
    return p;
}

int scaled(struct point p, int by)
{
    p.x *= by;
    return p.x;
}

struct point moved(struct point p)
{
    p.x += 1;
    return p;
}

int test(void)
{
    struct point a = make(2, 0.5);
    struct point b;
    int s = scaled(a, 10);

    b = moved(moved(a));
    return s * 100 + a.x * 10 + b.x + (b.y == 0.5) * 1000;
}

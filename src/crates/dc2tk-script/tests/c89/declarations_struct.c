// 3.5.2.1 Structure specifiers
// expect: 822
struct pair {
    int a;
    float b;
};

struct line {
    struct pair from;
    struct pair to;
    char name[4];
};

int test(void)
{
    struct pair p = {1, 2.5};
    struct line l;
    struct pair *q = &l.to;

    l.from = p;
    q->a = 3;
    q->b = p.b * 2;
    l.name[0] = 'x';
    return sizeof p * 100 + sizeof(struct line) - 4 + l.from.a + q->a + (l.to.b == 5.0) + (l.name[0] == 'x');
}

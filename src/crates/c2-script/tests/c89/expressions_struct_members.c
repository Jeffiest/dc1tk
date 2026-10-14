// 3.3.2.3 Structure and union members
// expect: 20321
struct point {
    int x;
    int y;
};

struct box {
    struct point corner[2];
    int tag;
};

int test(void)
{
    struct box b;
    struct box *p = &b;
    int i = 1;

    b.corner[0].x = 1;
    p->corner[i].y = 2;
    (*p).tag = 3;
    p->corner[0].y = b.corner[1].y * 10;
    return b.corner[0].x + b.corner[1].y * 10 + p->tag * 100 + (&b)->corner[0].y * 1000;
}

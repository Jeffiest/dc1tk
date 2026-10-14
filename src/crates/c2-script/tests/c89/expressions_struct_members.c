// 3.3.2.3 Structure and union members
// planned: step 5:
struct point {
    int x;
    int y;
};

int test(void)
{
    struct point p;

    p.x = 1;
    return p.x;
}

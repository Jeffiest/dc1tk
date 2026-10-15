// 3.5.4.1 Pointer declarators: pointers to pointers
// expect: 27
void point(int **out, int *to)
{
    *out = to;
}

int test(void)
{
    int x = 1;
    int y = 20;
    int *p = &x;
    int **pp = &p;
    int ***ppp = &pp;

    **pp = 3;
    point(pp, &y);
    ***ppp += 4;
    return x + **pp;
}

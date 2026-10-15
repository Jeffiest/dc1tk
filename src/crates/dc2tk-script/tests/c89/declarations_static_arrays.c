// 3.5.1 Storage-class specifiers: static arrays and pointers
// expect: 36
int count(void)
{
    static int seen[2];
    static int *last;

    seen[0]++;
    last = &seen[1];
    *last += 2;
    return seen[0] * 10 + seen[1];
}

int test(void)
{
    count();
    count();
    return count();
}

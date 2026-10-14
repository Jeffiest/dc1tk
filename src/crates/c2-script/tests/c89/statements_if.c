// 3.6.4.1 The if statement
// expect: 7
// else belongs to the nearest if.
int test(void)
{
    int r = 0;

    if (1)
        if (0)
            r = 1;
        else
            r = 2;
    if (r == 2) {
        r += 5;
    } else if (r == 1) {
        r = 0;
    }
    return r;
}

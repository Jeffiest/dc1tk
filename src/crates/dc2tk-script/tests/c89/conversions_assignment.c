// 3.2.1.3 Floating and integral: assignment
// expect: 4
// A float stored in an int is truncated, an int stored in a float becomes one, and so do
// arguments, returns and compound assignments.
int half(int n)
{
    return n;
}

float third(void)
{
    return 1;
}

int test(void)
{
    int x = 2.5;
    float f = 3;
    int y = 1;

    y += 0.75;
    return x + half(1.5) + (f / 2 == 1.5) + y - (third() / 2 == 0.5);
}

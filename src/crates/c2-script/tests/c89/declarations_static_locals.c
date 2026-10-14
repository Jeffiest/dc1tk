// 3.5.1 Storage-class specifiers: static
// expect: 63
// A static local keeps its value between calls, starting from its initializer once.
int counter(void)
{
    static int count;
    static int step = 10;

    count += step;
    step++;
    return count;
}

int test(void)
{
    counter();
    counter();
    return counter() + 30;
}

// 3.5.3 Type qualifiers: volatile
// expect: 4
// Every access to a variable is made, so volatile needs nothing more.
int test(void)
{
    volatile int v = 3;

    v++;
    return v;
}

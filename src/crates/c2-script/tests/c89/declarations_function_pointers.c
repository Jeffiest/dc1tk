// 3.5.4.3 Function declarators: pointers to functions
// planned: step 5:
int twice(int n)
{
    return n * 2;
}

int test(void)
{
    int (*f)(int) = twice;

    return f(2);
}

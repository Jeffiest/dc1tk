// 3.3.2.2 Function calls
// expect: 120
// Arguments are values; a function can call itself.
int factorial(int n)
{
    if (n <= 1) {
        return 1;
    }
    return n * factorial(n - 1);
}

void change(int n)
{
    n = 99;
}

int test(void)
{
    int n = 5;

    change(n);
    return factorial(n);
}

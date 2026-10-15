// 3.5.4.3 Function declarators: pointers to functions
// expect: 150
// A function pointer holds the function's number; a call through it switches on the number.
int twice(int n)
{
    return n * 2;
}

int square(int n)
{
    return n * n;
}

int add(int a, int b)
{
    return a + b;
}

int apply(int (*f)(int), int n)
{
    return f(n);
}

typedef int (*binary)(int, int);

int (*table[2])(int) = {twice, square};

int test(void)
{
    int (*f)(int) = twice;
    binary g = add;
    int total = f(2);

    f = &square;
    total += (*f)(3);
    total += apply(twice, 5);
    total += table[1](4);
    total += g(100, 0);
    return total + (f != 0) + (f == square) * 10;
}

// 3.5.4.3 Function declarators, 3.7.1 Function definitions
// expect: 7
int later(int a, int);
int none();

int test(void)
{
    return later(3, 4) + none();
}

int later(int a, int b)
{
    return a + b;
}

int none()
{
    return 0;
}

// 3.5.7 Initialization: constant expressions
// rejects: the value of global b must be a constant expression, not a
int a = 1;
int b = a + 1;

int test(void)
{
    return b;
}

// 3.5 Declarations: one declaration of a name in a block
// rejects: x is declared twice in one block
int test(void)
{
    int x = 1;
    int x = 2;

    return x;
}

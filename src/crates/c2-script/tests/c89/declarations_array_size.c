// 3.5.4.2 Array declarators: a size greater than zero
// rejects: empty needs a size of at least 1
int test(void)
{
    int empty[0];

    return 0;
}

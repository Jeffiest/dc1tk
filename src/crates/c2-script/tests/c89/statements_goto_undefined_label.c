// 3.6.6.1 The goto statement: the label must be in the function
// rejects: goto nowhere: no such label in test
int test(void)
{
    goto nowhere;
    return 0;
}

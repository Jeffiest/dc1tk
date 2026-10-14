// 3.5.1 Storage-class specifiers: static arrays
// planned: step 5: static arrays are not supported
int test(void)
{
    static int seen[2];

    return seen[0];
}

// 3.3.16.1 Simple assignment: an integer is not a pointer
// rejects: assigning int to int * needs a cast
int test(void)
{
    int x = 5;
    int *p;

    p = x;
    return 0;
}

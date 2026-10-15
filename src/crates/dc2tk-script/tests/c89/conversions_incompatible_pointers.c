// 3.3.16.1 Simple assignment: pointers to different types
// rejects: incompatible pointer types float * and int *
int test(void)
{
    float f;
    int *p = &f;

    return 0;
}

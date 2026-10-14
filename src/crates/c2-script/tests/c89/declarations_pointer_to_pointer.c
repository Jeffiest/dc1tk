// 3.5.4.1 Pointer declarators: pointers to pointers
// planned: step 5: pointers to pointers are not supported yet
int test(void)
{
    int x = 1;
    int *p = &x;
    int **pp = &p;

    return **pp;
}

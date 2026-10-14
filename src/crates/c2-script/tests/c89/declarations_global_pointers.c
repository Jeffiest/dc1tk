// 3.5.4.1 Pointer declarators: pointers with static storage
// planned: step 5: global pointers are not supported
int x;
int *where;

int test(void)
{
    where = &x;
    return 0;
}

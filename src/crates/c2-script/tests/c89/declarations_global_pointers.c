// 3.5.4.1 Pointer declarators: pointers with static storage
// expect: 12
int x;
int y = 5;
int *where;
int *nowhere = 0;
int *start = &y;

int test(void)
{
    where = &x;
    *where = 7;
    return x + *start + (nowhere == 0) - 1;
}

// 3.5.4.3 Function declarators: returning pointers
// planned: step 5: functions that return pointers are not supported yet
int x;

int *where(void)
{
    return &x;
}

int test(void)
{
    return 0;
}

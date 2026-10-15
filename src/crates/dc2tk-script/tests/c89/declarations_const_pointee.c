// 3.5.3 Type qualifiers: a pointer to const
// rejects: what values points at is const
void clear(const int *values)
{
    values[0] = 0;
}

int test(void)
{
    return 0;
}

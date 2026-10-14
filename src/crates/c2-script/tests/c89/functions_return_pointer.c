// 3.5.4.3 Function declarators: returning pointers
// expect: 50
int x;
int table[3];

int *where(void)
{
    return &x;
}

int *element(int *array, int i)
{
    return array + i;
}

int test(void)
{
    *where() = 4;
    *element(table, 2) = 5;
    return x * 10 + table[2] + *element(table, 2);
}

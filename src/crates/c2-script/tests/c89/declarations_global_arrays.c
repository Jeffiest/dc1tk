// 3.5.4.2 Array declarators: arrays with static storage
// expect: 31
int table[4];
int primes[] = {2, 3, 5, 7};
float scale[2] = {0.5};
int grid[2][3] = {{1, 2, 3}, {4, 5, 6}};

int sum(int *values, int count)
{
    int total = 0;
    int i;

    for (i = 0; i < count; i++) {
        total += values[i];
    }
    return total;
}

int test(void)
{
    int i = 2;

    table[1] = 2;
    table[i] = table[1] * 3;
    return table[1] + table[2] + sum(primes, 4) + (scale[0] == 0.5) + grid[1][i] - grid[0][0];
}

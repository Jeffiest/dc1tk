// 3.5.3 Type qualifiers: const
// expect: 12
const int limit = 10;

int sum(const int *values, int count)
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
    const int two = 2;
    int pair[2] = {1, 1};

    return limit + two + sum(pair, 0);
}

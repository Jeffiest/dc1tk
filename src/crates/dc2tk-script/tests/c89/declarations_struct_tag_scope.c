// 3.1.2.1 Scopes of identifiers: tags
// expect: 1517131
// A struct declared in a block is the block's: its tag hides an outer one until the block ends.
struct item {
    int value;
};

int outer(void)
{
    struct item i;

    i.value = 5;
    return i.value;
}

int test(void)
{
    struct item a;
    int seen;

    a.value = 1;
    {
        struct item {
            float weight;
            int count;
        };
        struct item b;

        b.weight = 2.5;
        b.count = 3;
        seen = b.count * 10 + (b.weight == 2.5) + (sizeof(struct item) == 8) * 100;
    }
    {
        struct item c;

        c.value = 7;
        seen += c.value * 1000;
    }
    return seen + a.value * 10000 + outer() * 100000 + (sizeof(struct item) == 4) * 1000000;
}

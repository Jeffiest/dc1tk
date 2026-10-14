// 3.5.2.1 Structure specifiers
// planned: step 5:
struct pair {
    int a;
    float b;
};

int test(void)
{
    struct pair p;

    return sizeof p;
}

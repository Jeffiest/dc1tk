// 3.5.2.1 Union specifiers
// planned: step 5:
union value {
    int i;
    float f;
};

int test(void)
{
    union value v;

    v.i = 1;
    return v.i;
}

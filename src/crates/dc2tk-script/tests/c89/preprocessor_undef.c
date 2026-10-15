// 3.8.3.5 Scope of macro definitions
// expect: 7
#define VALUE(x) (x + 1)
int first(void)
{
    return VALUE(2);
}
#undef VALUE
#define VALUE(x) (x * 2)

int test(void)
{
    return first() + VALUE(2);
}

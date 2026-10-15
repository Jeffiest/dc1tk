// 3.5.1, 3.7.2 extern and tentative definitions
// expect: 7
extern int shared;
int shared;
int shared = 7;

int test(void)
{
    extern int shared;

    return shared;
}

// 3.8.4 Line control
// expect: 1
int test(void)
{
#line 100 "renamed.c"
    return __LINE__ == 100 && __FILE__ == "renamed.c";
}

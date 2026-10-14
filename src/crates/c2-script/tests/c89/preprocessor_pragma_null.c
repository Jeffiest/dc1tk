// 3.8.6 Pragma directive, 3.8.7 Null directive
// expect: 1
// A pragma the compiler does not know is ignored.
#pragma some_other_compiler optimise
#
int test(void)
{
    return 1;
}

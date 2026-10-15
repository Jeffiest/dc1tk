// 3.8.5 Error directive
// rejects: #error this configuration is not supported
#if 0
#error not reached
#endif
#error this configuration is not supported
int test(void)
{
    return 0;
}

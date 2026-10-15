// 3.8.1 Conditional inclusion: one #else, last
// rejects: #elif after #else
#if 1
#else
#elif 1
#endif

int test(void)
{
    return 0;
}

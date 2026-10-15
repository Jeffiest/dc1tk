// 3.8.8 Predefined macro names
// expect: 1
// __STDC_VERSION__ came with C95, so a C89 compiler leaves it undefined.
int test(void)
{
    string date = __DATE__;
    string time = __TIME__;

#ifdef __STDC_VERSION__
    return 0;
#else
    return __STDC__ == 1 && __LINE__ == 12 && __FILE__ != "" && sizeof __DATE__ == 12 && sizeof __TIME__ == 9
        && date != time;
#endif
}
